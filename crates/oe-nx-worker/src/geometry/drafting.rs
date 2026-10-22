//! Prototype geometry sheet checks. Native tags never become persistent identity.
use anyhow::{Result, ensure};
use oe_contracts::{NativeDraftingPlan, NativeLayoutReference};
use serde_json::{Value, json};

// Rotation rows describe an orthonormal, right-handed frame. Agreement between
// two invalid matrices cannot establish a named-view orientation. This is the
// same bounded numerical requirement used when deriving import coordinates.
fn rotation_valid(rotation: &[f64]) -> bool {
    if rotation.len() != 9 || !rotation.iter().all(|n| n.is_finite()) {
        return false;
    }
    for i in 0..3 {
        for j in 0..3 {
            let dot: f64 = (0..3)
                .map(|k| rotation[i * 3 + k] * rotation[j * 3 + k])
                .sum();
            if (dot - if i == j { 1.0 } else { 0.0 }).abs() > 1e-8 || !dot.is_finite() {
                return false;
            }
        }
    }
    let determinant = rotation[0] * (rotation[4] * rotation[8] - rotation[5] * rotation[7])
        - rotation[1] * (rotation[3] * rotation[8] - rotation[5] * rotation[6])
        + rotation[2] * (rotation[3] * rotation[7] - rotation[4] * rotation[6]);
    determinant.is_finite() && (determinant - 1.0).abs() <= 1e-8
}

/// A placement hint from the union of all current solid bounding boxes.
/// It is not a center of mass, exact geometry, tolerance or engineering evidence.
/// The unchanged native page/visibility checks must still validate the result.
pub fn layout_anchor_from_bounds(bounds: &[[f64; 6]]) -> Result<[f64; 3]> {
    ensure!(
        !bounds.is_empty() && bounds.len() <= 10_000,
        "native_drafting_layout_bounds_inventory_invalid"
    );
    let mut lower = [f64::INFINITY; 3];
    let mut upper = [f64::NEG_INFINITY; 3];
    for bound in bounds {
        ensure!(
            bound.iter().all(|value| value.is_finite()),
            "native_drafting_layout_bounds_nonfinite"
        );
        for axis in 0..3 {
            ensure!(
                bound[axis] <= bound[axis + 3],
                "native_drafting_layout_bounds_inverted"
            );
            lower[axis] = lower[axis].min(bound[axis]);
            upper[axis] = upper[axis].max(bound[axis + 3]);
        }
    }
    let mut point = [0.0; 3];
    for axis in 0..3 {
        point[axis] = lower[axis] / 2.0 + upper[axis] / 2.0;
        ensure!(
            point[axis].is_finite() && point[axis] >= lower[axis] && point[axis] <= upper[axis],
            "native_drafting_layout_anchor_invalid"
        );
    }
    Ok(point)
}

/// Bounded import-coordinate candidate for unpanned orthographic named views.
/// The vendor mapped point and original page requirements independently check it.
pub fn layout_import_reference(
    anchor: [f64; 3],
    rotation: [f64; 9],
    model_center: [f64; 3],
    desired: [f64; 2],
    scale: f64,
) -> Result<[f64; 2]> {
    ensure!(
        anchor
            .iter()
            .chain(rotation.iter())
            .chain(model_center.iter())
            .chain(desired.iter())
            .all(|n| n.is_finite())
            && scale.is_finite()
            && scale > 0.0,
        "native_drafting_import_coordinates_nonfinite_or_scale_invalid"
    );
    ensure!(
        model_center == [0.0; 3],
        "native_drafting_nonzero_model_view_center_unsupported"
    );
    ensure!(
        rotation_valid(&rotation),
        "native_drafting_model_rotation_invalid"
    );
    let mut point = desired;
    for axis in 0..2 {
        let projected: f64 = (0..3)
            .map(|k| rotation[axis * 3 + k] * (anchor[k] - model_center[k]))
            .sum();
        point[axis] -= scale * projected;
    }
    ensure!(
        point.iter().all(|n| n.is_finite()),
        "native_drafting_import_coordinate_overflow"
    );
    Ok(point)
}

