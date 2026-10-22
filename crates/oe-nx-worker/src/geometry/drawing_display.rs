//! Source-bound, reversible drawing-display trials; rendered clarity is independent.
use super::{NativeApi, Workspace, drawing_identification};
use anyhow::{Context, Result, ensure};
use oe_contracts::DrawingDisplayPdfIntent;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const PARAMS_SCHEMA: &str = "oev2.nx-drawing-display-pdf-params/1";
pub const INTENT_SCHEMA: &str = "oev2.drawing-display-pdf-intent/1";
pub const INTENT_SCHEMA2: &str = "oev2.drawing-display-pdf-intent/2";
pub const PARAMS_SCHEMA2: &str = "oev2.nx-drawing-display-pdf-params/2";
pub const INTENT_SCHEMA3: &str = "oev2.drawing-display-pdf-intent/3";
pub const PARAMS_SCHEMA3: &str = "oev2.nx-drawing-display-pdf-params/3";
pub const INTENT_SCHEMA4: &str = "oev2.drawing-display-pdf-intent/4";
pub const PARAMS_SCHEMA4: &str = "oev2.nx-drawing-display-pdf-params/4";
pub const INTENT_SCHEMA5: &str = "oev2.drawing-display-pdf-intent/5";
pub const PARAMS_SCHEMA5: &str = "oev2.nx-drawing-display-pdf-params/5";
pub const INTENT_SCHEMA6: &str = "oev2.drawing-display-pdf-intent/6";
pub const PARAMS_SCHEMA6: &str = "oev2.nx-drawing-display-pdf-params/6";
pub fn local_update(i: &DrawingDisplayPdfIntent) -> bool {
    [INTENT_SCHEMA5, INTENT_SCHEMA6].contains(&i.schema.as_str())
}
pub fn annotation_restoration(i: &DrawingDisplayPdfIntent) -> bool {
    [INTENT_SCHEMA4, INTENT_SCHEMA5, INTENT_SCHEMA6].contains(&i.schema.as_str())
}
pub fn params_schema(i: &DrawingDisplayPdfIntent) -> &'static str {
    if i.schema == INTENT_SCHEMA6 {
        PARAMS_SCHEMA6
    } else if local_update(i) {
        PARAMS_SCHEMA5
    } else if annotation_restoration(i) {
        PARAMS_SCHEMA4
    } else if prepared_observation(i) {
        PARAMS_SCHEMA3
    } else if i.schema == INTENT_SCHEMA2 {
        PARAMS_SCHEMA2
    } else {
        PARAMS_SCHEMA
    }
}
pub fn guarded_restoration(i: &DrawingDisplayPdfIntent) -> bool {
    [
        INTENT_SCHEMA2,
        INTENT_SCHEMA3,
        INTENT_SCHEMA4,
        INTENT_SCHEMA5,
        INTENT_SCHEMA6,
    ]
    .contains(&i.schema.as_str())
}
pub fn prepared_observation(i: &DrawingDisplayPdfIntent) -> bool {
    [
        INTENT_SCHEMA3,
        INTENT_SCHEMA4,
        INTENT_SCHEMA5,
        INTENT_SCHEMA6,
    ]
    .contains(&i.schema.as_str())
}
pub fn check_wcs_restoration(v: &Value, original: u32) -> Result<()> {
    ensure!(
        v["api"] == "UF_CSYS_set_wcs_guarded"
            && v["wrapper_code"] == 0
            && v["abi_version"] == 1
            && v["original_tag"] == original
            && v["original_alive"] == true
            && v["owner_matches"] == true
            && v["type_matches"] == true
            && v["query_before_code"] == 0
            && v["query_after_code"] == 0
            && v["after_tag"] == original
            && v["set_code"] == 0,
        "display2_original_wcs_not_restored"
    );
    let before = v["before_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("display2_wcs_before_unknown")?;
    ensure!(
        v["set_called"] == (before != u64::from(original)),
        "display2_wcs_restore_call_mismatch"
    );
    Ok(())
}
pub fn validate(i: &DrawingDisplayPdfIntent) -> Result<()> {
    i.validate()
        .map_err(|_| anyhow::anyhow!("display_pdf_contract_invalid"))?;
    ensure!(
        [
            INTENT_SCHEMA,
            INTENT_SCHEMA2,
            INTENT_SCHEMA3,
            INTENT_SCHEMA4,
            INTENT_SCHEMA5,
            INTENT_SCHEMA6
        ]
        .contains(&i.schema.as_str()),
        "display_pdf_schema_invalid"
    );
    drawing_identification::validate(&i.base)?;
    ensure!(
        (i.schema == INTENT_SCHEMA6) == (i.base.schema == drawing_identification::INTENT_SCHEMA2),
        "display_pdf_nested_schema_mismatch"
    );
    ensure!(i.hidden_csys.len() <= 16, "display_pdf_selection_budget");
    for s in &i.hidden_csys {
        ensure!(
            s.origin.len() == 3
                && s.matrix.len() == 9
                && s.origin.iter().chain(&s.matrix).all(|v| v.is_finite())
                && s.tolerance.is_finite()
                && s.tolerance > 0.0
                && s.tolerance <= 1e-6,
            "display_pdf_selector_invalid"
        );
    }
    Ok(())
}
pub fn prepare(i: &DrawingDisplayPdfIntent, source: &[u8], part: &Value) -> Result<Value> {
    validate(i)?;
    let mut p = drawing_identification::prepare_with_observation(
        &i.base,
        source,
        part,
        prepared_observation(i),
    )?;
    for (t, r) in p["targets"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(&i.base.references)
    {
        let lines = [
            format!(
                "{} {}",
                r.surface.to_ascii_uppercase(),
                if guarded_restoration(i) {
                    "CTRS"
                } else {
                    "CENTERS"
                }
            ),
            format!("MODEL Z={} mm", r.dimension.reference.points[0].position[2]),
        ];
        ensure!(
            lines.iter().all(|s| s.is_ascii() && s.len() <= 24),
            "display_pdf_line_budget"
        );
        t["label"] = json!(lines.join("\n"));
    }
    Ok(p)
}
pub fn view(p: &Value) -> Result<u32> {
    let views = p["original"]["sheet"]["details"]["sheet"]["views"]
        .as_array()
        .filter(|v| v.len() == 1)
        .context("display_pdf_single_view_required")?;
    let v = views[0]["view_tag"]
        .as_u64()
        .context("display_pdf_view_unknown")?;
    ensure!(v > 0 && v <= u32::MAX as u64, "display_pdf_view_unknown");
    Ok(v as u32)
}
pub fn tag(v: &Value) -> Result<u32> {
    let n = v["tag"].as_u64().context("display_pdf_tag_unknown")?;
    ensure!(n > 0 && n <= u32::MAX as u64, "display_pdf_tag_unknown");
    Ok(n as u32)
}
fn csys(v: &Value) -> Result<()> {
    ensure!(
        v["wrapper_code"] == 0
            && v["abi_version"] == 1
            && v["alive"] == true
            && v["owner_matches"] == true
            && v["is_csys"] == true
            && v["is_wcs"] == false
            && v["csys_complete"] == true
            && v["display_complete"] == true
            && v["blank"].is_boolean(),
        "display_pdf_coordinate_identity_unproven"
    );
    tag(v)?;
    Ok(())
}
fn close(a: &Value, b: &[f64], tolerance: f64) -> bool {
    a.as_array().is_some_and(|a| {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| {
                x.as_f64()
                    .is_some_and(|x| x.is_finite() && (x - y).abs() <= tolerance)
            })
    })
}
pub fn select(i: &DrawingDisplayPdfIntent, v: &Value) -> Result<Vec<Value>> {
    validate(i)?;
    ensure!(
        v["schema"] == "oev2.native-drawing-display/1" && v["capture_complete"] == true,
        "display_pdf_inventory_incomplete"
    );
    let rows = v["coordinate_systems"]
        .as_array()
        .filter(|v| v.len() <= 4096)
        .context("display_pdf_coordinate_inventory_missing")?;
    let mut seen = BTreeSet::new();
    for r in rows {
        ensure!(seen.insert(tag(r)?), "display_pdf_duplicate_inventory");
    }
    let mut selected = Vec::new();
    let mut used = BTreeSet::new();
    if !i.hidden_csys.is_empty() {
        for r in rows {
            ensure!(
                r["csys_complete"] == true
                    && r["origin"].as_array().is_some_and(|a| a.len() == 3
                        && a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite)))
                    && r["matrix"].as_array().is_some_and(|a| a.len() == 9
                        && a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite))),
                "display_pdf_unknown_coordinate_cannot_exclude_ambiguity"
            );
        }
    }
    for s in &i.hidden_csys {
        // Unknown metadata must never erase an ambiguous geometry match.
        let matching: Vec<_> = rows
            .iter()
            .filter(|r| {
                close(&r["origin"], &s.origin, s.tolerance)
                    && close(&r["matrix"], &s.matrix, s.tolerance)
            })
            .collect();
        ensure!(matching.len() == 1, "display_pdf_coordinate_not_unique");
        let row = matching[0];
        csys(row)?;
        ensure!(
            row["blank"] == false && used.insert(tag(row)?),
            "display_pdf_coordinate_not_selectable"
        );
        selected.push(row.clone());
    }
    Ok(selected)
}
pub fn check_blank(v: &Value, before: &Value, blank: bool) -> Result<()> {
    csys(v)?;
    ensure!(
        v["write_returned"] == true && v["blank"] == blank,
        "display_pdf_blank_unproven"
    );
    for k in [
        "tag",
        "type_raw",
        "subtype_raw",
        "owner_tag",
        "wcs_tag",
        "matrix_tag",
        "origin",
        "matrix",
        "layer",
        "color",
        "font",
        "line_width",
    ] {
        ensure!(
            !v[k].is_null() && v[k] == before[k],
            "display_pdf_coordinate_changed"
        );
    }
    Ok(())
}
pub fn check_label(v: &Value, tag: u32, text: Option<&str>) -> Result<()> {
    let text_value = text.unwrap_or("");
    let n = if text_value.is_empty() { 0 } else { 2 };
    ensure!(
        text_value.is_empty() || text_value.split('\n').count() == 2,
        "display_pdf_line_count"
    );
    ensure!(
        v["api"] == "NXOpen::Annotations::Dimension::AppendedText"
            && v["wrapper_code"] == 0
            && v["abi_version"] == 2
            && v["dimension_tag"] == tag
            && v["owner_matches"] == true
            && v["dimension_type_matches"] == true
            && v["counts"] == json!([0, 0, 0, n])
            && v["below"] == text_value
            && v["write_returned"] == text.is_some(),
        "display_pdf_label_unproven"
    );
    Ok(())
}
pub fn check_display(i: &DrawingDisplayPdfIntent, r: &Value) -> Result<()> {
    if guarded_restoration(i) {
        let original = r["display_before"]["wcs_tag"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .context("display2_original_wcs_unknown")? as u32;
        check_wcs_restoration(&r["wcs_identity_restored"], original)?;
        let phases = r["display_phases"]
            .as_array()
            .filter(|v| v.len() == 5)
            .context("display2_phase_observations_missing")?;
        for (v, name) in phases.iter().zip([
            "after_labels",
            "after_visibility",
            "after_decorated_observation",
            "after_export",
            "before_identity_restore",
        ]) {
            ensure!(
                v["phase"] == name && v["observation"]["capture_complete"] == true,
                "display2_phase_observation_unproven"
            );
        }
    }
    ensure!(
        r["display_before"]["view_tag"] == view(&r["plan"])?
            && r["display_before"]["wcs_tag"]
                .as_u64()
                .is_some_and(|n| n > 0 && n <= u32::MAX as u64),
        "display_pdf_inventory_view_or_wcs_mismatch"
    );
    let selected = select(i, &r["display_before"])?;
    ensure!(
        r["display_before"] == r["display_after"],
        "display_pdf_inventory_not_restored"
    );
    for (key, blank) in [("display_written", true), ("display_restored", false)] {
        let rows = r[key].as_array().context("display_pdf_receipts_missing")?;
        ensure!(rows.len() == selected.len(), "display_pdf_receipts_missing");
        for (v, before) in rows.iter().zip(&selected) {
            check_blank(v, before, blank)?;
        }
    }
    Ok(())
}
pub fn check_execution(
    i: &DrawingDisplayPdfIntent,
    source: &[u8],
    r: &Value,
    pdf: &[u8],
) -> Result<Value> {
    validate(i)?;
    drawing_identification::check_with_display(&i.base, source, r, pdf, Some(i))
}
pub fn execute(
    api: &mut dyn NativeApi,
    ws: &mut dyn Workspace,
    name: &str,
    i: &DrawingDisplayPdfIntent,
) -> Result<Value> {
    validate(i)?;
    drawing_identification::execute_with_display(api, ws, name, &i.base, Some(i))
}
