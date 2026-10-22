//! Existing Browser Harness 0.1.13 POSIX wire contract. One newline-delimited
//! JSON request per AF_UNIX connection. No daemon discovery, startup or repair.
use anyhow::{Context, Result, ensure};
use oe_core::{create_new_private_dir, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixStream,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct EndpointIdentity {
    device: u64,
    inode: u64,
    uid: u32,
    mode: u32,
}

pub(super) struct IpcTransport {
    path: PathBuf,
    timeout: Duration,
    limit: usize,
    endpoint: EndpointIdentity,
    daemon_pid: Option<u32>,
    protocol: &'static str,
}

impl IpcTransport {
    pub(super) fn new(path: &Path, timeout: Duration, limit: usize) -> Result<Self> {
        ensure!(path.is_absolute(), "harness_absolute_socket_required");
        let endpoint = endpoint_identity(path)?;
        Ok(Self {
            path: path.to_owned(),
            timeout,
            limit,
            endpoint,
            daemon_pid: None,
            protocol: "ontology-engineering.browser-harness-ipc/v1",
        })
    }
    pub(super) fn new_peekaboo(path: &Path, timeout: Duration, limit: usize) -> Result<Self> {
        let mut transport = Self::new(path, timeout, limit)?;
        transport.protocol = "ontology-engineering.peekaboo-ipc/v1";
        Ok(transport)
    }
    pub(super) fn handshake(&mut self, output: &Path) -> Result<()> {
        let response = self.request(&json!({"meta":"ping"}), output)?;
        ensure!(response["pong"] == true, "existing_harness_daemon_required");
        let pid = response["pid"]
            .as_u64()
            .filter(|v| *v > 0 && *v < i32::MAX as u64)
            .context("harness_daemon_pid_invalid")? as u32;
        ensure!(
            matches!(
                response["browser_kind"].as_str(),
                Some("cloud" | "cdp" | "local")
            ),
            "harness_daemon_kind_unconfirmed"
        );
        self.daemon_pid = Some(pid);
        Ok(())
    }
    pub(super) fn request(&mut self, request: &Value, output: &Path) -> Result<Value> {
        self.request_with_retention(request, output, true)
    }
    pub(super) fn request_memory(&mut self, request: &Value, output: &Path) -> Result<Value> {
        self.request_with_retention(request, output, false)
    }
    fn request_with_retention(
        &mut self,
        request: &Value,
        output: &Path,
        persist_response: bool,
    ) -> Result<Value> {
        ensure!(request.is_object(), "harness_request_object_required");
        let raw = serde_json::to_vec(request)?;
        // The daemon's asyncio StreamReader default limit is 64 KiB. No request
        // may rely on a larger server-side configuration to accept its frame.
        ensure!(raw.len() < 65536, "harness_request_frame_too_large");
        create_new_private_dir(output)?;
        write_new_bytes(&output.join("request.json"), &raw)?;
        let started = Instant::now();
        let mut response = Vec::new();
        let result = self.exchange(&raw, &mut response);
        if persist_response {
            write_new_bytes(&output.join("response.raw"), &response)?;
        }
        write_new(
            &output.join("transport.json"),
            &json!({
                "schema":self.protocol,
                "endpoint":{"path":self.path,"device":self.endpoint.device,"inode":self.endpoint.inode,
                    "uid":self.endpoint.uid,"mode":self.endpoint.mode},
                "daemon_pid_observed":self.daemon_pid,"contract_version":if self.protocol.contains("peekaboo"){"oev2.peekaboo-bridge/1"}else{"0.1.13"},
                "actual_daemon_package_version":"not_advertised_by_protocol",
                "request_sha256":oe_core::sha256(&raw),"response_sha256":oe_core::sha256(&response),
                "request_bytes":raw.len(),"response_bytes":response.len(),"response_limit":self.limit,
                "elapsed_seconds":started.elapsed().as_secs_f64(),"timeout_seconds":self.timeout.as_secs_f64(),
                "status":if result.is_ok(){"completed"}else{"unconfirmed"},
                "response_retention":if persist_response{"private_raw_file"}else{"memory_only_hash_retained"},
                "request_retries":0,"daemon_started":false,"semantic_authority":false
            }),
        )?;
        result
    }
    #[cfg(unix)]
    fn exchange(&self, raw: &[u8], response: &mut Vec<u8>) -> Result<Value> {
        ensure!(
            endpoint_identity(&self.path)? == self.endpoint,
            "harness_endpoint_changed"
        );
        let deadline = Instant::now() + self.timeout;
        let mut stream = connect_bounded(&self.path, deadline)?;
        verify_peer(&stream, self.endpoint.uid)?;
        stream
            .set_nonblocking(true)
            .context("harness_nonblocking_io_failed")?;
        ensure!(
            endpoint_identity(&self.path)? == self.endpoint,
            "harness_endpoint_changed"
        );
        let mut frame = raw.to_vec();
        frame.push(b'\n');
        let mut sent = 0;
        while sent < frame.len() {
            poll_io(&stream, libc::POLLOUT, deadline)?;
            let n = match stream.write(&frame[sent..]) {
                Ok(n) => n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(_) => anyhow::bail!("harness_request_write_unconfirmed"),
            };
            ensure!(n > 0, "harness_request_write_unconfirmed");
            sent += n;
        }
        let mut chunk = [0u8; 16384];
        loop {
            poll_io(&stream, libc::POLLIN, deadline)?;
            let room = (self.limit + 1).saturating_sub(response.len());
            ensure!(room > 0, "harness_response_limit_exceeded");
            let take = room.min(chunk.len());
            let n = match stream.read(&mut chunk[..take]) {
                Ok(n) => n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(_) => anyhow::bail!("harness_response_unconfirmed"),
            };
            ensure!(n > 0, "harness_response_truncated");
            response.extend_from_slice(&chunk[..n]);
            ensure!(
                response.len() <= self.limit,
                "harness_response_limit_exceeded"
            );
            if let Some(end) = response.iter().position(|v| *v == b'\n') {
                ensure!(end + 1 == response.len(), "harness_response_extra_frames");
                break;
            }
        }
        ensure!(
            endpoint_identity(&self.path)? == self.endpoint,
            "harness_endpoint_changed"
        );
        let value =
            oe_core::json::strict_json(response).context("harness_invalid_response_json")?;
        ensure!(value.is_object(), "harness_response_object_required");
        Ok(value)
    }
    #[cfg(not(unix))]
    fn exchange(&self, _raw: &[u8], _response: &mut Vec<u8>) -> Result<Value> {
        anyhow::bail!("harness_unix_transport_unsupported_platform")
    }
}

#[cfg(unix)]
fn endpoint_identity(path: &Path) -> Result<EndpointIdentity> {
    oe_core::no_symlinks(path)?;
    let m = fs::symlink_metadata(path).context("existing_harness_socket_unavailable")?;
    ensure!(
        m.file_type().is_socket(),
        "existing_harness_socket_required"
    );
    let uid = unsafe { libc::geteuid() };
    ensure!(
        m.uid() == uid && m.mode() & 0o077 == 0,
        "harness_private_owned_socket_required"
    );
    Ok(EndpointIdentity {
        device: m.dev(),
        inode: m.ino(),
        uid: m.uid(),
        mode: m.mode() & 0o777,
    })
}
#[cfg(not(unix))]
fn endpoint_identity(_path: &Path) -> Result<EndpointIdentity> {
    anyhow::bail!("harness_unix_transport_unsupported_platform")
}

#[cfg(unix)]
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|v| !v.is_zero())
        .context("harness_transport_timeout")
}

