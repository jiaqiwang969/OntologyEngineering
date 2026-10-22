//! Preflight for associated sheet dimensions. No native mutation or CAD release.
//! Reuses generated sheet/reference contracts and the existing geometry owners.
use super::{drafting, model_edges::CircleIntentRef};
use anyhow::{Context, Result, ensure};
use oe_contracts::{DrawingReferenceIntent, NativeDraftingPlan};
use serde_json::{Value, json};

pub fn validate(
    sheet: &NativeDraftingPlan,
    reference: &DrawingReferenceIntent,
    text: [f64; 2],
) -> Result<()> {
    drafting::validate(sheet)?;
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("sheet_dimension_reference_invalid"))?;
    ensure!(
        serde_json::to_value(&sheet.object)? == serde_json::to_value(&reference.object)?
            && sheet.unit == reference.unit,
        "sheet_dimension_object_or_unit_mismatch"
    );
    ensure!(
        matches!(
            reference.schema.as_str(),
            "oev2.drawing-reference-intent/2" | "oev2.drawing-reference-intent/3"
        ) && reference.proof_scope == "unique_model_parent"
            && reference.local_frame == "part_absolute"
            && reference.points.len() == 2
            && reference.position_tolerance.is_finite()
            && reference.position_tolerance >= 0.0,
        "sheet_dimension_reference_scope_invalid"
    );
    ensure!(
        !reference.source_refs.is_empty()
            && reference.source_refs.iter().all(|s| !s.trim().is_empty())
            && reference
                .source_refs
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == reference.source_refs.len(),
        "sheet_dimension_sources_invalid"
    );
    ensure!(
        text.iter().all(|v| v.is_finite())
            && text[0] > 0.0
            && text[0] < sheet.width
            && text[1] > 0.0
            && text[1] < sheet.height,
        "sheet_dimension_text_origin_outside_page"
    );
    let value = reference
        .dimension_value
        .as_ref()
        .context("sheet_dimension_value_missing")?;
    value
        .validate()
        .map_err(|_| anyhow::anyhow!("sheet_dimension_value_invalid"))?;
    ensure!(
        value.metric == "computed_linear"
            && value.unit == reference.unit
            && value.dimension_subtype_raw == 3
            && value.value.is_finite()
            && value.value > 0.0
            && value.numeric_tolerance.is_finite()
            && value.numeric_tolerance >= 0.0,
        "sheet_dimension_value_scope_invalid"
    );
    let mut features = std::collections::BTreeSet::new();
    let mut points = std::collections::BTreeSet::new();
    for point in &reference.points {
        point
            .validate()
            .map_err(|_| anyhow::anyhow!("sheet_dimension_point_invalid"))?;
        let edge = CircleIntentRef::for_point(&reference.schema, point)?
            .context("sheet_dimension_edge_missing")?;
        edge.validate()?;
        ensure!(
            features.insert(edge.feature_id().to_owned())
                && points.insert(&point.point_id)
                && edge.unit() == reference.unit
                && edge.local_frame() == reference.local_frame
                && edge.full_circle()
                && point.parent_type_raw == 70
                && point.parent_subtype_raw == 3
                && point.point_type_raw == 2
                && point.line_type_raw == 0
                && point.position.len() == 3
                && point.position.iter().all(|v| v.is_finite()),
            "sheet_dimension_parent_scope_invalid"
        );
        let c = edge.center();
        ensure!(
            (point.position[0] - c[0])
                .hypot(point.position[1] - c[1])
                .hypot(point.position[2] - c[2])
                <= reference.position_tolerance,
            "sheet_dimension_center_mismatch"
        );
    }
    let a = &reference.points[0].position;
    let b = &reference.points[1].position;
    ensure!(
        ((a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2]) - value.value).abs()
            <= value.numeric_tolerance,
        "sheet_dimension_independent_length_mismatch"
    );
    Ok(())
}

