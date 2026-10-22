//! Frozen Windows regression for the installed source-session setup failure.
//! Actual ProcessTree children exercise transport; this does not certify semantics.
#![cfg(windows)]
use oe_core::process::{
    CommandSpec, run_command_with_bounded_stdin_file_deadline, run_command_with_stdin_file_deadline,
};
use oe_core::{FileIdentity, sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = root.join(format!(
            "var/maintenance/rust-tests/windows-stdin-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn input(&self, bytes: &[u8]) -> (PathBuf, FileIdentity) {
        let path = self.0.join("input.bin");
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
                "windows_stdin_fixture".into(),
                "--nocapture".into(),
            ],
            cwd: self.0.clone(),
            environment: BTreeMap::from([
                ("OE_WINDOWS_STDIN_MODE".into(), mode.into()),
                (
                    "OE_WINDOWS_STDIN_MARKER".into(),
                    self.0.join(marker).into_os_string(),
                ),
                (
                    "OE_WINDOWS_STDIN_SOURCE".into(),
                    self.0.join("input.bin").into_os_string(),
                ),
            ]),
            remove_environment: vec![],
            timeout: Duration::from_secs(10),
            output_limit: 4096,
        }
    }
    fn receipt(&self, name: &str, report: &serde_json::Value, input: &[u8]) {
        let dir = self.0.join(name);
        let stored: serde_json::Value = serde_json::from_slice(
            &fs::read(
                dir.join(
                    report["terminal_receipt"]
                        .as_str()
                        .unwrap_or("process.json"),
                ),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(&stored, report);
        assert_eq!(
            report["process_tree"],
            "windows_job_object_create_suspended_assignment"
        );
        for stream in ["stdout", "stderr"] {
            let bytes = fs::read(dir.join(stream)).unwrap();
            assert_eq!(report[stream]["bytes"], bytes.len());
            assert_eq!(report[stream]["sha256"], sha256(&bytes));
        }
        assert_eq!(fs::read(dir.join("stdin")).unwrap(), input);
        assert_eq!(
            report["stdin"]["snapshot"]["identity"]["bytes"],
            input.len()
        );
        assert_eq!(
            report["stdin"]["snapshot"]["identity"]["sha256"],
            sha256(input)
        );
        assert_eq!(report["stdin"]["target_consumption"], "not_inferred");
        assert_eq!(report["stdin"]["semantic_support"], "not_inferred");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fn remove_owned(path: &Path) {
            let Ok(meta) = fs::symlink_metadata(path) else {
                return;
            };
            if meta.is_dir() && !meta.file_type().is_symlink() {
                for entry in fs::read_dir(path).unwrap().flatten() {
                    remove_owned(&entry.path());
                }
                let _ = fs::remove_dir(path);
            } else if !meta.file_type().is_symlink() {
                let mut permissions = meta.permissions();
                permissions.set_readonly(false);
                let _ = fs::set_permissions(path, permissions);
                let _ = fs::remove_file(path);
            }
        }
        remove_owned(&self.0);
    }
}

fn ordinary(path: &Path) -> PathBuf {
    PathBuf::from(path.to_str().unwrap().strip_prefix(r"\\?\").unwrap())
}

#[test]
fn windows_stdin_fixture() {
    let Ok(mode) = std::env::var("OE_WINDOWS_STDIN_MODE") else {
        return;
    };
    let mut marker = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var_os("OE_WINDOWS_STDIN_MARKER").unwrap())
        .unwrap();
    marker.write_all(b"one actual child\n").unwrap();
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    eprintln!("bytes={} sha256={}", input.len(), sha256(&input));
    match mode.as_str() {
        "ok" => std::io::stdout().write_all(&input).unwrap(),
        "digest" => {}
        "mutate" => fs::write(
            std::env::var_os("OE_WINDOWS_STDIN_SOURCE").unwrap(),
            b"changed by actual child",
        )
        .unwrap(),
        "fail" => std::process::exit(13),
        "hold" => std::thread::sleep(Duration::from_secs(10)),
        _ => panic!("unknown fixture"),
    }
    std::io::stdout().flush().unwrap();
}

#[test]
fn windows_stdin_replays_ordinary_and_verbatim_binary_and_empty() {
    for bytes in [b"".as_slice(), b"\0\xff\xfe\r\nexact\0payload".as_slice()] {
        let f = Fixture::new();
        let (input, expected) = f.input(bytes);
        for (name, path) in [("ordinary", ordinary(&input)), ("verbatim", input)] {
            let marker = format!("{name}-marker");
            let result = run_command_with_stdin_file_deadline(
                &f.command("ok", &marker),
                &f.0.join(name),
                Instant::now() + Duration::from_secs(10),
                &path,
                &expected,
            )
            .expect("valid local disk stdin must start actual child");
            assert_eq!(result.status, "completed");
            assert_eq!(result.exit_code, Some(0));
            assert_eq!(result.report["stdin"]["unchanged"], true);
            assert_eq!(result.report["capture_complete"], true);
            assert!(String::from_utf8_lossy(&result.stderr).contains(&format!(
                "bytes={} sha256={}",
                bytes.len(),
                expected.sha256
            )));
            if !bytes.is_empty() {
                assert!(result.stdout.windows(bytes.len()).any(|v| v == bytes));
            }
            assert_eq!(fs::read(f.0.join(marker)).unwrap(), b"one actual child\n");
            f.receipt(name, &result.report, bytes);
        }
    }
}

#[test]
fn windows_stdin_bound_v2_keeps_v1_limit_and_original_stream_ceiling() {
    let f = Fixture::new();
    let bytes = vec![0xa5; 8192];
    let (input, expected) = f.input(&bytes);
    let spec = f.command("digest", "marker");
    let error = run_command_with_stdin_file_deadline(
        &spec,
        &f.0.join("v1"),
        Instant::now() + Duration::from_secs(10),
        &input,
        &expected,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("external_stdin_original_limit"));
    assert!(!f.0.join("v1").exists());
    assert!(!f.0.join("marker").exists());
    for (name, limit) in [
        ("zero", 0),
        ("too-small", 8191),
        ("unbounded", 256 * 1024 * 1024 + 1),
    ] {
        assert!(
            run_command_with_bounded_stdin_file_deadline(
                &spec,
                &f.0.join(name),
                Instant::now() + Duration::from_secs(10),
                &input,
                &expected,
                limit
            )
            .is_err()
        );
        assert!(!f.0.join(name).exists());
        assert!(!f.0.join("marker").exists());
    }
    let result = run_command_with_bounded_stdin_file_deadline(
        &spec,
        &f.0.join("v2"),
        Instant::now() + Duration::from_secs(10),
        &input,
        &expected,
        bytes.len(),
    )
    .unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(
        result.report["stdin"]["schema"],
        "oe-core.process-stdin-file/2"
    );
    assert_eq!(result.report["stdin"]["original_byte_limit"], bytes.len());
    assert_eq!(result.report["capture_complete"], true);
    assert!(result.stdout.len() <= spec.output_limit && result.stderr.len() <= spec.output_limit);
    assert!(String::from_utf8_lossy(&result.stderr).contains(&expected.sha256));
    f.receipt("v2", &result.report, &bytes);
}

#[test]
fn windows_stdin_invalid_paths_binding_and_expiry_start_no_child() {
    let f = Fixture::new();
    let (input, expected) = f.input(b"original");
    let plain = ordinary(&input);
    let parent = plain.parent().unwrap();
    let mut ads = plain.as_os_str().to_os_string();
    ads.push(":hidden");
    let mut traversal = parent.as_os_str().to_os_string();
    traversal.push(r"\..\input.bin");
    let invalid = [
        PathBuf::from("input.bin"),
        PathBuf::from(r"C:input.bin"),
        PathBuf::from(r"\input.bin"),
        PathBuf::from(r"\\server\share\input.bin"),
        PathBuf::from(r"\\?\UNC\server\share\input.bin"),
        PathBuf::from(r"\\.\C:\input.bin"),
        PathBuf::from(ads),
        PathBuf::from(traversal),
        parent.join("NUL"),
        parent.join("input.bin."),
        parent.join("input.bin "),
    ];
    let spec = f.command("ok", "must-not-start");
    for (i, path) in invalid.iter().enumerate() {
        let evidence = f.0.join(format!("path-{i}"));
        let error = run_command_with_stdin_file_deadline(
            &spec,
            &evidence,
            Instant::now() + Duration::from_secs(10),
            path,
            &expected,
        )
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("external_stdin_path_not_canonical"),
            "{path:?}: {error:#}"
        );
        assert!(!evidence.exists());
    }
    for (name, identity) in [
        (
            "wrong-digest",
            FileIdentity {
                sha256: "0".repeat(64),
                ..expected.clone()
            },
        ),
        (
            "wrong-count",
            FileIdentity {
                bytes: expected.bytes + 1,
                ..expected.clone()
            },
        ),
    ] {
        let evidence = f.0.join(name);
        let error = run_command_with_stdin_file_deadline(
            &spec,
            &evidence,
            Instant::now() + Duration::from_secs(10),
            &input,
            &identity,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("external_stdin_exact_bytes_mismatch"));
        assert!(!evidence.exists());
    }
    assert!(
        run_command_with_stdin_file_deadline(
            &spec,
            &f.0.join("directory"),
            Instant::now() + Duration::from_secs(10),
            &f.0,
            &expected
        )
        .is_err()
    );
    assert!(!f.0.join("directory").exists());
    assert!(
        run_command_with_stdin_file_deadline(
            &spec,
            &f.0.join("expired"),
            Instant::now() - Duration::from_secs(1),
            &input,
            &expected
        )
        .is_err()
    );
    assert!(!f.0.join("expired").exists());
    assert!(!f.0.join("must-not-start").exists());
}

#[test]
fn windows_stdin_source_mutation_keeps_snapshot_and_rejects_success() {
    let f = Fixture::new();
    let (input, expected) = f.input(b"original");
    let result = run_command_with_stdin_file_deadline(
        &f.command("mutate", "marker"),
        &f.0.join("mutation"),
        Instant::now() + Duration::from_secs(10),
        &input,
        &expected,
    )
    .unwrap();
    assert_eq!(result.status, "stdin_source_changed");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.report["observed_status"], "completed");
    assert_eq!(result.report["stdin"]["unchanged"], false);
    assert_eq!(result.report["capture_complete"], true);
    f.receipt("mutation", &result.report, b"original");
}

