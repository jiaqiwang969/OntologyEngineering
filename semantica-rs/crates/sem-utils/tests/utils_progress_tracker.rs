//! Characterisation of `semantica/utils/progress_tracker.py` (expected values from the oracle).
//!
//! Tests that touch the process-wide tracker singletons run one at a time.

mod common;

use std::io::Write;
use std::panic::Location;
use std::sync::{Arc, Mutex, MutexGuard};

use common::{obj, oracle};
use indexmap::IndexMap;
use regex::Regex;
use sem_utils::SemanticaError;
use sem_utils::logging::{Handler, LogLevel, Logger};
use sem_utils::progress_tracker::{
    ConsoleProgressDisplay, FileProgressDisplay, FrameInfo, IPYTHON_AVAILABLE,
    JupyterProgressDisplay, ModuleDetector, ProgressDisplay, ProgressItem, ProgressTracker,
    get_progress_tracker, set_global_progress_tracker, track_progress,
};
use serde_json::{Value, json};

static SERIAL: Mutex<()> = Mutex::new(());

struct Singletons {
    _serial: MutexGuard<'static, ()>,
}

impl Drop for Singletons {
    fn drop(&mut self) {
        ProgressTracker::set_instance(None);
        set_global_progress_tracker(None);
    }
}

fn singletons() -> Singletons {
    let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    ProgressTracker::set_instance(None);
    set_global_progress_tracker(None);
    Singletons { _serial: serial }
}

