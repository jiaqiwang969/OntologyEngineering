#![cfg(unix)]
//! Real OS locks and inode identity, with fixtures wholly below the skill's var.
use oe_core::AdvisoryFileLock;
use std::{
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, symlink},
    },
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Barrier,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
// macOS may briefly inherit an open flock description in a concurrently
// spawned test child before its CLOEXEC takes effect. Keep subprocess tests
// isolated from the thread-only race; the race itself remains eight-way.
static PROCESS_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
    root: PathBuf,
    _serial: std::sync::MutexGuard<'static, ()>,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let serial = PROCESS_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let path = root.join(format!(
            "var/maintenance/rust-tests/file-lock-{name}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        Self {
            root: path,
            _serial: serial,
        }
    }
    fn lock(&self) -> PathBuf {
        self.root.join("coordination.lock")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
}

fn rejected(path: &Path, exclusive: bool, code: &str) {
    let error = AdvisoryFileLock::acquire(path, exclusive)
        .err()
        .expect("lock must reject");
    assert!(
        error.to_string().contains(code),
        "unexpected error: {error}"
    );
}
fn child(path: &Path, exclusive: bool, accepted: bool) {
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_child_probe", "--nocapture"])
        .env_clear()
        .env("OE_LOCK_CHILD_PATH", path)
        .env("OE_LOCK_CHILD_EXCLUSIVE", if exclusive { "1" } else { "0" })
        .env("OE_LOCK_CHILD_ACCEPTED", if accepted { "1" } else { "0" })
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "child stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("lock-probe-completed"));
}
#[test]
fn lock_child_probe() {
    let Some(path) = std::env::var_os("OE_LOCK_CHILD_PATH") else {
        return;
    };
    let exclusive = std::env::var("OE_LOCK_CHILD_EXCLUSIVE").unwrap() == "1";
    let accepted = std::env::var("OE_LOCK_CHILD_ACCEPTED").unwrap() == "1";
    let result = AdvisoryFileLock::acquire(Path::new(&path), exclusive);
    if accepted {
        result.unwrap().ensure_current().unwrap();
    } else {
        assert_eq!(
            result.err().expect("child must conflict").to_string(),
            "resource_busy"
        );
    }
    println!("lock-probe-completed");
}

#[test]
fn shared_holders_coexist_but_exclusive_waits_for_every_drop_in_real_processes() {
    let f = Fixture::new("shared");
    let path = f.lock();
    let first = AdvisoryFileLock::acquire(&path, false).unwrap();
    let identity = fs::metadata(&path).unwrap();
    let second = AdvisoryFileLock::acquire(&path, false).unwrap();
    child(&path, false, true);
    child(&path, true, false);
    rejected(&path, true, "resource_busy");
    drop(first);
    second.ensure_current().unwrap();
    child(&path, true, false);
    drop(second);
    child(&path, true, true);
    let after = fs::metadata(&path).unwrap();
    assert_eq!((identity.dev(), identity.ino()), (after.dev(), after.ino()));
    assert_eq!(after.len(), 0);
    assert_eq!(after.nlink(), 1);
}

#[test]
fn exclusive_blocks_shared_and_exclusive_then_drop_releases_without_unlinking() {
    let f = Fixture::new("exclusive");
    let path = f.lock();
    let guard = AdvisoryFileLock::acquire(&path, true).unwrap();
    let inode = fs::metadata(&path).unwrap().ino();
    let started = Instant::now();
    rejected(&path, false, "resource_busy");
    rejected(&path, true, "resource_busy");
    assert!(started.elapsed() < Duration::from_secs(1));
    child(&path, false, false);
    child(&path, true, false);
    drop(guard);
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
    child(&path, false, true);
    child(&path, true, true);
}

#[test]
fn concurrent_exclusive_attempts_have_exactly_one_winner() {
    let f = Fixture::new("concurrent");
    let path = f.lock();
    drop(AdvisoryFileLock::acquire(&path, true).unwrap());
    let start = Arc::new(Barrier::new(8));
    let attempted = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let start = start.clone();
            let attempted = attempted.clone();
            let path = path.clone();
            std::thread::spawn(move || {
                start.wait();
                let result = AdvisoryFileLock::acquire(&path, true);
                attempted.wait();
                match result {
                    Ok(guard) => {
                        guard.ensure_current().unwrap();
                        true
                    }
                    Err(error) => {
                        assert_eq!(error.to_string(), "resource_busy");
                        false
                    }
                }
            })
        })
        .collect();
    let winners = threads
        .into_iter()
        .map(|t| usize::from(t.join().unwrap()))
        .sum::<usize>();
    assert_eq!(winners, 1);
    AdvisoryFileLock::acquire(&path, true)
        .unwrap()
        .ensure_current()
        .unwrap();
}

