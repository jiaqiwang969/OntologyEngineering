//! Characterisation tests for `ProvenanceManager::export_prov` (rdflib serialisation ported in
//! `rdf.rs`) against the oracle (`fixtures/oracle.json`, sections `export_rich`,
//! `export_multi` and `export_ids`).

mod common;

use common::{
    CountingClock, assert_exports_match, err_json, jsonld_nodes, nt_lines, oracle, xml_blocks,
};
use sem_provenance::schemas::{ActivityRecord, ProvenanceEntry};
use sem_provenance::storage::{InMemoryStorage, ProvenanceStorage};
use sem_provenance::{DEFAULT_BASE_URI, ProvenanceManager, TrackChunkOptions, TrackEntityOptions};
use serde_json::json;

fn te() -> TrackEntityOptions {
    TrackEntityOptions::default()
}

fn rich_manager() -> ProvenanceManager {
    let m = ProvenanceManager::new().unwrap();
    m.track_entity("e_parent", "doc_rdf", None, te());
    m.track_entity(
        "e_child",
        "doc_rdf",
        None,
        te().parent_entity_id("e_parent")
            .activity_id("act_transform")
            .agent_id("alice")
            .agent_type("person")
            .role("approver")
            .acted_on_behalf_of("org1")
            .informed_by(["ingest"])
            .bundle_id("run1"),
    );
    m.track_entity(
        "e3",
        "doc",
        None,
        te().activity(ActivityRecord {
            started_at_time: Some("2026-01-01T00:00:00".into()),
            ended_at_time: Some("2026-01-01T00:00:05".into()),
            ..ActivityRecord::new("a1")
        })
        .agent_id("unknown"),
    );
    m.track_entity(
        "e4",
        "doc",
        None,
        te().activity_id("unknown")
            .agent_id("org9")
            .agent_type("organization")
            .used_entities(["e3"]),
    );
    m.track_entity(
        "e5",
        "doc",
        None,
        te().agent_type("robot")
            .used_entities(["e_parent"])
            .parent_entity_id("e_parent"),
    );
    m.track_relationship("rel", "doc", None, te().agent_id("bob").activity_id("link"));
    m.track_chunk(
        "chunk",
        "doc",
        TrackChunkOptions::new().parent_chunk_id("e4"),
    );
    m.invalidate("e5", "reviewer", None, None).unwrap();
    m
}

#[test]
fn rich_graph_exports_match_oracle() {
    let expected = oracle("export_rich");
    let clock = CountingClock::new();
    let m = rich_manager();
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
    assert_exports_match(&m, None, &expected["default"], false);
    assert_exports_match(&m, Some(DEFAULT_BASE_URI), &expected["default"], false);
    assert_exports_match(&m, Some(""), &expected["default"], false);
    assert_exports_match(
        &m,
        Some("https://example.org/kg#"),
        &expected["custom_base"],
        false,
    );
    assert_exports_match(&m, Some("urn:x:"), &expected["urn_base"], false);
    assert_exports_match(&m, Some("http://x.org/ns"), &expected["no_sep_base"], false);

    // RDF/XML: same header, same description blocks (rdflib orders subjects by set order).
    let xml = m.export_prov("xml", None).unwrap();
    assert_eq!(
        xml_blocks(&xml),
        xml_blocks(expected["unknown_format"]["ok"].as_str().unwrap())
    );
}

#[test]
fn sibling_blank_nodes_export_isomorphic_to_oracle() {
    let expected = oracle("export_multi");
    let clock = CountingClock::new();
    let m = ProvenanceManager::new().unwrap();
    m.track_entity("e_parent", "doc_rdf", None, te());
    m.track_entity(
        "e_child",
        "doc_rdf",
        None,
        te().parent_entity_id("e_parent")
            .used_entities(["e_parent", "u2", "u3"])
            .activity_id("act_transform")
            .informed_by(["i1", "i2"]),
    );
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
    assert_exports_match(&m, None, &expected["export"], true);
}

fn fixed_entry(id: &str, timestamp: &str) -> ProvenanceEntry {
    ProvenanceEntry::with_timestamp(id, "entity", "act", timestamp)
}

