use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalConsistencyPort, SemanticTemporalConsistencyRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn request() -> Value {
    json!({"schema":"semantica.temporal-consistency-request/1","clock_at":"2024-01-10T00:00:00Z","graph":{},"granularity":"day"})
}
fn run(value: Value) -> Result<Value, SemanticRuntimeFailure> {
    let request: SemanticTemporalConsistencyRequest = serde_json::from_value(value).unwrap();
    RustSemanticRuntime
        .temporal_consistency(&request)
        .map(|v| serde_json::to_value(v).unwrap())
}
#[test]
fn temporal_consistency_original63_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/temporal-consistency-original63.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 63);
    for case in cases {
        let result = run(case["request"].clone());
        if case["expected_error"] == true {
            let error = result.unwrap_err();
            assert_eq!(
                error.code, "temporal_consistency_source_failure",
                "{}",
                case["id"]
            );
            assert_eq!(
                error.message,
                format!(
                    "{}: {}",
                    case["source_error"]["type"].as_str().unwrap(),
                    case["source_error"]["message"].as_str().unwrap()
                ),
                "{}",
                case["id"]
            );
        } else {
            let result = result.unwrap_or_else(|e| panic!("{}: {}", case["id"], e.message));
            assert_eq!(result["schema"], "semantica.temporal-consistency-result/1");
            assert_eq!(result["profile"], "semantica.source-temporal-consistency/1");
            for (key, value) in case["expected"].as_object().unwrap() {
                assert_eq!(&result[key], value, "{}:{key}", case["id"]);
            }
        }
    }
}
#[test]
fn temporal_consistency_identity_positions_and_unknowns() {
    let mut r = request();
    r["graph"] = json!({"entities":[{"id":"a"}],"relationships":[{"id":"duplicate","source":"a","target":"a"},{"id":"duplicate","source":"a","target":"a"}],"unknown_engineering_conditions":true});
    let original = r.clone();
    let out = run(r.clone()).unwrap();
    assert_eq!(r, original);
    assert_eq!(out["report"]["errors"], json!([]));
    assert_eq!(out["warning_relationship_indices"], json!([1]));
    assert_eq!(out["clock_fallback_count"], 4);
    assert_eq!(out["group_comparison_count"], 1);
    assert!(out.get("adopted").is_none() && out.get("engineering_valid").is_none());
    let typed: SemanticTemporalConsistencyRequest = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(
        out["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    r["graph"]["unknown_engineering_conditions"] = json!(false);
    let changed = run(r.clone()).unwrap();
    assert_ne!(out["request_sha256"], changed["request_sha256"]);
    assert_eq!(out["report"], changed["report"]);
    r["clock_at"] = json!("2025-01-10");
    let next = run(r.clone()).unwrap();
    assert_eq!(changed["report"], next["report"]);
    assert_ne!(changed["clock_at"], next["clock_at"]);
    assert_ne!(changed["request_sha256"], next["request_sha256"]);
    assert_eq!(next, run(r).unwrap());
}
#[test]
fn temporal_consistency_resources_and_failures() {
    for i in 0..13 {
        let mut r = request();
        match i {
            0 => r["schema"] = json!("unknown"),
            1 => r["clock_at"] = json!("bad"),
            2 => r["graph"] = Value::Null,
            3 => r["graph"]["relationships"] = Value::Null,
            4 => r["graph"]["entities"] = json!([false]),
            5 => r["graph"]["entities"] = json!([{"id":{}}]),
            6 => r["graph"]["relationships"] = json!([{"id":"r","source":[]}]),
            7 => r["graph"]["relationships"] = json!([{"id":"r","valid_from":7}]),
            8 => r["graph"]["relationships"] = json!([{"id":"r","valid_from":"x".repeat(257)}]),
            9 => r["graph"]["unused"] = json!("x".repeat(1024 * 1024)),
            10 => r["graph"]["entities"] = json!(vec![json!({"id":"a"}); 4097]),
            11 => {
                r["graph"]["relationships"] = json!(
                    (0..364)
                        .map(|i| json!({"id":"r","type":i}))
                        .collect::<Vec<_>>()
                )
            }
            _ => r["clock_at"] = json!("x".repeat(257)),
        }
        assert!(run(r).is_err(), "guard{i}");
    }
    let mut r = request();
    r["extra_adoption"] = json!(true);
    assert!(serde_json::from_value::<SemanticTemporalConsistencyRequest>(r).is_err());
    let mut r = request();
    r.as_object_mut().unwrap().remove("clock_at");
    assert!(serde_json::from_value::<SemanticTemporalConsistencyRequest>(r).is_err());
    let mut r = request();
    r["graph"] = json!({"entities":[{"id":"a"}],"relationships":vec![json!({"id":"r","source":"a","target":"a"});4095]});
    let out = run(r.clone()).unwrap();
    assert_eq!(out["report"]["warnings"].as_array().unwrap().len(), 4094);
    assert_eq!(out["group_comparison_count"], 4094);
    assert!(
        serde_json::to_vec_pretty(&out).unwrap().len() + 1
            <= semantica_runtime::temporal_consistency::MAX_OUTPUT_BYTES
    );
    r["graph"]["relationships"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"one-too-many"}));
    assert!(run(r).is_err());
}
