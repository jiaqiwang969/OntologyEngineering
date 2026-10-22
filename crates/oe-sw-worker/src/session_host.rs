//! Narrow task-owned session launcher. Scheduled InteractiveToken invocation is
//! an OS deployment boundary; all request validation and worker execution is Rust.
//! The GUI host and CREATE_NO_WINDOW worker never create a console or switch focus.
use crate::{
    installation::{observe_file, pinned_file},
    owned::process_record,
};
use anyhow::{Context, Result, ensure};
use oe_windows_controller::windows::{ProcessObservation, ProcessWatch, StableRoot};
use serde_json::{Value, json};
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

pub fn run(args: Vec<OsString>) -> Result<()> {
    ensure!(
        args.len() == 5 && args[1] == "run",
        "usage: oe-sw-session-host run REQUEST NEW_NATIVE_OUTPUT NEW_LAUNCH_DIRECTORY"
    );
    let request_path = PathBuf::from(&args[2]);
    let native_output = PathBuf::from(&args[3]);
    let launch_path = PathBuf::from(&args[4]);
    ensure!(
        request_path.is_absolute() && native_output.is_absolute() && launch_path.is_absolute(),
        "solidworks_session_paths_must_be_absolute"
    );
    ensure!(
        native_output != launch_path,
        "solidworks_session_output_overlap"
    );
    let parent = StableRoot::open(
        launch_path.parent().context("solidworks_launch_parent")?,
        None,
    )?;
    let directory =
        parent.create_directory_new(launch_path.file_name().context("solidworks_launch_leaf")?)?;
    let mut raw = Vec::new();
    std::fs::File::open(&request_path)?
        .take(crate::MAX_RECORD_BYTES as u64 + 1)
        .read_to_end(&mut raw)?;
    directory.create_file_new(
        OsStr::new("request.json"),
        &raw,
        crate::MAX_RECORD_BYTES as u64,
    )?;
    let mut result = json!({"schema":"oev2.solidworks-session-launch/1","status":"failed_before_worker_start","request_sha256":crate::sha256(&raw),"worker":null,"worker_exit_code":null,"native_output":native_output,"retry":"not_attempted","termination":"not_attempted","focus_change":"not_requested"});
    let attempt = (|| -> Result<()> {
        let parsed = oe_wire::json::strict_json(&raw)?;
        let request = serde_json::from_value(parsed)?;
        crate::validate_request(&request)?;
        let request_pin = observe_file(&request_path, crate::MAX_RECORD_BYTES as u64)?;
        ensure!(
            request_pin.sha256 == crate::sha256(&raw),
            "solidworks_launch_request_changed"
        );
        let mut request_owner = pinned_file(&request_pin)?;
        let host = process_record(&ProcessWatch::current()?)?;
        ensure!(
            host.session_id != 0,
            "solidworks_session_host_requires_interactive_token"
        );
        let executable = std::env::current_exe()?
            .parent()
            .context("solidworks_session_install_directory")?
            .join("oe-sw-worker.exe");
        let executable_pin = observe_file(&executable, 64 * 1024 * 1024)?;
        let mut executable_owner = pinned_file(&executable_pin)?;
        let arguments = vec![
            OsString::from("run"),
            request_path.as_os_str().to_owned(),
            native_output.as_os_str().to_owned(),
        ];
        let intent = json!({"schema":"oev2.solidworks-session-launch/1","status":"before_create_process","host":host,"request":request_pin,"worker_executable":executable_pin,"argv":arguments.iter().map(|s|s.to_string_lossy().into_owned()).collect::<Vec<_>>(),"creation_flags":"CREATE_NO_WINDOW","native_output":native_output,"focus_change":"not_requested","outcome":"unknown_until_return"});
        directory.create_file_new(
            OsStr::new("launch-intent.json"),
            &serde_json::to_vec_pretty(&intent)?,
            crate::MAX_RECORD_BYTES as u64,
        )?;
        let stdout = directory.create_log_new(OsStr::new("worker.raw.stdout"))?;
        let stderr = directory.create_log_new(OsStr::new("worker.raw.stderr"))?;
        let mut child = Command::new(&executable)
            .args(&arguments)
            .current_dir(directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()?;
        let watch = ProcessWatch::from_child(&child)?;
        let worker = process_record(&watch)?;
        ensure!(
            worker.session_id == host.session_id
                && worker.image.sha256 == executable_pin.sha256
                && worker.image.bytes == executable_pin.bytes,
            "solidworks_session_worker_identity_mismatch"
        );
        result["worker"] = json!(worker);
        result["host"] = json!(host);
        result["worker_executable"] = json!(executable_pin);
        result["argv"] = intent["argv"].clone();
        result["status"] = json!("worker_running_outcome_unknown");
        directory.create_file_new(
            OsStr::new("worker-started.json"),
            &serde_json::to_vec_pretty(&result)?,
            crate::MAX_RECORD_BYTES as u64,
        )?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(600) {
            if let ProcessObservation::Exited { exit_code } = watch.poll()? {
                let status = child.wait()?;
                ensure!(
                    status.code().map(|v| v as u32) == Some(exit_code),
                    "solidworks_session_exit_observation_mismatch"
                );
                executable_owner.verify()?;
                request_owner.verify()?;
                ensure!(
                    crate::sha256(&executable_owner.read_all(64 * 1024 * 1024)?)
                        == executable_pin.sha256,
                    "solidworks_session_worker_bytes_changed"
                );
                ensure!(
                    crate::sha256(&request_owner.read_all(crate::MAX_RECORD_BYTES as u64)?)
                        == request_pin.sha256,
                    "solidworks_session_request_bytes_changed"
                );
                result["worker_exit_code"] = json!(exit_code);
                result["status"] = json!(if exit_code == 0 {
                    "completed"
                } else {
                    "worker_failed"
                });
                result["stdout"] = json!(observe_file(
                    &directory.path().join("worker.raw.stdout"),
                    crate::MAX_RECORD_BYTES as u64
                )?);
                result["stderr"] = json!(observe_file(
                    &directory.path().join("worker.raw.stderr"),
                    crate::MAX_RECORD_BYTES as u64
                )?);
                ensure!(
                    exit_code == 0,
                    "solidworks_session_worker_failed:{exit_code}"
                );
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        anyhow::bail!("solidworks_session_worker_outcome_unknown_after_deadline")
    })();
    if let Err(error) = &attempt {
        result["error"] = Value::String(format!("{error:#}"));
    }
    directory.create_file_new(
        OsStr::new("launch-result.json"),
        &serde_json::to_vec_pretty(&result)?,
        crate::MAX_RECORD_BYTES as u64,
    )?;
    attempt
}
