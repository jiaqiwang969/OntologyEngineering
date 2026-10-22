//! Exact unweighted betweenness on the UNDIRECTED induced path-node subgraph.
//! This graph statistic identifies no physical weak point or causal mechanism.
use crate::dataset::DatasetSession;
use anyhow::{Context, Result, ensure};
use num_bigint::BigInt;
use num_rational::BigRational;
use semantica_contracts::{
    SemanticExplorerNodeMetric, SemanticExplorerPathBottleneck, SemanticExplorerPathQuery,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};
const MAX_INTEGER_BITS: u64 = 4096;
fn metric(status: &str, value: Option<String>, reason: &str) -> SemanticExplorerNodeMetric {
    SemanticExplorerNodeMetric {
        status: status.into(),
        value,
        reason: reason.into(),
    }
}
struct Budget {
    remaining: u64,
    used: u64,
    deadline: Instant,
}
impl Budget {
    fn check(&self) -> Result<()> {
        ensure!(
            Instant::now() < self.deadline,
            "explorer_path_deadline_exceeded"
        );
        Ok(())
    }
    fn tick(&mut self) -> Result<bool> {
        self.check()?;
        if self.remaining == 0 {
            return Ok(false);
        }
        self.remaining -= 1;
        self.used += 1;
        Ok(true)
    }
    fn finish(&self, m: SemanticExplorerNodeMetric) -> Result<(SemanticExplorerNodeMetric, u64)> {
        self.check()?;
        Ok((m, self.used))
    }
    fn exhausted(&self) -> Result<(SemanticExplorerNodeMetric, u64)> {
        self.finish(metric(
            "unavailable",
            None,
            "centrality_work_budget_exceeded",
        ))
    }
}
fn bounded(value: &BigRational) -> bool {
    value.numer().bits() <= MAX_INTEGER_BITS && value.denom().bits() <= MAX_INTEGER_BITS
}
fn compute(
    adj: &[Vec<usize>],
    ids: &[String],
    budget: &mut Budget,
) -> Result<(SemanticExplorerNodeMetric, u64)> {
    let n = adj.len();
    if n < 3 {
        return budget.finish(metric("not_applicable", None, "no_intermediate_nodes"));
    }
    // For an induced simple chain, exact unnormalized centrality is i*(n-1-i).
    // Its first maximum is (n-1)/2. This avoids quadratic work on a50k-node chain.
    let mut chain = true;
    for (i, neighbors) in adj.iter().enumerate() {
        if !budget.tick()? {
            return budget.exhausted();
        }
        let expected = (if i > 0 { 1 } else { 0 }) + (if i + 1 < n { 1 } else { 0 });
        if neighbors.len() != expected || neighbors.iter().any(|&j| i.abs_diff(j) != 1) {
            chain = false;
            break;
        }
    }
    if chain {
        return budget.finish(metric(
            "computed",
            Some(ids[(n - 1) / 2].clone()),
            "exact_undirected_chain_betweenness",
        ));
    }
    // A simple ring is vertex-transitive: every node has exactly equal
    // betweenness. Verify ALL neighbor pairs against the selected path order;
    // degree2 alone would also admit disconnected cycles. The first intermediate
    // is therefore the exact tie winner, independent of ring length or weights.
    let mut cycle = true;
    for (i, neighbors) in adj.iter().enumerate() {
        if !budget.tick()? {
            return budget.exhausted();
        }
        if neighbors.len() != 2
            || !neighbors.contains(&((i + n - 1) % n))
            || !neighbors.contains(&((i + 1) % n))
        {
            cycle = false;
            break;
        }
    }
    if cycle {
        return budget.finish(metric(
            "computed",
            Some(ids[1].clone()),
            "exact_undirected_cycle_equal_betweenness",
        ));
    }
    // Brandes dependencies with exact rational arithmetic: ties are resolved by
    // original path order, not floating summation noise. Bit/work exhaustion is
    // explicit unavailable, never a partially accumulated winner.
    let zero = BigRational::from_integer(BigInt::from(0));
    let one = BigRational::from_integer(BigInt::from(1));
    let mut centrality = vec![zero.clone(); n];
    for source in 0..n {
        if !budget.tick()? {
            return budget.exhausted();
        }
        let mut predecessors = vec![Vec::new(); n];
        let mut distance = vec![usize::MAX; n];
        let mut sigma = vec![BigInt::from(0); n];
        sigma[source] = BigInt::from(1);
        distance[source] = 0;
        let mut queue = VecDeque::from([source]);
        let mut order = Vec::with_capacity(n);
        while let Some(v) = queue.pop_front() {
            if !budget.tick()? {
                return budget.exhausted();
            }
            order.push(v);
            for &w in &adj[v] {
                if !budget.tick()? {
                    return budget.exhausted();
                }
                if distance[w] == usize::MAX {
                    distance[w] = distance[v] + 1;
                    queue.push_back(w);
                }
                if distance[w] == distance[v] + 1 {
                    let added = sigma[v].clone();
                    sigma[w] += added;
                    if sigma[w].bits() > MAX_INTEGER_BITS {
                        return budget.finish(metric(
                            "unavailable",
                            None,
                            "centrality_exact_numeric_budget_exceeded",
                        ));
                    }
                    predecessors[w].push(v);
                }
            }
        }
        let mut dependency = vec![zero.clone(); n];
        for &w in order.iter().rev() {
            if !budget.tick()? {
                return budget.exhausted();
            }
            for &v in &predecessors[w] {
                if !budget.tick()? {
                    return budget.exhausted();
                }
                let added =
                    BigRational::new(sigma[v].clone(), sigma[w].clone()) * (&one + &dependency[w]);
                if !bounded(&added) {
                    return budget.finish(metric(
                        "unavailable",
                        None,
                        "centrality_exact_numeric_budget_exceeded",
                    ));
                }
                dependency[v] += added;
                if !bounded(&dependency[v]) {
                    return budget.finish(metric(
                        "unavailable",
                        None,
                        "centrality_exact_numeric_budget_exceeded",
                    ));
                }
            }
            if w != source {
                centrality[w] += &dependency[w];
                if !bounded(&centrality[w]) {
                    return budget.finish(metric(
                        "unavailable",
                        None,
                        "centrality_exact_numeric_budget_exceeded",
                    ));
                }
            }
        }
    }
    let mut winner = 1;
    for i in 2..n - 1 {
        if !budget.tick()? {
            return budget.exhausted();
        }
        if centrality[i] > centrality[winner] {
            winner = i;
        }
    }
    budget.finish(metric(
        "computed",
        Some(ids[winner].clone()),
        "exact_unweighted_undirected_induced_path_betweenness",
    ))
}
pub fn execute(
    session: &DatasetSession,
    query: &SemanticExplorerPathQuery,
    work_limit: u64,
    deadline: Instant,
) -> Result<SemanticExplorerPathBottleneck> {
    ensure!(
        (1..=10_000_000).contains(&work_limit),
        "explorer_path_bottleneck_budget"
    );
    let result = crate::explorer_path::execute(session, query, deadline)?;
    let records = crate::context::records(session)?;
    let (bottleneck_node, work_used) = from_records(&records, &result, work_limit, deadline)?;
    let output = SemanticExplorerPathBottleneck {
        schema: "semantica.explorer-path-bottleneck/1".into(),
        query: query.clone(),
        result,
        bottleneck_node,
        work_used,
    };
    ensure!(
        output.validate().is_ok(),
        "explorer_path_bottleneck_contract"
    );
    Ok(output)
}
pub(crate) fn from_records(
    records: &crate::context::Records,
    result: &semantica_contracts::SemanticExplorerPathResult,
    work_limit: u64,
    deadline: Instant,
) -> Result<(SemanticExplorerNodeMetric, u64)> {
    ensure!(
        (1..=10_000_000).contains(&work_limit),
        "explorer_path_bottleneck_budget"
    );
    let mut budget = Budget {
        remaining: work_limit,
        used: 0,
        deadline,
    };
    let (bottleneck_node, work_used) = if !result.reachable {
        budget.finish(metric("not_applicable", None, "path_unreachable"))?
    } else if result.path.len() < 3 {
        budget.finish(metric("not_applicable", None, "no_intermediate_nodes"))?
    } else {
        budget.check()?;
        let mut indices = BTreeMap::new();
        for (i, id) in result.path.iter().enumerate() {
            budget.check()?;
            indices.insert(id.as_str(), i);
        }
        let mut sets = vec![BTreeSet::new(); result.path.len()];
        let mut exhausted = false;
        for edge in &records.edges {
            if !budget.tick()? {
                exhausted = true;
                break;
            }
            let a = edge["source"]
                .as_str()
                .context("context_edge_source_required")?;
            let b = edge["target"]
                .as_str()
                .context("context_edge_target_required")?;
            if let (Some(&a), Some(&b)) = (indices.get(a), indices.get(b)) {
                if a != b {
                    sets[a].insert(b);
                    sets[b].insert(a);
                }
            }
        }
        if exhausted {
            budget.exhausted()?
        } else {
            let mut adj = Vec::with_capacity(sets.len());
            for set in sets {
                if !budget.tick()? {
                    exhausted = true;
                    break;
                }
                adj.push(set.into_iter().collect::<Vec<_>>());
            }
            if exhausted {
                budget.exhausted()?
            } else {
                compute(&adj, &result.path, &mut budget)?
            }
        }
    };
    budget.check()?;
    Ok((bottleneck_node, work_used))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_chain_uses_exact_linear_middle_and_respects_budget() {
        let n = 50_000;
        let ids = (0..n).map(|i| i.to_string()).collect::<Vec<_>>();
        let mut adj = vec![vec![]; n];
        for i in 0..n - 1 {
            adj[i].push(i + 1);
            adj[i + 1].push(i);
        }
        let mut budget = Budget {
            remaining: 100_000,
            used: 0,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
        };
        let (m, work) = compute(&adj, &ids, &mut budget).unwrap();
        assert_eq!(m.value, Some("24999".into()));
        assert_eq!(work, 50_000);
        let mut budget = Budget {
            remaining: 49_999,
            used: 0,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
        };
        assert_eq!(
            compute(&adj, &ids, &mut budget).unwrap().0.status,
            "unavailable"
        );
    }
    #[test]
    fn cycle_shortcut_checks_complete_topology_and_original_allowance() {
        let n = 50_000;
        let ids = (0..n).map(|i| i.to_string()).collect::<Vec<_>>();
        let mut adj = (0..n)
            .map(|i| vec![(i + n - 1) % n, (i + 1) % n])
            .collect::<Vec<_>>();
        let mut budget = Budget {
            remaining: 50_001,
            used: 0,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
        };
        let (m, used) = compute(&adj, &ids, &mut budget).unwrap();
        assert_eq!(m.value, Some("1".into()));
        assert_eq!(used, 50_001);
        assert_eq!(m.reason, "exact_undirected_cycle_equal_betweenness");
        let mut budget = Budget {
            remaining: 50_000,
            used: 0,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
        };
        assert_eq!(
            compute(&adj, &ids, &mut budget).unwrap().0.status,
            "unavailable"
        );
        // A late chord must invalidate the shortcut after its long valid prefix.
        adj[n - 2].push(2);
        adj[2].push(n - 2);
        let mut budget = Budget {
            remaining: 50_001,
            used: 0,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
        };
        let (m, _) = compute(&adj, &ids, &mut budget).unwrap();
        assert_eq!(m.status, "unavailable");
        // Two degree2 components must not be mistaken for one ring.
        let adj = vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
            vec![4, 5],
            vec![3, 5],
            vec![3, 4],
        ];
        let ids = (0..6).map(|i| i.to_string()).collect::<Vec<_>>();
        let mut budget = Budget {
            remaining: 10_000,
            used: 0,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
        };
        let (m, _) = compute(&adj, &ids, &mut budget).unwrap();
        assert_ne!(m.reason, "exact_undirected_cycle_equal_betweenness");
    }
}
