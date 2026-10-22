//! Original Explorer search behavior over the sole DatasetSession.
//! Transport authentication and source guards belong to the existing service.
use anyhow::{Context, Result, ensure};
use semantica_contracts::{SemanticExplorerSearchQuery, SemanticExplorerSearchResult};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};
fn within(d: Instant) -> Result<()> {
    ensure!(Instant::now() < d, "explorer_search_deadline_exceeded");
    Ok(())
}

/// Original model admission, projected only onto fields the search uses.
/// Raw body stays in the generated request; this grants no service authority.
pub fn validate_request(body: &Value) -> Result<Value> {
    crate::schema::RustSchemaRuntime::new()
        .map_err(|e| anyhow::anyhow!("{}", e.code))?
        .validate_search_execution_fields(body)
        .map_err(|e| anyhow::anyhow!("{}", e.code))
}

pub fn execute(
    session: &mut crate::dataset::DatasetSession,
    q: &SemanticExplorerSearchQuery,
    deadline: Instant,
) -> Result<SemanticExplorerSearchResult> {
    within(deadline)?;
    ensure!(q.validate().is_ok(), "explorer_search_query_contract");
    ensure!(
        serde_json::to_vec(q)?.len() <= session.dataset_byte_limit(),
        "explorer_search_query_budget"
    );
    let validation = validate_request(&q.body)?;
    ensure!(
        validation["status"] == "validated",
        "explorer_search_request_model_invalid"
    );
    let b = &validation["output"];
    let query = b["query"]
        .as_str()
        .context("explorer_search_query_required")?;
    let limit = b["limit"]
        .as_u64()
        .context("explorer_search_limit_required")? as usize;
    ensure!((1..=200).contains(&limit), "explorer_search_limit_bounds");
    let records = crate::context::records(session)?;
    let matches = session.explorer_search_matches(query, limit, &b["filters"], deadline)?;
    let nodes: BTreeMap<_, _> = records
        .nodes
        .iter()
        .map(|n| (n["id"].as_str().unwrap_or(""), n))
        .collect();
    let anchor = b["anchor_node"].as_str().filter(|s| !s.is_empty());
    let mut hops = BTreeMap::new();
    let explicit = !b["max_hops"].is_null();
    let text = b["max_hops"].to_string();
    let negative = explicit && text.starts_with('-');
    // The graph is finite and visited once. Saturating an unbounded positive
    // request at usize::MAX changes no reachable hop; its original value stays
    // in the admitted request. Negative values retain the original exclusion.
    let depth = if !explicit {
        10
    } else if negative {
        0
    } else {
        text.parse::<usize>().unwrap_or(usize::MAX)
    };
    if let Some(anchor) = anchor {
        if nodes.contains_key(anchor) {
            let mut visited = BTreeSet::from([anchor]);
            let mut queue = VecDeque::from([(anchor, 0usize, 1.0f64)]);
            while let Some((at, hop, decay)) = queue.pop_front() {
                within(deadline)?;
                if hop >= depth {
                    continue;
                }
                for edge in records.edges.iter().filter(|e| e["source"] == at) {
                    within(deadline)?;
                    let weight = edge
                        .get("weight")
                        .map(semantica_json::python_builtin_float)
                        .transpose()
                        .map_err(|_| anyhow::anyhow!("explorer_search_edge_weight_invalid"))?
                        .unwrap_or(1.0);
                    if weight < 0.0 {
                        continue;
                    }
                    let target = edge["target"]
                        .as_str()
                        .context("context_edge_target_required")?;
                    if !visited.insert(target) {
                        continue;
                    }
                    let next = hop + 1;
                    let confidence = decay * weight;
                    queue.push_back((target, next, confidence));
                    if nodes.contains_key(target) && confidence >= 0.0 {
                        hops.insert(target, next);
                    }
                }
            }
        }
        hops.insert(anchor, 0);
    }
    let rank = b["rank_by"]
        .as_str()
        .context("explorer_search_rank_required")?;
    let mut items = vec![];
    for (id, raw_score) in matches.matches {
        within(deadline)?;
        // Index IDs are stripped; original graph lookup uses the stored raw ID.
        let Some(raw) = nodes.get(id.as_str()) else {
            continue;
        };
        let hop = anchor.and_then(|_| hops.get(id.as_str()).copied());
        if anchor.is_some() && explicit && (negative || hop.is_none_or(|h| h > depth)) {
            continue;
        }
        let score = match hop {
            Some(h) if rank == "proximity" => {
                if h == 0 {
                    1.0
                } else {
                    1.0 / h as f64
                }
            }
            Some(h) if rank == "hybrid" => {
                0.6 * raw_score + 0.4 * (if h == 0 { 1.0 } else { 1.0 / h as f64 })
            }
            _ => raw_score,
        };
        items.push(json!({"node":crate::explorer_read::node(raw)?,"score":score,"hop_distance":hop,"semantic_similarity":null}));
    }
    if anchor.is_some() && ["proximity", "hybrid"].contains(&rank) {
        items.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap()
                .total_cmp(&a["score"].as_f64().unwrap())
        });
    }
    let body = crate::explorer_read::validate_body(
        "SearchResultResponse",
        json!({"total":items.len(),"results":items,"query":query}),
    )?;
    within(deadline)?;
    Ok(SemanticExplorerSearchResult {
        schema: "semantica.explorer-search-result/1".into(),
        status: 200,
        body,
    })
}