#[cfg(unix)]
fn poll_io(stream: &UnixStream, events: libc::c_short, deadline: Instant) -> Result<()> {
    use std::os::fd::AsRawFd;
    loop {
        let ms = remaining(deadline)?
            .as_millis()
            .max(1)
            .min(i32::MAX as u128) as i32;
        let mut poll = libc::pollfd {
            fd: stream.as_raw_fd(),
            events,
            revents: 0,
        };
        let status = unsafe { libc::poll(&mut poll, 1, ms) };
        if status < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        ensure!(status > 0, "harness_transport_timeout");
        ensure!(
            poll.revents & libc::POLLNVAL == 0,
            "harness_socket_unavailable"
        );
        return Ok(());
    }
}

#[cfg(unix)]
fn connect_bounded(path: &Path, deadline: Instant) -> Result<UnixStream> {
    use std::os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    };
    let bytes = path.as_os_str().as_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    ensure!(
        !bytes.contains(&0) && bytes.len() < address.sun_path.len(),
        "harness_socket_path_too_long"
    );
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (to, from) in address.sun_path.iter_mut().zip(bytes) {
        *to = *from as libc::c_char;
    }
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    {
        address.sun_len = std::mem::size_of::<libc::sockaddr_un>() as u8;
    }
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    ensure!(raw >= 0, "harness_socket_create_failed");
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    ensure!(
        flags >= 0
            && unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
        "harness_nonblocking_failed"
    );
    ensure!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } >= 0,
        "harness_cloexec_failed"
    );
    let status = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
        )
    };
    if status != 0 {
        let error = std::io::Error::last_os_error();
        ensure!(
            matches!(
                error.raw_os_error(),
                Some(libc::EINPROGRESS) | Some(libc::EWOULDBLOCK)
            ),
            "harness_connect_failed"
        );
        loop {
            let ms = remaining(deadline)?
                .as_millis()
                .max(1)
                .min(i32::MAX as u128) as i32;
            let mut poll = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut poll, 1, ms) };
            if result < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {
                continue;
            }
            ensure!(result > 0, "harness_connect_timeout");
            let mut error: libc::c_int = 0;
            let mut len = std::mem::size_of_val(&error) as libc::socklen_t;
            ensure!(
                unsafe {
                    libc::getsockopt(
                        fd.as_raw_fd(),
                        libc::SOL_SOCKET,
                        libc::SO_ERROR,
                        (&mut error as *mut libc::c_int).cast(),
                        &mut len,
                    )
                } == 0
                    && error == 0,
                "harness_connect_failed"
            );
            break;
        }
    }
    ensure!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags) } >= 0,
        "harness_blocking_restore_failed"
    );
    Ok(UnixStream::from(fd))
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
))]
fn verify_peer(stream: &UnixStream, expected_uid: u32) -> Result<()> {
    use std::os::fd::AsRawFd;
    let (mut uid, mut gid) = (0, 0);
    ensure!(
        unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == 0
            && uid == expected_uid,
        "harness_peer_identity_mismatch"
    );
    Ok(())
}
#[cfg(any(target_os = "linux", target_os = "android"))]
fn verify_peer(stream: &UnixStream, expected_uid: u32) -> Result<()> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of_val(&cred) as libc::socklen_t;
    ensure!(
        unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut len,
            )
        } == 0
            && cred.uid == expected_uid,
        "harness_peer_identity_mismatch"
    );
    Ok(())
}
#[cfg(all(
    unix,
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "linux",
        target_os = "android"
    ))
))]
fn verify_peer(_stream: &UnixStream, _expected_uid: u32) -> Result<()> {
    anyhow::bail!("harness_peer_identity_unsupported_platform")
}
