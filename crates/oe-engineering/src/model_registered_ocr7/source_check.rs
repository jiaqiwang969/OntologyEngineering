//! Finite observations of the original current check, without new authority.
use anyhow::{Result, ensure};
use oe_contracts::ModelRegisteredOcrSourceCheck7;
use serde_json::Value;
use std::{fmt, time::Duration};

const CAUSES: usize = 16;
const CODE_BYTES: usize = 128;
const SCOPE: &str = "original_current_check_observation_only";
// Exact messages only. Never export arbitrary Display text, paths, source
// contents, provider responses or credentials. Missing classification remains
// explicit and cannot turn an original failure into a successful check.
const CODES: &[&str] = &[
    "registered_ocr6_deadline_expired",
    "registered_ocr6_foreign_source_root",
    "registered_ocr6_source_binding_changed",
    "registered_ocr6_original_pdf_changed",
    "registered_ocr6_current_registered_row_changed",
    "registered_ocr6_public_projection_changed",
    "registered_ocr6_current_page_projection",
    "registered_ocr6_current_content_projection",
    "registered_ocr6_selected_row",
    "registered_ocr6_registered_document",
    "registered_ocr6_selected_index",
    "ocr_page5_deadline_expired:verify_entry",
    "ocr_page5_deadline_expired:verify_return",
    "ocr_page5_asset_changed",
    "ocr_page5_read_identity",
    "ocr_page5_raw_original_changed",
    "ocr_page5_source_copy_projection",
    "ocr_page5_same_call_process_output",
    "ocr_page5_process_not_complete",
    "ocr_page5_process_stream_identity",
    "ocr_page5_borrowed_process_deadline",
    "page_content4_asset_changed",
    "page_content4_published_original_changed",
    "page_content4_selected_row_binding",
    "page_content4_native_identity",
    "page_content4_native_channel",
    "model_registered_ocr7_source_changed",
    "model_registered_ocr7_outside_root",
    "model_registered_ocr7_prepared_sources_changed",
    "model_registered_ocr7_capability_selection_mismatch",
    "model_registered_ocr7_expected_source_mismatch",
    "model_registered_ocr7_native_source_mismatch",
    "model_registered_ocr7_complete_content_mismatch",
    "model_assistance_source_changed",
    "model_assistance_source_unavailable",
    "model_page_index_changed",
    "model_page_selection_mismatch",
    "model_page_native_text_mismatch",
    "model_page_source_changed_during_prepare",
    "model_page_prepared_sources_changed",
    "tsv_source5_replay_changed",
];

#[derive(Default)]
struct BoundedCode {
    text: String,
    overflowed: bool,
}
impl fmt::Write for BoundedCode {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() > CODE_BYTES.saturating_sub(self.text.len()) {
            self.overflowed = true;
            return Err(fmt::Error);
        }
        self.text.push_str(s);
        Ok(())
    }
}

fn observation(
    stage: &str,
    elapsed: Duration,
    check: &Result<()>,
) -> Result<ModelRegisteredOcrSourceCheck7> {
    ensure!(
        ["admission", "before_dispatch", "terminal"].contains(&stage),
        "model_registered_ocr7_current_stage"
    );
    let mut codes = Vec::new();
    let mut unclassified = 0;
    let mut truncated = false;
    if let Err(error) = check {
        for (i, cause) in error.chain().take(CAUSES + 1).enumerate() {
            if i == CAUSES {
                truncated = true;
                break;
            }
            let mut display = BoundedCode::default();
            let formatted = fmt::write(&mut display, format_args!("{cause}"));
            if formatted.is_ok() && !display.overflowed && CODES.contains(&display.text.as_str()) {
                if !codes.contains(&display.text) {
                    codes.push(display.text);
                }
            } else {
                unclassified += 1;
            }
        }
    }
    let x = ModelRegisteredOcrSourceCheck7 {
        schema: "oev2.model-registered-ocr-source-check/7".into(),
        stage: stage.into(),
        status: if check.is_ok() { "current" } else { "refused" }.into(),
        error_codes: codes,
        unclassified_causes: unclassified,
        causes_truncated: truncated,
        elapsed_millis: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        scope: SCOPE.into(),
    };
    x.validate()
        .map_err(|_| anyhow::anyhow!("model_registered_ocr7_current_diagnostic_contract"))?;
    Ok(x)
}