#[test]
fn symlinks_at_leaf_or_parent_never_select_the_target_inode() {
    let f = Fixture::new("symlink");
    let real = f.root.join("real");
    fs::create_dir(&real).unwrap();
    let target = real.join("target.lock");
    fs::write(&target, []).unwrap();
    let leaf = f.root.join("linked.lock");
    symlink(&target, &leaf).unwrap();
    assert!(AdvisoryFileLock::acquire(&leaf, true).is_err());
    let parent = f.root.join("linked-parent");
    symlink(&real, &parent).unwrap();
    assert!(AdvisoryFileLock::acquire(&parent.join("target.lock"), false).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"");
    let guard = AdvisoryFileLock::acquire(&target, true).unwrap();
    fs::rename(&target, real.join("retained.lock")).unwrap();
    symlink(real.join("retained.lock"), &target).unwrap();
    assert!(guard.ensure_current().is_err());
}

#[test]
fn unlinked_or_replaced_coordination_inode_invalidates_the_old_guard() {
    let f = Fixture::new("retarget");
    let path = f.lock();
    let old = AdvisoryFileLock::acquire(&path, true).unwrap();
    let old_inode = fs::metadata(&path).unwrap().ino();
    fs::remove_file(&path).unwrap();
    assert!(old.ensure_current().is_err());
    let replacement = AdvisoryFileLock::acquire(&path, true).unwrap();
    assert_ne!(fs::metadata(&path).unwrap().ino(), old_inode);
    assert!(old.ensure_current().is_err());
    replacement.ensure_current().unwrap();
    // Dropping an obsolete guard must not unlink or release the replacement.
    drop(old);
    assert!(path.exists());
    child(&path, false, false);
    drop(replacement);
    child(&path, true, true);
}

#[test]
fn replacing_a_parent_directory_invalidates_the_held_path() {
    let f = Fixture::new("parent-retarget");
    let parent = f.root.join("owner");
    fs::create_dir(&parent).unwrap();
    let path = parent.join("lock");
    let guard = AdvisoryFileLock::acquire(&path, true).unwrap();
    fs::rename(&parent, f.root.join("old-owner")).unwrap();
    fs::create_dir(&parent).unwrap();
    fs::write(&path, []).unwrap();
    assert!(guard.ensure_current().is_err());
    let current = AdvisoryFileLock::acquire(&path, true).unwrap();
    current.ensure_current().unwrap();
    assert!(f.root.join("old-owner/lock").is_file());
}

