//! Complete original Variable wrapper actions, including actual state readbacks.
use super::source_failure::SourceFailure;
use super::*;
use serde_json::{Value, json};
use spareval::{SourceNodeObservation, SourceTermInput};
fn private_original() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_original.json"
    )
}
fn private_plan() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_plan.json"
    )
}
fn encode(v: SourceNodeObservation<'_>) -> Value {
    let node = super::source_update::node(v.term);
    let (kind, text, datatype, language) = match v.term {
        SourceTermInput::Variable(v) => ("Variable", v, None, None),
        SourceTermInput::Iri(v) => ("URIRef", v, None, None),
        SourceTermInput::Blank(v) => ("BNode", v, None, None),
        SourceTermInput::Literal {
            value,
            datatype,
            language,
        } => ("Literal", value, datatype, language),
    };
    let mut out =
        json!({"kind":kind,"text":text,"n3":node.n3(),"hash":v.hash,"repr":v.repr,"truth":v.truth});
    if kind == "Variable" {
        out["python"] = json!(node.n3());
    }
    if kind == "Literal" {
        out["datatype"] = json!(datatype);
        out["language"] = json!(language);
    }
    out
}
fn run(id: &str) {
    assert_eq!(
        crate::sha256(private_original()),
        "04897e1b5d024b7ef47b07caebde42a9445bbca3b99120678ff2f8474cd324c1"
    );
    assert_eq!(
        crate::sha256(private_plan()),
        "001e10de65e41964d10f9b88731a26dfe111bd83a66543f5c911a1fd8b4fb211"
    );
    let p: Value = serde_json::from_str(private_plan()).unwrap();
    let o: Value = serde_json::from_str(private_original()).unwrap();
    let plan = p["runtime_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap();
    let original = o["runtime"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap();
    let actions = plan["actions"].as_array().unwrap();
    let expected = original["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 21);
    assert_eq!(expected.len(), 21);
    let mut s = DatasetSession::new_source_data_internal(true);
    let mut failures = vec![];
    for (action, expected) in actions.iter().zip(expected) {
        let result = (|| -> anyhow::Result<Value> {
            Ok(match action["op"].as_str().unwrap() {
                "update" => {
                    let r = s.update_rdflib(action["query"].as_str().unwrap())?;
                    json!({"added":r.added,"removed":r.removed,"quad_count":r.quad_count,"revision":r.revision})
                }
                "query" => {
                    let r = s.query_source_internal(
                        action["query"].as_str().unwrap(),
                        &BTreeMap::new(),
                        &BTreeMap::new(),
                        true,
                        None,
                    )?;
                    let mut v = r.result;
                    v["revision"] = json!(r.revision);
                    v["cache_hit"] = json!(r.cache_hit);
                    v
                }
                "snapshot" => serde_json::to_value(s.snapshot_rdflib("2000-01-01T00:00:00Z")?)?,
                _ => panic!("unreceived original action"),
            })
        })();
        let outcome = match result {
            Ok(value) => json!({"status":"completed","value":value}),
            Err(error) => match error.downcast_ref::<SourceFailure>() {
                Some(e) => {
                    json!({"status":"original_error","error":{"module":e.module,"type":e.class,"message":e.to_string()}})
                }
                None => json!({"status":"unreceived_Rust_error","diagnostic":format!("{error:#}")}),
            },
        };
        let mut quads = vec![];
        let mut contexts = vec![];
        s.rdflib_projection
            .source_state
            .as_mut()
            .unwrap()
            .visit_observations(
                |q| {
                    quads.push(q.into_iter().map(encode).collect::<Vec<_>>());
                    Ok(())
                },
                |c| {
                    contexts.push(encode(c));
                    Ok(())
                },
            )
            .unwrap();
        let actual = json!({"id":expected["id"],"op":action["op"],"result":outcome,"revision":s.revision(),"quad_count":s.quad_count(),"query_cache_entries":s.query_cache.len(),"snapshot_cached":s.rdflib_snapshot_cache.get().is_some(),"actual_quads":quads,"actual_contexts":contexts});
        let matched = actual == *expected;
        println!(
            "OE_SOURCE_RUNTIME_DATA {} {}",
            expected["id"].as_str().unwrap(),
            if matched { "ok" } else { "failed" }
        );
        if !matched {
            println!(
                "OE_SOURCE_RUNTIME_DATA_MISMATCH {}",
                json!({"id":expected["id"],"expected":expected,"actual":actual})
            );
            failures.push(expected["id"].clone());
        }
    }
    assert!(failures.is_empty(), "{id}: {failures:?}");
}
macro_rules! cases {($($name:ident=>$id:literal),*$(,)?)=>{$(#[test]fn $name(){run($id);})*};}
cases! {source_runtime_data_vs=>"VS",source_runtime_data_vp=>"VP",source_runtime_data_vo=>"VO",source_runtime_data_va=>"VA",source_runtime_data_vg=>"VG"}

#[test]
fn source_runtime_data_failure_keeps_reached_state_clears_caches_without_revision() {
    let mut s = DatasetSession::new_source_data_internal(true);
    s.query_source_internal("ASK {}", &BTreeMap::new(), &BTreeMap::new(), true, None)
        .unwrap();
    s.snapshot_rdflib("2000-01-01T00:00:00Z").unwrap();
    assert_eq!(s.query_cache.len(), 1);
    assert!(s.rdflib_snapshot_cache.get().is_some());
    let e = s
        .update_rdflib("INSERT DATA {?x <urn:p> <urn:o>}; CREATE GRAPH <urn:g>")
        .unwrap_err();
    assert!(
        e.to_string()
            .contains("unreceived source UPDATE operation execution")
    );
    assert_eq!(s.quad_count(), 1);
    assert_eq!(s.revision(), 0);
    assert!(s.query_cache.is_empty());
    assert!(s.rdflib_snapshot_cache.get().is_none());
    assert_eq!(
        s.query_source_internal(
            "ASK {?s ?p ?o}",
            &BTreeMap::new(),
            &BTreeMap::new(),
            false,
            None
        )
        .unwrap()
        .result["boolean"],
        true
    );
    // Declared Rust control only; this unreceived CREATE diagnostic is not
    // the full original CREATE/error contract, which remains required.
}
#[test]
fn source_runtime_data_invalid_input_preserves_warm_caches_and_native_guard() {
    let mut s = DatasetSession::new_source_data_internal(true);
    s.query_source_internal("ASK {}", &BTreeMap::new(), &BTreeMap::new(), true, None)
        .unwrap();
    s.snapshot_rdflib("2000-01-01T00:00:00Z").unwrap();
    let e = s.update_rdflib(" ").unwrap_err();
    let e = e.downcast_ref::<SourceFailure>().unwrap();
    assert_eq!(e.class, "SemanticInputError");
    assert_eq!(e.to_string(), "SPARQL update must be a non-empty string");
    assert_eq!(s.query_cache.len(), 1);
    assert!(s.rdflib_snapshot_cache.get().is_some());
    assert_eq!(s.revision(), 0);
    s.update_rdflib("INSERT DATA {?x <urn:p> <urn:o>}").unwrap();
    assert!(s.rdflib_projection.require_native_complete().is_err());
    let mut native = DatasetSession::new();
    assert!(
        native
            .update_source_internal("INSERT DATA {<urn:s> <urn:p> <urn:o>}", &BTreeMap::new())
            .is_err()
    );
    assert_eq!(native.quad_count(), 0);
}
#[test]
fn source_runtime_data_state_crosses_threads_and_clones_remain_independent() {
    let mut s = DatasetSession::new_source_data_internal(true);
    s.update_rdflib("INSERT DATA {?x <urn:p> <urn:o>}").unwrap();
    let mut child = s.clone();
    let child = std::thread::spawn(move || {
        child
            .update_rdflib("DELETE DATA {?x <urn:p> <urn:o>}")
            .unwrap();
        child
    })
    .join()
    .unwrap();
    assert_eq!(s.quad_count(), 1);
    assert_eq!(s.revision(), 1);
    assert_eq!(child.quad_count(), 0);
    assert_eq!(child.revision(), 2);
}

#[test]
fn source_runtime_data_contract_extension_preserves_previous_admission() {
    let mut legacy = DatasetSession::new_source_internal(true);
    assert!(
        legacy
            .update_rdflib("INSERT DATA {<urn:s> <urn:p> <urn:o>}")
            .is_err()
    );
    assert!(
        legacy
            .update_source_internal("INSERT DATA {<urn:s> <urn:p> <urn:o>}", &BTreeMap::new())
            .is_err()
    );
    assert_eq!(legacy.revision(), 0);
    assert_eq!(legacy.quad_count(), 0);
    let mut extended = DatasetSession::new_source_data_internal(true);
    extended
        .update_rdflib("INSERT DATA {GRAPH <urn:g> {<urn:s> <urn:p> <urn:o>}}")
        .unwrap();
    assert_eq!(
        extended
            .query_source_internal(
                "ASK {?s ?p ?o}",
                &BTreeMap::new(),
                &BTreeMap::new(),
                false,
                None
            )
            .unwrap()
            .result["boolean"],
        false
    );
    assert_eq!(
        extended
            .query_source_internal("ASK {}", &BTreeMap::new(), &BTreeMap::new(), false, None)
            .unwrap()
            .result["boolean"],
        true
    );
    // Same actual source facts, different explicit count contracts. Both
    // retain the same partial native coverage; neither loses source identity.
    let input = br#"<urn:s> <urn:p> "same", "same"^^<http://www.w3.org/2001/XMLSchema#string> ."#;
    let mut old_count = DatasetSession::new_source_internal(true);
    let mut new_count = DatasetSession::new_source_data_internal(true);
    for session in [&mut old_count, &mut new_count] {
        session
            .load_source_internal(input, RdfFormat::Turtle, None, None)
            .unwrap();
        assert_eq!(session.dataset.len(), 1);
        assert_eq!(session.rdflib_projection.quads.as_ref().unwrap().len(), 2);
    }
    assert_eq!(old_count.quad_count(), 1);
    assert_eq!(new_count.quad_count(), 2);
}
