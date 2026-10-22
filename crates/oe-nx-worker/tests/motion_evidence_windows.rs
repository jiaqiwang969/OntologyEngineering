#![cfg(windows)]
use oe_contracts::{NxMotionEvidenceBinding, NxMotionEvidenceBudget};
use oe_nx_worker::motion_evidence::{segment_name, windows::*};
use oe_windows_controller::windows::StableRoot;
fn binding() -> NxMotionEvidenceBinding {
    serde_json::from_value(serde_json::json!({"job_id":"motion-test","object":{"object_id":"assembly","revision":"r1","condition_scope":"fixture","source_sha256":"a".repeat(64)},"intent":{"path":"motion.json","sha256":"b".repeat(64)},"samples":2})).unwrap()
}
fn budget() -> NxMotionEvidenceBudget {
    NxMotionEvidenceBudget {
        max_events: 100,
        max_bytes: 32768,
        max_event_bytes: 1024,
        segment_bytes: 2048,
        max_segments: 16,
    }
}
#[test]
fn owned_windows_stream_roundtrip_and_extra_file_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let root = StableRoot::open(temp.path(), None).unwrap();
    let dir = root
        .create_directory_new(std::ffi::OsStr::new("evidence"))
        .unwrap();
    let mut writer = create_owned_writer(&dir, budget(), binding()).unwrap();
    for i in 0..20 {
        writer
            .append(
                "pair_returned",
                serde_json::json!({"i":i,"raw":2,"detail":"x".repeat(180)}),
            )
            .unwrap();
    }
    let summary = writer.seal().unwrap();
    verify_owned_stream(&dir, &summary, &budget(), &binding()).unwrap();
    dir.create_file_new(std::ffi::OsStr::new("unexpected.jsonl"), b"x", 1)
        .unwrap();
    assert!(verify_owned_stream(&dir, &summary, &budget(), &binding()).is_err());
}
#[test]
fn owned_windows_stream_rejects_hardlinked_originals_and_existing_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = StableRoot::open(temp.path(), None).unwrap();
    let dir = root
        .create_directory_new(std::ffi::OsStr::new("evidence"))
        .unwrap();
    let mut writer = create_owned_writer(&dir, budget(), binding()).unwrap();
    let summary = writer.seal().unwrap();
    assert!(create_owned_writer(&dir, budget(), binding()).is_err());
    std::fs::hard_link(
        dir.path().join(segment_name(0)),
        temp.path().join("alias.jsonl"),
    )
    .unwrap();
    assert!(verify_owned_stream(&dir, &summary, &budget(), &binding()).is_err());
}

#[test]
fn owned_sink_checks_actual_directory_and_is_terminal_after_failure() {
    for extra in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = StableRoot::open(temp.path(), None).unwrap();
        let dir = root
            .create_directory_new(std::ffi::OsStr::new("evidence"))
            .unwrap();
        let mut sink = create_owned_sink(&dir, budget(), binding()).unwrap();
        oe_nx_worker::motion_evidence::sink::record_native(
            sink.as_mut(),
            serde_json::json!({"phase":"interference_pending","a":1,"b":2}),
        )
        .unwrap();
        if extra {
            dir.create_file_new(std::ffi::OsStr::new("unexpected.jsonl"), b"x", 1)
                .unwrap();
        }
        assert_eq!(sink.seal_and_verify().is_err(), extra);
        assert!(sink.seal_and_verify().is_err());
        assert!(sink.append("retry", serde_json::json!({})).is_err());
    }
}
