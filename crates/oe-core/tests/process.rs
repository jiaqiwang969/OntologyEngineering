#![cfg(unix)]
use oe_core::process::{CommandSpec, run_command, run_command_memory};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let installed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = installed.join(format!(
            "var/maintenance/rust-tests/process-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn spec(&self, mode: &str) -> CommandSpec {
        CommandSpec {
            executable: std::env::current_exe().unwrap(),
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
            print!("{}", "x".repeat(20000));
        }
        "hold" => {
            println!("hold-started");
            std::thread::sleep(Duration::from_secs(30));
        }
        "inherited" => {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_fixture", "--nocapture"])
                .env("OE_PROCESS_FIXTURE", "hold")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            println!("descendant:{}", child.id());
            let _ = child.try_wait();
        }
        "retarget" => {
            let path = PathBuf::from(std::env::var_os("OE_PROCESS_LINK").unwrap());
            fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink("/bin/echo", path).unwrap();
        }
        _ => panic!("unknown test mode"),
    }
}
#[test]
fn preserves_exit_logs_and_exclusive_output() {
    let f = Fixture::new();
    let s = f.spec("ok");
    let result = run_command(&s, &f.0.join("ok")).unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(result.exit_code, Some(0));
    assert!(String::from_utf8_lossy(&result.stdout).contains("payload:bounded"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("diagnostic"));
    assert_eq!(result.report["capture_complete"], true);
    assert_eq!(result.report["environment_values"], "not_recorded");
    assert!(run_command(&s, &f.0.join("ok")).is_err());
    let result = run_command(&f.spec("fail"), &f.0.join("failure")).unwrap();
    assert_eq!(result.status, "process_failed");
    assert_eq!(result.exit_code, Some(7));
    let persisted: Value =
        serde_json::from_slice(&fs::read(f.0.join("failure/process.json")).unwrap()).unwrap();
    assert_eq!(persisted, result.report);
}
#[test]
fn timeout_and_inherited_pipes_are_bounded_and_keep_partial_streams() {
    let f = Fixture::new();
    for mode in ["hold", "inherited"] {
        let mut s = f.spec(mode);
        s.timeout = Duration::from_millis(500);
        let start = Instant::now();
        let result = run_command(&s, &f.0.join(mode)).unwrap();
        assert_eq!(result.status, "timed_out");
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(String::from_utf8_lossy(&result.stdout).contains("hold-started"));
        assert_eq!(result.report["capture_complete"], false);
    }
}
#[test]
fn output_limit_and_executable_retargeting_cannot_report_success() {
    let f = Fixture::new();
    let mut s = f.spec("large");
    s.output_limit = 128;
    let result = run_command(&s, &f.0.join("large")).unwrap();
    assert_eq!(result.status, "output_limit");
    assert_eq!(result.stdout.len(), 128);
    assert_eq!(result.report["stdout"]["truncated"], true);
    let link = f.0.join("selected");
    std::os::unix::fs::symlink(std::env::current_exe().unwrap(), &link).unwrap();
    let mut s = f.spec("retarget");
    s.executable = link.clone();
    s.environment
        .insert("OE_PROCESS_LINK".into(), link.into_os_string());
    let result = run_command(&s, &f.0.join("retarget")).unwrap();
    assert_eq!(result.status, "executable_changed");
    assert_eq!(result.report["observed_status"], "completed");
    assert_eq!(result.report["executable"]["unchanged"], false);
}
#[test]
fn spawn_failure_is_recorded_without_environment_values() {
    let f = Fixture::new();
    let mut s = f.spec("ok");
    s.environment.insert(
        OsString::from("INVALID\0KEY"),
        "never-print-this-value".into(),
    );
    let result = run_command(&s, &f.0.join("spawn")).unwrap();
    assert_eq!(result.status, "spawn_failed");
    assert_eq!(result.report["streams_collected"], 0);
    assert!(!result.report.to_string().contains("never-print-this-value"));
    assert_eq!(result.report["semantic_execution"], json!("not_inferred"));
}

#[test]
fn memory_capture_retains_receipts_without_raw_stream_files() {
    let f = Fixture::new();
    for mode in ["ok", "fail", "large", "hold"] {
        let mut spec = f.spec(mode);
        if mode == "large" {
            spec.output_limit = 128;
        }
        if mode == "hold" {
            spec.timeout = Duration::from_millis(500);
        }
        let directory = f.0.join(mode);
        let result = run_command_memory(&spec, &directory).unwrap();
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
        let persisted = fs::read(directory.join("process.json")).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&persisted).unwrap(),
            result.report
        );
        let text = String::from_utf8(persisted).unwrap();
        for payload in [
            "payload:bounded",
            "diagnostic",
            "intentional failure",
            "hold-started",
        ] {
            assert!(!text.contains(payload));
        }
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        if mode == "ok" {
            assert!(String::from_utf8_lossy(&result.stdout).contains("payload:bounded"));
        }
        assert!(run_command_memory(&spec, &directory).is_err());
    }
}
