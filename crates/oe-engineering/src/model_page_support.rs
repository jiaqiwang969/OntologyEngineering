//! Explicit selected-page support: whole original PDF, index and actual native
//! page text have separate identities. Preparation dispatches no model; the
//! explicit run facade delegates only to the existing owning single dispatcher.
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    FailureRecord, ModelArtifactReference, ModelPageAssistanceInput2, ModelPageContent2,
    ModelPageExtractionIdentity2, ModelPageRunInput2, ModelPageRunPort, ModelPageRunReceipt2,
    ModelPageSupportPacket2, ModelPageSupportPort, ModelPageSupportRequest2,
};
use oe_core::{SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256};
use oe_kernel::project::Project;
use oe_knowledge::{index, poppler, registration, sources::Provider};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const REQUEST_SCHEMA: &str = "oev2.model-page-support-request/2";
pub const PACKET_SCHEMA: &str = "oev2.model-page-support-packet/2";
pub const ASSISTANCE_SCHEMA: &str = "oev2.model-page-assistance-input/2";
pub const NATIVE_PAGE_SCOPE: &str = "Selected page support contains native extracted text only. Empty native text and navigation_only describe the extraction supply, not a blank physical page or absence of engineering content. OCR has not run. Rendered page images are not supplied in this request; never claim to have read their content. Whole document identity, selected text and engineering applicability are separate.";
const OLD_ARTIFACT_LIMIT: usize = 16 * 1024 * 1024;

fn generated(r: std::result::Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|_| anyhow::anyhow!("model_page_contract_invalid"))
}
fn read_bound(root: &SkillRoot, r: &ModelArtifactReference, limit: usize) -> Result<Vec<u8>> {
    generated(r.validate())?;
    let path = root.expand(Path::new(&r.path))?;
    no_symlinks(&path)?;
    let raw = read_regular_bytes(&path, limit)?;
    ensure!(sha256(&raw) == r.sha256, "model_page_artifact_changed");
    Ok(raw)
}
fn identity(root: &SkillRoot, path: &Path) -> Result<ModelArtifactReference> {
    let actual = fingerprint_file(path, oe_knowledge::citations::MAX_SOURCE_BYTES)?;
    Ok(ModelArtifactReference {
        path: oe_core::portable_source_path(root.path(), path)?,
        sha256: actual.sha256,
    })
}
fn keys(value: &Value, expected: &[&str]) -> Result<()> {
    let actual = value.as_object().context("model_page_object_required")?;
    ensure!(
        actual.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == expected.iter().copied().collect(),
        "model_page_unexpected_or_missing_fields"
    );
    Ok(())
}
fn reference_keys(value: &Value) -> Result<()> {
    keys(value, &["path", "sha256"])
}
pub fn parse_request(value: &Value) -> Result<ModelPageSupportRequest2> {
    keys(
        value,
        &["schema", "project", "registration", "index", "pages"],
    )?;
    for key in ["project", "registration", "index"] {
        reference_keys(&value[key])?;
    }
    for page in value["pages"]
        .as_array()
        .context("model_page_pages_required")?
    {
        keys(
            page,
            &[
                "source_ref",
                "page_id",
                "physical_page",
                "document_sha256",
                "text_sha256",
                "raw_text_sha256",
            ],
        )?;
    }
    let request: ModelPageSupportRequest2 = serde_json::from_value(value.clone())?;
    generated(request.validate())?;
    Ok(request)
}

