#![cfg(unix)]
use oe_core::process::{CommandSpec, SpoolLimits, run_command, run_command_spooled};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
            "var/maintenance/spool-tests/{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn spec(&self, mode: &str, output_limit: usize) -> CommandSpec {
        CommandSpec {
            executable: std::env::current_exe().unwrap(),
            arguments: ["--exact", "spool_fixture", "--nocapture"]
                .map(Into::into)
                .to_vec(),
            cwd: self.0.clone(),
            environment: BTreeMap::from([("OE_SPOOL_FIXTURE".into(), mode.into())]),
            remove_environment: vec![],
            timeout: Duration::from_secs(5),
            output_limit,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn limits() -> SpoolLimits {
    SpoolLimits {
        preview_bytes: 1024,
        segment_bytes: 4096,
    }
}
fn raw(report: &Value, name: &str) -> Vec<u8> {
    report[name]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|segment| fs::read(segment["path"].as_str().unwrap()).unwrap())
        .collect()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn spool_fixture() {
    let Ok(mode) = std::env::var("OE_SPOOL_FIXTURE") else {
        return;
    };
    match mode.as_str() {
        "binary" => {
            let data = (0..38_111usize)
                .map(|i| ((i * 73 + i / 251) % 256) as u8)
                .collect::<Vec<_>>();
            std::io::stdout().write_all(&data).unwrap();
            std::io::stdout().flush().unwrap();
        }
        "fail" => {
            println!("original output");
            eprintln!("original compiler rejection");
            std::process::exit(7);
        }
        "large" => {
            std::io::stdout()
                .write_all(&vec![b'x'; 1024 * 1024])
                .unwrap();
            std::io::stdout().flush().unwrap();
        }
        "hold" => {
            println!("partial-started");
            std::io::stdout().flush().unwrap();
            std::thread::sleep(Duration::from_secs(3));
        }
        "descendants" => {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "spool_fixture", "--nocapture"])
                .env("OE_SPOOL_FIXTURE", "hold")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            println!("descendant:{}", child.id());
            std::io::stdout().flush().unwrap();
            std::thread::sleep(Duration::from_millis(150));
            let _ = child.try_wait();
        }
        "touch" => {
            fs::write(std::env::var_os("OE_SPOOL_MARKER").unwrap(), b"spawned").unwrap();
        }
        _ => panic!("unknown fixed spool fixture"),
    }
    std::process::exit(0);
}

