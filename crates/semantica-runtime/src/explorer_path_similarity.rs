//! Vector geometry on the actual path in the sole DatasetSession.
//! Equal dimensions do not establish a common embedding model/space, meaning,
//! engineering causality or confidence. No provider is invoked here.
use crate::dataset::DatasetSession;
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticExplorerPathQuery, SemanticExplorerPathSimilarity, SemanticExplorerScalarMetric,
};
use serde_json::Value;
use std::{collections::BTreeMap, time::Instant};
const KEYS: [&str; 7] = [
    "embedding",
    "embeddings",
    "vector",
    "node_embedding",
    "node2vec_embedding",
    "semantic_embedding",
    "reasoning_embedding",
];
const NESTED: [&str; 6] = [
    "embedding",
    "embeddings",
    "vector",
    "values",
    "node2vec",
    "semantic",
];
fn within(deadline: Instant) -> Result<()> {
    ensure!(Instant::now() < deadline, "explorer_path_deadline_exceeded");
    Ok(())
}
fn unavailable(reason: &str) -> SemanticExplorerScalarMetric {
    SemanticExplorerScalarMetric {
        status: "unavailable".into(),
        value: None,
        reason: reason.into(),
    }
}
fn not_applicable(reason: &str) -> SemanticExplorerScalarMetric {
    SemanticExplorerScalarMetric {
        status: "not_applicable".into(),
        value: None,
        reason: reason.into(),
    }
}
fn computed(value: f64, reason: &str) -> SemanticExplorerScalarMetric {
    SemanticExplorerScalarMetric {
        status: "computed".into(),
        value: Some(value),
        reason: reason.into(),
    }
}
// Internal normalized vector, with zero-vector convention kept distinct.
struct Vector {
    values: Vec<f64>,
    zero: bool,
}
type Embedding = std::result::Result<Vector, &'static str>;
// Extraction follows the original helper's alias precedence and built-in float
// conversion. Iterative traversal removes the earlier extra16-level restriction.
fn extract(value: &Value, deadline: Instant) -> Result<Option<Vec<f64>>> {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        within(deadline)?;
        if let Some(object) = value.as_object() {
            for key in NESTED.iter().rev() {
                if let Some(child) = object.get(*key) {
                    pending.push(child);
                }
            }
        } else if let Some(array) = value.as_array() {
            if array.is_empty() {
                continue;
            }
            let mut converted = Vec::with_capacity(array.len());
            let mut valid = true;
            for (i, item) in array.iter().enumerate() {
                if i % 256 == 0 {
                    within(deadline)?;
                }
                match semantica_contracts::json::python_builtin_float(item) {
                    Ok(number) => converted.push(number),
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
            }
            // NaN/inf are accepted by extraction, as upstream, but rejected at
            // arithmetic admission. They must not silently select a later alias.
            if valid {
                within(deadline)?;
                return Ok(Some(converted));
            }
        }
    }
    within(deadline)?;
    Ok(None)
}
fn select(node: &Value, deadline: Instant) -> Result<Option<Vec<f64>>> {
    for key in KEYS {
        let candidate = node
            .get(key)
            .or_else(|| node.get("metadata").and_then(|m| m.get(key)))
            .or_else(|| node.get("properties").and_then(|m| m.get(key)));
        if let Some(value) = candidate {
            if let Some(v) = extract(value, deadline)? {
                return Ok(Some(v));
            }
        }
    }
    within(deadline)?;
    Ok(None)
}
fn embedding(node: &Value, deadline: Instant) -> Result<Embedding> {
    let Some(mut values) = select(node, deadline)? else {
        return Ok(Err("embedding_missing_invalid_or_empty"));
    };
    let mut max = 0.0_f64;
    for (i, v) in values.iter().enumerate() {
        if i % 256 == 0 {
            within(deadline)?;
        }
        if !v.is_finite() {
            return Ok(Err("embedding_nonfinite"));
        }
        max = max.max(v.abs());
    }
    if max == 0.0 {
        return Ok(Ok(Vector { values, zero: true }));
    }
    // Scaling first handles both1e308 and subnormal inputs without overflowing
    // or underflowing every squared component. At least one scaled component=1.
    let mut squared = 0.0_f64;
    for (i, v) in values.iter_mut().enumerate() {
        if i % 256 == 0 {
            within(deadline)?;
        }
        *v /= max;
        squared += *v * *v;
    }
    let norm = squared.sqrt();
    for (i, v) in values.iter_mut().enumerate() {
        if i % 256 == 0 {
            within(deadline)?;
        }
        *v /= norm;
    }
    within(deadline)?;
    Ok(Ok(Vector {
        values,
        zero: false,
    }))
}
fn cosine(a: &Embedding, b: &Embedding, deadline: Instant) -> Result<SemanticExplorerScalarMetric> {
    within(deadline)?;
    let a = match a {
        Ok(v) => v,
        Err(reason) => return Ok(unavailable(&format!("left:{reason}"))),
    };
    let b = match b {
        Ok(v) => v,
        Err(reason) => return Ok(unavailable(&format!("right:{reason}"))),
    };
    if a.values.len() != b.values.len() {
        return Ok(unavailable("embedding_dimension_mismatch"));
    }
    if a.zero || b.zero {
        return Ok(computed(
            0.0,
            "zero_vector_legacy_convention_not_similarity_evidence",
        ));
    }
    // Compensated dot product limits cancellation error; clamp only roundoff
    // outside the mathematical [-1,1] interval after finite normalization.
    let mut sum = 0.0;
    let mut correction = 0.0;
    for (i, (x, y)) in a.values.iter().zip(&b.values).enumerate() {
        if i % 256 == 0 {
            within(deadline)?;
        }
        let item = x * y - correction;
        let next = sum + item;
        correction = (next - sum) - item;
        sum = next;
    }
    within(deadline)?;
    ensure!(sum.is_finite(), "explorer_path_similarity_nonfinite");
    Ok(computed(
        sum.clamp(-1.0, 1.0),
        "cosine_geometry_model_space_not_verified",
    ))
}
/// This combination computes its own path, so callers cannot inject an unrelated
/// path and obtain source-associated metrics for it. Shared DTOs come from ggen.
pub fn execute(
    session: &DatasetSession,
    query: &SemanticExplorerPathQuery,
    deadline: Instant,
) -> Result<SemanticExplorerPathSimilarity> {
    let result = crate::explorer_path::execute(session, query, deadline)?;
    let records = crate::context::records(session)?;
    let (semantic_similarity, path_coherence_score) = from_records(&records, &result, deadline)?;
    within(deadline)?;
    let output = SemanticExplorerPathSimilarity {
        schema: "semantica.explorer-path-similarity/1".into(),
        query: query.clone(),
        result,
        semantic_similarity,
        path_coherence_score,
    };
    ensure!(
        output.validate().is_ok(),
        "explorer_path_similarity_contract"
    );
    Ok(output)
}
pub(crate) fn from_records(
    records: &crate::context::Records,
    result: &semantica_contracts::SemanticExplorerPathResult,
    deadline: Instant,
) -> Result<(SemanticExplorerScalarMetric, SemanticExplorerScalarMetric)> {
    let metrics = if !result.reachable {
        (
            not_applicable("path_unreachable"),
            not_applicable("path_unreachable"),
        )
    } else {
        within(deadline)?;
        let mut nodes = BTreeMap::new();
        for node in &records.nodes {
            within(deadline)?;
            nodes.insert(
                node["id"].as_str().context("context_node_id_required")?,
                node,
            );
        }
        let mut embeddings = Vec::with_capacity(result.path.len());
        for id in &result.path {
            within(deadline)?;
            embeddings.push(embedding(
                nodes
                    .get(id.as_str())
                    .context("explorer_path_node_missing")?,
                deadline,
            )?);
        }
        let similarity = cosine(
            embeddings.first().context("explorer_path_empty")?,
            embeddings.last().context("explorer_path_empty")?,
            deadline,
        )?;
        let coherence = if embeddings.len() < 2 {
            not_applicable("no_adjacent_pairs")
        } else {
            let mut sum = 0.0;
            let mut failure = None;
            let mut zero_convention = false;
            for pair in embeddings.windows(2) {
                let metric = cosine(&pair[0], &pair[1], deadline)?;
                if let Some(v) = metric.value {
                    sum += v;
                    zero_convention |= metric.reason.starts_with("zero_vector_");
                } else {
                    failure.get_or_insert(metric.reason);
                }
            }
            if let Some(reason) = failure {
                unavailable(&format!("incomplete_adjacent_embeddings:{reason}"))
            } else {
                computed(
                    sum / (embeddings.len() - 1) as f64,
                    if zero_convention {
                        "complete_adjacent_mean_with_zero_vector_legacy_convention"
                    } else {
                        "complete_adjacent_cosine_mean_model_space_not_verified"
                    },
                )
            }
        };
        (similarity, coherence)
    };
    within(deadline)?;
    Ok(metrics)
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    #[test]
    fn original_embedding_extraction_bits_match_recorded_helpers() {
        let rows: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/explorer_embedding_compat.json"
        ))
        .unwrap();
        for row in rows.as_array().unwrap() {
            let actual = select(
                &row["node"],
                Instant::now() + std::time::Duration::from_secs(10),
            )
            .unwrap();
            let bits = actual.map(|values| {
                values
                    .iter()
                    .map(|v| format!("{:016x}", v.to_bits()))
                    .collect::<Vec<_>>()
            });
            assert_eq!(
                serde_json::to_value(bits).unwrap(),
                row["float64_bits"],
                "{}",
                row["name"]
            );
        }
    }
}
