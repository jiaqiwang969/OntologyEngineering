//! New independent XML observations plus the original NT/NQ/XML load sequences.
use super::*;

fn xml_fixture() -> Value {
    let bytes = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-rdf-formats-original.json"
    )
    .as_bytes();
    assert_eq!(
        crate::sha256(bytes),
        "d21101fd3000371ed93e08c44caf03f969d15bef088b84db1a72dcfbdd780daf"
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
                let before_source_query = s.clone();
                if let Err(error) = s.query_source_internal(
                    "ASK {}",
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    true,
                    None,
                ) {
                    // XMLLiteral/unknown datatype and named graph query domains
                    // remain unreceived; that refusal is not a load failure.
                    let text = format!("{error:#}");
                    assert!(
                        text.contains("source string datatype constructor")
                            || (matches!(id, "LU023" | "XF034")
                                && text == "source_query_named_dataset_not_received")
                            || (id == "XF029" && text == "No scheme found in an absolute IRI"),
                        "{text}"
                    );
                    unchanged(&s, &before_source_query);
                }
                s.snapshot().unwrap();
                s.snapshot_rdflib("2026-10-12T00:00:00Z").unwrap();
                let before = s.clone();
                let format = match a["format"].as_str().unwrap() {
                    "nt" => RdfFormat::NTriples,
                    "nquads" => RdfFormat::NQuads,
                    "xml" => RdfFormat::RdfXml,
                    _ => panic!("unselected original format"),
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
                    "OE_SOURCE_RDF_FORMAT_DIAGNOSTIC {id} {}",
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
            "OE_SOURCE_RDF_FORMAT_CASE {id} {}",
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
fn source_rdf_xml_original48_actions_preserve_construction_and_failure_state() {
    let f = xml_fixture();
    let actions = f["original_plan"]["actions"].as_array().unwrap();
    let ids: BTreeSet<_> = actions
        .iter()
        .filter(|a| a["operation"] != "new")
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(actions.len(), 60);
    assert_eq!(ids.len(), 48);
    receive(
        actions,
        f["original_observations"]["actions"].as_array().unwrap(),
        &ids,
    );
}

#[test]
fn source_rdf_original11_line_and_xml_loading_actions_are_received() {
    let f = fixture();
    let set = f["source_sets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["original_plan"]["actions"][0]["id"] == "LU001")
        .unwrap();
    let ids = BTreeSet::from([
        "LU014", "LU016", "LU017", "LU020", "LU022", "LU023", "LU032", "LU034", "LU035", "LU072",
        "LU074",
    ]);
    receive(
        set["original_plan"]["actions"].as_array().unwrap(),
        set["original_observations"]["actions"].as_array().unwrap(),
        &ids,
    );
}

#[test]
fn source_rdf_xml_language_reset_is_private_and_terms_reach_existing_query() {
    let f = xml_fixture();
    let actions = f["original_plan"]["actions"].as_array().unwrap();
    let identity = actions.iter().find(|a| a["id"] == "XF002").unwrap()["data"]
        .as_str()
        .unwrap();
    let reset = actions.iter().find(|a| a["id"] == "XF047").unwrap()["data"]
        .as_str()
        .unwrap();
    let mut native = DatasetSession::new();
    native
        .load(identity.as_bytes(), RdfFormat::RdfXml, None, None)
        .unwrap();
    assert_eq!(native.quad_count(), 2);
    assert!(
        native
            .load_source_internal(identity.as_bytes(), RdfFormat::RdfXml, None, None)
            .is_err()
    );
    let before = native.clone();
    assert!(
        native
            .load(reset.as_bytes(), RdfFormat::RdfXml, None, None)
            .is_err()
    );
    assert_eq!(native.dataset, before.dataset);
    assert_eq!(native.revision, before.revision);
    let mut source = DatasetSession::new_source_internal(true);
    source
        .load_source_internal(identity.as_bytes(), RdfFormat::RdfXml, None, None)
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
    let rows = query.result["rows"].as_array().unwrap();
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
    source
        .load_source_internal(reset.as_bytes(), RdfFormat::RdfXml, None, None)
        .unwrap();
}

#[test]
fn source_rdf_xml_lenient_native_language_keeps_original_constructor() {
    let input = br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:e="urn:"><rdf:Description rdf:about="urn:s" xml:lang=""><e:p>v</e:p></rdf:Description></rdf:RDF>"#;
    let triples = oxrdfxml::RdfXmlParser::new()
        .lenient()
        .for_slice(input)
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    let Term::Literal(literal) = &triples[0].object else {
        panic!("expected actual native literal");
    };
    assert_eq!(literal.value(), "v");
    assert_eq!(literal.language(), Some(""));
    let source = oxrdfxml::RdfXmlParser::new()
        .for_source_slice(input)
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(source[0].literal.as_ref().unwrap().language, None);
}
