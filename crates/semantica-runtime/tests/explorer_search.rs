use semantica_contracts::SemanticExplorerSearchQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_search::execute,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn query(body: Value) -> SemanticExplorerSearchQuery {
    SemanticExplorerSearchQuery {
        schema: "semantica.explorer-search-query/1".into(),
        body,
    }
}
fn graph(raw: &Value) -> Value {
    let mut raw = raw.clone();
    for e in raw["edges"].as_array_mut().unwrap() {
        let m = e.as_object_mut().unwrap();
        if let Some(v) = m.remove("source_id") {
            m.insert("source".into(), v);
        }
        if let Some(v) = m.remove("target_id") {
            m.insert("target".into(), v);
        }
    }
    raw
}
fn replace(s: &mut DatasetSession, raw: &Value) {
    let mut staged = DatasetSession::new();
    load_context_graph(&mut staged, raw).unwrap();
    // Same receiving session, through its real mutation publication path.
    s.replace_named_graph("urn:semantica:context:records/1", staged.dataset())
        .unwrap();
}
#[test]
fn original_successful_bodies_and_same_session_mutations() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_search_tcp.json")).unwrap();
    let mut raw = graph(&corpus["raw_graph_before"]);
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &raw).unwrap();
    let mut checked = 0;
    for row in corpus["cases"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        if name == "after_add" {
            raw["nodes"]
                .as_array_mut()
                .unwrap()
                .push(corpus["mutations"][0]["node"].clone());
            replace(&mut s, &raw);
        }
        if name == "after_update_old_query" {
            *raw["nodes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|n| n["id"] == "new")
                .unwrap() = corpus["mutations"][1]["node"].clone();
            replace(&mut s, &raw);
        }
        if name == "after_delete" {
            raw["nodes"]
                .as_array_mut()
                .unwrap()
                .retain(|n| n["id"] != "new");
            replace(&mut s, &raw);
        }
        if row["status"] == 500 {
            assert!(
                execute(&mut s, &query(row["body"].clone()), deadline())
                    .unwrap_err()
                    .to_string()
                    .contains("tags_not_iterable")
            );
            continue;
        }
        if row["status"] != 200 {
            continue;
        }
        let result = execute(&mut s, &query(row["body"].clone()), deadline())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(result.body, row["response"], "{name}");
        checked += 1;
    }
    assert_eq!(checked, 112);
}
#[test]
fn cloned_session_cache_follows_its_own_revision_and_expired_queries_do_not_succeed() {
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":[{"id":"a","content":"bearing"}],"edges":[]}),
    )
    .unwrap();
    let q = query(json!({"query":"bearing"}));
    assert_eq!(execute(&mut s, &q, deadline()).unwrap().body["total"], 1);
    let mut clone = s.clone();
    replace(
        &mut s,
        &json!({"nodes":[{"id":"a","content":"shaft"}],"edges":[]}),
    );
    assert_eq!(execute(&mut s, &q, deadline()).unwrap().body["total"], 0);
    assert_eq!(
        execute(&mut clone, &q, deadline()).unwrap().body["total"],
        1
    );
    assert!(execute(&mut clone, &q, Instant::now() - Duration::from_secs(1)).is_err());
    assert_eq!(
        execute(&mut clone, &q, deadline()).unwrap().body["total"],
        1
    );
}

#[test]
fn original_unused_similarity_nonfinite_inputs_keep_other_validation_and_general_guard() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_search_nonfinite.json")).unwrap();
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":[{"id":"a","type":"Part","properties":{"content":"bearing"}}],"edges":[]}),
    )
    .unwrap();
    assert_eq!(corpus["rows"].as_array().unwrap().len(), 11);
    for row in corpus["rows"].as_array().unwrap() {
        let body = row["body"].clone();
        let q = query(body.clone());
        let actual = execute(&mut s, &q, deadline());
        assert_eq!(q.body, body);
        if row["status"] == 200 {
            assert_eq!(actual.unwrap().body, row["response"], "{body}");
        } else {
            assert!(
                actual
                    .unwrap_err()
                    .to_string()
                    .contains("request_model_invalid"),
                "{body}"
            );
        }
    }
    let schema = semantica_runtime::schema::RustSchemaRuntime::new().unwrap();
    assert_eq!(
        schema
            .validate_model(
                "semantica.explorer.schemas::SearchRequest",
                &json!({"query":"bearing","min_semantic_similarity":"nan"})
            )
            .unwrap_err()
            .code,
        "schema_nonfinite_profile_unavailable"
    );
}
#[test]
fn selected_resources_bound_derived_index_without_changing_graph_or_scan_limits() {
    let content = (0..30000)
        .map(|i| format!("t{i:09}"))
        .collect::<Vec<_>>()
        .join(" ");
    let id = "a".repeat(512);
    let raw = json!({"nodes":[{"id":id,"content":content}],"edges":[]});
    let q = query(json!({"query":"t000000001"}));
    let mut small = DatasetSession::new();
    load_context_graph(&mut small, &raw).unwrap();
    let before = small.snapshot().unwrap();
    let revision = small.revision();
    assert!(
        execute(&mut small, &q, deadline())
            .unwrap_err()
            .to_string()
            .contains("explorer_search_cache_budget")
    );
    assert_eq!(small.snapshot().unwrap(), before);
    assert_eq!(small.revision(), revision);
    let mut large =
        DatasetSession::with_resources(&semantica_contracts::SemanticDatasetResourceSelection {
            schema: "semantica.dataset-resource-selection/1".into(),
            profile: "semantica.context-50k/1".into(),
        })
        .unwrap();
    load_context_graph(&mut large, &raw).unwrap();
    let result = execute(&mut large, &q, deadline()).unwrap();
    assert_eq!(result.body["total"], 1);
    assert_eq!(result.body["results"][0]["node"]["id"], id);
}