#[test]
fn windows_stdin_nonzero_and_original_deadline_stay_failures() {
    let f = Fixture::new();
    let (input, expected) = f.input(b"original");
    let failed = run_command_with_stdin_file_deadline(
        &f.command("fail", "failure-marker"),
        &f.0.join("nonzero"),
        Instant::now() + Duration::from_secs(10),
        &input,
        &expected,
    )
    .unwrap();
    assert_eq!(failed.status, "process_failed");
    assert_eq!(failed.exit_code, Some(13));
    assert_eq!(failed.report["stdin"]["unchanged"], true);
    f.receipt("nonzero", &failed.report, b"original");
    let deadline = Instant::now() + Duration::from_millis(1500);
    let timed = run_command_with_stdin_file_deadline(
        &f.command("hold", "timeout-marker"),
        &f.0.join("timeout"),
        deadline,
        &input,
        &expected,
    )
    .unwrap();
    assert_ne!(timed.status, "completed");
    assert_eq!(timed.report["borrowed_deadline"]["expired"], true);
    f.receipt("timeout", &timed.report, b"original");
    assert!(
        run_command_with_stdin_file_deadline(
            &f.command("ok", "must-not-restart"),
            &f.0.join("reused"),
            deadline,
            &input,
            &expected
        )
        .is_err()
    );
    assert!(!f.0.join("must-not-restart").exists());
    assert!(!f.0.join("reused").exists());
}
