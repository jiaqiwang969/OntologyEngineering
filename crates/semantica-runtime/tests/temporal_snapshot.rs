use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalSnapshotPort, SemanticTemporalSnapshotRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn run(value: Value) -> Result<Value, SemanticRuntimeFailure> {
    let input: SemanticTemporalSnapshotRequest = serde_json::from_value(value).unwrap();
    RustSemanticRuntime
        .temporal_snapshot(&input)
        .map(|r| serde_json::to_value(r).unwrap())
}
fn request() -> Value {
    json!({"schema":"semantica.temporal-snapshot-request/1","operation":"reconstruct","clock_at":"2024-01-10T00:00:00Z","fact":null,"graph":{},"at_time":"2024-01-05T00:00:00Z","time_axis":"valid","granularity":"day","query":"","include_history":false,"temporal_precision":null})
}
#[test]
fn temporal_snapshot_original62_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/temporal-snapshot-original62.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 62);
    for case in cases {
        let got = run(case["request"].clone());
        if case["expected_error"] == true {
            assert!(got.is_err(), "{}", case["id"]);
            continue;
        }
        let got = got.unwrap_or_else(|e| panic!("{}:{}", case["id"], e.message));
        assert_eq!(got["schema"], "semantica.temporal-snapshot-result/1");
        assert_eq!(got["profile"], "semantica.source-temporal-snapshot/1");
        for (k, v) in case["expected"].as_object().unwrap() {
            assert_eq!(&got[k], v, "{}:{k}", case["id"]);
        }
    }
}
#[test]
fn temporal_snapshot_clock_identity_and_positions() {
    let mut r = request();
    r["time_axis"] = json!("transaction");
    r["graph"] =
        json!({"entities":[{"id":"A"},{"id":"A","note":"duplicate"}],"source":{"unverified":true}});
    let original = r.clone();
    let before = run(r.clone()).unwrap();
    assert_eq!(r, original);
    assert_eq!(before["entity_indices"], json!([]));
    assert_eq!(before["clock_fallback_count"], 2);
    assert_eq!(before, run(r.clone()).unwrap());
    r["clock_at"] = json!("2024-01-01T00:00:00Z");
    let after = run(r.clone()).unwrap();
    assert_eq!(after["entity_indices"], json!([1]));
    assert_eq!(
        after["snapshot"]["entities"],
        json!([{"id":"A","note":"duplicate"}])
    );
    assert_eq!(after["snapshot"]["source"], r["graph"]["source"]);
    assert_ne!(before["request_sha256"], after["request_sha256"]);
    let typed: SemanticTemporalSnapshotRequest = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(
        after["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    r["clock_at"] = json!("2024-01-01T08:00:00+08:00");
    let offset = run(r.clone()).unwrap();
    assert_eq!(after["snapshot"], offset["snapshot"]);
    assert_eq!(after["clock_at"], offset["clock_at"]);
    assert_ne!(after["request_sha256"], offset["request_sha256"]);
    r["graph"]["source"]["unverified"] = json!(false);
    let opaque = run(r).unwrap();
    assert_ne!(opaque["request_sha256"], offset["request_sha256"]);
}
#[test]
fn temporal_snapshot_guards_and_limits() {
    for i in 0..15 {
        let mut r = request();
        match i {
            0 => r["schema"] = json!("unknown"),
            1 => r["operation"] = json!("adopt"),
            2 => r["clock_at"] = json!("bad"),
            3 => r["fact"] = json!({}),
            4 => r["include_history"] = json!(true),
            5 => r["query"] = json!("unused"),
            6 => r["graph"]["entities"] = Value::Null,
            7 => r["graph"]["entities"] = json!([false]),
            8 => r["graph"]["entities"] = json!([{"id":{},"valid_from":"2024-01-01"}]),
            9 => r["graph"]["entities"] = json!([{"valid_from":0}]),
            10 => r["graph"]["entities"] = json!([{"recorded_at":false}]),
            11 => r["clock_at"] = json!("x".repeat(257)),
            12 => r["graph"]["entities"] = json!(vec![json!({}); 4097]),
            13 => r["graph"]["metadata"] = json!("x".repeat(1024 * 1024)),
            _ => r["graph"]["relationships"] = json!([{"superseded_at":["invalid"]}]),
        }
        assert!(run(r).is_err(), "guard{i}");
    }
    let mut r = request();
    r["approval"] = json!(true);
    assert!(serde_json::from_value::<SemanticTemporalSnapshotRequest>(r).is_err());
    let mut r = request();
    r.as_object_mut().unwrap().remove("clock_at");
    assert!(serde_json::from_value::<SemanticTemporalSnapshotRequest>(r).is_err());
    let mut r = request();
    r["graph"]["relationships"] = json!(vec![json!({}); 4096]);
    let out = run(r.clone()).unwrap();
    assert_eq!(out["relationship_indices"].as_array().unwrap().len(), 4096);
    assert_eq!(out["clock_fallback_count"], 4096);
    r["graph"]["entities"] = json!([{}]);
    assert!(run(r).is_err());
}
