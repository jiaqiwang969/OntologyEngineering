//! Explicit two-page development consumer. No OCR, model, or Project effects.
//! Every expected input is frozen before candidate execution; JSON does not
//! supply producer authority. Only the original owned process factory does.
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use oe_knowledge::{
    page_content4::{parse_request, prepare_content4, verify_prepared_content4},
    poppler::PopplerTools,
};
use serde_json::{Value, json};
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 5,
        "usage: root request-json expected-json fresh-output-leaf"
    );
    let root = SkillRoot::new(&args[1])?;
    let input: Value =
        oe_core::json::strict_json(&read_regular_bytes(Path::new(&args[2]), 1024 * 1024)?)?;
    let expected: Value =
        oe_core::json::strict_json(&read_regular_bytes(Path::new(&args[3]), 1024 * 1024)?)?;
    let request = parse_request(&input)?;
    ensure!(
        (request.page_id == "local_pdf:interchangeability:page:188"
            && request.physical_page == 188)
            || (request.page_id == "local_pdf:vol2:page:24" && request.physical_page == 24),
        "development_two_page_domain"
    );
    let tools = PopplerTools {
        pdfinfo: "/opt/homebrew/Cellar/poppler/26.02.0_1/bin/pdfinfo".into(),
        pdftotext: "/nonexistent/not-used-by-stage2".into(),
        pdftoppm: request.renderer.path.clone().into(),
    };
    let prepared = prepare_content4(&root, &request, &tools, &args[4])?;
    verify_prepared_content4(&prepared, prepared.report())?;
    let observed = serde_json::to_value(prepared.report())?;
    ensure!(
        observed["page_id"] == expected["page_id"]
            && observed["native_text_chars"] == expected["native_text_chars"]
            && observed["navigation_only"] == expected["navigation_only"]
            && observed["text_sha256"] == expected["text_sha256"],
        "independent_original_native_gold_mismatch"
    );
    if let Some(gold) = expected.get("png_sha256") {
        ensure!(
            observed["render"]["png"]["sha256"] == *gold,
            "independent_original_png_gold_mismatch"
        );
        ensure!(
            sha256(prepared.pixels()) == expected["pixels_sha256"],
            "independent_original_pixels_gold_mismatch"
        );
    }
    // This positive replay consumes the newly generated DTO and opaque current
    // call. Public receipt mutations are tested without another render.
    let mut controls = Vec::new();
    for (name, key, value) in [
        (
            "native_projection",
            "navigation_only",
            json!(
                !observed["navigation_only"]
                    .as_bool()
                    .context("native_bool")?
            ),
        ),
        ("ocr_projection", "ocr", json!("run")),
        ("model_projection", "model_image_reading", json!("run")),
        (
            "adoption_projection",
            "engineering_adoption",
            json!("verified"),
        ),
        ("aggregate_claim", "aggregate_deadline", json!("bounded")),
        ("unknown_removal", "unknown", json!([])),
    ] {
        let mut changed = observed.clone();
        changed[key] = value;
        let rejected = match serde_json::from_value(changed) {
            Ok(report) => verify_prepared_content4(&prepared, &report).is_err(),
            Err(_) => true,
        };
        ensure!(rejected, "public_projection_accepted:{name}");
        controls.push(json!({"id":name,"rejected":true,"slice":"public_projection_only"}));
    }
    let receipt = json!({"schema":"private.stage2.actual-content-consumer/v1","report":observed,
        "pixels_sha256":sha256(prepared.pixels()),"pixels_bytes":prepared.pixels().len(),"public_projection_controls":controls,
        "source_before_after":"verified_by_current_owner","ocr":"not_run","model":"not_run","Project":"not_written",
        "scope":"selected two-page local content development;no whole skill or engineering qualification"});
    let target = root
        .path()
        .join("var/cache/knowledge/page-content4")
        .join(&args[4])
        .join("actual-consumer.json");
    write_new(&target, &receipt)?;
    println!(
        "{}",
        json!({"actual_receipt":target,"page_id":request.page_id,"availability":"decoded_image_available","ocr":"not_run","model":"not_run"})
    );
    Ok(())
}
