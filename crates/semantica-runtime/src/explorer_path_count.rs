//! Bounded counting of distinct simple NODE paths. Parallel edges do not add paths.
//! This supplies the count component of the legacy k-shortest-path display only:
//! no ordering, path enumeration, confidence, or engineering adoption is claimed.
use crate::dataset::DatasetSession;
use anyhow::{Context, Result, ensure};
use oxigraph::model::GraphNameRef;
use semantica_contracts::SemanticExplorerPathQuery;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

/// Internal algorithm result, not a wire protocol. A missing count is unfinished,
/// never zero. When capped, the exact total number of paths remains unknown.
#[derive(Debug, PartialEq, Eq)]
pub struct PathCount {
    pub count: Option<usize>,
    pub lower_bound: usize,
    pub capped: bool,
    pub work_used: u64,
}
struct Budget {
    remaining: u64,
    used: u64,
    deadline: Instant,
}
impl Budget {
    fn tick(&mut self) -> Result<bool> {
        ensure!(
            Instant::now() < self.deadline,
            "explorer_path_deadline_exceeded"
        );
        if self.remaining == 0 {
            return Ok(false);
        }
        self.remaining -= 1;
        self.used += 1;
        Ok(true)
    }
    fn result(&self, count: usize, complete: bool, capped: bool) -> Result<PathCount> {
        ensure!(
            Instant::now() < self.deadline,
            "explorer_path_deadline_exceeded"
        );
        Ok(PathCount {
            count: complete.then_some(count),
            lower_bound: count,
            capped,
            work_used: self.used,
        })
    }
}
/// A cap of shortest-path hops + 2 matches the original DISPLAY count request;
/// the caller subtracts the selected path only when count is known and positive.
/// Counting any simple paths until cap gives the same capped cardinality as
/// selecting cap shortest simple paths. It does not reproduce their order.
/// Work limit is explicit host policy; deadline is the caller's original deadline.
pub fn count(
    session: &DatasetSession,
    query: &SemanticExplorerPathQuery,
    cap: usize,
    work_limit: u64,
    deadline: Instant,
) -> Result<PathCount> {
    ensure!(query.validate().is_ok(), "explorer_path_query_contract");
    ensure!(
        (1..=50_002).contains(&cap) && (1..=10_000_000).contains(&work_limit),
        "explorer_path_count_budget"
    );
    ensure!(
        !query.source.is_empty()
            && !query.target.is_empty()
            && query
                .source
                .len()
                .checked_add(query.target.len())
                .is_some_and(|n| n <= session.dataset_byte_limit()),
        "explorer_path_identifier_budget"
    );
    let mut budget = Budget {
        remaining: work_limit,
        used: 0,
        deadline,
    };
    budget.tick()?;
    ensure!(
        session.dataset().iter().any(|q| matches!(q.graph_name,
        GraphNameRef::NamedNode(n) if n.as_str()==crate::context::CONTEXT_GRAPH)),
        "context_projection_unavailable"
    );
    let records = crate::context::records(session)?;
    count_records(&records, query, cap, budget)
}
pub(crate) fn from_records(
    records: &crate::context::Records,
    query: &SemanticExplorerPathQuery,
    cap: usize,
    work_limit: u64,
    deadline: Instant,
) -> Result<PathCount> {
    ensure!(
        (1..=50_002).contains(&cap) && (1..=10_000_000).contains(&work_limit),
        "explorer_path_count_budget"
    );
    let mut budget = Budget {
        remaining: work_limit,
        used: 0,
        deadline,
    };
    budget.tick()?;
    count_records(records, query, cap, budget)
}
fn count_records(
    records: &crate::context::Records,
    query: &SemanticExplorerPathQuery,
    cap: usize,
    mut budget: Budget,
) -> Result<PathCount> {
    let mut indices = BTreeMap::new();
    for node in &records.nodes {
        if !budget.tick()? {
            return budget.result(0, false, false);
        }
        let id = node["id"].as_str().context("context_node_id_required")?;
        let index = indices.len();
        ensure!(
            indices.insert(id, index).is_none(),
            "context_duplicate_node_id"
        );
    }
    let source = *indices
        .get(query.source.as_str())
        .context("explorer_path_source_missing")?;
    let target = *indices
        .get(query.target.as_str())
        .context("explorer_path_target_missing")?;
    let mut sets = vec![BTreeSet::new(); indices.len()];
    for edge in &records.edges {
        if !budget.tick()? {
            return budget.result(0, false, false);
        }
        let from = *indices
            .get(
                edge["source"]
                    .as_str()
                    .context("context_edge_source_required")?,
            )
            .context("context_projection_edge_endpoint_missing")?;
        let to = *indices
            .get(
                edge["target"]
                    .as_str()
                    .context("context_edge_target_required")?,
            )
            .context("context_projection_edge_endpoint_missing")?;
        // Ignore loops: a simple node path cannot revisit its current node.
        if from != to {
            sets[from].insert(to);
            if !query.directed {
                sets[to].insert(from);
            }
        }
    }
    let mut adjacency = Vec::with_capacity(sets.len());
    for set in sets {
        if !budget.tick()? {
            return budget.result(0, false, false);
        }
        adjacency.push(set.into_iter().collect::<Vec<_>>());
    }
    enumerate(&adjacency, source, target, cap, &mut budget)
}
fn enumerate(
    adj: &[Vec<usize>],
    source: usize,
    target: usize,
    cap: usize,
    budget: &mut Budget,
) -> Result<PathCount> {
    if !budget.tick()? {
        return budget.result(0, false, false);
    }
    if source == target {
        return budget.result(1, true, cap == 1);
    }
    // Reverse reachability prunes dead branches; it does not decide simple-path
    // reachability after excluding the current prefix. Exhaustion stays unknown.
    let mut reverse = vec![Vec::new(); adj.len()];
    for (from, neighbors) in adj.iter().enumerate() {
        for &to in neighbors {
            if !budget.tick()? {
                return budget.result(0, false, false);
            }
            reverse[to].push(from);
        }
    }
    let mut reachable = vec![false; adj.len()];
    reachable[target] = true;
    let mut pending = vec![target];
    while let Some(node) = pending.pop() {
        for &previous in &reverse[node] {
            if !budget.tick()? {
                return budget.result(0, false, false);
            }
            if !reachable[previous] {
                reachable[previous] = true;
                pending.push(previous);
            }
        }
    }
    if !reachable[source] {
        return budget.result(0, true, false);
    }
    // Explicit frames prevent a 50k-node chain from overflowing the Rust stack.
    let mut visited = vec![false; adj.len()];
    visited[source] = true;
    let mut stack = vec![(source, 0usize)];
    let mut found = 0;
    while let Some((node, next)) = stack.last_mut() {
        if !budget.tick()? {
            return budget.result(found, false, false);
        }
        if *next == adj[*node].len() {
            visited[*node] = false;
            stack.pop();
            continue;
        }
        let candidate = adj[*node][*next];
        *next += 1;
        if !reachable[candidate] || visited[candidate] {
            continue;
        }
        if candidate == target {
            found += 1;
            if found == cap {
                return budget.result(found, true, true);
            }
        } else {
            visited[candidate] = true;
            stack.push((candidate, 0));
        }
    }
    budget.result(found, true, false)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn fifty_thousand_node_chain_is_iterative_and_budgeted() {
        let mut adj = vec![vec![]; 50_000];
        for (i, edges) in adj.iter_mut().enumerate().take(49_999) {
            edges.push(i + 1);
        }
        let mut budget = Budget {
            remaining: 1_000_000,
            used: 0,
            deadline: Instant::now() + Duration::from_secs(10),
        };
        let result = enumerate(&adj, 0, 49_999, 50_001, &mut budget).unwrap();
        assert_eq!(result.count, Some(1));
        assert!(!result.capped);
        let mut budget = Budget {
            remaining: 1,
            used: 0,
            deadline: Instant::now() + Duration::from_secs(10),
        };
        assert_eq!(
            enumerate(&adj, 0, 49_999, 50_001, &mut budget)
                .unwrap()
                .count,
            None
        );
    }
}
