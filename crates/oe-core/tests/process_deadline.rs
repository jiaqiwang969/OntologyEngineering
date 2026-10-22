#![cfg(unix)]
use oe_core::process::{CommandSpec, run_command_with_deadline};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = root.join(format!(
            "var/maintenance/rust-tests/borrowed-deadline-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn spec(&self, mode: &str, marker: &str) -> CommandSpec {
        CommandSpec {
            executable: std::env::current_exe().unwrap(),
            arguments: vec![
                "--exact".into(),
                "deadline_process_fixture".into(),
                "--nocapture".into(),
            ],
            cwd: self.0.clone(),
            environment: BTreeMap::from([
                ("OE_DEADLINE_FIXTURE".into(), mode.into()),
                (
                    "OE_DEADLINE_MARKER".into(),
                    self.0.join(marker).into_os_string(),
                ),
            ]),
            remove_environment: vec![],
            timeout: Duration::from_secs(3),
            output_limit: 4096,
        }
    }
    fn receipt(&self, name: &str, returned: &Value) {
        let file = returned["terminal_receipt"]
            .as_str()
            .unwrap_or("process.json");
        let persisted: Value =
            serde_json::from_slice(&fs::read(self.0.join(name).join(file)).unwrap()).unwrap();
        assert_eq!(persisted, *returned);
        for stream in ["stdout", "stderr"] {
            let raw = fs::read(self.0.join(name).join(stream)).unwrap();
            assert_eq!(returned[stream]["sha256"], oe_core::sha256(&raw));
            assert_eq!(returned[stream]["bytes"], raw.len());
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn deadline_process_fixture() {
    let Ok(mode) = std::env::var("OE_DEADLINE_FIXTURE") else {
        return;
    };
    fs::write(
        std::env::var_os("OE_DEADLINE_MARKER").unwrap(),
        b"actual executable started",
    )
    .unwrap();
    println!("deadline-payload");
    eprintln!("deadline-diagnostic");
    match mode.as_str() {
        "ok" => {}
        "hold" => std::thread::sleep(Duration::from_secs(2)),
        "fail" => std::process::exit(9),
        _ => panic!("unknown fixed fixture mode"),
    }
}

#[test]
fn expired_absolute_deadline_starts_no_executable_and_creates_no_evidence() {
    let f = Fixture::new();
    let s = f.spec("ok", "no-spawn-marker");
    let expired = Instant::now() - Duration::from_millis(1);
    let error = run_command_with_deadline(&s, &f.0.join("expired"), expired).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("absolute_deadline_exceeded:before_resolve")
    );
    assert!(!f.0.join("no-spawn-marker").exists());
    assert!(!f.0.join("expired").exists());
}

#[test]
fn expired_gate_precedes_executable_resolution() {
    let f = Fixture::new();
    let mut s = f.spec("ok", "absent-marker");
    s.executable = f.0.join("nonexistent-executable");
    let error = run_command_with_deadline(
        &s,
        &f.0.join("expired-unavailable"),
        Instant::now() - Duration::from_millis(1),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("absolute_deadline_exceeded:before_resolve")
    );
    assert!(!f.0.join("expired-unavailable").exists());
    assert!(!f.0.join("absent-marker").exists());
}

#[test]
fn timely_process_retains_full_streams_and_records_borrowed_scope() {
    let f = Fixture::new();
    let s = f.spec("ok", "completed-marker");
    let r = run_command_with_deadline(
        &s,
        &f.0.join("completed"),
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(r.status, "completed");
    assert_eq!(r.exit_code, Some(0));
    assert_eq!(r.report["capture_complete"], true);
    assert_eq!(r.report["borrowed_deadline"]["expired"], false);
    assert_eq!(
        r.report["borrowed_deadline"]["hard_function_return_deadline"],
        "unknown"
    );
    assert!(String::from_utf8_lossy(&r.stdout).contains("deadline-payload"));
    assert!(String::from_utf8_lossy(&r.stderr).contains("deadline-diagnostic"));
    assert!(f.0.join("completed-marker").exists());
    f.receipt("completed", &r.report);
}

#[test]
fn running_child_consumes_deadline_without_success_or_discarding_partial_evidence() {
    let f = Fixture::new();
    let s = f.spec("hold", "started-before-expiry");
    let r = run_command_with_deadline(
        &s,
        &f.0.join("timed"),
        Instant::now() + Duration::from_millis(250),
    )
    .unwrap();
    assert_ne!(r.status, "completed");
    assert_eq!(r.report["borrowed_deadline"]["expired"], true);
    assert!(
        r.report["effective_process_timeout_seconds"]
            .as_f64()
            .unwrap()
            <= 0.25
    );
    assert!(f.0.join("started-before-expiry").exists());
    assert!(String::from_utf8_lossy(&r.stdout).contains("deadline-payload"));
    f.receipt("timed", &r.report);
}

#[test]
fn same_caller_instant_is_not_renewed_for_a_second_invocation() {
    let f = Fixture::new();
    let deadline = Instant::now() + Duration::from_millis(250);
    let first = run_command_with_deadline(
        &f.spec("hold", "first-marker"),
        &f.0.join("first"),
        deadline,
    )
    .unwrap();
    assert_ne!(first.status, "completed");
    let second = run_command_with_deadline(
        &f.spec("ok", "second-marker"),
        &f.0.join("second"),
        deadline,
    )
    .unwrap_err();
    assert!(
        second
            .to_string()
            .contains("absolute_deadline_exceeded:before_resolve")
    );
    assert!(!f.0.join("second-marker").exists());
    assert!(!f.0.join("second").exists());
}

#[test]
fn earlier_original_perprocess_timeout_and_nonzero_failure_remain_failures() {
    let f = Fixture::new();
    let mut s = f.spec("hold", "short-marker");
    s.timeout = Duration::from_millis(100);
    let r = run_command_with_deadline(
        &s,
        &f.0.join("short"),
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(r.status, "timed_out");
    assert_eq!(r.report["borrowed_deadline"]["expired"], false);
    assert!(
        r.report["effective_process_timeout_seconds"]
            .as_f64()
            .unwrap()
            <= 0.1
    );
    f.receipt("short", &r.report);
    let failed = run_command_with_deadline(
        &f.spec("fail", "failed-marker"),
        &f.0.join("failed"),
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(failed.status, "process_failed");
    assert_eq!(failed.exit_code, Some(9));
    assert_eq!(failed.report["capture_complete"], true);
    f.receipt("failed", &failed.report);
    assert_eq!(failed.report["semantic_execution"], json!("not_inferred"));
}
