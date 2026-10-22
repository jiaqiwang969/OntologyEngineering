//! A single, source-bound circular edge; nominal diameter is not a fit or tolerance.
use super::{NativeApi, Workspace, drafting, input_relative, model_edges, sheet_dimensions};
use anyhow::{Context, Result, ensure};
use oe_contracts::SheetDiameterIntent;
use serde_json::{Value, json};

pub const INTENT_SCHEMA: &str = "oev2.sheet-diameter-intent/1";
pub const PARAMS_SCHEMA: &str = "oev2.nx-sheet-diameter-params/1";
pub const INTENT_SCHEMA2: &str = "oev2.sheet-diameter-intent/2";
pub const PARAMS_SCHEMA2: &str = "oev2.nx-sheet-diameter-params/2";
pub fn params_schema(i: &SheetDiameterIntent) -> &'static str {
    if i.schema == INTENT_SCHEMA2 {
        PARAMS_SCHEMA2
    } else {
        PARAMS_SCHEMA
    }
}

pub fn validate(i: &SheetDiameterIntent) -> Result<()> {
    i.validate()
        .map_err(|_| anyhow::anyhow!("diameter_generated_invalid"))?;
    drafting::validate(&i.sheet)?;
    model_edges::validate_v2(&i.edge)?;
    ensure!(
        matches!(i.schema.as_str(), INTENT_SCHEMA | INTENT_SCHEMA2)
            && i.edge.full_circle
            && i.edge.local_frame == "part_absolute"
            && i.edge.unit == i.sheet.unit,
        "diameter_scope_invalid"
    );
    ensure!(
        i.diameter.is_finite()
            && i.diameter > 0.0
            && i.numeric_tolerance.is_finite()
            && i.numeric_tolerance >= 0.0
            && (i.diameter - 2.0 * i.edge.radius).abs() <= i.numeric_tolerance,
        "diameter_independent_value_mismatch"
    );
    ensure!(
        !i.source_refs.is_empty()
            && i.source_refs.iter().all(|s| !s.trim().is_empty())
            && i.source_refs
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == i.source_refs.len(),
        "diameter_sources_invalid"
    );
    let p = &i.text_origin_drawing;
    ensure!(
        p.len() == 2
            && p.iter().all(|v| v.is_finite())
            && p[0] > 0.0
            && p[0] < i.sheet.width
            && p[1] > 0.0
            && p[1] < i.sheet.height,
        "diameter_text_origin_outside_page"
    );
    Ok(())
}

fn tag(v: &Value) -> Result<u32> {
    v.as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .map(|n| n as u32)
        .context("diameter_tag_invalid")
}

pub fn prepare(i: &SheetDiameterIntent, source: &[u8], part: &Value) -> Result<Value> {
    validate(i)?;
    ensure!(
        oe_nx_protocol::BlobIdentity::of(source).sha256.as_str() == i.sheet.object.source_sha256,
        "diameter_source_mismatch"
    );
    ensure!(
        part["drafting_readback_schema"] == "oev2.native-drafting-observation/6",
        "diameter_current_observation_required"
    );
    let sheet = drafting::check(&i.sheet, part);
    ensure!(sheet["conclusion"] == "pass", "diameter_sheet_unproven");
    let selected = &sheet["details"]["sheet"];
    let view = &selected["views"][0];
    let rotation: Vec<f64> = serde_json::from_value(view["rotation"]["matrix"].clone())?;
    ensure!(rotation.len() == 9, "diameter_view_rotation_missing");
    // The drafting owner verifies the matrix. The circle owner verifies its normal.
    let n = &i.edge.normal;
    let norm = n[0].hypot(n[1]).hypot(n[2]);
    let dot = (0..3)
        .map(|j| rotation[6 + j] * n[j] / norm)
        .sum::<f64>()
        .abs();
    ensure!(
        dot.clamp(-1.0, 1.0).acos() <= i.edge.angular_tolerance_radians,
        "diameter_circle_foreshortened"
    );
    let edge = model_edges::select_v2(&i.edge, part)?;
    ensure!(
        edge["conclusion"] == "pass"
            && edge["candidate_edge_tags"]
                .as_array()
                .is_some_and(|a| a.len() == 1),
        "diameter_unique_circle_unproven"
    );
    Ok(
        json!({"schema":if i.schema==INTENT_SCHEMA2 {"oev2.sheet-diameter-preflight/2"} else {"oev2.sheet-diameter-preflight/1"},"conclusion":"pass",
        "sheet_tag":tag(&selected["drawing_tag"])?,"view_tag":tag(&view["view_tag"])?,
        "edge_tag":tag(&edge["candidate_edge_tags"][0])?,"circle":edge,"sheet":sheet,
        "manufacturing_drawing":"not_established"}),
    )
}

