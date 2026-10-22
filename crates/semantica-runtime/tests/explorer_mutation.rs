use semantica_contracts::SemanticExplorerSearchQuery;
use semantica_runtime::{
    context::load_context_graph,
    dataset::DatasetSession,
    explorer_mutation::{execute as run_mutation, observe},
    explorer_search,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn execute(
    session: &mut DatasetSession,
    bridge: bool,
    call: &Value,
    d: Instant,
) -> anyhow::Result<Value> {
    let input = semantica_contracts::SemanticExplorerMutationInput {
        schema: "semantica.explorer-mutation-input/1".into(),
        bridge,
        call: call.clone(),
    };
    Ok(serde_json::to_value(run_mutation(session, &input, d)?)?)
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
#[test]
fn original_54_session_mutations_keep_graph_revision_type_index_and_search() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_mutation.json")).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 54);
    let mut session = DatasetSession::new();
    let mut key = Value::Null;
    for (step, row) in rows.iter().enumerate() {
        let current = json!([row["mode"], row["scenario"]]);
        if current != key {
            key = current;
            session = DatasetSession::new();
            load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
        }
        let result = execute(
            &mut session,
            row["mode"] == "actual_app_bridge",
            &row["call"],
            deadline(),
        )
        .unwrap_or_else(|e| panic!("step {step} {key}: {e:#}"));
        assert_eq!(
            result["result"]["status"], row["result"]["status"],
            "step {step} {key}: {result}"
        );
        if row["result"]["status"] == "returned" {
            assert_eq!(
                result["result"]["value"], row["result"]["value"],
                "step {step}"
            );
        } else {
            assert_eq!(
                result["result"]["error_type"], row["result"]["error_type"],
                "step {step}"
            );
        }
        assert_eq!(
            observe(&session).unwrap(),
            row["after"],
            "state step {step} {key}"
        );
        for (query, expected) in row["search_after"].as_object().unwrap() {
            let q = SemanticExplorerSearchQuery {
                schema: "semantica.explorer-search-query/1".into(),
                body: json!({"query":query,"limit":20}),
            };
            let result = explorer_search::execute(&mut session, &q, deadline())
                .unwrap_or_else(|e| panic!("search step {step}: {e:#}"));
            let items = result.body["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    assert!(
                        item["hop_distance"].is_null() && item["semantic_similarity"].is_null()
                    );
                    json!({"node":item["node"],"score":item["score"]})
                })
                .collect::<Vec<_>>();
            assert_eq!(
                json!(items),
                expected["value"],
                "search step {step} {query}"
            );
        }
    }
}
#[test]
fn native_refusals_preserve_exact_dataset_and_changed_context_is_not_silently_rebound() {
    let mut session = DatasetSession::new();
    load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
    let call = json!({"method":"add_node","args":["a","Part","bearing"],"kwargs":{}});
    execute(&mut session, false, &call, deadline()).unwrap();
    let before = session.snapshot().unwrap();
    assert!(
        execute(
            &mut session,
            false,
            &call,
            Instant::now() - Duration::from_secs(1)
        )
        .is_err()
    );
    assert_eq!(session.snapshot().unwrap(), before);
    assert!(
        execute(&mut session, true, &call, deadline())
            .unwrap_err()
            .to_string()
            .contains("bridge_mode_changed")
    );
    assert_eq!(session.snapshot().unwrap(), before);
    let mut replacement = DatasetSession::new();
    load_context_graph(
        &mut replacement,
        &json!({"nodes":[{"id":"other"}],"edges":[]}),
    )
    .unwrap();
    session
        .replace_named_graph("urn:semantica:context:records/1", replacement.dataset())
        .unwrap();
    let changed = session.snapshot().unwrap();
    assert!(
        execute(&mut session, false, &call, deadline())
            .unwrap_err()
            .to_string()
            .contains("state_stale")
    );
    assert_eq!(session.snapshot().unwrap(), changed);
}