/// A `Write` sink the test can read back.
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Buffer {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

fn console(use_emoji: bool, interval: f64) -> (ConsoleProgressDisplay, Buffer) {
    let buffer = Buffer::default();
    (
        ConsoleProgressDisplay::with_writer(use_emoji, interval, Box::new(buffer.clone())),
        buffer,
    )
}

fn item(spec: &Value) -> ProgressItem {
    serde_json::from_value(spec.clone()).expect("progress item spec")
}

/// `ITEMS` of the fixture generator.
fn fixture_items() -> Vec<ProgressItem> {
    let mut seen = Vec::new();
    for case in oracle("console_lines").as_array().unwrap() {
        if case["use_emoji"] == json!(true) {
            seen.push(item(&case["item"]));
        }
    }
    seen
}

fn mask_times(text: &str) -> String {
    Regex::new(r"\d+\.\d\ds")
        .unwrap()
        .replace_all(text, "<t>s")
        .into_owned()
}

#[test]
fn test_console_lines_match_oracle() {
    for case in oracle("console_lines").as_array().unwrap() {
        let use_emoji = case["use_emoji"].as_bool().unwrap();
        let (display, buffer) = console(use_emoji, 0.0);
        display.update(&item(&case["item"]));
        assert_eq!(buffer.text(), case["output"].as_str().unwrap(), "{case}");
        let lines: Value = Value::Object(
            display
                .current_lines()
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect(),
        );
        assert_eq!(lines, case["current_lines"], "{case}");
    }
}

#[test]
fn test_console_summary_and_clear_match_oracle() {
    let items = fixture_items();
    for case in oracle("console_summary").as_array().unwrap() {
        let use_emoji = case["use_emoji"].as_bool().unwrap();
        let (display, buffer) = console(use_emoji, 0.0);
        display.show_summary(&items);
        assert_eq!(
            buffer.text(),
            case["output"].as_str().unwrap(),
            "emoji={use_emoji}"
        );
    }
    let expected = oracle("console_clear");
    let (display, buffer) = console(true, 0.0);
    display.update(&items[0]);
    display.clear();
    assert_eq!(buffer.text(), expected["output"].as_str().unwrap());
    assert!(display.current_lines().is_empty());
    assert_eq!(expected["current_lines"], json!({}));
    assert!(display.use_emoji());
    assert_eq!(display.update_interval(), 0.0);
}

#[test]
fn test_console_throttle_matches_oracle() {
    let _s = singletons();
    let expected = oracle("throttle");
    let tracker = ProgressTracker::new(true, false, 3600.0);
    let buffer = Buffer::default();
    let display = Arc::new(ConsoleProgressDisplay::with_writer(
        false,
        3600.0,
        Box::new(buffer.clone()),
    ));
    tracker.set_displays(vec![display.clone()]);
    let tid = tracker.start_tracking(None, Some("m"), Some("S"), "a", None);
    tracker.update_tracking(&tid, None, "b");
    tracker.update_progress(&tid, 1, 2, "c");
    tracker.update_tracking(&tid, None, "d");
    let masked = mask_times(&buffer.text());
    let masked = Regex::new(r"Rate: [0-9.]+/s")
        .unwrap()
        .replace_all(&masked, "Rate: <r>/s");
    let masked = Regex::new(r"ETA: [0-9.]+[smh]")
        .unwrap()
        .replace_all(&masked, "ETA: <e>");
    assert_eq!(masked, expected["output"].as_str().unwrap());
    assert_eq!(
        display.last_update() > 0.0,
        expected["last_update_restored"].as_bool().unwrap()
    );
    display.set_last_update(0.0);
    assert_eq!(display.last_update(), 0.0);
}

fn pipeline_order(pairs: &[(&str, i64)]) -> IndexMap<String, i64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

#[test]
fn test_pipeline_rendering_matches_oracle() {
    let _s = singletons();
    let tracker = Arc::new(ProgressTracker::new(true, true, 0.0));
    tracker.set_displays(Vec::new());
    ProgressTracker::set_instance(Some(tracker.clone()));
    tracker.register_pipeline_modules(
        "pipe",
        &["ingest", "parse", "core"],
        Some(&pipeline_order(&[("ingest", 0), ("parse", 1), ("core", 2)])),
    );
    let tid1 = tracker.start_tracking(
        Some("a.txt"),
        Some("ingest"),
        Some("FileIngestor"),
        "Ingesting",
        None,
    );
    let _tid2 = tracker.start_tracking(
        None,
        Some("core"),
        Some("Semantica"),
        "Building",
        Some("pipe"),
    );
    tracker.stop_tracking(
        &tid1,
        None,
        "",
        Some(&obj(&json!({"extraction_counts": {"pages": 2}}))),
    );
    let items: Vec<Value> = tracker
        .get_pipeline_items("pipe")
        .iter()
        .map(|i| {
            json!({"module": i.module, "submodule": i.submodule, "status": i.status,
                   "pipeline_id": i.pipeline_id, "pipeline_order": i.pipeline_order,
                   "metadata": i.metadata, "file": i.file})
        })
        .collect();
    assert_eq!(Value::Array(items), *oracle("pipeline_items"));

    let probe = ProgressItem {
        module: Some("core".into()),
        submodule: Some("Semantica".into()),
        status: "running".into(),
        message: "Building".into(),
        pipeline_id: Some("pipe".into()),
        ..ProgressItem::default()
    };
    let (display, buffer) = console(false, 0.0);
    display.update(&probe);
    display.update(&probe);
    let lines: Vec<Value> = mask_times(&buffer.text())
        .split('\n')
        .map(|l| Value::String(l.to_string()))
        .collect();
    assert_eq!(
        Value::Array(lines),
        *oracle("console_pipeline_output_lines")
    );

    let html = JupyterProgressDisplay::new(true).build_html(std::slice::from_ref(&probe));
    assert_eq!(
        mask_times(&html),
        oracle("jupyter_pipeline_html").as_str().unwrap()
    );
    assert_eq!(
        tracker.pipeline_contexts()["pipe"],
        vec!["ingest", "parse", "core"]
    );
    assert_eq!(tracker.pipeline_module_order()["pipe"]["core"], 2);
    assert_eq!(tracker.pipeline_items_by_id()["pipe"].len(), 2);
}

#[test]
fn test_jupyter_display_matches_oracle() {
    let _s = singletons();
    let items = fixture_items();
    for case in oracle("jupyter_html").as_array().unwrap() {
        let display = JupyterProgressDisplay::new(case["use_emoji"].as_bool().unwrap());
        assert_eq!(display.build_html(&items), case["html"].as_str().unwrap());
    }
    let many: Vec<ProgressItem> = (0..12)
        .map(|i| ProgressItem {
            module: Some(format!("m{i}")),
            submodule: Some("S".into()),
            status: "completed".into(),
            ..ProgressItem::default()
        })
        .collect();
    assert_eq!(
        JupyterProgressDisplay::default().build_html(&many),
        oracle("jupyter_html_last_ten").as_str().unwrap()
    );
    let summaries = oracle("jupyter_summary_html").as_array().unwrap();
    assert_eq!(
        JupyterProgressDisplay::new(true).summary_html(&items),
        summaries[0].as_str().unwrap()
    );
    assert_eq!(
        JupyterProgressDisplay::new(false).summary_html(&items),
        summaries[1].as_str().unwrap()
    );
}

#[test]
fn test_jupyter_display_keeps_items_without_ipython() {
    const { assert!(!IPYTHON_AVAILABLE) };
    let display = JupyterProgressDisplay::new(false);
    assert!(!display.use_emoji());
    let mut first = ProgressItem {
        module: Some("m".into()),
        submodule: Some("s".into()),
        ..ProgressItem::default()
    };
    display.update(&first);
    first.status = "completed".into();
    display.update(&first);
    display.update(&ProgressItem::default());
    assert_eq!(display.items().len(), 2);
    assert_eq!(display.items()[0].status, "completed");
    display.show_summary(&display.items());
    display.clear();
    assert!(display.items().is_empty());
}

#[test]
fn test_file_display_matches_oracle() {
    let expected = oracle("file_display");
    let logger = Logger::get("semantica.progress_fixture");
    let capture = Handler::capture();
    logger.add_handler(capture.clone());
    logger.set_level(LogLevel::DEBUG);
    logger.set_propagate(false);
    let display = FileProgressDisplay::new(Some(logger.clone()));
    let items = fixture_items();
    for item in &items {
        display.update(item);
    }
    let mut second = items[0].clone();
    second.message = "second".into();
    display.update(&second);
    display.show_summary(&items);
    let before_clear = display.items();
    display.clear();
    logger.remove_handler(&capture);
    assert_eq!(json!(capture.messages()), expected["messages"]);
    let rows: Vec<Value> = display
        .items()
        .iter()
        .map(|i| json!([i.module, i.submodule, i.file, i.message]))
        .collect();
    assert_eq!(Value::Array(rows), expected["items"]);
    // clear() is a no-op for log files.
    assert_eq!(display.items(), before_clear);
    assert_eq!(display.logger().name(), "semantica.progress_fixture");
    assert_eq!(
        FileProgressDisplay::default().logger().name(),
        "semantica.progress"
    );
}

#[test]
fn test_module_detector_matches_oracle() {
    for case in oracle("detect_from_frame").as_array().unwrap() {
        let spec = &case[0];
        let frame = FrameInfo {
            module_name: spec["module_name"].as_str().map(str::to_string),
            self_class: spec["self_class"].as_str().map(str::to_string),
            cls_name: spec["cls_name"].as_str().map(str::to_string),
            qualname: spec["qualname"].as_str().map(str::to_string),
        };
        let (module, submodule) = ModuleDetector::detect_from_frame(&frame);
        assert_eq!(json!([module, submodule]), case[1], "{spec}");
    }
    let (module, submodule) = ModuleDetector::detect_from_call_stack(0);
    assert_eq!(json!([module, submodule]), *oracle("detect_depth0"));
    // Any other depth resolves to the caller's source file: a test file is not a
    // `semantica.*` module, so nothing is detected (as for the original's test modules).
    assert_eq!(ModuleDetector::detect_from_call_stack(2), (None, None));
    // A crate source path maps to its Python module.
    let frame = FrameInfo::from_location(Location::caller());
    assert!(
        frame
            .module_name
            .unwrap()
            .contains("utils_progress_tracker")
    );
    let mapped = ModuleDetector::detect_from_frame(&FrameInfo {
        module_name: Some("semantica.ingest.file_ingestor".into()),
        ..FrameInfo::default()
    });
    assert_eq!(mapped, (Some("ingest".into()), None));
}

#[test]
fn test_tracker_state_machine_matches_oracle() {
    let _s = singletons();
    let expected = oracle("tracker_state");
    let t = ProgressTracker::new(true, true, 0.0);
    t.set_displays(Vec::new());
    let a = t.start_tracking(
        Some("f.txt"),
        Some("ingest"),
        Some("FileIngestor"),
        "m",
        None,
    );
    let b = t.start_tracking(None, Some("parse"), Some("Parser"), "", None);
    t.update_progress(&b, 0, 0, "");
    t.update_progress(&b, 3, 4, "three");
    let c = t.start_tracking(None, Some("kg"), Some("Builder"), "", None);
    t.update_tracking(&c, Some("failed"), "boom");
    t.stop_tracking(&a, None, "", Some(&obj(&json!({"k": 1}))));
    assert_eq!(json!([a, b, c]), expected["ids"]);
    let items: Vec<Value> = t
        .items()
        .iter()
        .map(|i| {
            json!([
                i.module,
                i.status,
                i.message,
                i.progress_percentage,
                i.estimated_remaining,
                i.metadata,
                i.emoji
            ])
        })
        .collect();
    assert_eq!(Value::Array(items), expected["items"]);
    let active: serde_json::Map<String, Value> = t
        .active_items()
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                json!([
                    v.module,
                    v.status,
                    v.message,
                    v.processed_items,
                    v.total_items,
                    v.progress_percentage
                ]),
            )
        })
        .collect();
    assert_eq!(Value::Object(active), expected["active"]);
    t.update_tracking("nope", None, "");
    assert_eq!(expected["update_unknown"], json!({"ok": null}));
    let disabled = ProgressTracker::new(false, true, 0.1);
    assert_eq!(
        disabled.start_tracking(None, Some("m"), Some("s"), "", None),
        expected["disabled_start"].as_str().unwrap()
    );
}

