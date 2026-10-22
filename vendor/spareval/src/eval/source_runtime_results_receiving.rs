//! Independent original SemanticRuntime results; no candidate-produced oracle.
use super::*;
use crate::dataset::source_memory::SourceMemoryGraph;
use crate::numeric::RawExpressionValue;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn graph(session: &Rc<QueryNumericSession>) -> SourceMemoryGraph {
    let mut graph = SourceMemoryGraph::new(Rc::clone(session), None).unwrap();
    for (predicate, text, datatype, language, iri) in [
        ("plain", "plain", None, None, false),
        (
            "typed",
            "typed",
            Some("http://www.w3.org/2001/XMLSchema#string"),
            None,
            false,
        ),
        ("lang", "测量", None, Some("zh-CN"), false),
        (
            "num",
            "01",
            Some("http://www.w3.org/2001/XMLSchema#integer"),
            None,
            false,
        ),
        ("link", "urn:o", None, None, true),
    ] {
        let node = |text: &str| {
            RawExpressionValue::from_import(oxrdf::NamedNode::new(text).unwrap().into())
                .ensure_source_object()
        };
        let object = if iri {
            node(text)
        } else {
            session
                .construct_source_direct_string(
                    text,
                    datatype.map(|d| oxrdf::NamedNode::new(d).unwrap()),
                    language.map(str::to_owned),
                    true,
                )
                .unwrap()
        };
        let s = graph.admit_source(node("urn:s")).unwrap();
        let p = graph
            .admit_source(node(&format!("urn:{predicate}")))
            .unwrap();
        let o = graph.admit_source(object).unwrap();
        graph.insert([s, p, o]).unwrap();
    }
    graph
}
fn received(first: usize, last: usize) {
    let fixture: Value = serde_json::from_str(crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_runtime_results_original.json"
    ))
    .unwrap();
    assert_eq!(fixture["candidate_results_used"], false);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 31);
    let mut failures = Vec::new();
    for case in &fixture["cases"].as_array().unwrap()[first - 1..last] {
        let id = case["input"]["id"].as_str().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let session = source_bgp_receiving::session();
            let graph = graph(&session);
            let actual = numeric_query::prepare_source_runtime_query(
                case["input"]["query"].as_str().unwrap(),
                graph,
                session,
                Vec::new(),
            )
            .map_err(|e| e.to_string())
            .and_then(|p| p.execute_source_runtime_result().map_err(|e| e.to_string()));
            if case["status"] == "original_error" {
                let error = actual.unwrap_err();
                assert!(
                    error.contains(case["error"]["message"].as_str().unwrap()),
                    "{id}: {error}"
                );
            } else {
                let actual = actual.unwrap_or_else(|e| panic!("{id}: {e}"));
                let mut expected = case["result"].clone();
                expected.as_object_mut().unwrap().remove("revision");
                expected.as_object_mut().unwrap().remove("cache_hit");
                if case["input"]["blank_comparison"] == "per-result-graph-isomorphism" {
                    let triples = actual["triples"].as_array().unwrap();
                    assert_eq!(triples.len(), 4);
                    let ids: BTreeSet<_> = triples
                        .iter()
                        .map(|t| t["subject"]["value"].as_str().unwrap())
                        .collect();
                    assert_eq!(ids.len(), 2);
                    for id in ids {
                        let members: Vec<_> = triples
                            .iter()
                            .filter(|t| t["subject"]["value"] == id)
                            .collect();
                        assert_eq!(members.len(), 2);
                        assert!(members.iter().any(|t| t["predicate"]["value"] == "urn:self"
                            && t["object"]["term_type"] == "blank_node"
                            && t["object"]["value"] == id));
                        assert!(members.iter().any(|t| t["predicate"]["value"] == "urn:p"
                            && [json!("1"), json!("2")].contains(&t["object"]["value"])));
                    }
                    let mut values: Vec<_> = triples
                        .iter()
                        .filter(|t| t["predicate"]["value"] == "urn:p")
                        .map(|t| t["object"].clone())
                        .collect();
                    let mut wanted: Vec<_> = expected["triples"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|t| t["predicate"]["value"] == "urn:p")
                        .map(|t| t["object"].clone())
                        .collect();
                    values.sort_by_key(Value::to_string);
                    wanted.sort_by_key(Value::to_string);
                    assert_eq!(values, wanted);
                } else {
                    assert_eq!(actual, expected, "{id}");
                }
            }
        }));
        println!(
            "OE_SOURCE_RESULT_CASE {id} {}",
            if result.is_ok() { "ok" } else { "failed" }
        );
        if result.is_err() {
            failures.push(id);
        }
    }
    assert!(
        failures.is_empty(),
        "original source query results: {failures:?}"
    );
}
#[test]
fn source_runtime_results_select_dto_preserves_domains_and_terms() {
    received(1, 9);
}
#[test]
fn source_runtime_results_ask_uses_actual_first_solution() {
    received(10, 17);
}
#[test]
fn source_runtime_results_construct_preserves_terms_templates_and_graph_identity() {
    received(18, 30);
}
#[test]
fn source_runtime_results_describe_remains_original_refusal() {
    received(31, 31);
}