#[test]
fn only_empty_regular_single_link_files_are_accepted_and_remain_required() {
    let f = Fixture::new("shape");
    let path = f.lock();
    fs::write(&path, b"not a lock file").unwrap();
    rejected(&path, true, "lock_not_private_regular_empty_file");
    assert_eq!(fs::read(&path).unwrap(), b"not a lock file");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(AdvisoryFileLock::acquire(&path, true).is_err());
    fs::remove_dir(&path).unwrap();
    let fifo = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let started = Instant::now();
    assert!(AdvisoryFileLock::acquire(&path, true).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    fs::remove_file(&path).unwrap();
    fs::write(&path, []).unwrap();
    let hard = f.root.join("hard-link");
    fs::hard_link(&path, &hard).unwrap();
    rejected(&path, true, "lock_not_private_regular_empty_file");
    fs::remove_file(&hard).unwrap();
    let guard = AdvisoryFileLock::acquire(&path, true).unwrap();
    fs::hard_link(&path, &hard).unwrap();
    assert!(guard.ensure_current().is_err());
    fs::remove_file(&hard).unwrap();
    guard.ensure_current().unwrap();
    fs::write(&path, b"changed after acquisition").unwrap();
    assert!(guard.ensure_current().is_err());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn exclusive_directory_rename_moves_exact_tree_and_preserves_inode() {
    let f = Fixture::new("rename-success");
    let from = f.root.join("prepared");
    let to = f.root.join("environment");
    fs::create_dir(&from).unwrap();
    fs::create_dir(from.join("nested")).unwrap();
    fs::write(from.join("nested/evidence.bin"), [0, 1, 2, 255]).unwrap();
    let inode = fs::metadata(&from).unwrap().ino();
    oe_core::rename_directory_exclusive(&from, &to).unwrap();
    assert!(!from.exists());
    assert_eq!(fs::metadata(&to).unwrap().ino(), inode);
    assert_eq!(
        fs::read(to.join("nested/evidence.bin")).unwrap(),
        [0, 1, 2, 255]
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn exclusive_rename_never_replaces_an_empty_nonempty_or_file_target() {
    let f = Fixture::new("rename-existing");
    for kind in ["empty", "nonempty", "file"] {
        let from = f.root.join(format!("source-{kind}"));
        let to = f.root.join(format!("target-{kind}"));
        fs::create_dir(&from).unwrap();
        fs::write(from.join("source.txt"), b"prepared candidate").unwrap();
        if kind == "file" {
            fs::write(&to, b"external owner").unwrap();
        } else {
            fs::create_dir(&to).unwrap();
            if kind == "nonempty" {
                fs::write(to.join("owner.txt"), b"external owner").unwrap();
            }
        }
        let target_inode = fs::metadata(&to).unwrap().ino();
        assert!(oe_core::rename_directory_exclusive(&from, &to).is_err());
        assert_eq!(fs::metadata(&to).unwrap().ino(), target_inode);
        assert_eq!(
            fs::read(from.join("source.txt")).unwrap(),
            b"prepared candidate"
        );
        match kind {
            "file" => assert_eq!(fs::read(&to).unwrap(), b"external owner"),
            "nonempty" => assert_eq!(fs::read(to.join("owner.txt")).unwrap(), b"external owner"),
            _ => assert_eq!(fs::read_dir(&to).unwrap().count(), 0),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn exclusive_rename_rejects_symlinked_parents_and_leaves_source_intact() {
    let f = Fixture::new("rename-links");
    let real = f.root.join("real");
    fs::create_dir(&real).unwrap();
    let from = real.join("source");
    fs::create_dir(&from).unwrap();
    fs::write(from.join("keep"), b"keep").unwrap();
    let linked = f.root.join("linked");
    symlink(&real, &linked).unwrap();
    assert!(
        oe_core::rename_directory_exclusive(&linked.join("source"), &f.root.join("target"))
            .is_err()
    );
    assert!(oe_core::rename_directory_exclusive(&from, &linked.join("target")).is_err());
    let source_link = f.root.join("source-link");
    symlink(&from, &source_link).unwrap();
    assert!(oe_core::rename_directory_exclusive(&source_link, &f.root.join("target")).is_err());
    let target_link = f.root.join("target-link");
    symlink(real.join("absent"), &target_link).unwrap();
    assert!(oe_core::rename_directory_exclusive(&from, &target_link).is_err());
    assert_eq!(fs::read(from.join("keep")).unwrap(), b"keep");
    assert!(!real.join("target").exists());
    assert!(!f.root.join("target").exists());
    assert!(!real.join("absent").exists());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn racing_directory_publications_do_not_overwrite_the_winner() {
    let f = Fixture::new("rename-race");
    let target = f.root.join("published");
    let start = Arc::new(Barrier::new(8));
    let mut workers = Vec::new();
    for i in 0..8 {
        let from = f.root.join(format!("source-{i}"));
        fs::create_dir(&from).unwrap();
        fs::write(from.join("owner"), i.to_string()).unwrap();
        let target = target.clone();
        let start = start.clone();
        workers.push(std::thread::spawn(move || {
            start.wait();
            (
                i,
                oe_core::rename_directory_exclusive(&from, &target).is_ok(),
            )
        }));
    }
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|(_, ok)| *ok).count(), 1);
    let winner = results.iter().find(|(_, ok)| *ok).unwrap().0;
    assert_eq!(
        fs::read_to_string(target.join("owner")).unwrap(),
        winner.to_string()
    );
    for (i, ok) in results {
        let source = f.root.join(format!("source-{i}"));
        if ok {
            assert!(!source.exists());
        } else {
            assert_eq!(
                fs::read_to_string(source.join("owner")).unwrap(),
                i.to_string()
            );
        }
    }
}
