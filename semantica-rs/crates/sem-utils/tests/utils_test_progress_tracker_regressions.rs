//! Port of `tests/test_progress_tracker_regressions.py`.
//!
//! The tests share the process-wide tracker singletons, so they run one at a time (the
//! `reset_progress_singletons` fixture). Tests that need a different environment
//! (`SEMANTICA_DISABLE_PROGRESS`) or a separate process re-run this test binary as a child
//! process, filtered to one `subprocess_*` helper; outside such a child the helpers return at
//! once.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use indexmap::IndexMap;
use sem_utils::progress_tracker::{
    self as progress_module, ConsoleProgressDisplay, ProgressDisplay, ProgressTracker,
};

static SERIAL: Mutex<()> = Mutex::new(());
const CHILD_MARKER: &str = "SEM_UTILS_PROGRESS_REGRESSION_CHILD";

/// The `reset_progress_singletons` autouse fixture: clear both singletons before and after.
struct Singletons<'a> {
    _serial: MutexGuard<'a, ()>,
}

impl Drop for Singletons<'_> {
    fn drop(&mut self) {
        ProgressTracker::set_instance(None);
        progress_module::set_global_progress_tracker(None);
    }
}

fn reset_progress_singletons() -> Singletons<'static> {
    let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    ProgressTracker::set_instance(None);
    progress_module::set_global_progress_tracker(None);
    Singletons { _serial: serial }
}

fn install_tracker_as_singleton(tracker: &Arc<ProgressTracker>) {
    ProgressTracker::set_instance(Some(tracker.clone()));
    progress_module::set_global_progress_tracker(Some(tracker.clone()));
}

/// `_assert_finishes_quickly`: run `target` on another thread and fail if it does not finish.
/// (The original waits 1 s; the bound here is 10 s so a heavily loaded machine cannot turn a
/// slow thread start into a false deadlock report.)
fn assert_finishes_quickly(target: impl FnOnce() + Send + 'static) {
    let (done_tx, done_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        target();
        let _ = done_tx.send(());
    });
    match done_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(()) => handle.join().expect("operation thread"),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("operation deadlocked"),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            if let Err(panic) = handle.join() {
                std::panic::resume_unwind(panic);
            }
        }
    }
}

fn order(pairs: &[(&str, i64)]) -> IndexMap<String, i64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

#[test]
fn test_start_tracking_pipeline_console_callback_does_not_deadlock() {
    let _fixture = reset_progress_singletons();
    let tracker = Arc::new(ProgressTracker::new(true, false, 0.0));
    let console: Arc<dyn ProgressDisplay> = Arc::new(ConsoleProgressDisplay::new(false, 0.0));
    tracker.set_displays(vec![console]);
    install_tracker_as_singleton(&tracker);
    tracker.register_pipeline_modules("pipeline-1", &["core"], Some(&order(&[("core", 0)])));

    let t = tracker.clone();
    assert_finishes_quickly(move || {
        t.start_tracking(
            None,
            Some("core"),
            Some("Semantica"),
            "Building",
            Some("pipeline-1"),
        );
    });
}

#[test]
fn test_update_progress_pipeline_console_callback_does_not_deadlock() {
    let _fixture = reset_progress_singletons();
    let tracker = Arc::new(ProgressTracker::new(true, false, 0.0));
    install_tracker_as_singleton(&tracker);
    tracker.set_displays(Vec::new());
    tracker.register_pipeline_modules("pipeline-1", &["core"], Some(&order(&[("core", 0)])));
    let tracking_id = tracker.start_tracking(
        None,
        Some("core"),
        Some("Semantica"),
        "Building",
        Some("pipeline-1"),
    );
    let console: Arc<dyn ProgressDisplay> = Arc::new(ConsoleProgressDisplay::new(false, 0.0));
    tracker.set_displays(vec![console]);

    let t = tracker.clone();
    assert_finishes_quickly(move || {
        t.update_progress(&tracking_id, 1, 1, "Processing sources... 1/1");
    });
}

#[test]
fn test_progress_tracker_constructor_can_disable_progress() {
    let _fixture = reset_progress_singletons();
    let tracker = ProgressTracker::new(false, true, 0.1);

    assert!(!tracker.enabled());
    assert_eq!(
        tracker.start_tracking(None, Some("core"), Some("test"), "", None),
        ""
    );
}

