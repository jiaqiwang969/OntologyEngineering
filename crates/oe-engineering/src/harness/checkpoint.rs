//! Explicit local checkpoints reuse Harness checks without preparing a model request.
use super::*;
use oe_contracts::HarnessCheckpointPolicy;

pub const CHECKPOINT_INPUT_SCHEMA: &str = "oev2.harness-checkpoint-input/1";
pub const CHECKPOINT_STEP_SCHEMA: &str = "oev2.harness-checkpoint/1";

fn node_limit(policy: &HarnessCheckpointPolicy) -> usize {
    if policy.schema == "oev2.harness-checkpoint-policy/2" {
        524_288
    } else {
        200_000
    }
}

fn structure(value: &Value, nodes: &mut usize, depth: usize, node_limit: usize) -> Result<()> {
    *nodes += 1;
    ensure!(
        *nodes <= node_limit && depth <= 96,
        "harness_checkpoint_structure_budget_exceeded"
    );
    match value {
        Value::Object(items) => {
            for (key, value) in items {
                ensure!(
                    !archive_has_credential_marker(key.as_bytes()),
                    "credential_material_in_trajectory"
                );
                structure(value, nodes, depth + 1, node_limit)?;
            }
        }
        Value::Array(items) => {
            for value in items {
                structure(value, nodes, depth + 1, node_limit)?;
            }
        }
        Value::String(value) => ensure!(
            !archive_has_credential_marker(value.as_bytes()),
            "credential_material_in_trajectory"
        ),
        _ => {}
    }
    Ok(())
}