pub fn validate(plan: &NativeDraftingPlan) -> Result<()> {
    plan.validate()
        .map_err(|e| anyhow::anyhow!("native_drafting_plan_invalid:{e:?}"))?;
    plan.object
        .validate()
        .map_err(|_| anyhow::anyhow!("native_drafting_object_invalid"))?;
    for (name, value) in [
        ("sheet", &plan.sheet_name),
        ("model_view", &plan.model_view_name),
    ] {
        ensure!(
            !value.trim().is_empty()
                && value.len() <= 64
                && value.is_ascii()
                && !value.bytes().any(|v| v < 0x20 || v == 0x7f),
            "native_drafting_{name}_name_invalid"
        );
    }
    ensure!(
        plan.width.is_finite()
            && plan.width > 0.0
            && plan.width <= 100_000.0
            && plan.height.is_finite()
            && plan.height > 0.0
            && plan.height <= 100_000.0,
        "native_drafting_sheet_size_invalid"
    );
    ensure!(
        plan.scale_numerator.is_finite()
            && plan.scale_numerator > 0.0
            && plan.scale_denominator.is_finite()
            && plan.scale_denominator > 0.0
            && (plan.scale_numerator / plan.scale_denominator).is_finite()
            && (plan.scale_numerator / plan.scale_denominator) > 0.0,
        "native_drafting_scale_invalid"
    );
    ensure!(
        plan.view_center.len() == 2
            && plan.view_center.iter().all(|v| v.is_finite())
            && plan.view_center[0] >= 0.0
            && plan.view_center[0] <= plan.width
            && plan.view_center[1] >= 0.0
            && plan.view_center[1] <= plan.height,
        "native_drafting_reference_point_invalid"
    );
    ensure!(
        !plan.source_refs.is_empty() && plan.source_refs.iter().all(|s| !s.trim().is_empty()),
        "native_drafting_source_missing"
    );
    Ok(())
}

// Floating native DTO comparison only. This is neither manufacturing allowance
// nor measurement uncertainty; engineering tolerance must have its own contract.
fn near(actual: f64, expected: f64) -> bool {
    actual.is_finite()
        && expected.is_finite()
        && (actual - expected).abs() <= 1e-7 * expected.abs().max(1.0)
}
fn vector(value: &Value, length: usize) -> Option<Vec<f64>> {
    let values = value.as_array()?;
    if values.len() != length {
        return None;
    }
    values
        .iter()
        .map(|v| v.as_f64().filter(|n| n.is_finite()))
        .collect()
}
fn metadata_valid(value: &Value) -> bool {
    value["type_query"]["return_code"] == 0
        && value["owning_part_query"]["return_code"] == 0
        && value["owning_part_query"]["value_valid"] == true
        && value["owning_part_query"]["matches_expected_owner"] == true
        && value["name_query"]["return_code"] == 0
        && value["name_query"]["value_valid"] == true
}
fn source_path(path: &str) -> String {
    path.strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('/', "\\")
        .to_ascii_lowercase()
}
/// Installed uf_draw_types.h documents these width/height values and enum
/// ordinals. The thin ABI asserts those ordinals and copies the union family;
/// standardisation by NX does not change the independently specified page size.
pub fn standard_sheet_dimensions(family: i32, code: i32, unit: &str) -> Option<[f64; 2]> {
    match (family, code, unit) {
        (1, 1, "mm") => Some([1189.0, 841.0]),
        (1, 2, "mm") => Some([841.0, 594.0]),
        (1, 3, "mm") => Some([594.0, 420.0]),
        (1, 4, "mm") => Some([420.0, 297.0]),
        (1, 5, "mm") => Some([297.0, 210.0]),
        (2, 1, "inch") => Some([11.0, 8.5]),
        (2, 2, "inch") => Some([17.0, 11.0]),
        (2, 3, "inch") => Some([22.0, 17.0]),
        (2, 4, "inch") => Some([34.0, 22.0]),
        (2, 5, "inch") => Some([44.0, 34.0]),
        (2, 6, "inch") => Some([40.0, 28.0]),
        (2, 7, "inch") => Some([44.0, 28.0]),
        (2, 8, "inch") => Some([55.0, 34.0]),
        _ => None,
    }
}
fn outcome(plan: &NativeDraftingPlan, conclusion: &str, reason: &str, details: Value) -> Value {
    json!({"schema":"oev2.native-drafting-check/1", "object":plan.object,
        "sheet_name":plan.sheet_name, "conclusion":conclusion, "reason":reason,
        "details":details, "purpose":plan.purpose,
        "coverage":"one custom sheet and one imported named model view; geometric readback only",
        "diagnostic_precision":"1e-7 relative to max(1, expected magnitude); not engineering tolerance",
        "engineering_adoption":"not_established",
        "manufacturing_drawing":"not_established", "visual_readability":"not_assessed"})
}

