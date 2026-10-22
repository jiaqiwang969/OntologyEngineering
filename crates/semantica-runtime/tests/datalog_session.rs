use semantica_contracts::{SemanticDatalogSessionPort, SemanticDatalogSessionRequest};
use semantica_runtime::{
    datalog::{ContextGraphData, DatalogReasoner},
    runtime::RustSemanticRuntime,
};
use serde_json::{Value, json};

fn input(actions: Value) -> Value {
    json!({"schema":"semantica.datalog-session-request/1","actions":actions,"on_error":"continue",
        "budget":{"max_iterations":1024,"max_match_attempts":10000000,"max_facts":100000,"max_query_rows":10000}})
}
fn run(value: Value) -> Result<Value, semantica_contracts::SemanticRuntimeFailure> {
    let input: SemanticDatalogSessionRequest = serde_json::from_value(value).unwrap();
    RustSemanticRuntime
        .datalog_session(&input)
        .map(|x| serde_json::to_value(x).unwrap())
}
fn normalize_state(s: &Value) -> Value {
    let mut facts: Vec<_> = s["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{}({})",
                f["predicate"].as_str().unwrap(),
                f["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    facts.sort();
    json!({"facts":facts,"rule_count":s["rule_count"],"derived":s["derived"]})
}
fn normalized(r: &Value) -> Value {
    let steps: Vec<_> = r["steps"].as_array().unwrap().iter().map(|s| {
        assert_eq!(s["failure"].is_null(), s["status"] != "failed");
        json!({"action_id":s["action_id"],"operation":s["operation"],"status":s["status"],
            "state_before":normalize_state(&s["state_before"]),"state_after":normalize_state(&s["state_after"]),
            "inserted":s["inserted"],"graph_facts_added":s["graph_facts_added"],"query_rows":s["query_rows"]})
    }).collect();
    json!({"steps":steps,"final_state":normalize_state(&r["final_state"]),"all_actions_succeeded":r["all_actions_succeeded"]})
}

#[test]
fn datalog_session_original22_histories() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/datalog-session-original22.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    assert_eq!(
        cases
            .iter()
            .map(|c| c["request"]["actions"].as_array().unwrap().len())
            .sum::<usize>(),
        94
    );
    for case in cases {
        let r = run(case["request"].clone())
            .unwrap_or_else(|e| panic!("{}: {}:{}", case["id"], e.code, e.message));
        assert_eq!(r["schema"], "semantica.datalog-session-result/1");
        assert_eq!(r["profile"], "semantica.bounded-source-datalog-session/1");
        assert_eq!(normalized(&r), case["expected"], "{}", case["id"]);
    }
}

#[test]
fn datalog_source_order_and_atomic_methods_remain_distinct() {
    let mut old = DatalogReasoner::new();
    old.add_fact("p(a)").unwrap();
    old.add_rule("q(X) :- p(X).").unwrap();
    assert!(old.query("invalid", None).is_err());
    assert!(!old.is_derived());
    assert_eq!(old.all_facts().len(), 1);
    assert!(old.query_source_order("invalid", None).is_err());
    assert!(old.is_derived());
    assert_eq!(old.all_facts().len(), 2);
    let graph = ContextGraphData {
        nodes: vec![],
        edges: vec![
            json!({"source":"a","target":"b","type":"edge"}),
            json!({"subject":"","predicate":"p","object":"x"}),
        ],
    };
    let mut atomic = DatalogReasoner::new();
    assert!(atomic.load_from_graph(&graph).is_err());
    assert!(atomic.all_facts().is_empty());
    assert!(atomic.load_from_graph_incremental(&graph).is_err());
    assert_eq!(atomic.all_facts().len(), 1);
    old.clear();
    assert!(old.all_facts().is_empty() && old.rules().is_empty());
    assert_eq!(old.indexed_predicates(), 0);
    assert!(!old.is_derived());
    assert!(old.last_stats().is_none());
}

#[test]
fn datalog_session_admission_and_identity_guards() {
    let sample = input(json!([
        {"action_id":"a","operation":"add_fact","fact":"p(a)"},
        {"action_id":"q","operation":"query","query":{"pattern":"p(?x)","bindings":{}}}
    ]));
    let base = run(sample.clone()).unwrap();
    let typed: SemanticDatalogSessionRequest = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(
        base["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    assert_eq!(base, run(sample.clone()).unwrap());
    for i in 0..3 {
        let mut altered = sample.clone();
        match i {
            0 => altered["on_error"] = json!("stop"),
            1 => altered["actions"][0]["action_id"] = json!("other"),
            _ => altered["budget"]["max_iterations"] = json!(512),
        }
        let r = run(altered).unwrap();
        assert_ne!(r["request_sha256"], base["request_sha256"]);
        assert_eq!(r["final_state"], base["final_state"]);
    }
    for i in 0..9 {
        let mut bad = sample.clone();
        match i {
            0 => bad["schema"] = json!("semantica.datalog-session-request/99"),
            1 => bad["on_error"] = json!("retry"),
            2 => bad["actions"][1]["action_id"] = json!("a"),
            3 => bad["actions"][0]["action_id"] = json!(" a"),
            4 => bad["actions"][0]["rule"] = json!("q(X) :- p(X)."),
            5 => bad["actions"][0]["fact"] = json!(42),
            6 => bad["actions"][0]["operation"] = json!("destroy"),
            7 => bad["budget"]["max_facts"] = json!(0),
            _ => bad["budget"]["max_match_attempts"] = json!(10000001),
        }
        assert!(run(bad).is_err(), "mutation{i}");
    }
    let mut bad = sample.clone();
    bad["actions"][0]["unrecognized"] = json!(true);
    assert!(serde_json::from_value::<SemanticDatalogSessionRequest>(bad).is_err());
    let too_many: Vec<_> = (0..33)
        .map(|i| json!({"action_id":format!("s{i}"),"operation":"snapshot"}))
        .collect();
    assert_eq!(
        run(input(json!(too_many))).unwrap_err().code,
        "datalog_session_action_budget"
    );
    let records = vec![json!({"unknown":1}); 4097];
    assert_eq!(
        run(input(
            json!([{"action_id":"g","operation":"load_graph","graph":{"edges":records,"nodes":[]}}])
        ))
        .unwrap_err()
        .code,
        "datalog_session_input_count_budget"
    );
    let invalid_graph =
        input(json!([{"action_id":"g","operation":"load_graph","graph":{"edges":[],"nodes":[1]}}]));
    assert_eq!(
        run(invalid_graph).unwrap_err().code,
        "datalog_graph_record_invalid"
    );
    let huge = input(
        json!([{"action_id":"f","operation":"add_fact","fact":{"unknown":"x".repeat(1024*1024)}}]),
    );
    assert_eq!(run(huge).unwrap_err().code, "datalog_input_byte_budget");
    let empty = run(input(json!([]))).unwrap();
    assert_eq!(empty["steps"], json!([]));
    assert_eq!(empty["final_state"]["facts"], json!([]));
    assert_eq!(empty["all_actions_succeeded"], true);
}

#[test]
fn datalog_session_resource_failures_preserve_actual_state() {
    let mut request = input(json!([
        {"action_id":"g","operation":"load_graph","graph":{"edges":[],"nodes":[{"id":"a","type":"p"},{"id":"b","type":"p"}]}},
        {"action_id":"q","operation":"query","query":{"pattern":"p(?x)","bindings":{}}},
        {"action_id":"c","operation":"clear"}
    ]));
    request["budget"]["max_facts"] = json!(1);
    let r = run(request).unwrap();
    assert_eq!(r["steps"][0]["status"], "failed");
    assert!(
        r["steps"][0]["failure"]["message"]
            .as_str()
            .unwrap()
            .contains("datalog_fact_budget")
    );
    assert_eq!(
        r["steps"][0]["state_after"]["facts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(r["steps"][1]["query_rows"], json!([{"x":"a"}]));
    assert_eq!(r["final_state"]["facts"], json!([]));
    let mut request = input(json!([
        {"action_id":"p","operation":"add_fact","fact":"p(a)"},
        {"action_id":"r","operation":"add_rule","rule":"q(X) :- p(X)."},
        {"action_id":"d","operation":"derive"},
        {"action_id":"s","operation":"snapshot"}
    ]));
    request["budget"]["max_iterations"] = json!(1);
    let r = run(request).unwrap();
    assert_eq!(r["steps"][2]["status"], "failed");
    assert_eq!(r["steps"][2]["state_before"], r["steps"][2]["state_after"]);
    assert_eq!(r["final_state"]["derived"], false);
    let mut request = input(json!([
        {"action_id":"a","operation":"add_fact","fact":"p(a)"},
        {"action_id":"b","operation":"add_fact","fact":"p(b)"},
        {"action_id":"r","operation":"add_rule","rule":"q(X) :- p(X)."},
        {"action_id":"q","operation":"query","query":{"pattern":"q(?x)","bindings":{}}},
        {"action_id":"one","operation":"query","query":{"pattern":"q(?x)","bindings":{"x":"a"}}}
    ]));
    request["budget"]["max_query_rows"] = json!(1);
    let r = run(request).unwrap();
    assert_eq!(r["steps"][3]["status"], "failed");
    assert_eq!(r["steps"][3]["state_after"]["derived"], true);
    assert_eq!(
        r["steps"][3]["state_after"]["facts"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(r["steps"][4]["query_rows"], json!([{"x":"a"}]));
    assert_eq!(r["all_actions_succeeded"], false);
}

#[test]
fn datalog_session_output_budget_is_whole_history() {
    let nodes: Vec<_> = (0..200)
        .map(|i| json!({"type":"item","id":format!("a{i:03}{}","x".repeat(3000))}))
        .collect();
    let mut actions = vec![
        json!({"action_id":"load","operation":"load_graph","graph":{"edges":[],"nodes":nodes}}),
    ];
    let one = run(input(json!(actions))).unwrap();
    assert_eq!(one["final_state"]["facts"].as_array().unwrap().len(), 200);
    for i in 0..20 {
        actions.push(json!({"action_id":format!("s{i}"),"operation":"snapshot"}));
    }
    let e = run(input(json!(actions))).unwrap_err();
    assert_eq!(e.code, "datalog_output_byte_budget");
    assert_eq!(e.phase, "result");
    assert!(
        run(input(json!([]))).unwrap()["final_state"]["facts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
