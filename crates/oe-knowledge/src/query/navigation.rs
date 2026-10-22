//! Local source navigation leaves applicability and topic judgments to the host Agent.
use super::{Engine, merge};
use anyhow::{Context, Result};
use serde_json::{Value, json};
pub(super) fn route(engine: &Engine, _chapters: &[Value]) -> Result<Value> {
    let topics = engine.policy.value["topics"]
        .as_array()
        .context("knowledge_topics_missing")?;
    let ids = topics
        .iter()
        .map(|t| t["id"].as_str().context("knowledge_topic_id_missing"))
        .collect::<Result<Vec<_>>>()?;
    let mut result = json!({"mode":"local","scope":"unassessed","needed_topics":[],"uncertain_topics":ids,
    "unanswered_topics":[],"chapters":[],"errors":[],"policy_sha256":engine.policy.sha256,"method_bundle_sha256":engine.policy.sha256,"topic_judgment":"not_run"});
    merge(
        &mut result,
        crate::policy::pattern_candidates(engine.policy, None)?,
    )?;
    return Ok(result);
}
