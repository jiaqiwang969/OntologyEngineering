//! Source-bound retirement of explicitly selected retained dimensions on copies.
//! Exact signatures identify observations within frozen bytes, not design topology.
use super::{NativeApi, Workspace, drafting, input_relative, sheet_dimensions};
use anyhow::{Context, Result, ensure};
use oe_contracts::DrawingRetirementIntent;
use oe_core::{json::canonical_json, sha256};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const PARAMS_SCHEMA: &str = "oev2.nx-drawing-retirement-params/1";
pub const INTENT_SCHEMA: &str = "oev2.drawing-retirement-intent/1";
fn array(v: &Value, n: usize) -> bool {
    v.as_array()
        .is_some_and(|a| a.len() == n && a.iter().all(|x| x.as_f64().is_some_and(f64::is_finite)))
}
fn owned(v: &Value) -> Result<Value> {
    ensure!(
        v["type_query"]["return_code"] == 0
            && v["type_query"]["type_raw"].as_i64().is_some()
            && v["type_query"]["subtype_raw"].as_i64().is_some()
            && v["owning_part_query"]["value_valid"] == true
            && v["owning_part_query"]["matches_expected_owner"] == true,
        "retirement_owner_or_type_unknown"
    );
    Ok(
        json!({"type":v["type_query"]["type_raw"],"subtype":v["type_query"]["subtype_raw"],"same_owner":true}),
    )
}
fn parent(v: &Value) -> Result<Value> {
    if v["present"] == false {
        ensure!(v["tag"] == 0, "retirement_absent_parent_has_tag");
        return Ok(json!({"present":false}));
    }
    ensure!(
        v["present"] == true
            && v["tag"]
                .as_u64()
                .is_some_and(|n| n > 0 && n <= u32::MAX as u64),
        "retirement_parent_unknown"
    );
    let metadata = owned(&v["metadata"])?;
    let g = &v["geometry"];
    ensure!(
        g["called"].as_bool().is_some(),
        "retirement_parent_geometry_unknown"
    );
    if g["called"] == true {
        ensure!(
            g["return_code"] == 0 && g["value_valid"] == true && array(&g["absolute_bounds"], 6),
            "retirement_parent_geometry_unknown"
        );
    }
    Ok(json!({"present":true,"metadata":metadata,"geometry":g}))
}
/// Fail on incomplete observations; omit only transient tags and names explicitly.
pub fn signature(d: &Value, unit: &str) -> Result<Value> {
    signature_for_kind(d, unit, 3, "oev2.drawing-dimension-signature/1")
}
pub(super) fn signature_for_kind(
    d: &Value,
    unit: &str,
    subtype: i64,
    schema: &str,
) -> Result<Value> {
    ensure!(
        d["retained_query_code"] == 0 && d["retained"].as_bool().is_some(),
        "retirement_retained_unknown"
    );
    let size = &d["computed_size"];
    let metadata = owned(&size["metadata"])?;
    ensure!(
        metadata["type"] == 26
            && metadata["subtype"] == subtype
            && size["wrapper_code"] == 0
            && size["value_valid"] == true
            && size["unit"] == unit
            && size["basis"] == "NXOpen::Annotations::Dimension::ComputedSize"
            && size["value"].as_f64().is_some_and(f64::is_finite),
        "retirement_dimension_scope_invalid"
    );
    ensure!(
        d["text_box_query_code"] == 0 && d["text_box_error"].is_null(),
        "retirement_text_boxes_unknown"
    );
    let boxes = d["text_boxes"]
        .as_array()
        .context("retirement_text_boxes_unknown")?;
    ensure!(
        !boxes.is_empty()
            && boxes.len() <= 64
            && boxes.iter().all(|b| array(&b["origin"], 3)
                && ["length", "height", "angle_radians", "distance"]
                    .iter()
                    .all(|k| b[*k].as_f64().is_some_and(f64::is_finite))
                && b["coordinate_basis"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())),
        "retirement_text_boxes_unknown"
    );
    let a = &d["associativity_parents"];
    ensure!(
        a["capture_complete"] == true && a["wrapper_code"] == 0 && a["uf_return_code"] == 0,
        "retirement_associations_unknown"
    );
    let entries = a["entries"]
        .as_array()
        .context("retirement_associations_unknown")?;
    ensure!(
        !entries.is_empty() && entries.len() <= 256,
        "retirement_association_budget"
    );
    let mut associations = Vec::new();
    for e in entries {
        ensure!(
            e["association_position_valid"] == true
                && array(&e["association_position"], 3)
                && e["unit"] == unit
                && e["point_type_raw"].as_i64().is_some()
                && e["line_type_raw"].as_i64().is_some()
                && e["coordinate_basis"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()),
            "retirement_association_position_unknown"
        );
        associations.push(json!({"object_1":parent(&e["object_1"] )?,"object_2":parent(&e["object_2"] )?,"view":parent(&e["view"] )?,
            "position":e["association_position"],"point_type":e["point_type_raw"],"line_type":e["line_type_raw"],"unit":unit,"coordinate_basis":e["coordinate_basis"]}));
    }
    associations.sort_by_key(|v| canonical_json(v).expect("JSON values"));
    let projection = json!({"schema":schema,"retained":d["retained"],"metadata":metadata,
        "value":size["value"],"unit":unit,"text_boxes":boxes,"associations":associations});
    Ok(json!({"sha256":sha256(canonical_json(&projection)?),"projection":projection}))
}
pub fn inventory(part: &Value) -> Result<BTreeMap<String, Value>> {
    inventory_using(part, signature)
}
pub(super) fn inventory_using(
    part: &Value,
    signature: fn(&Value, &str) -> Result<Value>,
) -> Result<BTreeMap<String, Value>> {
    let a = &part["annotations"];
    ensure!(
        a["schema"] == "oev2.nx-annotation-observation/2" && a["units_query"]["return_code"] == 0,
        "retirement_inventory_unknown"
    );
    let unit = a["units_query"]["unit"]
        .as_str()
        .context("retirement_units_unknown")?;
    ensure!(["mm", "inch"].contains(&unit), "retirement_units_unknown");
    let dims = a["dimensions"]
        .as_array()
        .context("retirement_inventory_unknown")?;
    ensure!(
        !dims.is_empty() && dims.len() <= 256,
        "retirement_inventory_budget"
    );
    let mut rows = BTreeMap::new();
    let mut tags = BTreeSet::new();
    for d in dims {
        let tag = d["dimension_tag"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .context("retirement_tag_invalid")?;
        ensure!(tags.insert(tag), "retirement_duplicate_tag");
        let s = signature(d, unit)?;
        let key = s["sha256"].as_str().unwrap().to_owned();
        ensure!(
            rows.insert(
                key,
                json!({"tag":tag,"retained":d["retained"],"signature":s})
            )
            .is_none(),
            "retirement_ambiguous_signature"
        );
    }
    Ok(rows)
}
fn hashes(values: &[String]) -> Result<BTreeSet<String>> {
    ensure!(
        !values.is_empty() && values.len() <= 256,
        "retirement_signature_budget"
    );
    let set: BTreeSet<_> = values.iter().cloned().collect();
    ensure!(
        set.len() == values.len()
            && values.iter().all(|s| s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
        "retirement_signatures_invalid"
    );
    Ok(set)
}
pub fn validate(i: &DrawingRetirementIntent) -> Result<()> {
    i.validate()
        .map_err(|_| anyhow::anyhow!("retirement_contract_invalid"))?;
    ensure!(
        i.schema == INTENT_SCHEMA && i.object == i.sheet.object,
        "retirement_object_mismatch"
    );
    drafting::validate(&i.sheet)?;
    let all = hashes(&i.inventory_signatures)?;
    let selected = hashes(&i.retire_signatures)?;
    ensure!(
        selected.is_subset(&all)
            && selected.len() < all.len()
            && i.preserve.len() == all.len() - selected.len(),
        "retirement_inventory_partition_invalid"
    );
    ensure!(
        !i.source_refs.is_empty()
            && i.source_refs.iter().all(|s| !s.trim().is_empty())
            && i.source_refs.iter().collect::<BTreeSet<_>>().len() == i.source_refs.len(),
        "retirement_sources_invalid"
    );
    let mut ids = BTreeSet::new();
    for p in &i.preserve {
        ensure!(
            p.text_origin_drawing.len() == 2
                && p.reference.object == i.object
                && ids.insert(&p.reference.intent_id),
            "retirement_preservation_contract_invalid"
        );
        sheet_dimensions::validate(
            &i.sheet,
            &p.reference,
            [p.text_origin_drawing[0], p.text_origin_drawing[1]],
        )?;
    }
    Ok(())
}
fn preserved(
    i: &DrawingRetirementIntent,
    source: &[u8],
    part: &Value,
    rows: &BTreeMap<String, Value>,
) -> Result<Value> {
    let selected = hashes(&i.retire_signatures)?;
    let mut tags = BTreeSet::new();
    let mut reports = Vec::new();
    for guard in &i.preserve {
        let mut matches = Vec::new();
        for (sig, row) in rows {
            if selected.contains(sig) {
                continue;
            }
            let tag = row["tag"].as_u64().unwrap() as u32;
            if let Ok(check) = sheet_dimensions::check_created(
                &i.sheet,
                &guard.reference,
                [guard.text_origin_drawing[0], guard.text_origin_drawing[1]],
                source,
                part,
                tag,
            ) {
                matches.push((tag, check));
            }
        }
        ensure!(
            matches.len() == 1,
            "retirement_preserved_dimension_unproven"
        );
        let (tag, check) = matches.remove(0);
        ensure!(tags.insert(tag), "retirement_preserved_dimension_reused");
        reports.push(check);
    }
    ensure!(
        tags.len() == rows.keys().filter(|k| !selected.contains(*k)).count(),
        "retirement_preserved_inventory_incomplete"
    );
    Ok(json!(reports))
}
pub fn prepare(i: &DrawingRetirementIntent, source: &[u8], part: &Value) -> Result<Value> {
    validate(i)?;
    ensure!(
        sha256(source) == i.object.source_sha256,
        "retirement_source_mismatch"
    );
    let rows = inventory(part)?;
    ensure!(
        rows.keys().cloned().collect::<BTreeSet<_>>() == hashes(&i.inventory_signatures)?,
        "retirement_inventory_mismatch"
    );
    let mut targets = Vec::new();
    for sig in &i.retire_signatures {
        ensure!(
            rows[sig]["retained"] == true,
            "retirement_target_is_not_retained"
        );
        targets.push(json!({"signature":sig,"tag":rows[sig]["tag"]}));
    }
    let guards = preserved(i, source, part, &rows)?;
    Ok(
        json!({"conclusion":"pass","targets":targets,"preserved":guards,"tags_scope":"this observation process only","engineering_adoption":"not_established"}),
    )
}
pub fn check_progress(
    i: &DrawingRetirementIntent,
    source: &[u8],
    before: &Value,
    after: &Value,
    removed: &BTreeSet<String>,
) -> Result<Value> {
    prepare(i, source, before)?;
    ensure!(
        removed.is_subset(&hashes(&i.retire_signatures)?),
        "retirement_removed_set_invalid"
    );
    sheet_dimensions::check_geometry_preserved(before, after)?;
    let rows = inventory(after)?;
    let expected: BTreeSet<_> = hashes(&i.inventory_signatures)?
        .difference(removed)
        .cloned()
        .collect();
    ensure!(
        rows.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "retirement_remaining_inventory_mismatch"
    );
    Ok(
        json!({"conclusion":"pass","removed":removed,"preserved":preserved(i,source,after,&rows)?,"visual_readability":"not_assessed","manufacturing_drawing":"not_established"}),
    )
}
pub fn check_reopened(
    i: &DrawingRetirementIntent,
    saved: &[u8],
    after_save: &Value,
    reopened: &Value,
) -> Result<Value> {
    validate(i)?;
    ensure!(
        reopened["view_refresh_requested"] == false,
        "retirement_reopen_refresh_forbidden"
    );
    let expected: BTreeSet<_> = hashes(&i.inventory_signatures)?
        .difference(&hashes(&i.retire_signatures)?)
        .cloned()
        .collect();
    let old = inventory(after_save)?;
    let rows = inventory(reopened)?;
    ensure!(
        old.keys().cloned().collect::<BTreeSet<_>>() == expected
            && rows.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "retirement_reopened_inventory_mismatch"
    );
    let mut current = i.clone();
    current.object.source_sha256 = sha256(saved);
    current.sheet.object = current.object.clone();
    for p in &mut current.preserve {
        p.reference.object = current.object.clone();
    }
    Ok(
        json!({"conclusion":"pass","preserved":preserved(&current,saved,reopened,&rows)?,"saved_source_sha256":sha256(saved),"current_Project_revision":"not_modified","engineering_adoption":"not_established","geometry_equivalence":"only_preserved_reference_geometry_checked","visual_readability":"not_assessed"}),
    )
}
pub fn execute(
    api: &mut dyn NativeApi,
    ws: &mut dyn Workspace,
    name: &str,
    i: &DrawingRetirementIntent,
) -> Result<Value> {
    let input = ws.input(&input_relative(name)?)?;
    let source = std::fs::read(&input)?;
    validate(i)?;
    ensure!(
        sha256(&source) == i.object.source_sha256,
        "retirement_source_mismatch"
    );
    let output = ws.output(name)?;
    std::fs::copy(&input, &output)?;
    ensure!(
        ws.require_regular_output(name)? == source,
        "retirement_copy_mismatch"
    );
    let part = api.open_part(&output)?;
    api.set_work_part(part)?;
    let before = api.detailed_info(part, false)?;
    ws.record("drawing_retirement_before", before.clone())?;
    let plan = prepare(i, &source, &before)?;
    ws.record("drawing_retirement_prepared", plan.clone())?;
    let mut removed = BTreeSet::new();
    let mut calls = Vec::new();
    let mut after = before.clone();
    // All selection/cycling has completed before the first deletion.
    for target in plan["targets"].as_array().unwrap() {
        let tag = target["tag"].as_u64().unwrap() as u32;
        ws.record("drawing_retirement_attempt", target.clone())?;
        let call = api.retire_retained_dimension(part, tag)?;
        ws.record("drawing_retirement_returned", call.clone())?;
        ensure!(
            call["api"] == "UF_OBJ_delete_object"
                && call["return_code"] == 0
                && call["dimension_tag"] == tag
                && call["after_lifecycle"]["status_observed"] == true
                && call["after_lifecycle"]["status"] == "deleted",
            "retirement_native_delete_failed"
        );
        calls.push(call);
        removed.insert(target["signature"].as_str().unwrap().to_owned());
        after = api.detailed_info(part, false)?;
        ws.record("drawing_retirement_after_delete", after.clone())?;
        check_progress(i, &source, &before, &after, &removed)?;
    }
    api.save_work_part()?;
    ws.require_regular_output(name)?;
    let saved = api.detailed_info(part, false)?;
    ws.record("drawing_retirement_after_save", saved.clone())?;
    let check = check_progress(i, &source, &before, &saved, &removed)?;
    ensure!(std::fs::read(&input)? == source, "retirement_input_changed");
    Ok(
        json!({"action":"retire_drawing_dimensions","intent":i,"calls":calls,"parts":[{"input":name,"before":before,"after":after,"after_save":saved}],"check":check,"independent_reopen":"required_separate_process","engineering_adoption":"not_established"}),
    )
}
