//! General registered physical-page OCR content in the existing knowledge owner.
//! Version6 is explicit. Version5 keeps its development188 admission unchanged.
use crate::{
    index,
    ocr_page5::{self, ActualOcrCall, OcrCallDomain},
    page_content4,
    poppler::PopplerTools,
    sources::Provider,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    KnowledgePreparedRegisteredOcrPage6 as Report, KnowledgeRegisteredOcrRequest6 as Request,
};
use oe_core::{SkillRoot, fingerprint_file, no_symlinks};
use semantica_contracts::SemanticOcrTsvSourceContent5;
use serde_json::Value;
use std::{collections::BTreeSet, fmt, path::Path, time::Instant};
const REQUEST_SCHEMA: &str = "oev2.knowledge-registered-page-ocr-request/6";
const UNKNOWN: &[&str] = &[
    "transcription_accuracy_and_engineering_numerical_truth",
    "actual_selected_languages_loaded",
    "tool_compiler_runtime_OS_cache_dynamic_loading_closure",
    "sync_Provider_hash_PNG_TSV_allocation_cleanup_hard_preemption",
    "old_stage4_render_hard_aggregate_deadline",
    "all_possible_descendant_cleanup",
    "reading_order_and_visual_interpretation",
    "model_capacity_and_execution",
    "engineering_applicability_and_adoption",
    "all_registered_books_readability",
    "all_original_13494_19modules_19protocols_22owners_formats_UI_SDK_server_platforms",
];
fn generated(r: std::result::Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|_| anyhow::anyhow!("registered_ocr6_contract"))
}
fn before_deadline(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "registered_ocr6_deadline_expired"
    );
    Ok(())
}
/// Public input selects a registered row, never a PDF path, producer receipt,
/// model permission, Project state or new resource budget.
pub fn parse_request6(value: &Value) -> Result<Request> {
    ensure!(
        value
            .as_object()
            .context("registered_ocr6_request_object")?
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == ["schema", "document_id", "page_count", "page", "ocr"]
                .into_iter()
                .collect(),
        "registered_ocr6_request_keys"
    );
    page_content4::parse_request(&value["page"])?;
    ocr_page5::parse_request5(&value["ocr"])?;
    let r: Request = serde_json::from_value(value.clone())?;
    validate_request6(&r)?;
    Ok(r)
}
pub fn validate_request6(r: &Request) -> Result<()> {
    generated(r.validate())?;
    generated(r.page.validate())?;
    ocr_page5::validate_request5(&r.ocr)?;
    ensure!(
        r.schema == REQUEST_SCHEMA
            && !r.document_id.is_empty()
            && r.document_id.len() <= 80
            && r.document_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "registered_ocr6_document_domain"
    );
    ensure!(
        r.page.schema == "oev2.knowledge-page-content-request/4"
            && r.page.dpi == 144
            && r.page.physical_page > 0
            && r.page_count > 0
            && r.page.physical_page <= r.page_count
            && r.page.page_id
                == format!("local_pdf:{}:page:{}", r.document_id, r.page.physical_page),
        "registered_ocr6_page_domain"
    );
    ensure!(
        r.page.registration.path == "sources/.indexes/local-pdf/registration.json"
            && r.page.index.path == "sources/.indexes/local-pdf/catalog.sqlite3",
        "registered_ocr6_registered_paths"
    );
    Ok(())
}
fn row_binding(row: &Value, r: &Request) -> Result<()> {
    let source = &row["source_registration"];
    let locator = &row["source_locator"];
    ensure!(
        row["provider"] == "local_pdf"
            && row["id"] == r.page.page_id
            && row["catalog"] == format!("local_pdf:{}", r.document_id)
            && row["source_sha256"] == r.page.document_sha256
            && row["text_sha256"] == r.page.text_sha256
            && row["raw_text_sha256"] == r.page.raw_text_sha256
            && row["pdf_page"].as_u64() == Some(r.page.physical_page),
        "registered_ocr6_selected_row"
    );
    ensure!(
        source["id"] == r.document_id
            && source["sha256"] == r.page.document_sha256
            && source["bytes"] == row["source_bytes"]
            && source["page_count"].as_u64() == Some(r.page_count)
            && locator["document_id"] == r.document_id
            && locator["document_sha256"] == r.page.document_sha256
            && locator["physical_page"].as_u64() == Some(r.page.physical_page)
            && locator["page_count"].as_u64() == Some(r.page_count),
        "registered_ocr6_registered_document"
    );
    ensure!(
        row["index_identity"]["sha256"] == r.page.index.sha256
            && row["index_identity"]["bytes"].as_u64() == Some(r.page.index.bytes),
        "registered_ocr6_selected_index"
    );
    Ok(())
}
fn current_row(root: &SkillRoot, r: &Request) -> Result<(Provider, Value)> {
    validate_request6(r)?;
    for (a, cap) in [
        (&r.page.registration, index::MAX_VALUE_BYTES as u64),
        (&r.page.index, index::MAX_INDEX_BYTES),
    ] {
        let p = root.expand(Path::new(&a.path))?;
        no_symlinks(&p)?;
        let f = fingerprint_file(&p, cap)?;
        ensure!(
            f.sha256 == a.sha256 && f.bytes == a.bytes,
            "registered_ocr6_source_binding_changed"
        );
    }
    let provider = Provider::open(root, "local_pdf", None)?;
    let row = provider.page(&r.page.page_id)?;
    row_binding(&row, r)?;
    // The path comes only from the same already-validated registration/row.
    let original = root.expand(Path::new(
        row["source_pdf"]
            .as_str()
            .context("registered_ocr6_source_path")?,
    ))?;
    no_symlinks(&original)?;
    let f = fingerprint_file(&original, crate::citations::MAX_SOURCE_BYTES)?;
    ensure!(
        f.sha256 == r.page.document_sha256 && Some(f.bytes) == row["source_bytes"].as_u64(),
        "registered_ocr6_original_pdf_changed"
    );
    Ok((provider, row))
}
/// Read-only admission audit. Success creates no current-process capability.
pub fn check_registered_request6(root: &SkillRoot, r: &Request) -> Result<()> {
    current_row(root, r)?;
    Ok(())
}
/// Only a real factory can create this current-call capability.
/// ```compile_fail
/// let _: oe_knowledge::registered_ocr6::PreparedRegisteredOcrPage6 = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// let _: oe_knowledge::registered_ocr6::PreparedRegisteredOcrPage6 = Default::default();
/// ```
/// ```compile_fail
/// fn clone(p: &oe_knowledge::registered_ocr6::PreparedRegisteredOcrPage6) {
///     let _: oe_knowledge::registered_ocr6::PreparedRegisteredOcrPage6 = p.clone();
/// }
/// ```
pub struct PreparedRegisteredOcrPage6 {
    root: SkillRoot,
    provider: Provider,
    selected_row: Value,
    call: ActualOcrCall,
    report: Report,
    deadline: Instant,
}
impl fmt::Debug for PreparedRegisteredOcrPage6 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedRegisteredOcrPage6")
            .field("scope", &"actual_registered_page_source_observation_only")
            .finish_non_exhaustive()
    }
}
impl PreparedRegisteredOcrPage6 {
    pub fn report(&self) -> &Report {
        &self.report
    }
}
/// Borrow the SAME absolute caller clock created before input/Provider/render.
/// The original renderer remains /4 at144dpi. Post-return expiry refuses OCR;
/// synchronous CPU/IO and hard render preemption remain explicit unknown.
pub fn prepare_registered_page_ocr6(
    root: &SkillRoot,
    request: &Request,
    tools: &PopplerTools,
    deadline: Instant,
    output_leaf: &str,
) -> Result<PreparedRegisteredOcrPage6> {
    let started = Instant::now();
    before_deadline(deadline)?;
    let millis = |d: std::time::Duration| d.as_millis().min(u128::from(u64::MAX)) as u64;
    let remaining_entry = millis(
        deadline
            .checked_duration_since(started)
            .context("registered_ocr6_deadline_expired")?,
    );
    let (provider, selected_row) = current_row(root, request)?;
    before_deadline(deadline)?;
    let call = ocr_page5::prepare_ocr_call(
        root,
        &request.page,
        &request.ocr,
        tools,
        deadline,
        output_leaf,
        OcrCallDomain::Registered6,
    )?;
    before_deadline(deadline)?;
    let a = call.audit()?;
    let report = Report {
        schema: "oev2.knowledge-prepared-registered-ocr-page/6".into(),
        request: request.clone(),
        selected_row_sha256: oe_core::json::canonical_digest(&selected_row)?,
        page: a.page,
        selected_copies: a.selected_copies,
        process_observations: a.process_observations,
        process_artifacts: a.process_artifacts,
        raw_tsv: a.raw_tsv,
        content: a.content,
        elapsed_millis: millis(started.elapsed()),
        remaining_millis_at_entry: remaining_entry,
        remaining_millis_before_qualification: millis(
            deadline
                .checked_duration_since(Instant::now())
                .context("registered_ocr6_deadline_expired")?,
        ),
        deadline_validation: "same_borrowed_instant_admission_postreturn".into(),
        language_loading: "unknown_not_attested".into(),
        transcription_accuracy: "unverified".into(),
        qualification: "actual_registered_page_process_source_content_observation_only".into(),
        model: "not_run".into(),
        engineering_adoption: "not_run".into(),
        unknown: UNKNOWN.iter().map(|s| (*s).into()).collect(),
    };
    generated(report.validate())?;
    let prepared = PreparedRegisteredOcrPage6 {
        root: root.clone(),
        provider,
        selected_row,
        call,
        report,
        deadline,
    };
    verify_prepared_registered_ocr6(&prepared, prepared.report())?;
    prepared.call.publish(
        "prepared-registered-ocr-audit.json",
        &serde_json::to_value(prepared.report())?,
    )?;
    before_deadline(deadline)?;
    Ok(prepared)
}
/// No public report, historical run or parser result can reconstruct authority.
pub fn verify_prepared_registered_ocr6(
    p: &PreparedRegisteredOcrPage6,
    candidate: &Report,
) -> Result<()> {
    before_deadline(p.deadline)?;
    generated(candidate.validate())?;
    validate_request6(&candidate.request)?;
    ensure!(
        candidate == &p.report,
        "registered_ocr6_public_projection_changed"
    );
    let (_, row) = current_row(&p.root, &candidate.request)?;
    ensure!(
        row == p.selected_row
            && p.provider.page(&candidate.request.page.page_id)? == row
            && oe_core::json::canonical_digest(&row)? == candidate.selected_row_sha256,
        "registered_ocr6_current_registered_row_changed"
    );
    ensure!(
        candidate.page == *p.call.page() && candidate.page.request == candidate.request.page,
        "registered_ocr6_current_page_projection"
    );
    ocr_page5::verify_actual_ocr_call(&p.call)?;
    ensure!(
        serde_json::to_value(p.call.content())? == candidate.content,
        "registered_ocr6_current_content_projection"
    );
    before_deadline(p.deadline)?;
    Ok(())
}
/// An explicitly borrowed source read for later same-Project preparation. It
/// owns no Project/model/budget, has no wire constructor, and never dispatches.
pub struct RegisteredOcrSourceRead6<'a> {
    prepared: &'a PreparedRegisteredOcrPage6,
}
impl RegisteredOcrSourceRead6<'_> {
    pub fn content(&self) -> &SemanticOcrTsvSourceContent5 {
        self.prepared.call.content()
    }
    pub fn provenance(&self) -> &Report {
        self.prepared.report()
    }
}
pub fn read_source_content6(
    p: &PreparedRegisteredOcrPage6,
) -> Result<RegisteredOcrSourceRead6<'_>> {
    verify_prepared_registered_ocr6(p, p.report())?;
    Ok(RegisteredOcrSourceRead6 { prepared: p })
}

/// Verify this actual capability belongs to the caller's same source root.
/// Identical bytes in a different root do not transfer a factory capability.
pub fn verify_in_root6(root: &SkillRoot, p: &PreparedRegisteredOcrPage6) -> Result<()> {
    ensure!(
        root.path() == p.root.path(),
        "registered_ocr6_foreign_source_root"
    );
    verify_prepared_registered_ocr6(p, p.report())
}

/// Same-root borrowed content with one fresh, complete original verification.
/// This combines the original root guard and source read; it caches neither
/// bytes nor qualification and does not change either original public method.
pub fn read_source_content_in_root6<'a>(
    root: &SkillRoot,
    p: &'a PreparedRegisteredOcrPage6,
) -> Result<RegisteredOcrSourceRead6<'a>> {
    ensure!(
        root.path() == p.root.path(),
        "registered_ocr6_foreign_source_root"
    );
    read_source_content6(p)
}
