use semantica_contracts::SemanticExplorerPathQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_path_bottleneck::execute,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn query(a: &str, b: &str, directed: bool) -> SemanticExplorerPathQuery {
    SemanticExplorerPathQuery {
        schema: "semantica.explorer-path-query/1".into(),
        source: a.into(),
        target: b.into(),
        algorithm: "dijkstra".into(),
        directed,
    }
}
fn session(n: usize, edges: &[(usize, usize, f64)]) -> DatasetSession {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s,&json!({"nodes":(0..n).map(|i|json!({"id":i.to_string()})).collect::<Vec<Value>>(),
        "edges":edges.iter().enumerate().map(|(i,(a,b,w))|json!({"id":format!("e{i}"),"source":a.to_string(),"target":b.to_string(),"weight":w})).collect::<Vec<Value>>()})).unwrap();
    s
}
#[test]
fn chain_ties_and_irrelevant_edges_follow_path_scope() {
    let mut s = session(
        6,
        &[
            (0, 1, 1.0),
            (1, 2, 1.0),
            (2, 3, 1.0),
            (1, 1, 1.0),
            (0, 1, 2.0),
            (1, 4, 10.0),
            (4, 5, 1.0),
        ],
    );
    let before = s.snapshot().unwrap();
    let revision = s.revision();
    let r = execute(&s, &query("0", "3", true), 10_000, deadline()).unwrap();
    assert_eq!(r.result.path, vec!["0", "1", "2", "3"]);
    assert_eq!(r.bottleneck_node.value, Some("1".into()));
    assert_eq!(
        r.bottleneck_node.reason,
        "exact_undirected_chain_betweenness"
    );
    assert_eq!(
        execute(&s, &query("3", "0", false), 10_000, deadline())
            .unwrap()
            .bottleneck_node
            .value,
        Some("2".into())
    );
    assert_eq!(s.snapshot().unwrap(), before);
    assert_eq!(s.revision(), revision);
}
// Deliberately independent oracle: enumerate every simple path, keep minimum
// length, then count internal occurrences using exact small integer fractions.
fn paths(adj: &[[bool; 5]; 5], target: usize, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    let node = *path.last().unwrap();
    if node == target {
        out.push(path.clone());
        return;
    }
    for next in 0..5 {
        if adj[node][next] && !path.contains(&next) {
            path.push(next);
            paths(adj, target, path, out);
            path.pop();
        }
    }
}
fn oracle(adj: &[[bool; 5]; 5]) -> usize {
    let mut scores = [0_u64; 5];
    for a in 0..5 {
        for b in a + 1..5 {
            let mut all = vec![];
            paths(adj, b, &mut vec![a], &mut all);
            let length = all.iter().map(Vec::len).min().unwrap();
            let shortest = all.iter().filter(|p| p.len() == length).collect::<Vec<_>>();
            assert_eq!(420 % shortest.len(), 0);
            for p in &shortest {
                for &i in &p[1..p.len() - 1] {
                    scores[i] += (420 / shortest.len()) as u64;
                }
            }
        }
    }
    let mut winner = 1;
    for i in 2..4 {
        if scores[i] > scores[winner] {
            winner = i;
        }
    }
    winner
}
#[test]
fn all_small_induced_graphs_match_exhaustive_shortest_path_oracle() {
    let chords = [(0, 2), (0, 3), (0, 4), (1, 3), (1, 4), (2, 4)];
    for mask in 0..64 {
        let mut edges = (0..4).map(|i| (i, i + 1, 1.0)).collect::<Vec<_>>();
        let mut adj = [[false; 5]; 5];
        for (i, &(a, b)) in chords.iter().enumerate() {
            if mask & (1 << i) != 0 {
                edges.push((a, b, 10.0));
            }
        }
        for &(a, b, _) in &edges {
            adj[a][b] = true;
            adj[b][a] = true;
        }
        let s = session(5, &edges);
        let r = execute(&s, &query("0", "4", true), 100_000, deadline()).unwrap();
        assert_eq!(r.result.path, vec!["0", "1", "2", "3", "4"]);
        assert_eq!(
            r.bottleneck_node.value,
            Some(oracle(&adj).to_string()),
            "mask={mask}"
        );
    }
}
#[test]
fn budget_exhaustion_never_publishes_partial_winner() {
    let s = session(4, &[(0, 1, 1.0), (1, 2, 1.0), (2, 3, 1.0), (0, 2, 10.0)]);
    let q = query("0", "3", true);
    let full = execute(&s, &q, 10_000, deadline()).unwrap();
    assert_eq!(full.bottleneck_node.status, "computed");
    for limit in 1..full.work_used {
        let r = execute(&s, &q, limit, deadline()).unwrap();
        assert_eq!(r.bottleneck_node.status, "unavailable");
        assert_eq!(r.bottleneck_node.value, None);
        assert_eq!(r.work_used, limit);
        assert_eq!(r.bottleneck_node.reason, "centrality_work_budget_exceeded");
    }
    for limit in [0, 10_000_001] {
        assert_eq!(
            execute(&s, &q, limit, deadline()).unwrap_err().to_string(),
            "explorer_path_bottleneck_budget"
        );
    }
}
#[test]
fn no_intermediate_unreachable_and_deadline_are_distinct() {
    let s = session(3, &[(0, 1, 1.0)]);
    for (target, reason) in [
        ("0", "no_intermediate_nodes"),
        ("1", "no_intermediate_nodes"),
        ("2", "path_unreachable"),
    ] {
        let r = execute(&s, &query("0", target, true), 100, deadline()).unwrap();
        assert_eq!(r.bottleneck_node.status, "not_applicable");
        assert_eq!(r.bottleneck_node.value, None);
        assert_eq!(r.bottleneck_node.reason, reason);
    }
    assert_eq!(
        execute(
            &s,
            &query("0", "1", true),
            100,
            Instant::now() - Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string(),
        "explorer_path_deadline_exceeded"
    );
}
