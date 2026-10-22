use super::*;
use serde_json::json;
use std::sync::{Arc, atomic::AtomicBool};

fn fixture() -> Value {
    serde_json::from_str(crate::private_test_fixtures::text_ref!("release-inputs-03/crates/semantica-runtime/tests/fixtures/source-session-helpers-original.json"))
    .unwrap()
}
fn query(
    s: &mut DatasetSession,
    text: &str,
    bindings: BTreeMap<String, Value>,
    cache: bool,
) -> anyhow::Result<semantica_contracts::SemanticSessionQueryResult> {
    s.query_source_internal(text, &BTreeMap::new(), &bindings, cache, None)
}
fn raw(v: semantica_contracts::SemanticSessionQueryResult) -> Value {
    let mut result = v.result;
    result["revision"] = json!(v.revision);
    result["cache_hit"] = json!(v.cache_hit);
    result
}
#[test]
fn source_session_original74_helpers_cache_and_mutations_are_received() {
    let f = fixture();
    assert_eq!(f["original_observations"]["candidate_results_used"], false);
    let actions = f["original_plan"]["actions"].as_array().unwrap();
    let observations = f["original_observations"]["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 74);
    assert_eq!(observations.len(), 74);
    let mut session = DatasetSession::new();
    let mut failed = Vec::new();
    for (action, expected) in actions.iter().zip(observations) {
        let id = action["id"].as_str().unwrap();
        assert_eq!(action["id"], expected["id"]);
        let op = action["operation"].as_str().unwrap();
        let result = (|| -> anyhow::Result<Value> {
            Ok(match op {
                "new" => {
                    assert_eq!(action["profile"], "rdf");
                    session = DatasetSession::new_source_internal(
                        action["cache_queries"].as_bool().unwrap(),
                    );
                    json!({"profile":"rdf","cache_queries":session.source_cache_enabled.unwrap()})
                }
                "load" => serde_json::to_value(session.load_rdflib(
                    action["data"].as_str().unwrap().as_bytes(),
                    RdfFormat::NTriples,
                    None,
                )?)?,
                "clear" => serde_json::to_value(session.clear_rdflib(None)?)?,
                _ => {
                    let ns = serde_json::from_value(action["namespaces"].clone())?;
                    let bindings = serde_json::from_value(action["bindings"].clone())?;
                    let text = action["query"].as_str().unwrap();
                    let cache = action["use_cache"].as_bool().unwrap();
                    raw(if op == "query" {
                        session.query_source_internal(text, &ns, &bindings, cache, None)?
                    } else {
                        session.helper_source_internal(op, text, &ns, &bindings, cache)?
                    })
                }
            })
        })();
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if expected["status"] == "completed" {
                assert_eq!(
                    result.as_ref().unwrap_or_else(|e| panic!("{id}: {e:#}")),
                    &expected["raw"],
                    "{id}"
                );
            } else {
                let error = result
                    .as_ref()
                    .err()
                    .unwrap_or_else(|| panic!("{id}: unexpectedly succeeded"));
                let text = format!("{error:#}");
                if expected["error"]["type"] == "UnsupportedQueryTypeError"
                    || action["query"] == " "
                {
                    assert!(
                        text.contains(expected["error"]["message"].as_str().unwrap()),
                        "{id}: {text}"
                    );
                } else {
                    // Backend parser diagnostics remain native. Preserve the raw
                    // Python failure separately; never synthesize its traceback.
                    assert!(matches!(op, "load" | "query"), "{id}");
                    assert!(!text.is_empty());
                }
            }
            assert_eq!(
                json!(session.revision()),
                expected["revision"],
                "{id}: revision"
            );
            assert_eq!(
                json!(session.query_cache.len()),
                expected["cache_entries"],
                "{id}: cache"
            );
        }));
        println!(
            "OE_SOURCE_SESSION_CASE {id} {}",
            if checked.is_ok() { "ok" } else { "failed" }
        );
        if let Err(error) = &result {
            println!(
                "OE_SOURCE_SESSION_DIAGNOSTIC {id} {}",
                serde_json::to_string(&format!("{error:#}")).unwrap()
            );
        }
        if checked.is_err() {
            failed.push(id);
        }
    }
    assert!(
        failed.is_empty(),
        "original source session cases: {failed:?}"
    );
}

