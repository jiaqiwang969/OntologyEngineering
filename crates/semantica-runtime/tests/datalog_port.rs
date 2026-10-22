use semantica_contracts::{SemanticDatalogPort, SemanticDatalogRequest};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};

fn sample() -> SemanticDatalogRequest {
    serde_json::from_value(json!({
        "schema":"semantica.datalog-request/1",
        "facts":["depends(drawing, assembly)","depends(assembly, part)"],
        "rules":["affected(X, Y) :- depends(X, Y).", "affected(X, Y) :- depends(X, Z), affected(Z, Y)."],
        "query":{"pattern":"affected(?object, part)","bindings":{}},
        "budget":{"max_iterations":1024,"max_match_attempts":10000000,"max_facts":100000,"max_query_rows":10000}
    })).unwrap()
}

#[test]
fn datalog_port_original18_observations() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/datalog-port-original18.json")).unwrap();
    assert_eq!(fixture["original_execution"], true);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let input: SemanticDatalogRequest =
            serde_json::from_value(case["request"].clone()).unwrap();
        let expected = &case["expected"];
        let result = RustSemanticRuntime.datalog(&input);
        if expected["outcome"] == "error" {
            let error = result.unwrap_err();
            let phase = if case["id"] == "bad-rule" {
                "rules"
            } else {
                "facts"
            };
            assert_eq!(error.phase, phase, "{}", case["id"]);
            assert!(!error.retryable);
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("{}: {}:{}", case["id"], e.code, e.message));
        let mut closure: Vec<_> = result
            .closure
            .iter()
            .map(|f| format!("{}({})", f.predicate, f.args.join(", ")))
            .collect();
        closure.sort();
        assert_eq!(json!(closure), expected["closure"], "{}", case["id"]);
        assert_eq!(json!(result.query_rows), expected["rows"], "{}", case["id"]);
        assert!(result.complete);
        assert_eq!(
            result.closure.len() as u64,
            result.seed_fact_count + result.derived_fact_count
        );
        assert_eq!(result.input_inserted.len(), input.facts.len());
        assert_eq!(
            result.input_inserted.iter().filter(|v| **v).count() as u64,
            result.seed_fact_count
        );
    }
}

#[test]
fn datalog_port_budget_and_identity_guards() {
    let original = sample();
    let result = RustSemanticRuntime.datalog(&original).unwrap();
    assert_eq!(result.profile, "semantica.positive-function-free-datalog/1");
    assert_eq!(result.seed_fact_count, 2);
    assert_eq!(result.derived_fact_count, 3);
    assert_eq!(
        json!(result.query_rows),
        json!([{"object":"assembly"},{"object":"drawing"}])
    );
    let hash = semantica_runtime::sha256(
        semantica_contracts::json::canonical_json(&serde_json::to_value(&original).unwrap())
            .unwrap(),
    );
    assert_eq!(result.request_sha256, hash);
    let mut changed = original.clone();
    changed.budget.max_iterations = 100;
    let revised = RustSemanticRuntime.datalog(&changed).unwrap();
    assert_ne!(result.request_sha256, revised.request_sha256);
    assert_eq!(json!(result.closure), json!(revised.closure));
    changed.facts.reverse();
    assert_ne!(
        revised.request_sha256,
        RustSemanticRuntime
            .datalog(&changed)
            .unwrap()
            .request_sha256
    );
    for (field, max) in [
        ("max_iterations", 1024),
        ("max_match_attempts", 10000000),
        ("max_facts", 100000),
        ("max_query_rows", 10000),
    ] {
        for value in [0, max + 1] {
            let mut wire = serde_json::to_value(&original).unwrap();
            wire["budget"][field] = json!(value);
            let input = serde_json::from_value(wire).unwrap();
            assert_eq!(
                RustSemanticRuntime.datalog(&input).unwrap_err().code,
                "datalog_invalid_budget"
            );
        }
    }
    let mut bad = original.clone();
    bad.schema = "semantica.datalog-request/2".into();
    assert_eq!(
        RustSemanticRuntime.datalog(&bad).unwrap_err().code,
        "invalid_contract"
    );
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["extra"] = json!(true);
    assert!(serde_json::from_value::<SemanticDatalogRequest>(wire).is_err());
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["budget"]["extra"] = json!(true);
    assert!(serde_json::from_value::<SemanticDatalogRequest>(wire).is_err());
}

