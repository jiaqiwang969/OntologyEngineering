//! Finite detached batch jobs. No credentials are copied into job artifacts.
use super::*;
use std::process::{Command, Stdio};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Job {
    pub input: BatchInput,
    pub journal: PathBuf,
    pub credential_file: Option<PathBuf>,
    pub fixture_response: Option<PathBuf>,
    pub experiment: bool,
    pub compatibility: Option<PathBuf>,
    pub compatibility_root: Option<PathBuf>,
}

pub(super) fn start(root: &SkillRoot, directory: &Path, job: Job, prepared: &Value) -> Result<u8> {
    let manifest = json!({"schema":"oev2.judgment-background-job/1","job":job,
        "input_sha256":prepared["input_sha256"],"deployment_sha256":prepared["deployment_sha256"],
        "scope":"finite_candidate_judgments"});
    spawn(root, directory, manifest)
}

pub(crate) fn start_route(
    root: &SkillRoot,
    directory: &Path,
    job: crate::route_batch::Run,
    identity: &Value,
) -> Result<u8> {
    let manifest = json!({"schema":"oev2.route-background-job/1","job":job,"route_identity":identity,
        "scope":"finite_adaptive_route_candidates"});
    spawn(root, directory, manifest)
}

fn supported(manifest: &Value) -> bool {
    manifest["schema"] == "oev2.judgment-background-job/1"
        || manifest["schema"] == "oev2.route-background-job/1"
}
fn spawn(root: &SkillRoot, directory: &Path, mut manifest: Value) -> Result<u8> {
    ensure!(cfg!(unix), "background_platform_not_received");
    let path = root.new_private_dir(directory)?;
    let exe = std::env::current_exe()?;
    let identity = oe_core::fingerprint_file(&exe, 1024 * 1024 * 1024)?;
    manifest["root"] = json!(root.path());
    manifest["executable_sha256"] = json!(identity.sha256);
    let manifest_path = path.join("manifest.json");
    write_new(&manifest_path, &manifest)?;
    let digest = sha256(read_regular_bytes(&manifest_path, 1024 * 1024)?);
    let stdout = oe_core::create_new_regular_file(&path.join("events.jsonl"))?;
    let stderr = oe_core::create_new_regular_file(&path.join("stderr.txt"))?;
    let mut command = Command::new(exe);
    command
        .current_dir(root.path())
        .arg("--root")
        .arg(root.path())
        .args(["judgment", "background-worker", "--job"])
        .arg(&path)
        .arg("--manifest-sha256")
        .arg(&digest)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let child = command.spawn().context("background_spawn_failed")?;
    let receipt = json!({"schema":"oev2.judgment-background-spawn/1","status":"spawned",
        "pid":child.id(),"job":path,"manifest_sha256":digest,
        "completion":"not_yet_observed","engineering_adoption":"not_performed"});
    write_new(&path.join("spawn.json"), &receipt)?;
    println!("{receipt}");
    Ok(0)
}

