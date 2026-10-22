use semantica_contracts::SemanticExplorerPathQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_path_similarity::execute,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn query(a: &str, b: &str) -> SemanticExplorerPathQuery {
    SemanticExplorerPathQuery {
        schema: "semantica.explorer-path-query/1".into(),
        source: a.into(),
        target: b.into(),
        algorithm: "bfs".into(),
        directed: true,
    }
}
fn session(nodes: Vec<Value>, edges: Vec<(&str, &str)>) -> DatasetSession {
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":nodes,"edges":edges.iter().enumerate().map(|(i,(a,b))|
        json!({"id":format!("e{i}"),"source":a,"target":b})).collect::<Vec<_>>()}),
    )
    .unwrap();
    s
}
fn close(actual: Option<f64>, expected: f64) {
    assert!(
        (actual.unwrap() - expected).abs() < 1e-14,
        "{actual:?} versus {expected}"
    );
}
#[test]
fn known_geometry_and_complete_path_mean_are_distinct() {
    let mut s = session(
        vec![
            json!({"id":"a","embedding":[1,0]}),
            json!({"id":"b","embedding":[1,1]}),
            json!({"id":"z","embedding":[0,1]}),
        ],
        vec![("a", "b"), ("b", "z")],
    );
    let snapshot = s.snapshot().unwrap();
    let revision = s.revision();
    let r = execute(&s, &query("a", "z"), deadline()).unwrap();
    assert_eq!(r.result.path, vec!["a", "b", "z"]);
    close(r.semantic_similarity.value, 0.0);
    close(r.path_coherence_score.value, 1.0 / 2.0_f64.sqrt());
    assert_eq!(r.semantic_similarity.status, "computed");
    assert_eq!(r.path_coherence_score.status, "computed");
    assert!(
        r.semantic_similarity
            .reason
            .contains("model_space_not_verified")
    );
    let wire = serde_json::to_value(&r).unwrap();
    assert_eq!(wire["schema"], "semantica.explorer-path-similarity/1");
    assert_eq!(s.snapshot().unwrap(), snapshot);
    assert_eq!(s.revision(), revision);
}
#[test]
fn missing_invalid_and_mismatched_embeddings_are_not_zero() {
    for bad in [
        json!({"id":"b"}),
        json!({"id":"b","embedding":[]}),
        json!({"id":"b","embedding":[1]}),
        json!({"id":"b","embedding":[null,0]}),
    ] {
        let s = session(
            vec![
                json!({"id":"a","embedding":[1,0]}),
                bad,
                json!({"id":"z","embedding":[1,0]}),
            ],
            vec![("a", "b"), ("b", "z")],
        );
        let r = execute(&s, &query("a", "z"), deadline()).unwrap();
        close(r.semantic_similarity.value, 1.0);
        assert_eq!(r.path_coherence_score.status, "unavailable");
        assert_eq!(r.path_coherence_score.value, None);
        assert!(
            r.path_coherence_score
                .reason
                .starts_with("incomplete_adjacent_embeddings:")
        );
        assert!(serde_json::to_value(&r).unwrap()["path_coherence_score"]["value"].is_null());
    }
    let s = session(
        vec![
            json!({"id":"a","embedding":[1]}),
            json!({"id":"z","embedding":[1,0]}),
        ],
        vec![("a", "z")],
    );
    assert_eq!(
        execute(&s, &query("a", "z"), deadline())
            .unwrap()
            .semantic_similarity
            .reason,
        "embedding_dimension_mismatch"
    );
}
#[test]
fn embedding_aliases_and_extreme_scales_are_explicit() {
    for scale in [1.0, 1e308, 1e-308, 5e-324] {
        let s = session(
            vec![
                json!({"id":"a","metadata":{"semantic_embedding":{"values":[scale,scale]}}}),
                json!({"id":"z","properties":{"node2vec_embedding":[-scale,-scale]}}),
            ],
            vec![("a", "z")],
        );
        let r = execute(&s, &query("a", "z"), deadline()).unwrap();
        close(r.semantic_similarity.value, -1.0);
        close(r.path_coherence_score.value, -1.0);
    }
    let s = session(
        vec![
            json!({"id":"a","embedding":[0,0]}),
            json!({"id":"z","embedding":[1,0]}),
        ],
        vec![("a", "z")],
    );
    let r = execute(&s, &query("a", "z"), deadline()).unwrap();
    close(r.semantic_similarity.value, 0.0);
    assert_eq!(
        r.semantic_similarity.reason,
        "zero_vector_legacy_convention_not_similarity_evidence"
    );
    assert!(
        r.path_coherence_score
            .reason
            .contains("zero_vector_legacy_convention")
    );
    let s = session(
        vec![
            json!({"id":"a","embedding":null,"metadata":{"embedding":[1,0]}}),
            json!({"id":"z","embedding":[1,0]}),
        ],
        vec![("a", "z")],
    );
    assert_eq!(
        execute(&s, &query("a", "z"), deadline())
            .unwrap()
            .semantic_similarity
            .status,
        "unavailable"
    );
    let mut nested = json!([1, 0]);
    for _ in 0..18 {
        nested = json!({"values":nested});
    }
    let s = session(
        vec![
            json!({"id":"a","embedding":nested}),
            json!({"id":"z","embedding":[1,0]}),
        ],
        vec![("a", "z")],
    );
    close(
        execute(&s, &query("a", "z"), deadline())
            .unwrap()
            .semantic_similarity
            .value,
        1.0,
    );
}
#[test]
fn unreachable_identity_and_deadline_keep_their_meaning() {
    let mut s = session(
        vec![
            json!({"id":"a","embedding":[1,0]}),
            json!({"id":"z","embedding":[1,0]}),
        ],
        vec![],
    );
    let before = s.snapshot().unwrap();
    let r = execute(&s, &query("a", "z"), deadline()).unwrap();
    assert!(!r.result.reachable);
    assert_eq!(r.semantic_similarity.status, "not_applicable");
    assert_eq!(r.path_coherence_score.status, "not_applicable");
    let r = execute(&s, &query("a", "a"), deadline()).unwrap();
    close(r.semantic_similarity.value, 1.0);
    assert_eq!(r.path_coherence_score.status, "not_applicable");
    assert_eq!(r.path_coherence_score.reason, "no_adjacent_pairs");
    assert_eq!(
        execute(
            &s,
            &query("a", "a"),
            Instant::now() - Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string(),
        "explorer_path_deadline_exceeded"
    );
    assert_eq!(s.snapshot().unwrap(), before);
}

#[test]
fn original_coercion_is_admitted_but_nonfinite_does_not_fall_through() {
    for embedding in [
        json!([true, false]),
        json!(["1_0", "0"]),
        json!(["\u{2003}１\u{2003}", "０"]),
    ] {
        let s = session(
            vec![
                json!({"id":"a","embedding":embedding}),
                json!({"id":"z","embedding":[1,0]}),
            ],
            vec![("a", "z")],
        );
        close(
            execute(&s, &query("a", "z"), deadline())
                .unwrap()
                .semantic_similarity
                .value,
            1.0,
        );
    }
    for embedding in [json!(["NaN", 0]), json!(["Infinity", 0])] {
        let s = session(
            vec![
                json!({"id":"a","embedding":embedding,"vector":[1,0]}),
                json!({"id":"z","embedding":[1,0]}),
            ],
            vec![("a", "z")],
        );
        let r = execute(&s, &query("a", "z"), deadline()).unwrap();
        assert_eq!(r.semantic_similarity.status, "unavailable");
        assert_eq!(r.semantic_similarity.reason, "left:embedding_nonfinite");
    }
}
