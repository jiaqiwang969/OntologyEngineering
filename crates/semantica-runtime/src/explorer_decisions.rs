//! Original decision observations over the existing ordered context records.
//! Graph compliance and neighbor chains are not engineering acceptance or causal proof.
use crate::{
    dataset::DatasetSession,
    explorer_read::{self, dates, metadata, node, truth, validate_body},
    explorer_search_unicode::lower,
};
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticExplorerDecisionQuery, SemanticExplorerDecisionResult, SemanticExplorerGraphReadQuery,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Instant};
fn within(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "explorer_decisions_deadline_exceeded"
    );
    Ok(())
}
fn reply(status: u64, body: Value) -> SemanticExplorerDecisionResult {
    SemanticExplorerDecisionResult {
        schema: "semantica.explorer-decision-result/1".into(),
        status,
        body,
    }
}
#[derive(Debug)]
enum DecisionInputFailure {
    Value,
    Type,
    Overflow,
}
impl std::fmt::Display for DecisionInputFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "explorer_decision_input_{self:?}")
    }
}
impl std::error::Error for DecisionInputFailure {}
/// Only original input exceptions become historical HTTP responses. Source,
/// budget, deadline, contract and internal schema failures remain service errors.
pub fn original_error_response(error: &anyhow::Error) -> Option<SemanticExplorerDecisionResult> {
    match error.downcast_ref::<DecisionInputFailure>()? {
        DecisionInputFailure::Value => Some(reply(422, json!({"detail":"Invalid input"}))),
        DecisionInputFailure::Type | DecisionInputFailure::Overflow => {
            Some(reply(500, json!({"detail":"Internal server error"})))
        }
    }
}
fn decision(n: &Value) -> Result<Value> {
    let p = &n["properties"];
    let raw = p
        .get("confidence")
        .filter(|v| truth(v))
        .cloned()
        .unwrap_or(json!(0.0));
    let confidence = semantica_json::python_builtin_float(&raw).map_err(|e| match e {
        semantica_json::PythonFloatError::IntegerOverflow => DecisionInputFailure::Overflow,
        semantica_json::PythonFloatError::InvalidInput => {
            if raw.is_array() || raw.is_object() {
                DecisionInputFailure::Type
            } else {
                DecisionInputFailure::Value
            }
        }
    })?;
    if !confidence.is_finite() {
        return Err(DecisionInputFailure::Value.into());
    }
    let body = json!({"decision_id":n["id"],"category":p.get("category").unwrap_or(&json!("")),"scenario":p.get("scenario").unwrap_or(&json!("")),"reasoning":p.get("reasoning").unwrap_or(&json!("")),"outcome":p.get("outcome").unwrap_or(&json!("")),"confidence":confidence,"timestamp":p.get("timestamp").unwrap_or(&Value::Null),"metadata":p});
    let runtime =
        crate::schema::RustSchemaRuntime::new().map_err(|e| anyhow::anyhow!("{}", e.code))?;
    let validated = runtime
        .validate_model("semantica.explorer.schemas::DecisionResponse", &body)
        .map_err(|e| -> anyhow::Error {
            if e.code == "schema_python_float_overflow" {
                DecisionInputFailure::Overflow.into()
            } else {
                anyhow::anyhow!("{}", e.code)
            }
        })?;
    if validated["status"] != "validated" {
        return Err(DecisionInputFailure::Value.into());
    }
    Ok(validated["output"].clone())
}
fn words(value: &Value) -> Result<BTreeSet<String>> {
    Ok(lower(&semantica_json::python_str(value)?)
        .split(|c| semantica_json::python_character_class(c).1)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect())
}
pub fn execute(
    session: &DatasetSession,
    q: &SemanticExplorerDecisionQuery,
    deadline: Instant,
) -> Result<SemanticExplorerDecisionResult> {
    within(deadline)?;
    ensure!(q.validate().is_ok(), "explorer_decision_query_contract");
    ensure!(
        serde_json::to_vec(q)?.len() <= session.dataset_byte_limit(),
        "explorer_decision_query_budget"
    );
    ensure!(
        !q.skip.is_empty() && q.skip.bytes().all(|b| b.is_ascii_digit()),
        "explorer_decision_skip_invalid"
    );
    let records = crate::context::records(session)?;
    within(deadline)?;
    let target = if q.operation == "list" {
        None
    } else {
        let id = q
            .decision_id
            .as_deref()
            .context("explorer_decision_id_required")?;
        match records.nodes.iter().find(|n| n["id"] == id) {
            Some(n) => Some(node(n)?),
            None => {
                return Ok(reply(
                    404,
                    json!({"detail":format!("Decision '{id}' not found")}),
                ));
            }
        }
    };
    let body = match q.operation.as_str() {
        "get" => decision(target.as_ref().unwrap())?,
        "chain" => {
            let id = q.decision_id.clone().unwrap();
            let input = SemanticExplorerGraphReadQuery {
                schema: "semantica.explorer-graph-read-query/1".into(),
                operation: "neighbors".into(),
                node_id: Some(id.clone()),
                node_type: None,
                edge_type: None,
                source: None,
                target: None,
                search: None,
                bbox: None,
                cursor: None,
                skip: "0".into(),
                limit: 5000,
                depth: 5,
            };
            let found = explorer_read::execute(session, &input, deadline)?;
            ensure!(
                found.status == 200,
                "explorer_decision_neighbors_unavailable"
            );
            let chain=found.body.as_array().context("explorer_decision_neighbors_invalid")?.iter().map(|n|json!({"id":n["id"],"type":n["type"],"relationship":n["relationship"],"hop":n["hop"],"content":n["content"]})).collect::<Vec<_>>();
            validate_body(
                "CausalChainResponse",
                json!({"decision_id":id,"chain":chain}),
            )?
        }
        "compliance" => {
            let id = q.decision_id.as_deref().unwrap();
            let mut edges = records.edges.iter().collect::<Vec<_>>();
            edges.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
            within(deadline)?;
            let mut violations = vec![];
            for e in edges.into_iter().take(999_999) {
                within(deadline)?;
                let kind =
                    semantica_json::python_str(e.get("type").unwrap_or(&json!("related_to")))?;
                if e["source"] == id
                    && ["violates", "non_compliant", "breaches"].contains(&kind.as_str())
                {
                    let mut meta = metadata(e)?;
                    dates(e, &mut meta);
                    violations.push(json!({"policy_id":e["target"],"type":kind,"metadata":meta}));
                }
            }
            validate_body(
                "ComplianceResponse",
                json!({"decision_id":id,"compliant":violations.is_empty(),"violations":violations}),
            )?
        }
        "list" | "precedents" => {
            ensure!(
                q.limit >= 1 && q.limit <= if q.operation == "list" { 500 } else { 100 },
                "explorer_decision_limit_invalid"
            );
            let mut nodes = records
                .nodes
                .iter()
                .filter(|n| n["type"] == "decision")
                .collect::<Vec<_>>();
            nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
            within(deadline)?;
            let nodes = nodes
                .into_iter()
                .take(999_999)
                .map(node)
                .collect::<Result<Vec<_>>>()?;
            within(deadline)?;
            if q.operation == "list" {
                let category = q.category.as_deref().filter(|v| !v.is_empty()).map(lower);
                let skip = q.skip.parse::<usize>().unwrap_or(usize::MAX);
                let mut filtered = vec![];
                for n in nodes {
                    within(deadline)?;
                    if let Some(c) = &category {
                        if lower(&semantica_json::python_str(
                            n["properties"].get("category").unwrap_or(&json!("")),
                        )?) != *c
                        {
                            continue;
                        }
                    }
                    filtered.push(n);
                }
                let values = filtered
                    .iter()
                    .skip(skip)
                    .take(q.limit as usize)
                    .map(decision)
                    .collect::<Result<Vec<_>>>()?;
                json!(values)
            } else {
                let target = target.as_ref().unwrap();
                let p = &target["properties"];
                let category = semantica_json::python_str(p.get("category").unwrap_or(&json!("")))?;
                let scenario = words(p.get("scenario").unwrap_or(&json!("")))?;
                let mut scored = vec![];
                for n in nodes {
                    within(deadline)?;
                    if n["id"] == target["id"] {
                        continue;
                    }
                    let p = &n["properties"];
                    let mut score = 0.0f64;
                    if !category.is_empty()
                        && lower(&semantica_json::python_str(
                            p.get("category").unwrap_or(&json!("")),
                        )?) == lower(&category)
                    {
                        score += 0.5;
                    }
                    let other = words(p.get("scenario").unwrap_or(&json!("")))?;
                    if !scenario.is_empty() && !other.is_empty() {
                        score += 0.5 * scenario.intersection(&other).count() as f64
                            / scenario.union(&other).count().max(1) as f64;
                    }
                    scored.push((score, n));
                }
                scored.sort_by(|a, b| b.0.total_cmp(&a.0));
                within(deadline)?;
                json!(
                    scored
                        .iter()
                        .take(q.limit as usize)
                        .map(|(_, n)| decision(n))
                        .collect::<Result<Vec<_>>>()?
                )
            }
        }
        _ => anyhow::bail!("explorer_decision_operation_invalid"),
    };
    within(deadline)?;
    ensure!(
        serde_json::to_vec(&body)?.len() <= session.dataset_byte_limit(),
        "explorer_decision_response_budget"
    );
    Ok(reply(200, body))
}
