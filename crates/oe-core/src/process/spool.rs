//! Explicit segmented raw retention. The legacy stream APIs are unchanged.
//! The caller selects finite raw limits and owns purpose, paths and authority.
use super::*;
use crate::{FileIdentity, create_new_regular_file, read_regular_bytes};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;

const MAX_PREVIEW: usize = 4 * 1024 * 1024;
const MAX_SEGMENT: usize = 4 * 1024 * 1024;
const MAX_SEGMENTS: usize = 16_384;

/// These cap retained preview and each file. CommandSpec.output_limit keeps
/// its original raw per-stream meaning; this API does not turn it into memory.
#[derive(Debug, Clone, Copy)]
pub struct SpoolLimits {
    pub preview_bytes: usize,
    pub segment_bytes: usize,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
struct PhysicalFile {
    identity: FileIdentity,
    mode: u32,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

fn physical(path: &Path, limit: usize) -> Result<PhysicalFile> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "spool_not_regular_file"
    );
    let identity = fingerprint_file(path, limit as u64)?;
    let after = fs::symlink_metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            metadata.dev() == after.dev() && metadata.ino() == after.ino(),
            "spool_file_path_changed"
        );
        ensure!(
            metadata.permissions().mode() & 0o777 == 0o600,
            "spool_file_not_private"
        );
        ensure!(
            after.permissions().mode() & 0o777 == 0o600,
            "spool_file_mode_changed"
        );
        Ok(PhysicalFile {
            identity,
            mode: 0o600,
            device: after.dev(),
            inode: after.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        ensure!(
            metadata.len() == after.len() && metadata.modified()? == after.modified()?,
            "spool_file_changed"
        );
        Ok(PhysicalFile {
            identity,
            mode: u32::from(after.permissions().readonly()),
        })
    }
}

#[derive(Debug, serde::Serialize)]
struct Segment {
    ordinal: usize,
    byte_offset: u64,
    path: PathBuf,
    file: PhysicalFile,
}

#[derive(Debug)]
struct RetainedStream {
    directory: PathBuf,
    segments: Vec<Segment>,
    bytes: u64,
    sha256: String,
    preview_bytes: usize,
    preview_sha256: String,
    eof: bool,
    truncated: bool,
    failure: Option<String>,
    observed_bytes_at_least: u64,
}

impl RetainedStream {
    fn report(&self) -> Value {
        json!({"bytes": self.bytes, "sha256": self.sha256, "segments": self.segments,
            "preview_bytes": self.preview_bytes, "preview_sha256": self.preview_sha256,
            "preview_truncated": self.bytes > self.preview_bytes as u64,
            "eof": self.eof, "truncated": self.truncated, "failure": self.failure,
            "observed_bytes_at_least": self.observed_bytes_at_least})
    }

