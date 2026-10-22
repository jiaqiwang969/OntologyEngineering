//! Compare persisted observations, never infer intended topology from transient
//! tags or an approximate box. Native execution is established by the NX job.
use anyhow::{Context, Result, ensure};
use oe_core::{json::canonical_json, read_regular_bytes};
use oe_nx_protocol::{JobManifest, NativeResult, ResultOutcome, State, StateRecord, decode};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// Existing inputs/outputs and worker/source closure remain bound to the job.
/// This verifies recorded completion and bytes; it does not re-observe the OS.
pub(crate) fn receipt(root: &Path) -> Result<(NativeResult, JobManifest)> {
    let read = |name: &str| read_regular_bytes(&super::member(root, name)?, 16 * 1024 * 1024);
    let job: JobManifest = decode(&read("job.json")?)?;
    let result: NativeResult = decode(&read("native-result.json")?)?;
    let state: StateRecord = decode(&read("state.json")?)?;
    job.validate_parameters(&read("params.json")?)?;
    result.process.validate()?;
    result.runtime.validate()?;
    ensure!(
        result.schema == oe_nx_protocol::RESULT_SCHEMA
            && result.job_id == job.job_id
            && result.root == job.root
            && result.manifest_digest == job.digest()?
            && result.input_digest == job.input_digest()?
            && result.worker_manifest_digest == job.worker.digest()?
            && result.worker == job.worker.executable.identity
            && result.process.image == result.worker
            && result.process.host_id == job.root.host_id
            && result.runtime == job.worker.required_runtime
            && result.action == job.action,
        "drawing_native_receipt_identity_mismatch"
    );
    ensure!(
        state.state == State::Completed
            && state.job_id == job.job_id
            && state.root == job.root
            && state.manifest_digest == result.manifest_digest
            && state.process.as_ref() == Some(&result.process)
            && state.result_digest.as_ref() == Some(&oe_nx_protocol::record_digest(&result)?)
            && state
                .dispatch
                .as_ref()
                .is_some_and(|d| d.dispatch_id == result.dispatch_id),
        "drawing_native_completion_not_bound"
    );
    state
        .dispatch
        .as_ref()
        .context("drawing_dispatch_missing")?
        .validate(&job)?;
    ensure!(
        result.outcome == ResultOutcome::Succeeded
            && result.errors.is_empty()
            && result.unknowns.is_empty()
            && result.readback["lifecycle"]["terminate_return"] == 0,
        "drawing_native_execution_incomplete"
    );
    for file in job.payload_files().iter().chain(&result.artifacts) {
        file.identity.verify(&read_regular_bytes(
            &super::member(root, file.path.as_str())?,
            512 * 1024 * 1024,
        )?)?;
    }
    Ok((result, job))
}

/// Explicitly omit process-local tags, names and file paths from the comparison.
/// Matching observations do not establish unique parent or design identity.
pub fn observation_projection(part: &Value) -> Result<Value> {
    let a = &part["annotations"];
    ensure!(
        a["schema"] == "oev2.nx-annotation-observation/2",
        "drawing_association_schema_unsupported"
    );
    ensure!(
        a["units_query"]["return_code"] == 0
            && [Some("mm"), Some("inch")].contains(&a["units_query"]["unit"].as_str()),
        "drawing_association_units_unknown"
    );
    let dimensions = a["dimensions"]
        .as_array()
        .context("drawing_dimensions_missing")?;
    ensure!(!dimensions.is_empty(), "drawing_dimensions_absent");
    let mut observations = Vec::new();
    let mut unknowns = Vec::new();
    for (index, dimension) in dimensions.iter().enumerate() {
        let association = &dimension["associativity_parents"];
        if dimension["retained_query_code"] != 0
            || dimension["retained"].as_bool().is_none()
            || association["capture_complete"] != true
            || association["wrapper_code"] != 0
            || association["uf_return_code"] != 0
        {
            unknowns.push(json!({"dimension_index":index,"reason":"retained_or_association_query_incomplete"}));
            continue;
        }
        let entries = association["entries"]
            .as_array()
            .context("drawing_association_entries_missing")?;
        if entries.is_empty() {
            unknowns.push(json!({"dimension_index":index,"reason":"no_association_observed; design intent not established"}));
        }
        let mut normalized = Vec::new();
        for entry in entries {
            if entry["association_position_valid"] != true
                || entry["unit"] != a["units_query"]["unit"]
            {
                unknowns.push(json!({"dimension_index":index,"reason":"association_position_or_unit_unknown"}));
            }
            let mut parents = Vec::new();
            for role in ["object_1", "object_2", "view"] {
                let parent = &entry[role];
                if parent["present"] == false {
                    parents.push(json!({"present":false}));
                    continue;
                }
                let metadata = &parent["metadata"];
                if parent["present"] != true
                    || metadata["type_query"]["return_code"] != 0
                    || metadata["owning_part_query"]["value_valid"] != true
                {
                    unknowns.push(json!({"dimension_index":index,"role":role,"reason":"parent_type_or_owner_unknown"}));
                }
                if parent["geometry"]["called"] == true && parent["geometry"]["value_valid"] != true
                {
                    unknowns.push(json!({"dimension_index":index,"role":role,"reason":"parent_geometry_query_failed"}));
                }
                parents.push(json!({"present":parent["present"],"type":metadata["type_query"]["type_raw"],
                    "subtype":metadata["type_query"]["subtype_raw"],"same_owner":metadata["owning_part_query"]["matches_expected_owner"],
                    "geometry":parent["geometry"]}));
            }
            normalized.push(json!({"parents":parents,"point_type_raw":entry["point_type_raw"],"line_type_raw":entry["line_type_raw"],
                "position":entry["association_position"],"unit":entry["unit"]}));
        }
        normalized.sort_by_key(|v| canonical_json(v).expect("JSON value"));
        observations.push(json!({"retained":dimension["retained"],"associations":normalized}));
    }
    observations.sort_by_key(|v| canonical_json(v).expect("JSON value"));
    Ok(
        json!({"dimension_count":dimensions.len(),"observations":observations,"unknowns":unknowns,"unit":a["units_query"]["unit"]}),
    )
}

