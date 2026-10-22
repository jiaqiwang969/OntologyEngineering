//! Local descendants of one existing ProcessTree. This is not a process registry.
//! Only PID/birth/parent metadata is inspected; no argv, environment or credentials.
//! macOS libproc and Linux /proc are independent backends. This is cooperative
//! local cancellation, not a hostile-code containment or hard-real-time claim.
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    time::{Duration, Instant},
};
const MAX_LIVE_DESCENDANTS: usize = 8192;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    pid: i32,
    birth: u128,
}
#[derive(Clone, Copy)]
struct Info {
    identity: Identity,
    parent: i32,
    inactive: bool,
}

#[cfg(target_os = "macos")]
fn info(pid: i32) -> io::Result<Option<Info>> {
    let mut b = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let expected = std::mem::size_of::<libc::proc_bsdinfo>();
    unsafe {
        *libc::__error() = 0;
    }
    let n = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            b.as_mut_ptr().cast(),
            expected as i32,
        )
    };
    if n == 0 {
        let error = io::Error::last_os_error();
        return if matches!(
            error.raw_os_error(),
            Some(0) | Some(libc::ESRCH) | Some(libc::ENOENT)
        ) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    if n != expected as i32 {
        return Err(io::Error::other("process_identity_incomplete"));
    }
    let b = unsafe { b.assume_init() };
    Ok(Some(Info {
        identity: Identity {
            pid,
            birth: ((b.pbi_start_tvsec as u128) << 64) | b.pbi_start_tvusec as u128,
        },
        parent: b.pbi_ppid as i32,
        inactive: b.pbi_status == 5,
    }))
}
#[cfg(target_os = "macos")]
fn child_pids(pid: i32) -> io::Result<Vec<i32>> {
    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_listchildpids(
            ppid: libc::pid_t,
            buffer: *mut libc::c_void,
            buffersize: libc::c_int,
        ) -> libc::c_int;
    }
    let mut size = 32;
    loop {
        let mut pids = vec![0i32; size];
        unsafe {
            *libc::__error() = 0;
        }
        let n = unsafe {
            proc_listchildpids(
                pid,
                pids.as_mut_ptr().cast(),
                (size * std::mem::size_of::<i32>()) as i32,
            )
        };
        let error = unsafe { *libc::__error() };
        if n < 0 || (n == 0 && error != 0) {
            if matches!(error, libc::ESRCH | libc::ENOENT) {
                return Ok(vec![]);
            }
            return Err(io::Error::last_os_error());
        }
        if (n as usize) < size {
            pids.truncate(n as usize);
            return Ok(pids.into_iter().filter(|p| *p > 0).collect());
        }
        if size >= MAX_LIVE_DESCENDANTS {
            return Err(io::Error::other("process_descendant_budget"));
        }
        size = (size * 2).min(MAX_LIVE_DESCENDANTS);
    }
}
#[cfg(target_os = "linux")]
fn info(pid: i32) -> io::Result<Option<Info>> {
    let bytes = match std::fs::read(format!("/proc/{pid}/stat")) {
        Ok(x) => x,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let position = bytes
        .iter()
        .rposition(|v| *v == b')')
        .ok_or_else(|| io::Error::other("process_stat_invalid"))?;
    let tail = std::str::from_utf8(&bytes[position + 1..])
        .map_err(|_| io::Error::other("process_stat_numeric_tail_invalid"))?;
    let fields = tail.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 20 {
        return Err(io::Error::other("process_stat_incomplete"));
    }
    let parent = fields[1]
        .parse()
        .map_err(|_| io::Error::other("process_parent_invalid"))?;
    let birth = fields[19]
        .parse()
        .map_err(|_| io::Error::other("process_birth_invalid"))?;
    Ok(Some(Info {
        identity: Identity { pid, birth },
        parent,
        inactive: matches!(fields[0], "Z" | "X"),
    }))
}
#[cfg(target_os = "linux")]
fn child_pids(pid: i32) -> io::Result<Vec<i32>> {
    let tasks = match std::fs::read_dir(format!("/proc/{pid}/task")) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut out = std::collections::BTreeSet::new();
    for task in tasks {
        let path = task?.path().join("children");
        let text = match std::fs::read_to_string(path) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for word in text.split_whitespace() {
            let child = word
                .parse::<i32>()
                .map_err(|_| io::Error::other("process_child_invalid"))?;
            if child > 0 {
                out.insert(child);
            }
            if out.len() >= MAX_LIVE_DESCENDANTS {
                return Err(io::Error::other("process_descendant_budget"));
            }
        }
    }
    Ok(out.into_iter().collect())
}

fn current(id: Identity) -> io::Result<bool> {
    Ok(info(id.pid)?.is_some_and(|p| p.identity == id && !p.inactive))
}
fn signal(id: Identity, signal: i32) -> io::Result<()> {
    if !current(id)? {
        return Ok(());
    }
    if unsafe { libc::kill(id.pid, signal) } < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}

pub(super) struct Descendants {
    root: Option<Identity>,
    live: BTreeMap<i32, Identity>,
    complete: bool,
    last_refresh: Instant,
    observed: usize,
}
impl Descendants {
    pub(super) fn new(pid: u32) -> Self {
        let found = info(pid as i32);
        let complete = found.is_ok();
        let root = found.ok().flatten().map(|v| v.identity);
        Self {
            root,
            live: BTreeMap::new(),
            complete,
            last_refresh: Instant::now() - Duration::from_secs(1),
            observed: 0,
        }
    }
    pub(super) fn refresh(&mut self) {
        if self.last_refresh.elapsed() >= Duration::from_millis(25) {
            self.collect(false);
            self.last_refresh = Instant::now();
        }
    }
    fn collect(&mut self, stop: bool) {
        self.live.retain(|_, id| match current(*id) {
            Ok(v) => v,
            Err(_) => {
                self.complete = false;
                false
            }
        });
        let mut queue: VecDeque<_> = self
            .root
            .into_iter()
            .chain(self.live.values().copied())
            .collect();
        let mut visited = BTreeMap::new();
        while let Some(id) = queue.pop_front() {
            if visited.insert(id.pid, id).is_some() {
                continue;
            }
            match current(id) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => {
                    self.complete = false;
                    continue;
                }
            }
            // Pause only this process's birth-bound owner nodes before walking
            // their children, preventing a normal node from forking during its
            // own cleanup. Previously observed orphan nodes remain reachable.
            if stop && signal(id, libc::SIGSTOP).is_err() {
                self.complete = false;
            }
            let pids = match child_pids(id.pid) {
                Ok(v) => v,
                Err(_) => {
                    self.complete = false;
                    continue;
                }
            };
            for pid in pids {
                let child = match info(pid) {
                    Ok(Some(v)) => v,
                    Ok(None) => continue,
                    Err(_) => {
                        self.complete = false;
                        continue;
                    }
                };
                if child.parent != id.pid || child.inactive {
                    continue;
                }
                if self.live.len() >= MAX_LIVE_DESCENDANTS {
                    self.complete = false;
                    break;
                }
                if self.live.insert(pid, child.identity).is_none() {
                    self.observed += 1;
                }
                queue.push_back(child.identity);
            }
        }
    }
    pub(super) fn stop(&mut self) -> (bool, usize) {
        self.collect(true);
        for id in self.live.values().copied() {
            if signal(id, libc::SIGKILL).is_err() {
                self.complete = false;
            }
        }
        // The direct Child is reaped by the existing ProcessTree, not by a new
        // child owner. Birth-bound orphan nodes can be adopted by the OS; a
        // zombie is already unable to execute or write further artifacts.
        let until = Instant::now() + Duration::from_millis(100);
        loop {
            let mut running = false;
            for id in self.live.values().copied() {
                match current(id) {
                    Ok(true) => running = true,
                    Ok(false) => {}
                    Err(_) => self.complete = false,
                }
            }
            if !running {
                break;
            }
            if Instant::now() >= until {
                self.complete = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        (self.complete, self.observed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrong_birth_identity_does_not_signal_a_live_unrelated_process() {
        let actual = info(std::process::id() as i32).unwrap().unwrap().identity;
        let wrong = Identity {
            birth: actual.birth.wrapping_add(1),
            ..actual
        };
        assert!(!current(wrong).unwrap());
        signal(wrong, libc::SIGSTOP).unwrap();
        assert!(current(actual).unwrap());
    }
}