#[test]
fn test_tracker_pipeline_registration_matches_oracle() {
    let _s = singletons();
    let t2 = ProgressTracker::new(true, false, 0.0);
    t2.set_displays(Vec::new());
    t2.register_pipeline_modules("p1", &["a", "b", "a"], None);
    let snapshot = json!({
        "contexts": t2.pipeline_contexts(),
        "order": t2.pipeline_module_order(),
        "items": t2.pipeline_items_by_id().keys().collect::<Vec<_>>(),
    });
    assert_eq!(snapshot, *oracle("tracker_pipeline_order"));
    let x = t2.start_tracking(None, Some("b"), Some("B"), "", None);
    let active = &t2.active_items()[&x];
    assert_eq!(
        json!({"id": x, "pipeline_id": active.pipeline_id, "order": active.pipeline_order, "emoji": active.emoji}),
        *oracle("tracker_pipeline_join")
    );
    t2.update_tracking(&x, Some("completed"), "");
    let kept = t2.pipeline_items_by_id();
    assert_eq!(
        json!({"items": t2.items().len(), "pipeline": kept["p1"].keys().collect::<Vec<_>>(),
               "pct": kept["p1"][&x].progress_percentage}),
        *oracle("tracker_pipeline_kept")
    );
    t2.clear_pipeline_context("p1");
    assert_eq!(
        json!({"contexts": t2.pipeline_contexts(), "items": t2.pipeline_items_by_id().keys().collect::<Vec<_>>()}),
        *oracle("tracker_after_clear")
    );
}

