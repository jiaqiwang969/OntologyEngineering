use semantica_runtime::{
    context::{self, DecisionEvent},
    dataset::DatasetSession,
};
use serde_json::json;

#[test]
fn accepted_context_can_exceed_legacy_update_text_budget_without_partial_mutation() {
    let mut session = DatasetSession::new();
    let nodes: Vec<_> = (0..2000).map(|i| json!({"id":format!("n{i}"),"type":"entity","content":"x".repeat(128),"properties":{}})).collect();
    context::load_context_graph(&mut session, &json!({"nodes":nodes,"edges":[]})).unwrap();
    let before = session.snapshot().unwrap();
    assert_eq!(before.quad_count, 8001);
    assert!(before.canonical_nquads.len() > 1024 * 1024);
    let event = DecisionEvent {
        id: "unused",
        timestamp: 0.,
        recorded_at: "1970-01-01T00:00:00",
    };
    let error = context::execute_tool(&mut session, "add_entity", &json!({"id":"next"}), &event)
        .unwrap_err();
    assert_eq!(error.to_string(), "dataset_update_byte_budget");
    assert_eq!(session.snapshot().unwrap(), before);
}

#[test]
fn typed_graph_replacement_preserves_unrelated_terms_and_publishes_once() {
    use oxigraph::{
        io::RdfFormat,
        model::{Dataset, GraphName, Literal, NamedNode, Quad},
    };
    let mut session = DatasetSession::new();
    session.load(b"<urn:s> <urn:p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> <urn:other> .\n<urn:old> <urn:p> <urn:o> <urn:target> .", RdfFormat::NQuads, None, None).unwrap();
    let before = session.snapshot().unwrap();
    let query = "ASK { GRAPH <urn:target> { <urn:old> <urn:p> <urn:o> } }";
    assert_eq!(session.query(query, false).unwrap().result["value"], true);
    let retained: Vec<_> = session
        .dataset()
        .iter()
        .filter(|q| q.graph_name.to_string() == "<urn:other>")
        .map(|q| q.into_owned())
        .collect();
    let mut graph = Dataset::new();
    graph.insert(&Quad::new(
        NamedNode::new("urn:new").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("x".repeat(1024 * 1024 + 1)),
        GraphName::NamedNode(NamedNode::new("urn:target").unwrap()),
    ));
    let result = session.replace_named_graph("urn:target", &graph).unwrap();
    assert_eq!(result.revision, before.revision + 1);
    assert_eq!((result.added, result.removed), (1, 1));
    assert_eq!(session.query(query, false).unwrap().result["value"], false);
    for quad in retained {
        assert!(session.dataset().contains(&quad));
    }
    let after = session.snapshot().unwrap();
    assert_ne!(after.dataset_sha256, before.dataset_sha256);
    assert!(after.canonical_nquads.len() > 1024 * 1024);
    assert!(session.snapshot_rdflib("1970-01-01T00:00:00").is_err());

    let error = session
        .replace_named_graph("urn:other", &graph)
        .unwrap_err();
    assert_eq!(error.to_string(), "dataset_replacement_graph_mismatch");
    assert_eq!(session.snapshot().unwrap(), after);
    let mut oversized = Dataset::new();
    oversized.insert(&Quad::new(
        NamedNode::new("urn:new").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("x".repeat(semantica_runtime::MAX_BYTES)),
        GraphName::NamedNode(NamedNode::new("urn:target").unwrap()),
    ));
    assert_eq!(
        session
            .replace_named_graph("urn:target", &oversized)
            .unwrap_err()
            .to_string(),
        "dataset_byte_budget"
    );
    assert_eq!(session.snapshot().unwrap(), after);
    session
        .replace_named_graph("urn:target", &Dataset::new())
        .unwrap();
    assert_eq!(session.quad_count(), 1);
}

fn expanded_session() -> DatasetSession {
    DatasetSession::with_resources(&semantica_contracts::SemanticDatasetResourceSelection {
        schema: "semantica.dataset-resource-selection/1".into(),
        profile: "semantica.context-50k/1".into(),
    })
    .unwrap()
}

