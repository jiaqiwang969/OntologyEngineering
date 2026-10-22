//! Bounded external programs. Callers own purpose, permitted paths and policy.
//! No shell interpolation, business decisions or semantic authority live here.
use crate::{create_new_private_dir, fingerprint_file, sha256, write_new, write_new_bytes};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

mod spool;
mod stdin_file;
pub use spool::{SpoolLimits, SpoolProcessObservation, run_command_spooled};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix_tree;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub cwd: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
    pub remove_environment: Vec<OsString>,
    pub timeout: Duration,
    /// Per stream, capped at 64 MiB. Limit exhaustion never reports completion.
    pub output_limit: usize,
}

#[derive(Debug)]
pub struct ProcessObservation {
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub report: Value,
}

fn executable_file(path: &Path) -> bool {
    // Batch files would implicitly introduce cmd.exe argument interpretation.
    #[cfg(windows)]
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return false;
    }
    fs::metadata(path).is_ok_and(|m| {
        if !m.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            m.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

/// Keep the invocation path: resolving a venv Python symlink for execution
/// would change sys.prefix. Canonical targets are used only for byte identity.
pub fn resolve_executable(path: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    let path = &if path.extension().is_none() {
        path.with_extension("exe")
    } else {
        path.to_owned()
    };
    if path.is_absolute() || path.components().count() > 1 {
        let p = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        ensure!(executable_file(&p), "external_executable_unavailable");
        return Ok(p);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let directory = if directory.is_absolute() {
                directory
            } else {
                std::env::current_dir()?.join(directory)
            };
            let p = directory.join(path);
            if executable_file(&p) {
                return Ok(p);
            }
        }
    }
    bail!("external_executable_unavailable")
}

fn text(value: &OsStr) -> Result<&str> {
    value.to_str().context("external_process_non_utf8_argument")
}

#[cfg(unix)]
fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    ensure!(
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
        "external_pipe_nonblocking_failed"
    );
    Ok(())
}

trait CapturePipe: Read {
    fn wait_readable(&self) -> std::io::Result<()>;
}

#[cfg(unix)]
impl<T: Read + std::os::fd::AsRawFd> CapturePipe for T {
    fn wait_readable(&self) -> std::io::Result<()> {
        // Retain the original five-millisecond cancellation check interval,
        // but wake when the owned pipe has bytes instead of sleeping through
        // them. The reader owns this descriptor until the thread exits.
        let mut fd = libc::pollfd {
            fd: self.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut fd, 1, 5) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != ErrorKind::Interrupted {
                return Err(error);
            }
        }
        if fd.revents & libc::POLLNVAL != 0 {
            return Err(std::io::Error::new(
                ErrorKind::InvalidInput,
                "capture_pipe_closed",
            ));
        }
        // EOF/HUP and pipe errors are read by the original read() branch.
        Ok(())
    }
}

#[cfg(not(unix))]
impl<T: Read> CapturePipe for T {
    fn wait_readable(&self) -> std::io::Result<()> {
        thread::sleep(Duration::from_millis(5));
        Ok(())
    }
}

fn read_stream(mut pipe: impl CapturePipe, limit: usize, stop: &AtomicBool) -> (Vec<u8>, bool) {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let room = (limit + 1).saturating_sub(bytes.len());
        if room == 0 {
            return (bytes, false);
        }
        let len = room.min(buffer.len());
        match pipe.read(&mut buffer[..len]) {
            Ok(0) => return (bytes, false),
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if stop.load(Ordering::Acquire) {
                    return (bytes, false);
                }
                if pipe.wait_readable().is_err() {
                    return (bytes, true);
                }
            }
            Err(_) => return (bytes, true),
        }
    }
}

struct ProcessTree {
    child: Child,
    cleanup: bool,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    descendants: unix_tree::Descendants,
    #[cfg(windows)]
    job: windows::Job,
}

