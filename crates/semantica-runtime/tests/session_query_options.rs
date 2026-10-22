//! Original public wrapper observations, frozen before the session API change.
use oxigraph::{
    io::RdfFormat,
    model::{Literal, NamedNode, Term, Triple},
};
use semantica_runtime::dataset::DatasetSession;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn term(v: &Value) -> Term {
    match v["term_type"].as_str().unwrap() {
        "iri" => NamedNode::new(v["value"].as_str().unwrap()).unwrap().into(),
        "literal" => {
            let text = v["value"].as_str().unwrap();
            if let Some(lang) = v["language"].as_str() {
                Literal::new_language_tagged_literal(text, lang)
                    .unwrap()
                    .into()
            } else {
                // The frozen comparison has no plain-string RDF1.0/1.1 collapse.
                Literal::new_typed_literal(
                    text,
                    NamedNode::new(v["datatype"].as_str().unwrap()).unwrap(),
                )
                .into()
            }
        }
        _ => panic!("term outside independently frozen comparison domain"),
    }
}
fn expected_result(raw: &Value) -> Value {
    match raw["query_type"].as_str().unwrap() {
        "SELECT" => {
            let rows: Vec<Value> = raw["rows"].as_array().unwrap().iter().map(|r| {
                let values = r["values"].as_array().unwrap().iter().map(|pair| {
                    // Compare the declared RDF1.1 projection, including language
                    // datatype; original raw DTO bytes remain in the fixture.
                    let t=&pair[1];
                    let value=if t["term_type"]=="iri" {json!({"kind":"iri","value":t["value"]})}
                        else {json!({"kind":"literal","value":t["value"],"language":t["language"],"datatype":if t["language"].is_string(){json!("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString")}else{t["datatype"].clone()}})};
                    (pair[0].as_str().unwrap().to_owned(), value)
                }).collect();
                Value::Object(values)
            }).collect();
            json!({"kind":"select","columns":raw["variables"],"row_count":rows.len(),"rows":rows})
        }
        "ASK" => json!({"kind":"ask","value":raw["boolean"]}),
        "CONSTRUCT" => {
            let mut triples: Vec<_> = raw["triples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    Triple::new(
                        NamedNode::new(t["subject"]["value"].as_str().unwrap()).unwrap(),
                        NamedNode::new(t["predicate"]["value"].as_str().unwrap()).unwrap(),
                        term(&t["object"]),
                    )
                    .to_string()
                })
                .collect();
            triples.sort();
            json!({"kind":"graph","triples":triples})
        }
        _ => panic!("undeclared original result type"),
    }
}
#[test]
fn original_wrapper_option_cache_revision_sequence_is_received() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/session-query-options-original.json")).unwrap();
    let actions = fixture["actions"].as_array().unwrap();
    let observations = fixture["observations"].as_array().unwrap();
    assert_eq!(actions.len(), 34);
    assert_eq!(observations.len(), 34);
    let mut session = DatasetSession::new();
    for (action, expected) in actions.iter().zip(observations) {
        assert_eq!(action["id"], expected["id"]);
        let id = action["id"].as_str().unwrap();
        match action["operation"].as_str().unwrap() {
            "query" => {
                let ns = serde_json::from_value(action["namespaces"].clone()).unwrap();
                let bindings = action["bindings"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), term(v)))
                    .collect();
                let actual = session.query_with_options(
                    action["query"].as_str().unwrap(),
                    &ns,
                    &bindings,
                    action["use_cache"].as_bool().unwrap(),
                );
                if expected["status"] == "original_error" {
                    assert!(actual.is_err(), "{id}");
                } else {
                    let actual = actual.unwrap_or_else(|e| panic!("{id}: {e}"));
                    assert_eq!(
                        actual.revision,
                        expected["revision"].as_u64().unwrap(),
                        "{id}"
                    );
                    assert_eq!(
                        actual.cache_hit,
                        expected["raw"]["cache_hit"].as_bool().unwrap(),
                        "{id}"
                    );
                    assert_eq!(actual.result, expected_result(&expected["raw"]), "{id}");
                }
            }
            operation => {
                let actual = if operation == "clear" {
                    session.clear(None)
                } else {
                    session.load(
                        action["data"].as_str().unwrap().as_bytes(),
                        RdfFormat::NTriples,
                        None,
                        None,
                    )
                };
                if expected["status"] == "original_error" {
                    assert!(actual.is_err(), "{id}");
                } else {
                    let actual = serde_json::to_value(actual.unwrap()).unwrap();
                    for key in ["added", "removed", "quad_count", "revision"] {
                        assert_eq!(actual[key], expected["raw"][key], "{id} {key}");
                    }
                }
            }
        }
        assert_eq!(
            session.revision(),
            expected["revision"].as_u64().unwrap(),
            "{id}"
        );
        println!("OE_SESSION_QUERY_CASE {id} ok");
    }
}