    fn verify(&self, limit: usize, segment_bytes: usize, preview: &[u8]) -> Result<()> {
        ensure!(
            preview.len() == self.preview_bytes && sha256(preview) == self.preview_sha256,
            "spool_preview_changed"
        );
        let names = fs::read_dir(&self.directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        ensure!(
            names == self.segments.iter().map(|s| s.path.clone()).collect(),
            "spool_segment_inventory_changed"
        );
        let mut digest = Sha256::new();
        let mut offset = 0u64;
        let mut preview_offset = 0usize;
        for (ordinal, segment) in self.segments.iter().enumerate() {
            ensure!(
                segment.ordinal == ordinal
                    && segment.byte_offset == offset
                    && segment.path == self.directory.join(format!("segment-{ordinal:05}.bin")),
                "spool_segment_order_changed"
            );
            ensure!(
                physical(&segment.path, segment_bytes)? == segment.file,
                "spool_segment_changed"
            );
            let bytes = read_regular_bytes(&segment.path, segment_bytes)?;
            ensure!(
                !bytes.is_empty()
                    && sha256(&bytes) == segment.file.identity.sha256
                    && bytes.len() as u64 == segment.file.identity.bytes,
                "spool_segment_content_changed"
            );
            ensure!(
                ordinal + 1 == self.segments.len() || bytes.len() == segment_bytes,
                "spool_short_middle_segment"
            );
            ensure!(
                physical(&segment.path, segment_bytes)? == segment.file,
                "spool_segment_changed_after_read"
            );
            let count = (preview.len() - preview_offset).min(bytes.len());
            ensure!(
                preview[preview_offset..preview_offset + count] == bytes[..count],
                "spool_preview_not_raw_prefix"
            );
            preview_offset += count;
            digest.update(&bytes);
            offset = offset
                .checked_add(bytes.len() as u64)
                .context("spool_offset_overflow")?;
            ensure!(offset <= limit as u64, "spool_raw_budget_changed");
        }
        ensure!(
            offset == self.bytes
                && preview_offset == preview.len()
                && format!("{:x}", digest.finalize()) == self.sha256,
            "spool_complete_digest_changed"
        );
        Ok(())
    }
}

struct Sink {
    directory: PathBuf,
    limits: SpoolLimits,
    segments: Vec<Segment>,
    current: Option<(PathBuf, File, usize)>,
    bytes: u64,
    digest: Sha256,
    preview: Vec<u8>,
}

impl Sink {
    fn new(directory: PathBuf, limits: SpoolLimits) -> Self {
        Self {
            directory,
            limits,
            segments: Vec::new(),
            current: None,
            bytes: 0,
            digest: Sha256::new(),
            preview: Vec::new(),
        }
    }

    fn seal_segment(&mut self) -> Result<()> {
        if let Some((path, file, bytes)) = self.current.take() {
            file.sync_all().context("spool_segment_sync_failed")?;
            let metadata = file.metadata()?;
            let pin = physical(&path, self.limits.segment_bytes)?;
            ensure!(
                pin.identity.bytes == bytes as u64 && metadata.len() == bytes as u64,
                "spool_segment_length_changed"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                ensure!(
                    metadata.dev() == pin.device && metadata.ino() == pin.inode,
                    "spool_open_segment_changed"
                );
            }
            self.segments.push(Segment {
                ordinal: self.segments.len(),
                byte_offset: self.bytes - bytes as u64,
                path,
                file: pin,
            });
        }
        Ok(())
    }

    fn write(&mut self, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            if self.current.is_none() {
                ensure!(
                    self.segments.len() < MAX_SEGMENTS,
                    "spool_segment_count_limit"
                );
                let path = self
                    .directory
                    .join(format!("segment-{:05}.bin", self.segments.len()));
                let file = create_new_regular_file(&path)?;
                self.current = Some((path, file, 0));
            }
            let (_, file, used) = self
                .current
                .as_mut()
                .context("spool_current_segment_missing")?;
            let count = bytes.len().min(self.limits.segment_bytes - *used);
            let written = match file.write(&bytes[..count]) {
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                result => result.context("spool_segment_write_failed")?,
            };
            ensure!(written > 0, "spool_segment_zero_write");
            self.digest.update(&bytes[..written]);
            let preview = written.min(self.limits.preview_bytes - self.preview.len());
            self.preview.extend_from_slice(&bytes[..preview]);
            *used += written;
            self.bytes += written as u64;
            bytes = &bytes[written..];
            if *used == self.limits.segment_bytes {
                self.seal_segment()?;
            }
        }
        Ok(())
    }

    fn finish(
        mut self,
        eof: bool,
        truncated: bool,
        mut failure: Option<String>,
        observed: u64,
    ) -> (RetainedStream, Vec<u8>) {
        if let Err(error) = self.seal_segment() {
            failure = Some(format!("stream_retention_failed:{error}"));
        }
        let retained = RetainedStream {
            directory: self.directory,
            segments: self.segments,
            bytes: self.bytes,
            sha256: format!("{:x}", self.digest.finalize()),
            preview_bytes: self.preview.len(),
            preview_sha256: sha256(&self.preview),
            eof: eof && failure.is_none(),
            truncated,
            failure,
            observed_bytes_at_least: observed,
        };
        (retained, self.preview)
    }
}

