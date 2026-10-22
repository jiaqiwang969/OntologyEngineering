//! Explicit nominal diameter reception. Original linear-dimension contracts stay unchanged.
use anyhow::{Context, Result, ensure};
use oe_contracts::SheetDiameterIntent;
use oe_core::read_regular_bytes;
use oe_nx_protocol::{BlobIdentity, decode};
use oe_nx_worker::geometry::{Parameters, drawing_retirement, sheet_diameter};
use serde_json::{Value, json};
use std::path::Path;

fn dimensions(part: &Value) -> Result<&Vec<Value>> {
    let d = part["annotations"]["dimensions"]
        .as_array()
        .context("diameter_reopen_inventory_missing")?;
    ensure!(d.len() <= 4096, "diameter_reopen_inventory_limit");
    Ok(d)
}

/// Existing dimensions use the original complete linear signature owner.
/// Other existing dimension types need an explicit reception contract.
fn prior_signatures(part: &Value, exclude: Option<u32>, unit: &str) -> Result<Vec<String>> {
    let mut values = Vec::new();
    for d in dimensions(part)? {
        if exclude.is_some_and(|n| d["dimension_tag"] == n) {
            continue;
        }
        values.push(
            drawing_retirement::signature(d, unit)?["sha256"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    values.sort();
    Ok(values)
}

#[allow(clippy::too_many_arguments)]
pub fn check_persistence(
    i: &SheetDiameterIntent,
    original: &[u8],
    saved: &[u8],
    before: &Value,
    after_save: &Value,
    reopened: &Value,
    id: u32,
) -> Result<Value> {
    sheet_diameter::check_addition(before, after_save, id)?;
    let created = sheet_diameter::check_created(i, original, after_save, id)?;
    ensure!(
        reopened["view_refresh_requested"] == false,
        "diameter_reopen_must_not_refresh"
    );
    let mut current = i.clone();
    current.sheet.object.source_sha256 = BlobIdentity::of(saved).sha256.as_str().to_owned();
    let mut candidates = Vec::new();
    for d in dimensions(reopened)? {
        if let Some(n) = d["dimension_tag"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        {
            if let Ok(proof) = sheet_diameter::check_created(&current, saved, reopened, n as u32) {
                candidates.push((n as u32, proof));
            }
        }
    }
    ensure!(
        candidates.len() == 1,
        "diameter_unique_reopened_dimension_unproven"
    );
    let (new_id, proof) = candidates.remove(0);
    let expected = prior_signatures(before, None, &i.sheet.unit)?;
    ensure!(
        expected == prior_signatures(after_save, Some(id), &i.sheet.unit)?
            && expected == prior_signatures(reopened, Some(new_id), &i.sheet.unit)?,
        "diameter_prior_dimension_persistence_mismatch"
    );
    let a = super::associations::observation_projection(after_save)?;
    let b = super::associations::observation_projection(reopened)?;
    ensure!(
        a["unknowns"] == json!([]) && b["unknowns"] == json!([]) && a == b,
        "diameter_association_persistence_unproven"
    );
    Ok(
        json!({"conclusion":"pass","created":created,"reopened":proof,"prior_dimension_signatures":expected,
        "original_source":BlobIdentity::of(original),"saved_source":BlobIdentity::of(saved),
        "manufacturing_drawing":"not_established","visual_readability":"not_assessed",
        "engineering_adoption":"not_established","project_revision_update":"not_performed"}),
    )
}

pub fn compare(saved: &Path, reopened: &Path) -> Result<Value> {
    let (a, job) = super::associations::receipt(saved)?;
    let (b, reopen_job) = super::associations::receipt(reopened)?;
    ensure!(
        a.process != b.process
            && a.worker == b.worker
            && a.worker_manifest_digest == b.worker_manifest_digest
            && a.runtime == b.runtime,
        "diameter_reopen_process_or_tool_identity_mismatch"
    );
    ensure!(
        a.action == "create_sheet_diameter_dimension" && b.action == "inspect_detailed",
        "diameter_reopen_action_mismatch"
    );
    let read = |root: &Path, name: &str| {
        read_regular_bytes(&super::member(root, name)?, 512 * 1024 * 1024)
    };
    let params = read(saved, "params.json")?;
    job.validate_parameters(&params)?;
    let parameters: Parameters = decode(&params)?;
    parameters.validate()?;
    let Parameters::CreateSheetDiameterDimension {
        part: name, intent, ..
    } = parameters
    else {
        anyhow::bail!("diameter_reopen_parameters_required")
    };
    let old = a.readback["parts"]
        .as_array()
        .context("diameter_saved_parts_missing")?;
    let new = b.readback["parts"]
        .as_array()
        .context("diameter_reopened_parts_missing")?;
    ensure!(
        old.len() == 1
            && new.len() == 1
            && old[0]["input"] == name
            && new[0]["path"]
                .as_str()
                .is_some_and(|p| p.rsplit(['/', '\\']).next() == Some(name.as_str())),
        "diameter_reopen_part_mismatch"
    );
    let input_name = format!("inputs/{name}");
    let original = read(saved, &input_name)?;
    let bytes = read(saved, &format!("out/{name}"))?;
    job.inputs
        .iter()
        .find(|f| f.path.as_str() == input_name)
        .context("diameter_original_missing")?
        .identity
        .verify(&original)?;
    reopen_job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == input_name)
        .context("diameter_saved_input_missing")?
        .identity
        .verify(&bytes)?;
    let id = a.readback["creation"]["dimension_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("diameter_created_tag_missing")? as u32;
    let check = check_persistence(
        &intent,
        &original,
        &bytes,
        &old[0]["before"],
        &old[0]["after_save"],
        &new[0],
        id,
    )?;
    Ok(
        json!({"schema":if intent.schema==sheet_diameter::INTENT_SCHEMA2 {"oev2.sheet-diameter-reopen/2"} else {"oev2.sheet-diameter-reopen/1"},"conclusion":"pass","check":check,
        "saved_process":a.process,"reopened_process":b.process,"worker":b.worker,"worker_manifest_digest":b.worker_manifest_digest,
        "manufacturing_drawing":"not_established","visual_readability":"not_assessed","engineering_adoption":"not_established",
        "scope":"recorded completed jobs, byte-bound saved source and independent process; no fresh OS attestation or engineering adoption"}),
    )
}