fn raw_guard(
    raw: &[u8],
    bytes: &mut usize,
    nodes: &mut usize,
    limit: usize,
    node_limit: usize,
) -> Result<()> {
    *bytes = bytes
        .checked_add(raw.len())
        .context("harness_checkpoint_byte_budget_exceeded")?;
    ensure!(*bytes <= limit, "harness_checkpoint_byte_budget_exceeded");
    ensure!(
        !archive_has_credential_marker(raw),
        "credential_material_in_trajectory"
    );
    match strict_json(raw) {
        Ok(value) => structure(&value, nodes, 0, node_limit)?,
        Err(_) => {
            // A parser-depth error, duplicate key or malformed structured source
            // cannot silently become opaque bytes to evade the JSON limits.
            // Other non-JSON artifacts keep their original binary/text identity.
            let first = raw.iter().copied().find(|b| !b.is_ascii_whitespace());
            ensure!(
                !matches!(first, Some(b'{' | b'[')),
                "harness_checkpoint_json_source_invalid"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_node_capacity_is_explicit_and_exact() {
        let mut policy = HarnessCheckpointPolicy {
            schema: "oev2.harness-checkpoint-policy/1".into(),
            mode: "verified_no_model".into(),
            byte_budget: 16777216,
            reason: "resource boundary fixture".into(),
        };
        let small = serde_json::to_vec(&json!(vec![Value::Null; 199999])).unwrap();
        raw_guard(&small, &mut 0, &mut 0, 16777216, node_limit(&policy)).unwrap();
        let over = serde_json::to_vec(&json!(vec![Value::Null; 200000])).unwrap();
        assert!(raw_guard(&over, &mut 0, &mut 0, 16777216, node_limit(&policy)).is_err());
        policy.schema = "oev2.harness-checkpoint-policy/2".into();
        raw_guard(&over, &mut 0, &mut 0, 16777216, node_limit(&policy)).unwrap();
        let exact = serde_json::to_vec(&json!(vec![Value::Null; 524287])).unwrap();
        raw_guard(&exact, &mut 0, &mut 0, 16777216, node_limit(&policy)).unwrap();
        let exceeded = serde_json::to_vec(&json!(vec![Value::Null; 524288])).unwrap();
        assert!(raw_guard(&exceeded, &mut 0, &mut 0, 16777216, node_limit(&policy)).is_err());
    }
}

pub(super) fn policy(document: &Value) -> Result<Option<HarnessCheckpointPolicy>> {
    if document["schema"] != CHECKPOINT_INPUT_SCHEMA {
        return Ok(None);
    }
    exact_keys(
        document,
        &["schema", "task_id", "context", "request", "harness"],
        &["external_skills"],
        "invalid_harness_checkpoint_input",
    )?;
    for key in ["task_id", "request"] {
        text(&document[key], key)?;
    }
    ensure!(
        matches!(
            document["context"],
            Value::String(_) | Value::Object(_) | Value::Array(_)
        ),
        "invalid_context"
    );
    if let Some(external) = document.get("external_skills") {
        ensure!(external.is_array(), "invalid_harness_checkpoint_input");
    }
    ensure!(
        document["harness"].get("context_policy").is_none(),
        "invalid_harness_checkpoint_policy"
    );
    let value = &document["harness"]["checkpoint_policy"];
    exact_keys(
        value,
        &["schema", "mode", "byte_budget", "reason"],
        &[],
        "invalid_harness_checkpoint_policy",
    )?;
    let policy: HarnessCheckpointPolicy =
        serde_json::from_value(value.clone()).context("invalid_harness_checkpoint_policy")?;
    policy
        .validate()
        .map_err(|_| anyhow::anyhow!("invalid_harness_checkpoint_policy"))?;
    ensure!(
        (1..=16_777_216).contains(&policy.byte_budget),
        "invalid_harness_checkpoint_policy"
    );
    text(&value["reason"], "invalid_harness_checkpoint_policy")?;
    ensure!(
        ordered_json(document)?.len() <= 1_048_576,
        "harness_checkpoint_document_budget_exceeded"
    );
    structure(document, &mut 0, 0, node_limit(&policy))?;
    Ok(Some(policy))
}

/// Bound original reads before formal checks or copies. Repeated observation
/// identities are stored once; references themselves remain in the full input.
pub(super) fn preflight_sources(
    root: &SkillRoot,
    document: &Value,
    policy: &HarnessCheckpointPolicy,
) -> Result<()> {
    let h = &document["harness"];
    let mut refs = Vec::new();
    for field in ["current_project", "previous"] {
        if let Some(value) = h.get(field).filter(|v| !v.is_null()) {
            refs.push(value);
        }
    }
    let empty = json!([]);
    for observation in h
        .get("observations")
        .unwrap_or(&empty)
        .as_array()
        .context("invalid_observations")?
    {
        for reference in observation["evidence"]
            .as_array()
            .context("invalid_observation_evidence")?
        {
            refs.push(reference);
        }
    }
    let mut bytes = 0;
    let mut nodes = 0;
    let mut seen = BTreeSet::new();
    raw_guard(
        &serde_json::to_vec_pretty(document)?,
        &mut bytes,
        &mut nodes,
        policy.byte_budget as usize,
        node_limit(policy),
    )?;
    for reference in refs {
        if !seen.insert(reference["sha256"].clone().to_string()) {
            continue;
        }
        ensure!(
            seen.len() <= 256,
            "harness_checkpoint_artifact_budget_exceeded"
        );
        let (path, raw) = read_ref(root, reference)?;
        let _ = path;
        raw_guard(
            &raw,
            &mut bytes,
            &mut nodes,
            policy.byte_budget as usize,
            node_limit(policy),
        )?;
    }
    Ok(())
}

pub(super) fn validate_stored(
    root: &SkillRoot,
    output: &Path,
    result: &Value,
    policy: &HarnessCheckpointPolicy,
    terminal_bytes: Option<&[u8]>,
) -> Result<()> {
    let artifacts = result["artifacts"]
        .as_object()
        .context("invalid_harness_artifacts")?;
    ensure!(
        artifacts.len() <= 256,
        "harness_checkpoint_artifact_budget_exceeded"
    );
    let mut bytes = 0;
    let mut nodes = 0;
    for (relative, expected) in artifacts {
        ensure!(
            relative != "iteration.json",
            "invalid_harness_artifact_path"
        );
        let raw = read_artifact(root, output, relative, expected)?;
        raw_guard(
            &raw,
            &mut bytes,
            &mut nodes,
            policy.byte_budget as usize,
            node_limit(policy),
        )?;
    }
    let mut terminal = serde_json::to_vec_pretty(result)?;
    terminal.push(b'\n');
    raw_guard(
        terminal_bytes.unwrap_or(&terminal),
        &mut bytes,
        &mut nodes,
        policy.byte_budget as usize,
        node_limit(policy),
    )?;
    Ok(())
}

pub(super) fn load(
    root: &SkillRoot,
    path: &Path,
    result: &Value,
    original: &impl Fn(&str) -> Result<Value>,
) -> Result<Value> {
    let input = original("input.json")?;
    let policy = policy(&input)?.context("invalid_harness_checkpoint_input")?;
    let h = &input["harness"];
    let full = original("semantic-state.json")?;
    let feedback = original("feedback.json")?;
    let premises = original("premises.json")?;
    ensure!(
        result["status"] == "recorded"
            && result["model_execution"] == "not_run"
            && result["model_calls"] == 0
            && result["candidate_routes"] == json!([])
            && result["uncertain_routes"] == json!([])
            && result["checkpoint_policy"] == input["harness"]["checkpoint_policy"]
            && result.get("context_transport").is_none(),
        "harness_checkpoint_authority_mismatch"
    );
    ensure!(
        result["state"] == identity(h)?
            && result["task_id"] == input["task_id"]
            && result["premises"] == premises
            && result["feedback"] == feedback
            && result["previous"] == h["previous"]
            && result["change_reason"] == h["change_reason"],
        "harness_summary_mismatch"
    );
    for (summary, field) in [
        ("state", "project_state"),
        ("premises", "premises"),
        ("feedback", "current_feedback"),
        ("observations", "observations"),
        ("review_context_association", "review_context_association"),
        ("changes", "changes"),
        ("baseline", "baseline"),
        ("baseline_changes", "baseline_changes"),
    ] {
        ensure!(result[summary] == full[field], "harness_summary_mismatch");
    }
    ensure!(
        result["baseline"]["schema"] == "oev2.harness-checkpoint-baseline/1"
            && result["baseline"]["checkpoint_policy"] == h["checkpoint_policy"]
            && result["baseline"]["model_execution"] == "not_run"
            && result["baseline"]["semantic_premises_sha256"] == premises["snapshot_sha256"],
        "harness_summary_mismatch"
    );
    for field in ["current_project_check", "engineering_checks"] {
        ensure!(
            result[field] == full[field] && result[field] == feedback[field],
            "harness_summary_mismatch"
        );
    }
    let supported = full["current_project_check"]["conclusion"] == "pass"
        && full["engineering_checks"]
            .as_array()
            .is_some_and(|checks| checks.iter().all(|c| c["conclusion"] == "pass"));
    ensure!(
        result["current_use"]
            == if supported {
                json!("content_bound; engineering acceptance separate")
            } else {
                json!("blocked_or_historical_only")
            }
            && full["current_use"]
                == if supported {
                    json!("content_bound; native/physical acceptance separate")
                } else {
                    json!("blocked_or_historical_only")
                },
        "harness_summary_mismatch"
    );
    let artifacts = result["artifacts"]
        .as_object()
        .context("invalid_harness_artifacts")?;
    ensure!(
        !artifacts
            .keys()
            .any(|p| p.starts_with("routing/") || p == "semantic-view.json"),
        "harness_checkpoint_authority_mismatch"
    );
    if let Some(reference) = h.get("current_project") {
        ensure!(
            artifacts.get("current-project.json") == Some(&reference["sha256"]),
            "harness_reference_changed"
        );
        original("current-project.json")?;
    }
    if !h["previous"].is_null() {
        let prior = original("previous-iteration.json")?;
        ensure!(
            artifacts.get("previous-iteration.json") == Some(&h["previous"]["sha256"])
                && full["previous"] == previous_context(&prior, &h["previous"]),
            "harness_previous_snapshot_mismatch"
        );
    } else {
        ensure!(
            full["previous"].is_null(),
            "harness_previous_snapshot_mismatch"
        );
    }
    let mut observations = Vec::new();
    let mut refs = Vec::new();
    let empty = json!([]);
    for observation in h
        .get("observations")
        .unwrap_or(&empty)
        .as_array()
        .context("invalid_observations")?
    {
        exact_keys(
            observation,
            &["kind", "statement", "evidence"],
            &[],
            "invalid_observation",
        )?;
        let mut hashes = Vec::new();
        for reference in observation["evidence"]
            .as_array()
            .context("invalid_observation_evidence")?
        {
            exact_keys(
                reference,
                &["path", "sha256"],
                &[],
                "invalid_harness_reference",
            )?;
            let hash = text(&reference["sha256"], "observation_sha256")?;
            ensure!(
                artifacts.get(&format!("observations/{hash}")) == Some(&reference["sha256"]),
                "harness_observation_snapshot_missing"
            );
            hashes.push(hash);
            refs.push(reference);
        }
        observations.push(json!({"kind":observation["kind"],"statement":observation["statement"],"evidence_sha256":hashes,"truth_status":"source_statement_not_admitted"}));
    }
    ensure!(
        result["observations"] == json!(observations) && result["observation_refs"] == json!(refs),
        "harness_summary_mismatch"
    );
    let terminal = read_regular_bytes(path, 16_777_216)?;
    validate_stored(
        root,
        path.parent().context("missing_parent")?,
        result,
        &policy,
        Some(&terminal),
    )?;
    Ok(result.clone())
}
