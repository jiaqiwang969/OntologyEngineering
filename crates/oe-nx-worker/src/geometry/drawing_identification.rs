//! Bounded, ephemeral reference identification for native PDF review.
//! Labels describe nominal model circle centers, not manufacturing tolerances.
use super::{
    NativeApi, Workspace, drawing_annotation, drawing_pdf, input_relative, sheet_dimensions,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::DrawingIdentifiedPdfIntent;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const PARAMS_SCHEMA: &str = "oev2.nx-drawing-identified-pdf-params/1";
pub const INTENT_SCHEMA: &str = "oev2.drawing-identified-pdf-intent/1";
pub const INTENT_SCHEMA2: &str = "oev2.drawing-identified-pdf-intent/2";
pub const PARAMS_SCHEMA2: &str = "oev2.nx-drawing-identified-pdf-params/2";
pub fn params_schema(i: &DrawingIdentifiedPdfIntent) -> &'static str {
    if i.schema == INTENT_SCHEMA2 {
        PARAMS_SCHEMA2
    } else {
        PARAMS_SCHEMA
    }
}
pub fn validate(i: &DrawingIdentifiedPdfIntent) -> Result<()> {
    i.validate()
        .map_err(|_| anyhow::anyhow!("identified_pdf_contract_invalid"))?;
    ensure!(
        matches!(i.schema.as_str(), INTENT_SCHEMA | INTENT_SCHEMA2)
            && (i.schema == INTENT_SCHEMA2) == drawing_pdf::mixed(&i.base),
        "identified_pdf_schema_invalid"
    );
    drawing_pdf::validate(&i.base)?;
    ensure!(
        !i.references.is_empty()
            && i.references.len() <= 64
            && if i.schema == INTENT_SCHEMA2 {
                i.references.len() < i.base.inventory_signatures.len()
            } else {
                i.references.len() == i.base.inventory_signatures.len()
            },
        "identified_pdf_reference_coverage"
    );
    let mut ids = BTreeSet::new();
    for r in &i.references {
        ensure!(
            ["upper", "lower"].contains(&r.surface.as_str())
                && r.dimension.text_origin_drawing.len() == 2
                && ids.insert(&r.dimension.reference.intent_id),
            "identified_pdf_reference_invalid"
        );
        sheet_dimensions::validate(
            &i.base.sheet,
            &r.dimension.reference,
            [
                r.dimension.text_origin_drawing[0],
                r.dimension.text_origin_drawing[1],
            ],
        )?;
    }
    Ok(())
}
pub fn prepare(i: &DrawingIdentifiedPdfIntent, source: &[u8], part: &Value) -> Result<Value> {
    prepare_with_observation(i, source, part, false)
}
pub(super) fn prepare_with_observation(
    i: &DrawingIdentifiedPdfIntent,
    source: &[u8],
    part: &Value,
    prepared: bool,
) -> Result<Value> {
    validate(i)?;
    let original = drawing_pdf::prepare_with_observation(&i.base, source, part, prepared)?;
    ensure!(part["solid_count"] == 1, "identified_pdf_single_body_scope");
    let bbox = part["bbox"]
        .as_array()
        .filter(|a| a.len() == 1)
        .and_then(|a| a[0].as_array())
        .filter(|a| a.len() == 6)
        .context("identified_pdf_bounds_unknown")?;
    let bounds: Vec<f64> = bbox
        .iter()
        .map(|v| {
            v.as_f64()
                .filter(|v| v.is_finite())
                .context("identified_pdf_bounds_unknown")
        })
        .collect::<Result<_>>()?;
    let rows = drawing_pdf::inventory(&i.base, part)?;
    let mut used = BTreeSet::new();
    let mut targets = Vec::new();
    for r in &i.references {
        let reference = &r.dimension.reference;
        let z = reference.points[0].position[2];
        let bound = bounds[if r.surface == "upper" { 5 } else { 2 }];
        ensure!(
            bounds[5] - bounds[2] > 2.0 * reference.position_tolerance
                && reference
                    .points
                    .iter()
                    .all(|p| (p.position[2] - bound).abs() <= reference.position_tolerance)
                && reference
                    .points
                    .iter()
                    .all(|p| (p.position[2] - z).abs() <= reference.position_tolerance),
            "identified_pdf_surface_mismatch"
        );
        let mut matches = Vec::new();
        for row in rows.values() {
            let tag = row["tag"].as_u64().unwrap() as u32;
            if sheet_dimensions::check_created_with_observation(
                &i.base.sheet,
                reference,
                [
                    r.dimension.text_origin_drawing[0],
                    r.dimension.text_origin_drawing[1],
                ],
                source,
                part,
                tag,
                prepared,
            )
            .is_ok()
            {
                matches.push(tag);
            }
        }
        ensure!(
            matches.len() == 1 && used.insert(matches[0]),
            "identified_pdf_dimension_not_unique"
        );
        let label = format!(
            "{} CIRCLE CENTERS (MODEL Z={} mm)",
            r.surface.to_ascii_uppercase(),
            z
        );
        ensure!(label.len() <= 96, "identified_pdf_label_budget");
        targets.push(
            json!({"dimension_tag":matches[0], "label":label,"reference_id":reference.intent_id}),
        );
    }
    ensure!(
        used.len()
            == if i.schema == INTENT_SCHEMA2 {
                part["annotations"]["dimensions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|d| super::drawing_inventory::linear(d))
                    .count()
            } else {
                rows.len()
            },
        "identified_pdf_reference_coverage"
    );
    Ok(json!({"original":original,"targets":targets}))
}
pub fn check_label(v: &Value, tag: u32, label: Option<&str>) -> Result<()> {
    let text = label.unwrap_or("");
    ensure!(
        v["api"] == "NXOpen::Annotations::Dimension::AppendedText"
            && v["wrapper_code"] == 0
            && v["abi_version"] == 1
            && v["dimension_tag"] == tag
            && v["owner_matches"] == true
            && v["dimension_type_matches"] == true
            && v["counts"] == json!([0, 0, 0, if text.is_empty() { 0 } else { 1 }])
            && v["below"] == text
            && v["write_returned"] == label.is_some(),
        "identified_pdf_label_readback_unproven"
    );
    Ok(())
}
pub fn check_wcs(v: &Value, write: bool, expected: Option<bool>) -> Result<bool> {
    ensure!(
        v["api"] == "NXOpen::WCS::Visibility"
            && v["wrapper_code"] == 0
            && v["abi_version"] == 1
            && v["display_part_matches"] == true
            && v["write_returned"] == write,
        "identified_pdf_wcs_unproven"
    );
    let visible = v["visible"]
        .as_bool()
        .context("identified_pdf_wcs_unknown")?;
    ensure!(
        expected.is_none_or(|x| x == visible),
        "identified_pdf_wcs_mismatch"
    );
    Ok(visible)
}
pub(super) fn check_decorated(
    i: &DrawingIdentifiedPdfIntent,
    source: &[u8],
    before: &Value,
    after: &Value,
    plan: &Value,
    prepared: bool,
) -> Result<()> {
    sheet_dimensions::check_geometry_preserved(before, after)?;
    ensure!(
        super::drafting::check(&i.base.sheet, after)["conclusion"] == "pass",
        "identified_pdf_sheet_changed"
    );
    let dims = after["annotations"]["dimensions"]
        .as_array()
        .context("identified_pdf_inventory_unknown")?;
    ensure!(
        dims.len() == i.base.inventory_signatures.len(),
        "identified_pdf_inventory_changed"
    );
    // New mixed scope preserves every unlabelled dimension exactly, including text/association observations.
    if i.schema == INTENT_SCHEMA2 {
        let targets = plan["targets"].as_array().unwrap();
        for old in before["annotations"]["dimensions"].as_array().unwrap() {
            if !targets
                .iter()
                .any(|t| t["dimension_tag"] == old["dimension_tag"])
            {
                ensure!(
                    dims.iter()
                        .filter(|d| d["dimension_tag"] == old["dimension_tag"])
                        .count()
                        == 1
                        && dims.iter().any(|d| d == old),
                    "identified_pdf_unlabelled_dimension_changed"
                );
            }
        }
    }
    for (r, t) in i.references.iter().zip(plan["targets"].as_array().unwrap()) {
        let tag = t["dimension_tag"].as_u64().unwrap() as u32;
        sheet_dimensions::check_created_with_observation(
            &i.base.sheet,
            &r.dimension.reference,
            [
                r.dimension.text_origin_drawing[0],
                r.dimension.text_origin_drawing[1],
            ],
            source,
            after,
            tag,
            prepared,
        )?;
        let old = before["annotations"]["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["dimension_tag"] == tag)
            .unwrap();
        let new = dims.iter().find(|d| d["dimension_tag"] == tag).unwrap();
        for key in [
            "retained",
            "retained_query_code",
            "computed_size",
            "associativity_parents",
        ] {
            ensure!(
                old[key] == new[key] && !old[key].is_null(),
                "identified_pdf_dimension_changed"
            );
        }
    }
    Ok(())
}
pub(super) fn check_with_display(
    i: &DrawingIdentifiedPdfIntent,
    source: &[u8],
    r: &Value,
    pdf: &[u8],
    display: Option<&oe_contracts::DrawingDisplayPdfIntent>,
) -> Result<Value> {
    ensure!(
        r["action"]
            == if display.is_some() {
                "export_display_drawing_pdf"
            } else {
                "export_identified_drawing_pdf"
            }
            && r["intent"]
                == if let Some(d) = display {
                    serde_json::to_value(d)?
                } else {
                    serde_json::to_value(i)?
                }
            && r["output"] == drawing_pdf::PDF_NAME
            && r["native_save_called"] == false,
        "identified_pdf_recorded_intent_mismatch"
    );
    let plan = policy_plan(i, source, &r["before"], display)?;
    ensure!(r["plan"] == plan, "identified_pdf_plan_mismatch");
    if display.is_some_and(super::drawing_display::prepared_observation) {
        check_preparation(i, source, &r["context_preparation"], &r["before"], &plan)?;
        ensure!(
            r["decorated"]["drafting_readback_schema"] == "oev2.native-drafting-observation/7",
            "display3_decorated_observation_required"
        );
    }
    check_decorated(
        i,
        source,
        &r["before"],
        &r["decorated"],
        &plan,
        display.is_some_and(super::drawing_display::prepared_observation),
    )?;
    ensure!(
        policy_plan(i, source, &r["after"], display)? == plan,
        "identified_pdf_restored_inventory_mismatch"
    );
    sheet_dimensions::check_geometry_preserved(&r["before"], &r["after"])?;
    let targets = plan["targets"].as_array().unwrap();
    for key in ["labels_before", "labels_written", "labels_restored"] {
        let rows = r[key]
            .as_array()
            .context("identified_pdf_label_records_missing")?;
        ensure!(
            rows.len() == targets.len(),
            "identified_pdf_label_records_missing"
        );
        for (v, t) in rows.iter().zip(targets) {
            check_policy_label(
                display.is_some(),
                v,
                t["dimension_tag"].as_u64().unwrap() as u32,
                match key {
                    "labels_before" => None,
                    "labels_written" => t["label"].as_str(),
                    _ => Some(""),
                },
            )?;
        }
    }
    if let Some(d) = display {
        super::drawing_display::check_display(d, r)?;
        if super::drawing_display::annotation_restoration(d) {
            drawing_annotation::check_report(r, &plan)?;
        }
        if super::drawing_display::local_update(d) {
            super::drawing_update::check_report(r, &plan)?;
        }
    }
    let original = check_wcs(&r["wcs_before"], false, None)?;
    check_wcs(&r["wcs_hidden"], true, Some(false))?;
    check_wcs(&r["wcs_restored"], true, Some(original))?;
    drawing_pdf::check_call(
        &r["export"],
        plan["original"]["sheet_tag"].as_u64().unwrap() as u32,
    )?;
    let file = drawing_pdf::envelope(pdf)?;
    ensure!(r["pdf"] == file, "identified_pdf_output_identity_mismatch");
    Ok(
        json!({"conclusion":"pass","plan":plan,"pdf":file,"geometry_and_parent_references":"preserved",
        "annotation_and_WCS_state":"restored","native_save_called":false,"PDF_parser_acceptance":"required_separate_consumer",
        "printed_coordinate_triad_absence":"requires_visual_review","visual_legibility":"not_assessed","engineering_adoption":"not_established"}),
    )
}
pub(super) fn execute_with_display(
    api: &mut dyn NativeApi,
    ws: &mut dyn Workspace,
    name: &str,
    i: &DrawingIdentifiedPdfIntent,
    display: Option<&oe_contracts::DrawingDisplayPdfIntent>,
) -> Result<Value> {
    validate(i)?;
    let input = ws.input(&input_relative(name)?)?;
    let source = std::fs::read(&input)?;
    ensure!(
        oe_core::sha256(&source) == i.base.sheet.object.source_sha256,
        "identified_pdf_source_mismatch"
    );
    let copy = ws.output(name)?;
    let output = ws.output(drawing_pdf::PDF_NAME)?;
    ensure!(
        !copy.try_exists()? && !output.try_exists()?,
        "identified_pdf_output_exists"
    );
    std::fs::copy(&input, &copy)?;
    ensure!(
        ws.require_regular_output(name)? == source,
        "identified_pdf_copy_mismatch"
    );
    let part = api.open_part(&copy)?;
    api.set_work_part(part)?;
    let prepared = display.is_some_and(super::drawing_display::prepared_observation);
    let mut context_preparation = Value::Null;
    let before = if prepared {
        context_preparation = api.detailed_info(part, false)?;
        ws.record("display3_context_preparation", context_preparation.clone())?;
        let initial = prepare(i, &source, &context_preparation)?;
        api.observe_prepared_drawing(
            part,
            initial["original"]["sheet_tag"].as_u64().unwrap() as u32,
        )?
    } else {
        api.detailed_info(part, false)?
    };
    ws.record("identified_pdf_before", before.clone())?;
    let plan = policy_plan(i, &source, &before, display)?;
    if prepared {
        check_preparation(i, &source, &context_preparation, &before, &plan)?;
    }
    ws.record("identified_pdf_prepared", plan.clone())?;
    let targets = plan["targets"].as_array().unwrap();
    let display_before = if display.is_some() {
        api.drawing_display_inventory(part, super::drawing_display::view(&plan)?)?
    } else {
        Value::Null
    };
    ws.record("identified_pdf_display_before", display_before.clone())?;
    let hidden = if let Some(d) = display {
        super::drawing_display::select(d, &display_before)?
    } else {
        Vec::new()
    };
    let mut display_written = Vec::new();
    let mut display_phases = Vec::new();
    let restore_identity = display.is_some_and(super::drawing_display::guarded_restoration);
    let restore_layout = display.is_some_and(super::drawing_display::annotation_restoration);
    let updated = display.is_some_and(super::drawing_display::local_update);
    let mut annotation_original = Value::Null;
    let mut annotation_phases = Vec::new();
    let mut annotation_restoration = Vec::new();
    if restore_layout {
        annotation_original = drawing_annotation::capture(api, part, targets)?;
        ws.record("display4_annotation_original", annotation_original.clone())?;
        // Reject unknown or associative origins before any label mutation.
        drawing_annotation::check_capture(&annotation_original, part, targets)?;
    }
    let mut labels_before = Vec::new();
    // Query every original field before the first change. Never overwrite existing text.
    for t in targets {
        let tag = t["dimension_tag"].as_u64().unwrap() as u32;
        let value = policy_label(api, part, tag, None, display.is_some(), updated)?;
        ws.record("identified_pdf_original_label", value.clone())?;
        check_policy_label(display.is_some(), &value, tag, None)?;
        labels_before.push(value);
    }
    let wcs_before = api.drawing_wcs(part, None)?;
    ws.record("identified_pdf_original_wcs", wcs_before.clone())?;
    let original_visibility = check_wcs(&wcs_before, false, None)?;
    let attempt = (|| -> Result<(Value, Value, Value, Value)> {
        let mut written = Vec::new();
        for t in targets {
            let tag = t["dimension_tag"].as_u64().unwrap() as u32;
            ws.record("identified_pdf_label_attempt", t.clone())?;
            let value = policy_label(
                api,
                part,
                tag,
                t["label"].as_str(),
                display.is_some(),
                updated,
            )?;
            ws.record("identified_pdf_label_returned", value.clone())?;
            check_policy_label(display.is_some(), &value, tag, t["label"].as_str())?;
            if updated {
                super::drawing_update::check(&value["update"], part, tag)?;
            }
            written.push(value);
        }
        if restore_layout {
            let v = drawing_annotation::capture(api, part, targets)?;
            ws.record("display4_after_labels", v.clone())?;
            annotation_phases.push(json!({"phase":"after_labels","observations":v}));
            drawing_annotation::check_capture(&v, part, targets)?;
        }
        if restore_identity {
            let v = api.drawing_display_inventory(part, super::drawing_display::view(&plan)?)?;
            ws.record("display2_after_labels", v.clone())?;
            display_phases.push(json!({"phase":"after_labels","observation":v}));
        }
        for row in &hidden {
            let tag = super::drawing_display::tag(row)?;
            ws.record("identified_pdf_csys_blank_attempt", row.clone())?;
            let result = api.drawing_csys_blank(part, tag, true)?;
            ws.record("identified_pdf_csys_blank_returned", result.clone())?;
            super::drawing_display::check_blank(&result, row, true)?;
            display_written.push(result);
        }
        let hidden = api.drawing_wcs(part, Some(false))?;
        ws.record("identified_pdf_hidden_wcs", hidden.clone())?;
        check_wcs(&hidden, true, Some(false))?;
        if restore_identity {
            let v = api.drawing_display_inventory(part, super::drawing_display::view(&plan)?)?;
            ws.record("display2_after_visibility", v.clone())?;
            display_phases.push(json!({"phase":"after_visibility","observation":v}));
        }
        let decorated = policy_observe(api, part, &plan, prepared)?;
        ws.record("identified_pdf_decorated", decorated.clone())?;
        check_decorated(i, &source, &before, &decorated, &plan, prepared)?;
        if restore_identity {
            let v = api.drawing_display_inventory(part, super::drawing_display::view(&plan)?)?;
            ws.record("display2_after_decorated_observation", v.clone())?;
            display_phases.push(json!({"phase":"after_decorated_observation","observation":v}));
        }
        ws.record(
            "identified_pdf_export_attempt",
            json!({"output":drawing_pdf::PDF_NAME}),
        )?;
        let call = api.export_drawing_pdf(
            part,
            plan["original"]["sheet_tag"].as_u64().unwrap() as u32,
            &output,
        )?;
        ws.record("identified_pdf_export_returned", call.clone())?;
        drawing_pdf::check_call(
            &call,
            plan["original"]["sheet_tag"].as_u64().unwrap() as u32,
        )?;
        if restore_identity {
            let v = api.drawing_display_inventory(part, super::drawing_display::view(&plan)?)?;
            ws.record("display2_after_export", v.clone())?;
            display_phases.push(json!({"phase":"after_export","observation":v}));
        }
        if restore_layout {
            let v = drawing_annotation::capture(api, part, targets)?;
            ws.record("display4_after_export", v.clone())?;
            annotation_phases.push(json!({"phase":"after_export","observations":v}));
            drawing_annotation::check_capture(&v, part, targets)?;
        }
        Ok((json!(written), hidden, decorated, call))
    })();
    // Every cleanup runs even if a prior native call or event write failed.
    let mut restored = Vec::new();
    for t in targets {
        let tag = t["dimension_tag"].as_u64().unwrap() as u32;
        restored.push(
            policy_label(api, part, tag, Some(""), display.is_some(), updated)
                .unwrap_or_else(|e| json!({"error":e.to_string(),"dimension_tag":tag})),
        );
    }
    // An observation failure cannot prevent the other cleanups. Retain its
    // raw result and independently recheck each dimension before restoration.
    if restore_layout {
        let v = drawing_annotation::capture(api, part, targets)
            .unwrap_or_else(|e| json!({"error":e.to_string()}));
        annotation_phases.push(json!({"phase":"after_removal","observations":v}));
        for (t, old) in targets.iter().zip(annotation_original.as_array().unwrap()) {
            let tag = drawing_annotation::target(t)?;
            annotation_restoration.push(
                drawing_annotation::restore(api, part, tag, old)
                    .unwrap_or_else(|e| json!({"dimension_tag":tag,"error":e.to_string()})),
            );
        }
    }
    let mut display_restored = Vec::new();
    // /1 preserves its existing cleanup order. /2 first restores WCS identity so
    // coordinate readbacks also have the original WCS context.
    if !restore_identity {
        for row in &hidden {
            let tag = super::drawing_display::tag(row)?;
            display_restored.push(
                api.drawing_csys_blank(part, tag, false)
                    .unwrap_or_else(|e| json!({"error":e.to_string(),"tag":tag})),
            );
        }
    }
    let wcs_restored = api
        .drawing_wcs(part, Some(original_visibility))
        .unwrap_or_else(|e| json!({"error":e.to_string()}));
    let mut identity_restored = Value::Null;
    let mut identity_observed = Value::Null;
    if restore_identity {
        let observed = api
            .drawing_display_inventory(part, super::drawing_display::view(&plan)?)
            .unwrap_or_else(|e| json!({"error":e.to_string()}));
        identity_observed = observed.clone();
        display_phases.push(json!({"phase":"before_identity_restore","observation":observed}));
        let original = display_before["wcs_tag"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .context("display2_original_wcs_unknown")? as u32;
        identity_restored = api
            .restore_drawing_wcs_identity(part, original)
            .unwrap_or_else(|e| json!({"error":e.to_string()}));
        for row in &hidden {
            let tag = super::drawing_display::tag(row)?;
            display_restored.push(
                api.drawing_csys_blank(part, tag, false)
                    .unwrap_or_else(|e| json!({"error":e.to_string(),"tag":tag})),
            );
        }
    }
    if restore_layout {
        ws.record(
            "display4_after_removal",
            annotation_phases.last().unwrap()["observations"].clone(),
        )?;
        ws.record(
            "display4_annotation_restoration",
            json!(annotation_restoration),
        )?;
    }
    if restore_identity {
        ws.record("display2_before_identity_restore", identity_observed)?;
        ws.record("display2_identity_restoration", identity_restored.clone())?;
    }
    ws.record("identified_pdf_restoration", json!({"labels":restored,"wcs":wcs_restored,"display":display_restored,"attempt_error":attempt.as_ref().err().map(ToString::to_string)}))?;
    let after = policy_observe(api, part, &plan, prepared)?;
    ws.record("identified_pdf_after_restoration", after.clone())?;
    if restore_layout {
        let v = drawing_annotation::capture(api, part, targets)?;
        ws.record("display4_after_final_observation", v.clone())?;
        annotation_phases.push(json!({"phase":"after_final_observation","observations":v}));
    }
    ensure!(
        std::fs::read(&input)? == source && ws.require_regular_output(name)? == source,
        "identified_pdf_source_or_copy_changed"
    );
    let display_after = if display.is_some() {
        api.drawing_display_inventory(part, super::drawing_display::view(&plan)?)?
    } else {
        Value::Null
    };
    ws.record("identified_pdf_display_after", display_after.clone())?;
    let (written, hidden, decorated, call) = attempt?;
    let pdf = ws.require_regular_output(drawing_pdf::PDF_NAME)?;
    let mut report = json!({"action":"export_identified_drawing_pdf","input":name,"output":drawing_pdf::PDF_NAME,"intent":i,"plan":plan,
        "before":before,"decorated":decorated,"after":after,"labels_before":labels_before,"labels_written":written,"labels_restored":restored,
        "wcs_before":wcs_before,"wcs_hidden":hidden,"wcs_restored":wcs_restored,"export":call,"pdf":drawing_pdf::envelope(&pdf)?,
        "native_save_called":false,"engineering_adoption":"not_established"});
    if let Some(d) = display {
        report["action"] = json!("export_display_drawing_pdf");
        report["intent"] = serde_json::to_value(d)?;
        report["display_before"] = display_before;
        report["display_written"] = json!(display_written);
        report["display_restored"] = json!(display_restored);
        report["display_after"] = display_after;
        if restore_identity {
            report["wcs_identity_restored"] = identity_restored;
            report["display_phases"] = json!(display_phases);
        }
        if prepared {
            report["context_preparation"] = context_preparation;
        }
        if restore_layout {
            report["annotation_original"] = annotation_original;
            report["annotation_phases"] = json!(annotation_phases);
            report["annotation_restoration"] = json!(annotation_restoration);
        }
    }
    check_with_display(i, &source, &report, &pdf, display)?;
    Ok(report)
}

pub fn check_execution(
    i: &DrawingIdentifiedPdfIntent,
    source: &[u8],
    r: &Value,
    pdf: &[u8],
) -> Result<Value> {
    check_with_display(i, source, r, pdf, None)
}
pub fn execute(
    api: &mut dyn NativeApi,
    ws: &mut dyn Workspace,
    name: &str,
    i: &DrawingIdentifiedPdfIntent,
) -> Result<Value> {
    execute_with_display(api, ws, name, i, None)
}
fn policy_plan(
    i: &DrawingIdentifiedPdfIntent,
    source: &[u8],
    part: &Value,
    d: Option<&oe_contracts::DrawingDisplayPdfIntent>,
) -> Result<Value> {
    if let Some(d) = d {
        super::drawing_display::prepare(d, source, part)
    } else {
        prepare(i, source, part)
    }
}
fn check_preparation(
    i: &DrawingIdentifiedPdfIntent,
    source: &[u8],
    preparation: &Value,
    before: &Value,
    plan: &Value,
) -> Result<()> {
    let initial = prepare(i, source, preparation)?;
    sheet_dimensions::check_geometry_preserved(preparation, before)?;
    ensure!(
        preparation["part_tag"] == before["part_tag"]
            && initial["original"]["sheet_tag"] == plan["original"]["sheet_tag"],
        "display3_preparation_binding_changed"
    );
    for (old, new) in initial["targets"]
        .as_array()
        .unwrap()
        .iter()
        .zip(plan["targets"].as_array().unwrap())
    {
        ensure!(
            old["dimension_tag"] == new["dimension_tag"]
                && old["reference_id"] == new["reference_id"],
            "display3_preparation_reference_changed"
        );
    }
    Ok(())
}
fn policy_observe(
    api: &mut dyn NativeApi,
    part: u32,
    plan: &Value,
    prepared: bool,
) -> Result<Value> {
    if prepared {
        let value = api.observe_prepared_drawing(
            part,
            plan["original"]["sheet_tag"].as_u64().unwrap() as u32,
        )?;
        ensure!(
            value["drafting_readback_schema"] == "oev2.native-drafting-observation/7",
            "display3_prepared_observation_required"
        );
        Ok(value)
    } else {
        api.detailed_info(part, false)
    }
}
fn policy_label(
    api: &mut dyn NativeApi,
    part: u32,
    tag: u32,
    label: Option<&str>,
    display: bool,
    updated: bool,
) -> Result<Value> {
    if updated && let Some(label) = label {
        api.dimension_label_lines_updated(part, tag, label)
    } else if display {
        api.dimension_label_lines(part, tag, label)
    } else {
        api.dimension_label(part, tag, label)
    }
}
fn check_policy_label(display: bool, v: &Value, tag: u32, label: Option<&str>) -> Result<()> {
    if display {
        super::drawing_display::check_label(v, tag, label)
    } else {
        check_label(v, tag, label)
    }
}
