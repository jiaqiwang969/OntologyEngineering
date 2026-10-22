#![cfg(unix)]
use oe_core::process::{CommandSpec, run_command_with_stdin_file_deadline};
use oe_core::{FileIdentity, sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
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
        let directory = root.join(format!(
            "var/maintenance/rust-tests/stdin-file-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        Self(directory)
    }
    fn input(&self, bytes: &[u8]) -> (PathBuf, FileIdentity) {
        let path = self.0.join("original-input");
        fs::write(&path, bytes).unwrap();
        (
            path,
            FileIdentity {
                bytes: bytes.len() as u64,
                sha256: sha256(bytes),
            },
        )
    }
    fn command(&self, mode: &str, marker: &str) -> CommandSpec {
        CommandSpec {
            executable: std::env::current_exe().unwrap(),
            arguments: vec![
                "--exact".into(),
                "stdin_replay_fixture".into(),
                "--nocapture".into(),
            ],
            cwd: self.0.clone(),
            environment: BTreeMap::from([
                ("OE_STDIN_FILE_FIXTURE".into(), mode.into()),
                (
                    "OE_STDIN_FILE_MARKER".into(),
                    self.0.join(marker).into_os_string(),
                ),
                (
                    "OE_STDIN_FILE_SOURCE".into(),
                    self.0.join("original-input").into_os_string(),
                ),
            ]),
            remove_environment: vec![],
            timeout: Duration::from_secs(3),
            output_limit: 8192,
        }
    }
    fn receipt(&self, name: &str, returned: &serde_json::Value) {
        let directory = self.0.join(name);
        let file = returned["terminal_receipt"]
            .as_str()
            .unwrap_or("process.json");
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join(file)).unwrap()).unwrap();
        assert_eq!(stored, *returned);
        for stream in ["stdout", "stderr"] {
            let raw = fs::read(directory.join(stream)).unwrap();
            assert_eq!(returned[stream]["bytes"], raw.len());
            assert_eq!(returned[stream]["sha256"], sha256(&raw));
        }
        let stdin = fs::read(directory.join("stdin")).unwrap();
        assert_eq!(
            returned["stdin"]["snapshot"]["identity"]["bytes"],
            stdin.len()
        );
        assert_eq!(
            returned["stdin"]["snapshot"]["identity"]["sha256"],
            sha256(&stdin)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn stdin_replay_fixture() {
    let Ok(mode) = std::env::var("OE_STDIN_FILE_FIXTURE") else {
        return;
    };
    let mut counter = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(std::env::var_os("OE_STDIN_FILE_MARKER").unwrap())
        .unwrap();
    counter.write_all(b"one actual process\n").unwrap();
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes).unwrap();
    std::io::stdout().write_all(&bytes).unwrap();
    eprintln!(
        "stdin-bytes={} stdin-sha256={}",
        bytes.len(),
        sha256(&bytes)
    );
    match mode.as_str() {
        "ok" => {}
        "hold" => std::thread::sleep(Duration::from_secs(2)),
        "fail" => std::process::exit(13),
        "mutate_source" => fs::write(
            std::env::var_os("OE_STDIN_FILE_SOURCE").unwrap(),
            b"changed during actual child",
        )
        .unwrap(),
        _ => panic!("unknown original fixture mode"),
    }
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    std::process::exit(0);
}

#[test]
fn stdin_file_actual_binary_and_empty_replay_keep_original_argv_and_complete_streams() {
    for bytes in [vec![0, 255, b'\n', b'x', 128], vec![]] {
        let f = Fixture::new();
        let (input, expected) = f.input(&bytes);
        let s = f.command("ok", "started");
        let r = run_command_with_stdin_file_deadline(
            &s,
            &f.0.join("actual"),
            Instant::now() + Duration::from_secs(3),
            &input,
            &expected,
        )
        .unwrap();
        assert_eq!(r.status, "completed");
        assert_eq!(r.exit_code, Some(0));
        assert_eq!(
            r.report["schema"],
            "ontology-engineering.external-process/stdin-file/1"
        );
        assert_eq!(r.report["capture_complete"], true);
        assert_eq!(r.report["stdin"]["unchanged"], true);
        assert_eq!(r.report["stdin"]["target_consumption"], "not_inferred");
        assert_eq!(r.report["semantic_execution"], "not_inferred");
        let actual_arguments = r.report["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            actual_arguments,
            s.arguments
                .iter()
                .map(|v| v.to_str().unwrap())
                .collect::<Vec<_>>()
        );
        if !bytes.is_empty() {
            assert!(r.stdout.windows(bytes.len()).any(|v| v == bytes));
        }
        let error = String::from_utf8(r.stderr.clone()).unwrap();
        assert!(error.contains(&format!(
            "stdin-bytes={} stdin-sha256={}",
            bytes.len(),
            expected.sha256
        )));
        assert_eq!(
            fs::read(f.0.join("started")).unwrap(),
            b"one actual process\n"
        );
        assert_eq!(fs::read(&input).unwrap(), bytes);
        f.receipt("actual", &r.report);
    }
}

#[test]
fn stdin_file_mismatch_expiry_and_source_path_controls_start_no_process() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let (input, expected) = f.input(b"original input");
    let s = f.command("ok", "must-not-start");
    let mut wrong_sha = expected.clone();
    wrong_sha.sha256 = sha256(b"different input");
    let mut wrong_count = expected.clone();
    wrong_count.bytes += 1;
    for (name, binding) in [("bad-sha", wrong_sha), ("bad-count", wrong_count)] {
        assert!(
            run_command_with_stdin_file_deadline(
                &s,
                &f.0.join(name),
                Instant::now() + Duration::from_secs(3),
                &input,
                &binding
            )
            .is_err()
        );
        assert!(!f.0.join(name).exists());
    }
    let mut over = expected.clone();
    over.bytes = 8193;
    assert!(
        run_command_with_stdin_file_deadline(
            &s,
            &f.0.join("over"),
            Instant::now() + Duration::from_secs(3),
            &input,
            &over
        )
        .is_err()
    );
    assert!(!f.0.join("over").exists());
    let link = f.0.join("symlink-input");
    symlink(&input, &link).unwrap();
    for (name, path) in [
        ("link", link),
        ("directory", f.0.clone()),
        ("escape", f.0.join("../original-input")),
    ] {
        assert!(
            run_command_with_stdin_file_deadline(
                &s,
                &f.0.join(name),
                Instant::now() + Duration::from_secs(3),
                &path,
                &expected
            )
            .is_err()
        );
        assert!(!f.0.join(name).exists());
    }
    let e = run_command_with_stdin_file_deadline(
        &s,
        &f.0.join("expired"),
        Instant::now(),
        &input,
        &expected,
    )
    .unwrap_err();
    assert!(
        e.to_string()
            .contains("absolute_deadline_exceeded:before_resolve")
    );
    assert!(!f.0.join("expired").exists());
    assert!(!f.0.join("must-not-start").exists());
}