#[test]
fn test_track_and_track_progress_match_oracle() {
    let _s = singletons();
    let t3 = Arc::new(ProgressTracker::new(true, true, 0.0));
    t3.set_displays(Vec::new());
    ProgressTracker::set_instance(Some(t3.clone()));
    let mut ids = Vec::new();
    t3.track(Some("doc.pdf"), Some("ingest"), Some("X"), "m", |tid| {
        ids.push(tid.to_string());
        Ok::<_, SemanticaError>(())
    })
    .unwrap();
    let propagated = t3
        .track(Some("doc.pdf"), Some("ingest"), Some("Y"), "m", |tid| {
            ids.push(tid.to_string());
            Err::<(), _>(SemanticaError::Value("boom".into()))
        })
        .unwrap_err();
    let expected = oracle("track");
    let rows = |items: &[ProgressItem]| -> Vec<Value> {
        items
            .iter()
            .map(|i| json!([i.file, i.module, i.submodule, i.status, i.message]))
            .collect()
    };
    assert_eq!(
        json!({"ids": ids, "propagated": propagated.to_string(), "items": rows(&t3.items())}),
        *expected
    );

    set_global_progress_tracker(Some(t3.clone()));
    let r1 = track_progress(Some("doc.pdf"), Some("ingest"), Some("Decorated"))
        .run("decorated_ok", || Ok::<_, SemanticaError>(21 * 2))
        .unwrap();
    let r2 = track_progress(None, Some("ingest"), Some("Decorated"))
        .run("decorated_bad", || {
            Err::<i32, _>(SemanticaError::Runtime("bad things".into()))
        })
        .unwrap_err();
    let r3 = track_progress(None, None, None)
        .run("bare", || Ok::<_, SemanticaError>("bare"))
        .unwrap();
    let expected = oracle("track_progress");
    assert_eq!(
        json!({"results": [r1, r2.to_string(), r3], "items": rows(&t3.items()[2..])}),
        *expected
    );
}