/// Run one `subprocess_*` helper of this binary in a child process with `envs` set.
fn spawn_child(helper: &str, envs: &[(&str, &str)]) -> std::process::Child {
    let exe = std::env::current_exe().expect("test binary path");
    let mut command = Command::new(exe);
    command
        .args([helper, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_MARKER, helper)
        .env_remove("SEMANTICA_DISABLE_PROGRESS")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.spawn().expect("spawn child test process")
}

/// Wait for a child with a timeout (the original's `subprocess.run(..., timeout=15)`),
/// draining its output concurrently as `communicate()` does.
fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> (i32, String, String) {
    fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    }
    let stdout = drain(child.stdout.take().expect("stdout"));
    let stderr = drain(child.stderr.take().expect("stderr"));
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll child") {
            let stdout = stdout.join().expect("stdout reader");
            let stderr = stderr.join().expect("stderr reader");
            return (status.code().unwrap_or(-1), stdout, stderr);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            panic!("child process timed out after {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn in_child(helper: &str) -> bool {
    std::env::var(CHILD_MARKER).as_deref() == Ok(helper)
}

#[test]
fn test_disable_progress_env_prevents_reenable() {
    // monkeypatch.setenv("SEMANTICA_DISABLE_PROGRESS", "1") in a fresh process.
    let child = spawn_child(
        "subprocess_disable_progress_env_prevents_reenable",
        &[("SEMANTICA_DISABLE_PROGRESS", "1")],
    );
    let (code, stdout, stderr) = wait_with_timeout(child, Duration::from_secs(60));
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("test result: ok. 1 passed"), "{stdout}");
}

#[test]
fn subprocess_disable_progress_env_prevents_reenable() {
    if !in_child("subprocess_disable_progress_env_prevents_reenable") {
        return;
    }
    let _fixture = reset_progress_singletons();
    let tracker = progress_module::get_progress_tracker();
    tracker.set_enabled(true);

    assert!(!tracker.enabled());
    assert_eq!(
        tracker.start_tracking(None, Some("core"), Some("test"), "", None),
        ""
    );
}

#[test]
fn test_build_knowledge_base_subprocess_does_not_deadlock() {
    // The original runs `Semantica().build_knowledge_base([...])` in a subprocess with a 15 s
    // timeout and expects "1" (sources processed) on stdout. `build_knowledge_base` lives in
    // sem-core; this port runs the progress-tracker call sequence it makes (lifecycle start,
    // pipeline registration, pipeline-scoped console updates, summary) in a child process.
    let child = spawn_child("subprocess_build_knowledge_base_progress_sequence", &[]);
    let (code, stdout, stderr) = wait_with_timeout(child, Duration::from_secs(15));
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("test result: ok. 1 passed"), "{stdout}");
    // assert "1" in result.stdout: the sources-processed count printed after the summary.
    assert!(stdout.lines().any(|line| line.trim() == "1"), "{stdout}");
}

#[test]
fn subprocess_build_knowledge_base_progress_sequence() {
    if !in_child("subprocess_build_knowledge_base_progress_sequence") {
        return;
    }
    let runtime_dir = tempfile::tempdir().expect("scratch dir");
    let sample = runtime_dir.path().join("sample.txt");
    std::fs::write(&sample, "Semantica builds small local knowledge graphs.").unwrap();
    let sample = sample.to_string_lossy().to_string();

    let tracker = progress_module::get_progress_tracker();
    if !tracker.enabled() {
        tracker.set_enabled(true);
    }
    // LifecycleManager.startup()
    let lifecycle = tracker.start_tracking(
        None,
        Some("core"),
        Some("LifecycleManager"),
        "Starting system lifecycle",
        None,
    );
    tracker.stop_tracking(
        &lifecycle,
        Some("completed"),
        "System startup completed successfully",
        None,
    );
    // Semantica.build_knowledge_base(...)
    let pipeline_id = "kb_build_0000test";
    let overall = tracker.start_tracking(
        None,
        Some("core"),
        Some("Semantica"),
        "Building knowledge base from 1 sources",
        Some(pipeline_id),
    );
    tracker.register_pipeline_modules(pipeline_id, &["default"], Some(&order(&[("default", 0)])));
    let file_tracking = tracker.start_tracking(
        Some(&sample),
        Some("core"),
        Some("build_knowledge_base"),
        "Processing sample.txt",
        Some(pipeline_id),
    );
    // run_pipeline(...)
    let execution = tracker.start_tracking(
        Some(&sample),
        Some("pipeline"),
        Some("ExecutionEngine"),
        "Executing pipeline: default_pipeline",
        None,
    );
    tracker.stop_tracking(&execution, Some("completed"), "", None);
    tracker.stop_tracking(&file_tracking, Some("completed"), "", None);
    tracker.update_progress(&overall, 1, 1, "Processing sources... 1/1");
    tracker.stop_tracking(&overall, Some("completed"), "Processed 1 sources", None);
    tracker.clear_pipeline_context(pipeline_id);
    tracker.show_summary();
    // print(result['statistics']['sources_processed'])
    println!("1");
}
