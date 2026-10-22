//! Declared causal-edge traversal, not a physical causal inference or probability.
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticExplorerCausalDistanceQuery, SemanticExplorerCausalDistanceResult,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};
fn within(d: Instant) -> Result<()> {
    ensure!(Instant::now() < d, "explorer_causal_deadline_exceeded");
    Ok(())
}
fn interpretation(hops: u64, decay: f64) -> String {
    match crate::explorer_path_details::band(hops).as_str() {
        "direct" => format!("Direct cause with confidence {decay:.2}."),
        "near" => format!(
            "Mediated through {} decision(s); confidence decays to {decay:.2} — {} evidence.",
            hops - 1,
            if decay > 0.4 { "moderate" } else { "weak" }
        ),
        _ => format!(
            "Distal influence across {hops} causal steps; confidence near {decay:.2} — weak signal."
        ),
    }
}
pub fn execute(
    session: &crate::dataset::DatasetSession,
    q: &SemanticExplorerCausalDistanceQuery,
    deadline: Instant,
) -> Result<SemanticExplorerCausalDistanceResult> {
    within(deadline)?;
    ensure!(q.validate().is_ok(), "explorer_causal_query_contract");
    ensure!(
        serde_json::to_vec(q)?.len() <= session.dataset_byte_limit(),
        "explorer_causal_query_budget"
    );
    let source = q
        .source_id
        .as_deref()
        .context("explorer_causal_source_required")?;
    let target = q
        .target_id
        .as_deref()
        .context("explorer_causal_target_required")?;
    let records = crate::context::records(session)?;
    let nodes: BTreeMap<_, _> = records
        .nodes
        .iter()
        .map(|n| (n["id"].as_str().unwrap_or(""), n))
        .collect();
    within(deadline)?;
    let mut body = json!({"source_id":q.source_id,"target_id":q.target_id,"causal_path":[],"causal_hop_count":0,"intermediate_decisions":[],"confidence_decay":0.0,"weakest_link":null,"interpretation":"No causal path found between the two nodes."});
    if nodes.contains_key(source) {
        let mut queue = VecDeque::from([(source, 1.0f64, Value::Null)]);
        let mut visited = BTreeSet::from([source]);
        let mut parent = BTreeMap::new();
        while let Some((at, decay, weakest)) = queue.pop_front() {
            within(deadline)?;
            if at == target {
                let mut path = vec![at];
                let mut current = at;
                while current != source {
                    within(deadline)?;
                    current = *parent
                        .get(current)
                        .context("explorer_causal_parent_missing")?;
                    path.push(current);
                }
                path.reverse();
                let hops = (path.len() - 1) as u64;
                let intermediates = path
                    .iter()
                    .skip(1)
                    .take(path.len().saturating_sub(2))
                    .filter(|id| {
                        nodes.get(**id).is_some_and(|n| {
                            crate::explorer_search_unicode::lower(n["type"].as_str().unwrap_or(""))
                                == "decision"
                        })
                    })
                    .copied()
                    .collect::<Vec<_>>();
                // Decimal formatting rounds the actual binary64 value; multiplying by 1e6
                // before rounding would change original cases such as 0.1234575.
                let rounded = format!("{decay:.6}").parse::<f64>()?;
                body = json!({"source_id":q.source_id,"target_id":q.target_id,"causal_path":path,"causal_hop_count":hops,"intermediate_decisions":intermediates,"confidence_decay":rounded,"weakest_link":weakest,"interpretation":interpretation(hops,decay)});
                break;
            }
            for e in records.edges.iter().filter(|e| e["source"] == at) {
                within(deadline)?;
                let kind = e["type"].as_str().unwrap_or("");
                if ![
                    "causes",
                    "influences",
                    "leads_to",
                    "supports",
                    "CAUSED",
                    "INFLUENCED",
                    "PRECEDENT_FOR",
                ]
                .contains(&kind)
                {
                    continue;
                }
                let target = e["target"]
                    .as_str()
                    .context("explorer_causal_target_required")?;
                if !visited.insert(target) {
                    continue;
                }
                let weight =
                    semantica_json::python_builtin_float(e.get("weight").unwrap_or(&json!(1.0)))
                        .map_err(|e| anyhow::anyhow!("explorer_causal_weight:{e:?}"))?;
                let next_decay = decay * weight;
                ensure!(
                    weight.is_finite() && next_decay.is_finite(),
                    "explorer_causal_nonfinite_unavailable"
                );
                let next_weakest = if weakest.is_null()
                    || weight < weakest["edge_weight"].as_f64().unwrap_or(1.0)
                {
                    json!({"source":at,"target":target,"edge_weight":weight})
                } else {
                    weakest.clone()
                };
                parent.insert(target, at);
                queue.push_back((target, next_decay, next_weakest));
            }
        }
    }
    let body = crate::explorer_read::validate_body("CausalDistanceReport", body)?;
    within(deadline)?;
    ensure!(
        serde_json::to_vec(&body)?.len() <= session.dataset_byte_limit(),
        "explorer_causal_response_budget"
    );
    Ok(SemanticExplorerCausalDistanceResult {
        schema: "semantica.explorer-causal-distance-result/2".into(),
        status: 200,
        body,
    })
}