pub(super) fn worker(root: &SkillRoot, directory: &Path, expected: &str) -> Result<u8> {
    let path = root.private_path(directory)?;
    let raw = read_regular_bytes(&path.join("manifest.json"), 1024 * 1024)?;
    ensure!(sha256(&raw) == expected, "background_manifest_changed");
    let manifest = strict_json(&raw)?;
    ensure!(
        supported(&manifest) && manifest["root"] == json!(root.path()),
        "background_identity_mismatch"
    );
    ensure!(
        manifest["executable_sha256"]
            == oe_core::fingerprint_file(&std::env::current_exe()?, 1024 * 1024 * 1024)?.sha256,
        "background_executable_changed"
    );
    let run = (|| -> Result<u8> {
        if manifest["schema"] == "oev2.route-background-job/1" {
            write_new(
                &path.join("started.json"),
                &json!({"status":"running","pid":std::process::id(),
                "manifest_sha256":expected}),
            )?;
            return crate::route_batch::worker(root, &path, &manifest);
        }
        let job: Job = serde_json::from_value(manifest["job"].clone())?;
        let prepared = prepare(root, &job.input)?;
        ensure!(
            prepared["input_sha256"] == manifest["input_sha256"]
                && prepared["deployment_sha256"] == manifest["deployment_sha256"],
            "background_inputs_changed"
        );
        write_new(
            &path.join("started.json"),
            &json!({"status":"running","pid":std::process::id(),
            "started_unix":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),"manifest_sha256":expected}),
        )?;
        super::execute(
            root,
            JudgmentCommand::Run {
                input: job.input,
                journal: job.journal,
                credential_file: job.credential_file,
                fixture_response: job.fixture_response,
                experiment: job.experiment,
                compatibility: job.compatibility,
                compatibility_root: job.compatibility_root,
                background: None,
                cancel_file: Some(path.join("cancel.json")),
            },
        )
    })();
    // Error prose can come from lower layers; completion uses a safe generic
    // code, while the CLI's own stderr retains its ordinary diagnostics.
    write_new(
        &path.join("completed.json"),
        &json!({"status":match &run {
        Ok(0)=>"completed",Ok(_)=>"incomplete",Err(_)=>"failed"},
        "exit_code":run.as_ref().copied().unwrap_or(2),"completed_unix":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
        "manifest_sha256":expected,"engineering_adoption":"not_performed"}),
    )?;
    run
}

pub(crate) fn status(root: &SkillRoot, directory: &Path) -> Result<u8> {
    let path = root.private_path(directory)?;
    let raw = read_regular_bytes(&path.join("manifest.json"), 1024 * 1024)?;
    let manifest = strict_json(&raw)?;
    let digest = sha256(&raw);
    ensure!(
        supported(&manifest) && manifest["root"] == json!(root.path()),
        "invalid_background_job"
    );
    let mut result =
        json!({"job":path,"status":"completion_unknown","engineering_adoption":"not_performed"});
    for name in ["spawn", "started", "completed"] {
        let p = path.join(format!("{name}.json"));
        if p.try_exists()? {
            result[name] = read_json(root, &p)?;
            ensure!(
                result[name]["manifest_sha256"] == digest,
                "background_observation_identity_mismatch"
            );
        }
    }
    if result["completed"].is_object() {
        result["status"] = result["completed"]["status"].clone();
    }
    // A stale PID is never evidence that this particular job is running. The
    // most recent event is an observation; no completion remains unknown.
    let p = path.join("events.jsonl");
    if p.try_exists()? {
        match read_regular_bytes(&p, 16 * 1024 * 1024) {
            Ok(bytes) => {
                result["latest_observation"] = bytes
                    .split_inclusive(|b| *b == b'\n')
                    .rev()
                    .filter(|line| line.last() == Some(&b'\n'))
                    .find_map(|line| strict_json(line).ok())
                    .unwrap_or(Value::Null);
                result["progress_read_status"] = json!("read_bounded_log");
            }
            Err(_) => {
                // The source-bound completion is independently readable. A
                // large or unavailable progress log must not hide that status.
                result["latest_observation"] = Value::Null;
                result["progress_read_status"] = json!("unavailable_or_exceeds_16_mib");
            }
        }
    }
    println!("{result}");
    Ok(0)
}

pub(crate) fn cancel(root: &SkillRoot, directory: &Path) -> Result<u8> {
    let path = root.private_path(directory)?;
    let manifest = read_json(root, &path.join("manifest.json"))?;
    ensure!(
        supported(&manifest) && manifest["root"] == json!(root.path()),
        "invalid_background_job"
    );
    let marker = path.join("cancel.json");
    if !marker.try_exists()? {
        write_new(
            &marker,
            &json!({"status":"cancel_requested","requested_unix":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            "scope":"stop_new_dispatch_and_drain_inflight"}),
        )?;
    }
    println!(
        "{}",
        json!({"status":"cancel_requested","job":path,
        "inflight":"may_complete_within_request_timeout","completion":"read_background_status"})
    );
    Ok(0)
}
