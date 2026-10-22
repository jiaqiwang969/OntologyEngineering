use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalVersionRecordRequest,
    SemanticTemporalVersionRecordsPort,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn request(operation: &str, arguments: Value) -> Value {
    json!({"schema":"semantica.temporal-version-records-request/1","operation":operation,"arguments":arguments,"clock_local":"2024-06-01T09:30:00.123456"})
}
fn run(value: Value) -> Result<Value, SemanticRuntimeFailure> {
    RustSemanticRuntime
        .temporal_version_records(
            &serde_json::from_value::<SemanticTemporalVersionRecordRequest>(value).unwrap(),
        )
        .map(|v| serde_json::to_value(v).unwrap())
}
#[test]
fn temporal_version_records_original102() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/temporal-version-records-original102.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 102);
    for case in cases {
        let out =
            run(case["request"].clone()).unwrap_or_else(|e| panic!("{}:{}", case["id"], e.message));
        let e = &case["expected"];
        assert_eq!(out["schema"], "semantica.temporal-version-records-result/1");
        assert_eq!(
            out["profile"],
            "semantica.source-temporal-version-records/1"
        );
        assert_eq!(out["operation"], case["request"]["operation"]);
        assert_eq!(out["clock_local"], case["request"]["clock_local"]);
        assert_eq!(out["result"], e["result"], "{}", case["id"]);
        assert_eq!(
            out["clock_fallback_count"], e["clock_fallback_count"],
            "{}",
            case["id"]
        );
        if e["source_error"].is_null() {
            assert!(
                out["failure"].is_null(),
                "{}:{}",
                case["id"],
                out["failure"]
            );
        } else {
            assert_eq!(
                out["failure"]["code"],
                "temporal_version_records_source_failure"
            );
            assert_eq!(
                out["failure"]["message"],
                format!(
                    "{}: {}",
                    e["source_error"]["type"].as_str().unwrap(),
                    e["source_error"]["message"].as_str().unwrap()
                ),
                "{}",
                case["id"]
            );
            assert_eq!(out["failure"]["retryable"], false);
        }
    }
}
#[test]
fn temporal_version_records_profile_and_resources() {
    let base = request("create_version", json!({"graph":{}}));
    for (key, value) in [
        ("schema", json!("unknown")),
        ("operation", json!("apply_revision")),
        ("arguments", json!({})),
        ("arguments", json!([])),
        ("clock_local", json!("2024-06-01T09:30:00Z")),
        ("clock_local", json!("2024-06-01T09:30:00.000000")),
        ("clock_local", json!("2024-06-01T09:30:00.1")),
        ("clock_local", json!("2024-02-30T09:30:00")),
        ("clock_local", json!("2024-06-01 09:30:00")),
        ("clock_local", json!("2024-06-01T09:30:60")),
        ("clock_local", json!("x".repeat(100000))),
    ] {
        let mut r = base.clone();
        r[key] = value;
        assert_eq!(
            run(r).unwrap_err().code,
            "temporal_version_records_invalid",
            "{key}"
        );
    }
    for op in ["validate_snapshot", "migrate_snapshot"] {
        assert!(run(request(op, json!({"snapshot":{},"extra":1}))).is_err());
    }
    let mut over = base.clone();
    over["arguments"]["ignored"] = json!("x".repeat(1024 * 1024));
    assert_eq!(
        run(over).unwrap_err().message,
        "Complete request exceeds1MiB"
    );
    // Byte admission counts every field, including options unused by the original.
    let mut boundary = base;
    boundary["arguments"]["ignored"] = json!("");
    let used = serde_json::to_vec(&boundary).unwrap().len();
    boundary["arguments"]["ignored"] = json!("x".repeat(1024 * 1024 - used));
    assert!(run(boundary.clone()).unwrap()["failure"].is_null());
    boundary["arguments"]["ignored"] = json!("x".repeat(1024 * 1024 - used + 1));
    assert!(run(boundary).is_err());
}
#[test]
fn temporal_version_records_copy_and_identity() {
    let mut input = request(
        "create_version",
        json!({"graph":{"entities":[{"nested":[1]}]},"metadata":{"condition":"unknown"}}),
    );
    let before = input.clone();
    let out = run(input.clone()).unwrap();
    assert_eq!(input, before);
    input["arguments"]["graph"]["entities"][0]["nested"][0] = json!(2);
    assert_eq!(out["result"]["entities"][0]["nested"][0], 1);
    assert_ne!(out["request_sha256"], run(input).unwrap()["request_sha256"]);
    let mut opaque = before.clone();
    opaque["arguments"]["unused"] = json!("bound");
    let changed = run(opaque).unwrap();
    assert_eq!(changed["result"], out["result"]);
    assert_ne!(changed["request_sha256"], out["request_sha256"]);
    let migrated = run(request("migrate_snapshot", json!({"snapshot":{}}))).unwrap();
    let checked = run(request(
        "validate_snapshot",
        json!({"snapshot":migrated["result"]}),
    ))
    .unwrap();
    assert_eq!(checked["result"], false);
    assert!(checked["failure"].is_null());
    let created = run(request(
        "validate_snapshot",
        json!({"snapshot":out["result"]}),
    ))
    .unwrap();
    assert_eq!(created["result"], false);
}

#[test]
fn temporal_version_records_original_integer_admission() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/temporal-version-records-integer-boundary.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let r: SemanticTemporalVersionRecordRequest =
            serde_json::from_str(case["request_raw"].as_str().unwrap()).unwrap();
        let got = RustSemanticRuntime.temporal_version_records(&r);
        if case["expected_admitted"] == true {
            let out = got.unwrap();
            assert!(out.failure.is_none());
            assert_eq!(
                out.result.unwrap(),
                case["expected_result"],
                "{}",
                case["id"]
            );
        } else {
            let failure = got.unwrap_err();
            assert_eq!(failure.code, "temporal_version_records_ingress_failure");
            assert_eq!(
                failure.message,
                format!(
                    "builtins.ValueError: {}",
                    case["expected_original_error"]["message"].as_str().unwrap()
                )
            );
        }
    }
}
