#![cfg(windows)]

use oe_core::process::{CommandSpec, run_command, run_command_memory};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, STILL_ACTIVE,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn process_still_active(pid: u32) -> bool {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle == 0 {
        assert_eq!(unsafe { GetLastError() }, ERROR_INVALID_PARAMETER);
        return false;
    }
    let mut exit_code = 0;
    let queried = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe { CloseHandle(handle) };
    assert_ne!(queried, 0, "descendant exit status unavailable");
    exit_code == STILL_ACTIVE as u32
}

#[test]
fn windows_drive_prefix_is_not_queried_as_a_path() {
    oe_core::no_symlinks(&std::env::current_dir().expect("current directory"))
        .expect("Windows absolute path");
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root");
        let path = root.join(format!(
            "var/maintenance/rust-tests/process-windows-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("fixture directory");
        Self(path)
    }

    fn spec(&self, mode: &str) -> CommandSpec {
        CommandSpec {
            executable: std::env::current_exe().expect("test executable"),
            arguments: vec![
                "--exact".into(),
                "process_fixture".into(),
                "--nocapture".into(),
            ],
            cwd: self.0.clone(),
            environment: BTreeMap::from([("OE_PROCESS_FIXTURE".into(), mode.into())]),
            remove_environment: vec![],
            timeout: Duration::from_secs(10),
            output_limit: 4096,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn process_fixture() {
    let Ok(mode) = std::env::var("OE_PROCESS_FIXTURE") else {
        return;
    };
    match mode.as_str() {
        "ok" => {
            println!("payload:bounded");
            eprintln!("diagnostic");
        }
        "fail" => {
            eprintln!("intentional failure");
            std::process::exit(7);
        }
        "large" => {
            print!("{}", "x".repeat(20_000));
        }
        "hold" => {
            println!("hold-started");
            std::thread::sleep(Duration::from_secs(30));
        }
        "inherited" => {
            let mut child = Command::new(std::env::current_exe().expect("test executable"))
                .args(["--exact", "process_fixture", "--nocapture"])
                .env("OE_PROCESS_FIXTURE", "hold")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("descendant");
            println!("descendant:{}", child.id());
            let _ = child.try_wait();
        }
        _ => panic!("unknown test mode"),
    }
}

#[test]
fn windows_job_preserves_receipts_and_failure_state() {
    let fixture = Fixture::new();
    let result = run_command(&fixture.spec("ok"), &fixture.0.join("ok")).expect("run");
    assert_eq!(result.status, "completed");
    assert_eq!(result.exit_code, Some(0));
    assert!(String::from_utf8_lossy(&result.stdout).contains("payload:bounded"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("diagnostic"));
    assert_eq!(result.report["capture_complete"], true);
    assert_eq!(
        result.report["process_tree"],
        "windows_job_object_create_suspended_assignment"
    );

    let result = run_command(&fixture.spec("fail"), &fixture.0.join("failure")).expect("run");
    assert_eq!(result.status, "process_failed");
    assert_eq!(result.exit_code, Some(7));
    let persisted: Value =
        serde_json::from_slice(&fs::read(fixture.0.join("failure/process.json")).expect("receipt"))
            .expect("receipt json");
    assert_eq!(persisted, result.report);
}

#[test]
fn windows_job_terminates_descendants_on_timeout() {
    let fixture = Fixture::new();
    for mode in ["hold", "inherited"] {
        let mut spec = fixture.spec(mode);
        spec.timeout = Duration::from_millis(500);
        let started = Instant::now();
        let result = run_command(&spec, &fixture.0.join(mode)).expect("run");
        assert_eq!(result.status, "timed_out");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(String::from_utf8_lossy(&result.stdout).contains("hold-started"));
        assert_eq!(result.report["capture_complete"], false);
        assert_eq!(
            result.report["process_tree"],
            "windows_job_object_create_suspended_assignment"
        );
        if mode == "inherited" {
            let output = String::from_utf8_lossy(&result.stdout);
            let descendant = output
                .lines()
                .find_map(|line| line.strip_prefix("descendant:"))
                .expect("descendant PID recorded")
                .parse::<u32>()
                .expect("descendant PID is numeric");
            assert!(
                !process_still_active(descendant),
                "descendant remained alive"
            );
        }
    }
}

#[test]
fn windows_job_rejects_output_over_limit() {
    let fixture = Fixture::new();
    let mut spec = fixture.spec("large");
    spec.output_limit = 128;
    let result = run_command(&spec, &fixture.0.join("large")).expect("run");
    assert_eq!(result.status, "output_limit");
    assert_eq!(result.stdout.len(), 128);
    assert_eq!(result.report["stdout"]["truncated"], true);
    assert_eq!(result.report["capture_complete"], false);
}

#[test]
fn windows_memory_capture_keeps_streams_out_of_receipt() {
    let fixture = Fixture::new();
    for mode in ["ok", "fail", "large", "hold"] {
        let mut spec = fixture.spec(mode);
        if mode == "large" {
            spec.output_limit = 128;
        }
        if mode == "hold" {
            spec.timeout = Duration::from_millis(500);
        }
        let directory = fixture.0.join(format!("memory-{mode}"));
        let result = run_command_memory(&spec, &directory).expect("run");
        assert_eq!(
            result.status,
            match mode {
                "ok" => "completed",
                "fail" => "process_failed",
                "large" => "output_limit",
                _ => "timed_out",
            }
        );
        assert_eq!(result.report["stream_retention"], "memory_only");
        for (name, bytes) in [("stdout", &result.stdout), ("stderr", &result.stderr)] {
            assert!(!directory.join(name).exists());
            assert!(result.report[name]["path"].is_null());
            assert_eq!(result.report[name]["bytes"], bytes.len());
            assert_eq!(result.report[name]["sha256"], oe_core::sha256(bytes));
            assert!(bytes.len() <= spec.output_limit);
        }
        let persisted = fs::read(directory.join("process.json")).expect("receipt");
        assert_eq!(
            serde_json::from_slice::<Value>(&persisted).expect("receipt json"),
            result.report
        );
        let text = String::from_utf8(persisted).expect("utf8 receipt");
        for payload in [
            "payload:bounded",
            "diagnostic",
            "intentional failure",
            "hold-started",
        ] {
            assert!(!text.contains(payload));
        }
        assert_eq!(fs::read_dir(&directory).expect("directory").count(), 1);
        if mode == "ok" {
            assert!(String::from_utf8_lossy(&result.stdout).contains("payload:bounded"));
        }
    }
}
