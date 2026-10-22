//! One development188144dpi controlled attempt, not an accuracy/unseen oracle.
use anyhow::{Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use oe_knowledge::{ocr_page5, page_content4, poppler::PopplerTools};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 6,
        "usage:root page-request ocr-request original-image-gold fresh-leaf"
    );
    // Same absolute caller clock starts before input/Provider/render preparation.
    let deadline = Instant::now() + Duration::from_secs(120);
    let root = SkillRoot::new(&args[1])?;
    let read = |s: &str| -> Result<Value> {
        Ok(oe_core::json::strict_json(&read_regular_bytes(
            Path::new(s),
            1024 * 1024,
        )?)?)
    };
    let page = page_content4::parse_request(&read(&args[2])?)?;
    let request = ocr_page5::parse_request5(&read(&args[3])?)?;
    let gold = read(&args[4])?;
    let tools = PopplerTools {
        pdfinfo: "/opt/homebrew/Cellar/poppler/26.02.0_1/bin/pdfinfo".into(),
        pdftotext: "/not-used".into(),
        pdftoppm: page.renderer.path.clone().into(),
    };
    let prepared =
        ocr_page5::prepare_page_and_ocr5(&root, &page, &request, &tools, deadline, &args[5])?;
    ocr_page5::verify_prepared_ocr5(&prepared, prepared.report())?;
    let report = serde_json::to_value(prepared.report())?;
    ensure!(
        report["page"]["render"]["png"]["sha256"] == gold["png_sha256"]
            && report["page"]["image_content"]["decoded_sha256"] == gold["pixels_sha256"]
            && report["page"]["native_text_chars"] == gold["native_text_chars"]
            && report["page"]["navigation_only"] == gold["navigation_only"],
        "before_candidate_independent_original_page_gold"
    );
    let c = prepared.content();
    let mut raw = c.header_raw.clone();
    for (i, r) in c.rows.iter().enumerate() {
        ensure!(
            r.ordinal == i as u64 && r.byte_offset == raw.len() as u64,
            "actual_raw_order_offset"
        );
        raw.push_str(&r.raw_line);
    }
    ensure!(
        sha256(raw.as_bytes()) == c.raw_sha256 && raw.len() as u64 == c.raw_bytes,
        "actual_full_tsv_losslessness"
    );
    // Public projection mutations require the same actual capability; no extra
    // rendering or OCR is performed for these refusal observations.
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
    ] {
        let mut changed = report.clone();
        changed[key] = value;
        let rejected = match serde_json::from_value(changed) {
            Ok(dto) => ocr_page5::verify_prepared_ocr5(&prepared, &dto).is_err(),
            Err(_) => true,
        };
        ensure!(rejected, "actual_public_projection_accepted:{key}");
        controls.push(json!({"key":key,"rejected":true,"scope":"samecall_public_projection_only"}));
    }
    ocr_page5::verify_prepared_ocr5(&prepared, prepared.report())?;
    let receipt = json!({"schema":"private.stage3.once-actual-ocr-consumer/1","report":report,"public_projection_controls":controls,
  "raw_tsv_reconstruction":"exact_SHA_bytes_order_offsets","actual_ocr_attempts":1,"accuracy_gold":"none",
  "development_page_seen":true,"model":"not_run","Project":"not_written","whole_qualification":"unknown"});
    let target = root
        .path()
        .join("var/cache/knowledge/ocr-page5")
        .join(&args[5])
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