#[test]
fn complete_large_binary_stream_preserves_all_bytes_and_bounded_preview() {
    let f = Fixture::new();
    let s = f.spec("binary", 64 * 1024);
    let baseline = run_command(&s, &f.0.join("reference")).unwrap();
    assert_eq!(baseline.status, "completed");
    let result = run_command_spooled(&s, &f.0.join("spooled"), deadline(), limits()).unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.report["capture_complete"], true);
    assert_eq!(raw(&result.report, "stdout"), baseline.stdout);
    assert_eq!(raw(&result.report, "stderr"), baseline.stderr);
    assert!(baseline.stdout.contains(&0) && std::str::from_utf8(&baseline.stdout).is_err());
    assert_eq!(result.stdout, baseline.stdout[..1024]);
    assert_eq!(result.report["stdout"]["preview_truncated"], true);
    assert_eq!(result.report["stdout"]["truncated"], false);
    assert!(
        result.report["stdout"]["segments"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    result.verify_retained().unwrap();
    let mut empty = s.clone();
    empty.executable = PathBuf::from("/usr/bin/true");
    empty.arguments.clear();
    let zero = run_command_spooled(
        &empty,
        &f.0.join("empty"),
        deadline(),
        SpoolLimits {
            preview_bytes: 0,
            segment_bytes: 4096,
        },
    )
    .unwrap();
    assert_eq!(zero.status, "completed");
    assert_eq!(zero.report["capture_complete"], true);
    assert_eq!(zero.report["stdout"]["bytes"], 0);
    assert_eq!(zero.report["stderr"]["bytes"], 0);
    assert_eq!(
        zero.report["stdout"]["segments"].as_array().unwrap().len(),
        0
    );
    zero.verify_retained().unwrap();
}

#[test]
fn retains_both_streams_and_nonzero_exit() {
    let f = Fixture::new();
    let s = f.spec("fail", 64 * 1024);
    let result = run_command_spooled(&s, &f.0.join("fail"), deadline(), limits()).unwrap();
    assert_eq!(result.status, "process_failed");
    assert_eq!(result.exit_code, Some(7));
    assert_eq!(result.report["capture_complete"], true);
    assert!(
        String::from_utf8(raw(&result.report, "stdout"))
            .unwrap()
            .contains("original output")
    );
    assert!(
        String::from_utf8(raw(&result.report, "stderr"))
            .unwrap()
            .contains("original compiler rejection")
    );
    result.verify_retained().unwrap();
}

#[test]
fn raw_limit_exhaustion_retains_prefix_and_refuses_complete() {
    let f = Fixture::new();
    let s = f.spec("large", 64 * 1024);
    let result = run_command_spooled(&s, &f.0.join("limit"), deadline(), limits()).unwrap();
    assert_eq!(result.status, "output_limit");
    assert_eq!(result.report["capture_complete"], false);
    assert_eq!(result.report["stdout"]["bytes"], 64 * 1024);
    assert_eq!(result.report["stdout"]["truncated"], true);
    assert_eq!(
        result.report["stdout"]["observed_bytes_at_least"],
        64 * 1024 + 1
    );
    assert_eq!(raw(&result.report, "stdout").len(), 64 * 1024);
    assert_eq!(result.stdout.len(), 1024);
    result.verify_retained().unwrap();
}

#[test]
fn same_borrowed_deadline_preserves_partial_timeout_and_expired_setup() {
    let f = Fixture::new();
    let s = f.spec("hold", 64 * 1024);
    let before = Instant::now();
    let result = run_command_spooled(
        &s,
        &f.0.join("timeout"),
        before + Duration::from_millis(600),
        limits(),
    )
    .unwrap();
    assert!(before.elapsed() < Duration::from_secs(3));
    assert_eq!(result.status, "timed_out");
    assert_eq!(result.report["capture_complete"], false);
    assert!(
        String::from_utf8(raw(&result.report, "stdout"))
            .unwrap()
            .contains("partial-started")
    );
    assert!(
        result.report["effective_process_timeout_seconds"]
            .as_f64()
            .unwrap()
            < 0.6
    );
    result.verify_retained().unwrap();
    let not_started = f.0.join("expired");
    assert!(
        run_command_spooled(
            &s,
            &not_started,
            Instant::now() - Duration::from_secs(1),
            limits()
        )
        .is_err()
    );
    assert!(!not_started.exists());
}

#[test]
fn descendant_pipe_holder_is_cancelled_by_original_tree() {
    let f = Fixture::new();
    let s = f.spec("descendants", 64 * 1024);
    let start = Instant::now();
    let result = run_command_spooled(
        &s,
        &f.0.join("descendant"),
        start + Duration::from_millis(800),
        limits(),
    )
    .unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(result.status, "timed_out");
    assert_eq!(result.report["capture_complete"], false);
    assert_eq!(
        result.report["descendant_cleanup"]["state"],
        "local_cleanup_completed"
    );
    assert!(
        result.report["descendant_cleanup"]["observed_descendants"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        String::from_utf8(raw(&result.report, "stdout"))
            .unwrap()
            .contains("descendant:")
    );
    result.verify_retained().unwrap();
}

#[test]
fn malformed_limits_and_existing_destination_refuse_before_spawn() {
    let f = Fixture::new();
    let mut s = f.spec("touch", 64 * 1024);
    let marker = f.0.join("executed");
    s.environment
        .insert("OE_SPOOL_MARKER".into(), marker.clone().into_os_string());
    for (index, bad) in [
        SpoolLimits {
            preview_bytes: 4 * 1024 * 1024 + 1,
            segment_bytes: 4096,
        },
        SpoolLimits {
            preview_bytes: 0,
            segment_bytes: 0,
        },
        SpoolLimits {
            preview_bytes: 0,
            segment_bytes: 1,
        },
        SpoolLimits {
            preview_bytes: 0,
            segment_bytes: 128 * 1024,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let path = f.0.join(format!("bad-{index}"));
        assert!(run_command_spooled(&s, &path, deadline(), bad).is_err());
        assert!(!path.exists());
        assert!(!marker.exists());
    }
    let existing = f.0.join("existing");
    fs::create_dir(&existing).unwrap();
    assert!(run_command_spooled(&s, &existing, deadline(), limits()).is_err());
    assert!(!marker.exists());
}

#[test]
fn verification_refuses_missing_modified_reordered_or_symlinked_segments() {
    let f = Fixture::new();
    let s = f.spec("binary", 64 * 1024);
    let mut result = run_command_spooled(&s, &f.0.join("verified"), deadline(), limits()).unwrap();
    result.verify_retained().unwrap();
    let rows = result.report["stdout"]["segments"].as_array().unwrap();
    let first = PathBuf::from(rows[0]["path"].as_str().unwrap());
    let second = PathBuf::from(rows[1]["path"].as_str().unwrap());
    let held = f.0.join("held");
    fs::rename(&first, &held).unwrap();
    assert!(result.verify_retained().is_err());
    std::os::unix::fs::symlink(&held, &first).unwrap();
    assert!(result.verify_retained().is_err());
    fs::remove_file(&first).unwrap();
    fs::rename(&held, &first).unwrap();
    result.verify_retained().unwrap();
    fs::rename(&first, &held).unwrap();
    fs::rename(&second, &first).unwrap();
    fs::rename(&held, &second).unwrap();
    assert!(result.verify_retained().is_err());
    fs::rename(&first, &held).unwrap();
    fs::rename(&second, &first).unwrap();
    fs::rename(&held, &second).unwrap();
    result.verify_retained().unwrap();
    let original = fs::read(&first).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    fs::write(&first, &changed).unwrap();
    assert!(result.verify_retained().is_err());
    fs::write(&first, &original).unwrap();
    result.verify_retained().unwrap();
    result.stdout[0] ^= 1;
    assert!(result.verify_retained().is_err());
    result.stdout[0] ^= 1;
    result.report["capture_complete"] = Value::Bool(false);
    assert!(result.verify_retained().is_err());
}
