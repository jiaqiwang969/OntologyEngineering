use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalFactsPort, SemanticTemporalFactsRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn run(value: Value) -> Result<Value, SemanticRuntimeFailure> {
    let input: SemanticTemporalFactsRequest = serde_json::from_value(value).unwrap();
    RustSemanticRuntime
        .temporal_facts(&input)
        .map(|x| serde_json::to_value(x).unwrap())
}
fn request() -> Value {
    json!({"schema":"semantica.temporal-facts-request/1","operation":"timeline","graph":{"entities":[],"relationships":[]},"entity_id":"A","revision":null,"facts":[]})
}
#[test]
fn temporal_facts_original44_cases() {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/temporal-facts-original44.json")).unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 44);
    for case in cases {
        let got = run(case["request"].clone());
        if case["expected_error"] == true {
            assert!(got.is_err(), "{}", case["id"]);
            continue;
        }
        let got = got.unwrap_or_else(|e| panic!("{}:{}", case["id"], e.message));
        assert_eq!(got["schema"], "semantica.temporal-facts-result/1");
        assert_eq!(got["profile"], "semantica.source-temporal-facts/1");
        for (key, value) in case["expected"].as_object().unwrap() {
            assert_eq!(&got[key], value, "{}:{key}", case["id"]);
        }
    }
}
#[test]
fn temporal_facts_identity_positions_and_boundaries() {
    let mut r = request();
    r["graph"]["entities"] = json!([
      {"id":"A","valid_from":"2024-01-01T00:00:00.000001Z"},
      {"id":"A","valid_from":"2024-01-01T00:00:00Z"},
      {"id":"A","valid_from":"2024-01-01T00:00:00.000001Z"}
    ]);
    let before = r.clone();
    let out = run(r.clone()).unwrap();
    assert_eq!(r, before);
    assert_eq!(
        out["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["origin_index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 0, 2]
    );
    for e in out["events"].as_array().unwrap() {
        assert_eq!(
            e["fact"],
            r["graph"]["entities"][e["origin_index"].as_u64().unwrap() as usize]
        );
    }
    let typed: SemanticTemporalFactsRequest = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(
        out["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    r["graph"]["unconsumed_source_note"] = json!("current source condition");
    let second = run(r).unwrap();
    assert_eq!(out["events"], second["events"]);
    assert_ne!(out["request_sha256"], second["request_sha256"]);
    let r = json!({"schema":"semantica.temporal-facts-request/1","operation":"revision-impact","graph":null,"entity_id":null,"revision":{},"facts":[{},{}]});
    let one = run(r.clone()).unwrap();
    let two = run(r).unwrap();
    assert_eq!(one, two);
    assert_eq!(
        one["affected"],
        json!([{"index":0,"fact":{}},{"index":1,"fact":{}}])
    );
    let mut r = request();
    r["entity_id"] = json!("None");
    r["graph"]["relationships"] =
        json!([{"source":null,"target":{"ignored":"short-circuited"},"valid_from":"0001-01-01"}]);
    assert_eq!(
        run(r).unwrap()["events"][0]["timestamp"],
        "0001-01-01T00:00:00Z"
    );
}
#[test]
fn temporal_facts_guards_and_limits() {
    for i in 0..13 {
        let mut r = request();
        match i {
            0 => r["schema"] = json!("semantica.temporal-facts-request/99"),
            1 => r["operation"] = json!("adopt"),
            2 => r["revision"] = json!({}),
            3 => r["facts"] = json!([{}]),
            4 => r["entity_id"] = json!(["A"]),
            5 => r["graph"]["entities"] = Value::Null,
            6 => r["graph"]["entities"] = json!([false]),
            7 => r["graph"]["entities"] = json!([{"id":"A","valid_from":0}]),
            8 => r["graph"]["entities"] = json!([{"id":"A","valid_from":"x".repeat(257)}]),
            9 => r["graph"]["entities"] = json!(vec![json!({}); 4097]),
            10 => r["graph"]["irrelevant"] = json!("x".repeat(1024 * 1024)),
            11 => r["graph"]["entities"] = json!([{"id":{},"valid_from":"2024-01-01"}]),
            _ => r["graph"]["relationships"] = json!([{"source":"A","superseded_at":false}]),
        }
        assert!(run(r).is_err(), "{i}");
    }
    let mut r = request();
    r["approval"] = json!(true);
    assert!(serde_json::from_value::<SemanticTemporalFactsRequest>(r).is_err());
    let mut r = request();
    r["graph"]["entities"] = json!(vec![json!({}); 4096]);
    assert_eq!(run(r.clone()).unwrap()["events"], json!([]));
    r["graph"]["relationships"] = json!([{}]);
    assert!(run(r).is_err());
    let mut r = request();
    r["operation"] = json!("revision-impact");
    r["graph"] = Value::Null;
    r["entity_id"] = Value::Null;
    r["revision"] = json!({});
    r["facts"] = json!(vec![json!({}); 4097]);
    assert!(run(r).is_err());
}
