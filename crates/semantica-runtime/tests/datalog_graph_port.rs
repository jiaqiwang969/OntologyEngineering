use semantica_contracts::{SemanticDatalogGraphPort, SemanticDatalogGraphRequest};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};

fn sample() -> SemanticDatalogGraphRequest {
    serde_json::from_value(json!({
        "schema":"semantica.datalog-graph-request/1",
        "graph":{"nodes":[],"edges":[
            {"source":"drawing","target":"assembly","type":"depends"},
            {"source":"assembly","target":"part","type":"depends"}]},
        "facts":[],
        "rules":["affected(X, Y) :- depends(X, Y).", "affected(X, Y) :- depends(X, Z), affected(Z, Y)."],
        "query":{"pattern":"affected(?object, part)","bindings":{}},
        "budget":{"max_iterations":1024,"max_match_attempts":10000000,"max_facts":100000,"max_query_rows":10000}
    })).unwrap()
}

#[test]
fn datalog_graph_original18_observations() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/datalog-graph-original18.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let input: SemanticDatalogGraphRequest =
            serde_json::from_value(case["request"].clone()).unwrap();
        let expected = &case["expected"];
        let result = RustSemanticRuntime.datalog_graph(&input);
        if expected["outcome"] == "error" {
            let error = result.unwrap_err();
            assert_eq!(
                error.phase,
                if case["id"] == "graph-empty-argument" {
                    "graph"
                } else {
                    "rules"
                }
            );
            assert!(!error.retryable);
            continue;
        }
        let r = result.unwrap_or_else(|e| panic!("{}: {}:{}", case["id"], e.code, e.message));
        let mut closure: Vec<_> = r
            .closure
            .iter()
            .map(|f| format!("{}({})", f.predicate, f.args.join(", ")))
            .collect();
        closure.sort();
        assert_eq!(json!(closure), expected["closure"], "{}", case["id"]);
        assert_eq!(json!(r.query_rows), expected["rows"], "{}", case["id"]);
        assert_eq!(
            json!(r.graph_facts_added),
            expected["graph_facts_added"],
            "{}",
            case["id"]
        );
        assert_eq!(r.graph_node_count as usize, input.graph.nodes.len());
        assert_eq!(r.graph_edge_count as usize, input.graph.edges.len());
        assert_eq!(
            r.input_inserted.iter().filter(|v| **v).count() as u64 + r.graph_facts_added,
            r.seed_fact_count
        );
        assert_eq!(
            r.closure.len() as u64,
            r.seed_fact_count + r.derived_fact_count
        );
        assert!(r.complete);
    }
}

#[test]
fn datalog_graph_identity_and_failure_guards() {
    let input = sample();
    let r = RustSemanticRuntime.datalog_graph(&input).unwrap();
    assert_eq!(
        r.request_sha256,
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(&input).unwrap())
                .unwrap()
        )
    );
    for change in 0..3 {
        let mut x = input.clone();
        match change {
            0 => x.graph.edges.reverse(),
            1 => x
                .graph
                .nodes
                .push(json!({"unrecognized":"original retained"})),
            _ => x.budget.max_iterations = 100,
        }
        let y = RustSemanticRuntime.datalog_graph(&x).unwrap();
        assert_ne!(r.request_sha256, y.request_sha256);
        assert_eq!(json!(r.closure), json!(y.closure));
    }
    for (field, phase, fragment) in [
        ("max_facts", "graph", "datalog_fact_budget"),
        ("max_iterations", "derive", "datalog_iteration_budget"),
        ("max_match_attempts", "derive", "datalog_match_budget"),
        ("max_query_rows", "query", "datalog_query_row_budget"),
    ] {
        let mut wire = serde_json::to_value(&input).unwrap();
        wire["budget"][field] = json!(1);
        let error = RustSemanticRuntime
            .datalog_graph(&serde_json::from_value(wire).unwrap())
            .unwrap_err();
        assert_eq!(error.phase, phase);
        assert!(error.message.contains(fragment));
        assert_eq!(
            RustSemanticRuntime
                .datalog_graph(&input)
                .unwrap()
                .graph_facts_added,
            2
        );
    }
}

#[test]
fn graph_record_kinds_and_complete_combined_budgets_are_checked() {
    for bad in [
        json!(null),
        json!(42),
        json!(true),
        json!([]),
        json!("part(a)"),
    ] {
        let mut x = sample();
        x.graph.nodes.push(bad);
        assert_eq!(
            RustSemanticRuntime.datalog_graph(&x).unwrap_err().code,
            "datalog_graph_record_invalid"
        );
    }
    let mut x = sample();
    x.facts = vec![json!("part(a)"); 4094];
    assert!(RustSemanticRuntime.datalog_graph(&x).is_ok());
    x.graph.nodes.push(json!({"unrecognized":true}));
    assert_eq!(
        RustSemanticRuntime.datalog_graph(&x).unwrap_err().code,
        "datalog_input_count_budget"
    );
    let mut x = sample();
    x.graph.nodes = vec![json!({}); 4095];
    assert_eq!(
        RustSemanticRuntime.datalog_graph(&x).unwrap_err().code,
        "datalog_input_count_budget"
    );
    let mut x = sample();
    x.graph
        .nodes
        .push(json!({"unrecognized":"x".repeat(1024*1024)}));
    assert_eq!(
        RustSemanticRuntime.datalog_graph(&x).unwrap_err().code,
        "datalog_input_byte_budget"
    );
    let mut x = sample();
    x.graph.nodes.push(json!({"id":["a"],"type":"part"}));
    assert!(
        RustSemanticRuntime
            .datalog_graph(&x)
            .unwrap_err()
            .message
            .contains("datalog_dictionary_complex_term_unsupported")
    );
}

#[test]
fn graph_wire_is_explicit_and_does_not_accept_old_schema_or_unknown_fields() {
    let mut x = sample();
    x.schema = "semantica.datalog-request/1".into();
    assert_eq!(
        RustSemanticRuntime.datalog_graph(&x).unwrap_err().code,
        "invalid_contract"
    );
    for place in ["root", "graph"] {
        let mut x = serde_json::to_value(sample()).unwrap();
        if place == "root" {
            x["extra"] = json!(true)
        } else {
            x["graph"]["handle"] = json!("private")
        };
        assert!(serde_json::from_value::<SemanticDatalogGraphRequest>(x).is_err());
    }
    for (field, max) in [
        ("max_iterations", 1024),
        ("max_match_attempts", 10000000),
        ("max_facts", 100000),
        ("max_query_rows", 10000),
    ] {
        for value in [0, max + 1] {
            let mut x = serde_json::to_value(sample()).unwrap();
            x["budget"][field] = json!(value);
            assert_eq!(
                RustSemanticRuntime
                    .datalog_graph(&serde_json::from_value(x).unwrap())
                    .unwrap_err()
                    .code,
                "datalog_invalid_budget"
            );
        }
    }
}

#[test]
fn graph_output_overflow_never_returns_truncated_success() {
    let mut x = sample();
    x.facts.clear();
    x.query = None;
    x.graph.edges.clear();
    x.graph.nodes = (0..48)
        .map(|i| json!({"id":format!("n{i:02}{}","x".repeat(3997)),"type":"node"}))
        .collect();
    x.rules = vec!["pair(X, Y) :- node(X), node(Y).".into()];
    assert_eq!(
        RustSemanticRuntime.datalog_graph(&x).unwrap_err().code,
        "datalog_output_byte_budget"
    );
}
