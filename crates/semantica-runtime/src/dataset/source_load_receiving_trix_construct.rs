//! Independent full original TriX/ConstructDTO outcomes, source identity and failures.
use super::*;
fn trix_fixture() -> Value {
    let bytes = crate::private_test_fixtures::text_ref!("release-inputs-03/crates/semantica-runtime/tests/fixtures/source-trix-construct-original.json").as_bytes();
    assert_eq!(
        crate::sha256(bytes),
        "e6cf066345c4820aefbc58a15e7e677ca55a62d7f507c72c38c3ea10bcb30690"
    );
    let f: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(f["candidate_results_used"], false);
    f
}
fn warm_source(s: &mut DatasetSession) {
    if s.rdflib_projection.require_native_complete().is_ok() {
        s.query("ASK {}", true).unwrap();
        s.snapshot().unwrap();
    }
    if s.rdflib_projection
        .quads
        .as_ref()
        .unwrap()
        .iter()
        .all(|q| matches!(q.1, Node::Iri(_)))
    {
        s.snapshot_rdflib("2026-10-12T00:00:00Z").unwrap();
    } else {
        assert!(
            s.snapshot_rdflib("2026-10-12T00:00:00Z")
                .unwrap_err()
                .to_string()
                .contains("RDF Dataset predicate must be an IRI")
        );
    }
}
fn receive(actions: &[Value], expected: &[Value], ids: &BTreeSet<&str>) {
    assert_eq!(actions.len(), expected.len());
    let mut s = DatasetSession::new_source_internal(true);
    let mut construct = None;
    let mut received = BTreeSet::new();
    let mut failed = vec![];
    for (a, e) in actions.iter().zip(expected) {
        assert_eq!(a["id"], e["id"]);
        if a["operation"] == "new" {
            s = DatasetSession::new_source_internal(a["cache_queries"].as_bool().unwrap());
        }
        let id = a["id"].as_str().unwrap();
        if !ids.contains(id) {
            continue;
        }
        assert!(received.insert(id));
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let op = a["operation"].as_str().unwrap();
            // Query cache-hit behavior is independently observed in LU080..089:
            // do not inject unrelated warms in that sequence.
            if id.starts_with("TC") && op != "snapshot" {
                warm_source(&mut s);
            }
            let before = s.clone();
            let result = (|| -> anyhow::Result<Value> {
                Ok(match op {
                    "snapshot" => {
                        serde_json::to_value(s.snapshot_rdflib(a["created_at"].as_str().unwrap())?)?
                    }
                    "load" | "load_rdf" => serde_json::to_value(if a["format"] == "trix" {
                        s.load_trix_source_internal(
                            a["data"].as_str().unwrap().as_bytes(),
                            a["base_uri"].as_str(),
                            a["graph_name"].as_str(),
                        )?
                    } else {
                        let format = match a["format"].as_str() {
                            Some("nt") => RdfFormat::NTriples,
                            None | Some("turtle") => RdfFormat::Turtle,
                            _ => panic!("unselected syntax"),
                        };
                        s.load_source_internal(
                            a["data"].as_str().unwrap().as_bytes(),
                            format,
                            a["base_uri"].as_str(),
                            a["graph_name"].as_str(),
                        )?
                    })?,
                    "construct_dto" => serde_json::to_value(s.load_construct_source_internal(
                        a,
                        a["format"].as_str(),
                        a["graph_name"].as_str(),
                    )?)?,
                    "load_construct" => serde_json::to_value(s.load_construct_source_internal(
                        construct.as_ref().unwrap(),
                        a["format"].as_str(),
                        a["graph_name"].as_str(),
                    )?)?,
                    "query" | "construct" => {
                        let ns = serde_json::from_value(a["namespaces"].clone())?;
                        let bindings = serde_json::from_value(a["bindings"].clone())?;
                        let result = s.query_source_internal(
                            a["query"].as_str().unwrap(),
                            &ns,
                            &bindings,
                            a["use_cache"].as_bool().unwrap(),
                            None,
                        )?;
                        let mut raw = result.result;
                        raw["revision"] = json!(result.revision);
                        raw["cache_hit"] = json!(result.cache_hit);
                        if op == "construct" {
                            construct = Some(raw.clone());
                        }
                        raw
                    }
                    _ => panic!("unselected operation"),
                })
            })();
            if let Err(error) = &result {
                println!(
                    "OE_SOURCE_TRIX_CONSTRUCT_DIAGNOSTIC {id} {}",
                    serde_json::to_string(&format!("{error:#}")).unwrap()
                );
                unchanged(&s, &before);
            } else if matches!(op, "load" | "load_rdf" | "construct_dto" | "load_construct") {
                assert!(s.query_cache.is_empty());
                assert!(s.snapshot_cache.is_none());
                assert!(s.rdflib_snapshot_cache.get().is_none());
            }
            if e["status"] == "completed" {
                assert_eq!(
                    result.unwrap_or_else(|e| panic!("{id}: {e:#}")),
                    e["raw"],
                    "{id}"
                );
            } else {
                assert!(result.is_err(), "{id}: unexpected success");
            }
            assert_eq!(json!(s.revision()), e["state"]["revision"], "{id}");
            let actual = source_quads(&s);
            assert_eq!(json!(actual.len()), e["state"]["quad_count"], "{id}");
            assert!(
                graph_equal(&actual, e["state"]["quads"].as_array().unwrap()),
                "{id}: actual={actual:?}; expected={}",
                e["state"]["quads"]
            );
            assert_eq!(s.rdflib_projection.native_projection().unwrap(), s.dataset);
        }));
        println!(
            "OE_SOURCE_TRIX_CONSTRUCT_CASE {id} {}",
            if checked.is_ok() { "ok" } else { "failed" }
        );
        if checked.is_err() {
            failed.push(id);
        }
    }
    assert_eq!(&received, ids);
    assert!(failed.is_empty(), "unreceived actions: {failed:?}");
}
#[test]
fn source_trix_construct_original196_actions_are_received() {
    let f = trix_fixture();
    let ids = f["received_action_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), 196);
    receive(
        f["original_plan"]["actions"].as_array().unwrap(),
        f["original_observations"]["actions"].as_array().unwrap(),
        &ids,
    );
}
#[test]
fn source_trix_construct_original_lu_twelve_actions_are_received() {
    let f = fixture();
    let set = f["source_sets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["original_plan"]["actions"].as_array().unwrap().len() == 173)
        .unwrap();
    receive(
        set["original_plan"]["actions"].as_array().unwrap(),
        set["original_observations"]["actions"].as_array().unwrap(),
        &BTreeSet::from([
            "LU038", "LU040", "LU041", "LU080", "LU081", "LU083", "LU084", "LU085", "LU086",
            "LU087", "LU088", "LU089",
        ]),
    );
}
#[test]
fn source_trix_construct_native_coverage_and_recovery_are_explicit() {
    let f = trix_fixture();
    let actions = f["original_plan"]["actions"].as_array().unwrap();
    let action = |id: &str| actions.iter().find(|a| a["id"] == id).unwrap();
    for id in ["TC072", "TC077", "TC112", "TC117", "TC122", "TC137"] {
        let a = action(id);
        let mut s = DatasetSession::new_source_internal(true);
        s.load_source_internal(
            b"<urn:seed> <urn:p> \"retained\" .",
            RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
        s.load_trix_source_internal(
            a["data"].as_str().unwrap().as_bytes(),
            a["base_uri"].as_str(),
            None,
        )
        .unwrap();
        assert_eq!(source_quads(&s).len(), 2);
        assert_eq!(s.quad_count(), 1);
        warm_source(&mut s);
        let before = s.clone();
        assert!(s.snapshot().is_err());
        assert!(s.query("SELECT * WHERE {?s ?p ?o}", true).is_err());
        assert!(s.serialize(RdfFormat::NQuads, None).is_err());
        assert!(s.update("CLEAR ALL").is_err());
        unchanged(&s, &before);
        // A later ordinary parser load must retain generalized source rows.
        s.load_source_internal(
            b"<urn:another> <urn:p> \"new\" .",
            RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
        assert_eq!(source_quads(&s).len(), 3);
        assert_eq!(s.quad_count(), 2);
        assert_eq!(s.clear_rdflib(None).unwrap().removed, 3);
        assert!(s.rdflib_projection.require_native_complete().is_ok());
        assert_eq!(s.quad_count(), 0);
        assert!(s.snapshot().is_ok());
    }
    // An explicit source binding cannot falsely hide an actually representable row.
    let mut s = DatasetSession::new_source_internal(true);
    s.load_source_internal(b"<urn:s> <urn:p> \"o\" .", RdfFormat::NTriples, None, None)
        .unwrap();
    for value in s.rdflib_projection.core.values_mut() {
        value.clear();
    }
    assert!(
        s.rdflib_projection
            .native_projection()
            .unwrap_err()
            .to_string()
            .contains("source_native_coverage_false_gap")
    );
    let mut ordinary = DatasetSession::new();
    let before = ordinary.clone();
    assert!(
        ordinary
            .load_trix_source_internal(
                action("TC002")["data"].as_str().unwrap().as_bytes(),
                None,
                None
            )
            .is_err()
    );
    assert!(
        ordinary
            .load_construct_source_internal(action("TC144"), None, None)
            .is_err()
    );
    unchanged(&ordinary, &before);
}
#[test]
fn source_trix_construct_blank_identity_repeats_and_failure_atomicity() {
    let f = trix_fixture();
    let actions = f["original_plan"]["actions"].as_array().unwrap();
    let a = actions.iter().find(|a| a["id"] == "TC151").unwrap();
    let mut s = DatasetSession::new_source_internal(true);
    assert_eq!(
        s.load_construct_source_internal(a, None, None)
            .unwrap()
            .added,
        2
    );
    assert_eq!(
        s.load_construct_source_internal(a, None, None)
            .unwrap()
            .added,
        0
    );
    assert!(source_quads(&s).iter().any(|q| {
        q.as_array()
            .unwrap()
            .iter()
            .any(|t| t["term_type"] == "blank_node" && t["value"] == "shared")
    }));
    let query = s
        .query_source_internal(
            "SELECT ?s ?p ?o WHERE {?s ?p ?o}",
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
            None,
        )
        .unwrap();
    assert_eq!(query.result["rows"].as_array().unwrap().len(), 2);
    warm_source(&mut s);
    let before = s.clone();
    let bad = actions.iter().find(|a| a["id"] == "TC179").unwrap();
    assert!(s.load_construct_source_internal(bad, None, None).is_err());
    unchanged(&s, &before);
}