impl ProcessTree {
    fn spawn(command: &mut Command) -> std::io::Result<Self> {
        #[cfg(windows)]
        windows::prepare_command(command);
        #[cfg(windows)]
        let job = windows::Job::new()?;
        let child = command.spawn()?;
        #[cfg(windows)]
        let mut tree = Self {
            child,
            cleanup: true,
            job,
        };
        #[cfg(not(windows))]
        let tree = Self {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            descendants: unix_tree::Descendants::new(child.id()),
            child,
            cleanup: true,
        };
        #[cfg(windows)]
        if let Err(error) = tree.job.assign(&tree.child) {
            tree.stop();
            return Err(error);
        }
        #[cfg(windows)]
        if let Err(error) = tree.job.resume_primary(tree.child.id()) {
            tree.stop();
            return Err(error);
        }
        Ok(tree)
    }

    fn stop(&mut self) -> (bool, usize) {
        if !self.cleanup {
            return (true, 0);
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let observed = self.descendants.stop();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let observed = (true, 0);
        #[cfg(windows)]
        self.job.terminate();
        stop_child(&mut self.child);
        self.cleanup = false;
        observed
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.stop();
    }
}

fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Outputs are private, exclusive artifacts. A failed command returns its
/// observed state, including partial streams; malformed setup returns an error.
pub fn run_command(spec: &CommandSpec, evidence_dir: &Path) -> Result<ProcessObservation> {
    run_command_with_retention(spec, evidence_dir, true)
}

/// Capture sensitive response streams only in memory. The private process
/// receipt retains status, byte counts and hashes, but no stream files.
/// Arguments are still recorded; callers must never put secrets in arguments.
pub fn run_command_memory(spec: &CommandSpec, evidence_dir: &Path) -> Result<ProcessObservation> {
    run_command_with_retention(spec, evidence_dir, false)
}

/// Execute under the same absolute Instant supplied by the caller. This does
/// not create or renew a budget. Expiry is checked before/after setup and after
/// capture/retention; expired observations cannot report completed success.
/// Synchronous file I/O, spawn, cleanup, joins and artifact writes are not
/// preemptible here, so a hard deadline for function return remains unknown.
/// Malformed/expired setup returns Err without starting an executable. A real
/// attempted command retains its actual streams and process observation.
pub fn run_command_with_deadline(
    spec: &CommandSpec,
    evidence_dir: &Path,
    deadline: Instant,
) -> Result<ProcessObservation> {
    run_command_controlled(spec, evidence_dir, true, Some(deadline), None)
}

/// Replay the exact bounded bytes of a checked regular input file on stdin.
/// Uses the existing ProcessTree and the same caller Instant; it changes no
/// command argument and infers neither consumption nor semantic support.
/// The original no-input APIs and receipts keep their original behavior.
pub fn run_command_with_stdin_file_deadline(
    spec: &CommandSpec,
    evidence_dir: &Path,
    deadline: Instant,
    input: &Path,
    expected: &crate::FileIdentity,
) -> Result<ProcessObservation> {
    run_command_controlled(
        spec,
        evidence_dir,
        true,
        Some(deadline),
        Some((
            input,
            expected,
            spec.output_limit.min(4 * 1024 * 1024),
            false,
        )),
    )
}

/// Explicit input capacity independent of output capture. The caller supplies
/// a bounded resource contract and the SAME absolute deadline. Legacy replay
/// retains min(output_limit, 4MiB); this entry emits version2 stdin evidence.
pub fn run_command_with_bounded_stdin_file_deadline(
    spec: &CommandSpec,
    evidence_dir: &Path,
    deadline: Instant,
    input: &Path,
    expected: &crate::FileIdentity,
    input_limit: usize,
) -> Result<ProcessObservation> {
    ensure!(
        (1..=256 * 1024 * 1024).contains(&input_limit),
        "external_stdin_resource_limit"
    );
    run_command_controlled(
        spec,
        evidence_dir,
        true,
        Some(deadline),
        Some((input, expected, input_limit, true)),
    )
}

fn check_borrowed_deadline(deadline: Option<Instant>, phase: &str) -> Result<()> {
    if let Some(deadline) = deadline {
        ensure!(
            Instant::now() < deadline,
            "external_process_absolute_deadline_exceeded:{phase}"
        );
    }
    Ok(())
}

fn deadline_observation(deadline: Instant, phase: &str) -> Value {
    let remaining = deadline.checked_duration_since(Instant::now());
    json!({"schema":"oe-core.process-borrowed-deadline/1",
        "expired":remaining.is_none_or(|v| v.is_zero()),
        "remaining_seconds":remaining.unwrap_or(Duration::ZERO).as_secs_f64(),
        "checked_phase":phase,
        "scope":"Same caller Instant; expired observations are rejected, no renewed budget",
        "hard_function_return_deadline":"unknown",
        "uncovered":"Synchronous filesystem/CPU/spawn/cleanup/join/retention cannot be preempted"})
}

fn observed_deadline_gate(deadline: Instant, phase: &str, status: &mut &str) -> Value {
    let observation = deadline_observation(deadline, phase);
    if observation["expired"] == true && *status == "completed" {
        *status = "deadline_exceeded";
    }
    observation
}

fn run_command_with_retention(
    spec: &CommandSpec,
    evidence_dir: &Path,
    retain_streams: bool,
) -> Result<ProcessObservation> {
    run_command_controlled(spec, evidence_dir, retain_streams, None, None)
}

fn run_command_controlled(
    spec: &CommandSpec,
    evidence_dir: &Path,
    retain_streams: bool,
    deadline: Option<Instant>,
    input: Option<(&Path, &crate::FileIdentity, usize, bool)>,
) -> Result<ProcessObservation> {
    ensure!(
        cfg!(any(unix, windows)),
        "external_process_tree_transport_unsupported_platform"
    );
    ensure!(
        spec.timeout > Duration::ZERO && spec.timeout <= Duration::from_secs(12 * 60 * 60),
        "external_process_timeout_out_of_range"
    );
    ensure!(
        (1..=64 * 1024 * 1024).contains(&spec.output_limit),
        "external_process_output_limit_out_of_range"
    );
    ensure!(
        spec.cwd.is_absolute() && spec.cwd.is_dir(),
        "external_process_cwd_unavailable"
    );
    check_borrowed_deadline(deadline, "before_resolve")?;
    let executable = resolve_executable(&spec.executable)?;
    check_borrowed_deadline(deadline, "after_resolve")?;
    let canonical = fs::canonicalize(&executable)?;
    check_borrowed_deadline(deadline, "before_fingerprint")?;
    let identity = fingerprint_file(&canonical, 512 * 1024 * 1024)?;
    check_borrowed_deadline(deadline, "after_fingerprint")?;
    let args = spec
        .arguments
        .iter()
        .map(|a| text(a))
        .collect::<Result<Vec<_>>>()?;
    let added = spec
        .environment
        .keys()
        .map(|k| text(k))
        .collect::<Result<Vec<_>>>()?;
    let removed = spec
        .remove_environment
        .iter()
        .map(|k| text(k))
        .collect::<Result<Vec<_>>>()?;
    let input = if let Some((path, expected, input_limit, version2)) = input {
        check_borrowed_deadline(deadline, "before_stdin_check")?;
        let value = stdin_file::CheckedInput::open(path, expected, input_limit, version2)?;
        check_borrowed_deadline(deadline, "after_stdin_check")?;
        Some(value)
    } else {
        None
    };
    check_borrowed_deadline(deadline, "before_create_evidence")?;
    create_new_private_dir(evidence_dir)?;
    check_borrowed_deadline(deadline, "after_create_evidence")?;
    let input = input
        .map(|value| value.snapshot(evidence_dir))
        .transpose()?;
    let mut cmd = Command::new(&executable);
    cmd.args(&spec.arguments)
        .current_dir(&spec.cwd)
        .envs(&spec.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(input) = &input {
        check_borrowed_deadline(deadline, "before_stdin_configuration")?;
        cmd.stdin(input.stdio()?);
        check_borrowed_deadline(deadline, "after_stdin_configuration")?;
    }
    for key in &spec.remove_environment {
        cmd.env_remove(key);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    check_borrowed_deadline(deadline, "before_spawn")?;
    let started = Instant::now();
    let process_timeout = match deadline {
        Some(deadline) => spec.timeout.min(
            deadline
                .checked_duration_since(started)
                .context("external_process_absolute_deadline_exceeded:before_spawn")?,
        ),
        None => spec.timeout,
    };
    let mut status = "spawn_failed";
    let mut exit_code = None;
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut streams = 0;
    let mut spawn_error = None;
    let mut cleanup_observation = (true, 0usize);
    let mut execution_status = "spawn_failed";
    match ProcessTree::spawn(&mut cmd) {
        Err(error) => spawn_error = Some(format!("{:?}", error.kind())),
        Ok(mut tree) => {
            let stdout = tree
                .child
                .stdout
                .take()
                .context("external_stdout_unavailable")?;
            let stderr = tree
                .child
                .stderr
                .take()
                .context("external_stderr_unavailable")?;
            #[cfg(unix)]
            if nonblocking(&stdout)
                .and_then(|_| nonblocking(&stderr))
                .is_err()
            {
                bail!("external_pipe_nonblocking_failed");
            }
            let (sender, receiver) = mpsc::channel();
            let stop = Arc::new(AtomicBool::new(false));
            let limit = spec.output_limit;
            let send = sender.clone();
            let flag = stop.clone();
            let a = thread::spawn(move || {
                let (b, e) = read_stream(stdout, limit, &flag);
                let _ = send.send((true, b, e));
            });
            let flag = stop.clone();
            let b = thread::spawn(move || {
                let (b, e) = read_stream(stderr, limit, &flag);
                let _ = sender.send((false, b, e));
            });
            let mut code = None;
            status = loop {
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                tree.descendants.refresh();
                let mut failure = None;
                while let Ok((out, bytes, failed)) = receiver.try_recv() {
                    if failed {
                        failure = Some("read_failed");
                    }
                    if bytes.len() > spec.output_limit {
                        failure = Some("output_limit");
                    }
                    if out {
                        output = bytes;
                    } else {
                        errors = bytes;
                    }
                    streams += 1;
                }
                if let Some(failure) = failure {
                    break failure;
                }
                if code.is_none() {
                    match tree.child.try_wait() {
                        Ok(value) => code = value,
                        Err(_) => break "wait_failed",
                    }
                }
                if let Some(code) = code
                    && streams == 2
                {
                    exit_code = code.code();
                    break if code.success() {
                        "completed"
                    } else {
                        "process_failed"
                    };
                }
                if started.elapsed() >= process_timeout {
                    break "timed_out";
                }
                thread::sleep(Duration::from_millis(5));
            };
            if status != "completed" && status != "process_failed" {
                cleanup_observation = tree.stop();
                exit_code = tree.child.try_wait().ok().flatten().and_then(|s| s.code());
            }
            stop.store(true, Ordering::Release);
            let first_joined = a.join().is_ok();
            let second_joined = b.join().is_ok();
            let joined = first_joined && second_joined;
            while let Ok((out, bytes, _)) = receiver.try_recv() {
                if out {
                    output = bytes;
                } else {
                    errors = bytes;
                }
                streams += 1;
            }
            if !joined {
                status = "read_failed";
            }
            if joined && matches!(status, "completed" | "process_failed") {
                cleanup_observation = tree.stop();
            }
            execution_status = status;
            if !cleanup_observation.0 {
                status = "cleanup_unverified";
            }
        }
    }
    let exe_unchanged = fs::canonicalize(&executable).ok().as_ref() == Some(&canonical)
        && fingerprint_file(&canonical, 512 * 1024 * 1024).is_ok_and(|after| after == identity);
    let observed_status = execution_status;
    if !exe_unchanged {
        status = "executable_changed";
    }
    let stdin_observation = input
        .as_ref()
        .map(|value| value.observation(spawn_error.is_none()));
    if stdin_observation
        .as_ref()
        .is_some_and(|v| v["unchanged"] != true)
        && status == "completed"
    {
        status = "stdin_source_changed";
    }
    let stdout_truncated = output.len() > spec.output_limit;
    let stderr_truncated = errors.len() > spec.output_limit;
    output.truncate(spec.output_limit);
    errors.truncate(spec.output_limit);
    if retain_streams {
        write_new_bytes(&evidence_dir.join("stdout"), &output)?;
        write_new_bytes(&evidence_dir.join("stderr"), &errors)?;
    }
    let borrowed = deadline
        .map(|at| observed_deadline_gate(at, "after_capture_and_stream_retention", &mut status));
    let mut report = json!({"schema":"ontology-engineering.external-process/v1","status":status,"observed_status":observed_status,
        "exit_code":exit_code,"spawn_error_kind":spawn_error,"executable":{"path":executable,"resolved":canonical,"identity":identity,"unchanged":exe_unchanged},
        "arguments":args,"cwd":spec.cwd,"environment_added_keys":added,"environment_removed_keys":removed,
        "environment_values":"not_recorded","timeout_seconds":spec.timeout.as_secs_f64(),"per_stream_limit":spec.output_limit,
        "elapsed_seconds":started.elapsed().as_secs_f64(),"streams_collected":streams,
        "capture_complete":matches!(observed_status,"completed"|"process_failed") && cleanup_observation.0 && streams==2,
        "process_tree":if cfg!(windows){"windows_job_object_create_suspended_assignment"}else{"unix_process_group"},
        "descendant_cleanup":{"state":if cleanup_observation.0{"local_cleanup_completed"}else{"unverified"},"observed_descendants":cleanup_observation.1,
            "backend":if cfg!(target_os="macos"){"macos_libproc_observed_pid_birth_subtree"}else if cfg!(target_os="linux"){"linux_proc_observed_pid_birth_subtree"}else if cfg!(windows){"existing_windows_job_object"}else{"existing_unix_process_group_only"},
            "scope":"Same existing ProcessTree; cancellation of locally observed PID/birth subtree, not proof of every possible descendant",
            "uncovered":"Rapid exit/reparenting before a 25ms observation, other-Unix separate groups, hostile detachment and hard-real-time/total-resource cost are not certified"},
        "stdout":{"path":retain_streams.then_some("stdout"),"bytes":output.len(),"sha256":sha256(&output),"truncated":stdout_truncated},
        "stderr":{"path":retain_streams.then_some("stderr"),"bytes":errors.len(),"sha256":sha256(&errors),"truncated":stderr_truncated},
        "semantic_execution":"not_inferred"});
    if let Some(stdin) = stdin_observation {
        report["schema"] = json!(if stdin["schema"] == "oe-core.process-stdin-file/2" {
            "ontology-engineering.external-process/stdin-file/2"
        } else {
            "ontology-engineering.external-process/stdin-file/1"
        });
        report["stdin"] = stdin;
    }
    if !retain_streams {
        report["stream_retention"] = json!("memory_only");
    }
    if let Some(borrowed) = borrowed {
        report["borrowed_deadline"] = borrowed;
        report["effective_process_timeout_seconds"] = json!(process_timeout.as_secs_f64());
    }
    write_new(&evidence_dir.join("process.json"), &report)?;
    // The original receipt is retained even when its synchronous publication
    // itself consumes the caller's remaining time. The late rejection is an
    // additive terminal observation, never an overwrite of captured evidence.
    if let Some(deadline) = deadline
        && Instant::now() >= deadline
        && status == "completed"
    {
        status = "deadline_exceeded";
        report["status"] = json!(status);
        report["borrowed_deadline"] =
            deadline_observation(deadline, "after_process_receipt_retention");
        report["terminal_receipt"] = json!("deadline-rejection.json");
        write_new(&evidence_dir.join("deadline-rejection.json"), &report)?;
    }
    Ok(ProcessObservation {
        status: status.into(),
        exit_code,
        stdout: output,
        stderr: errors,
        report,
    })
}

#[cfg(test)]
mod borrowed_deadline_gate_tests {
    use super::*;

    #[test]
    fn expired_acceptance_observation_does_not_treat_completed_as_timely() {
        let expired = Instant::now() - Duration::from_millis(1);
        assert!(check_borrowed_deadline(Some(expired), "fixed_gate_control").is_err());
        let mut status = "completed";
        let gate = observed_deadline_gate(expired, "fixed_gate_control", &mut status);
        assert_eq!(status, "deadline_exceeded");
        assert_eq!(gate["expired"], true);
        assert_eq!(gate["hard_function_return_deadline"], "unknown");
        assert_eq!(gate["remaining_seconds"], 0.0);
    }
}
