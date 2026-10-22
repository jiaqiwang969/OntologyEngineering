//! Shortest paths over a validated, disposable view of the sole DatasetSession.
//! Reachability and graph weights do not establish engineering causality or confidence.
use anyhow::{Context, Result, ensure};
use oxigraph::model::GraphNameRef;
use petgraph::{Directed, Graph, algo::astar};
use semantica_contracts::{SemanticExplorerPathQuery, SemanticExplorerPathResult};
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone)]
struct Edge {
    id: String,
    weight: f64,
}
fn within(deadline: Instant) -> Result<()> {
    ensure!(Instant::now() < deadline, "explorer_path_deadline_exceeded");
    Ok(())
}

/// Cooperative deadline checks refuse late results. A host must supply process
/// supervision when it requires preemption; this function does not claim it.
/// Equal-cost alternatives use a deterministic native graph traversal convention,
/// not the legacy adapter's broken dictionary-as-graph behavior.
pub fn execute(
    session: &crate::dataset::DatasetSession,
    query: &SemanticExplorerPathQuery,
    deadline: Instant,
) -> Result<SemanticExplorerPathResult> {
    let records = prepare(session, query, deadline)?;
    execute_records(&records, query, deadline)
}
pub(crate) fn prepare(
    session: &crate::dataset::DatasetSession,
    query: &SemanticExplorerPathQuery,
    deadline: Instant,
) -> Result<crate::context::Records> {
    within(deadline)?;
    ensure!(query.validate().is_ok(), "explorer_path_query_contract");
    ensure!(
        !query.source.is_empty()
            && !query.target.is_empty()
            && query
                .source
                .len()
                .checked_add(query.target.len())
                .is_some_and(|bytes| bytes <= session.dataset_byte_limit()),
        "explorer_path_identifier_budget"
    );
    ensure!(
        session.dataset().iter().any(|q| matches!(q.graph_name,
        GraphNameRef::NamedNode(n) if n.as_str()==crate::context::CONTEXT_GRAPH)),
        "context_projection_unavailable"
    );
    let mut records = crate::context::records(session)?;
    within(deadline)?;
    records
        .nodes
        .sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    records
        .edges
        .sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    within(deadline)?;
    Ok(records)
}
pub(crate) fn execute_records(
    records: &crate::context::Records,
    query: &SemanticExplorerPathQuery,
    deadline: Instant,
) -> Result<SemanticExplorerPathResult> {
    within(deadline)?;
    let mut graph = Graph::<String, Edge, Directed>::new();
    let mut indices = BTreeMap::new();
    for node in &records.nodes {
        within(deadline)?;
        let id = node["id"]
            .as_str()
            .context("context_node_id_required")?
            .to_owned();
        let index = graph.add_node(id.clone());
        ensure!(
            indices.insert(id, index).is_none(),
            "context_duplicate_node_id"
        );
    }
    let source = *indices
        .get(&query.source)
        .context("explorer_path_source_missing")?;
    let target = *indices
        .get(&query.target)
        .context("explorer_path_target_missing")?;
    for edge in &records.edges {
        within(deadline)?;
        let id = edge["id"]
            .as_str()
            .context("context_edge_id_required")?
            .to_owned();
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
        let weight = match edge.get("weight") {
            None => 1.0,
            Some(value) => value.as_f64().context("explorer_path_weight_invalid")?,
        };
        ensure!(weight.is_finite(), "explorer_path_weight_invalid");
        ensure!(
            query.algorithm != "dijkstra" || weight >= 0.0,
            "explorer_path_negative_weight"
        );
        let edge = Edge { id, weight };
        graph.add_edge(from, to, edge.clone());
        if !query.directed && from != to {
            graph.add_edge(to, from, edge);
        }
    }
    within(deadline)?;
    // A zero heuristic gives Dijkstra; unit costs give the BFS hop minimum.
    // Checking in is_goal lets an expired traversal exit without publishing its
    // partial result. The mandatory check below discards that artificial goal.
    let found = astar(
        &graph,
        source,
        |node| node == target || Instant::now() >= deadline,
        |edge| {
            if query.algorithm == "bfs" {
                1.0
            } else {
                edge.weight().weight
            }
        },
        |_| 0.0,
    );
    within(deadline)?;
    let Some((cost, nodes)) = found else {
        return Ok(SemanticExplorerPathResult {
            schema: "semantica.explorer-path-result/1".into(),
            reachable: false,
            path: vec![],
            edge_ids: vec![],
            hop_count: None,
            total_weight: None,
        });
    };
    ensure!(cost.is_finite(), "explorer_path_cost_overflow");
    let mut edge_ids = Vec::new();
    let mut total = 0.0f64;
    for pair in nodes.windows(2) {
        within(deadline)?;
        let edge = graph
            .edges_connecting(pair[0], pair[1])
            .min_by(|a, b| {
                let order = if query.algorithm == "dijkstra" {
                    a.weight().weight.total_cmp(&b.weight().weight)
                } else {
                    std::cmp::Ordering::Equal
                };
                order.then_with(|| a.weight().id.cmp(&b.weight().id))
            })
            .context("explorer_path_edge_missing")?;
        total += edge.weight().weight;
        ensure!(total.is_finite(), "explorer_path_cost_overflow");
        edge_ids.push(edge.weight().id.clone());
    }
    let result = SemanticExplorerPathResult {
        schema: "semantica.explorer-path-result/1".into(),
        reachable: true,
        path: nodes.iter().map(|index| graph[*index].clone()).collect(),
        hop_count: Some(edge_ids.len() as u64),
        edge_ids,
        total_weight: Some(total),
    };
    within(deadline)?;
    ensure!(result.validate().is_ok(), "explorer_path_result_contract");
    Ok(result)
}