#[test]
fn resource_selection_is_explicit_and_does_not_change_canonical_identity() {
    use oxigraph::io::RdfFormat;
    let mut small = DatasetSession::new();
    let mut expanded = expanded_session();
    for session in [&mut small, &mut expanded] {
        session
            .load(b"<urn:s> <urn:p> \"same\" .", RdfFormat::NQuads, None, None)
            .unwrap();
    }
    assert_eq!(small.snapshot().unwrap(), expanded.snapshot().unwrap());
    assert_eq!(small.resource_profile(), "semantica.dataset-default/1");
    assert_eq!(expanded.resource_profile(), "semantica.context-50k/1");
    assert!(
        DatasetSession::with_resources(&semantica_contracts::SemanticDatasetResourceSelection {
            schema: "semantica.dataset-resource-selection/1".into(),
            profile: "unbounded".into(),
        })
        .is_err()
    );
    assert!(
        DatasetSession::with_resources(&semantica_contracts::SemanticDatasetResourceSelection {
            schema: "wrong/1".into(),
            profile: "semantica.context-50k/1".into(),
        })
        .is_err()
    );
}

#[test]
fn explicit_profile_loads_updates_and_snapshots_fifty_thousand_records() {
    use oxigraph::io::RdfFormat;
    let nodes: Vec<_> = (0..50_000)
        .map(|i| json!({"id":format!("n{i}"),"type":"entity","content":"original","properties":{}}))
        .collect();
    let edges: Vec<_> = (0..50_000).map(|i| json!({"id":format!("e{i}"),"source":format!("n{i}"),"target":format!("n{}",(i+1)%50_000),"type":"LINKS"})).collect();
    let input = json!({"nodes":nodes,"edges":edges});
    let mut default = DatasetSession::new();
    assert_eq!(
        context::load_context_graph(&mut default, &input)
            .unwrap_err()
            .to_string(),
        "context_record_count_budget"
    );
    assert_eq!(default.quad_count(), 0);
    let mut session = expanded_session();
    context::load_context_graph(&mut session, &input).unwrap();
    let before = session.snapshot().unwrap();
    assert_eq!(before.quad_count, 300001);
    assert!(before.canonical_nquads.len() > semantica_runtime::MAX_BYTES);
    let bytes = session.serialize(RdfFormat::NQuads, None).unwrap();
    let mut roundtrip = expanded_session();
    roundtrip
        .load(bytes.as_bytes(), RdfFormat::NQuads, None, None)
        .unwrap();
    assert_eq!(
        roundtrip.snapshot().unwrap().dataset_sha256,
        before.dataset_sha256
    );
    assert!(
        default
            .load(bytes.as_bytes(), RdfFormat::NQuads, None, None)
            .is_err()
    );
    let event = DecisionEvent {
        id: "unused",
        timestamp: 0.,
        recorded_at: "1970-01-01T00:00:00",
    };
    context::execute_tool(
        &mut session,
        "add_entity",
        &json!({"id":"n0","content":"updated"}),
        &event,
    )
    .unwrap();
    let after = session.snapshot().unwrap();
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(after.quad_count, 300001);
    assert_ne!(after.dataset_sha256, before.dataset_sha256);
    let diff = session.diff(&before, Some(&after)).unwrap();
    assert!(!diff.added.is_empty() && !diff.removed.is_empty());
    assert!(semantica_runtime::dataset::diff_snapshots(&before, &after).is_err());
    assert_eq!(
        context::execute_tool(
            &mut session,
            "add_entity",
            &json!({"id":"overflow"}),
            &event
        )
        .unwrap_err()
        .to_string(),
        "context_record_count_budget"
    );
    assert_eq!(session.snapshot().unwrap(), after);
}

#[test]
fn typed_context_import_matches_default_identity_and_late_failure_is_atomic() {
    let graph = json!({"nodes":[{"id":"机械","type":"Part","content":"a\nb","properties":{"number":1,"flag":true}}],"edges":[]});
    let mut default = DatasetSession::new();
    let mut expanded = expanded_session();
    for session in [&mut default, &mut expanded] {
        context::load_context_graph(session, &graph).unwrap();
    }
    assert_eq!(default.snapshot().unwrap(), expanded.snapshot().unwrap());
    assert!(expanded.snapshot_rdflib("1970-01-01T00:00:00").is_err());
    let mut session = expanded_session();
    session
        .load(
            b"<urn:s> <urn:p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .",
            oxigraph::io::RdfFormat::NQuads,
            None,
            None,
        )
        .unwrap();
    let before = session.snapshot().unwrap();
    let invalid =
        json!({"nodes":graph["nodes"],"edges":[{"id":"last","source":"机械","target":"missing"}]});
    assert_eq!(
        context::load_context_graph(&mut session, &invalid)
            .unwrap_err()
            .to_string(),
        "context_edge_endpoint_missing"
    );
    assert_eq!(session.snapshot().unwrap(), before);
    context::load_context_graph(&mut session, &graph).unwrap();
    assert!(
        session
            .dataset()
            .iter()
            .any(|q| q.subject.to_string() == "<urn:s>"
                && q.object.to_string().starts_with("\"01\""))
    );
}
