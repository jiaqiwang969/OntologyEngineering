//! One registered vol2/page24 development replay; no OCR accuracy/unseen gold.
use anyhow::{Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use oe_knowledge::{ocr_page5, poppler::PopplerTools, registered_ocr6};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 5,
        "usage:root request6 original-image-gold fresh-leaf"
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    let root = SkillRoot::new(&args[1])?;
    let read = |s: &str| -> Result<Value> {
        Ok(oe_core::json::strict_json(&read_regular_bytes(
            Path::new(s),
            1024 * 1024,
        )?)?)
    };
    let input = read(&args[2])?;
    let request = registered_ocr6::parse_request6(&input)?;
    let gold = read(&args[3])?;
    ensure!(
        request.document_id == "vol2"
            && request.page.physical_page == 24
            && request.page.dpi == 144
            && gold["development_seen"] == true
            && gold["unseen"] == false
            && gold["ocr_accuracy_gold"] == "none",
        "frozen_development_replay_input"
    );
    let tools = PopplerTools {
        pdfinfo: "/opt/homebrew/Cellar/poppler/26.02.0_1/bin/pdfinfo".into(),
        pdftotext: "/not-used".into(),
        pdftoppm: request.page.renderer.path.clone().into(),
    };
    // New legal registered page remains refused by original development5 before
    // any old5 render/tool invocation. This is not an extra OCR attempt.
    let refused = ocr_page5::prepare_page_and_ocr5(
        &root,
        &request.page,
        &request.ocr,
        &tools,
        deadline,
        "unused-old5-refused-general-page6",
    )
    .unwrap_err();
    ensure!(
        refused
            .to_string()
            .contains("ocr_page5_development_page_domain"),
        "old5_domain_changed"
    );
    registered_ocr6::check_registered_request6(&root, &request)?;
    let mut admission_controls = Vec::new();
    for (name, path, value) in [
        ("registered_count", vec!["page_count"], json!(352)),
        (
            "indexed_native_sha",
            vec!["page", "text_sha256"],
            json!("b".repeat(64)),
        ),
        (
            "whole_document_sha",
            vec!["page", "document_sha256"],
            json!("b".repeat(64)),
        ),
        (
            "registration_sha",
            vec!["page", "registration", "sha256"],
            json!("b".repeat(64)),
        ),
        (
            "index_sha",
            vec!["page", "index", "sha256"],
            json!("b".repeat(64)),
        ),
    ] {
        let mut changed = input.clone();
        let mut selected = &mut changed;
        for key in path {
            selected = &mut selected[key];
        }
        *selected = value;
        let r = registered_ocr6::parse_request6(&changed)?;
        let refused = registered_ocr6::check_registered_request6(&root, &r).is_err();
        ensure!(refused, "registered_admission_accepted:{name}");
        admission_controls.push(
            json!({"name":name,"refused":true,"scope":"current_source_read_only_no_process"}),
        );
    }
    let prepared =
        registered_ocr6::prepare_registered_page_ocr6(&root, &request, &tools, deadline, &args[4])?;
    registered_ocr6::verify_prepared_registered_ocr6(&prepared, prepared.report())?;
    let source = registered_ocr6::read_source_content6(&prepared)?;
    let report = serde_json::to_value(source.provenance())?;
    let c = source.content();
    ensure!(
        report["page"]["render"]["png"]["sha256"] == gold["png_sha256"]
            && report["page"]["render"]["png"]["bytes"] == gold["png_bytes"]
            && report["page"]["image_content"]["decoded_sha256"] == gold["pixels_sha256"]
            && report["page"]["native_text_chars"] == gold["native_text_chars"]
            && report["page"]["navigation_only"] == gold["navigation_only"]
            && report["page"]["text_sha256"] == gold["native_text_sha256"]
            && report["page"]["raw_text_sha256"] == gold["raw_native_text_sha256"],
        "pre_output_independent_original_page_gold"
    );
    let mut raw = c.header_raw.clone();
    for (i, r) in c.rows.iter().enumerate() {
        ensure!(
            r.ordinal == i as u64 && r.byte_offset == raw.len() as u64,
            "raw_order_offset"
        );
        raw.push_str(&r.raw_line);
    }
    ensure!(
        sha256(raw.as_bytes()) == c.raw_sha256 && raw.len() as u64 == c.raw_bytes,
        "full_tsv_losslessness"
    );
    let mut controls = Vec::new();
    for (key, value) in [
        ("qualification", json!("engineering_verified")),
        ("model", json!("run")),
        ("engineering_adoption", json!("accepted")),
        ("language_loading", json!("verified")),
        ("unknown", json!([])),
        (
            "elapsed_millis",
            json!(prepared.report().elapsed_millis + 1),
        ),
        ("selected_row_sha256", json!("b".repeat(64))),
    ] {
        let mut changed = report.clone();
        changed[key] = value;
        let refused = match serde_json::from_value(changed) {
            Ok(dto) => registered_ocr6::verify_prepared_registered_ocr6(&prepared, &dto).is_err(),
            Err(_) => true,
        };
        ensure!(refused, "public_projection_accepted:{key}");
        controls.push(json!({"key":key,"refused":true,"scope":"samecall_public_projection_only"}));
    }
    registered_ocr6::verify_prepared_registered_ocr6(&prepared, prepared.report())?;
    let receipt = json!({"schema":"private.registered-page6.once-actual-consumer/1","report":report,
  "old5_general_page":"refused_before_process","admission_controls":admission_controls,"public_projection_controls":controls,
  "source_read_entrance":"current_samecall_borrowed_neutral_content_plus_provenance","raw_tsv_reconstruction":"exact_SHA_bytes_order_offsets",
  "actual_ocr_attempts":1,"accuracy_gold":"none","development_page_seen":true,"unseen":false,
  "model":"not_run","Project":"not_written","whole_qualification":"unknown"});
    let target = root
        .path()
        .join("var/cache/knowledge/ocr-page6")
        .join(&args[4])
        .join("actual-consumer.json");
    write_new(&target, &receipt)?;
    ensure!(
        Instant::now() < deadline,
        "caller_deadline_expired_after_final_receipt"
    );
    println!(
        "{}",
        json!({"actual_receipt":target,"rows":c.rows.len(),"raw_bytes":c.raw_bytes,"qualification":"source_observation_only","model":"not_run","Project":"not_written"})
    );
    Ok(())
}