/// /7 records queries of an already active context. A missing setter result
/// means not called, never an inferred successful setter return.
pub fn check_prepared_context(part: u32, sheet: u32, display: &Value, state: &Value) -> bool {
    part != 0
        && sheet != 0
        && display["set_called"] == false
        && display.get("return_code") == Some(&Value::Null)
        && display["query"] == "UF_PART_ask_display_part"
        && display["actual_part"] == part
        && display["expected_part"] == part
        && display["matches_expected"] == true
        && state["set_called"] == false
        && state.get("set_code") == Some(&Value::Null)
        && state["query_code"] == 0
        && state["state_raw"] == 2
        && state["current_drawing_code"] == 0
        && state["current_drawing_tag"] == sheet
        && state["expected_drawing_tag"] == sheet
        && state["work_view_code"] == 0
        && state["work_view_tag"]
            .as_u64()
            .is_some_and(|n| n > 0 && n <= u32::MAX as u64)
        && state["work_view_type"]["return_code"] == 0
        && state["work_view_type"]["type_raw"] == 2
        && state["work_view_type"]["kind"] == "drawing_sheet"
        && state["capture_complete"] == true
}

pub fn check(plan: &NativeDraftingPlan, part: &Value) -> Value {
    if let Err(error) = validate(plan) {
        return outcome(
            plan,
            "unknown",
            "native_drafting_plan_invalid",
            json!({"error":error.to_string()}),
        );
    }
    let prepared = part["drafting_readback_schema"] == "oev2.native-drafting-observation/7";
    let direct_layout =
        prepared || part["drafting_readback_schema"] == "oev2.native-drafting-observation/6";
    let centered_model = part["drafting_readback_schema"] == "oev2.native-drafting-observation/5";
    let active_display = centered_model || direct_layout;
    let mapped_anchor =
        centered_model || part["drafting_readback_schema"] == "oev2.native-drafting-observation/4";
    let layout_anchor =
        mapped_anchor || part["drafting_readback_schema"] == "oev2.native-drafting-observation/3";
    let object_boundary = direct_layout
        || layout_anchor
        || part["drafting_readback_schema"] == "oev2.native-drafting-observation/2";
    if (!object_boundary
        && part["drafting_readback_schema"] != "oev2.native-drafting-observation/1")
        || part["drawing_catalog_complete"] != true
    {
        return outcome(
            plan,
            "unknown",
            "native_drafting_catalog_incomplete",
            json!({}),
        );
    }
    if part["units"] != plan.unit {
        return outcome(
            plan,
            "unknown",
            "native_drafting_unit_mismatch",
            json!({"actual":part["units"],"required":plan.unit}),
        );
    }
    if mapped_anchor || direct_layout {
        let display = &part["display_part"];
        if (!prepared && display["return_code"] != 0)
            || display["matches_expected"] != true
            || display["actual_part"] != part["part_tag"]
            || display["expected_part"] != part["part_tag"]
            || part["part_tag"]
                .as_u64()
                .is_none_or(|tag| tag == 0 || tag > u32::MAX as u64)
        {
            return outcome(
                plan,
                "unknown",
                "native_drafting_display_part_unavailable",
                json!({"display_part":display}),
            );
        }
    }
    let Some(sheets) = part["drawing_sheets"].as_array() else {
        return outcome(plan, "unknown", "native_drafting_sheets_missing", json!({}));
    };
    if part["drawing_sheet_count"].as_u64() != Some(sheets.len() as u64)
        || sheets.len() > 10_000
        || sheets.iter().any(|s| !metadata_valid(&s["metadata"]))
    {
        return outcome(
            plan,
            "unknown",
            "native_drafting_sheet_inventory_incomplete",
            json!({}),
        );
    }
    if prepared
        && sheets.iter().any(|s| {
            s["display_open_called"] != false
                || s.get("display_open_code") != Some(&Value::Null)
                || s["display_state"]["set_called"] != false
                || s["display_state"].get("set_code") != Some(&Value::Null)
                || s["views"].as_array().is_none_or(|views| {
                    views
                        .iter()
                        .any(|v| v.get("update_code") != Some(&Value::Null))
                })
        })
    {
        return outcome(
            plan,
            "unknown",
            "native_prepared_context_mutation_recorded",
            json!({}),
        );
    }
    let selected: Vec<_> = sheets
        .iter()
        .filter(|s| s["metadata"]["name_query"]["utf8"] == plan.sheet_name)
        .collect();
    if selected.len() != 1 {
        return outcome(
            plan,
            "violated",
            if selected.is_empty() {
                "native_drafting_named_sheet_missing"
            } else {
                "native_drafting_named_sheet_ambiguous"
            },
            json!({"matching_sheet_count":selected.len()}),
        );
    }
    let sheet = selected[0];
    if prepared {
        let valid = part["part_tag"]
            .as_u64()
            .zip(sheet["drawing_tag"].as_u64())
            .filter(|(p, s)| *p <= u32::MAX as u64 && *s <= u32::MAX as u64)
            .is_some_and(|(p, s)| {
                check_prepared_context(
                    p as u32,
                    s as u32,
                    &part["display_part"],
                    &sheet["display_state"],
                )
            });
        if !valid
            || part["context_mode"] != "observe_prepared"
            || part["prepared_drawing_tag"] != sheet["drawing_tag"]
            || part["view_refresh_requested"] != false
            || sheet["display_open_called"] != false
            || sheet.get("display_open_code") != Some(&Value::Null)
            || part["context_after"]["display_part"] != part["display_part"]
            || part["context_after"]["display_state"] != sheet["display_state"]
        {
            return outcome(
                plan,
                "unknown",
                "native_prepared_context_unproven",
                json!({"sheet":sheet}),
            );
        }
    }
    if !prepared && (mapped_anchor || direct_layout) && sheet["display_open_code"] != 0 {
        return outcome(
            plan,
            "unknown",
            "native_drafting_display_sheet_unavailable",
            json!({"display_open_code":sheet["display_open_code"]}),
        );
    }
    if active_display {
        let display = &sheet["display_state"];
        if (!prepared && display["set_code"] != 0)
            || display["query_code"] != 0
            || display["state_raw"] != 2
            || display["current_drawing_code"] != 0
            || display["current_drawing_tag"] != sheet["drawing_tag"]
            || display["work_view_code"] != 0
            || display["work_view_type"]["return_code"] != 0
            || display["work_view_type"]["type_raw"] != 2
            || display["work_view_type"]["kind"] != "drawing_sheet"
            || display["work_view_tag"]
                .as_u64()
                .is_none_or(|tag| tag == 0 || tag > u32::MAX as u64)
        {
            return outcome(
                plan,
                "unknown",
                "native_drafting_display_state_unavailable",
                json!({"display_state":display}),
            );
        }
    }
    let info = &sheet["sheet_info"];
    if info["wrapper_code"] != 0 || info["query_code"] != 0 || info["value_valid"] != true {
        return outcome(
            plan,
            "unknown",
            "native_drafting_sheet_info_unavailable",
            json!({"sheet_info":info}),
        );
    }
    let standard = if info["schema"] == "oev2.native-drafting-sheet-info/2"
        && info["size_state"] == "standard"
    {
        let decoded = info["standard_family"]
            .as_i64()
            .zip(info["standard_code"].as_i64())
            .and_then(|(family, code)| {
                let state = match family {
                    1 => 0,
                    2 => 1,
                    _ => -1,
                };
                if info["size_state_raw"].as_i64() != Some(state) {
                    return None;
                }
                standard_sheet_dimensions(
                    i32::try_from(family).ok()?,
                    i32::try_from(code).ok()?,
                    info["unit"].as_str()?,
                )
            });
        let Some(decoded) = decoded else {
            return outcome(
                plan,
                "unknown",
                "native_drafting_standard_size_unavailable",
                json!({"sheet_info":info}),
            );
        };
        if !info["width"].as_f64().is_some_and(|n| near(n, decoded[0]))
            || !info["height"].as_f64().is_some_and(|n| near(n, decoded[1]))
        {
            return outcome(
                plan,
                "unknown",
                "native_drafting_standard_size_inconsistent",
                json!({"sheet_info":info}),
            );
        }
        true
    } else {
        false
    };
    if info["size_state"] != "custom" && !standard {
        return outcome(
            plan,
            "unknown",
            "native_drafting_sheet_size_kind_unsupported",
            json!({"sheet_info":info}),
        );
    }
    let Some(width) = info["width"].as_f64().filter(|n| n.is_finite()) else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_sheet_size_missing",
            json!({}),
        );
    };
    let Some(height) = info["height"].as_f64().filter(|n| n.is_finite()) else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_sheet_size_missing",
            json!({}),
        );
    };
    let expected_scale = plan.scale_numerator / plan.scale_denominator;
    if info["unit"] != plan.unit
        || info["projection_angle"] != plan.projection_angle
        || !near(width, plan.width)
        || !near(height, plan.height)
        || !info["scale"]
            .as_f64()
            .is_some_and(|n| near(n, expected_scale))
    {
        return outcome(
            plan,
            "violated",
            "native_drafting_sheet_plan_mismatch",
            json!({"sheet_info":info}),
        );
    }
    let Some(views) = sheet["views"].as_array() else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_view_inventory_missing",
            json!({}),
        );
    };
    if sheet["view_catalog_complete"] != true || views.len() > 10_000 {
        return outcome(
            plan,
            "unknown",
            "native_drafting_view_inventory_incomplete",
            json!({}),
        );
    }
    if views.len() != 1 {
        return outcome(
            plan,
            "violated",
            "native_drafting_single_view_required",
            json!({"view_count":views.len()}),
        );
    }
    let view = &views[0];
    if !metadata_valid(&view["metadata"])
        || view["stale_query_code"] != 0
        || view["view_type"]["return_code"] != 0
        || view["scale"]["return_code"] != 0
        || view["reference_point"]["return_code"] != 0
        || view["borders"]["return_code"] != 0
        || view["owning_drawing"]["return_code"] != 0
        || view["model_part"]["return_code"] != 0
        || view["rotation"]["return_code"] != 0
        || view["visible_objects"]["return_code"] != 0
        || view["visible_objects"]["capture_complete"] != true
    {
        return outcome(
            plan,
            "unknown",
            "native_drafting_view_readback_incomplete",
            json!({"view":view}),
        );
    }
    if view["stale_after"] != false
        || view["view_type"]["kind"] != "drawing_member"
        || view["view_type"]["subtype"] != "imported"
        || view["owning_drawing"]["drawing_tag"] != sheet["drawing_tag"]
    {
        return outcome(
            plan,
            "violated",
            "native_drafting_view_state_mismatch",
            json!({"view":view}),
        );
    }
    if object_boundary {
        let boundary = &view["boundary"];
        if boundary["type_query_code"] != 0
            || boundary["objects_query_code"] != 0
            || boundary["capture_complete"] != true
        {
            return outcome(
                plan,
                "unknown",
                "native_drafting_object_boundary_unavailable",
                json!({"boundary":boundary}),
            );
        }
        let tags = |value: &Value| -> Option<Vec<u64>> {
            let a = value.as_array()?;
            if a.is_empty() || a.len() > 10_000 {
                return None;
            }
            let mut tags = a
                .iter()
                .map(|v| v.as_u64().filter(|n| *n > 0 && *n <= u32::MAX as u64))
                .collect::<Option<Vec<_>>>()?;
            tags.sort_unstable();
            if tags.windows(2).any(|p| p[0] == p[1]) {
                return None;
            }
            Some(tags)
        };
        let (Some(solids), Some(objects)) =
            (tags(&part["solid_tags"]), tags(&boundary["object_tags"]))
        else {
            return outcome(
                plan,
                "unknown",
                "native_drafting_object_boundary_inventory_invalid",
                json!({"boundary":boundary}),
            );
        };
        if part["solid_count"].as_u64() != Some(solids.len() as u64)
            || boundary["object_count"].as_u64() != Some(objects.len() as u64)
        {
            return outcome(
                plan,
                "unknown",
                "native_drafting_object_boundary_inventory_invalid",
                json!({"boundary":boundary}),
            );
        }
        if boundary["kind"] != "bound_by_objects" || boundary["type_raw"] != 4 || solids != objects
        {
            return outcome(
                plan,
                "violated",
                "native_drafting_object_boundary_mismatch",
                json!({"boundary":boundary,
                "required_complete_solid_tags":solids}),
            );
        }
    }
    if direct_layout {
        if let Some(failure) = check_layout_reference(plan, part, view) {
            return failure;
        }
    }
    if layout_anchor {
        let anchor = &view["layout_anchor"];
        let point = &anchor["point_data"];
        let valid_tag = |value: &Value| value.as_u64().filter(|n| *n > 0 && *n <= u32::MAX as u64);
        if anchor["query_code"] != 0
            || point["point_query_code"] != 0
            || point["smart_query_code"] != 0
            || point["owner_query_code"] != 0
            || point["capture_complete"] != true
            || point["is_smart"] != true
            || point["smart_raw"] != 1
            || point["matches_expected_owner"] != true
            || valid_tag(&anchor["point_tag"]).is_none()
            || valid_tag(&point["owning_part"]).is_none()
            || valid_tag(&part["part_tag"]).is_none()
        {
            return outcome(
                plan,
                "unknown",
                "native_drafting_layout_anchor_unavailable",
                json!({"anchor":anchor}),
            );
        }
        if anchor["point_tag"] != point["point_tag"] || point["owning_part"] != part["part_tag"] {
            return outcome(
                plan,
                "violated",
                "native_drafting_layout_anchor_identity_mismatch",
                json!({"anchor":anchor}),
            );
        }
        let bounds = serde_json::from_value::<Vec<[f64; 6]>>(part["bbox"].clone()).ok();
        let expected = bounds
            .filter(|bounds| part["solid_count"].as_u64() == Some(bounds.len() as u64))
            .and_then(|bounds| layout_anchor_from_bounds(&bounds).ok());
        let (Some(expected), Some(actual)) = (expected, vector(&point["coordinates"], 3)) else {
            return outcome(
                plan,
                "unknown",
                "native_drafting_layout_anchor_geometry_unavailable",
                json!({"anchor":anchor}),
            );
        };
        if actual.iter().zip(expected).any(|(a, b)| !near(*a, b)) {
            return outcome(
                plan,
                "violated",
                "native_drafting_layout_anchor_position_mismatch",
                json!({"anchor":anchor,
                "required_layout_midpoint":expected,"scope":"approximate geometry layout only"}),
            );
        }
        if mapped_anchor {
            let mapping = &anchor["drawing_mapping"];
            if mapping["return_code"] != 0 || mapping["capture_complete"] != true {
                return outcome(
                    plan,
                    "unknown",
                    "native_drafting_anchor_mapping_unavailable",
                    json!({"mapping":mapping}),
                );
            }
            let Some(mapped) = vector(&mapping["coordinates"], 2) else {
                return outcome(
                    plan,
                    "unknown",
                    "native_drafting_anchor_mapping_unavailable",
                    json!({"mapping":mapping}),
                );
            };
            if mapped
                .iter()
                .zip(&plan.view_center)
                .any(|(a, b)| !near(*a, *b))
            {
                return outcome(
                    plan,
                    "violated",
                    "native_drafting_anchor_mapping_mismatch",
                    json!({"mapping":mapping,
                    "required_sheet_reference_point":plan.view_center}),
                );
            }
        }
    }
    let (Some(part_path), Some(model_path)) =
        (part["path"].as_str(), view["model_part"]["path"].as_str())
    else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_model_lineage_missing",
            json!({}),
        );
    };
    if source_path(part_path) != source_path(model_path) {
        return outcome(
            plan,
            "violated",
            "native_drafting_model_part_mismatch",
            json!({"part":part_path,"view_model":model_path}),
        );
    }
    let Some(model_views) = part["modeling_views"].as_array() else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_model_views_missing",
            json!({}),
        );
    };
    if part["modeling_view_catalog_complete"] != true
        || model_views.len() > 10_000
        || model_views.iter().any(|v| !metadata_valid(&v["metadata"]))
    {
        return outcome(
            plan,
            "unknown",
            "native_drafting_model_view_inventory_incomplete",
            json!({}),
        );
    }
    let models: Vec<_> = model_views
        .iter()
        .filter(|v| v["metadata"]["name_query"]["utf8"] == plan.model_view_name)
        .collect();
    if models.len() != 1 {
        return outcome(
            plan,
            "unknown",
            "native_drafting_named_model_view_not_unique",
            json!({"matches":models.len()}),
        );
    }
    let model = models[0];
    if centered_model {
        let center = &model["view_center"];
        let bounds = serde_json::from_value::<Vec<[f64; 6]>>(part["bbox"].clone()).ok();
        let expected = bounds
            .filter(|b| part["solid_count"].as_u64() == Some(b.len() as u64))
            .and_then(|b| layout_anchor_from_bounds(&b).ok());
        let actual = vector(&center["coordinates"], 3);
        let (Some(expected), Some(actual)) = (expected, actual) else {
            return outcome(
                plan,
                "unknown",
                "native_drafting_model_center_unavailable",
                json!({"model_view_center":center}),
            );
        };
        if center["query_code"] != 0 {
            return outcome(
                plan,
                "unknown",
                "native_drafting_model_center_unavailable",
                json!({"model_view_center":center}),
            );
        }
        if actual.iter().zip(expected).any(|(a, b)| !near(*a, b)) {
            return outcome(
                plan,
                "violated",
                "native_drafting_model_center_mismatch",
                json!({"model_view_center":center,
                "required_layout_midpoint":expected,"scope":"view-plane layout; entity ABS geometry unchanged"}),
            );
        }
    }
    if model["view_type"]["return_code"] != 0
        || model["view_type"]["kind"] != "model"
        || model["rotation"]["return_code"] != 0
    {
        return outcome(
            plan,
            "unknown",
            "native_drafting_model_orientation_unavailable",
            json!({"model_view":model}),
        );
    }
    let (Some(source_rotation), Some(view_rotation)) = (
        vector(&model["rotation"]["matrix"], 9),
        vector(&view["rotation"]["matrix"], 9),
    ) else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_rotation_missing",
            json!({}),
        );
    };
    if !rotation_valid(&source_rotation) || !rotation_valid(&view_rotation) {
        return outcome(
            plan,
            "unknown",
            "native_drafting_rotation_invalid",
            json!({"source_rotation":source_rotation,"drawing_rotation":view_rotation}),
        );
    }
    if source_rotation
        .iter()
        .zip(&view_rotation)
        .any(|(a, b)| !near(*b, *a))
    {
        return outcome(
            plan,
            "violated",
            "native_drafting_named_view_orientation_mismatch",
            json!({"source_rotation":source_rotation,"drawing_rotation":view_rotation}),
        );
    }
    let (Some(reference), Some(borders)) = (
        vector(&view["reference_point"]["point"], 2),
        vector(&view["borders"]["values"], 4),
    ) else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_view_geometry_missing",
            json!({}),
        );
    };
    if !view["scale"]["value"]
        .as_f64()
        .is_some_and(|n| near(n, expected_scale))
        || reference
            .iter()
            .zip(&plan.view_center)
            .any(|(a, b)| !near(*a, *b))
    {
        return outcome(
            plan,
            "violated",
            "native_drafting_view_placement_mismatch",
            json!({"view":view}),
        );
    }
    if borders[2] <= borders[0]
        || borders[3] <= borders[1]
        || borders[0] < 0.0
        || borders[1] < 0.0
        || borders[2] > width
        || borders[3] > height
    {
        return outcome(
            plan,
            "violated",
            "native_drafting_view_blank_or_outside_sheet",
            json!({"borders":borders,"width":width,"height":height}),
        );
    }
    let visible = &view["visible_objects"];
    let (Some(n_visible), Some(n_clipped)) = (
        visible["visible_count"].as_u64(),
        visible["clipped_count"].as_u64(),
    ) else {
        return outcome(
            plan,
            "unknown",
            "native_drafting_visible_inventory_missing",
            json!({}),
        );
    };
    if n_visible > 10_000 || n_clipped > 10_000 {
        return outcome(
            plan,
            "unknown",
            "native_drafting_visible_inventory_inconsistent",
            json!({"visible":visible}),
        );
    }
    // Historical count-only observations keep their original limited scope.
    // When the native DTO supplies either tag list, both inventories must be
    // present and agree with their counts; a complete flag cannot override them.
    let tags_present =
        visible.get("visible_tags").is_some() || visible.get("clipped_tags").is_some();
    if direct_layout && !tags_present {
        return outcome(
            plan,
            "unknown",
            "native_drafting_visible_inventory_inconsistent",
            json!({"visible":visible,
            "scope":"direct layout /6 requires explicit native tag inventories"}),
        );
    }
    if tags_present {
        let inventory_valid = |value: &Value, count: u64| {
            value.as_array().is_some_and(|tags| {
                tags.len() as u64 == count
                    && tags
                        .iter()
                        .all(|tag| tag.as_u64().is_some_and(|n| n > 0 && n <= u32::MAX as u64))
            })
        };
        if !inventory_valid(&visible["visible_tags"], n_visible)
            || !inventory_valid(&visible["clipped_tags"], n_clipped)
        {
            return outcome(
                plan,
                "unknown",
                "native_drafting_visible_inventory_inconsistent",
                json!({"visible":visible}),
            );
        }
    }
    if n_visible == 0 || n_clipped != 0 {
        return outcome(
            plan,
            "violated",
            "native_drafting_empty_or_clipped_view",
            json!({"visible":visible}),
        );
    }
    outcome(
        plan,
        "pass",
        "native_drafting_geometric_reference_matches",
        json!({"sheet":sheet,"named_model_view":model,
        "layout_reference_scope":if direct_layout {"current complete-solid bbox ABS midpoint mapped to plan center; optional anchor not used as evidence"}
            else {"original observation protocol retained"},
        "visibility_inventory_scope":if tags_present {"native tag/count consistency; not per-entity occlusion or visual readability"}
            else {"historical count-only observation; individual native tags not verified"},
        "boundary_scope":if object_boundary {"complete same-part solids; model annotations retained outside this geometry-only obligation"}
            else {"legacy view envelope; no separately bound object inventory"}}),
    )
}