pub fn check_created(
    i: &SheetDiameterIntent,
    source: &[u8],
    part: &Value,
    id: u32,
) -> Result<Value> {
    let p = prepare(i, source, part)?;
    let a = &part["annotations"];
    ensure!(
        a["schema"] == "oev2.nx-annotation-observation/2"
            && a["units_query"]["return_code"] == 0
            && a["units_query"]["unit"] == i.sheet.unit,
        "diameter_units_unknown"
    );
    let dims = a["dimensions"]
        .as_array()
        .context("diameter_inventory_missing")?;
    let mut ids = std::collections::BTreeSet::new();
    ensure!(
        dims.len() <= 4096
            && dims
                .iter()
                .all(|d| tag(&d["dimension_tag"]).is_ok_and(|n| ids.insert(n))),
        "diameter_inventory_invalid"
    );
    let d = dims
        .iter()
        .find(|d| d["dimension_tag"] == id)
        .context("diameter_created_missing")?;
    ensure!(
        id != 0 && d["retained_query_code"] == 0 && d["retained"] == false,
        "diameter_retained_or_unknown"
    );
    let owned = |m: &Value| {
        m["type_query"]["return_code"] == 0
            && m["owning_part_query"]["value_valid"] == true
            && m["owning_part_query"]["matches_expected_owner"] == true
    };
    let v = &d["computed_size"];
    ensure!(
        v["wrapper_code"] == 0
            && v["value_valid"] == true
            && v["basis"] == "NXOpen::Annotations::Dimension::ComputedSize"
            && v["unit"] == i.sheet.unit
            && owned(&v["metadata"])
            && v["metadata"]["type_query"]["type_raw"] == 26
            && v["metadata"]["type_query"]["subtype_raw"] == 10,
        "diameter_value_unavailable"
    );
    ensure!(
        v["value"]
            .as_f64()
            .is_some_and(|x| x.is_finite() && (x - i.diameter).abs() <= i.numeric_tolerance),
        "diameter_value_mismatch"
    );
    let assoc = &d["associativity_parents"];
    ensure!(
        assoc["wrapper_code"] == 0
            && assoc["uf_return_code"] == 0
            && assoc["capture_complete"] == true,
        "diameter_association_unknown"
    );
    let entries = assoc["entries"]
        .as_array()
        .context("diameter_association_missing")?;
    ensure!(entries.len() == 1, "diameter_single_parent_required");
    let e = &entries[0];
    let parent = &e["object_1"];
    let view = &e["view"];
    ensure!(
        parent["present"] == true
            && parent["tag"] == p["edge_tag"]
            && owned(&parent["metadata"])
            && parent["metadata"]["type_query"]["type_raw"] == 70
            && parent["metadata"]["type_query"]["subtype_raw"] == 3
            && e["object_2"]["present"] == false
            && e["object_2"]["tag"] == 0
            && e["point_type_raw"] == if i.schema == INTENT_SCHEMA2 { 0 } else { 2 }
            && e["line_type_raw"] == 0
            && e["unit"] == i.sheet.unit,
        "diameter_circle_association_mismatch"
    );
    ensure!(
        view["present"] == true && owned(&view["metadata"]) && view["tag"] == p["view_tag"],
        "diameter_member_view_mismatch"
    );
    if i.schema == INTENT_SCHEMA2 {
        // NO_POINT_TYPE is a whole circle object association, not a missing parent.
        // Preserve the raw type; independently check its reported ABS position.
        ensure!(
            e["association_position_valid"] == true
                && e["coordinate_basis"] == "UF_DRF assoc_point absolute coordinates",
            "diameter_whole_circle_position_unknown"
        );
        let at = model_edges::vector(&e["association_position"])
            .context("diameter_whole_circle_position_unknown")?;
        let center = &i.edge.center;
        ensure!(
            (at[0] - center[0])
                .hypot(at[1] - center[1])
                .hypot(at[2] - center[2])
                <= i.edge.center_tolerance,
            "diameter_whole_circle_center_mismatch"
        );
    }
    Ok(
        json!({"schema":if i.schema==INTENT_SCHEMA2 {"oev2.sheet-diameter-check/2"} else {"oev2.sheet-diameter-check/1"},"conclusion":"pass","preflight":p,
        "dimension_tag":id,"computed_size":v,"associations":entries,
        "manufacturing_drawing":"not_established","visual_readability":"not_assessed","engineering_adoption":"not_established"}),
    )
}

