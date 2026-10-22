use semantica_contracts::{SemanticExplorerPathDetails, SemanticExplorerPathQuery};
use semantica_runtime::{
    context::load_context_graph,
    dataset::DatasetSession,
    explorer_path_details::{execute, validate},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn query(a: &str, z: &str) -> SemanticExplorerPathQuery {
    SemanticExplorerPathQuery {
        schema: "semantica.explorer-path-query/1".into(),
        source: a.into(),
        target: z.into(),
        algorithm: "dijkstra".into(),
        directed: true,
    }
}
fn session(nodes: Vec<Value>, edges: Vec<Value>) -> DatasetSession {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &json!({"nodes":nodes,"edges":edges})).unwrap();
    s
}
fn edge(id: &str, a: &str, b: &str, w: f64) -> Value {
    json!({"id":id,"source":a,"target":b,"weight":w})
}
fn fixture() -> DatasetSession {
    session(
        vec![
            json!({"id":"a","embedding":[1,0]}),
            json!({"id":"b","embedding":[1,1]}),
            json!({"id":"z","embedding":[0,1]}),
        ],
        vec![
            edge("ab-selected", "a", "b", 0.5),
            edge("ab-other", "a", "b", 9.0),
            edge("bz", "b", "z", 0.25),
            edge("az", "a", "z", 5.0),
        ],
    )
}
#[test]
fn composed_metrics_use_selected_edges_and_one_session() {
    let mut s = fixture();
    let before = s.snapshot().unwrap();
    let revision = s.revision();
    let q = query("a", "z");
    let d = execute(&s, &q, 100_000, deadline()).unwrap();
    assert_eq!(d.result.path, vec!["a", "b", "z"]);
    assert_eq!(d.result.total_weight, Some(0.75));
    assert_eq!(d.confidence_decay.value, Some(0.125));
    assert_eq!(d.semantic_similarity.value, Some(0.0));
    assert!((d.path_coherence_score.value.unwrap() - 1.0 / 2.0_f64.sqrt()).abs() < 1e-14);
    assert_eq!(d.bottleneck_node.value, Some("b".into()));
    assert_eq!(d.distance_band, Some("near".into()));
    assert_eq!(d.alternative_path_count.value, Some(1));
    assert!(!d.alternative_path_count.capped);
    assert_eq!(
        d.work_used,
        d.bottleneck_work_used + d.alternative_work_used
    );
    validate(&d, &q).unwrap();
    assert_eq!(s.snapshot().unwrap(), before);
    assert_eq!(s.revision(), revision);
    let mut reverse = query("z", "a");
    reverse.directed = false;
    assert_eq!(
        execute(&s, &reverse, 100_000, deadline())
            .unwrap()
            .confidence_decay
            .value,
        Some(0.125)
    );
    let nodes = (0..5).map(|i| json!({"id":i.to_string()})).collect();
    let mut edges = vec![];
    for a in 0..5 {
        for b in a + 1..5 {
            edges.push(edge(
                &format!("{a}-{b}"),
                &a.to_string(),
                &b.to_string(),
                if b == a + 1 { 1.0 } else { 10.0 },
            ));
        }
    }
    let s = session(nodes, edges);
    let d = execute(&s, &query("0", "4"), 100_000, deadline()).unwrap();
    assert_eq!(d.result.hop_count, Some(4));
    assert_eq!(d.alternative_path_count.value, Some(5));
    assert!(d.alternative_path_count.capped);
}
#[test]
fn unknown_metrics_and_shared_budget_remain_explicit() {
    let s = session(
        vec![
            json!({"id":"a","embedding":[1,0]}),
            json!({"id":"b"}),
            json!({"id":"z","embedding":[1,0]}),
        ],
        vec![edge("ab", "a", "b", 1.0), edge("bz", "b", "z", 1.0)],
    );
    let d = execute(&s, &query("a", "z"), 1, deadline()).unwrap();
    assert_eq!(d.work_used, 1);
    assert_eq!(d.semantic_similarity.value, Some(1.0));
    assert_eq!(d.path_coherence_score.status, "unavailable");
    assert_eq!(d.bottleneck_node.status, "unavailable");
    assert_eq!(d.alternative_path_count.status, "unavailable");
    assert_eq!(d.alternative_work_used, 0);
    assert_eq!(
        d.alternative_path_count.reason,
        "shared_work_budget_exhausted"
    );
    assert_eq!(d.confidence_decay.value, Some(1.0));
    for (weight, reason) in [
        (1e200, "selected_edge_product_overflow"),
        (1e-200, "selected_edge_product_underflow"),
    ] {
        let s = session(
            vec![json!({"id":"a"}), json!({"id":"b"}), json!({"id":"z"})],
            vec![edge("ab", "a", "b", weight), edge("bz", "b", "z", weight)],
        );
        let d = execute(&s, &query("a", "z"), 10_000, deadline()).unwrap();
        assert!(d.result.reachable);
        assert_eq!(d.confidence_decay.status, "unavailable");
        assert_eq!(d.confidence_decay.value, None);
        assert_eq!(d.confidence_decay.reason, reason);
    }
}
#[test]
fn distance_bands_unreachable_and_identity_are_consistent() {
    for (hops, band) in [
        (0, "direct"),
        (1, "direct"),
        (2, "near"),
        (3, "near"),
        (4, "mid-range"),
        (6, "mid-range"),
        (7, "distant"),
    ] {
        let s = session(
            (0..=hops)
                .map(|i| json!({"id":i.to_string(),"embedding":[1,0]}))
                .collect(),
            (0..hops)
                .map(|i| edge(&i.to_string(), &i.to_string(), &(i + 1).to_string(), 1.0))
                .collect(),
        );
        let d = execute(&s, &query("0", &hops.to_string()), 100_000, deadline()).unwrap();
        assert_eq!(d.distance_band.as_deref(), Some(band));
        assert_eq!(d.alternative_path_count.value, Some(0));
        if hops == 0 {
            assert_eq!(d.work_used, 0);
            assert_eq!(d.confidence_decay.value, Some(1.0));
            assert_eq!(d.path_coherence_score.status, "not_applicable");
        }
    }
    let s = session(vec![json!({"id":"a"}), json!({"id":"z"})], vec![]);
    let d = execute(&s, &query("a", "z"), 10, deadline()).unwrap();
    assert!(!d.result.reachable);
    assert_eq!(d.distance_band, None);
    assert_eq!(d.alternative_path_count.status, "not_applicable");
    assert_eq!(d.confidence_decay.status, "not_applicable");
    assert_eq!(d.work_used, 0);
    assert_eq!(
        execute(
            &s,
            &query("a", "z"),
            10,
            Instant::now() - Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string(),
        "explorer_path_deadline_exceeded"
    );
    for limit in [0, 10_000_001] {
        assert_eq!(
            execute(&s, &query("a", "z"), limit, deadline())
                .unwrap_err()
                .to_string(),
            "explorer_path_details_budget"
        );
    }
}
#[test]
fn wire_guards_reject_inconsistent_metrics_and_fabricated_interpretation() {
    let q = query("a", "z");
    let good = execute(&fixture(), &q, 100_000, deadline()).unwrap();
    let corruptions: Vec<fn(&mut SemanticExplorerPathDetails)> = vec![
        |d| d.semantic_similarity.status = "unavailable".into(),
        |d| d.semantic_similarity.value = None,
        |d| d.semantic_similarity.value = Some(2.0),
        |d| d.semantic_similarity.reason.clear(),
        |d| d.path_coherence_score.status = "not_applicable".into(),
        |d| d.confidence_decay.value = Some(f64::INFINITY),
        |d| d.bottleneck_node.value = Some("a".into()),
        |d| d.bottleneck_node.status = "unavailable".into(),
        |d| d.alternative_path_count.value = Some(99),
        |d| d.alternative_path_count.lower_bound = 2,
        |d| d.alternative_path_count.capped = true,
        |d| d.work_used += 1,
        |d| d.work_limit = 0,
        |d| d.distance_band = Some("distant".into()),
        |d| d.result.path[1].clear(),
        |d| d.result.edge_ids[1] = d.result.edge_ids[0].clone(),
        |d| d.query.source = "forged".into(),
        |d| d.result.schema = "forged".into(),
        |d| d.result.total_weight = Some(f64::NAN),
        |d| d.interpretation = "Engineering reliability verified".into(),
        |d| d.result.reachable = false,
    ];
    for (i, corrupt) in corruptions.iter().enumerate() {
        let mut bad = good.clone();
        corrupt(&mut bad);
        assert!(validate(&bad, &q).is_err(), "mutation={i}");
    }
    let wire = serde_json::to_vec(&good).unwrap();
    let decoded: SemanticExplorerPathDetails = serde_json::from_slice(&wire).unwrap();
    validate(&decoded, &q).unwrap();
}
