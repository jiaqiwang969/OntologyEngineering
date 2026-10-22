//! One explicit prototype drawing page; source, geometry and manufacture differ.
use anyhow::Result;
use oe_contracts::{NativeDraftingPlan, NativeDrawingObservation};
use serde_json::Value;
use std::path::Path;

pub fn validate(plan: &NativeDraftingPlan) -> Result<()> {
    oe_nx_worker::geometry::drafting::validate(plan)
}

pub fn check(plan: &NativeDraftingPlan, part: &Value) -> Result<Value> {
    validate(plan)?;
    Ok(oe_nx_worker::geometry::drafting::check(plan, part))
}

/// The same completed-job observer binds the full original part, including
/// failed or unsupported queries. No synthetic reference intent is necessary.
pub fn observe(
    job_root: &Path,
    input: &str,
    plan: &NativeDraftingPlan,
    native_result_ref: &str,
) -> Result<NativeDrawingObservation> {
    validate(plan)?;
    super::intent::observe_part_bound(job_root, input, &plan.object, native_result_ref)
}

/// Receive one prototype page created in a job-owned copy and independently
/// reopened. This does not adopt a Project revision or validate its dimensions.
pub fn compare_saved_reopened(saved: &Path, reopened: &Path) -> Result<Value> {
    use anyhow::{Context, ensure};
    use oe_core::read_regular_bytes;
    use oe_nx_protocol::{BlobIdentity, decode, record_digest};
    use oe_nx_worker::geometry::{Parameters, READBACK_SCHEMA, sheet_dimensions};
    use serde_json::json;

    let (created, create_job) = super::associations::receipt(saved)?;
    let (inspected, inspect_job) = super::associations::receipt(reopened)?;
    ensure!(
        created.process != inspected.process
            && created.worker == inspected.worker
            && created.worker_manifest_digest == inspected.worker_manifest_digest
            && created.runtime == inspected.runtime
            && (create_job.root != inspect_job.root || create_job.job_id != inspect_job.job_id),
        "drafting_reopen_process_or_tool_identity_mismatch"
    );
    ensure!(
        created.action == "create_drafting_sheet"
            && inspected.action == "inspect_detailed"
            && created.readback_schema == READBACK_SCHEMA
            && inspected.readback_schema == READBACK_SCHEMA,
        "drafting_reopen_action_or_readback_schema_mismatch"
    );
    let parse = |directory: &Path| -> Result<Parameters> {
        let p: Parameters = decode(&read_regular_bytes(
            &super::member(directory, "params.json")?,
            16 * 1024 * 1024,
        )?)?;
        p.validate()?;
        Ok(p)
    };
    let creation_parameters = parse(saved)?;
    let inspection_parameters = parse(reopened)?;
    ensure!(
        creation_parameters.action() == create_job.action
            && creation_parameters.parameter_schema() == create_job.params_schema
            && inspection_parameters.action() == inspect_job.action
            && inspection_parameters.parameter_schema() == inspect_job.params_schema,
        "drafting_reopen_parameter_binding_mismatch"
    );
    let Parameters::CreateDraftingSheet { part, plan, .. } = creation_parameters else {
        anyhow::bail!("drafting_reopen_creation_parameters_required")
    };
    let Parameters::InspectDetailed { parts, .. } = inspection_parameters else {
        anyhow::bail!("drafting_reopen_inspection_parameters_required")
    };
    ensure!(
        parts.len() == 1
            && parts[0] == part
            && create_job.inputs.len() == 1
            && inspect_job.inputs.len() == 1,
        "drafting_reopen_single_part_inventory_required"
    );
    let input_name = format!("inputs/{part}");
    let output_name = format!("out/{part}");
    ensure!(
        create_job.inputs[0].path.as_str() == input_name
            && inspect_job.inputs[0].path.as_str() == input_name,
        "drafting_reopen_input_path_mismatch"
    );
    let original = read_regular_bytes(&super::member(saved, &input_name)?, 512 * 1024 * 1024)?;
    let output = read_regular_bytes(&super::member(saved, &output_name)?, 512 * 1024 * 1024)?;
    let original_identity = BlobIdentity::of(&original);
    let output_identity = BlobIdentity::of(&output);
    ensure!(
        original_identity.sha256.as_str() == plan.object.source_sha256,
        "drafting_reopen_original_plan_source_mismatch"
    );
    let artifacts: Vec<_> = created
        .artifacts
        .iter()
        .filter(|f| {
            oe_nx_protocol::path_collision_key(f.path.as_str())
                == oe_nx_protocol::path_collision_key(&output_name)
        })
        .collect();
    ensure!(
        artifacts.len() == 1
            && artifacts[0].path.as_str() == output_name
            && artifacts[0].identity == output_identity
            && inspect_job.inputs[0].identity == output_identity
            && original_identity != output_identity,
        "drafting_reopen_saved_artifact_lineage_mismatch"
    );
    let states = created.readback["parts"]
        .as_array()
        .context("drafting_reopen_saved_observations_missing")?;
    let observations = inspected.readback["parts"]
        .as_array()
        .context("drafting_reopen_observations_missing")?;
    ensure!(
        states.len() == 1 && observations.len() == 1 && states[0]["input"] == part,
        "drafting_reopen_observation_inventory_mismatch"
    );
    let before = &states[0]["before"];
    let after = &states[0]["after"];
    let after_save = &states[0]["after_save"];
    let observed = &observations[0];
    let path = |value: &Value, relative: &str| -> Result<String> {
        let actual = super::windows_identity(
            value["path"]
                .as_str()
                .context("drafting_reopen_observed_path_missing")?,
        )?;
        let suffix = format!("/{}", super::windows_identity(relative)?);
        ensure!(
            actual.len() > suffix.len() && actual.ends_with(&suffix),
            "drafting_reopen_observed_path_mismatch"
        );
        Ok(actual)
    };
    let saved_path = path(before, &output_name)?;
    ensure!(
        path(after, &output_name)? == saved_path && path(after_save, &output_name)? == saved_path,
        "drafting_reopen_saved_part_path_changed"
    );
    path(observed, &input_name)?;
    let tag = before["part_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("drafting_reopen_original_part_tag_missing")?;
    ensure!(
        after["part_tag"] == tag && after_save["part_tag"] == tag,
        "drafting_reopen_saved_part_owner_changed"
    );
    ensure!(
        created.readback["action"] == "create_drafting_sheet"
            && inspected.readback["action"] == "fresh_process_detailed_readback"
            && created.readback["creation"]["complete"] == true
            && created.readback["plan"] == serde_json::to_value(&plan)?
            && inspected.readback["save_requested"] == false
            && observed["view_refresh_requested"] == false,
        "drafting_reopen_creation_or_readonly_reopen_incomplete"
    );
    // Existing same-process geometry owner; no cross-process tag comparison.
    sheet_dimensions::check_geometry_preserved(before, after)?;
    sheet_dimensions::check_geometry_preserved(before, after_save)?;
    ensure!(
        before["annotations"].is_object()
            && before["annotations"] == after["annotations"]
            && before["annotations"] == after_save["annotations"],
        "drafting_reopen_existing_annotations_changed"
    );

    let absent = check(&plan, before)?;
    ensure!(
        absent["conclusion"] == "violated"
            && absent["reason"] == "native_drafting_named_sheet_missing",
        "drafting_reopen_new_sheet_not_independently_absent"
    );
    let created_tag = created.readback["creation"]["sheet_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("drafting_reopen_created_sheet_tag_missing")?;
    let mut expected = sheet_inventory(before)?;
    ensure!(
        expected.insert(created_tag),
        "drafting_reopen_sheet_tag_not_new"
    );
    ensure!(
        sheet_inventory(after)? == expected
            && sheet_inventory(after_save)? == expected
            && sheet_inventory(observed)?.len() == expected.len(),
        "drafting_reopen_sheet_inventory_changed"
    );
    for stage in [after, after_save] {
        let matches: Vec<_> = stage["drawing_sheets"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["metadata"]["name_query"]["utf8"] == plan.sheet_name)
            .collect();
        ensure!(
            matches.len() == 1 && matches[0]["drawing_tag"] == created_tag,
            "drafting_reopen_created_sheet_identity_mismatch"
        );
    }
    let mut saved_plan = plan.clone();
    saved_plan.object.source_sha256 = output_identity.sha256.as_str().to_owned();
    let checks = json!({"after_creation":check(&plan, after)?,
        "after_save":check(&saved_plan, after_save)?,
        "independent_reopen":check(&saved_plan, observed)?});
    let conclusion = if checks
        .as_object()
        .unwrap()
        .values()
        .all(|v| v["conclusion"] == "pass")
    {
        "pass"
    } else if checks
        .as_object()
        .unwrap()
        .values()
        .any(|v| v["conclusion"] == "unknown")
    {
        "unknown"
    } else {
        "violated"
    };
    // Re-read original payloads, output inventory and terminal bindings before
    // emitting a receipt; a concurrent rewrite cannot become a valid result.
    for (directory, expected_result, expected_job) in [
        (saved, &created, &create_job),
        (reopened, &inspected, &inspect_job),
    ] {
        let (now, job) = super::associations::receipt(directory)?;
        ensure!(
            record_digest(&now)? == record_digest(expected_result)?
                && job.digest()? == expected_job.digest()?,
            "drafting_reopen_inputs_changed_during_check"
        );
    }
    Ok(
        json!({"schema":"oev2.prototype-drafting-reopen/1","conclusion":conclusion,
        "original_plan":plan,"saved_observation_plan":saved_plan,
        "original_identity":original_identity,"saved_identity":output_identity,
        "saved_job_digest":create_job.digest()?,"reopened_job_digest":inspect_job.digest()?,
        "saved_process":created.process,"reopened_process":inspected.process,
        "worker":inspected.worker,"worker_manifest_digest":inspected.worker_manifest_digest,
        "checks":checks,"observations":{"before":before,"after_creation":after,
            "after_save":after_save,"independent_reopen":observed},
        "same_process_geometry_fields":"unchanged_under_existing_owner",
        "independent_geometry_equivalence":"not_assessed",
        "existing_annotation_validity":"not_assessed","dimension_obligations":"unchanged",
        "project_revision_update":"not_performed","manufacturing_drawing":"not_established",
        "engineering_adoption":"not_established","visual_readability":"not_assessed",
        "scope":"created prototype page and frozen layout, recorded source/tool/input/output lineage and independent process; not fresh OS attestation, full geometry equivalence or engineering acceptance"}),
    )
}

fn sheet_inventory(part: &Value) -> Result<std::collections::BTreeSet<u64>> {
    use anyhow::{Context, ensure};
    ensure!(
        part["drawing_catalog_complete"] == true,
        "drafting_reopen_sheet_catalog_incomplete"
    );
    let sheets = part["drawing_sheets"]
        .as_array()
        .context("drafting_reopen_sheet_inventory_missing")?;
    ensure!(
        sheets.len() <= 10_000 && part["drawing_sheet_count"].as_u64() == Some(sheets.len() as u64),
        "drafting_reopen_sheet_count_mismatch"
    );
    let mut tags = std::collections::BTreeSet::new();
    for sheet in sheets {
        let tag = sheet["drawing_tag"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .context("drafting_reopen_sheet_tag_invalid")?;
        ensure!(tags.insert(tag), "drafting_reopen_duplicate_sheet_tag");
    }
    Ok(tags)
}
