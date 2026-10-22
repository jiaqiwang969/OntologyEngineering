//! Frozen JSON-LD source observations; original raw output is the independent oracle.
use super::*;

fn jsonld_fixture() -> Value {
    let bytes = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-jsonld-original.json"
    )
    .as_bytes();
    assert_eq!(
        crate::sha256(bytes),
        "46cc834b81b46f8a978eef100d5e537b3f84ccceb1577383233f4ac47728b61f"
    );
    let f: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(f["candidate_results_used"], false);
    f
}

fn receive(actions: &[Value], expected: &[Value], ids: &BTreeSet<&str>) {
    assert_eq!(actions.len(), expected.len());
    let mut s = DatasetSession::new_source_internal(true);
    let mut received = BTreeSet::new();
    let mut failed = Vec::new();
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
            let result = if a["operation"] == "snapshot" {
                s.snapshot_rdflib(a["created_at"].as_str().unwrap())
                    .map(|v| serde_json::to_value(v).unwrap())
            } else {
                assert!(matches!(a["operation"].as_str(), Some("load" | "load_rdf")));
                s.query("ASK {}", true).unwrap();
                // Source querying has separate unreceived named/JSON-literal
                // domains. Warm native/snapshot caches for atomic load checks.
                s.snapshot().unwrap();
                s.snapshot_rdflib("2026-10-12T00:00:00Z").unwrap();
                let before = s.clone();
                assert!(matches!(a["format"].as_str(), Some("json-ld" | "jsonld")));
                let format = RdfFormat::JsonLd {
                    profile: Default::default(),
                };
                let result = s
                    .load_source_internal(
                        a["data"].as_str().unwrap().as_bytes(),
                        format,
                        a["base_uri"].as_str(),
                        a["graph_name"].as_str(),
                    )
                    .map(|v| serde_json::to_value(v).unwrap());
                if result.is_err() {
                    unchanged(&s, &before);
                } else {
                    assert!(s.query_cache.is_empty());
                    assert!(s.snapshot_cache.is_none());
                    assert!(s.rdflib_snapshot_cache.get().is_none());
                }
                result
            };
            if let Err(error) = &result {
                println!(
                    "OE_SOURCE_JSONLD_DIAGNOSTIC {id} {}",
                    serde_json::to_string(&format!("{error:#}")).unwrap()
                );
            }
            if e["status"] == "completed" {
                assert_eq!(
                    result.unwrap_or_else(|error| panic!("{id}: {error:#}")),
                    e["raw"],
                    "{id}"
                );
            } else {
                assert!(result.is_err(), "{id}: unexpected success");
            }
            assert_eq!(json!(s.revision()), e["state"]["revision"]);
            let actual = source_quads(&s);
            assert_eq!(json!(actual.len()), e["state"]["quad_count"]);
            assert!(
                graph_equal(&actual, e["state"]["quads"].as_array().unwrap()),
                "{id}: actual={actual:?}; expected={}",
                e["state"]["quads"]
            );
            assert_eq!(s.rdflib_projection.dataset().unwrap(), s.dataset);
        }));
        println!(
            "OE_SOURCE_JSONLD_CASE {id} {}",
            if checked.is_ok() { "ok" } else { "failed" }
        );
        if checked.is_err() {
            failed.push(id);
        }
    }
    assert_eq!(&received, ids);
    assert!(failed.is_empty(), "unreceived format actions: {failed:?}");
}

#[test]
fn source_jsonld_original88_loading_snapshot_actions_are_received() {
    let f = jsonld_fixture();
    let ids: BTreeSet<_> = f["received_loading_and_snapshot_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 88);
    receive(
        f["original_plan"]["actions"].as_array().unwrap(),
        f["original_observations"]["actions"].as_array().unwrap(),
        &ids,
    );
}
#[test]
fn source_jsonld_original_lu_three_actions_are_received() {
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
        &BTreeSet::from(["LU044", "LU046", "LU047"]),
    );
}
#[test]
fn source_jsonld_native_admission_and_constructor_meanings_stay_separate() {
    let f = jsonld_fixture();
    let actions = f["original_plan"]["actions"].as_array().unwrap();
    let input = |id: &str| {
        actions.iter().find(|a| a["id"] == id).unwrap()["data"]
            .as_str()
            .unwrap()
    };
    let format = RdfFormat::JsonLd {
        profile: Default::default(),
    };
    let mut native = DatasetSession::new();
    let before = native.clone();
    assert!(
        native
            .load(input("JL002").as_bytes(), format, None, None)
            .is_err()
    );
    unchanged(&native, &before);
    // The lower native parser still keeps named graphs and rejects language+type.
    let named = oxjsonld::JsonLdParser::new()
        .for_slice(input("JL047"))
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(named.len(), 1);
    assert!(matches!(named[0].graph_name, GraphName::NamedNode(_)));
    assert!(
        oxjsonld::JsonLdParser::new()
            .for_slice(input("JL097"))
            .collect::<std::result::Result<Vec<_>, _>>()
            .is_err()
    );
    let mut source = DatasetSession::new_source_internal(true);
    source
        .load_source_internal(input("JL002").as_bytes(), format, None, None)
        .unwrap();
    assert_eq!(source.quad_count(), 2);
    assert_eq!(source_quads(&source).len(), 3);
    let query = source
        .query_source_internal(
            "SELECT ?v WHERE {<urn:s> <urn:p> ?v}",
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
            None,
        )
        .unwrap();
    assert_eq!(query.result["rows"].as_array().unwrap().len(), 2);
    let before = source.clone();
    assert!(
        source
            .load_source_internal(input("JL107").as_bytes(), format, None, None)
            .is_err()
    );
    unchanged(&source, &before);
    let mut invalid = oxjsonld::JsonLdParser::new()
        .for_source_slice(input("JL092").as_bytes())
        .unwrap();
    assert!(invalid.next().unwrap().is_err());
    assert!(invalid.next().is_none());
}
