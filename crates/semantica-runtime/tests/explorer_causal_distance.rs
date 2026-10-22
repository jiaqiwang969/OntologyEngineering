use semantica_contracts::SemanticExplorerCausalDistanceQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_causal_distance::execute,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn query(source: &str, target: &str) -> SemanticExplorerCausalDistanceQuery {
    SemanticExplorerCausalDistanceQuery {
        schema: "semantica.explorer-causal-distance-query/2".into(),
        source_id: Some(source.into()),
        target_id: Some(target.into()),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
#[test]
fn original_47_causal_computations_keep_path_order_rounding_and_interpretation() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_causal_distance.json")).unwrap();
    assert_eq!(corpus["rows"].as_array().unwrap().len(), 47);
    for row in corpus["rows"].as_array().unwrap() {
        let mut raw = row["graph"].clone();
        for e in raw["edges"].as_array_mut().unwrap() {
            let m = e.as_object_mut().unwrap();
            for (a, b) in [("source_id", "source"), ("target_id", "target")] {
                if let Some(v) = m.remove(a) {
                    m.insert(b.into(), v);
                }
            }
        }
        let mut s = DatasetSession::new();
        load_context_graph(&mut s, &raw).unwrap();
        let before = s.snapshot().unwrap();
        let result = execute(
            &s,
            &query(
                row["source"].as_str().unwrap(),
                row["target"].as_str().unwrap(),
            ),
            deadline(),
        )
        .unwrap();
        assert_eq!(result.status, 200);
        assert_eq!(result.body, row["result"], "{}", row["name"]);
        assert_eq!(s.snapshot().unwrap(), before);
    }
}
#[test]
fn changed_graph_and_expired_query_do_not_reuse_prior_causal_result() {
    let mut s = DatasetSession::new();
    let raw =
        json!({"nodes":[{"id":"a","type":"decision"},{"id":"b","type":"decision"}],"edges":[]});
    load_context_graph(&mut s, &raw).unwrap();
    let q = query("a", "b");
    assert_eq!(
        execute(&s, &q, deadline()).unwrap().body["causal_path"],
        json!([])
    );
    let mut next = raw;
    next["edges"] = json!([{"id":"ab","source":"a","target":"b","type":"supports","weight":0.5}]);
    let mut staged = DatasetSession::new();
    load_context_graph(&mut staged, &next).unwrap();
    s.replace_named_graph("urn:semantica:context:records/1", staged.dataset())
        .unwrap();
    assert_eq!(
        execute(&s, &q, deadline()).unwrap().body["causal_path"],
        json!(["a", "b"])
    );
    assert!(execute(&s, &q, Instant::now() - Duration::from_secs(1)).is_err());
    assert_eq!(
        execute(&s, &query("", "b"), deadline()).unwrap().body["causal_path"],
        json!([])
    );
}

#[test]
fn omitted_endpoint_is_not_the_same_as_explicit_empty_id() {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &json!({"nodes":[],"edges":[]})).unwrap();
    let mut q = query("", "");
    assert!(execute(&s, &q, deadline()).is_ok());
    q.source_id = None;
    assert!(
        execute(&s, &q, deadline())
            .unwrap_err()
            .to_string()
            .contains("source_required")
    );
    q.source_id = Some("".into());
    q.target_id = None;
    assert!(
        execute(&s, &q, deadline())
            .unwrap_err()
            .to_string()
            .contains("target_required")
    );
}