#[test]
fn stdin_file_actual_source_change_keeps_original_snapshot_and_rejects_success() {
    let f = Fixture::new();
    let (input, expected) = f.input(b"exact original payload");
    let r = run_command_with_stdin_file_deadline(
        &f.command("mutate_source", "one-child"),
        &f.0.join("changed"),
        Instant::now() + Duration::from_secs(3),
        &input,
        &expected,
    )
    .unwrap();
    assert_eq!(r.status, "stdin_source_changed");
    assert_eq!(r.exit_code, Some(0));
    assert_eq!(r.report["observed_status"], "completed");
    assert_eq!(r.report["stdin"]["unchanged"], false);
    assert_eq!(r.report["capture_complete"], true);
    assert!(r.stdout.windows(22).any(|v| v == b"exact original payload"));
    assert_eq!(
        fs::read(f.0.join("changed/stdin")).unwrap(),
        b"exact original payload"
    );
    assert_eq!(
        fs::read(f.0.join("one-child")).unwrap(),
        b"one actual process\n"
    );
    f.receipt("changed", &r.report);
}

#[test]
fn stdin_file_original_deadline_and_nonzero_completion_remain_distinct_failures() {
    let f = Fixture::new();
    let (input, expected) = f.input(b"exact payload");
    let deadline = Instant::now() + Duration::from_millis(250);
    let r = run_command_with_stdin_file_deadline(
        &f.command("hold", "first-child"),
        &f.0.join("timed"),
        deadline,
        &input,
        &expected,
    )
    .unwrap();
    assert_ne!(r.status, "completed");
    assert_eq!(r.report["borrowed_deadline"]["expired"], true);
    assert!(String::from_utf8_lossy(&r.stderr).contains(&expected.sha256));
    f.receipt("timed", &r.report);
    assert!(
        run_command_with_stdin_file_deadline(
            &f.command("ok", "second-child"),
            &f.0.join("not-renewed"),
            deadline,
            &input,
            &expected
        )
        .is_err()
    );
    assert!(!f.0.join("second-child").exists());
    assert!(!f.0.join("not-renewed").exists());
    let failed = run_command_with_stdin_file_deadline(
        &f.command("fail", "failed-child"),
        &f.0.join("nonzero"),
        Instant::now() + Duration::from_secs(3),
        &input,
        &expected,
    )
    .unwrap();
    assert_eq!(failed.status, "process_failed");
    assert_eq!(failed.exit_code, Some(13));
    assert_eq!(failed.report["capture_complete"], true);
    assert_eq!(failed.report["stdin"]["unchanged"], true);
    f.receipt("nonzero", &failed.report);
}