pub fn check_addition(before: &Value, after: &Value, id: u32) -> Result<()> {
    sheet_dimensions::check_addition(before, after, id)?;
    let old = before["annotations"]["dimensions"]
        .as_array()
        .context("diameter_prior_inventory_missing")?;
    let new = after["annotations"]["dimensions"]
        .as_array()
        .context("diameter_inventory_missing")?;
    ensure!(
        old.iter().all(|d| new.iter().any(|n| n == d)),
        "diameter_prior_annotation_changed"
    );
    Ok(())
}

pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    name: &str,
    i: &SheetDiameterIntent,
) -> Result<Value> {
    let source = w.input(&input_relative(name)?)?;
    let raw = std::fs::read(&source)?;
    validate(i)?;
    ensure!(
        oe_nx_protocol::BlobIdentity::of(&raw).sha256.as_str() == i.sheet.object.source_sha256,
        "diameter_source_mismatch"
    );
    let target = w.output(name)?;
    std::fs::copy(&source, &target)?;
    ensure!(
        w.require_regular_output(name)? == raw,
        "diameter_copy_mismatch"
    );
    let part = api.open_part(&target)?;
    api.set_work_part(part)?;
    let before = api.detailed_info(part, false)?;
    w.record("diameter_before", before.clone())?;
    let p = prepare(i, &raw, &before)?;
    w.record("diameter_prepared", p.clone())?;
    let created = api.create_sheet_diameter(
        tag(&p["sheet_tag"])?,
        tag(&p["view_tag"])?,
        tag(&p["edge_tag"])?,
        [i.text_origin_drawing[0], i.text_origin_drawing[1]],
    )?;
    w.record("diameter_creation", created.clone())?;
    ensure!(
        created["complete"] == true,
        "diameter_native_creation_incomplete"
    );
    let id = tag(&created["dimension_tag"])?;
    let after = api.detailed_info(part, false)?;
    w.record("diameter_after", after.clone())?;
    check_addition(&before, &after, id)?;
    w.record("diameter_check", check_created(i, &raw, &after, id)?)?;
    api.save_work_part()?;
    w.require_regular_output(name)?;
    let saved = api.detailed_info(part, false)?;
    w.record("diameter_saved", saved.clone())?;
    check_addition(&before, &saved, id)?;
    let check = check_created(i, &raw, &saved, id)?;
    ensure!(std::fs::read(&source)? == raw, "diameter_source_changed");
    Ok(
        json!({"action":"create_sheet_diameter_dimension","intent":i,"creation":created,
        "parts":[{"input":name,"before":before,"after":after,"after_save":saved}],"check":check,
        "independent_reopen":"required_separate_process","visual_readability":"not_assessed",
        "manufacturing_drawing":"not_established","engineering_adoption":"not_established"}),
    )
}