#[test]
fn exhaustion_never_returns_a_partial_closure_or_query() {
    for (field, phase, detail) in [
        ("max_iterations", "derive", "datalog_iteration_budget"),
        ("max_match_attempts", "derive", "datalog_match_budget"),
        ("max_facts", "facts", "datalog_fact_budget"),
        ("max_query_rows", "query", "datalog_query_row_budget"),
    ] {
        let mut wire = serde_json::to_value(sample()).unwrap();
        wire["budget"][field] = json!(1);
        let input = serde_json::from_value(wire).unwrap();
        let error = RustSemanticRuntime.datalog(&input).unwrap_err();
        assert_eq!(error.phase, phase, "{field}: {}", error.message);
        assert!(error.message.contains(detail), "{field}: {}", error.message);
        // A later independent request succeeds; no retained partially derived state.
        assert_eq!(
            RustSemanticRuntime
                .datalog(&sample())
                .unwrap()
                .derived_fact_count,
            3
        );
    }
}

#[test]
fn invalid_fact_rule_query_and_count_boundaries_remain_explicit_errors() {
    for fact in [json!(true), json!(null), json!([]), json!(42)] {
        let mut input = sample();
        input.facts = vec![fact];
        assert_eq!(
            RustSemanticRuntime.datalog(&input).unwrap_err().code,
            "datalog_fact_kind_invalid"
        );
    }
    for rule in [
        "unsafe(X) :- edge(Y, Z).",
        "ok(X) :- edge(X, Y) ignored",
        "bad(f(X)) :- edge(X, Y).",
    ] {
        let mut input = sample();
        input.rules = vec![rule.into()];
        assert_eq!(
            RustSemanticRuntime.datalog(&input).unwrap_err().phase,
            "rules"
        );
    }
    let mut input = sample();
    input.facts = vec![json!({"subject":["a"],"predicate":"p","object":"b"})];
    assert!(
        RustSemanticRuntime
            .datalog(&input)
            .unwrap_err()
            .message
            .contains("datalog_dictionary_complex_term_unsupported")
    );
    let mut input = sample();
    input.query.as_mut().unwrap().pattern = "broken".into();
    assert_eq!(
        RustSemanticRuntime.datalog(&input).unwrap_err().phase,
        "query"
    );
    let mut input = sample();
    input.facts = vec![json!("a(b)"); 4097];
    assert_eq!(
        RustSemanticRuntime.datalog(&input).unwrap_err().code,
        "datalog_input_count_budget"
    );
    let mut input = sample();
    input.rules = vec!["a(X) :- b(X).".into(); 257];
    assert_eq!(
        RustSemanticRuntime.datalog(&input).unwrap_err().code,
        "datalog_input_count_budget"
    );
    let mut input = sample();
    input.facts = vec![json!("x".repeat(1024 * 1024))];
    assert_eq!(
        RustSemanticRuntime.datalog(&input).unwrap_err().code,
        "datalog_input_byte_budget"
    );
}

#[test]
fn complete_closure_over_output_bound_is_rejected_without_truncation() {
    let mut input = sample();
    input.query = None;
    input.facts = (0..48)
        .map(|n| json!(format!("node(n{n:02}{})", "x".repeat(3997))))
        .collect();
    input.rules = vec!["pair(X, Y) :- node(X), node(Y).".into()];
    let error = RustSemanticRuntime.datalog(&input).unwrap_err();
    assert_eq!(error.code, "datalog_output_byte_budget");
    assert_eq!(error.phase, "result");
}
