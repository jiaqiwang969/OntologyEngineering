//! Bounded development geometry adapter protocol. Native OCCT parity is pending.
//!
//! An adapter must accept `--step FILE --out DIRECTORY`, emit one JSON object on
//! stdout, and write artifacts into that fresh directory. Existing Python
//! geometry tools have different interfaces; this is not a port of those tools.
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{SkillRoot, fingerprint_file, open_regular_file, sha256, write_new};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub const PROTOCOL_VERSION: &str = "2.0";
pub const JOB_SCHEMA: &str = "ontology-engineering.geometry-job/v2";
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
const MAX_SCRIPT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ARTIFACTS: usize = 128;

#[derive(Debug, Clone)]
pub struct GeometryJob {
    job_id: String,
    script: PathBuf,
    input_sha256: String,
    input_path: PathBuf,
    extra_args: Vec<String>,
    output_dir: PathBuf,
    timeout: Duration,
}
impl GeometryJob {
    pub fn new(script: PathBuf, input_path: PathBuf, output_dir: PathBuf) -> Result<Self> {
        ensure!(
            safe_relative(&script),
            "script must be a nonempty skill-relative path"
        );
        let input_path = if input_path.is_absolute() {
            input_path
        } else {
            std::env::current_dir()?.join(input_path)
        };
        let input_sha256 = fingerprint_file(&input_path, crate::intake::MAX_STEP_BYTES)?.sha256;
        Ok(Self {
            job_id: format!(
                "geo-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ),
            script,
            input_sha256,
            input_path,
            extra_args: Vec::new(),
            output_dir,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        })
    }
    pub fn job_id(&self) -> &str {
        &self.job_id
    }
    pub fn with_arg(mut self, flag: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_args.extend([flag.into(), value.into()]);
        self
    }
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }
    pub fn to_identity(&self) -> Value {
        json!({"protocol":PROTOCOL_VERSION,"job_id":self.job_id,"script":self.script,"input_sha256":self.input_sha256})
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Completed,
    ProcessFailed,
    TimedOut,
    OutputLimit,
    ProtocolError,
    InputChanged,
    ScriptChanged,
    InvalidArtifacts,
}
/// Produced only by actual dispatch. Its private receipt binding is also the
/// input to readback; a caller-supplied status string cannot construct it.
#[derive(Debug)]
pub struct JobResult {
    pub job_id: String,
    pub status: JobStatus,
    pub output: Value,
    pub stdout_sha256: String,
    pub elapsed_seconds: f64,
    pub exit_code: Option<i32>,
    pub(crate) output_dir: PathBuf,
    pub(crate) record_sha256: String,
    pub(crate) input_sha256: String,
}
impl JobResult {
    pub fn to_json(&self) -> Value {
        json!({"job_id":self.job_id,"status":self.status,"exit_code":self.exit_code,
            "stdout_sha256":self.stdout_sha256,"elapsed_seconds":self.elapsed_seconds,"output":self.output,
            "receipt":{"path":self.output_dir.join(format!("{}.job.json",self.job_id)),"sha256":self.record_sha256},
            "engineering_acceptance":"not_determined","native_occt_acceptance":"not_run"})
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactIdentity {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
pub(crate) fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}
/// Fingerprints bind both names and bytes, not concatenated file contents.
pub(crate) fn artifacts(root: &Path, record_name: &str) -> Result<Vec<ArtifactIdentity>> {
    fn visit(
        root: &Path,
        dir: &Path,
        depth: usize,
        record_name: &str,
        found: &mut Vec<ArtifactIdentity>,
        nodes: &mut usize,
        total_bytes: &mut u64,
    ) -> Result<()> {
        ensure!(depth <= 16, "geometry_artifact_depth_limit");
        oe_core::no_symlinks(dir)?;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if depth == 0 && (entry.file_name() == "_oe" || entry.file_name() == record_name) {
                continue;
            }
            *nodes = nodes
                .checked_add(1)
                .context("geometry_artifact_node_overflow")?;
            ensure!(*nodes <= 4096, "geometry_artifact_node_limit");
            let kind = entry.file_type()?;
            ensure!(!kind.is_symlink(), "geometry_artifact_symlink");
            if kind.is_dir() {
                visit(
                    root,
                    &path,
                    depth + 1,
                    record_name,
                    found,
                    nodes,
                    total_bytes,
                )?;
            } else {
                ensure!(kind.is_file(), "geometry_artifact_not_regular");
                ensure!(found.len() < MAX_ARTIFACTS, "geometry_artifact_count_limit");
                let is_json = path.extension().is_some_and(|s| s == "json");
                let limit = if is_json {
                    MAX_OUTPUT_BYTES as u64
                } else {
                    crate::intake::MAX_STEP_BYTES
                };
                let size = entry.metadata()?.len();
                *total_bytes = total_bytes
                    .checked_add(size)
                    .context("geometry_artifact_size_overflow")?;
                ensure!(
                    *total_bytes <= crate::intake::MAX_STEP_BYTES,
                    "geometry_artifact_total_size_limit"
                );
                let identity = fingerprint_file(&path, limit)?;
                ensure!(identity.bytes == size, "geometry_artifact_size_changed");
                if is_json {
                    let value = oe_core::json::strict_json(&oe_core::read_regular_bytes(
                        &path,
                        MAX_OUTPUT_BYTES,
                    )?)?;
                    ensure!(value.is_object(), "geometry_artifact_json_must_be_object");
                    ensure!(
                        fingerprint_file(&path, limit)? == identity,
                        "geometry_artifact_changed_during_read"
                    );
                }
                let relative = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("geometry_artifact_path_not_utf8")?
                    .replace('\\', "/");
                found.push(ArtifactIdentity {
                    path: relative,
                    sha256: identity.sha256,
                    bytes: identity.bytes,
                });
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    visit(root, root, 0, record_name, &mut found, &mut 0, &mut 0)?;
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}
pub(crate) fn artifacts_sha256(files: &[ArtifactIdentity]) -> Result<String> {
    Ok(sha256(serde_json::to_vec(files)?))
}
/// Compatibility entry for callers already running at the skill root.
pub fn dispatch(python: &Path, job: &GeometryJob) -> Result<JobResult> {
    dispatch_in(&SkillRoot::new(std::env::current_dir()?)?, python, job)
}
/// Setup rejection returns Err. Executed failures return an observed result and
/// retain partial streams and the exact process receipt. No command is retried.
pub fn dispatch_in(root: &SkillRoot, python: &Path, job: &GeometryJob) -> Result<JobResult> {
    ensure!(
        python.is_absolute(),
        "geometry_interpreter_must_be_absolute"
    );
    ensure!(
        job.timeout > Duration::ZERO && job.timeout <= Duration::from_secs(12 * 60 * 60),
        "geometry_timeout_out_of_range"
    );
    ensure!(
        job.extra_args.len() <= 128 && job.extra_args.iter().all(|a| a.len() <= 16384),
        "geometry_argument_limit"
    );
    for pair in job.extra_args.chunks_exact(2) {
        ensure!(
            pair[0].starts_with("--")
                && !pair[0].contains('=')
                && !["--step", "--out"].contains(&pair[0].as_str()),
            "geometry_reserved_or_invalid_argument"
        );
    }
    let script = root.path().join(&job.script);
    let script_identity = fingerprint_file(&script, MAX_SCRIPT_BYTES)?;
    let input_identity = fingerprint_file(&job.input_path, crate::intake::MAX_STEP_BYTES)?;
    ensure!(
        input_identity.sha256 == job.input_sha256,
        "geometry_input_changed_before_dispatch"
    );
    let output_dir = root.new_private_dir(&job.output_dir)?;
    let metadata_dir = output_dir.join("_oe");
    oe_core::create_new_private_dir(&metadata_dir)?;
    let snapshot_dir = metadata_dir.join("input");
    oe_core::create_new_private_dir(&snapshot_dir)?;
    let snapshot = snapshot_dir.join(
        job.input_path
            .file_name()
            .context("geometry_input_name_missing")?,
    );
    let mut input = open_regular_file(&job.input_path, crate::intake::MAX_STEP_BYTES)?
        .take(crate::intake::MAX_STEP_BYTES + 1);
    let mut copy = oe_core::create_new_regular_file(&snapshot)?;
    let copied = std::io::copy(&mut input, &mut copy)?;
    ensure!(
        copied <= crate::intake::MAX_STEP_BYTES,
        "geometry_input_exceeds_limit"
    );
    copy.sync_all()?;
    ensure!(
        fingerprint_file(&snapshot, crate::intake::MAX_STEP_BYTES)? == input_identity,
        "geometry_input_changed_during_snapshot"
    );
    let mut arguments = vec![
        script.as_os_str().to_owned(),
        "--step".into(),
        snapshot.as_os_str().to_owned(),
        "--out".into(),
        output_dir.as_os_str().to_owned(),
    ];
    arguments.extend(job.extra_args.iter().map(Into::into));
    let started = Instant::now();
    let process = run_command(
        &CommandSpec {
            executable: python.to_owned(),
            arguments,
            cwd: root.path().to_owned(),
            environment: BTreeMap::new(),
            remove_environment: vec![
                "PYTHONPATH".into(),
                "PYTHONHOME".into(),
                "PYTHONSTARTUP".into(),
            ],
            timeout: job.timeout,
            output_limit: MAX_OUTPUT_BYTES,
        },
        &metadata_dir.join("process"),
    )?;
    let mut errors = Vec::new();
    let mut status = match process.status.as_str() {
        "completed" => JobStatus::Completed,
        "timed_out" => JobStatus::TimedOut,
        "output_limit" => JobStatus::OutputLimit,
        _ => JobStatus::ProcessFailed,
    };
    let output = match oe_core::json::strict_json(&process.stdout) {
        Ok(value) if value.is_object() => value,
        _ => {
            errors.push("geometry_stdout_is_not_one_json_object".to_owned());
            if status == JobStatus::Completed {
                status = JobStatus::ProtocolError;
            }
            Value::Null
        }
    };
    if !fingerprint_file(&job.input_path, crate::intake::MAX_STEP_BYTES)
        .is_ok_and(|i| i == input_identity)
    {
        errors.push("geometry_input_changed_during_execution".to_owned());
        status = JobStatus::InputChanged;
    }
    if !fingerprint_file(&script, MAX_SCRIPT_BYTES).is_ok_and(|i| i == script_identity) {
        errors.push("geometry_script_changed_during_execution".to_owned());
        status = JobStatus::ScriptChanged;
    }
    let record_name = format!("{}.job.json", job.job_id);
    let outputs = match artifacts(&output_dir, &record_name) {
        Ok(files) if !files.is_empty() => files,
        result => {
            errors.push(match result {
                Ok(_) => "geometry_no_output_artifacts".to_owned(),
                Err(e) => format!("geometry_invalid_artifacts: {e:#}"),
            });
            if status == JobStatus::Completed {
                status = JobStatus::InvalidArtifacts;
            }
            Vec::new()
        }
    };
    let stdout_sha256 = sha256(&process.stdout);
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let receipt = fingerprint_file(
        &metadata_dir.join("process/process.json"),
        crate::acceptance::MAX_JOB_RECORD_BYTES as u64,
    )?;
    let record = json!({"schema":JOB_SCHEMA,"identity":job.to_identity(),"script_identity":script_identity,
        "input_identity":input_identity,"input_snapshot":snapshot,"status":status,"exit_code":process.exit_code,
        "elapsed_seconds":elapsed_seconds,"stdout_sha256":stdout_sha256,
        "stderr_excerpt":String::from_utf8_lossy(&process.stderr).chars().take(2048).collect::<String>(),
        "process_receipt":receipt,"outputs":outputs,"outputs_sha256":artifacts_sha256(&outputs)?,"errors":errors,
        "engineering_acceptance":"not_determined","native_occt_acceptance":"not_run"});
    let record_path = output_dir.join(record_name);
    write_new(&record_path, &record)?;
    let record_sha256 =
        fingerprint_file(&record_path, crate::acceptance::MAX_JOB_RECORD_BYTES as u64)?.sha256;
    Ok(JobResult {
        job_id: job.job_id.clone(),
        status,
        output,
        stdout_sha256,
        elapsed_seconds,
        exit_code: process.exit_code,
        output_dir,
        record_sha256,
        input_sha256: job.input_sha256.clone(),
    })
}