#[test]
fn test_singletons_match_oracle() {
    let _s = singletons();
    let expected = oracle("singletons");
    let g1 = get_progress_tracker();
    let same = Arc::ptr_eq(&g1, &ProgressTracker::get_instance());
    ProgressTracker::set_instance(None);
    let g2 = get_progress_tracker();
    let fresh = ProgressTracker::get_instance();
    let display_types: Vec<&str> = g1
        .displays()
        .iter()
        .map(|d| {
            let any: &dyn std::any::Any = d.as_ref();
            if any.is::<ConsoleProgressDisplay>() {
                "ConsoleProgressDisplay"
            } else if any.is::<FileProgressDisplay>() {
                "FileProgressDisplay"
            } else {
                "other"
            }
        })
        .collect();
    assert_eq!(
        json!({"global_is_instance": same, "global_cached": Arc::ptr_eq(&g2, &g1),
               "instance_recreated": !Arc::ptr_eq(&fresh, &g1),
               "default_enabled": g1.enabled(), "default_use_emoji": g1.use_emoji(),
               "default_interval": g1.update_interval(), "display_types": display_types}),
        *expected
    );
    assert!(!g1.is_jupyter());
    assert!(!g1.disable_jupyter_progress());
}

#[test]
fn test_tracker_show_summary_and_disabled_noops_match_oracle() {
    let _s = singletons();
    let t5 = ProgressTracker::new(true, false, 0.0);
    let buffer = Buffer::default();
    let display = Arc::new(ConsoleProgressDisplay::with_writer(
        false,
        0.0,
        Box::new(buffer.clone()),
    ));
    t5.set_displays(vec![display]);
    let a5 = t5.start_tracking(Some("/x/a.txt"), Some("parse"), Some("P"), "", None);
    t5.stop_tracking(&a5, Some("failed"), "bad", None);
    let _b5 = t5.start_tracking(None, Some("kg"), Some("K"), "", None);
    buffer.0.lock().unwrap().clear();
    t5.show_summary();
    assert_eq!(
        mask_times(&buffer.text()),
        oracle("tracker_show_summary").as_str().unwrap()
    );
    t5.set_enabled(false);
    assert_eq!(
        json!([
            t5.get_pipeline_items("x"),
            t5.start_tracking(None, Some("a"), Some("b"), "", None)
        ]),
        *oracle("tracker_disabled_noops")
    );
}

/// A user-defined display: the abstract methods of `ProgressDisplay` are what the tracker
/// calls.
#[derive(Default)]
struct Recording {
    calls: Mutex<Vec<String>>,
}

impl ProgressDisplay for Recording {
    fn update(&self, item: &ProgressItem) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("update {} {}", item.status, item.message));
    }
    fn show_summary(&self, items: &[ProgressItem]) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("summary {}", items.len()));
    }
    fn clear(&self) {
        self.calls.lock().unwrap().push("clear".into());
    }
}

#[test]
fn test_custom_display_receives_tracker_events() {
    let _s = singletons();
    let tracker = ProgressTracker::new(true, true, 0.0);
    let recording = Arc::new(Recording::default());
    tracker.set_displays(vec![recording.clone()]);
    assert_eq!(tracker.displays().len(), 1);
    let id = tracker.start_tracking(None, Some("export"), Some("E"), "start", None);
    tracker.update_tracking(&id, None, "working");
    tracker.update_progress(&id, 1, 2, "half");
    tracker.stop_tracking(&id, None, "done", None);
    tracker.show_summary();
    for display in tracker.displays() {
        display.clear();
    }
    assert_eq!(
        *recording.calls.lock().unwrap(),
        vec![
            "update running start",
            "update running working",
            "update running half",
            "update completed done",
            "summary 1",
            "clear",
        ]
    );
}