pub(crate) fn record_current_check7(
    result: &mut Value,
    stage: &str,
    elapsed: Duration,
    check: &Result<()>,
) -> Result<()> {
    let x = observation(stage, elapsed, check)?;
    let field = &mut result["source_current_checks"];
    if field.is_null() {
        *field = Value::Array(Vec::new());
    }
    let checks = field
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("model_registered_ocr7_current_diagnostic_array"))?;
    ensure!(
        checks.len() < 3,
        "model_registered_ocr7_current_diagnostic_count"
    );
    checks.push(serde_json::to_value(x)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use serde_json::json;

    #[test]
    fn current_check7_retains_exact_error_codes_without_raw_details() {
        let check: Result<()> = Err(anyhow!("registered_ocr6_deadline_expired")
            .context("private path secret provider text"));
        let x = observation("before_dispatch", Duration::from_millis(17), &check).unwrap();
        assert_eq!(x.status, "refused");
        assert_eq!(x.error_codes, ["registered_ocr6_deadline_expired"]);
        assert_eq!(x.unclassified_causes, 1);
        assert!(!x.causes_truncated);
        assert_eq!(x.elapsed_millis, 17);
        let raw = serde_json::to_string(&x).unwrap();
        assert!(
            !raw.contains("private path")
                && !raw.contains("secret")
                && !raw.contains("provider text")
        );
        let spoof: Result<()> = Err(anyhow!("registered_ocr6_deadline_expired secret"));
        let x = observation("terminal", Duration::ZERO, &spoof).unwrap();
        assert!(x.error_codes.is_empty());
        assert_eq!(x.unclassified_causes, 1);
    }

    #[test]
    fn current_check7_bounds_unclassified_display_and_chain() {
        let mut error = anyhow!("x".repeat(10_000));
        for _ in 0..20 {
            error = error.context("unclassified");
        }
        let x = observation("admission", Duration::MAX, &Err(error)).unwrap();
        assert_eq!(x.status, "refused");
        assert!(x.error_codes.is_empty());
        assert_eq!(x.unclassified_causes, 16);
        assert!(x.causes_truncated);
        assert_eq!(x.elapsed_millis, u64::MAX);
        let long: Result<()> = Err(anyhow!("x".repeat(10_000)));
        let x = observation("terminal", Duration::ZERO, &long).unwrap();
        assert!(x.error_codes.is_empty());
        assert_eq!(x.unclassified_causes, 1);
        assert!(serde_json::to_vec(&x).unwrap().len() < 512);
    }

    #[test]
    fn current_check7_observes_original_result_without_changing_adoption() {
        let mut result = json!({"status":"stale","adopted":false,"advisory":{"retained":true},"client_dispatch_attempts":1});
        let original = result.clone();
        let failed: Result<()> = Err(anyhow!("ocr_page5_asset_changed"));
        record_current_check7(&mut result, "admission", Duration::ZERO, &Ok(())).unwrap();
        record_current_check7(&mut result, "before_dispatch", Duration::ZERO, &Ok(())).unwrap();
        record_current_check7(&mut result, "terminal", Duration::ZERO, &failed).unwrap();
        assert_eq!(result["source_current_checks"].as_array().unwrap().len(), 3);
        assert_eq!(
            result["source_current_checks"][2]["error_codes"],
            json!(["ocr_page5_asset_changed"])
        );
        assert_eq!(result["source_current_checks"][0]["error_codes"], json!([]));
        result
            .as_object_mut()
            .unwrap()
            .remove("source_current_checks");
        assert_eq!(result, original);
        assert!(failed.is_err());
        assert!(observation("invented_stage", Duration::ZERO, &Ok(())).is_err());
    }
}
