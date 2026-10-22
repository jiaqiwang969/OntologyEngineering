//! Existing registered source reader borrowed by the explicit typed consumer.
//! No source capability can be deserialized or recovered from audit JSON.
use crate::{
    registered_ocr6::{self, PreparedRegisteredOcrPage6},
    sources::Provider,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::KnowledgeTypedExtractionInput8 as Input;
use oe_core::{SkillRoot, fingerprint_file, no_symlinks, sha256};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path, time::Instant};
pub const INPUT_SCHEMA8: &str = "oev2.source-bound-typed-extraction-input/8";
pub fn parse_input8(value: &Value) -> Result<Input> {
    let expected = [
        "schema",
        "source_ref",
        "page_id",
        "document_sha256",
        "text_sha256",
        "raw_text_sha256",
        "physical_page",
        "page_count",
        "channel",
        "selected_text_sha256",
        "start_byte",
        "end_byte",
        "generation",
        "entities",
        "relation_types",
        "temporal",
        "max_retries",
        "maximum_request_bytes",
        "context_window_tokens",
        "reserved_output_tokens",
    ];
    ensure!(
        value
            .as_object()
            .context("typed_source8_input_object")?
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == expected.into_iter().collect(),
        "typed_source8_input_keys"
    );
    let input: Input = serde_json::from_value(value.clone())?;
    ensure!(
        input.validate().is_ok()
            && input.schema == INPUT_SCHEMA8
            && !input.source_ref.trim().is_empty(),
        "typed_source8_input_contract"
    );
    ensure!(
        input.physical_page > 0
            && input.physical_page <= input.page_count
            && input.start_byte < input.end_byte,
        "typed_source8_page_window"
    );
    Ok(input)
}
fn clock(deadline: Instant) -> Result<()> {
    ensure!(Instant::now() < deadline, "typed_source8_deadline_expired");
    Ok(())
}
fn current(root: &SkillRoot, input: &Input) -> Result<(Provider, Value)> {
    let provider = Provider::open(root, "local_pdf", None)?;
    let row = provider.page(&input.page_id)?;
    ensure!(
        row["provider"] == "local_pdf"
            && row["source_sha256"] == input.document_sha256
            && row["text_sha256"] == input.text_sha256
            && row["raw_text_sha256"] == input.raw_text_sha256
            && row["pdf_page"].as_u64() == Some(input.physical_page)
            && row["source_registration"]["sha256"] == input.document_sha256
            && row["source_registration"]["page_count"].as_u64() == Some(input.page_count),
        "typed_source8_current_row_binding"
    );
    let path = root.expand(Path::new(
        row["source_pdf"]
            .as_str()
            .context("typed_source8_document_path")?,
    ))?;
    ensure!(
        path.starts_with(root.path()),
        "typed_source8_foreign_document"
    );
    no_symlinks(&path)?;
    let document = fingerprint_file(&path, crate::citations::MAX_SOURCE_BYTES)?;
    ensure!(
        document.sha256 == input.document_sha256
            && Some(document.bytes) == row["source_bytes"].as_u64(),
        "typed_source8_document_changed"
    );
    let text = row["body"].as_str().context("typed_source8_native_body")?;
    ensure!(
        sha256(text) == input.text_sha256,
        "typed_source8_native_identity"
    );
    Ok((provider, row))
}
/// ```compile_fail
/// let _: oe_knowledge::typed_source8::PreparedExtractionSource8<'static> = serde_json::from_str("{}").unwrap();
/// ```
pub struct PreparedExtractionSource8<'a> {
    root: SkillRoot,
    input: Input,
    _provider: Provider,
    row: Value,
    ocr: Option<&'a PreparedRegisteredOcrPage6>,
    selected: String,
    text: String,
    deadline: Instant,
    report: Value,
}
impl PreparedExtractionSource8<'_> {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn input(&self) -> &Input {
        &self.input
    }
    pub fn verify_current8(&self) -> Result<()> {
        clock(self.deadline)?;
        let (_, row) = current(&self.root, &self.input)?;
        ensure!(row == self.row, "typed_source8_row_changed");
        let selected = selected_text8(&self.root, &self.input, &row, self.ocr)?;
        ensure!(
            selected == self.selected && sha256(&selected) == self.input.selected_text_sha256,
            "typed_source8_selected_content_changed"
        );
        clock(self.deadline)?;
        Ok(())
    }
}
fn selected_text8(
    root: &SkillRoot,
    input: &Input,
    row: &Value,
    ocr: Option<&PreparedRegisteredOcrPage6>,
) -> Result<String> {
    match input.channel.as_str() {
        "native" => {
            ensure!(ocr.is_none(), "typed_source8_native_ocr_mismatch");
            let text = row["body"].as_str().context("typed_source8_native_body")?;
            ensure!(!text.trim().is_empty(), "typed_source8_native_body_missing");
            Ok(text.into())
        }
        "ocr" => {
            let source = ocr.context("typed_source8_actual_opaque6_required")?;
            registered_ocr6::verify_in_root6(root, source)?;
            let read = registered_ocr6::read_source_content6(source)?;
            let r = &read.provenance().request;
            ensure!(
                r.page.page_id == input.page_id
                    && r.page.document_sha256 == input.document_sha256
                    && r.page.text_sha256 == input.text_sha256
                    && r.page.raw_text_sha256 == input.raw_text_sha256
                    && r.page.physical_page == input.physical_page
                    && r.page_count == input.page_count,
                "typed_source8_ocr_selection_mismatch"
            );
            // Complete observed rows/source/image/unknown are kept in actual
            // serialization order. This is never claimed as visual reading order.
            Ok(String::from_utf8(oe_core::json::ordered_json(
                &serde_json::to_value(read.content())?,
            )?)?)
        }
        _ => anyhow::bail!("typed_source8_channel_domain"),
    }
}
fn prepare8<'a>(
    root: &SkillRoot,
    input: &Input,
    ocr: Option<&'a PreparedRegisteredOcrPage6>,
    deadline: Instant,
) -> Result<PreparedExtractionSource8<'a>> {
    clock(deadline)?;
    parse_input8(&serde_json::to_value(input)?)?;
    let (provider, row) = current(root, input)?;
    clock(deadline)?;
    let selected = selected_text8(root, input, &row, ocr)?;
    ensure!(
        sha256(&selected) == input.selected_text_sha256,
        "typed_source8_selected_content_identity"
    );
    let start = usize::try_from(input.start_byte)?;
    let end = usize::try_from(input.end_byte)?;
    let text = selected
        .get(start..end)
        .context("typed_source8_utf8_byte_window")?
        .to_owned();
    ensure!(
        !text.trim().is_empty() && start < end,
        "typed_source8_empty_window"
    );
    let report = json!({"schema":"oev2.typed-extraction-source-observation/8","source_ref":input.source_ref,"page_id":input.page_id,
        "channel":input.channel,"source_row":row,"source_row_sha256":sha256(oe_core::json::ordered_json(&row)?),
        "selected_content_sha256":input.selected_text_sha256,"selected_content_bytes":selected.len(),
        "window":{"start_byte":start,"end_byte":end,"bytes":text.len(),"sha256":sha256(&text),"omitted_prefix_bytes":start,"omitted_suffix_bytes":selected.len()-end},
        "complete_selected_window":true,"whole_page_read_by_consumer":start==0&&end==selected.len(),
        "native_extraction_process":"historical_index_process_unknown","OCR":if ocr.is_some(){"actual_borrowed6_observation"}else{"not_run"},
        "transcription_accuracy":"unknown","reading_order":if ocr.is_some(){"raw_complete_row_serialization_not_visual_order"}else{"indexed_native_text_order"},
        "entity_offsets_causal_temporal_and_engineering_applicability":"unknown","authority":"current_process_read_and_fresh_verification_only"});
    let source = PreparedExtractionSource8 {
        root: root.clone(),
        input: input.clone(),
        _provider: provider,
        row,
        ocr,
        selected,
        text,
        deadline,
        report,
    };
    source.verify_current8()?;
    Ok(source)
}
pub fn prepare_native8(
    root: &SkillRoot,
    input: &Input,
    deadline: Instant,
) -> Result<PreparedExtractionSource8<'static>> {
    ensure!(
        input.channel == "native",
        "typed_source8_actual_opaque6_required"
    );
    prepare8(root, input, None, deadline)
}
pub fn prepare_borrowed_ocr8<'a>(
    root: &SkillRoot,
    input: &Input,
    source: &'a PreparedRegisteredOcrPage6,
    deadline: Instant,
) -> Result<PreparedExtractionSource8<'a>> {
    ensure!(input.channel == "ocr", "typed_source8_ocr_channel_required");
    prepare8(root, input, Some(source), deadline)
}
