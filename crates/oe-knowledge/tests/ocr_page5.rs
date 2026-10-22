use oe_core::process::ProcessObservation;
use oe_knowledge::ocr_page5::{parse_request5, process_completed5};
use serde_json::{Value, json};
fn valid_request() -> Value {
    let a = |path: &str| json!({"path":path,"sha256":"a".repeat(64),"bytes":1});
    json!({"schema":"oev2.knowledge-ocr-source-request/5","tool":a("/explicit/source/tesseract"),
  "config":{"path":"/explicit/source/tsv","sha256":"59d079bb75d8b3d7c839a3564580cb559e362c93a9d70f234e421c0c3e767e04","bytes":22},
  "languages":[{"name":"chi_sim","source":a("/explicit/source/chi_sim.traineddata")},{"name":"eng","source":a("/explicit/source/eng.traineddata")}]})
}
fn observation() -> ProcessObservation {
    ProcessObservation {
        status: "completed".into(),
        exit_code: Some(0),
        stdout: b"literal".to_vec(),
        stderr: Vec::new(),
        report: json!({
 "status":"completed","observed_status":"completed","exit_code":0,"capture_complete":true,"streams_collected":2,
 "stdout":{"truncated":false,"bytes":7,"sha256":oe_core::sha256("literal")},"stderr":{"truncated":false,"bytes":0,"sha256":oe_core::sha256([])},
 "executable":{"unchanged":true},"borrowed_deadline":{"expired":false}}),
    }
}
#[test]
fn ocr_page5_selected_request_and_process_success_are_explicit() {
    let r = valid_request();
    assert!(parse_request5(&r).is_ok());
    for (k, v) in [
        ("schema", json!("oev2.knowledge-ocr-source-request/3")),
        ("producer_receipt", json!({})),
        ("languages", json!([])),
        ("process_observation", json!({})),
    ] {
        let mut changed = r.clone();
        changed[k] = v;
        assert!(parse_request5(&changed).is_err());
    }
    let mut reversed = r.clone();
    reversed["languages"].as_array_mut().unwrap().reverse();
    assert!(parse_request5(&reversed).is_err());
    let mut extra = r.clone();
    extra["tool"]["verified"] = json!(true);
    assert!(parse_request5(&extra).is_err());
    let mut conf = r.clone();
    conf["config"]["sha256"] = json!("a".repeat(64));
    assert!(parse_request5(&conf).is_err());
    assert!(process_completed5(&observation()).is_ok());
}
#[test]
fn public_process_guard_rejects_each_missing_success_conjunct_without_authority() {
    for status in [
        "timed_out",
        "process_failed",
        "cleanup_unverified",
        "deadline_exceeded",
        "executable_changed",
    ] {
        let mut o = observation();
        o.status = status.into();
        assert!(process_completed5(&o).is_err());
    }
    for (path, value) in [
        (vec!["capture_complete"], json!(false)),
        (vec!["streams_collected"], json!(1)),
        (vec!["observed_status"], json!("timed_out")),
        (vec!["status"], json!("deadline_exceeded")),
        (vec!["exit_code"], json!(7)),
        (vec!["stdout", "truncated"], json!(true)),
        (vec!["stderr", "truncated"], json!(true)),
        (vec!["executable", "unchanged"], json!(false)),
        (vec!["borrowed_deadline", "expired"], json!(true)),
        (vec!["stdout", "bytes"], json!(6)),
        (vec!["stdout", "sha256"], json!("a".repeat(64))),
    ] {
        let mut o = observation();
        let mut selected = &mut o.report;
        for k in path {
            selected = &mut selected[k];
        }
        *selected = value;
        assert!(process_completed5(&o).is_err());
    }
    let mut o = observation();
    o.exit_code = Some(7);
    assert!(process_completed5(&o).is_err());
    // Every value in this test is fabricated public data. None constructs the
    // private production PreparedOcrPage5 or certifies an invocation.
}

#[test]
fn actual_recorded_tool_identity_matches_selected_bytes_and_separate_paths() {
    use oe_contracts::KnowledgePageAsset4;
    use oe_knowledge::ocr_page5::process_selected_tool5;
    use std::path::Path;
    let selected = KnowledgePageAsset4 {
        path: "/explicit/source/tesseract".into(),
        sha256: "a".repeat(64),
        bytes: 86480,
    };
    let mut o = observation();
    o.report["executable"] = json!({"unchanged":true,"path":selected.path,"resolved":selected.path,"identity":{"bytes":selected.bytes,"sha256":selected.sha256}});
    assert!(
        process_selected_tool5(
            &o,
            &selected,
            Path::new(&selected.path),
            Path::new(&selected.path)
        )
        .is_ok()
    );
    for (key, value) in [("sha256", json!("b".repeat(64))), ("bytes", json!(86479))] {
        let mut changed = observation();
        changed.report = o.report.clone();
        changed.report["executable"]["identity"][key] = value;
        assert!(
            process_selected_tool5(
                &changed,
                &selected,
                Path::new(&selected.path),
                Path::new(&selected.path)
            )
            .is_err()
        );
    }
    for key in ["path", "resolved"] {
        let mut changed = observation();
        changed.report = o.report.clone();
        changed.report["executable"][key] = json!("/different/tesseract");
        assert!(
            process_selected_tool5(
                &changed,
                &selected,
                Path::new(&selected.path),
                Path::new(&selected.path)
            )
            .is_err()
        );
    }
    let mut declared = selected.clone();
    declared.path = "/explicit/./source/tesseract".into();
    o.report["executable"]["path"] = json!(declared.path);
    assert!(
        process_selected_tool5(
            &o,
            &declared,
            Path::new(&declared.path),
            Path::new(&selected.path)
        )
        .is_ok()
    );
    // This pure audit context does not execute or authorize filesystem aliases.
}