fn read_spooled(
    mut pipe: impl Read,
    directory: PathBuf,
    limits: SpoolLimits,
    raw_limit: usize,
    stop: &AtomicBool,
) -> (RetainedStream, Vec<u8>) {
    let mut sink = Sink::new(directory, limits);
    let mut buffer = [0u8; 16 * 1024];
    let mut observed = 0u64;
    loop {
        if stop.load(Ordering::Acquire) {
            return sink.finish(false, false, Some("capture_cancelled".into()), observed);
        }
        let remaining = raw_limit as u64 - sink.bytes;
        let count = ((remaining + 1) as usize).min(buffer.len());
        match pipe.read(&mut buffer[..count]) {
            Ok(0) => return sink.finish(true, false, None, observed),
            Ok(count) => {
                observed += count as u64;
                let retained = count.min(remaining as usize);
                if let Err(error) = sink.write(&buffer[..retained]) {
                    return sink.finish(
                        false,
                        false,
                        Some(format!("stream_retention_failed:{error}")),
                        observed,
                    );
                }
                if count > retained {
                    return sink.finish(false, true, Some("output_limit".into()), observed);
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(error) => {
                return sink.finish(
                    false,
                    false,
                    Some(format!("read_failed:{:?}", error.kind())),
                    observed,
                );
            }
        }
    }
}

/// Only an actual local call constructs the private stream/receipt bindings.
/// Retained content still cannot grant native execution or engineering adoption.
#[derive(Debug)]
pub struct SpoolProcessObservation {
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub report: Value,
    stdout_retained: RetainedStream,
    stderr_retained: RetainedStream,
    receipt_files: Vec<(PathBuf, PhysicalFile)>,
    expected_report: Value,
    raw_limit: usize,
    segment_bytes: usize,
}

impl SpoolProcessObservation {
    /// Recheck exact raw content, inventory, order, physical files and receipts.
    /// This is content verification, not a recreated process/owner capability.
    pub fn verify_retained(&self) -> Result<()> {
        ensure!(
            self.report == self.expected_report
                && self.report["status"] == self.status
                && self.report["exit_code"] == json!(self.exit_code),
            "spool_observation_changed"
        );
        self.stdout_retained
            .verify(self.raw_limit, self.segment_bytes, &self.stdout)?;
        self.stderr_retained
            .verify(self.raw_limit, self.segment_bytes, &self.stderr)?;
        for (path, pin) in &self.receipt_files {
            ensure!(
                physical(path, 16 * 1024 * 1024)? == *pin,
                "spool_receipt_changed"
            );
        }
        Ok(())
    }
}

/// Explicit finite raw output, bounded preview and segmented exclusive files.
/// Uses the original ProcessTree and one caller Instant. Legacy APIs never call
/// this implicitly; native resource/qualification contracts must opt in separately.
pub fn run_command_spooled(
    spec: &CommandSpec,
    evidence_dir: &Path,
    deadline: Instant,
    limits: SpoolLimits,
) -> Result<SpoolProcessObservation> {
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
        limits.preview_bytes <= MAX_PREVIEW
            && limits.preview_bytes <= spec.output_limit
            && (1..=MAX_SEGMENT).contains(&limits.segment_bytes)
            && limits.segment_bytes <= spec.output_limit
            && spec.output_limit.div_ceil(limits.segment_bytes) <= MAX_SEGMENTS,
        "spool_limits_out_of_range"
    );
    ensure!(
        spec.cwd.is_absolute() && spec.cwd.is_dir() && evidence_dir.is_absolute(),
        "spool_paths_unavailable"
    );
    check_borrowed_deadline(Some(deadline), "before_spool_resolve")?;
    let executable = resolve_executable(&spec.executable)?;
    let canonical = fs::canonicalize(&executable)?;
    check_borrowed_deadline(Some(deadline), "before_spool_fingerprint")?;
    let identity = fingerprint_file(&canonical, 512 * 1024 * 1024)?;
    check_borrowed_deadline(Some(deadline), "after_spool_fingerprint")?;
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
    create_new_private_dir(evidence_dir)?;
    let out_dir = evidence_dir.join("stdout-segments");
    let err_dir = evidence_dir.join("stderr-segments");
    create_new_private_dir(&out_dir)?;
    create_new_private_dir(&err_dir)?;
    check_borrowed_deadline(Some(deadline), "after_spool_setup")?;
    let mut cmd = Command::new(&executable);
    cmd.args(&spec.arguments)
        .current_dir(&spec.cwd)
        .envs(&spec.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in &spec.remove_environment {
        cmd.env_remove(key);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    check_borrowed_deadline(Some(deadline), "before_spool_spawn")?;
    let started = Instant::now();
    let process_timeout = spec.timeout.min(
        deadline
            .checked_duration_since(started)
            .context("external_process_absolute_deadline_exceeded:before_spool_spawn")?,
    );
    let mut status = "spawn_failed";
    let mut exit_code = None;
    let mut spawn_error = None;
    let mut cleanup = (true, 0usize);
    let mut stdout_result = None;
    let mut stderr_result = None;
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
            nonblocking(&stdout).and_then(|_| nonblocking(&stderr))?;
            let (send, receive) = mpsc::channel();
            let stop = Arc::new(AtomicBool::new(false));
            let out_send = send.clone();
            let flag = stop.clone();
            let raw_limit = spec.output_limit;
            let directory = out_dir.clone();
            let out_reader = thread::spawn(move || {
                let result = read_spooled(stdout, directory, limits, raw_limit, &flag);
                let _ = out_send.send((true, result));
            });
            let flag = stop.clone();
            let directory = err_dir.clone();
            let err_reader = thread::spawn(move || {
                let result = read_spooled(stderr, directory, limits, raw_limit, &flag);
                let _ = send.send((false, result));
            });
            let mut code = None;
            status = loop {
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                tree.descendants.refresh();
                while let Ok((out, result)) = receive.try_recv() {
                    if out {
                        stdout_result = Some(result);
                    } else {
                        stderr_result = Some(result);
                    }
                }
                let streams = [stdout_result.as_ref(), stderr_result.as_ref()];
                if streams.iter().flatten().any(|r| r.0.truncated) {
                    break "output_limit";
                }
                if streams.iter().flatten().any(|r| r.0.failure.is_some()) {
                    break "stream_retention_failed";
                }
                if code.is_none() {
                    match tree.child.try_wait() {
                        Ok(value) => code = value,
                        Err(_) => break "wait_failed",
                    }
                }
                if let Some(code) = code
                    && streams.iter().all(|r| r.is_some())
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
            cleanup = tree.stop();
            if exit_code.is_none() {
                exit_code = tree.child.try_wait().ok().flatten().and_then(|s| s.code());
            }
            stop.store(true, Ordering::Release);
            let out_joined = out_reader.join().is_ok();
            let err_joined = err_reader.join().is_ok();
            while let Ok((out, result)) = receive.try_recv() {
                if out {
                    stdout_result = Some(result);
                } else {
                    stderr_result = Some(result);
                }
            }
            if !out_joined || !err_joined {
                status = "read_failed";
            }
            if !cleanup.0 {
                status = "cleanup_unverified";
            }
        }
    }
    let observed_status = status;
    let executable_unchanged = fs::canonicalize(&executable).ok().as_ref() == Some(&canonical)
        && fingerprint_file(&canonical, 512 * 1024 * 1024).is_ok_and(|current| current == identity);
    if !executable_unchanged {
        status = "executable_changed";
    }
    let empty = |directory| {
        Sink::new(directory, limits).finish(
            false,
            false,
            Some(
                if spawn_error.is_some() {
                    "spawn_failed"
                } else {
                    "reader_result_missing"
                }
                .into(),
            ),
            0,
        )
    };
    let (stdout_retained, stdout) = stdout_result.unwrap_or_else(|| empty(out_dir));
    let (stderr_retained, stderr) = stderr_result.unwrap_or_else(|| empty(err_dir));
    let capture_complete = matches!(observed_status, "completed" | "process_failed")
        && stdout_retained.eof
        && stderr_retained.eof
        && cleanup.0;
    let borrowed = observed_deadline_gate(deadline, "after_spool_retention", &mut status);
    let mut report = json!({"schema":"ontology-engineering.external-process/segmented-streams/1",
        "status":status, "observed_status":observed_status, "exit_code":exit_code,
        "spawn_error_kind":spawn_error, "executable":{"path":executable,"resolved":canonical,"identity":identity,"unchanged":executable_unchanged},
        "arguments":args,"cwd":spec.cwd,"environment_added_keys":added,"environment_removed_keys":removed,
        "environment_values":"not_recorded","timeout_seconds":spec.timeout.as_secs_f64(),
        "effective_process_timeout_seconds":process_timeout.as_secs_f64(),"per_stream_limit":spec.output_limit,
        "preview_limit":limits.preview_bytes,"segment_limit":limits.segment_bytes,"segment_count_limit":MAX_SEGMENTS,
        "elapsed_seconds":started.elapsed().as_secs_f64(),"capture_complete":capture_complete,
        "stdout":stdout_retained.report(),"stderr":stderr_retained.report(),"borrowed_deadline":borrowed,
        "process_tree":if cfg!(windows){"windows_job_object_create_suspended_assignment"}else{"unix_process_group"},
        "descendant_cleanup":{"state":if cleanup.0{"local_cleanup_completed"}else{"unverified"},"observed_descendants":cleanup.1,
            "backend":if cfg!(target_os="macos"){"macos_libproc_observed_pid_birth_subtree"}else if cfg!(target_os="linux"){"linux_proc_observed_pid_birth_subtree"}else if cfg!(windows){"existing_windows_job_object"}else{"existing_unix_process_group_only"},
            "scope":"Original ProcessTree backend and locally observed PID/birth cancellation; no hostile containment or complete descendant attestation"},
        "semantic_execution":"not_inferred","native_or_engineering_authority":"not_granted",
        "stream_retention":"complete_raw_segments_with_bounded_preview",
        "resource_scope":"Raw bytes, each segment, preview and segment count are finite. Peak process memory, total cost and hard synchronous return deadline are not certified."});
    let receipt = evidence_dir.join("process.json");
    write_new(&receipt, &report)?;
    let mut receipt_files = vec![(receipt.clone(), physical(&receipt, 16 * 1024 * 1024)?)];
    if Instant::now() >= deadline && status == "completed" {
        status = "deadline_exceeded";
        report["status"] = json!(status);
        report["borrowed_deadline"] =
            deadline_observation(deadline, "after_spool_receipt_retention");
        let rejection = evidence_dir.join("deadline-rejection.json");
        report["terminal_receipt"] = json!("deadline-rejection.json");
        write_new(&rejection, &report)?;
        receipt_files.push((rejection.clone(), physical(&rejection, 16 * 1024 * 1024)?));
    }
    Ok(SpoolProcessObservation {
        status: status.into(),
        exit_code,
        stdout,
        stderr,
        expected_report: report.clone(),
        report,
        stdout_retained,
        stderr_retained,
        receipt_files,
        raw_limit: spec.output_limit,
        segment_bytes: limits.segment_bytes,
    })
}
