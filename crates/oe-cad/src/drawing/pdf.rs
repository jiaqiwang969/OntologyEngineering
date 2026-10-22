//! Receipt acceptance does not substitute PDF parsing, visibility or fabrication checks.
use anyhow::{Context, Result, ensure};
use oe_nx_worker::geometry::{Parameters, drawing_display, drawing_identification, drawing_pdf};
use serde_json::{Value, json};
use std::path::Path;
pub fn receive(root: &Path) -> Result<Value> {
    let (result, job) = super::associations::receipt(root)?;
    ensure!(
        [
            "export_drawing_pdf",
            "export_identified_drawing_pdf",
            "export_display_drawing_pdf"
        ]
        .contains(&result.action.as_str()),
        "drawing_pdf_action_required"
    );
    let read =
        |name: &str, limit: usize| oe_core::read_regular_bytes(&super::member(root, name)?, limit);
    let params: Parameters = oe_nx_protocol::decode(&read("params.json", 8 * 1024 * 1024)?)?;
    params.validate()?;
    let (part, intent, identified, display) = match params {
        Parameters::ExportDrawingPdf { part, intent, .. } => (part, intent, None, None),
        Parameters::ExportIdentifiedDrawingPdf { part, intent, .. } => {
            (part, intent.base.clone(), Some(intent), None)
        }
        Parameters::ExportDisplayDrawingPdf { part, intent, .. } => {
            (part, intent.base.base.clone(), None, Some(intent))
        }
        _ => anyhow::bail!("drawing_pdf_parameters_required"),
    };
    let source = read(&format!("inputs/{part}"), 512 * 1024 * 1024)?;
    job.inputs
        .iter()
        .find(|f| f.path.as_str() == format!("inputs/{part}"))
        .context("drawing_pdf_original_input_missing")?
        .identity
        .verify(&source)?;
    ensure!(
        result.readback["input"] == part && result.readback["native_save_called"] == false,
        "drawing_pdf_input_or_save_mismatch"
    );
    ensure!(
        read(&format!("out/{part}"), 512 * 1024 * 1024)? == source,
        "drawing_pdf_copy_changed"
    );
    let bytes = read("out/drawing.pdf", 32 * 1024 * 1024)?;
    let check = if let Some(d) = &display {
        drawing_display::check_execution(d, &source, &result.readback, &bytes)?
    } else if let Some(i) = &identified {
        drawing_identification::check_execution(i, &source, &result.readback, &bytes)?
    } else {
        drawing_pdf::check_execution(&intent, &source, &result.readback, &bytes)?
    };
    Ok(
        json!({"schema":if drawing_pdf::mixed(&intent){"oev2.drawing-pdf-receiving/4"}else if display.is_some(){"oev2.drawing-pdf-receiving/3"}else if identified.is_some(){"oev2.drawing-pdf-receiving/2"}else{"oev2.drawing-pdf-receiving/1"},"check":check,"process":result.process,"worker":result.worker,"worker_manifest_digest":result.worker_manifest_digest,"runtime":result.runtime,"source_sha256":intent.sheet.object.source_sha256,"pdf":drawing_pdf::envelope(&bytes)?,"PDF_parser_acceptance":"required_separate_consumer","visual_review":"required","engineering_adoption":"not_established"}),
    )
}
