use semantica_contracts::SemanticExplorerPathQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_path::execute,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn session(nodes: &[&str], edges: Vec<Value>) -> DatasetSession {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s,&json!({"nodes":nodes.iter().map(|id|json!({"id":id,"type":"Part"})).collect::<Vec<_>>(),"edges":edges})).unwrap();
    s
}
fn edge(id: &str, from: &str, to: &str, weight: f64) -> Value {
    json!({"id":id,"source":from,"target":to,"type":"LINKS","weight":weight})
}
fn query(from: &str, to: &str, algorithm: &str, directed: bool) -> SemanticExplorerPathQuery {
    SemanticExplorerPathQuery {
        schema: "semantica.explorer-path-query/1".into(),
        source: from.into(),
        target: to.into(),
        algorithm: algorithm.into(),
        directed,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
#[test]
fn original_adapter_failure_has_a_native_same_session_path() {
    // Frozen original HTTP gave404 Source node a not found for this connected graph.
    let mut s = session(
        &["b", "a", "z"],
        vec![edge("edge-b", "b", "z", 0.5), edge("edge-a", "a", "b", 1.0)],
    );
    let before = s.snapshot().unwrap();
    let revision = s.revision();
    for algorithm in ["bfs", "dijkstra"] {
        let result = execute(&s, &query("a", "z", algorithm, true), deadline()).unwrap();
        assert!(result.reachable);
        assert_eq!(result.path, vec!["a", "b", "z"]);
        assert_eq!(result.edge_ids, vec!["edge-a", "edge-b"]);
        assert_eq!(result.hop_count, Some(2));
        assert_eq!(result.total_weight, Some(1.5));
    }
    assert_eq!(s.snapshot().unwrap(), before);
    assert_eq!(s.revision(), revision);
}
#[test]
fn hop_and_weight_minima_and_direction_are_distinct() {
    let s = session(
        &["a", "b", "z"],
        vec![
            edge("direct", "a", "z", 10.0),
            edge("ab", "a", "b", 1.0),
            edge("bz", "b", "z", 2.0),
        ],
    );
    let hops = execute(&s, &query("a", "z", "bfs", true), deadline()).unwrap();
    assert_eq!(hops.path, vec!["a", "z"]);
    assert_eq!(hops.total_weight, Some(10.0));
    let weighted = execute(&s, &query("a", "z", "dijkstra", true), deadline()).unwrap();
    assert_eq!(weighted.path, vec!["a", "b", "z"]);
    assert_eq!(weighted.total_weight, Some(3.0));
    let none = execute(&s, &query("z", "a", "dijkstra", true), deadline()).unwrap();
    assert!(!none.reachable);
    assert!(none.path.is_empty() && none.edge_ids.is_empty());
    assert_eq!(none.hop_count, None);
    assert_eq!(none.total_weight, None);
    let reverse = execute(&s, &query("z", "a", "dijkstra", false), deadline()).unwrap();
    assert_eq!(reverse.path, vec!["z", "b", "a"]);
    assert_eq!(reverse.edge_ids, vec!["bz", "ab"]);
    assert_eq!(reverse.total_weight, Some(3.0));
}
#[test]
fn parallel_edges_zero_cycles_and_identity_path_are_consistent() {
    let s = session(
        &["a", "b", "z"],
        vec![
            edge("expensive", "a", "b", 8.0),
            edge("cheap", "a", "b", 0.0),
            edge("back", "b", "a", 0.0),
            edge("bz", "b", "z", 1.0),
        ],
    );
    let result = execute(&s, &query("a", "z", "dijkstra", true), deadline()).unwrap();
    assert_eq!(result.path, vec!["a", "b", "z"]);
    assert_eq!(result.edge_ids, vec!["cheap", "bz"]);
    assert_eq!(result.total_weight, Some(1.0));
    let identity = execute(&s, &query("a", "a", "dijkstra", true), deadline()).unwrap();
    assert!(identity.reachable);
    assert_eq!(identity.path, vec!["a"]);
    assert!(identity.edge_ids.is_empty());
    assert_eq!(identity.hop_count, Some(0));
    assert_eq!(identity.total_weight, Some(0.0));
}
#[test]
fn invalid_endpoints_weights_and_deadlines_do_not_change_session() {
    let mut s = session(
        &["a", "b", "z"],
        vec![edge("ab", "a", "b", -1.0), edge("bz", "b", "z", 1.0)],
    );
    let before = s.snapshot().unwrap();
    let revision = s.revision();
    for (q, time, error) in [
        (
            query("missing", "z", "bfs", true),
            deadline(),
            "explorer_path_source_missing",
        ),
        (
            query("a", "missing", "bfs", true),
            deadline(),
            "explorer_path_target_missing",
        ),
        (
            query("a", "z", "dijkstra", true),
            deadline(),
            "explorer_path_negative_weight",
        ),
        (
            query("a", "z", "other", true),
            deadline(),
            "explorer_path_query_contract",
        ),
        (
            query("a", "z", "bfs", true),
            Instant::now() - Duration::from_secs(1),
            "explorer_path_deadline_exceeded",
        ),
    ] {
        assert_eq!(execute(&s, &q, time).unwrap_err().to_string(), error);
    }
    assert_eq!(
        execute(&s, &query("a", "z", "bfs", true), deadline())
            .unwrap()
            .total_weight,
        Some(0.0)
    );
    assert_eq!(s.snapshot().unwrap(), before);
    assert_eq!(s.revision(), revision);
    let overflow = session(
        &["a", "b", "z"],
        vec![edge("ab", "a", "b", 1e308), edge("bz", "b", "z", 1e308)],
    );
    for algorithm in ["bfs", "dijkstra"] {
        assert_eq!(
            execute(&overflow, &query("a", "z", algorithm, true), deadline())
                .unwrap_err()
                .to_string(),
            "explorer_path_cost_overflow"
        );
    }
    let invalid = session(
        &["a", "z"],
        vec![json!({"id":"az","source":"a","target":"z","weight":"NaN"})],
    );
    assert_eq!(
        execute(&invalid, &query("a", "z", "bfs", true), deadline())
            .unwrap_err()
            .to_string(),
        "explorer_path_weight_invalid"
    );
    assert_eq!(
        execute(
            &DatasetSession::new(),
            &query("a", "z", "bfs", true),
            deadline()
        )
        .unwrap_err()
        .to_string(),
        "context_projection_unavailable"
    );
}
// Independent exhaustive simple-path oracle on four vertices, not another
// shortest-path implementation. Nonnegative costs admit a simple optimal path.
fn enumerate(
    edges: &[(usize, usize, f64)],
    at: usize,
    target: usize,
    seen: u8,
    cost: f64,
    hops: u64,
    directed: bool,
    out: &mut Vec<(f64, u64)>,
) {
    if at == target {
        out.push((cost, hops));
        return;
    }
    for &(a, b, w) in edges {
        let next = if a == at {
            Some(b)
        } else if !directed && b == at {
            Some(a)
        } else {
            None
        };
        if let Some(next) = next {
            if seen & (1 << next) == 0 {
                enumerate(
                    edges,
                    next,
                    target,
                    seen | (1 << next),
                    cost + w,
                    hops + 1,
                    directed,
                    out,
                );
            }
        }
    }
}
#[test]
fn tiny_graph_minima_match_independent_exhaustive_paths() {
    let candidates = [
        (0, 1, 0.0),
        (1, 0, 0.0),
        (0, 2, 3.0),
        (1, 2, 1.0),
        (1, 3, 5.0),
        (2, 1, 2.0),
        (2, 3, 2.0),
        (3, 0, 4.0),
    ];
    let ids = ["a", "b", "c", "d"];
    for mask in 0..256u16 {
        let edges: Vec<_> = candidates
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, e)| *e)
            .collect();
        let s = session(
            &ids,
            edges
                .iter()
                .enumerate()
                .map(|(i, (a, b, w))| edge(&format!("e{i}"), ids[*a], ids[*b], *w))
                .collect(),
        );
        for directed in [true, false] {
            for (source, target) in [(0, 3), (3, 0)] {
                let mut paths = vec![];
                enumerate(
                    &edges,
                    source,
                    target,
                    1 << source,
                    0.0,
                    0,
                    directed,
                    &mut paths,
                );
                for algorithm in ["bfs", "dijkstra"] {
                    let result = execute(
                        &s,
                        &query(ids[source], ids[target], algorithm, directed),
                        deadline(),
                    )
                    .unwrap();
                    assert_eq!(result.reachable, !paths.is_empty(), "mask={mask}");
                    if !paths.is_empty() {
                        assert_eq!(result.path.first().unwrap(), ids[source]);
                        assert_eq!(result.path.last().unwrap(), ids[target]);
                        if algorithm == "bfs" {
                            assert_eq!(result.hop_count, paths.iter().map(|x| x.1).min());
                        } else {
                            assert_eq!(
                                result.total_weight,
                                paths.iter().map(|x| x.0).min_by(f64::total_cmp)
                            );
                        }
                        let mut sum = 0.0;
                        for (pair, id) in result.path.windows(2).zip(&result.edge_ids) {
                            let i: usize = id.strip_prefix('e').unwrap().parse().unwrap();
                            let (a, b, w) = edges[i];
                            assert!(
                                (pair[0] == ids[a] && pair[1] == ids[b])
                                    || (!directed && pair[0] == ids[b] && pair[1] == ids[a])
                            );
                            sum += w;
                        }
                        assert_eq!(result.total_weight, Some(sum));
                        assert_eq!(result.path.len(), result.edge_ids.len() + 1);
                    }
                }
            }
        }
    }
}

