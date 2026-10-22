use semantica_contracts::SemanticExplorerPathQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_path_count::count,
};
use serde_json::json;
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn query(source: &str, target: &str, directed: bool) -> SemanticExplorerPathQuery {
    SemanticExplorerPathQuery {
        schema: "semantica.explorer-path-query/1".into(),
        source: source.into(),
        target: target.into(),
        algorithm: "bfs".into(),
        directed,
    }
}
fn session(edges: &[(usize, usize)]) -> DatasetSession {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s,&json!({"nodes":(0..4).map(|i|json!({"id":i.to_string()})).collect::<Vec<_>>(),
        "edges":edges.iter().enumerate().map(|(i,(a,b))|json!({"id":format!("e{i}"),"source":a.to_string(),"target":b.to_string()})).collect::<Vec<_>>()})).unwrap();
    s
}
#[test]
fn capped_counts_match_independent_literal_path_catalogue() {
    // Every simple 0→3 path on these four vertices appears in this catalogue.
    // No traversal algorithm is used by the oracle.
    let catalogue = [
        vec![0, 3],
        vec![0, 1, 3],
        vec![0, 2, 3],
        vec![0, 1, 2, 3],
        vec![0, 2, 1, 3],
    ];
    let edges = [
        (0, 1),
        (1, 0),
        (0, 2),
        (1, 2),
        (2, 1),
        (1, 3),
        (2, 3),
        (0, 3),
    ];
    for mask in 0..256 {
        let present = edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| ((mask >> i) & 1 == 1).then_some(*e))
            .collect::<Vec<_>>();
        let s = session(&present);
        for directed in [true, false] {
            let expected = catalogue
                .iter()
                .filter(|p| {
                    p.windows(2).all(|w| {
                        present.contains(&(w[0], w[1]))
                            || (!directed && present.contains(&(w[1], w[0])))
                    })
                })
                .count();
            for cap in [1, 2, 6] {
                let result =
                    count(&s, &query("0", "3", directed), cap, 10_000, deadline()).unwrap();
                assert_eq!(
                    result.count,
                    Some(expected.min(cap)),
                    "mask={mask},directed={directed},cap={cap}"
                );
                assert_eq!(result.lower_bound, expected.min(cap));
                assert_eq!(result.capped, expected >= cap);
            }
        }
    }
}
#[test]
fn multiedges_cycles_direction_and_identity_do_not_invent_paths() {
    let mut s = session(&[(0, 1), (0, 1), (1, 0), (1, 1), (1, 3), (3, 3)]);
    let before = s.snapshot().unwrap();
    let revision = s.revision();
    assert_eq!(
        count(&s, &query("0", "3", true), 9, 10_000, deadline())
            .unwrap()
            .count,
        Some(1)
    );
    assert_eq!(
        count(&s, &query("3", "0", true), 9, 10_000, deadline())
            .unwrap()
            .count,
        Some(0)
    );
    assert_eq!(
        count(&s, &query("3", "0", false), 9, 10_000, deadline())
            .unwrap()
            .count,
        Some(1)
    );
    assert_eq!(
        count(&s, &query("0", "0", true), 9, 10_000, deadline())
            .unwrap()
            .count,
        Some(1)
    );
    assert_eq!(s.snapshot().unwrap(), before);
    assert_eq!(s.revision(), revision);
}
#[test]
fn incomplete_count_is_unknown_and_deadline_remains_a_rejection() {
    let mut s = session(&[(0, 1), (1, 3), (0, 2), (2, 3)]);
    let before = s.snapshot().unwrap();
    let full = count(&s, &query("0", "3", true), 9, 10_000, deadline()).unwrap();
    assert_eq!(full.count, Some(2));
    let mut observed_partial = false;
    for limit in 1..full.work_used {
        let result = count(&s, &query("0", "3", true), 9, limit, deadline()).unwrap();
        assert_eq!(result.count, None);
        assert!(!result.capped);
        assert!(result.lower_bound <= 2);
        assert_eq!(result.work_used, limit);
        observed_partial |= result.lower_bound > 0;
    }
    assert!(observed_partial);
    assert_eq!(
        count(
            &s,
            &query("0", "3", true),
            9,
            10_000,
            Instant::now() - Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string(),
        "explorer_path_deadline_exceeded"
    );
    for (cap, limit) in [(0, 1), (50_003, 1), (1, 0), (1, 10_000_001)] {
        assert_eq!(
            count(&s, &query("0", "3", true), cap, limit, deadline())
                .unwrap_err()
                .to_string(),
            "explorer_path_count_budget"
        );
    }
    assert_eq!(
        count(&s, &query("missing", "3", true), 9, 10_000, deadline())
            .unwrap_err()
            .to_string(),
        "explorer_path_source_missing"
    );
    assert_eq!(
        count(&s, &query("0", "missing", true), 9, 10_000, deadline())
            .unwrap_err()
            .to_string(),
        "explorer_path_target_missing"
    );
    assert_eq!(
        count(
            &DatasetSession::new(),
            &query("0", "3", true),
            9,
            10_000,
            deadline()
        )
        .unwrap_err()
        .to_string(),
        "context_projection_unavailable"
    );
    assert_eq!(s.snapshot().unwrap(), before);
}