#[test]
fn source_session_namespaces_and_source_binding_identity_use_actual_parser() {
    let mut s = DatasetSession::new_source_internal(true);
    s.load_rdflib(b"<urn:s> <urn:p> \"plain\" .", RdfFormat::NTriples, None)
        .unwrap();
    let ns = BTreeMap::from([("ex".into(), "urn:".into())]);
    let q = "SELECT ?v WHERE {ex:s ex:p ?v}";
    let r = s
        .query_source_internal(q, &ns, &BTreeMap::new(), true, None)
        .unwrap();
    assert_eq!(
        r.result["rows"][0]["values"][0][1],
        json!({"value":"plain","term_type":"literal","datatype":null,"language":null})
    );
    assert!(
        s.query_source_internal(q, &ns, &BTreeMap::new(), true, None)
            .unwrap()
            .cache_hit
    );
    let local = "PREFIX ex:<urn:absent:> SELECT ?v WHERE {ex:s ex:p ?v}";
    assert_eq!(
        s.query_source_internal(local, &ns, &BTreeMap::new(), true, None)
            .unwrap()
            .result["rows"],
        json!([])
    );
    let b = BTreeMap::from([(
        "v".into(),
        json!({"term_type":"literal","value":"测量","language":"zh-CN","datatype":null}),
    )]);
    let r = query(&mut s, "SELECT ?v WHERE {}", b, true).unwrap();
    assert_eq!(r.result["rows"][0]["values"][0][1]["language"], "zh-CN");
}

#[test]
fn source_session_legacy_cache_domain_and_mutation_owner_stay_shared() {
    let mut s = DatasetSession::new_source_internal(true);
    let q = "SELECT ?v WHERE {BIND(\"x\" AS ?v)}";
    let native = s.query(q, true).unwrap();
    assert!(!native.cache_hit);
    let source = query(&mut s, q, BTreeMap::new(), true).unwrap();
    assert!(!source.cache_hit);
    assert_ne!(native.result, source.result);
    assert_eq!(s.query_cache.len(), 2);
    assert!(s.query(q, true).unwrap().cache_hit);
    assert!(query(&mut s, q, BTreeMap::new(), true).unwrap().cache_hit);
    let revision = s.revision();
    assert!(
        s.load_rdflib(b"not RDF", RdfFormat::NTriples, None)
            .is_err()
    );
    assert_eq!(s.revision(), revision);
    assert_eq!(s.query_cache.len(), 2);
    s.load_rdflib(b"<urn:s> <urn:p> <urn:o> .", RdfFormat::NTriples, None)
        .unwrap();
    assert_eq!(s.revision(), revision + 1);
    assert!(s.query_cache.is_empty());
    assert_eq!(s.cache_bytes, 0);
}

#[test]
fn source_session_projection_and_admission_are_checked_before_cache_hits() {
    let q = "SELECT ?v WHERE {BIND(1 AS ?v)}";
    assert!(query(&mut DatasetSession::new(), q, BTreeMap::new(), true).is_err());
    let mut s = DatasetSession::new_source_internal(true);
    query(&mut s, q, BTreeMap::new(), true).unwrap();
    let count = s.query_cache.len();
    s.rdflib_projection.invalidate();
    assert!(
        query(&mut s, q, BTreeMap::new(), true)
            .unwrap_err()
            .to_string()
            .contains("projection_unavailable")
    );
    assert_eq!(s.query_cache.len(), count);
    let mut named = DatasetSession::new_source_internal(true);
    named
        .load_rdflib(
            b"<urn:s> <urn:p> <urn:o> <urn:g> .",
            RdfFormat::NQuads,
            None,
        )
        .unwrap();
    assert!(
        query(&mut named, q, BTreeMap::new(), true)
            .unwrap_err()
            .to_string()
            .contains("named_dataset_not_received")
    );
}

