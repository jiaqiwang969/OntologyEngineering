use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalRangePort, SemanticTemporalRangeRequest,
    SemanticTemporalSnapshotPort, SemanticTemporalSnapshotRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn run(v: Value) -> Result<Value, SemanticRuntimeFailure> {
    let r: SemanticTemporalRangeRequest = serde_json::from_value(v).unwrap();
    RustSemanticRuntime
        .temporal_range(&r)
        .map(|v| serde_json::to_value(v).unwrap())
}
fn request() -> Value {
    json!({"schema":"semantica.temporal-range-request/1","clock_at":"2024-01-10T00:00:00Z","graph":{},"start_time":"2024-01-03","end_time":"2024-01-05","time_axis":"valid","granularity":"day","query":"","aggregation":"union","include_intervals":true})
}
#[test]
fn temporal_range_original67_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/temporal-range-original67.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 67);
    for case in cases {
        let got = run(case["request"].clone());
        if case["expected_error"] == true {
            assert!(got.is_err(), "{}", case["id"]);
            continue;
        }
        let got = got.unwrap_or_else(|e| panic!("{}:{}", case["id"], e.message));
        assert_eq!(got["schema"], "semantica.temporal-range-result/1");
        assert_eq!(got["profile"], "semantica.source-temporal-range/1");
        for (k, v) in case["expected"].as_object().unwrap() {
            assert_eq!(&got[k], v, "{}:{k}", case["id"]);
        }
    }
}
#[test]
fn temporal_range_identity_positions_and_boundary_modes() {
    let mut r = request();
    r["graph"] = json!({"entities":"ignored source data","relationships":[{"id":"r","valid_until":"2024-01-03"},{"id":"r","valid_until":"2024-01-03"}],"unverified":true});
    let original = r.clone();
    let union = run(r.clone()).unwrap();
    assert_eq!(r, original);
    assert_eq!(union["relationship_indices"], json!([0, 1]));
    assert_eq!(union["clock_fallback_count"], 2);
    let point:SemanticTemporalSnapshotRequest=serde_json::from_value(json!({"schema":"semantica.temporal-snapshot-request/1","operation":"reconstruct","clock_at":r["clock_at"],"fact":null,"graph":{"relationships":r["graph"]["relationships"]},"at_time":"2024-01-03","time_axis":"valid","granularity":"day","query":"","include_history":false,"temporal_precision":null})).unwrap();
    assert!(
        RustSemanticRuntime
            .temporal_snapshot(&point)
            .unwrap()
            .relationship_indices
            .is_empty()
    );
    let typed: SemanticTemporalRangeRequest = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(
        union["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    r["include_intervals"] = json!(false);
    let unused = run(r.clone()).unwrap();
    assert_eq!(union["query_result"], unused["query_result"]);
    assert_ne!(union["request_sha256"], unused["request_sha256"]);
    r["graph"]["unverified"] = json!(false);
    assert_ne!(
        unused["request_sha256"],
        run(r.clone()).unwrap()["request_sha256"]
    );
    r["aggregation"] = json!("intersection");
    assert_eq!(run(r.clone()).unwrap()["relationship_indices"], json!([]));
    r["graph"] = json!({"relationships":[{}]});
    r["time_axis"] = json!("transaction");
    r["aggregation"] = json!("union");
    assert_eq!(run(r.clone()).unwrap()["relationship_indices"], json!([]));
    r["clock_at"] = json!("2024-01-01");
    let earlier = run(r.clone()).unwrap();
    assert_eq!(earlier["relationship_indices"], json!([0]));
    assert_eq!(earlier, run(r).unwrap());
}
#[test]
fn temporal_range_resources_and_refusals() {
    for i in 0..11 {
        let mut r = request();
        match i {
            0 => r["schema"] = json!("unknown"),
            1 => r["clock_at"] = json!("bad"),
            2 => r["graph"] = Value::Null,
            3 => r["graph"]["relationships"] = Value::Null,
            4 => r["graph"]["relationships"] = json!([false]),
            5 => r["graph"]["relationships"] = json!([{"recorded_at":7}]),
            6 => r["clock_at"] = json!("x".repeat(257)),
            7 => r["graph"]["relationships"] = json!(vec![json!({}); 4097]),
            8 => r["graph"]["unused"] = json!("x".repeat(1024 * 1024)),
            9 => {
                r["aggregation"] = json!("evolution");
                r["end_time"] = json!("2040-01-01");
            }
            _ => {
                r["aggregation"] = json!("evolution");
                r["graph"]["relationships"] = json!(vec![json!({}); 4096]);
                r["end_time"] = json!("2024-01-19");
            }
        }
        assert!(run(r).is_err(), "guard{i}");
    }
    let mut r = request();
    r["adopt"] = json!(true);
    assert!(serde_json::from_value::<SemanticTemporalRangeRequest>(r).is_err());
    let mut r = request();
    r.as_object_mut().unwrap().remove("clock_at");
    assert!(serde_json::from_value::<SemanticTemporalRangeRequest>(r).is_err());
    let mut r = request();
    r["graph"]["relationships"] = json!(vec![json!({}); 4096]);
    assert_eq!(
        run(r.clone()).unwrap()["relationship_indices"]
            .as_array()
            .unwrap()
            .len(),
        4096
    );
    r["aggregation"] = json!("evolution");
    r["end_time"] = json!("2024-01-18");
    assert_eq!(run(r.clone()).unwrap()["clock_fallback_count"], 69632); // 4096 filtering + 65536 bucket checks.
    r["end_time"] = json!("2024-01-19");
    assert!(run(r).is_err());
    let mut r = request();
    r["aggregation"] = json!("evolution");
    r["end_time"] = json!("2026-01-01");
    r["graph"]["relationships"] = json!([{"payload":"x".repeat(32768)}]);
    let e = run(r).unwrap_err();
    assert!(e.message.contains("payload exceeds16MiB"));
}
