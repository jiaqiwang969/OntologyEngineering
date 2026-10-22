//! Finite SSH observation, not a native process supervisor. Closing a local SSH
//! channel says nothing about whether its remote request was executed. Callers
//! reconcile the same job; this module never resends a request.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use oe_core::{SkillRoot, process::resolve_executable, write_new, write_new_bytes};
use oe_nx_protocol::{
    BlobIdentity, Digest, Frame, MAX_PAYLOAD_BYTES, MAX_RECORD_BYTES, Request, RootIdentity,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    io::{ErrorKind, Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_FRAME: usize = MAX_RECORD_BYTES + MAX_PAYLOAD_BYTES as usize + 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProfile {
    pub schema: String,
    /// Trusted operator transport configuration, never shell text or secrets.
    pub ssh_command: Vec<String>,
    pub remote_root: String,
    pub nx_root: String,
    pub controller_path: String,
    pub controller: BlobIdentity,
    pub expected_root: Option<RootIdentity>,
    pub legacy_review: Option<LegacyReviewPin>,
    /// Separate, source-pinned review of one terminated unknown dispatch's
    /// writer occupancy. This never changes its native result or authorizes a retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_writer_review: Option<DispatchWriterReviewPin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyReviewPin {
    pub remote_path: String,
    pub sha256: Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchWriterReviewPin {
    pub remote_path: String,
    pub sha256: Digest,
}

fn checked_string(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 8192 && !value.chars().any(char::is_control),
        "invalid transport string"
    );
    Ok(())
}

impl RemoteProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "cad-agent.nx-remote-profile/v2",
            "profile schema differs"
        );
        ensure!(
            (2..=128).contains(&self.ssh_command.len()),
            "invalid transport argv"
        );
        for value in
            self.ssh_command
                .iter()
                .chain([&self.remote_root, &self.nx_root, &self.controller_path])
        {
            checked_string(value)?;
        }
        let name = Path::new(&self.ssh_command[0])
            .file_name()
            .and_then(|v| v.to_str());
        ensure!(
            matches!(name, Some("ssh" | "fleet")),
            "unsupported transport executable"
        );
        if name == Some("fleet") {
            ensure!(
                self.ssh_command.len() >= 3 && self.ssh_command[1] == "ssh",
                "expected fleet ssh"
            );
            ensure!(
                !self.ssh_command.iter().any(|v| v == "--print-only"),
                "profile must not be print-only"
            );
        }
        ensure!(
            self.controller.bytes > 0 && self.controller.bytes <= MAX_PAYLOAD_BYTES,
            "invalid companion identity"
        );
        if let Some(root) = &self.expected_root {
            root.validate()?;
        }
        if let Some(review) = &self.legacy_review {
            checked_string(&review.remote_path)?;
        }
        if let Some(review) = &self.dispatch_writer_review {
            checked_string(&review.remote_path)?;
            ensure!(
                self.expected_root.is_some(),
                "dispatch writer review requires a pinned root"
            );
        }
        Ok(())
    }

    /// Only process launch and executable-byte pinning live in this bootstrap.
    /// Stage, dispatch, state, locking and result decisions stay in Rust.
    pub fn powershell_bootstrap(&self) -> Result<String> {
        self.validate()?;
        fn quote(s: &str) -> String {
            format!("'{}'", s.replace('\'', "''"))
        }
        let expected = self
            .expected_root
            .as_ref()
            .map(|r| {
                format!(
                    ", '--expected-host', {}, '--expected-volume', {}, '--expected-directory', {}",
                    quote(&r.host_id),
                    quote(&r.volume_id),
                    quote(&r.directory_id)
                )
            })
            .unwrap_or_default();
        let review = self.legacy_review.as_ref().map(|r| format!(
            ", '--legacy-review', ([Environment]::ExpandEnvironmentVariables({})), '--legacy-review-sha256', '{}'",
            quote(&r.remote_path), r.sha256.as_str()
        )).unwrap_or_default();
        let dispatch_review = self.dispatch_writer_review.as_ref().map(|r| format!(
            ", '--dispatch-writer-review', ([Environment]::ExpandEnvironmentVariables({})), '--dispatch-writer-review-sha256', '{}'",
            quote(&r.remote_path), r.sha256.as_str()
        )).unwrap_or_default();
        Ok(format!(
            "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; \
             $exe=[Environment]::ExpandEnvironmentVariables({exe}); \
             $root=[Environment]::ExpandEnvironmentVariables({root}); \
             $nx=[Environment]::ExpandEnvironmentVariables({nx}); \
             $pin=[IO.File]::Open($exe,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read); \
             try {{ $sha=[Security.Cryptography.SHA256]::Create(); \
             try {{ $hex=([BitConverter]::ToString($sha.ComputeHash($pin))).Replace('-','').ToLowerInvariant(); }} \
             finally {{ $sha.Dispose(); }} \
             if ($pin.Length -ne {bytes} -or $hex -ne '{digest}') {{ throw 'companion identity differs'; }} \
             function Quote-NativeArgument([string]$value) {{ \
                 $b=[Text.StringBuilder]::new(); [void]$b.Append([char]34); $slashes=0; \
                 foreach($ch in $value.ToCharArray()) {{ \
                     if($ch -eq [char]92) {{ $slashes++; continue; }} \
                     if($ch -eq [char]34) {{ [void]$b.Append([char]92, (2*$slashes+1)); }} \
                     else {{ [void]$b.Append([char]92, $slashes); }} \
                     [void]$b.Append($ch); $slashes=0; \
                 }} \
                 [void]$b.Append([char]92, (2*$slashes)); [void]$b.Append([char]34); $b.ToString(); \
             }} \
             $arguments=@('--root', $root, '--nx-root', $nx{expected}{review}{dispatch_review}, '--serve-one'); \
             $info=[Diagnostics.ProcessStartInfo]::new(); $info.FileName=$exe; \
             $info.Arguments=(($arguments | ForEach-Object {{ Quote-NativeArgument $_ }}) -join ' '); \
             $info.UseShellExecute=$false; $info.RedirectStandardInput=$false; \
             $info.RedirectStandardOutput=$false; $info.RedirectStandardError=$false; \
             $child=[Diagnostics.Process]::Start($info); \
             try {{ $child.WaitForExit(); $code=$child.ExitCode; }} finally {{ $child.Dispose(); }} \
             }} finally {{ $pin.Dispose(); }}; exit $code",
            exe = quote(&self.controller_path),
            root = quote(&self.remote_root),
            nx = quote(&self.nx_root),
            bytes = self.controller.bytes,
            digest = self.controller.sha256.as_str()
        ))
    }
}

