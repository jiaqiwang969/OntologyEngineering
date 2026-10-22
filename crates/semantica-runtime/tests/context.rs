use oxigraph::io::RdfFormat;
use semantica_runtime::{
    context::{self, DecisionEvent},
    dataset::DatasetSession,
};
use serde_json::{Value, json};
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/original-context-oracle.json")).unwrap()
}
#[test]
fn original_context_eleven_mutations_and_queries_use_one_session() {
    let cases = oracle();
    let mut session = DatasetSession::new();
    let fallback = DecisionEvent {
        id: "unused",
        timestamp: 1790860000.,
        recorded_at: "2026-10-01T12:00:00",
    };
    for case in cases["cases"].as_array().unwrap() {
        let event = case.get("event").map(|e| DecisionEvent {
            id: e["id"].as_str().unwrap(),
            timestamp: e["timestamp"].as_f64().unwrap(),
            recorded_at: e["recorded_at"].as_str().unwrap(),
        });
        let tool = case["tool"].as_str().unwrap().trim_start_matches("_tool_");
        let actual = context::execute_tool(
            &mut session,
            tool,
            &case["arguments"],
            event.as_ref().unwrap_or(&fallback),
        )
        .unwrap();
        assert_eq!(actual, case["expected"], "{tool}");
        let summary = context::context_summary(&session).unwrap();
        assert_eq!(
            summary["node_count"],
            case["nodes"].as_array().unwrap().len()
        );
        let exported = context::execute_tool(
            &mut session,
            "export_graph",
            &json!({"format":"json"}),
            &fallback,
        )
        .unwrap();
        assert_eq!(
            exported["data"]["edges"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| &e["id"])
                .collect::<Vec<_>>(),
            case["edges"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| &e["id"])
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn original_invalid_decisions_do_not_publish_partial_records() {
    let fixture = oracle();
    for case in fixture["invalid"].as_array().unwrap() {
        let mut session = DatasetSession::new();
        let before = session.snapshot().unwrap();
        let event = DecisionEvent {
            id: "case",
            timestamp: 1790860000.,
            recorded_at: "2026-10-01T12:00:00",
        };
        assert!(
            context::execute_tool(&mut session, "record_decision", &case["arguments"], &event)
                .is_err(),
            "{}",
            case["arguments"]
        );
        assert_eq!(session.snapshot().unwrap(), before);
    }
}
#[test]
fn context_updates_preserve_existing_rdf_and_deduplicate_original_edge_identity() {
    let mut session = DatasetSession::new();
    session
        .load(
            b"<urn:other:s> <urn:other:p> <urn:other:o> .",
            RdfFormat::Turtle,
            None,
            None,
        )
        .unwrap();
    let event = DecisionEvent {
        id: "unused",
        timestamp: 0.,
        recorded_at: "1970-01-01T00:00:00",
    };
    context::execute_tool(
        &mut session,
        "add_relationship",
        &json!({"source":"a","target":"b","type":"SUPPORTS"}),
        &event,
    )
    .unwrap();
    let revision = session.revision();
    context::execute_tool(
        &mut session,
        "add_relationship",
        &json!({"source":"a","target":"b","type":"SUPPORTS"}),
        &event,
    )
    .unwrap();
    assert_eq!(session.revision(), revision);
    assert_eq!(
        session
            .query("ASK {<urn:other:s> <urn:other:p> <urn:other:o>}", false)
            .unwrap()
            .result["value"],
        true
    );
    assert_eq!(context::context_summary(&session).unwrap()["node_count"], 2);
    let jsonld = context::execute_tool(&mut session, "export_graph", &json!({}), &event).unwrap();
    assert!(serde_json::from_str::<Value>(jsonld["data"].as_str().unwrap()).is_ok());
}
#[test]
fn original_causal_walk_is_serialized_only_in_declared_corrected_profile() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/original-context-supplement.json")).unwrap();
    let case = &fixture["causal"];
    let mut session = DatasetSession::new();
    context::load_context_graph(
        &mut session,
        &json!({"nodes":case["nodes"],"edges":case["edges"]}),
    )
    .unwrap();
    let event = DecisionEvent {
        id: "unused",
        timestamp: 0.,
        recorded_at: "1970-01-01T00:00:00",
    };
    let result =
        context::execute_tool(&mut session, "get_causal_chain", &case["arguments"], &event)
            .unwrap();
    assert_eq!(result["chain"], case["native_serialized_expected"]);
    assert_eq!(
        fixture["causal"]["canonical_actual_error"]["message"],
        "Object of type Decision is not JSON serializable"
    );
}
#[test]
fn native_analytics_has_an_independent_finite_pagerank_expectation() {
    let mut session = DatasetSession::new();
    let event = DecisionEvent {
        id: "unused",
        timestamp: 0.,
        recorded_at: "1970-01-01T00:00:00",
    };
    context::execute_tool(
        &mut session,
        "add_relationship",
        &json!({"source":"a","target":"b"}),
        &event,
    )
    .unwrap();
    context::execute_tool(&mut session, "add_entity", &json!({"id":"c"}), &event).unwrap();
    let result =
        context::execute_tool(&mut session, "get_graph_analytics", &json!({}), &event).unwrap();
    let ranks = result["top_nodes_by_pagerank"].as_array().unwrap();
    assert_eq!(ranks[0][0], "b");
    assert!((ranks[0][1].as_f64().unwrap() - 0.0925).abs() < 1e-14);
    assert!((ranks[1][1].as_f64().unwrap() - 0.05).abs() < 1e-14);
    assert_eq!(result["community_count"], 2);
    assert_eq!(result["edge_count"], 0); // Canonical edge_count default.
}
#[test]
fn original_edge_identity_binds_unicode_nonbmp_and_large_numeric_metadata() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/original-context-edge-identities.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut session = DatasetSession::new();
        let event = DecisionEvent {
            id: "unused",
            timestamp: 0.,
            recorded_at: "1970-01-01T00:00:00",
        };
        context::execute_tool(&mut session, "add_relationship", &case["arguments"], &event)
            .unwrap();
        let result = context::execute_tool(
            &mut session,
            "export_graph",
            &json!({"format":"json"}),
            &event,
        )
        .unwrap();
        assert_eq!(result["data"]["edges"][0]["id"], case["expected_uuid5"]);
    }
}
#[test]
fn original_skos_cycle_preconditions_are_checked_before_any_mutation() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/original-skos-oracle.json")).unwrap();
    let event = DecisionEvent {
        id: "unused",
        timestamp: 0.,
        recorded_at: "1970-01-01T00:00:00",
    };
    for case in cases["cases"].as_array().unwrap() {
        let mut session = DatasetSession::new();
        let edges=case["existing"].as_array().unwrap().iter().enumerate().map(|(i,e)|json!({"id":format!("existing-{i}"),"source":e["source"],"target":e["target"],"type":e["type"]})).collect::<Vec<_>>();
        let nodes = edges
            .iter()
            .flat_map(|e| [e["source"].as_str().unwrap(), e["target"].as_str().unwrap()])
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|id| json!({"id":id,"type":"entity"}))
            .collect::<Vec<_>>();
        if !edges.is_empty() {
            context::load_context_graph(&mut session, &json!({"nodes":nodes,"edges":edges}))
                .unwrap();
        }
        let before = session.snapshot().unwrap();
        let actual = context::execute_tool(&mut session, "add_relationship", &case["new"], &event);
        assert_eq!(
            actual.is_ok(),
            case["expected"]["allowed"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if !case["expected"]["allowed"].as_bool().unwrap() {
            assert!(
                actual
                    .unwrap_err()
                    .to_string()
                    .starts_with("SKOS hierarchy contains a cycle involving '")
            );
            assert_eq!(session.snapshot().unwrap(), before);
        }
    }
}
