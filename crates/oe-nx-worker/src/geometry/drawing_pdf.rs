//! Native, source-bound single-sheet PDF export. File structure and legibility
//! require separate consumers; an SDK commit is not drawing acceptance.
use super::{NativeApi, Workspace, drafting, drawing_retirement, input_relative, sheet_dimensions};
use anyhow::{Context, Result, ensure};
use oe_contracts::DrawingPdfIntent;
use oe_core::sha256;
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub const PARAMS_SCHEMA: &str = "oev2.nx-drawing-pdf-params/1";
pub const INTENT_SCHEMA: &str = "oev2.drawing-pdf-intent/1";
pub const INTENT_SCHEMA2: &str = "oev2.drawing-pdf-intent/2";
pub const PARAMS_SCHEMA2: &str = "oev2.nx-drawing-pdf-params/2";
pub fn mixed(i: &DrawingPdfIntent) -> bool {
    i.schema == INTENT_SCHEMA2
}
pub fn params_schema(i: &DrawingPdfIntent) -> &'static str {
    if mixed(i) {
        PARAMS_SCHEMA2
    } else {
        PARAMS_SCHEMA
    }
}
pub fn inventory(
    i: &DrawingPdfIntent,
    part: &Value,
) -> Result<std::collections::BTreeMap<String, Value>> {
    if mixed(i) {
        super::drawing_inventory::inventory(part)
    } else {
        drawing_retirement::inventory(part)
    }
}
pub const PDF_NAME: &str = "drawing.pdf";
pub fn validate(i: &DrawingPdfIntent) -> Result<()> {
    i.validate()
        .map_err(|_| anyhow::anyhow!("drawing_pdf_contract_invalid"))?;
    ensure!(
        matches!(i.schema.as_str(), INTENT_SCHEMA | INTENT_SCHEMA2),
        "drawing_pdf_schema_invalid"
    );
    drafting::validate(&i.sheet)?;
    ensure!(i.sheet.unit == "mm", "drawing_pdf_metric_scope_required");
    ensure!(
        !i.inventory_signatures.is_empty()
            && i.inventory_signatures.len() <= 256
            && i.inventory_signatures.iter().all(|v| v.len() == 64
                && v.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
            && i.inventory_signatures.iter().collect::<BTreeSet<_>>().len()
                == i.inventory_signatures.len(),
        "drawing_pdf_inventory_invalid"
    );
    ensure!(
        !i.source_refs.is_empty()
            && i.source_refs.len() <= 64
            && i.source_refs.iter().all(|s| !s.trim().is_empty())
            && i.source_refs.iter().collect::<BTreeSet<_>>().len() == i.source_refs.len(),
        "drawing_pdf_sources_invalid"
    );
    Ok(())
}
pub fn prepare(i: &DrawingPdfIntent, source: &[u8], part: &Value) -> Result<Value> {
    prepare_with_observation(i, source, part, false)
}
pub(super) fn prepare_with_observation(
    i: &DrawingPdfIntent,
    source: &[u8],
    part: &Value,
    prepared: bool,
) -> Result<Value> {
    validate(i)?;
    ensure!(
        sha256(source) == i.sheet.object.source_sha256,
        "drawing_pdf_source_mismatch"
    );
    ensure!(
        part["view_refresh_requested"] == false
            && part["drafting_readback_schema"]
                == if prepared {
                    "oev2.native-drafting-observation/7"
                } else {
                    "oev2.native-drafting-observation/6"
                },
        "drawing_pdf_current_unmodified_observation_required"
    );
    let sheet = drafting::check(&i.sheet, part);
    ensure!(sheet["conclusion"] == "pass", "drawing_pdf_sheet_unproven");
    let rows = inventory(i, part)?;
    if mixed(i) {
        let dims = part["annotations"]["dimensions"].as_array().unwrap();
        ensure!(
            dims.iter().any(super::drawing_inventory::linear)
                && dims.iter().any(|d| !super::drawing_inventory::linear(d)),
            "drawing_pdf_mixed_kinds_required"
        );
    }
    ensure!(
        rows.keys().cloned().collect::<BTreeSet<_>>()
            == i.inventory_signatures.iter().cloned().collect(),
        "drawing_pdf_inventory_mismatch"
    );
    ensure!(
        rows.values().all(|v| v["retained"] == false),
        "drawing_pdf_retained_dimension_present"
    );
    let tag = sheet["details"]["sheet"]["drawing_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("drawing_pdf_sheet_tag_invalid")?;
    Ok(json!({"conclusion":"pass","sheet_tag":tag,"sheet":sheet,"dimension_count":rows.len()}))
}
pub fn check_call(call: &Value, sheet: u32) -> Result<()> {
    ensure!(
        call["api"] == "NXOpen::PrintPDFBuilder"
            && call["wrapper_code"] == 0
            && call["sheet_tag"] == sheet
            && call["abi_version"] == 1
            && [
                "display_part_matches",
                "sheet_owner_matches",
                "selected_sheet_matches",
                "settings_match",
                "validated",
                "commit_entered",
                "commit_returned",
                "builder_destroyed"
            ]
            .iter()
            .all(|k| call[*k] == true)
            && call["selected_count"] == 1,
        "drawing_pdf_native_export_unproven"
    );
    Ok(())
}
pub fn envelope(pdf: &[u8]) -> Result<Value> {
    ensure!(
        pdf.len() >= 16 && pdf.len() <= 32 * 1024 * 1024 && pdf.starts_with(b"%PDF-"),
        "drawing_pdf_envelope_invalid"
    );
    let tail = &pdf[pdf.len().saturating_sub(1024)..];
    let pos = tail
        .windows(5)
        .rposition(|w| w == b"%%EOF")
        .context("drawing_pdf_eof_missing")?;
    ensure!(
        tail[pos + 5..].iter().all(u8::is_ascii_whitespace),
        "drawing_pdf_trailing_content"
    );
    Ok(
        json!({"bytes":pdf.len(),"sha256":sha256(pdf),"envelope_present":true,"PDF_parser_acceptance":"requires_independent_consumer","visual_legibility":"not_assessed"}),
    )
}
pub fn check_execution(
    i: &DrawingPdfIntent,
    source: &[u8],
    r: &Value,
    pdf: &[u8],
) -> Result<Value> {
    ensure!(
        r["action"] == "export_drawing_pdf"
            && r["intent"] == serde_json::to_value(i)?
            && r["output"] == PDF_NAME,
        "drawing_pdf_recorded_intent_mismatch"
    );
    let before = prepare(i, source, &r["before"])?;
    let after = prepare(i, source, &r["after"])?;
    sheet_dimensions::check_geometry_preserved(&r["before"], &r["after"])?;
    check_call(&r["export"], before["sheet_tag"].as_u64().unwrap() as u32)?;
    let file = envelope(pdf)?;
    ensure!(r["pdf"] == file, "drawing_pdf_output_identity_mismatch");
    Ok(
        json!({"conclusion":"pass","before":before,"after":after,"pdf":file,"scope":"native source and output binding, unchanged observed geometry and annotations; independent PDF parsing and visual inspection required","engineering_adoption":"not_established"}),
    )
}
pub fn execute(
    api: &mut dyn NativeApi,
    ws: &mut dyn Workspace,
    name: &str,
    i: &DrawingPdfIntent,
) -> Result<Value> {
    let input = ws.input(&input_relative(name)?)?;
    let source = std::fs::read(&input)?;
    validate(i)?;
    ensure!(
        sha256(&source) == i.sheet.object.source_sha256,
        "drawing_pdf_source_mismatch"
    );
    let copy = ws.output(name)?;
    let output = ws.output(PDF_NAME)?;
    ensure!(
        !copy.try_exists()? && !output.try_exists()?,
        "drawing_pdf_output_already_exists"
    );
    std::fs::copy(&input, &copy)?;
    ensure!(
        ws.require_regular_output(name)? == source,
        "drawing_pdf_copy_mismatch"
    );
    let part = api.open_part(&copy)?;
    api.set_work_part(part)?;
    let before = api.detailed_info(part, false)?;
    ws.record("drawing_pdf_before", before.clone())?;
    let plan = prepare(i, &source, &before)?;
    ws.record("drawing_pdf_prepared", plan.clone())?;
    let sheet = plan["sheet_tag"].as_u64().unwrap() as u32;
    ws.record(
        "drawing_pdf_export_attempt",
        json!({"part":part,"sheet":sheet,"output":PDF_NAME}),
    )?;
    let call = api.export_drawing_pdf(part, sheet, &output)?;
    ws.record("drawing_pdf_export_returned", call.clone())?;
    check_call(&call, sheet)?;
    let after = api.detailed_info(part, false)?;
    ws.record("drawing_pdf_after", after.clone())?;
    let pdf = ws.require_regular_output(PDF_NAME)?;
    let report = json!({"action":"export_drawing_pdf","intent":i,"input":name,"output":PDF_NAME,"before":before,"after":after,"export":call,"pdf":envelope(&pdf)?,"native_save_called":false,"engineering_adoption":"not_established"});
    check_execution(i, &source, &report, &pdf)?;
    ensure!(
        std::fs::read(&input)? == source && ws.require_regular_output(name)? == source,
        "drawing_pdf_source_or_copy_changed"
    );
    Ok(report)
}
