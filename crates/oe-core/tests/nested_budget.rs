//! Independent normal-process regression: no fake compilation or authority.
#![cfg(unix)]
use oe_core::process::{CommandSpec, run_command};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn command(root: &Path, role: &str, timeout: Duration) -> CommandSpec {
    CommandSpec {
        executable: std::env::current_exe().unwrap(),
        arguments: vec![
            "--exact".into(),
            "nested_fixture_worker".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ],
        cwd: root.into(),
        environment: BTreeMap::from([
            ("OE_NESTED_TEST_ROLE".into(), role.into()),
            ("OE_NESTED_TEST_ROOT".into(), root.as_os_str().into()),
        ]),
        remove_environment: vec![],
        timeout,
        output_limit: 4096,
    }
}
fn folder() -> PathBuf {
    folder_at(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    )
}
fn folder_at(timestamp: u128) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("var/nested-budget-tests");
    fs::create_dir_all(&root).unwrap();
    let sequence = DIRECTORY_SEQUENCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .expect("private fixture directory sequence exhausted");
    let root = root.join(format!("{}-{}-{}", std::process::id(), timestamp, sequence));
    fs::create_dir(&root).unwrap();
    root
}
#[test]
#[ignore = "private self-executable normal-process fixture only"]
fn nested_fixture_worker() {
    let role = std::env::var("OE_NESTED_TEST_ROLE").unwrap();
    let root = PathBuf::from(std::env::var_os("OE_NESTED_TEST_ROOT").unwrap());
    match role.as_str() {
        "leaf-timeout" | "leaf-normal" => {
            fs::write(root.join("leaf-started"), std::process::id().to_string()).unwrap();
            std::thread::sleep(if role == "leaf-timeout" {
                Duration::from_millis(1000)
            } else {
                Duration::from_millis(20)
            });
            fs::write(root.join("leaf-finished"), b"own test leaf only").unwrap();
        }
        "parent-timeout" | "parent-normal" => {
            let leaf = if role == "parent-timeout" {
                "leaf-timeout"
            } else {
                "leaf-normal"
            };
            let observed = run_command(
                &command(&root, leaf, Duration::from_secs(10)),
                &root.join("inner-process"),
            )
            .unwrap();
            assert_eq!(observed.status, "completed");
            assert_eq!(observed.exit_code, Some(0));
        }
        _ => panic!("unknown private fixture role"),
    }
}
#[test]
fn nested_descendant_timeout_is_cleaned_before_it_can_write_later() {
    let root = folder();
    let start = Instant::now();
    let observed = run_command(
        &command(&root, "parent-timeout", Duration::from_millis(250)),
        &root.join("outer-process"),
    )
    .unwrap();
    assert_eq!(observed.status, "timed_out");
    assert!(
        root.join("leaf-started").exists(),
        "fixture must really dispatch its own nested child before timeout"
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "outer normal cancellation remains bounded"
    );
    std::thread::sleep(Duration::from_millis(1200));
    assert!(
        !root.join("leaf-finished").exists(),
        "own nested child continued after the supervising process timed out: {}",
        root.display()
    );
}
#[test]
fn nested_normal_completion_keeps_original_streams_and_result() {
    let root = folder();
    let observed = run_command(
        &command(&root, "parent-normal", Duration::from_secs(5)),
        &root.join("outer-process"),
    )
    .unwrap();
    assert_eq!(observed.status, "completed");
    assert_eq!(observed.exit_code, Some(0));
    assert_eq!(observed.report["capture_complete"], true);
    assert!(root.join("leaf-started").exists() && root.join("leaf-finished").exists());
}

#[test]
fn nested_fixture_directories_are_unique_at_the_same_timestamp() {
    let barrier = std::sync::Barrier::new(8);
    let roots = std::thread::scope(|scope| {
        let workers = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    folder_at(0)
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    let unique = roots.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        8,
        "same-tick concurrent fixtures must own distinct directories"
    );
    assert!(
        roots.iter().all(|root| root.is_dir()),
        "each fixture must create its actual directory"
    );
}