/// Same-process tags are provisional handles only. This does not authorize native
/// mutation: callers still own execution, source-copy, save and reopen checks.
pub fn prepare(
    sheet: &NativeDraftingPlan,
    reference: &DrawingReferenceIntent,
    text: [f64; 2],
    source_bytes: &[u8],
    part: &Value,
) -> Result<Value> {
    prepare_with_observation(sheet, reference, text, source_bytes, part, false)
}
fn prepare_with_observation(
    sheet: &NativeDraftingPlan,
    reference: &DrawingReferenceIntent,
    text: [f64; 2],
    source_bytes: &[u8],
    part: &Value,
    observation_only: bool,
) -> Result<Value> {
    validate(sheet, reference, text)?;
    ensure!(
        oe_nx_protocol::BlobIdentity::of(source_bytes)
            .sha256
            .as_str()
            == sheet.object.source_sha256,
        "sheet_dimension_source_mismatch"
    );
    ensure!(
        part["drafting_readback_schema"]
            == if observation_only {
                "oev2.native-drafting-observation/7"
            } else {
                "oev2.native-drafting-observation/6"
            },
        "sheet_dimension_current_drawing_observation_required"
    );
    let sheet_check = drafting::check(sheet, part);
    ensure!(
        sheet_check["conclusion"] == "pass",
        "sheet_dimension_drawing_plan_unproven"
    );
    let selected = &sheet_check["details"]["sheet"];
    let tag = |v: &Value| -> Result<u32> {
        let n = v.as_u64().context("sheet_dimension_native_tag_missing")?;
        ensure!(
            n > 0 && n <= u32::MAX as u64,
            "sheet_dimension_native_tag_invalid"
        );
        Ok(n as u32)
    };
    let sheet_tag = tag(&selected["drawing_tag"])?;
    let view = &selected["views"][0];
    let view_tag = tag(&view["view_tag"])?;
    let mut edges = Vec::new();
    let mut selections = Vec::new();
    for point in &reference.points {
        let selection = CircleIntentRef::for_point(&reference.schema, point)?
            .unwrap()
            .select(part)?;
        ensure!(
            selection["conclusion"] == "pass",
            "sheet_dimension_unique_parent_unproven"
        );
        let candidates = selection["candidate_edge_tags"]
            .as_array()
            .context("sheet_dimension_parent_inventory_missing")?;
        ensure!(
            candidates.len() == 1,
            "sheet_dimension_unique_parent_unproven"
        );
        edges.push(tag(&candidates[0])?);
        selections.push(selection);
    }
    ensure!(edges[0] != edges[1], "sheet_dimension_parents_not_distinct");
    let matrix: Vec<f64> = serde_json::from_value(view["rotation"]["matrix"].clone())?;
    // The drafting owner already verifies a finite right-handed orthonormal matrix.
    ensure!(matrix.len() == 9, "sheet_dimension_rotation_missing");
    let a = &reference.points[0].position;
    let b = &reference.points[1].position;
    let delta = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let x: f64 = (0..3).map(|i| matrix[i] * delta[i]).sum();
    let y: f64 = (0..3).map(|i| matrix[3 + i] * delta[i]).sum();
    let projected_length = x.hypot(y);
    let value = reference.dimension_value.as_ref().unwrap();
    ensure!(
        (projected_length - value.value).abs() <= value.numeric_tolerance && projected_length > 0.0,
        "sheet_dimension_projected_length_mismatch"
    );
    Ok(
        json!({"schema":"oev2.sheet-dimension-preflight/1","conclusion":"pass",
        "sheet_tag":sheet_tag,"view_tag":view_tag,"edge_tags":edges,"parent_selections":selections,
        "projected_model_length":projected_length,"unit":reference.unit,"text_origin_drawing":text,
        "sheet_check":sheet_check,"native_creation":"not_performed","independent_reopen":"not_performed",
        "visual_readability":"not_assessed","manufacturing_drawing":"not_established","engineering_adoption":"not_established",
        "scope":"source-bound preflight only; tags belong to this observation and cannot be reused across processes"}),
    )
}

