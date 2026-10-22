//! Receive source-bound retained-dimension retirement without adopting a drawing.
use anyhow::{Context, Result, ensure};
use oe_core::read_regular_bytes;
use oe_nx_protocol::{BlobIdentity, decode};
use oe_nx_worker::geometry::{Parameters, drawing_retirement as check};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

fn bytes(root: &Path, name: &str) -> Result<Vec<u8>> {
    read_regular_bytes(&super::member(root, name)?, 512 * 1024 * 1024)
}
fn observed_part<'a>(parts: &'a Value, name: &str) -> Result<&'a Value> {
    let parts = parts
        .as_array()
        .context("retirement_observed_parts_missing")?;
    ensure!(parts.len() == 1, "retirement_single_part_required");
    let part = &parts[0];
    ensure!(
        part["path"]
            .as_str()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            == Some(name),
        "retirement_observed_part_mismatch"
    );
    ensure!(
        part["view_refresh_requested"] == false,
        "retirement_reopen_refresh_forbidden"
    );
    Ok(part)
}

/// Inspect a completed, source-bound, non-refreshing job before freezing targets.
pub fn source_inventory(root: &Path, part_input: &str) -> Result<Value> {
    let (result, job) = super::associations::receipt(root)?;
    ensure!(
        result.action == "inspect_detailed",
        "retirement_inventory_inspection_required"
    );
    let original = bytes(root, &format!("inputs/{part_input}"))?;
    let input = job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == format!("inputs/{part_input}"))
        .context("retirement_original_input_missing")?;
    input.identity.verify(&original)?;
    let part = observed_part(&result.readback["parts"], part_input)?;
    Ok(
        json!({"schema":"oev2.drawing-retirement-inventory/1", "source":input.identity,
        "inventory":check::inventory(part)?, "process":result.process,
        "worker":result.worker,"worker_manifest_digest":result.worker_manifest_digest,
        "engineering_adoption":"not_established", "identity_scope":"source-local complete observations; not persistent topology"}),
    )
}

/// Recompute the execution checks from original observations; the worker's pass
/// field is not an independent expectation and is deliberately not consumed.
pub fn check_execution(
    intent: &oe_contracts::DrawingRetirementIntent,
    source: &[u8],
    readback: &Value,
) -> Result<Value> {
    ensure!(
        readback["intent"] == serde_json::to_value(intent)?,
        "retirement_recorded_intent_mismatch"
    );
    let parts = readback["parts"]
        .as_array()
        .context("retirement_saved_parts_missing")?;
    ensure!(parts.len() == 1, "retirement_single_part_required");
    let part = &parts[0];
    let plan = check::prepare(intent, source, &part["before"])?;
    let calls = readback["calls"]
        .as_array()
        .context("retirement_delete_calls_missing")?;
    let targets = plan["targets"].as_array().unwrap();
    ensure!(
        calls.len() == targets.len(),
        "retirement_delete_calls_incomplete"
    );
    for (call, target) in calls.iter().zip(targets) {
        ensure!(
            call["api"] == "UF_OBJ_delete_object"
                && call["return_code"] == 0
                && call["dimension_tag"] == target["tag"]
                && call["before_lifecycle"]["status_observed"] == true
                && call["before_lifecycle"]["status"] == "alive"
                && call["retained_query_code"] == 0
                && call["retained_raw"] == 1
                && call["after_lifecycle"]["status_observed"] == true
                && call["after_lifecycle"]["status"] == "deleted",
            "retirement_native_delete_unproven"
        );
    }
    let removed: BTreeSet<_> = intent.retire_signatures.iter().cloned().collect();
    let after = check::check_progress(intent, source, &part["before"], &part["after"], &removed)?;
    let saved = check::check_progress(
        intent,
        source,
        &part["before"],
        &part["after_save"],
        &removed,
    )?;
    Ok(json!({"conclusion":"pass","after":after,"saved":saved}))
}

pub fn compare(saved: &Path, reopened: &Path) -> Result<Value> {
    let (before, saved_job) = super::associations::receipt(saved)?;
    let (after, reopened_job) = super::associations::receipt(reopened)?;
    ensure!(
        before.process != after.process
            && before.worker == after.worker
            && before.worker_manifest_digest == after.worker_manifest_digest
            && before.runtime == after.runtime,
        "retirement_reopen_process_or_tool_identity_mismatch"
    );
    ensure!(
        before.action == "retire_drawing_dimensions" && after.action == "inspect_detailed",
        "retirement_reopen_action_mismatch"
    );
    let parameters: Parameters = decode(&bytes(saved, "params.json")?)?;
    parameters.validate()?;
    let Parameters::RetireDrawingDimensions {
        part: name, intent, ..
    } = parameters
    else {
        anyhow::bail!("retirement_parameters_required")
    };
    ensure!(
        before.readback["parts"][0]["input"] == name,
        "retirement_saved_part_mismatch"
    );
    let original = bytes(saved, &format!("inputs/{name}"))?;
    saved_job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == format!("inputs/{name}"))
        .context("retirement_original_input_missing")?
        .identity
        .verify(&original)?;
    let output = bytes(saved, &format!("out/{name}"))?;
    let input = reopened_job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == format!("inputs/{name}"))
        .context("retirement_reopen_input_missing")?;
    input.identity.verify(&output)?;
    let execution = check_execution(&intent, &original, &before.readback)?;
    let part = observed_part(&after.readback["parts"], &name)?;
    let reopened_check = check::check_reopened(
        &intent,
        &output,
        &before.readback["parts"][0]["after_save"],
        part,
    )?;
    Ok(
        json!({"schema":"oev2.drawing-retirement-reopen/1","conclusion":"pass",
        "intent":intent,"execution":execution,"reopened":reopened_check,
        "saved_source":BlobIdentity::of(&output),"saved_process":before.process,"reopened_process":after.process,
        "worker":after.worker,"worker_manifest_digest":after.worker_manifest_digest,
        "engineering_adoption":"not_established","visual_readability":"not_assessed",
        "identity_scope":"recorded completion and source/input/artifact/tool identities; separate process, not fresh OS attestation"}),
    )
}