#[derive(Debug)]
pub struct Observation {
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub report: Value,
}
impl Observation {
    /// This describes transport only. Even a zero exit needs a bound, valid
    /// protocol response before a remote job outcome can be known.
    pub fn transport_completed(&self) -> bool {
        self.status == "completed" && self.exit_code == Some(0)
    }

    /// Command-specific root, job and result bindings are checked by callers.
    pub fn response_frame(&self) -> Result<Frame> {
        ensure!(
            self.transport_completed(),
            "transport incomplete; reconcile original job"
        );
        let mut reader = std::io::Cursor::new(&self.stdout);
        let frame = oe_nx_protocol::read_frame(&mut reader)?;
        ensure!(
            reader.position() as usize == self.stdout.len(),
            "trailing response bytes"
        );
        Ok(frame)
    }
}

pub struct Exchange<'a> {
    pub argv: &'a [String],
    pub input: &'a [u8],
    pub timeout: Duration,
    pub output_limit: usize,
}

#[cfg(unix)]
fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    ensure!(
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
        "cannot set nonblocking transport pipe"
    );
    Ok(())
}

fn drain(pipe: &mut impl Read, output: &mut Vec<u8>, limit: usize) -> std::io::Result<bool> {
    let mut buffer = [0u8; 16 * 1024];
    // Bound each turn so a flooding peer cannot prevent deadline checks.
    for _ in 0..16 {
        let room = (limit + 1).saturating_sub(output.len());
        if room == 0 {
            return Ok(false);
        }
        let count = room.min(buffer.len());
        match pipe.read(&mut buffer[..count]) {
            Ok(0) => return Ok(true),
            Ok(n) => output.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(false),
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}

/// A new evidence directory and request intent are created before spawn. Timeout
/// terminates ONLY the local SSH process; never a remote PID or process group.
/// Remote execution is unknown until same-job status/result reconciliation.
#[cfg(unix)]
pub fn exchange(root: &SkillRoot, evidence: &Path, request: &Exchange<'_>) -> Result<Observation> {
    exchange_tracked(root, evidence, request, None)
}

#[cfg(unix)]
fn exchange_tracked(
    root: &SkillRoot,
    evidence: &Path,
    request: &Exchange<'_>,
    dispatch_started: Option<&Cell<bool>>,
) -> Result<Observation> {
    ensure!(
        !request.argv.is_empty() && request.argv.len() <= 160,
        "invalid invocation argv"
    );
    for arg in request.argv {
        checked_string(arg)?;
    }
    ensure!(
        request.input.len() <= MAX_FRAME,
        "transport input exceeds bound"
    );
    ensure!(
        (1..=MAX_FRAME).contains(&request.output_limit),
        "transport output limit invalid"
    );
    ensure!(
        request.timeout > Duration::ZERO && request.timeout <= Duration::from_secs(3600),
        "transport timeout invalid"
    );
    let executable = resolve_executable(Path::new(&request.argv[0]))?;
    let canonical = std::fs::canonicalize(&executable)?;
    let identity = oe_core::fingerprint_file(&canonical, 512 * 1024 * 1024)?;
    let directory = root.new_private_dir(evidence)?;
    write_new(
        &directory.join("intent.json"),
        &json!({
            "schema":"cad-agent.nx-transport-intent/v2", "argv":request.argv,
            "executable":{"path":executable,"canonical":canonical,"identity":identity},
            "input":BlobIdentity::of(request.input),"timeout_seconds":request.timeout.as_secs_f64(),
            "output_limit":request.output_limit, "retry":"never", "remote_execution":"not_observed"
        }),
    )?;
    // Request is retained exactly; no credential is part of this protocol.
    write_new_bytes(&directory.join("request.bin"), request.input)?;
    let mut command = Command::new(&executable);
    command
        .args(&request.argv[1..])
        .current_dir(root.path())
        .stdin(if request.input.is_empty() {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut status = "spawn_failed";
    let mut exit_code = None;
    let mut sent = 0usize;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut error_kind = None;
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let mut process_finished = false;
    match command.spawn() {
        Err(e) => error_kind = Some(format!("{:?}", e.kind())),
        Ok(mut child) => {
            // Record the local transport boundary before any fallible post-spawn
            // I/O. This does not assert delivery or remote dispatch, but failures
            // from here cannot safely be described as local input rejection.
            if let Some(started) = dispatch_started {
                started.set(true);
            }
            let mut input = child.stdin.take();
            let mut out = child.stdout.take().context("missing stdout pipe")?;
            let mut err = child.stderr.take().context("missing stderr pipe")?;
            let setup = nonblocking(&out)
                .and_then(|_| nonblocking(&err))
                .and_then(|_| {
                    if let Some(pipe) = &input {
                        nonblocking(pipe)
                    } else {
                        Ok(())
                    }
                });
            if setup.is_err() {
                status = "pipe_setup_failed";
            } else {
                status = loop {
                    if let Some(pipe) = &mut input {
                        match pipe.write(&request.input[sent..]) {
                            Ok(0) => break "input_closed_early",
                            Ok(n) => {
                                sent += n;
                                if sent == request.input.len() {
                                    input.take();
                                }
                            }
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    ErrorKind::Interrupted | ErrorKind::WouldBlock
                                ) => {}
                            Err(e) => {
                                error_kind = Some(format!("{:?}", e.kind()));
                                break "input_failed";
                            }
                        }
                    }
                    for (pipe, bytes, eof) in [
                        (&mut out as &mut dyn Read, &mut stdout, &mut stdout_eof),
                        (&mut err as &mut dyn Read, &mut stderr, &mut stderr_eof),
                    ] {
                        if !*eof {
                            match drain(&mut &mut *pipe, bytes, request.output_limit) {
                                Ok(done) => *eof = done,
                                Err(e) => {
                                    error_kind = Some(format!("{:?}", e.kind()));
                                }
                            }
                        }
                    }
                    if error_kind.is_some() {
                        break "output_failed";
                    }
                    if stdout.len() > request.output_limit || stderr.len() > request.output_limit {
                        break "output_limit";
                    }
                    match child.try_wait() {
                        Ok(Some(code)) => {
                            process_finished = true;
                            exit_code = code.code();
                            if stdout_eof && stderr_eof {
                                break if sent != request.input.len() {
                                    "input_incomplete"
                                } else if code.success() {
                                    "completed"
                                } else {
                                    "process_failed"
                                };
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            error_kind = Some(format!("{:?}", e.kind()));
                            break "wait_failed";
                        }
                    }
                    if started.elapsed() >= request.timeout {
                        break "observation_timeout";
                    }
                    std::thread::sleep(Duration::from_millis(2));
                };
            }
            input.take();
            // Child::kill uses the held child identity, not a remote PID. No
            // process group kill: a detached peer/monitor may legitimately live.
            if !process_finished {
                let _ = child.kill();
                if let Ok(code) = child.wait() {
                    exit_code = code.code();
                }
            }
            let _ = drain(&mut out, &mut stdout, request.output_limit);
            let _ = drain(&mut err, &mut stderr, request.output_limit);
        }
    }
    let executable_unchanged = std::fs::canonicalize(&executable).ok().as_ref() == Some(&canonical)
        && oe_core::fingerprint_file(&canonical, 512 * 1024 * 1024).is_ok_and(|v| v == identity);
    let observed_status = status;
    if !executable_unchanged {
        status = "executable_changed";
    }
    let stdout_truncated = stdout.len() > request.output_limit;
    let stderr_truncated = stderr.len() > request.output_limit;
    stdout.truncate(request.output_limit);
    stderr.truncate(request.output_limit);
    write_new_bytes(&directory.join("stdout.bin"), &stdout)?;
    write_new_bytes(&directory.join("stderr.bin"), &stderr)?;
    let report = json!({
        "schema":"cad-agent.nx-transport-observation/v2", "status":status,"observed_status":observed_status,
        "exit_code":exit_code,"error_kind":error_kind,"input_bytes_sent":sent,
        "input_complete":sent == request.input.len(), "stdout_eof":stdout_eof,"stderr_eof":stderr_eof,
        "elapsed_seconds":started.elapsed().as_secs_f64(),"executable_unchanged":executable_unchanged,
        "stdout":{"identity":BlobIdentity::of(&stdout),"truncated":stdout_truncated},
        "stderr":{"identity":BlobIdentity::of(&stderr),"truncated":stderr_truncated},
        "remote_execution":"requires_bound_response_or_same_job_reconciliation", "automatic_retry":false
    });
    write_new(&directory.join("observation.json"), &report)?;
    Ok(Observation {
        status: status.into(),
        exit_code,
        stdout,
        stderr,
        report,
    })
}

#[cfg(not(unix))]
pub fn exchange(root: &SkillRoot, evidence: &Path, request: &Exchange<'_>) -> Result<Observation> {
    exchange_tracked(root, evidence, request, None)
}

#[cfg(not(unix))]
fn exchange_tracked(
    _root: &SkillRoot,
    _evidence: &Path,
    _request: &Exchange<'_>,
    _started: Option<&Cell<bool>>,
) -> Result<Observation> {
    anyhow::bail!("local NX SSH observation currently requires Unix; Windows companion is separate")
}

pub fn resolve_route(
    root: &SkillRoot,
    profile: &RemoteProfile,
    evidence: &Path,
) -> Result<Vec<String>> {
    profile.validate()?;
    if Path::new(&profile.ssh_command[0])
        .file_name()
        .is_some_and(|n| n == "ssh")
    {
        return Ok(profile.ssh_command.clone());
    }
    let mut argv = profile.ssh_command.clone();
    argv.push("--print-only".into());
    let observation = exchange(
        root,
        evidence,
        &Exchange {
            argv: &argv,
            input: &[],
            timeout: Duration::from_secs(60),
            output_limit: 64 * 1024,
        },
    )?;
    ensure!(
        observation.transport_completed(),
        "Fleet resolution failed; inspect its evidence"
    );
    let text = std::str::from_utf8(&observation.stdout).context("Fleet response not UTF8")?;
    let resolved = shlex::split(text).context("Fleet returned invalid argv quoting")?;
    ensure!(
        resolved.len() >= 2
            && Path::new(&resolved[0])
                .file_name()
                .is_some_and(|n| n == "ssh"),
        "Fleet did not return SSH argv"
    );
    for arg in &resolved {
        checked_string(arg)?;
    }
    Ok(resolved)
}

/// Keep one SSH destination, preserve quoted option values, and fix the channel
/// conditions needed by binary stdin/stdout even when ssh_config requests a TTY.
/// Query/forward-only/background modes and explicit conflicting options fail
/// before a remote invocation; no arbitrary remote command is accepted here.
pub fn binary_ssh_route(route: &[String]) -> Result<Vec<String>> {
    ensure!(
        route.len() >= 2 && Path::new(&route[0]).file_name().is_some_and(|n| n == "ssh"),
        "not resolved SSH argv"
    );
    for arg in route {
        checked_string(arg)?;
    }
    let fixed = [
        ("RequestTTY", "no"),
        ("StdinNull", "no"),
        ("SessionType", "default"),
        ("ForkAfterAuthentication", "no"),
        ("RemoteCommand", "none"),
        ("BatchMode", "yes"),
    ];
    let mut index = 1usize;
    while index < route.len() && route[index].starts_with('-') {
        let arg = &route[index];
        if arg == "--" {
            index += 1;
            break;
        }
        ensure!(arg.len() > 1, "invalid SSH option");
        for (offset, option) in arg[1..].char_indices() {
            ensure!(
                !"tfnNGVOQWs".contains(option),
                "SSH mode conflicts with finite binary exchange"
            );
            if "BbcDEeFIiJLlmoPpRSw".contains(option) {
                let tail = &arg[offset + 2..];
                let value = if tail.is_empty() {
                    index += 1;
                    route
                        .get(index)
                        .context("missing SSH option value")?
                        .as_str()
                } else {
                    tail
                };
                if option == 'o' {
                    let (name, value) = value
                        .split_once(['=', ' ', '\t'])
                        .context("SSH -o requires key/value")?;
                    let value = value.trim_start_matches(['=', ' ', '\t']);
                    for (key, required) in fixed {
                        ensure!(
                            !name.eq_ignore_ascii_case(key) || value.eq_ignore_ascii_case(required),
                            "explicit SSH {key} conflicts with binary exchange"
                        );
                    }
                }
                break;
            }
            ensure!(
                "46AaCgKkMqTvXxYy".contains(option),
                "unsupported SSH option"
            );
        }
        index += 1;
    }
    ensure!(
        index + 1 == route.len() && !route[index].starts_with('-'),
        "SSH route must end at exactly one destination"
    );
    let mut argv = vec![route[0].clone(), "-T".into()];
    for (key, value) in fixed {
        argv.extend(["-o".into(), format!("{key}={value}")]);
    }
    argv.extend_from_slice(&route[1..]);
    Ok(argv)
}

pub fn invocation(profile: &RemoteProfile, route: &[String]) -> Result<Vec<String>> {
    profile.validate()?;
    let mut argv = binary_ssh_route(route)?;
    let source = profile.powershell_bootstrap()?;
    let utf16 = source
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    argv.extend(
        [
            "powershell.exe",
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
        ]
        .map(str::to_owned),
    );
    argv.push(STANDARD.encode(utf16));
    Ok(argv)
}

pub fn invoke(
    root: &SkillRoot,
    profile: &RemoteProfile,
    frame: &[u8],
    evidence: &Path,
    timeout: Duration,
) -> Result<Observation> {
    invoke_inner(root, profile, frame, evidence, timeout, None)
}

/// Track only the actual remote-request transport, not Fleet route resolution.
/// Kept internal so callers cannot mistake this for an NX execution receipt.
pub(crate) fn invoke_tracked(
    root: &SkillRoot,
    profile: &RemoteProfile,
    frame: &[u8],
    evidence: &Path,
    timeout: Duration,
    started: &Cell<bool>,
) -> Result<Observation> {
    invoke_inner(root, profile, frame, evidence, timeout, Some(started))
}

fn invoke_inner(
    root: &SkillRoot,
    profile: &RemoteProfile,
    frame: &[u8],
    evidence: &Path,
    timeout: Duration,
    started: Option<&Cell<bool>>,
) -> Result<Observation> {
    let mut reader = std::io::Cursor::new(frame);
    let (request, _) = oe_nx_protocol::read_request(&mut reader)?;
    ensure!(
        reader.position() as usize == frame.len(),
        "trailing request bytes"
    );
    if !matches!(request, Request::Probe {}) {
        let expected = profile
            .expected_root
            .as_ref()
            .context("probe and pin remote root first")?;
        if let Request::Stage { job, .. } = &request {
            ensure!(
                job.root == *expected,
                "prepared job root differs from profile"
            );
        }
    }
    let directory = root.new_private_dir(evidence)?;
    let route = resolve_route(root, profile, &directory.join("route"))?;
    let argv = invocation(profile, &route)?;
    exchange_tracked(
        root,
        &directory.join("request"),
        &Exchange {
            argv: &argv,
            input: frame,
            timeout,
            output_limit: MAX_FRAME,
        },
        started,
    )
}