#[test]
fn identifier_and_literal_forms_match_oracle() {
    let expected = oracle("export_ids");

    let s = InMemoryStorage::new();
    for id in [
        "doc:X",
        "a.b",
        "ab.",
        "a(b)",
        "a%zz",
        "a%41",
        "a/b",
        "123",
        "_x",
        "-x",
        "\u{e9}t\u{e9}",
        "x\u{b7}y",
        "e_parent:v:2026-01-01T00:00:00",
        "a#b",
        "ns#tail",
        "",
        "a-b",
        "\u{5b9e}\u{4f53}",
        "a~b",
        "a'b",
    ] {
        s.store(&ProvenanceEntry {
            agent_id: "ag".into(),
            ..fixed_entry(id, "2026-01-01T00:00:00")
        })
        .unwrap();
    }
    assert_exports_match(
        &ProvenanceManager::with_storage(s),
        None,
        &expected["export"],
        false,
    );

    let invalid = InMemoryStorage::new();
    invalid
        .store(&fixed_entry("a b", "2026-01-01T00:00:00"))
        .unwrap();
    assert_exports_match(
        &ProvenanceManager::with_storage(invalid),
        None,
        &expected["invalid"],
        false,
    );

    let escapes = InMemoryStorage::new();
    escapes
        .store(&fixed_entry("ok", "2026 \"quoted\" \\ back\nline\r\ttab"))
        .unwrap();
    assert_exports_match(
        &ProvenanceManager::with_storage(escapes),
        None,
        &expected["literal_escapes"],
        false,
    );

    let norm = InMemoryStorage::new();
    for (i, t) in [
        "t0",
        "2026-01-01",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:00.000000",
        "20260101T000000",
        "2026-13-01T00:00:00",
        "2026-01-01T00:00:00+05:30",
    ]
    .into_iter()
    .enumerate()
    {
        norm.store(&ProvenanceEntry {
            activity_started_at_time: Some(t.to_string()),
            ..ProvenanceEntry::with_timestamp(format!("t{i}"), "entity", format!("act{i}"), t)
        })
        .unwrap();
    }
    assert_exports_match(
        &ProvenanceManager::with_storage(norm),
        None,
        &expected["literal_norm"],
        false,
    );

    assert_exports_match(
        &ProvenanceManager::with_storage(InMemoryStorage::new()),
        None,
        &expected["empty"],
        false,
    );
}

#[test]
fn serializer_names_follow_rdflib() {
    let expected = oracle("formats");
    let m = ProvenanceManager::with_storage(InMemoryStorage::new());
    m.storage()
        .store(&ProvenanceEntry {
            agent_id: "ag".into(),
            ..fixed_entry("e1", "2026-01-01T00:00:00")
        })
        .unwrap();
    for (name, want) in expected.as_object().unwrap() {
        let got = m.export_prov(name, None);
        let Some(text) = want.get("ok").and_then(|v| v.as_str()) else {
            assert_eq!(err_json(&got.unwrap_err()), *want, "{name}");
            continue;
        };
        match name.as_str() {
            "turtle" | "ttl" | "text/turtle" | "n3" | "text/n3" => {
                assert_eq!(got.unwrap(), text, "{name}")
            }
            "nt" | "ntriples" | "nt11" | "application/n-triples" | "hext" => {
                assert_eq!(nt_lines(&got.unwrap()), nt_lines(text), "{name}")
            }
            "json-ld" | "jsonld" | "application/ld+json" => {
                assert_eq!(jsonld_nodes(&got.unwrap()), jsonld_nodes(text), "{name}")
            }
            "xml" | "application/rdf+xml" => {
                assert_eq!(xml_blocks(&got.unwrap()), xml_blocks(text), "{name}")
            }
            // Not ported (see PORT-NOTES.md): rdflib's pretty-xml, longturtle and TriG writers.
            "pretty-xml" | "longturtle" | "trig" | "application/trig" => {
                assert_eq!(
                    got.unwrap_err().type_name(),
                    "NotImplementedError",
                    "{name}"
                )
            }
            other => panic!("unexpected format {other}"),
        }
    }
    let graph = m.prov_graph(None).unwrap();
    let statements = expected["nt"]["ok"].as_str().unwrap().lines().count();
    assert_eq!(graph.len(), statements);
    assert_eq!(graph.to_graph().len(), statements);
    assert_eq!(graph.base(), DEFAULT_BASE_URI);
    assert!(!graph.is_empty());
}