#[test]
fn empty_options_share_legacy_cache_and_returned_values_are_isolated() {
    let mut s = DatasetSession::new();
    let q = "ASK {}";
    let empty = BTreeMap::new();
    let first = s.query(q, true).unwrap();
    assert!(!first.cache_hit);
    let mut next = s
        .query_with_options(q, &empty, &BTreeMap::new(), true)
        .unwrap();
    assert!(next.cache_hit);
    next.result["value"] = false.into();
    assert_eq!(s.query(q, true).unwrap().result["value"], true);
    let mut copy = s.clone();
    copy.clear(None).unwrap();
    assert!(!copy.query(q, true).unwrap().cache_hit);
    assert!(s.query(q, true).unwrap().cache_hit);
    assert_eq!(s.revision(), 0);
}

#[test]
fn failed_option_queries_preserve_dataset_and_prior_cache() {
    let mut s = DatasetSession::new();
    let q = "ASK {}";
    s.query(q, true).unwrap();
    let before = s.snapshot().unwrap();
    let ns = BTreeMap::from([("ex".into(), "urn:".into())]);
    for query in [
        "ASK { SERVICE <https://example.invalid/> {?s ?p ?o}}",
        "ASK FROM <https://example.invalid/> {?s ?p ?o}",
        "SELECT (1 AS ?s) WHERE {}",
    ] {
        let b = BTreeMap::from([("s".into(), NamedNode::new("urn:a").unwrap().into())]);
        assert!(s.query_with_options(query, &ns, &b, true).is_err());
        assert!(s.query(q, true).unwrap().cache_hit);
    }
    let bad = BTreeMap::from([("ex".into(), "relative iri".into())]);
    assert!(
        s.query_with_options(q, &bad, &BTreeMap::new(), true)
            .is_err()
    );
    assert_eq!(s.snapshot().unwrap().dataset_sha256, before.dataset_sha256);
    assert_eq!(s.revision(), 0);
}

#[test]
fn parameter_limits_and_no_cache_requests_do_not_seed_entries() {
    let mut s = DatasetSession::new();
    let ns = BTreeMap::from([("ex".into(), "urn:".into())]);
    let b = BTreeMap::new();
    let q = "ASK {}";
    assert!(!s.query_with_options(q, &ns, &b, false).unwrap().cache_hit);
    assert!(!s.query_with_options(q, &ns, &b, true).unwrap().cache_hit);
    assert!(s.query_with_options(q, &ns, &b, true).unwrap().cache_hit);
    let too_many = (0..129).map(|n| (format!("p{n}"), "urn:".into())).collect();
    assert!(
        s.query_with_options(q, &too_many, &b, true)
            .unwrap_err()
            .to_string()
            .contains("query_parameter_count_budget")
    );
    assert!(
        s.query_with_options(&" ".repeat(1024 * 1024 + 1), &ns, &b, true)
            .unwrap_err()
            .to_string()
            .contains("query_byte_budget")
    );
    assert!(s.query_with_options(q, &ns, &b, true).unwrap().cache_hit);
}