#[test]
fn source_session_cancel_invalid_parameters_and_result_limit_leave_state_intact() {
    let mut s = DatasetSession::new_source_internal(true);
    let q = "SELECT ?v WHERE {}";
    let b = BTreeMap::from([("v".into(), json!("small"))]);
    query(&mut s, q, b.clone(), true).unwrap();
    assert!(
        s.query_source_internal(
            q,
            &BTreeMap::new(),
            &b,
            true,
            Some(Arc::new(AtomicBool::new(true)))
        )
        .is_err()
    );
    assert!(
        query(
            &mut s,
            q,
            BTreeMap::from([(
                "v".into(),
                json!({"value":"x","term_type":"literal","extra":true})
            )]),
            true
        )
        .is_err()
    );
    assert!(
        query(
            &mut s,
            q,
            (0..129).map(|i| (format!("v{i}"), json!("x"))).collect(),
            true
        )
        .is_err()
    );
    assert_eq!(s.query_cache.len(), 1);
    assert_eq!(s.revision(), 0);
    let large_query = format!("SELECT ?v WHERE {{VALUES ?x {{{}}}}}", "1 ".repeat(5000));
    let error = query(
        &mut s,
        &large_query,
        BTreeMap::from([("v".into(), json!("x".repeat(4000)))]),
        true,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("source_runtime_result_byte_budget"));
    assert_eq!(s.query_cache.len(), 1);
    assert_eq!(s.revision(), 0);
}

#[test]
fn source_session_cache_budget_and_disabled_configuration_are_enforced() {
    let mut s = DatasetSession::new_source_internal(true);
    for i in 0..270 {
        query(
            &mut s,
            "SELECT ?v WHERE {}",
            BTreeMap::from([("v".into(), json!(i.to_string()))]),
            true,
        )
        .unwrap();
    }
    assert_eq!(s.query_cache.len(), 256);
    let total: usize = s
        .query_cache
        .iter()
        .map(|(k, v)| k.len() + serde_json::to_vec(v).unwrap().len())
        .sum();
    assert_eq!(total, s.cache_bytes);
    assert!(total <= crate::MAX_BYTES);
    let mut disabled = DatasetSession::new_source_internal(false);
    for _ in 0..2 {
        assert!(
            !query(&mut disabled, "ASK {}", BTreeMap::new(), true)
                .unwrap()
                .cache_hit
        );
    }
    assert!(disabled.query_cache.is_empty());
    assert_eq!(disabled.cache_bytes, 0);
}

#[test]
fn source_session_query_failure_keeps_cache_but_helper_type_error_can_seed_it() {
    let mut s = DatasetSession::new_source_internal(true);
    assert!(
        s.helper_source_internal("select", "ASK {}", &BTreeMap::new(), &BTreeMap::new(), true)
            .is_err()
    );
    assert_eq!(s.query_cache.len(), 1);
    assert!(
        query(&mut s, "ASK {}", BTreeMap::new(), true)
            .unwrap()
            .cache_hit
    );
    for text in ["DESCRIBE <urn:s>", "SELECT WHERE {}", ""] {
        assert!(query(&mut s, text, BTreeMap::new(), true).is_err());
        assert_eq!(s.query_cache.len(), 1);
    }
}

#[test]
fn source_session_remote_and_dataset_scope_are_explicit_refusals() {
    let mut s = DatasetSession::new_source_internal(true);
    for query_text in [
        "SELECT * WHERE {SERVICE <https://invalid.example/sparql> {?s ?p ?o}}",
        "SELECT * WHERE {SERVICE SILENT <https://invalid.example/sparql> {?s ?p ?o}}",
        "SELECT * FROM <urn:g> WHERE {?s ?p ?o}",
        "ASK FROM NAMED <urn:g> {}",
        "CONSTRUCT {?s ?p ?o} FROM <urn:g> WHERE {?s ?p ?o}",
        "SELECT * FROM NAMED <urn:g> WHERE {GRAPH <urn:g> {?s ?p ?o}}",
    ] {
        let error = query(&mut s, query_text, BTreeMap::new(), true).expect_err(query_text);
        if query_text.contains(" FROM ") {
            assert!(
                error
                    .to_string()
                    .contains("source_query_dataset_clause_not_received")
            );
        }
        assert!(s.query_cache.is_empty());
        assert_eq!(s.revision(), 0);
    }
}