#[test]
fn identifiers_respect_existing_context_admission_and_session_budget() {
    let id = "轴".repeat(170) + "ab";
    assert_eq!(id.len(), 512);
    let mut s = session(&[&id, "z"], vec![edge("long-id-edge", &id, "z", 2.0)]);
    let before = s.snapshot().unwrap();
    let result = execute(&s, &query(&id, "z", "dijkstra", true), deadline()).unwrap();
    assert_eq!(result.path, vec![id, "z".into()]);
    assert_eq!(result.total_weight, Some(2.0));
    assert_eq!(s.snapshot().unwrap(), before);
    let mut refused = DatasetSession::new();
    let empty = refused.snapshot().unwrap();
    assert_eq!(
        load_context_graph(
            &mut refused,
            &json!({"nodes":[{"id":"x".repeat(513),"type":"Part"}],"edges":[]})
        )
        .unwrap_err()
        .to_string(),
        "context_node_identity_invalid"
    );
    assert_eq!(refused.snapshot().unwrap(), empty);
    let oversized = "x".repeat(s.dataset_byte_limit());
    assert_eq!(
        execute(&s, &query(&oversized, "z", "bfs", true), deadline())
            .unwrap_err()
            .to_string(),
        "explorer_path_identifier_budget"
    );
    assert_eq!(s.snapshot().unwrap(), before);
}
