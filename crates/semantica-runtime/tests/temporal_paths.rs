use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalPathsPort, SemanticTemporalPathsRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn request() -> Value {
    json!({"schema":"semantica.temporal-paths-request/1","clock_at":"2024-01-10T00:00:00Z","graph":{},"source":"A","target":"T","start_time":null,"end_time":null,"max_path_length":null,"temporal_constraints":null,"enforce_causal_ordering":true,"ordering_strategy":"strict","granularity":"day"})
}
fn run(value: Value) -> Result<Value, SemanticRuntimeFailure> {
    let request: SemanticTemporalPathsRequest = serde_json::from_value(value).unwrap();
    RustSemanticRuntime
        .temporal_paths(&request)
        .map(|v| serde_json::to_value(v).unwrap())
}
#[test]
fn temporal_paths_original70_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/temporal-paths-original70.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 70);
    for case in cases {
        let result = run(case["request"].clone());
        if case["expected_error"] == true {
            let error = result.unwrap_err();
            assert_eq!(
                error.code, "temporal_paths_source_failure",
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
            assert_eq!(result["schema"], "semantica.temporal-paths-result/1");
            assert_eq!(result["profile"], "semantica.source-temporal-paths/1");
            for (key, value) in case["expected"].as_object().unwrap() {
                assert_eq!(&result[key], value, "{}:{key}", case["id"]);
            }
        }
    }
}
#[test]
fn temporal_paths_identity_positions_and_unknowns() {
    let mut r = request();
    r["graph"] = json!({"relationships":[{"id":"same","source":"A","target":"B"},{"id":"same","source":"B","target":"T"},{"id":"same","source":"B","target":"T"}],"unknown_engineering_conditions":true});
    let original = r.clone();
    let out = run(r.clone()).unwrap();
    assert_eq!(r, original);
    assert_eq!(out["path_relationship_indices"], json!([[0, 1], [0, 2]]));
    assert_eq!(out["clock_fallback_count"], 4);
    assert_eq!(out["edge_expansion_count"], 3);
    assert_eq!(out["queued_path_count"], 4);
    assert!(out.get("engineering_valid").is_none() && out.get("adopted").is_none());
    let typed: SemanticTemporalPathsRequest = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(
        out["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    r["graph"]["unknown_engineering_conditions"] = json!(false);
    r["temporal_constraints"] = json!({"require_causal_proof":true});
    let changed = run(r.clone()).unwrap();
    assert_eq!(out["report"], changed["report"]);
    assert_ne!(out["request_sha256"], changed["request_sha256"]);
    r["clock_at"] = json!("2025-01-10");
    let next = run(r.clone()).unwrap();
    assert_eq!(changed["report"], next["report"]);
    assert_ne!(changed["clock_at"], next["clock_at"]);
    assert_ne!(changed["request_sha256"], next["request_sha256"]);
    assert_eq!(next, run(r).unwrap());
}
#[test]
fn temporal_paths_resources_and_failures() {
    for i in 0..11 {
        let mut r = request();
        match i {
            0 => r["schema"] = json!("unknown"),
            1 => r["clock_at"] = json!("bad"),
            2 => r["graph"] = Value::Null,
            3 => r["graph"]["relationships"] = Value::Null,
            4 => r["graph"]["relationships"] = json!([false]),
            5 => {
                r["graph"]["relationships"] = json!([{"source":"A","target":"T","valid_from":7}]);
                r["start_time"] = json!("2024-01-01");
            }
            6 => {
                r["graph"]["relationships"] =
                    json!([{"source":"A","target":"T","valid_from":"x".repeat(257)}]);
                r["start_time"] = json!("2024-01-01");
            }
            7 => r["graph"]["unused"] = json!("x".repeat(1024 * 1024)),
            8 => {
                r["graph"]["relationships"] = json!(vec![json!({"source":"A","target":"T"}); 4097])
            }
            9 => r["clock_at"] = json!("x".repeat(257)),
            _ => {
                let mut edges =
                    vec![json!({"source":"A","target":"B","payload":"x".repeat(600000)})];
                edges.extend((0..30).map(|_| json!({"source":"B","target":"T"})));
                r["graph"]["relationships"] = json!(edges);
            }
        }
        assert!(run(r).is_err(), "guard{i}");
    }
    let mut r = request();
    r["extra_adoption"] = json!(true);
    assert!(serde_json::from_value::<SemanticTemporalPathsRequest>(r).is_err());
    let mut r = request();
    r.as_object_mut().unwrap().remove("clock_at");
    assert!(serde_json::from_value::<SemanticTemporalPathsRequest>(r).is_err());
    let mut r = request();
    r["graph"]["relationships"] = json!(vec![json!({"source":"A","target":"T"}); 4096]);
    assert_eq!(run(r).unwrap()["report"]["num_paths"], 4096);
    // Closed form: initial cell + sum(2*n+1,n=1..1023) = 1048576.
    let mut r = request();
    r["source"] = json!(0);
    r["target"] = json!(1023);
    r["graph"]["relationships"] = json!(
        (0..1023)
            .map(|i| json!({"source":i,"target":i+1}))
            .collect::<Vec<_>>()
    );
    let out = run(r.clone()).unwrap();
    assert_eq!(out["report"]["paths"][0]["length"], 1023);
    assert_eq!(out["clock_fallback_count"], 2044);
    r["target"] = json!(1024);
    r["graph"]["relationships"]
        .as_array_mut()
        .unwrap()
        .push(json!({"source":1023,"target":1024}));
    assert_eq!(
        run(r).unwrap_err().message,
        "More than1048576 cumulative queued path cells"
    );
    let mut r = request();
    r["source"] = json!(0);
    r["graph"]["relationships"] = json!(
        (1..4097)
            .map(|i| json!({"source":0,"target":i}))
            .collect::<Vec<_>>()
    );
    let star = run(r).unwrap();
    assert_eq!(star["report"]["num_paths"], 0);
    assert_eq!(star["edge_expansion_count"], 4096);
    assert_eq!(star["identity_comparison_count"], 8_407_041);
}
