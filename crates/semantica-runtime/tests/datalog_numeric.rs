//! Literal finite-number expectations frozen before D68 and observed in oe.7.
//! These are symbolic Datalog terms, not arithmetic or engineering units.
use semantica_contracts::{SemanticDatalogGraphPort, SemanticDatalogPort};
use semantica_runtime::{
    datalog::{DatalogLimits, DatalogReasoner},
    runtime::RustSemanticRuntime,
};
use serde_json::{Value, json};

#[test]
fn datalog_numeric_original60_observations() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/datalog-numeric-original60.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 60);
    for case in cases {
        // Keep raw lexical float/integer distinctions through the public parser.
        let wire =
            semantica_json::strict_json(case["request_raw"].as_str().unwrap().as_bytes()).unwrap();
        let r = if case["command"] == "datalog" {
            serde_json::to_value(
                RustSemanticRuntime
                    .datalog(&serde_json::from_value(wire).unwrap())
                    .unwrap(),
            )
            .unwrap()
        } else {
            serde_json::to_value(
                RustSemanticRuntime
                    .datalog_graph(&serde_json::from_value(wire).unwrap())
                    .unwrap(),
            )
            .unwrap()
        };
        let expected = &case["expected"];
        let mut closure: Vec<_> = r["closure"]
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
                        .map(|v| v.as_str().unwrap())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        closure.sort();
        assert_eq!(json!(closure), expected["closure"], "{}", case["id"]);
        assert_eq!(r["query_rows"], expected["rows"], "{}", case["id"]);
        assert_eq!(
            r["seed_fact_count"], expected["seed_fact_count"],
            "{}",
            case["id"]
        );
        assert_eq!(
            r["graph_facts_added"], expected["graph_facts_added"],
            "{}",
            case["id"]
        );
        assert_eq!(r["complete"], true);
    }
}

#[test]
fn dictionary_numeric_roles_use_one_symbol_owner() {
    let mut engine = DatalogReasoner::new();
    for raw in [
        r#"{"subject":1e-6,"predicate":1e-5,"object":1e-7}"#,
        r#"{"source":1e-6,"type":"edge","target":1e-7}"#,
        r#"{"id":1e-5,"type":"number"}"#,
        r#"{"id":0.0,"type":"skipped"}"#,
        r#"{"source":-0.0,"target":"x","type":"skipped"}"#,
    ] {
        engine
            .add_fact(&serde_json::from_str::<Value>(raw).unwrap())
            .unwrap();
    }
    assert_eq!(
        engine.derive_all().unwrap(),
        vec!["1e-05(1e-06, 1e-07)", "edge(1e-06, 1e-07)", "number(1e-05)"]
    );
}

#[test]
fn numeric_terms_keep_finite_domain_and_existing_limits() {
    let mut engine = DatalogReasoner::with_limits(DatalogLimits {
        max_term_bytes: 4,
        ..Default::default()
    })
    .unwrap();
    assert!(
        engine
            .add_fact(&json!({"subject":"a","predicate":"v","object":0.00001}))
            .unwrap_err()
            .to_string()
            .contains("datalog_term_byte_budget")
    );
    assert!(engine.all_facts().is_empty());
    let mut engine = DatalogReasoner::new();
    for term in [json!([]), json!({"n":1})] {
        assert!(
            engine
                .add_fact(&json!({"subject":"a","predicate":"v","object":term}))
                .unwrap_err()
                .to_string()
                .contains("datalog_dictionary_complex_term_unsupported")
        );
    }
    assert!(engine.all_facts().is_empty());
    for raw in [b"1e400".as_slice(), b"NaN", b"Infinity"] {
        assert!(semantica_json::strict_json(raw).is_err());
    }
}
