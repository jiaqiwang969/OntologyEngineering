use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalPatternsPort, SemanticTemporalPatternsRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn request(actions: Value) -> Value {
    json!({"schema":"semantica.temporal-patterns-request/1","clock_at":"2024-01-20T00:00:00Z","initial_config":{},"actions":actions})
}
fn run(r: Value) -> Result<Value, SemanticRuntimeFailure> {
    RustSemanticRuntime
        .temporal_patterns(&serde_json::from_value::<SemanticTemporalPatternsRequest>(r).unwrap())
        .map(|v| serde_json::to_value(v).unwrap())
}
fn act(graph: Value, label: &str) -> Value {
    json!({"entry":"detector","arguments":{"graph":graph,"pattern_type":label,"min_frequency":1}})
}
#[test]
fn temporal_patterns_original79_histories98_actions() {
    let histories: Value =
        serde_json::from_str(include_str!("fixtures/temporal-patterns-original79.json")).unwrap();
    let histories = histories.as_array().unwrap();
    assert_eq!(histories.len(), 79);
    let mut count = 0;
    for h in histories {
        let out =
            run(h["request"].clone()).unwrap_or_else(|e| panic!("{}: {}", h["id"], e.message));
        assert_eq!(out["schema"], "semantica.temporal-patterns-result/1");
        assert_eq!(out["profile"], "semantica.source-temporal-patterns/1");
        assert_eq!(out["final_config"], h["final_config"], "{}", h["id"]);
        let steps = out["steps"].as_array().unwrap();
        assert_eq!(steps.len(), h["steps"].as_array().unwrap().len());
        for (i, (s, e)) in steps.iter().zip(h["steps"].as_array().unwrap()).enumerate() {
            count += 1;
            assert_eq!(s["index"], i);
            assert_eq!(s["entry"], h["request"]["actions"][i]["entry"]);
            for k in [
                "result",
                "config_after",
                "occurrence_relationship_indices",
                "clock_fallback_count",
            ] {
                assert_eq!(s[k], e[k], "{} step{i} {k}", h["id"]);
            }
            if e["source_error"].is_null() {
                assert!(s["failure"].is_null(), "{}: {}", h["id"], s["failure"]);
            } else {
                assert_eq!(s["failure"]["code"], "temporal_patterns_source_failure");
                assert_eq!(
                    s["failure"]["message"],
                    format!(
                        "{}: {}",
                        e["source_error"]["type"].as_str().unwrap(),
                        e["source_error"]["message"].as_str().unwrap()
                    ),
                    "{}",
                    h["id"]
                );
            }
        }
    }
    assert_eq!(count, 98);
}
#[test]
fn temporal_patterns_failure_state_and_resources() {
    let at = "2024-01-01";
    let loop_graph = json!({"relationships":[{"source":"A","target":"B","valid_from":at,"valid_until":at},{"source":"B","target":"A","valid_from":at,"valid_until":at}]});
    let mut looping = act(loop_graph, "sequence");
    looping["arguments"]["gap_tolerance"] = json!(1);
    let mut failure = act(json!({"relationships":[7]}), "sequence");
    failure["arguments"]["gap_tolerance"] = json!(3);
    let r = request(json!([looping, failure, act(json!({}), "trend")]));
    let out = run(r).unwrap();
    assert_eq!(
        out["steps"][0]["failure"]["code"],
        "temporal_patterns_resource_limit"
    );
    assert_eq!(
        out["steps"][0]["failure"]["message"],
        "More than65536 edge visits in action"
    );
    assert_eq!(out["steps"][0]["edge_visit_count"], 65536);
    assert!(out["steps"][0]["result"].is_null());
    assert_eq!(
        out["steps"][0]["occurrence_relationship_indices"],
        json!([])
    );
    assert_eq!(out["steps"][0]["config_after"], json!({"gap_tolerance":1}));
    assert_eq!(
        out["steps"][1]["failure"]["code"],
        "temporal_patterns_source_failure"
    );
    assert_eq!(out["steps"][2]["result"], json!([]));
    assert_eq!(out["final_config"], json!({"gap_tolerance":3}));
    let mut edges = vec![json!({"source":"A","target":"B","payload":"x".repeat(600000)})];
    edges.extend((0..30).map(|_| json!({"source":"B","target":"A"})));
    let out = run(request(json!([
        act(json!({"relationships":edges}), "cycle"),
        act(json!({}), "trend")
    ])))
    .unwrap();
    assert_eq!(
        out["steps"][0]["failure"]["code"],
        "temporal_patterns_resource_limit"
    );
    assert!(out["steps"][0]["result"].is_null());
    assert_eq!(out["steps"][1]["result"], json!([]));
    for n in [4096, 4097] {
        let out = run(request(json!([act(
            json!({"relationships":vec![json!({"source":"A","target":"B"});n]}),
            "sequence"
        )])))
        .unwrap();
        assert_eq!(out["steps"][0]["failure"].is_null(), n == 4096);
    }
    assert_eq!(
        run(request(json!(vec![act(json!({}), "trend"); 128]))).unwrap()["steps"]
            .as_array()
            .unwrap()
            .len(),
        128
    );
    assert!(run(request(json!(vec![act(json!({}), "trend"); 129]))).is_err());
    let mut r = request(json!([]));
    r["initial_config"] = json!({"opaque":"x".repeat(1024*1024)});
    assert!(run(r).is_err());
    let mut r = request(json!(vec![act(json!({}), "trend"); 128]));
    r["initial_config"] = json!({"opaque":"x".repeat(200000)});
    assert_eq!(
        run(r).unwrap_err().message,
        "Complete session output exceeds16MiB"
    );
    for v in [json!(7), json!("x".repeat(257))] {
        let out = run(request(json!([act(
            json!({"relationships":[{"source":"A","target":"B","valid_from":v}]}),
            "sequence"
        )])))
        .unwrap();
        assert_eq!(
            out["steps"][0]["failure"]["code"],
            "temporal_patterns_invalid"
        );
    }
}
#[test]
fn temporal_patterns_identity_and_request_binding() {
    let mut r = request(json!([act(
        json!({"relationships":[{"source":true,"target":1},{"source":1,"target":true}]}),
        "cycle"
    )]));
    let before = r.clone();
    let out = run(r.clone()).unwrap();
    assert_eq!(r, before);
    assert_eq!(
        out["steps"][0]["occurrence_relationship_indices"],
        json!([[[0], [1]]])
    );
    assert_eq!(out["steps"][0]["result"][0]["signature"], json!([true, 1]));
    assert!(out.get("adopted").is_none() && out.get("engineering_valid").is_none());
    let typed: SemanticTemporalPatternsRequest = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(
        out["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    r["actions"][0]["arguments"]["time_window"] = json!({"future_obligation":"unknown"});
    let changed = run(r.clone()).unwrap();
    assert_eq!(out["steps"], changed["steps"]);
    assert_ne!(out["request_sha256"], changed["request_sha256"]);
    assert_eq!(changed, run(r).unwrap());
    for v in [
        json!({"entry":"query","arguments":{"graph":{}}}),
        json!({"entry":"bad","arguments":{"graph":{}}}),
        json!({"entry":"query","arguments":{"graph":{},"pattern":"sequence","min_frequency":1}}),
    ] {
        assert!(run(request(json!([v]))).is_err());
    }
    let mut r = request(json!([]));
    r["extra_adoption"] = json!(true);
    assert!(serde_json::from_value::<SemanticTemporalPatternsRequest>(r).is_err());
}