#[test]
fn original_property_insertion_order_survives_later_index_rebuild() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_mutation_order.json")).unwrap();
    let mut session = DatasetSession::new();
    load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
    for (step, row) in corpus["rows"].as_array().unwrap().iter().enumerate() {
        let result = execute(&mut session, false, &row["call"], deadline()).unwrap();
        assert_eq!(result["result"]["value"], row["result"]);
        assert_eq!(
            observe(&session).unwrap()["index_documents"],
            row["index_documents"],
            "property order step {step}"
        );
    }
}

#[test]
fn partial_state_survives_dataset_reload_and_native_late_refusals_are_atomic() {
    let mut session = DatasetSession::new();
    load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
    let partial = json!({"method":"add_nodes","args":[[{"id":"a"},{"id":"bad","properties":{"tags":true}},{"id":"z"}]],"kwargs":{}});
    let result = execute(&mut session, false, &partial, deadline()).unwrap();
    assert_eq!(result["result"]["status"], "raised");
    let expected = observe(&session).unwrap();
    let snapshot = session.snapshot().unwrap();
    let mut restored = DatasetSession::new();
    restored
        .load(
            snapshot.canonical_nquads.as_bytes(),
            oxigraph::io::RdfFormat::NQuads,
            None,
            None,
        )
        .unwrap();
    assert_eq!(observe(&restored).unwrap(), expected);
    let repair = json!({"method":"add_node","args":["bad","Part","repaired"],"kwargs":{}});
    execute(&mut restored, false, &repair, deadline()).unwrap();
    assert!(
        observe(&restored).unwrap()["index_documents"]
            .get("z")
            .is_none()
    );
    let before = restored.snapshot().unwrap();
    for call in [
        json!({"method":"add_nodes","args":[[{"id":"first"},{"id":"x".repeat(513)}]],"kwargs":{}}),
        json!({"method":"add_node","args":["a","Part","invalid"],"kwargs":{"confidence":"NaN"}}),
        json!({"method":"add_node","args":["a","Part","future"],"kwargs":{"valid_from":"2026-01-01"}}),
    ] {
        assert!(execute(&mut restored, false, &call, deadline()).is_err());
        assert_eq!(restored.snapshot().unwrap(), before);
    }
}

#[test]
fn persisted_postings_corruption_is_rejected_before_query_or_mutation() {
    use oxigraph::model::{Dataset, GraphNameRef, Literal, NamedNode, Quad, TermRef};
    let mut session = DatasetSession::new();
    load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
    let call = json!({"method":"add_node","args":["a","Part","bearing"],"kwargs":{}});
    execute(&mut session, false, &call, deadline()).unwrap();
    let graph_name = "urn:semantica:explorer:mutation-state/1";
    let mut corrupted = Dataset::new();
    for q in session.dataset().iter() {
        if matches!(q.graph_name,GraphNameRef::NamedNode(n)if n.as_str()==graph_name) {
            let TermRef::Literal(literal) = q.object else {
                panic!()
            };
            let mut state: Value = serde_json::from_str(literal.value()).unwrap();
            state["index"]["tokens"] = json!({"bearing":["missing-document"]});
            corrupted.insert(&Quad::new(
                q.subject.into_owned(),
                q.predicate.into_owned(),
                Literal::new_simple_literal(serde_json::to_string(&state).unwrap()),
                NamedNode::new(graph_name).unwrap(),
            ));
        }
    }
    session.replace_named_graph(graph_name, &corrupted).unwrap();
    let before = session.snapshot().unwrap();
    assert!(
        execute(&mut session, false, &call, deadline())
            .unwrap_err()
            .to_string()
            .contains("postings_invalid")
    );
    assert_eq!(session.snapshot().unwrap(), before);
    let query = SemanticExplorerSearchQuery {
        schema: "semantica.explorer-search-query/1".into(),
        body: json!({"query":"bearing"}),
    };
    assert!(
        explorer_search::execute(&mut session, &query, deadline())
            .unwrap_err()
            .to_string()
            .contains("postings_invalid")
    );
    assert_eq!(session.snapshot().unwrap(), before);
}