// /6 is a separately declared layout model. Earlier /3-/5 smart-point contracts
// remain intact; this does not turn their lost association into a pass.
fn check_layout_reference(plan: &NativeDraftingPlan, part: &Value, view: &Value) -> Option<Value> {
    let reference = match serde_json::from_value::<NativeLayoutReference>(
        view["layout_reference"].clone(),
    ) {
        Ok(reference) => reference,
        Err(error) => {
            return Some(outcome(
                plan,
                "unknown",
                "native_drafting_layout_reference_unavailable",
                json!({
            "layout_reference":view["layout_reference"],"contract_error":error.to_string(),"contract_stage":"deserialize"}),
            ));
        }
    };
    if let Err(errors) = reference.validate() {
        return Some(outcome(
            plan,
            "unknown",
            "native_drafting_layout_reference_unavailable",
            json!({
            "layout_reference":view["layout_reference"],"contract_errors":errors,"contract_stage":"validate"}),
        ));
    }
    let valid_tag = |tag: u64| tag > 0 && tag <= u32::MAX as u64;
    if !valid_tag(reference.model_part_tag)
        || !valid_tag(reference.drawing_view_tag)
        || !part["part_tag"].as_u64().is_some_and(valid_tag)
        || !view["view_tag"].as_u64().is_some_and(valid_tag)
    {
        return Some(outcome(
            plan,
            "unknown",
            "native_drafting_layout_reference_identity_unavailable",
            json!({"layout_reference":reference}),
        ));
    }
    if part["part_tag"].as_u64() != Some(reference.model_part_tag)
        || view["view_tag"].as_u64() != Some(reference.drawing_view_tag)
    {
        return Some(outcome(
            plan,
            "violated",
            "native_drafting_layout_reference_identity_mismatch",
            json!({"layout_reference":reference}),
        ));
    }
    if reference.unit != plan.unit || part["units"] != reference.unit {
        return Some(outcome(
            plan,
            "unknown",
            "native_drafting_layout_reference_unit_mismatch",
            json!({"layout_reference":reference}),
        ));
    }
    let bounds = serde_json::from_value::<Vec<[f64; 6]>>(part["bbox"].clone()).ok();
    let expected = bounds
        .filter(|b| part["solid_count"].as_u64() == Some(b.len() as u64))
        .and_then(|b| layout_anchor_from_bounds(&b).ok());
    let Some(expected) = expected else {
        return Some(outcome(
            plan,
            "unknown",
            "native_drafting_layout_reference_geometry_unavailable",
            json!({}),
        ));
    };
    let mapping = &reference.drawing_mapping;
    if reference.model_coordinates.len() != 3
        || mapping.model_point.len() != 3
        || mapping.coordinates.len() != 2
        || !reference
            .model_coordinates
            .iter()
            .chain(&mapping.model_point)
            .chain(&mapping.coordinates)
            .all(|n| n.is_finite())
        || mapping.return_code != 0
        || !mapping.capture_complete
    {
        return Some(outcome(
            plan,
            "unknown",
            "native_drafting_layout_reference_mapping_unavailable",
            json!({"layout_reference":reference}),
        ));
    }
    if reference
        .model_coordinates
        .iter()
        .zip(expected)
        .any(|(a, b)| !near(*a, b))
    {
        return Some(outcome(
            plan,
            "violated",
            "native_drafting_layout_reference_position_mismatch",
            json!({"layout_reference":reference,"required_layout_midpoint":expected}),
        ));
    }
    if mapping
        .model_point
        .iter()
        .zip(&reference.model_coordinates)
        .any(|(a, b)| !near(*a, *b))
    {
        return Some(outcome(
            plan,
            "violated",
            "native_drafting_layout_reference_mapping_input_mismatch",
            json!({"layout_reference":reference}),
        ));
    }
    if mapping
        .coordinates
        .iter()
        .zip(&plan.view_center)
        .any(|(a, b)| !near(*a, *b))
    {
        return Some(outcome(
            plan,
            "violated",
            "native_drafting_layout_reference_mapping_mismatch",
            json!({"layout_reference":reference,"required_sheet_reference_point":plan.view_center}),
        ));
    }
    None
}