/// Borrows the same Project. Registration/index bytes are pinned explicitly;
/// each native page is freshly re-extracted by the existing Poppler boundary.
pub fn prepare(
    root: &SkillRoot,
    project: &Project,
    request: &ModelPageSupportRequest2,
    tools: &poppler::PopplerTools,
) -> Result<ModelPageSupportPacket2> {
    generated(request.validate())?;
    oe_kernel::project::validate(project)?;
    ensure!(
        request.schema == REQUEST_SCHEMA && !request.pages.is_empty() && request.pages.len() <= 64,
        "model_page_request_invalid"
    );
    let project_raw = read_bound(root, &request.project, OLD_ARTIFACT_LIMIT)?;
    let actual_project: Project =
        serde_json::from_value(oe_core::json::strict_json(&project_raw)?)?;
    ensure!(
        serde_json::to_value(&actual_project)? == serde_json::to_value(project)?,
        "model_page_foreign_project"
    );
    let index_path = root
        .path()
        .join("sources/.indexes/local-pdf/catalog.sqlite3");
    let registration_path = root
        .path()
        .join("sources/.indexes/local-pdf/registration.json");
    ensure!(
        root.expand(Path::new(&request.index.path))? == index_path
            && root.expand(Path::new(&request.registration.path))? == registration_path,
        "model_page_registered_paths_required"
    );
    let registration_raw = read_bound(root, &request.registration, index::MAX_VALUE_BYTES)?;
    let expected_index = fingerprint_file(&index_path, index::MAX_INDEX_BYTES)?;
    ensure!(
        expected_index.sha256 == request.index.sha256,
        "model_page_index_changed"
    );
    let provider = Provider::open(root, "local_pdf", None)?;
    let mut documents = BTreeMap::<PathBuf, poppler::Extraction>::new();
    let mut seen = BTreeSet::new();
    let mut pages = Vec::new();
    for selection in &request.pages {
        generated(selection.validate())?;
        ensure!(
            seen.insert((selection.source_ref.clone(), selection.page_id.clone())),
            "model_page_duplicate_selection"
        );
        ensure!(
            project.evidence.get(&selection.source_ref) == Some(&selection.document_sha256),
            "model_page_source_ref_unbound"
        );
        let row = provider.page(&selection.page_id)?;
        ensure!(
            row["pdf_page"].as_u64() == Some(selection.physical_page)
                && row["source_sha256"] == selection.document_sha256
                && row["text_sha256"] == selection.text_sha256
                && row["raw_text_sha256"] == selection.raw_text_sha256,
            "model_page_selection_mismatch"
        );
        oe_knowledge::citations::verify_locator_with_tools(root.path(), &row, tools)?;
        let document = root.expand(Path::new(
            row["source_pdf"]
                .as_str()
                .context("model_page_pdf_missing")?,
        ))?;
        if !documents.contains_key(&document) {
            documents.insert(
                document.clone(),
                poppler::extract(
                    root.path(),
                    &document,
                    &selection.document_sha256,
                    tools,
                    &poppler::PopplerLimits::default(),
                )?,
            );
        }
        let extracted = &documents[&document];
        let raw = extracted
            .pages
            .get((selection.physical_page - 1) as usize)
            .context("model_page_physical_page_invalid")?;
        let body = registration::normalized_native_body(raw);
        let sparse = body.chars().filter(|c| !c.is_whitespace()).count() < 80;
        ensure!(
            sha256(raw) == selection.raw_text_sha256
                && sha256(&body) == selection.text_sha256
                && row["body"] == body
                && row["raw_text_chars"].as_u64() == Some(raw.chars().count() as u64)
                && row["native_text_sparse"].as_u64() == Some(u64::from(sparse)),
            "model_page_native_text_mismatch"
        );
        let observations = &extracted.observations;
        let tool = |key: &str| -> Result<ModelArtifactReference> {
            Ok(ModelArtifactReference {
                path: observations[key]["path"]
                    .as_str()
                    .context("model_page_tool_identity_missing")?
                    .into(),
                sha256: observations[key]["sha256"]
                    .as_str()
                    .context("model_page_tool_identity_missing")?
                    .into(),
            })
        };
        let page = ModelPageContent2 {
            source_ref: selection.source_ref.clone(),
            page_id: selection.page_id.clone(),
            physical_page: selection.physical_page,
            page_count: extracted.pages.len() as u64,
            document: identity(root, &document)?,
            document_bytes: row["source_bytes"]
                .as_u64()
                .context("model_page_pdf_bytes_missing")?,
            title: row["catalog_title"]
                .as_str()
                .context("model_page_title_missing")?
                .into(),
            edition: row["source_registration"]["edition"]
                .as_str()
                .context("model_page_edition_missing")?
                .into(),
            text: body.clone(),
            text_sha256: sha256(&body),
            raw_text_sha256: sha256(raw),
            raw_text_chars: raw.chars().count() as u64,
            native_text_sparse: sparse,
            content_status: if body.trim().is_empty() {
                "navigation_only"
            } else if sparse {
                "sparse_native_text"
            } else {
                "native_text"
            }
            .into(),
            headings: serde_json::from_value(row["headings"].clone())?,
            printed_page_status: "unverified".into(),
            ocr: "not_run".into(),
            applicability: "unknown".into(),
            extraction: ModelPageExtractionIdentity2 {
                pdfinfo: tool("pdfinfo")?,
                pdftotext: tool("pdftotext")?,
                pdfinfo_version: observations["pdfinfo"]["version"]
                    .as_str()
                    .context("model_page_tool_version_missing")?
                    .into(),
                pdftotext_version: observations["pdftotext"]["version"]
                    .as_str()
                    .context("model_page_tool_version_missing")?
                    .into(),
                pdfinfo_output_sha256: observations["pdfinfo_output_sha256"]
                    .as_str()
                    .context("model_page_tool_output_missing")?
                    .into(),
                text_output_sha256: observations["text_output_sha256"]
                    .as_str()
                    .context("model_page_tool_output_missing")?
                    .into(),
            },
        };
        generated(page.validate())?;
        pages.push(page);
    }
    for (selection, page) in request.pages.iter().zip(&pages) {
        let row = provider.page(&selection.page_id)?;
        ensure!(
            row["body"] == page.text && row["source_sha256"] == page.document.sha256,
            "model_page_source_changed_during_prepare"
        );
        ensure!(
            identity(root, &root.expand(Path::new(&page.document.path))?)? == page.document,
            "model_page_source_changed_during_prepare"
        );
    }
    ensure!(
        read_bound(root, &request.project, OLD_ARTIFACT_LIMIT)? == project_raw
            && read_bound(root, &request.registration, index::MAX_VALUE_BYTES)? == registration_raw
            && fingerprint_file(&index_path, index::MAX_INDEX_BYTES)? == expected_index,
        "model_page_source_changed_during_prepare"
    );
    let packet = ModelPageSupportPacket2 {
        schema: PACKET_SCHEMA.into(),
        project: request.project.clone(),
        project_id: project.project_id.clone(),
        project_revision: project.revision.clone(),
        registration: request.registration.clone(),
        index: request.index.clone(),
        pages,
    };
    generated(packet.validate())?;
    Ok(packet)
}