/// Check the new dimension against reselected current geometry and member view.
/// A creation return code alone cannot satisfy this check.
pub fn check_created(
    sheet: &NativeDraftingPlan,
    reference: &DrawingReferenceIntent,
    text: [f64; 2],
    source: &[u8],
    part: &Value,
    dimension_tag: u32,
) -> Result<Value> {
    check_created_with_observation(sheet, reference, text, source, part, dimension_tag, false)
}
pub(super) fn check_created_with_observation(
    sheet: &NativeDraftingPlan,
    reference: &DrawingReferenceIntent,
    text: [f64; 2],
    source: &[u8],
    part: &Value,
    dimension_tag: u32,
    observation_only: bool,
) -> Result<Value> {
    let prepared =
        prepare_with_observation(sheet, reference, text, source, part, observation_only)?;
    ensure!(dimension_tag != 0, "sheet_dimension_created_tag_missing");
    let annotations = &part["annotations"];
    ensure!(
        annotations["schema"] == "oev2.nx-annotation-observation/2"
            && annotations["units_query"]["return_code"] == 0
            && annotations["units_query"]["unit"] == reference.unit,
        "sheet_dimension_annotation_units_unavailable"
    );
    let dimensions = annotations["dimensions"]
        .as_array()
        .context("sheet_dimension_annotation_inventory_missing")?;
    let mut ids = std::collections::BTreeSet::new();
    ensure!(
        dimensions.len() <= 4096
            && dimensions.iter().all(|d| d["dimension_tag"]
                .as_u64()
                .is_some_and(|n| n > 0 && n <= u32::MAX as u64 && ids.insert(n))),
        "sheet_dimension_annotation_inventory_invalid"
    );
    let dimension = dimensions
        .iter()
        .find(|d| d["dimension_tag"] == dimension_tag)
        .context("sheet_dimension_created_annotation_missing")?;
    ensure!(
        dimension["retained_query_code"] == 0 && dimension["retained"] == false,
        "sheet_dimension_retained_or_unknown"
    );
    let owned = |metadata: &Value| {
        metadata["type_query"]["return_code"] == 0
            && metadata["owning_part_query"]["value_valid"] == true
            && metadata["owning_part_query"]["matches_expected_owner"] == true
    };
    let value = &dimension["computed_size"];
    let required = reference.dimension_value.as_ref().unwrap();
    ensure!(
        value["wrapper_code"] == 0
            && value["value_valid"] == true
            && value["unit"] == reference.unit
            && value["basis"] == "NXOpen::Annotations::Dimension::ComputedSize"
            && owned(&value["metadata"])
            && value["metadata"]["type_query"]["type_raw"] == 26
            && value["metadata"]["type_query"]["subtype_raw"] == required.dimension_subtype_raw,
        "sheet_dimension_computed_value_unavailable"
    );
    ensure!(
        value["value"].as_f64().is_some_and(
            |v| v.is_finite() && (v - required.value).abs() <= required.numeric_tolerance
        ),
        "sheet_dimension_computed_value_mismatch"
    );
    let associations = &dimension["associativity_parents"];
    ensure!(
        associations["wrapper_code"] == 0
            && associations["uf_return_code"] == 0
            && associations["capture_complete"] == true,
        "sheet_dimension_associations_unavailable"
    );
    let entries = associations["entries"]
        .as_array()
        .context("sheet_dimension_associations_missing")?;
    ensure!(
        entries.len() == 2,
        "sheet_dimension_association_count_mismatch"
    );
    let mut actual = std::collections::BTreeSet::new();
    for entry in entries {
        let parent = &entry["object_1"];
        let view = &entry["view"];
        ensure!(
            parent["present"] == true
                && owned(&parent["metadata"])
                && parent["metadata"]["type_query"]["type_raw"] == 70
                && parent["metadata"]["type_query"]["subtype_raw"] == 3
                && entry["object_2"]["present"] == false
                && entry["object_2"]["tag"] == 0
                && entry["point_type_raw"] == 2
                && entry["line_type_raw"] == 0
                && entry["unit"] == reference.unit,
            "sheet_dimension_parent_association_mismatch"
        );
        ensure!(
            view["present"] == true
                && owned(&view["metadata"])
                && view["tag"] == prepared["view_tag"],
            "sheet_dimension_member_view_mismatch"
        );
        let n = parent["tag"]
            .as_u64()
            .context("sheet_dimension_parent_tag_missing")?;
        ensure!(
            n > 0 && n <= u32::MAX as u64 && actual.insert(n),
            "sheet_dimension_parent_tag_duplicate_or_invalid"
        );
    }
    let expected: std::collections::BTreeSet<u64> =
        serde_json::from_value(prepared["edge_tags"].clone())?;
    ensure!(actual == expected, "sheet_dimension_unique_parents_differ");
    Ok(
        json!({"schema":"oev2.sheet-dimension-check/1","conclusion":"pass","dimension_tag":dimension_tag,
        "preflight":prepared,"computed_size":value,"native_member_view_associations":entries,
        "visual_readability":"not_assessed","manufacturing_drawing":"not_established","engineering_adoption":"not_established",
        "scope":"raw native dimension, same-part circular parents and member-view relation; save and independent reopening require their own execution"}),
    )
}

/// Exact same-process observed fields; not independent geometric equivalence.
pub fn check_geometry_preserved(before: &Value, after: &Value) -> Result<()> {
    for field in ["units", "solid_count", "solid_tags", "bbox", "model_edges"] {
        ensure!(
            !before[field].is_null() && before[field] == after[field],
            "sheet_dimension_original_geometry_changed"
        );
    }
    Ok(())
}

pub fn check_addition(before: &Value, after: &Value, new_tag: u32) -> Result<()> {
    check_geometry_preserved(before, after)?;
    let tags = |part: &Value| -> Result<std::collections::BTreeSet<u64>> {
        ensure!(
            part["annotations"]["schema"] == "oev2.nx-annotation-observation/2",
            "sheet_dimension_inventory_schema_invalid"
        );
        let dims = part["annotations"]["dimensions"]
            .as_array()
            .context("sheet_dimension_inventory_missing")?;
        let mut result = std::collections::BTreeSet::new();
        ensure!(
            dims.len() <= 4096
                && dims.iter().all(|d| d["dimension_tag"]
                    .as_u64()
                    .is_some_and(|n| n > 0 && n <= u32::MAX as u64 && result.insert(n))),
            "sheet_dimension_inventory_invalid"
        );
        Ok(result)
    };
    let mut expected = tags(before)?;
    ensure!(
        new_tag > 0 && expected.insert(u64::from(new_tag)),
        "sheet_dimension_created_tag_not_new"
    );
    ensure!(
        expected == tags(after)?,
        "sheet_dimension_annotation_addition_mismatch"
    );
    Ok(())
}