pub fn compare(saved: &Path, reopened: &Path) -> Result<Value> {
    compare_kind(saved, reopened, false)
}

/// Explicit source-bound creation support. Legacy maintenance comparison is unchanged.
pub fn compare_sheet_reference(saved: &Path, reopened: &Path) -> Result<Value> {
    compare_kind(saved, reopened, true)
}

fn compare_kind(saved: &Path, reopened: &Path, sheet_reference: bool) -> Result<Value> {
    let (before, saved_job) = receipt(saved)?;
    let (after, reopened_job) = receipt(reopened)?;
    ensure!(
        before.process != after.process
            && before.worker == after.worker
            && before.worker_manifest_digest == after.worker_manifest_digest
            && before.runtime == after.runtime,
        "drawing_reopen_process_or_tool_identity_mismatch"
    );
    ensure!(
        before.action
            == if sheet_reference {
                "create_sheet_reference_dimension"
            } else {
                "maintain_drawings"
            }
            && after.action == "inspect_detailed",
        "drawing_reopen_action_mismatch"
    );
    let old = before.readback["parts"]
        .as_array()
        .context("drawing_saved_parts_missing")?;
    let new = after.readback["parts"]
        .as_array()
        .context("drawing_reopened_parts_missing")?;
    let mut targets = BTreeMap::new();
    for part in new {
        let name = part["path"]
            .as_str()
            .context("drawing_reopened_path_missing")?
            .rsplit(['/', '\\'])
            .next()
            .unwrap();
        ensure!(
            targets.insert(name, part).is_none(),
            "drawing_duplicate_reopened_part"
        );
    }
    ensure!(
        !old.is_empty() && old.len() == targets.len(),
        "drawing_reopen_inventory_mismatch"
    );
    let sheet_parameters = if sheet_reference {
        let raw = read_regular_bytes(&super::member(saved, "params.json")?, 16 * 1024 * 1024)?;
        saved_job.validate_parameters(&raw)?;
        let parameters: oe_nx_worker::geometry::Parameters = decode(&raw)?;
        parameters.validate()?;
        ensure!(old.len() == 1, "sheet_reference_single_part_required");
        Some(parameters)
    } else {
        None
    };
    let mut reports = Vec::new();
    for part in old {
        let name = part["input"]
            .as_str()
            .context("drawing_saved_input_missing")?;
        let reopened_part = targets
            .remove(name)
            .context("drawing_reopened_part_missing")?;
        let raw = read_regular_bytes(
            &super::member(saved, &format!("out/{name}"))?,
            512 * 1024 * 1024,
        )?;
        let input = reopened_job
            .inputs
            .iter()
            .find(|f| f.path.as_str() == format!("inputs/{name}"))
            .context("drawing_reopen_input_binding_missing")?;
        input.identity.verify(&raw)?;
        let reference_check = if let Some(parameters) = &sheet_parameters {
            let oe_nx_worker::geometry::Parameters::CreateSheetReferenceDimension {
                part: expected_name,
                sheet,
                reference,
                text_origin_drawing,
                ..
            } = parameters
            else {
                anyhow::bail!("sheet_reference_parameters_required")
            };
            ensure!(expected_name == name, "sheet_reference_part_mismatch");
            let original = read_regular_bytes(
                &super::member(saved, &format!("inputs/{name}"))?,
                512 * 1024 * 1024,
            )?;
            saved_job
                .inputs
                .iter()
                .find(|f| f.path.as_str() == format!("inputs/{name}"))
                .context("sheet_reference_original_input_missing")?
                .identity
                .verify(&original)?;
            let tag = before.readback["creation"]["dimension_tag"]
                .as_u64()
                .filter(|v| *v > 0 && *v <= u32::MAX as u64)
                .context("sheet_reference_created_tag_missing")? as u32;
            Some(check_sheet_persistence(
                sheet,
                reference,
                *text_origin_drawing,
                &original,
                &raw,
                &part["before"],
                &part["after_save"],
                reopened_part,
                tag,
            )?)
        } else {
            None
        };
        let a = observation_projection(&part["after_save"])?;
        let b = observation_projection(reopened_part)?;
        let incomplete = !a["unknowns"].as_array().unwrap().is_empty()
            || !b["unknowns"].as_array().unwrap().is_empty();
        let mut report = json!({"part":name,"saved_input_identity":input.identity,"saved":a,"reopened":b,
            "observation_persistence":if incomplete {"unknown"}else if a==b {"matches"}else{"changed"},
            "intended_parent_correctness":"unknown","reassociation":"not_performed","visual_readability":"not_assessed",
            "reason":"transient tags excluded; matching positions/types/approximate boxes do not establish unique intended parent"});
        if let Some(check) = reference_check {
            report["sheet_reference_check"] = check;
        }
        reports.push(report);
    }
    Ok(
        json!({"schema":if sheet_reference {"oev2.sheet-reference-reopen/1"} else {"oev2.drawing-association-reopen/1"},"parts":reports,
        "saved_process":before.process,"reopened_process":after.process,"worker":after.worker,"worker_manifest_digest":after.worker_manifest_digest,
        "engineering_acceptance":"not_assessed","identity_scope":"recorded completion, job/source/tool/input/artifact hashes and independent process identities; not a fresh OS attestation"}),
    )
}

