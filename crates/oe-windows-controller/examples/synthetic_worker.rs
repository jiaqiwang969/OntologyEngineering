//! Explicit acceptance fixture. No NX SDK import/load, geometry or engineering pass.
#[cfg(windows)]
fn execute() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use oe_nx_protocol as p;
    use oe_windows_controller::{adapter::*, windows::*};
    use std::{
        ffi::OsStr,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };
    fn pending(dir: &PinnedDirectory<'_>) -> std::io::Result<bool> {
        match dir.read_record(OsStr::new(PENDING_NAME)) {
            Ok(v) => Ok(v.is_some()),
            Err(e) if e.raw_os_error() == Some(32) => Ok(true),
            Err(e) => Err(e),
        }
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4 && args[0] == "--job-dir" && args[2] == "--nx-root",
        "fixture_arguments"
    );
    let path = PathBuf::from(&args[1]);
    let root = StableRoot::open(path.parent().context("job parent")?, None)?;
    let dir = root.directory(path.file_name().context("job name")?)?;
    let job: p::JobManifest = p::decode(
        &dir.read_record(OsStr::new("job.json"))?
            .context("job missing")?,
    )?;
    let intent: p::DispatchIntent = p::decode(
        &dir.read_record(OsStr::new("dispatch.json"))?
            .context("dispatch missing")?,
    )?;
    intent.validate(&job)?;
    ensure!(
        job.action == "synthetic.echo" && job.params_schema == "owned.synthetic-params/v1",
        "synthetic capability only"
    );
    let mut image = pin_relative_file(&dir, &job.worker.executable.path, p::MAX_PAYLOAD_BYTES)?;
    let own = current_process_identity(file_identity(&mut image)?)?;
    let start = Instant::now();
    loop {
        ensure!(
            start.elapsed() < Duration::from_secs(10),
            "fixture handshake timeout"
        );
        if pending(&dir)? {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let state: p::StateRecord = p::decode(
            &dir.read_record(OsStr::new(STATE_NAME))?
                .context("state missing")?,
        )?;
        state.validate(&job)?;
        match state.state {
            p::State::Starting => thread::sleep(Duration::from_millis(10)),
            p::State::Running => {
                ensure!(
                    state.process.as_ref() == Some(&own)
                        && state.dispatch.as_ref() == Some(&intent),
                    "fixture process handshake differs"
                );
                if pending(&dir)? {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                break;
            }
            _ => anyhow::bail!("fixture refuses nonrunning state"),
        }
    }
    let raw = dir
        .read_record(OsStr::new("params.json"))?
        .context("params missing")?;
    let params = job.validate_parameters(&raw)?;
    let delay = params
        .get("sleep_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000);
    ensure!(delay <= 30000, "fixture sleep bound");
    thread::sleep(Duration::from_millis(delay));
    let out = dir.directory(OsStr::new("out"))?;
    let bytes = b"owned synthetic controller evidence; no NX";
    out.create_file_new(OsStr::new("result.bin"), bytes, 1024)?;
    if params.get("omit_result").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(());
    }
    let result = p::NativeResult {
        schema: p::RESULT_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: job.root.clone(),
        manifest_digest: job.digest()?,
        dispatch_id: intent.dispatch_id.clone(),
        worker_manifest_digest: job.worker.digest()?,
        worker: own.image.clone(),
        input_digest: job.input_digest()?,
        process: own,
        runtime: job.worker.required_runtime.clone(),
        action: job.action.clone(),
        outcome: p::ResultOutcome::Succeeded,
        readback_schema: "owned.synthetic-readback/v1".into(),
        readback: serde_json::json!({"synthetic":true,"nx_started":false}),
        artifacts: vec![p::FileEntry::from_bytes("out/result.bin", bytes)?],
        errors: vec![],
        unknowns: vec![],
    };
    dir.create_file_new(
        OsStr::new("native-result.json"),
        &serde_json::to_vec(&result)?,
        p::MAX_RECORD_BYTES as u64,
    )?;
    Ok(())
}
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    execute()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows synthetic fixture only");
    std::process::exit(2);
}