pub struct PageSupportExecutor<'a> {
    pub root: &'a SkillRoot,
    pub project: &'a Project,
    pub tools: &'a poppler::PopplerTools,
}
impl ModelPageSupportPort for PageSupportExecutor<'_> {
    fn prepare_pages(
        &self,
        request: &ModelPageSupportRequest2,
    ) -> std::result::Result<ModelPageSupportPacket2, FailureRecord> {
        prepare(self.root, self.project, request, self.tools)
            .map_err(|e| crate::artifacts::failure("model_page_support", e))
    }
}

/// An offline request view, never accepted by the old /1 dispatcher. The opaque
/// object retains originals so callers cannot authorize execution with JSON.
pub struct PageAssistedPrepared {
    base: crate::model_assistance::Prepared,
    request: Value,
    report: Value,
    page_request: ModelPageSupportRequest2,
    page_packet: ModelPageSupportPacket2,
    project: Project,
    input: ModelArtifactReference,
}
impl PageAssistedPrepared {
    pub fn admitted(&self) -> bool {
        self.report["status"] == "prepared"
    }
    pub fn service_profile(&self) -> &oe_contracts::ModelServiceProfile {
        &self.base.profile
    }
    pub(crate) fn execution_base(&self) -> &crate::model_assistance::Prepared {
        &self.base
    }
    pub(crate) fn execution_input_bytes(&self, root: &SkillRoot) -> Result<Vec<u8>> {
        read_bound(root, &self.input, OLD_ARTIFACT_LIMIT)
    }
    pub(crate) fn execution_input_reference(&self) -> &ModelArtifactReference {
        &self.input
    }
    pub fn request(&self) -> &Value {
        &self.request
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn pages(&self) -> &ModelPageSupportPacket2 {
        &self.page_packet
    }
    pub fn verify_sources(&self, root: &SkillRoot, tools: &poppler::PopplerTools) -> Result<()> {
        crate::model_assistance::verify_page_base_sources(&self.base)?;
        read_bound(root, &self.input, OLD_ARTIFACT_LIMIT)?;
        ensure!(
            prepare(root, &self.project, &self.page_request, tools)? == self.page_packet,
            "model_page_prepared_sources_changed"
        );
        Ok(())
    }
}
pub fn prepare_with_pages(
    root: &SkillRoot,
    input_path: &Path,
    profile_path: &Path,
    tools: &poppler::PopplerTools,
) -> Result<PageAssistedPrepared> {
    let input = identity(root, &root.expand(input_path)?)?;
    let value = oe_core::json::strict_json(&read_bound(root, &input, OLD_ARTIFACT_LIMIT)?)?;
    keys(&value, &["schema", "assistance", "page_support"])?;
    reference_keys(&value["assistance"])?;
    let page_request = parse_request(&value["page_support"])?;
    let typed: ModelPageAssistanceInput2 = serde_json::from_value(value)?;
    generated(typed.validate())?;
    ensure!(
        typed.schema == ASSISTANCE_SCHEMA,
        "model_page_assistance_schema_invalid"
    );
    let assistance_raw = read_bound(root, &typed.assistance, OLD_ARTIFACT_LIMIT)?;
    let assistance: oe_contracts::ModelAssistanceInput =
        serde_json::from_value(oe_core::json::strict_json(&assistance_raw)?)?;
    ensure!(
        assistance.project == page_request.project,
        "model_page_foreign_project"
    );
    let base =
        crate::model_assistance::prepare(root, Path::new(&typed.assistance.path), profile_path)?;
    let project: Project = serde_json::from_value(oe_core::json::strict_json(&read_bound(
        root,
        &assistance.project,
        OLD_ARTIFACT_LIMIT,
    )?)?)?;
    let page_packet = prepare(root, &project, &page_request, tools)?;
    let mut request = base.request().clone();
    let old_input = request["input"][0]["content"][0]["text"]
        .as_str()
        .context("model_page_base_text_missing")?;
    let old_bytes = old_input.len() as u64;
    let mut payload = oe_core::json::strict_json(old_input.as_bytes())?;
    payload["schema"] = json!("oev2.model-assistance-payload/3");
    payload["selected_page_support"] = serde_json::to_value(&page_packet)?;
    payload["selected_page_support_scope"] = json!(NATIVE_PAGE_SCOPE);
    let text = String::from_utf8(oe_core::json::ordered_json(&payload)?)?;
    let added = (text.len() as u64)
        .checked_sub(old_bytes)
        .context("model_page_budget_delta_invalid")?;
    request["input"][0]["content"][0]["text"] = json!(text);
    // Reuse the retained original budget, profile and output schema. The only
    // changed model input is this exact additive decoded page-support text.
    let mut report = base.value().clone();
    for key in [
        "input_text_bytes",
        "model_content_bytes",
        "conservative_input_tokens",
        "conservative_total_tokens",
    ] {
        let old = report["budget"][key]
            .as_u64()
            .context("model_page_base_budget_missing")?;
        report["budget"][key] = json!(
            old.checked_add(added)
                .context("model_page_budget_overflow")?
        );
    }
    let wire = oe_core::json::ordered_json(&request)?;
    report["budget"]["http_wire_bytes"] = json!(wire.len());
    report["request_bytes"] = json!(wire.len());
    report["request_sha256"] = json!(sha256(&wire));
    report["schema"] = json!("oev2.model-page-assistance-plan/2");
    report["context_schema"] = json!("oev2.model-assistance-payload/3");
    report["model_execution"] = json!("not_run");
    report["execution_port"] = json!("not_connected");
    report["selected_page_support"] = serde_json::to_value(&page_packet)?;
    report["engineering_adoption"] = json!("not_established");
    if wire.len() as u64 > base.profile.maximum_request_bytes {
        report["diagnostics"]
            .as_array_mut()
            .context("model_page_base_diagnostics_missing")?
            .push(json!({"code":"model_assistance_request_byte_budget_exceeded"}));
        report["status"] = json!("blocked");
    }
    if report["budget"]["conservative_total_tokens"]
        .as_u64()
        .context("model_page_budget_missing")?
        > base.profile.context_window_tokens
    {
        report["diagnostics"]
            .as_array_mut()
            .context("model_page_base_diagnostics_missing")?
            .push(json!({"code":"model_assistance_profile_context_budget_exceeded"}));
        report["status"] = json!("blocked");
    }
    read_bound(root, &typed.assistance, OLD_ARTIFACT_LIMIT)?;
    let prepared = PageAssistedPrepared {
        base,
        request,
        report,
        page_request,
        page_packet,
        project,
        input,
    };
    prepared.verify_sources(root, tools)?;
    Ok(prepared)
}

/// A facade over the single existing model dispatcher. The borrowed transport
/// is supplied by the existing host; plain input JSON does not request live I/O.
pub struct PageModelExecutor<'a> {
    pub root: &'a SkillRoot,
    pub tools: &'a poppler::PopplerTools,
    pub transport: Option<&'a dyn crate::model_assistance::transport::ModelTransport>,
}
impl ModelPageRunPort for PageModelExecutor<'_> {
    fn run_pages(
        &self,
        input: &ModelPageRunInput2,
    ) -> std::result::Result<ModelPageRunReceipt2, FailureRecord> {
        let execute = || -> Result<ModelPageRunReceipt2> {
            generated(input.validate())?;
            ensure!(
                input.schema == "oev2.model-page-run-input/2",
                "model_page_run_schema_invalid"
            );
            let output = self.root.private_path(Path::new(&input.output_dir))?;
            let result = crate::model_assistance::run_pages_bound(
                self.root,
                &input.input,
                &input.profile,
                &output,
                self.tools,
                self.transport,
            )?;
            let receipt = ModelPageRunReceipt2 {
                schema: "oev2.model-page-run-receipt/2".into(),
                status: result["status"]
                    .as_str()
                    .context("model_page_terminal_status_missing")?
                    .into(),
                model_execution: result["model_execution"]
                    .as_str()
                    .context("model_page_terminal_execution_missing")?
                    .into(),
                result: identity(self.root, &output.join("result.json"))?,
                engineering_adoption: "not_established".into(),
            };
            generated(receipt.validate())?;
            Ok(receipt)
        };
        execute().map_err(|e| crate::artifacts::failure("model_page_run", e))
    }
}
