//! Read-only native readback for a workspace stage. A recorded result retains
//! failed/partial/unknown observations and never grants engineering acceptance.

use crate::{artifacts, harness};
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, CapabilityRegistry, WorkspaceStage, WorkspaceStageReceipt};
use oe_core::{SkillRoot, sha256};
use oe_kernel::project::{self, Project, tool_result};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn dto<T>(value: std::result::Result<T, Vec<String>>) -> Result<T> {
    value.map_err(|_| anyhow::anyhow!("workspace_stage_receipt_contract_invalid"))
}

fn same_reference(left: &ArtifactReference, right: &ArtifactReference) -> bool {
    left.path == right.path && left.sha256 == right.sha256
}

fn read(
    root: &SkillRoot,
    reference: &ArtifactReference,
    originals: &mut BTreeMap<String, (ArtifactReference, Vec<u8>)>,
) -> Result<Vec<u8>> {
    if let Some((old, bytes)) = originals.get(&reference.path) {
        ensure!(
            same_reference(old, reference),
            "workspace_stage_source_conflict"
        );
        return Ok(bytes.clone());
    }
    ensure!(originals.len() < 256, "workspace_stage_source_count_limit");
    let bytes = artifacts::read(root, reference)?;
    let already_read: usize = originals.values().map(|(_, bytes)| bytes.len()).sum();
    ensure!(
        already_read
            .checked_add(bytes.len())
            .is_some_and(|total| total <= 64 * 1024 * 1024),
        "workspace_stage_source_byte_limit"
    );
    originals.insert(reference.path.clone(), (reference.clone(), bytes.clone()));
    Ok(bytes)
}

fn json_read(
    root: &SkillRoot,
    reference: &ArtifactReference,
    originals: &mut BTreeMap<String, (ArtifactReference, Vec<u8>)>,
) -> Result<Value> {
    Ok(oe_core::json::strict_json(&read(
        root, reference, originals,
    )?)?)
}

