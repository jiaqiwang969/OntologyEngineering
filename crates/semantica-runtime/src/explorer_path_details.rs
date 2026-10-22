//! Complete path explanation on ONE disposable record view of DatasetSession.
//! Metric availability and engineering adoption remain separate.
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticExplorerCountMetric, SemanticExplorerPathDetails, SemanticExplorerPathQuery,
    SemanticExplorerPathResult, SemanticExplorerScalarMetric,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
fn within(deadline: Instant) -> Result<()> {
    ensure!(Instant::now() < deadline, "explorer_path_deadline_exceeded");
    Ok(())
}
fn scalar(status: &str, value: Option<f64>, reason: &str) -> SemanticExplorerScalarMetric {
    SemanticExplorerScalarMetric {
        status: status.into(),
        value,
        reason: reason.into(),
    }
}
pub(crate) fn band(hops: u64) -> String {
    match hops {
        0..=1 => "direct",
        2..=3 => "near",
        4..=6 => "mid-range",
        _ => "distant",
    }
    .into()
}
fn weight_product(
    records: &crate::context::Records,
    result: &SemanticExplorerPathResult,
    deadline: Instant,
) -> Result<SemanticExplorerScalarMetric> {
    if !result.reachable {
        return Ok(scalar("not_applicable", None, "path_unreachable"));
    }
    if result.edge_ids.is_empty() {
        return Ok(scalar(
            "computed",
            Some(1.0),
            "selected_edge_weight_product_not_engineering_confidence",
        ));
    }
    let mut edges = BTreeMap::new();
    for edge in &records.edges {
        within(deadline)?;
        edges.insert(
            edge["id"].as_str().context("context_edge_id_required")?,
            edge,
        );
    }
    let mut product = 1.0_f64;
    for id in &result.edge_ids {
        within(deadline)?;
        let edge = edges
            .get(id.as_str())
            .context("explorer_path_edge_missing")?;
        let weight = match edge.get("weight") {
            None => 1.0,
            Some(v) => v.as_f64().context("explorer_path_weight_invalid")?,
        };
        ensure!(weight.is_finite(), "explorer_path_weight_invalid");
        let next = product * weight;
        if !next.is_finite() {
            return Ok(scalar(
                "unavailable",
                None,
                "selected_edge_product_overflow",
            ));
        }
        if next == 0.0 && product != 0.0 && weight != 0.0 {
            return Ok(scalar(
                "unavailable",
                None,
                "selected_edge_product_underflow",
            ));
        }
        product = next;
    }
    within(deadline)?;
    Ok(scalar(
        "computed",
        Some(product),
        "selected_edge_weight_product_not_engineering_confidence",
    ))
}
fn interpretation(result: &SemanticExplorerPathResult) -> String {
    if result.reachable {
        format!(
            "Graph path: {} hops. Metrics describe graph structure and vector geometry; weight product is not engineering confidence. Unavailable metrics remain unknown. No causal or engineering acceptance conclusion.",
            result.hop_count.unwrap_or(0)
        )
    } else {
        "No graph path in the requested direction. Path metrics are not applicable; this does not establish a physical impossibility.".into()
    }
}
/// Host selects one shared operation allowance for bottleneck then alternatives.
/// Arithmetic/vector work is governed by the original deadline and dataset bounds;
/// this operation counter is not presented as total CPU time or monetary cost.
pub fn execute(
    session: &crate::dataset::DatasetSession,
    query: &SemanticExplorerPathQuery,
    work_limit: u64,
    deadline: Instant,
) -> Result<SemanticExplorerPathDetails> {
    ensure!(
        (1..=10_000_000).contains(&work_limit),
        "explorer_path_details_budget"
    );
    let records = crate::explorer_path::prepare(session, query, deadline)?;
    let result = crate::explorer_path::execute_records(&records, query, deadline)?;
    let (semantic_similarity, path_coherence_score) =
        crate::explorer_path_similarity::from_records(&records, &result, deadline)?;
    let confidence_decay = weight_product(&records, &result, deadline)?;
    let (bottleneck_node, bottleneck_work_used) =
        crate::explorer_path_bottleneck::from_records(&records, &result, work_limit, deadline)?;
    let remaining = work_limit
        .checked_sub(bottleneck_work_used)
        .context("explorer_path_details_budget")?;
    let mut alternative_work_used = 0;
    let alternative_path_count = if !result.reachable {
        SemanticExplorerCountMetric {
            status: "not_applicable".into(),
            value: None,
            lower_bound: 0,
            capped: false,
            reason: "path_unreachable".into(),
        }
    } else if result.hop_count == Some(0) {
        SemanticExplorerCountMetric {
            status: "computed".into(),
            value: Some(0),
            lower_bound: 0,
            capped: false,
            reason: "identity_simple_path".into(),
        }
    } else if remaining == 0 {
        SemanticExplorerCountMetric {
            status: "unavailable".into(),
            value: None,
            lower_bound: 0,
            capped: false,
            reason: "shared_work_budget_exhausted".into(),
        }
    } else {
        let cap = result
            .hop_count
            .context("explorer_path_hops_missing")?
            .checked_add(2)
            .context("explorer_path_count_budget")? as usize;
        let count =
            crate::explorer_path_count::from_records(&records, query, cap, remaining, deadline)?;
        alternative_work_used = count.work_used;
        SemanticExplorerCountMetric {
            status: if count.count.is_some() {
                "computed"
            } else {
                "unavailable"
            }
            .into(),
            value: count.count.map(|n| n.saturating_sub(1) as u64),
            lower_bound: count.lower_bound.saturating_sub(1) as u64,
            capped: count.capped,
            reason: if count.count.is_none() {
                "simple_path_count_work_budget_exceeded"
            } else if count.capped {
                "capped_simple_node_path_alternatives_total_unknown"
            } else {
                "exhaustive_simple_node_path_alternatives"
            }
            .into(),
        }
    };
    within(deadline)?;
    let output = SemanticExplorerPathDetails {
        schema: "semantica.explorer-path-details/1".into(),
        query: query.clone(),
        distance_band: result.hop_count.map(band),
        interpretation: interpretation(&result),
        result,
        semantic_similarity,
        path_coherence_score,
        confidence_decay,
        bottleneck_node,
        alternative_path_count,
        work_limit,
        work_used: bottleneck_work_used + alternative_work_used,
        bottleneck_work_used,
        alternative_work_used,
    };
    validate(&output, query)?;
    within(deadline)?;
    Ok(output)
}
fn validate_scalar(m: &SemanticExplorerScalarMetric, cosine: bool) -> Result<()> {
    ensure!(
        m.validate().is_ok(),
        "explorer_path_details_metric_contract"
    );
    ensure!(
        match m.status.as_str() {
            "computed" => m
                .value
                .is_some_and(|v| v.is_finite() && (!cosine || (-1.0..=1.0).contains(&v))),
            "unavailable" | "not_applicable" => m.value.is_none(),
            _ => false,
        },
        "explorer_path_details_metric_state"
    );
    Ok(())
}
/// Structural wire guard only. Source binding and trusted worker identity are
/// still required; this does not independently prove optimality or metric values.
pub fn validate(d: &SemanticExplorerPathDetails, query: &SemanticExplorerPathQuery) -> Result<()> {
    ensure!(
        d.validate().is_ok()
            && query.validate().is_ok()
            && d.query == *query
            && d.result.validate().is_ok(),
        "explorer_path_details_contract"
    );
    ensure!(
        (1..=10_000_000).contains(&d.work_limit)
            && d.work_used <= d.work_limit
            && d.bottleneck_work_used.checked_add(d.alternative_work_used) == Some(d.work_used),
        "explorer_path_details_budget"
    );
    let r = &d.result;
    if r.reachable {
        ensure!(
            !r.path.is_empty()
                && r.path.len() <= 50_000
                && r.path.first() == Some(&query.source)
                && r.path.last() == Some(&query.target)
                && r.edge_ids.len().checked_add(1) == Some(r.path.len())
                && r.hop_count == Some(r.edge_ids.len() as u64)
                && r.total_weight.is_some_and(f64::is_finite)
                && r.path.iter().collect::<BTreeSet<_>>().len() == r.path.len()
                && r.edge_ids.iter().all(|s| !s.is_empty())
                && r.edge_ids.iter().collect::<BTreeSet<_>>().len() == r.edge_ids.len()
                && r.path.iter().all(|s| !s.is_empty() && s.len() <= 512),
            "explorer_path_details_result"
        );
    } else {
        ensure!(
            r.path.is_empty()
                && r.edge_ids.is_empty()
                && r.hop_count.is_none()
                && r.total_weight.is_none(),
            "explorer_path_details_result"
        );
    }
    ensure!(
        d.distance_band == r.hop_count.map(band) && d.interpretation == interpretation(r),
        "explorer_path_details_interpretation"
    );
    validate_scalar(&d.semantic_similarity, true)?;
    validate_scalar(&d.path_coherence_score, true)?;
    validate_scalar(&d.confidence_decay, false)?;
    ensure!(
        d.bottleneck_node.validate().is_ok()
            && match d.bottleneck_node.status.as_str() {
                "computed" =>
                    r.path.len() > 2
                        && d.bottleneck_node
                            .value
                            .as_ref()
                            .is_some_and(|v| r.path[1..r.path.len() - 1].contains(v)),
                "unavailable" | "not_applicable" => d.bottleneck_node.value.is_none(),
                _ => false,
            },
        "explorer_path_details_node_state"
    );
    let c = &d.alternative_path_count;
    ensure!(c.validate().is_ok(), "explorer_path_details_count_contract");
    let max = r
        .hop_count
        .unwrap_or(0)
        .checked_add(1)
        .context("explorer_path_details_count_state")?;
    ensure!(
        match c.status.as_str() {
            "computed" =>
                c.value == Some(c.lower_bound)
                    && c.lower_bound <= max
                    && (c.capped == (c.value == Some(max))),
            "unavailable" => c.value.is_none() && !c.capped && c.lower_bound < max,
            "not_applicable" => c.value.is_none() && c.lower_bound == 0 && !c.capped,
            _ => false,
        },
        "explorer_path_details_count_state"
    );
    if !r.reachable {
        ensure!(
            [
                d.semantic_similarity.status.as_str(),
                d.path_coherence_score.status.as_str(),
                d.confidence_decay.status.as_str(),
                d.bottleneck_node.status.as_str(),
                c.status.as_str()
            ]
            .iter()
            .all(|s| *s == "not_applicable")
                && d.work_used == 0,
            "explorer_path_details_applicability"
        );
    } else {
        ensure!(
            d.semantic_similarity.status != "not_applicable"
                && d.confidence_decay.status != "not_applicable"
                && c.status != "not_applicable"
                && (d.path_coherence_score.status == "not_applicable") == (r.hop_count == Some(0))
                && (d.bottleneck_node.status == "not_applicable") == (r.path.len() < 3),
            "explorer_path_details_applicability"
        );
        if r.hop_count == Some(0) {
            ensure!(
                c.value == Some(0)
                    && !c.capped
                    && d.confidence_decay.value == Some(1.0)
                    && r.total_weight == Some(0.0)
                    && d.work_used == 0,
                "explorer_path_details_identity"
            );
        }
    }
    Ok(())
}