#[test]
fn explicit_input_capacity_replays_large_bytes_and_preserves_legacy_refusal() {
    use oe_core::process::run_command_with_bounded_stdin_file_deadline;
    let f = Fixture::new();
    let bytes = vec![b'x'; 5 * 1024 * 1024];
    let (input, expected) = f.input(&bytes);
    let mut spec = f.command("ok", "large-started");
    spec.output_limit = 8 * 1024 * 1024;
    let error = run_command_with_stdin_file_deadline(
        &spec,
        &f.0.join("legacy"),
        Instant::now() + Duration::from_secs(3),
        &input,
        &expected,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "external_stdin_original_limit");
    assert!(!f.0.join("legacy").exists());
    assert!(!f.0.join("large-started").exists());
    for (name, limit) in [
        ("short", bytes.len() - 1),
        ("zero", 0),
        ("unbounded", 256 * 1024 * 1024 + 1),
    ] {
        assert!(
            run_command_with_bounded_stdin_file_deadline(
                &spec,
                &f.0.join(name),
                Instant::now() + Duration::from_secs(3),
                &input,
                &expected,
                limit
            )
            .is_err()
        );
        assert!(!f.0.join(name).exists());
        assert!(!f.0.join("large-started").exists());
    }
    let r = run_command_with_bounded_stdin_file_deadline(
        &spec,
        &f.0.join("expanded"),
        Instant::now() + Duration::from_secs(3),
        &input,
        &expected,
        bytes.len(),
    )
    .unwrap();
    assert_eq!(r.status, "completed");
    assert_eq!(r.exit_code, Some(0));
    assert_eq!(
        r.report["schema"],
        "ontology-engineering.external-process/stdin-file/2"
    );
    assert_eq!(r.report["stdin"]["schema"], "oe-core.process-stdin-file/2");
    assert_eq!(r.report["stdin"]["original_byte_limit"], bytes.len());
    assert_eq!(r.report["stdin"]["unchanged"], true);
    assert_eq!(r.report["capture_complete"], true);
    assert!(r.stdout.windows(bytes.len()).any(|v| v == bytes));
    assert!(String::from_utf8_lossy(&r.stderr).contains(&expected.sha256));
    f.receipt("expanded", &r.report);
}