#[test]
fn source_runtime_results_initial_bindings_preserve_source_terms_and_scope() {
    use crate::numeric::{NumericInitialBinding, NumericInitialValue};
    let fixture: Value = serde_json::from_str(crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_runtime_initial_original.json"
    ))
    .unwrap();
    assert_eq!(fixture["candidate_results_used"], false);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 8);
    for case in fixture["cases"].as_array().unwrap() {
        let session = source_bgp_receiving::session();
        let graph = SourceMemoryGraph::new(Rc::clone(&session), None).unwrap();
        let bindings = case["input"]["bindings"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, v)| NumericInitialBinding {
                variable: Variable::new(name).unwrap(),
                value: match v["term_type"].as_str().unwrap() {
                    "iri" => NumericInitialValue::UnknownNormalized(
                        oxrdf::NamedNode::new(v["value"].as_str().unwrap())
                            .unwrap()
                            .into(),
                    ),
                    "blank_node" => NumericInitialValue::UnknownNormalized(
                        oxrdf::BlankNode::new(v["value"].as_str().unwrap())
                            .unwrap()
                            .into(),
                    ),
                    "literal" => NumericInitialValue::SourceDirectString {
                        lexical: v["value"].as_str().unwrap().into(),
                        datatype: v["datatype"]
                            .as_str()
                            .map(|v| oxrdf::NamedNode::new(v).unwrap()),
                        language: v["language"].as_str().map(str::to_owned),
                        normalize: true,
                    },
                    _ => panic!("original binding kind"),
                },
            })
            .collect();
        let actual = numeric_query::prepare_source_runtime_query(
            case["input"]["query"].as_str().unwrap(),
            graph,
            session,
            bindings,
        )
        .unwrap()
        .execute_source_runtime_result()
        .unwrap();
        let mut expected = case["result"].clone();
        expected.as_object_mut().unwrap().remove("revision");
        expected.as_object_mut().unwrap().remove("cache_hit");
        assert_eq!(actual, expected, "{}", case["input"]["id"]);
        println!(
            "OE_SOURCE_RESULT_CASE {} ok",
            case["input"]["id"].as_str().unwrap()
        );
    }
}

#[test]
fn source_runtime_results_admission_preserves_old_select_and_owner_boundaries() {
    let session = source_bgp_receiving::session();
    let old = numeric_query::prepare_source_graph_numeric_query(
        "SELECT * WHERE {}",
        graph(&session),
        Rc::clone(&session),
        vec![],
    )
    .unwrap();
    assert!(
        old.execute_source_runtime_result()
            .unwrap_err()
            .to_string()
            .contains("admission required")
    );
    assert!(
        numeric_query::prepare_source_graph_numeric_query(
            "ASK {}",
            graph(&session),
            Rc::clone(&session),
            vec![]
        )
        .is_err()
    );
    let ask = numeric_query::prepare_source_runtime_query(
        "ASK {}",
        graph(&session),
        Rc::clone(&session),
        vec![],
    )
    .unwrap();
    assert!(
        ask.execute_source_presence()
            .err()
            .unwrap()
            .to_string()
            .contains("requires a SELECT")
    );
    assert!(
        numeric_query::prepare_source_runtime_query(
            "SELECT * WHERE {}",
            graph(&session),
            source_bgp_receiving::session(),
            vec![]
        )
        .is_err()
    );
}

#[test]
fn source_runtime_results_cancellation_and_actual_row_limit_are_enforced() {
    for query in [
        "SELECT * WHERE {}",
        "ASK {}",
        "CONSTRUCT {<urn:s> <urn:p> <urn:o>} WHERE {}",
    ] {
        let session = source_bgp_receiving::session();
        let token = Arc::new(AtomicBool::new(false));
        let graph = SourceMemoryGraph::new(Rc::clone(&session), Some(Arc::clone(&token))).unwrap();
        let prepared =
            numeric_query::prepare_source_runtime_query(query, graph, session, vec![]).unwrap();
        token.store(true, atomic::Ordering::Relaxed);
        assert!(matches!(
            prepared.execute_source_runtime_result(),
            Err(QueryEvaluationError::Cancelled)
        ));
    }
    let session = source_bgp_receiving::session();
    let q = format!("SELECT ?v WHERE {{VALUES ?v {{{}}}}}", "1 ".repeat(10_001));
    let error = numeric_query::prepare_source_runtime_query(&q, graph(&session), session, vec![])
        .unwrap()
        .execute_source_runtime_result()
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source_runtime_result_row_budget")
    );
}
