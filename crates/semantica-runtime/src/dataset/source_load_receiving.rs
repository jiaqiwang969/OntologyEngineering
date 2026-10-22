//! Independent original loading observations, without claiming skipped queries.
use super::*;
#[path = "source_load_receiving_formats.rs"]
mod formats;
#[path = "source_load_receiving_jsonld.rs"]
mod jsonld;
#[path = "source_load_receiving_trix_construct.rs"]
mod trix_construct;
use crate::rdflib_snapshot::Node;

fn fixture() -> Value {
    let bytes = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-rdf-loading-original.json"
    )
    .as_bytes();
    assert_eq!(
        crate::sha256(bytes),
        "7e1fc6dd5ae195c038a3329cd44093ab8bd2080a0bd40ad9f09cc69decde0baf"
    );
    serde_json::from_slice(bytes).unwrap()
}
fn term(node: &Node) -> Value {
    match node {
        Node::Variable(_) => panic!("original RDF loader cannot construct a source Variable"),
        Node::Iri(value) => {
            json!({"term_type":"iri","value":value,"datatype":null,"language":null})
        }
        Node::Blank(value) => {
            json!({"term_type":"blank_node","value":value,"datatype":null,"language":null})
        }
        Node::Literal(l) => {
            json!({"term_type":"literal","value":l.text,"datatype":l.datatype,"language":l.language})
        }
    }
}
fn source_quads(s: &DatasetSession) -> Vec<Value> {
    s.rdflib_projection
        .quads
        .as_ref()
        .unwrap()
        .iter()
        .map(|q| json!([term(&q.0), term(&q.1), term(&q.2), q.3.as_ref().map(term)]))
        .collect()
}
// Only generated blank labels may change. All fields, repeated identity,
// graph placement, literals, and the global bijection remain constrained.
fn graph_equal(actual: &[Value], expected: &[Value]) -> bool {
    fn walk(
        i: usize,
        a: &[Value],
        e: &[Value],
        used: &mut [bool],
        map: &BTreeMap<String, String>,
        reverse: &BTreeMap<String, String>,
    ) -> bool {
        if i == a.len() {
            return true;
        }
        for j in 0..e.len() {
            if used[j] {
                continue;
            }
            let mut next = map.clone();
            let mut back = reverse.clone();
            let ok = a[i]
                .as_array()
                .unwrap()
                .iter()
                .zip(e[j].as_array().unwrap())
                .all(|(x, y)| {
                    if x["term_type"] != "blank_node" || y["term_type"] != "blank_node" {
                        return x == y;
                    }
                    let (Some(xx), Some(yy)) = (x["value"].as_str(), y["value"].as_str()) else {
                        return false;
                    };
                    let mut left = x.clone();
                    let mut right = y.clone();
                    left["value"] = Value::Null;
                    right["value"] = Value::Null;
                    if left != right
                        || next.get(xx).is_some_and(|v| v != yy)
                        || back.get(yy).is_some_and(|v| v != xx)
                    {
                        return false;
                    }
                    next.insert(xx.into(), yy.into());
                    back.insert(yy.into(), xx.into());
                    true
                });
            if ok {
                used[j] = true;
                if walk(i + 1, a, e, used, &next, &back) {
                    return true;
                }
                used[j] = false;
            }
        }
        false
    }
    actual.len() == expected.len()
        && walk(
            0,
            actual,
            expected,
            &mut vec![false; expected.len()],
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
}
fn warm(s: &mut DatasetSession) {
    s.query("ASK {}", true).unwrap();
    if s.rdflib_projection
        .quads
        .as_ref()
        .unwrap()
        .iter()
        .all(|q| q.3.is_none())
    {
        s.query_source_internal("ASK {}", &BTreeMap::new(), &BTreeMap::new(), true, None)
            .unwrap();
    }
    s.snapshot().unwrap();
    s.snapshot_rdflib("2026-10-12T00:00:00Z").unwrap();
}
fn unchanged(s: &DatasetSession, before: &DatasetSession) {
    assert_eq!(s.dataset, before.dataset);
    assert_eq!(s.revision, before.revision);
    assert_eq!(source_quads(s), source_quads(before));
    assert_eq!(s.rdflib_projection.core, before.rdflib_projection.core);
    assert_eq!(
        s.rdflib_projection.source_sha256,
        before.rdflib_projection.source_sha256
    );
    assert!(s.query_cache == before.query_cache, "query cache changed");
    assert_eq!(s.cache_bytes, before.cache_bytes);
    assert_eq!(
        serde_json::to_value(&s.snapshot_cache).unwrap(),
        serde_json::to_value(&before.snapshot_cache).unwrap()
    );
    assert_eq!(
        serde_json::to_value(s.rdflib_snapshot_cache.get()).unwrap(),
        serde_json::to_value(before.rdflib_snapshot_cache.get()).unwrap()
    );
}
#[test]
fn source_rdf_original48_loading_snapshot_actions_are_received() {
    let f = fixture();
    assert_eq!(f["candidate_results_used"], false);
    let mut total = 0;
    let mut failed = Vec::new();
    for set in f["source_sets"].as_array().unwrap() {
        let ids: BTreeSet<_> = set["received_loading_and_snapshot_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let actions = set["original_plan"]["actions"].as_array().unwrap();
        let expected = set["original_observations"]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), expected.len());
        assert!(matches!(actions.len(), 173 | 53));
        let mut s = DatasetSession::new_source_internal(true);
        for (a, e) in actions.iter().zip(expected) {
            assert_eq!(a["id"], e["id"]);
            if a["operation"] == "new" {
                s = DatasetSession::new_source_internal(a["cache_queries"].as_bool().unwrap());
            }
            let id = a["id"].as_str().unwrap();
            if !ids.contains(id) {
                continue;
            }
            total += 1;
            let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let result = if a["operation"] == "snapshot" {
                    s.snapshot_rdflib(a["created_at"].as_str().unwrap())
                        .map(|v| serde_json::to_value(v).unwrap())
                } else {
                    assert!(matches!(
                        a["operation"].as_str().unwrap(),
                        "load" | "load_rdf"
                    ));
                    warm(&mut s);
                    let before = s.clone();
                    let format = match a["format"].as_str() {
                        None | Some("turtle") => RdfFormat::Turtle,
                        Some("trig") => RdfFormat::TriG,
                        _ => panic!("unselected syntax"),
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
                        "OE_SOURCE_RDF_LOAD_DIAGNOSTIC {id} {}",
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
                "OE_SOURCE_RDF_LOAD_CASE {id} {}",
                if checked.is_ok() { "ok" } else { "failed" }
            );
            if checked.is_err() {
                failed.push(id.to_owned());
            }
        }
    }
    assert_eq!(total, 48);
    assert!(failed.is_empty(), "source loading cases: {failed:?}");
}
#[test]
fn source_rdf_duplicate_emissions_keep_original_unique_dataset_budget() {
    let bytes = crate::private_test_fixtures::text_ref!("release-inputs-03/crates/semantica-runtime/tests/fixtures/source-rdf-duplicate-original.json").as_bytes();
    assert_eq!(
        crate::sha256(bytes),
        "68ef73b5f80c1ee478e5032463d636cee3592801e57bc321c935c9da36075853"
    );
    let f: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(f["candidate_results_used"], false);
    let input = f["recipe"]["unit"]
        .as_str()
        .unwrap()
        .repeat(f["recipe"]["repetitions"].as_u64().unwrap() as usize);
    assert_eq!(json!(input.len()), f["recipe"]["data_bytes"]);
    assert_eq!(
        json!(crate::sha256(input.as_bytes())),
        f["recipe"]["data_sha256"]
    );
    let mut s = DatasetSession::new_source_internal(true);
    let expected = &f["original_observations"]["actions"];
    assert_eq!(
        serde_json::to_value(
            s.load_source_internal(input.as_bytes(), RdfFormat::Turtle, None, None)
                .unwrap()
        )
        .unwrap(),
        expected[1]["raw"]
    );
    assert_eq!(s.quad_count(), 1);
    assert_eq!(s.rdflib_projection.core.len(), 1);
    println!("OE_SOURCE_RDF_LOAD_CASE LD002 ok");
    assert_eq!(
        serde_json::to_value(s.snapshot_rdflib("2026-10-12T00:00:00Z").unwrap()).unwrap(),
        expected[2]["raw"]
    );
    println!("OE_SOURCE_RDF_LOAD_CASE LD003 ok");
}
#[test]
fn source_rdf_metadata_reaches_existing_query_and_native_mode_stays_explicit() {
    let input = b"<urn:s> <urn:p> \"same\", \"same\"^^<http://www.w3.org/2001/XMLSchema#string> .";
    let mut s = DatasetSession::new_source_internal(true);
    s.load_source_internal(input, RdfFormat::Turtle, None, None)
        .unwrap();
    assert_eq!(s.quad_count(), 1);
    let result = s
        .query_source_internal(
            "SELECT ?v WHERE {<urn:s> <urn:p> ?v}",
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
            None,
        )
        .unwrap();
    let rows = result.result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let types: BTreeSet<_> = rows
        .iter()
        .map(|row| row["values"][0][1]["datatype"].to_string())
        .collect();
    assert_eq!(
        types,
        BTreeSet::from([
            "null".into(),
            "\"http://www.w3.org/2001/XMLSchema#string\"".into()
        ])
    );
    let mut native = DatasetSession::new();
    assert!(
        native
            .load_source_internal(input, RdfFormat::Turtle, None, None)
            .is_err()
    );
    native.load(input, RdfFormat::Turtle, None, None).unwrap();
    assert_eq!(native.quad_count(), 1);
    assert!(native.rdflib_projection.quads.is_none());
    let exceptional =
        b"<urn:s> <urn:p> \"v\"^^<http://www.w3.org/1999/02/22-rdf-syntax-ns#langString> .";
    assert!(
        native
            .load(exceptional, RdfFormat::Turtle, None, None)
            .is_err()
    );
    s.load_source_internal(exceptional, RdfFormat::Turtle, None, None)
        .unwrap();
}
#[test]
fn source_rdf_named_graph_and_budget_refusals_keep_existing_owner() {
    let mut s = DatasetSession::new_source_internal(true);
    s.load_source_internal(
        b"<urn:g> {<urn:s> <urn:p> \"v\" .}",
        RdfFormat::TriG,
        None,
        None,
    )
    .unwrap();
    warm(&mut s);
    let before = s.clone();
    assert!(
        s.query_source_internal("ASK {}", &BTreeMap::new(), &BTreeMap::new(), true, None)
            .is_err()
    );
    unchanged(&s, &before);
    for input in [
        vec![b' '; crate::MAX_BYTES + 1],
        b"<urn:new> <urn:p> \"v\" . <unterminated".to_vec(),
    ] {
        assert!(
            s.load_source_internal(&input, RdfFormat::Turtle, None, None)
                .is_err()
        );
        unchanged(&s, &before);
    }
    // Small input expands to more than the unchanged exact native NQuads
    // byte budget. No cap on duplicate emission count is substituted for it.
    let prefix = format!("urn:{}", "x".repeat(600_000));
    let mut input = format!("@prefix e:<{prefix}> .\n");
    for i in 0..15 {
        input.push_str(&format!("e:s{i} e:p e:o .\n"));
    }
    assert!(input.len() < crate::MAX_BYTES);
    let error = s
        .load_source_internal(input.as_bytes(), RdfFormat::Turtle, None, None)
        .unwrap_err();
    assert!(format!("{error:#}").contains("dataset_byte_budget"));
    unchanged(&s, &before);
}
#[test]
fn source_rdf_receiving_blank_bijection_does_not_erase_literal_metadata() {
    let b = |v: &str| json!({"value":v,"term_type":"blank_node","datatype":null,"language":null});
    let p = term(&Node::Iri("urn:p".into()));
    let x = json!({"value":"same","term_type":"literal","datatype":null,"language":null});
    let a = vec![
        json!([b("a"), p, x, null]),
        json!([b("a"), p, b("b"), null]),
    ];
    let mut e = vec![
        json!([b("x"), p, b("y"), null]),
        json!([b("x"), p, x, null]),
    ];
    assert!(graph_equal(&a, &e));
    e[1][2]["datatype"] = json!("http://www.w3.org/2001/XMLSchema#string");
    assert!(!graph_equal(&a, &e));
    e[1][2]["datatype"] = Value::Null;
    e[1][0] = b("y");
    assert!(!graph_equal(&a, &e));
}