/// Reuse the worker's pure geometry checks; never carry tags between processes.
/// CAD source changes here only from the verified original to the verified saved
/// file. This does not change or adopt a Project object's revision.
#[allow(clippy::too_many_arguments)]
pub fn check_sheet_persistence(
    sheet: &oe_contracts::NativeDraftingPlan,
    reference: &oe_contracts::DrawingReferenceIntent,
    text: [f64; 2],
    original: &[u8],
    saved: &[u8],
    before: &Value,
    after_save: &Value,
    reopened: &Value,
    created_tag: u32,
) -> Result<Value> {
    use oe_nx_worker::geometry::sheet_dimensions as check;
    check::check_addition(before, after_save, created_tag)?;
    let creation = check::check_created(sheet, reference, text, original, after_save, created_tag)?;
    ensure!(
        reopened["view_refresh_requested"] == false,
        "sheet_reference_reopen_must_not_refresh"
    );
    let mut saved_sheet = sheet.clone();
    let mut saved_reference = reference.clone();
    let digest = oe_nx_protocol::BlobIdentity::of(saved)
        .sha256
        .as_str()
        .to_owned();
    saved_sheet.object.source_sha256 = digest.clone();
    saved_reference.object.source_sha256 = digest;
    let dimensions = reopened["annotations"]["dimensions"]
        .as_array()
        .context("sheet_reference_reopened_dimensions_missing")?;
    ensure!(
        dimensions.len() <= 4096,
        "sheet_reference_dimension_inventory_limit"
    );
    let mut matches = Vec::new();
    for d in dimensions {
        if let Some(tag) = d["dimension_tag"]
            .as_u64()
            .filter(|v| *v > 0 && *v <= u32::MAX as u64)
        {
            if let Ok(v) = check::check_created(
                &saved_sheet,
                &saved_reference,
                text,
                saved,
                reopened,
                tag as u32,
            ) {
                matches.push(v);
            }
        }
    }
    ensure!(
        matches.len() == 1,
        "sheet_reference_unique_reopened_dimension_unproven"
    );
    Ok(
        json!({"conclusion":"pass","created":creation,"reopened":matches.remove(0),
        "original_source":oe_nx_protocol::BlobIdentity::of(original),"saved_source":oe_nx_protocol::BlobIdentity::of(saved),
        "object_transition":"verified_saved_artifact_only; current Project revision requires separate binding",
        "manufacturing_drawing":"not_established","visual_readability":"not_assessed","engineering_adoption":"not_established"}),
    )
}