/// Verify a recorded stage through its existing native owner without dispatch,
/// repair, retry, registration, Project mutation or engineering adoption.
///
/// The execution snapshot may precede tool-result registration. Both snapshots
/// are checked by the original tool owner; changed objects/conditions/sources
/// reject current use. A Harness checkpoint is instead a complete state snapshot
/// and must bind the exact current Project. `references` returns the checked
/// source closure for the workspace owner's bounded archival transaction.
pub fn verify(
    root: &SkillRoot,
    stage: &WorkspaceStage,
    current: &Project,
    current_ref: &ArtifactReference,
) -> Result<Value> {
    dto(stage.validate())?;
    project::validate(current)?;
    ensure!(
        stage.execution == "results_recorded",
        "workspace_stage_result_not_recorded"
    );
    let reference = stage
        .receipt
        .as_ref()
        .context("workspace_stage_receipt_required")?;
    let mut originals = BTreeMap::new();
    let receipt: WorkspaceStageReceipt =
        serde_json::from_value(json_read(root, reference, &mut originals)?)?;
    dto(receipt.validate())?;
    ensure!(
        receipt.schema == "oev2.workspace-stage-receipt/1" && receipt.stage_id == stage.id,
        "workspace_stage_receipt_identity_mismatch"
    );
    ensure!(
        json_read(root, current_ref, &mut originals)? == serde_json::to_value(current)?,
        "workspace_stage_current_project_mismatch"
    );
    let execution_project: Project =
        serde_json::from_value(json_read(root, &receipt.project, &mut originals)?)?;
    project::validate(&execution_project)?;
    ensure!(
        execution_project.project_id == current.project_id
            && execution_project.purpose == current.purpose,
        "workspace_stage_project_mismatch"
    );
    let native = match receipt.owner.as_str() {
        "tool_result" => {
            let registry_ref = receipt
                .registry
                .as_ref()
                .context("workspace_stage_tool_registry_required")?;
            let registry: CapabilityRegistry =
                serde_json::from_value(json_read(root, registry_ref, &mut originals)?)?;
            let bytes = read(root, &receipt.artifact, &mut originals)?;
            let before = tool_result::verify(root, &execution_project, &registry, &bytes)?;
            ensure!(
                before.check.status == "accepted",
                "workspace_stage_execution_result_rejected: {:?}",
                before.check.failures
            );
            ensure!(
                !before.check.project_revised_since_run,
                "workspace_stage_execution_project_not_original"
            );
            let after = tool_result::verify(root, current, &registry, &bytes)?;
            ensure!(
                after.check.status == "accepted",
                "workspace_stage_current_result_rejected: {:?}",
                after.check.failures
            );
            let result = after
                .result
                .as_ref()
                .context("workspace_stage_tool_result_missing")?;
            ensure!(
                result.route_stage.as_deref() == Some(stage.id.as_str()),
                "workspace_stage_tool_route_mismatch"
            );
            ensure!(
                !stage.acceptance_ids.is_empty()
                    && stage
                        .acceptance_ids
                        .iter()
                        .all(|id| result.acceptance.contains(id)),
                "workspace_stage_tool_acceptance_mismatch"
            );
            // These are source references belonging to the existing native
            // result. Registry executable pins are not project artifacts.
            for record in &result.records {
                read(root, &record.artifact, &mut originals)?;
            }
            for input in &result.inputs.files {
                read(root, input, &mut originals)?;
            }
            let raw = oe_core::json::strict_json(&bytes)?;
            if !raw["subject"]["manifest"].is_null() {
                let manifest: ArtifactReference =
                    serde_json::from_value(raw["subject"]["manifest"].clone())?;
                read(root, &manifest, &mut originals)?;
            }
            json!({"owner":"tool_result", "execution":result.execution,
                "execution_project_check":before.check,"current_project_check":after.check,
                "scope":"Original result, registry, current executable and declared original records rechecked; reported findings are not authenticated observations or engineering acceptance"})
        }
        "harness" => {
            ensure!(
                receipt.registry.is_none(),
                "workspace_stage_harness_registry_unexpected"
            );
            ensure!(
                same_reference(&receipt.project, current_ref),
                "workspace_stage_harness_project_changed"
            );
            read(root, &receipt.artifact, &mut originals)?;
            let checkpoint = harness::load_step(root, &json!(receipt.artifact))?;
            ensure!(
                checkpoint["status"] == "recorded"
                    && checkpoint["task_id"] == stage.id
                    && checkpoint["state"]["project_id"] == current.project_id,
                "workspace_stage_harness_identity_mismatch"
            );
            let archive = checkpoint["artifacts"]
                .as_object()
                .context("workspace_stage_harness_artifacts_missing")?;
            let parent = Path::new(&receipt.artifact.path)
                .parent()
                .context("workspace_stage_harness_parent_missing")?;
            let mut input = None;
            let mut snapshot = None;
            for (relative, digest) in archive {
                oe_semantic::safe_relative(relative)?;
                let artifact = ArtifactReference {
                    path: parent
                        .join(relative)
                        .to_str()
                        .context("workspace_stage_artifact_path_invalid")?
                        .to_owned(),
                    sha256: digest
                        .as_str()
                        .context("workspace_stage_artifact_digest_invalid")?
                        .to_owned(),
                };
                let bytes = read(root, &artifact, &mut originals)?;
                if relative == "input.json" {
                    input = Some(oe_core::json::strict_json(&bytes)?);
                }
                if relative == "current-project.json" {
                    snapshot = Some(bytes);
                }
            }
            let input = input.context("workspace_stage_harness_input_missing")?;
            let declared: ArtifactReference =
                serde_json::from_value(input["harness"]["current_project"].clone())
                    .context("workspace_stage_harness_project_binding_missing")?;
            ensure!(
                same_reference(&declared, &receipt.project)
                    && sha256(
                        snapshot.context("workspace_stage_harness_project_snapshot_missing")?
                    ) == current_ref.sha256,
                "workspace_stage_harness_project_binding_mismatch"
            );
            let object_id = checkpoint["state"]["object_id"]
                .as_str()
                .context("workspace_stage_harness_object_missing")?;
            ensure!(
                current.objects.get(object_id).map(String::as_str)
                    == checkpoint["state"]["revision"].as_str(),
                "workspace_stage_harness_object_changed"
            );
            json!({"owner":"harness", "status":checkpoint["status"],
                "current_use":checkpoint["current_use"],"current_project_check":checkpoint["current_project_check"],
                "engineering_checks":checkpoint["engineering_checks"],"model_execution":checkpoint["model_execution"],
                "engineering_acceptance":checkpoint["engineering_acceptance"],
                "scope":"Existing Harness owner checked complete stored sources and internal consistency; recorded is not engineering acceptance"})
        }
        "exploration" => anyhow::bail!("workspace_stage_exploration_readback_unsupported"),
        _ => anyhow::bail!("workspace_stage_receipt_owner_unsupported"),
    };
    for (reference, original) in originals.values() {
        ensure!(
            artifacts::read(root, reference)? == *original,
            "workspace_stage_source_changed_during_readback"
        );
    }
    let references: Vec<_> = originals
        .values()
        .map(|(reference, _)| reference.clone())
        .collect();
    Ok(
        json!({"schema":"oev2.workspace-stage-readback/1","stage_id":stage.id,
        "status":"records_verified","receipt":reference,"project":current_ref,"execution_project":receipt.project,
        "native":native,"references":references,"engineering_adoption":"not_granted","rrsi_validation":"not_run"}),
    )
}
