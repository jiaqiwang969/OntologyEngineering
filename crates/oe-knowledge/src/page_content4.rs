//! Registered source and actual local render content, with no OCR/model/adoption.
use crate::{index, poppler, sources::Provider};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    KnowledgePageAsset4 as Asset, KnowledgePageContentRequest4 as Request, KnowledgePageProcess4,
    KnowledgePageRender4, KnowledgePreparedPageContent4,
};
use oe_core::{SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256};
use semantica_contracts::{SemanticContentAsset3, SemanticPageImageBinding3};
use semantica_runtime::{
    books::SourceProvider,
    page_content3::{ContentLimits, DecodedPageImage, read_png_source, verify_png_content},
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

const REQUEST_SCHEMA: &str = "oev2.knowledge-page-content-request/4";
const PNG_ID: &str = "selected/render-stdout.png";
const UNKNOWN: &[&str] = &[
    "aggregate_source_read_render_deadline",
    "provider_hard_allocation_bound",
    "historical_native_extraction_process",
    "renderer_semantic_and_runtime_loading_closure",
    "all_descendant_cleanup_closure",
    "ocr_tool_language_execution_and_transcription",
    "model_image_capacity_and_execution",
    "engineering_applicability_numeric_verification_and_adoption",
    "all_original_formats_interfaces_UI_SDK_server_platforms",
];
const RENDER_UNKNOWN: &[&str] = &[
    "aggregate_source_render_deadline",
    "renderer_semantics_and_dynamic_loading",
    "all_descendant_cleanup_closure",
    "original_historical_sidecar_process",
];
fn generated(r: std::result::Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|_| anyhow::anyhow!("page_content4_contract"))
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|x| (*x).into()).collect()
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key].as_str().context("page_content4_row_string")
}
fn number(v: &Value, key: &str) -> Result<u64> {
    v[key].as_u64().context("page_content4_row_integer")
}
fn truth(v: &Value) -> Result<bool> {
    if let Some(b) = v.as_bool() {
        Ok(b)
    } else {
        match v.as_u64() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => anyhow::bail!("page_content4_row_boolean"),
        }
    }
}
fn asset(root: &SkillRoot, path: &Path, limit: u64) -> Result<Asset> {
    no_symlinks(path)?;
    let actual = fingerprint_file(path, limit)?;
    Ok(Asset {
        path: oe_core::portable_source_path(root.path(), path)?,
        sha256: actual.sha256,
        bytes: actual.bytes,
    })
}
fn verify_asset(root: &SkillRoot, a: &Asset, limit: u64) -> Result<PathBuf> {
    generated(a.validate())?;
    let path = root.expand(Path::new(&a.path))?;
    no_symlinks(&path)?;
    let actual = fingerprint_file(&path, limit)?;
    ensure!(
        actual.sha256 == a.sha256 && actual.bytes == a.bytes,
        "page_content4_asset_changed"
    );
    Ok(path)
}
/// Strict parsing prevents caller producer/view/status extras being ignored.
pub fn parse_request(value: &Value) -> Result<Request> {
    let expected: BTreeSet<_> = [
        "schema",
        "registration",
        "index",
        "page_id",
        "document_sha256",
        "text_sha256",
        "raw_text_sha256",
        "physical_page",
        "dpi",
        "renderer",
    ]
    .into_iter()
    .collect();
    ensure!(
        value
            .as_object()
            .context("page_content4_request_object")?
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == expected,
        "page_content4_request_keys"
    );
    for k in ["registration", "index", "renderer"] {
        ensure!(
            value[k]
                .as_object()
                .context("page_content4_asset_object")?
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == ["path", "sha256", "bytes"].into_iter().collect(),
            "page_content4_asset_keys"
        );
    }
    let request: Request = serde_json::from_value(value.clone())?;
    generated(request.validate())?;
    Ok(request)
}
struct Published {
    identity: Asset,
    original: Vec<u8>,
}
fn publish(root: &SkillRoot, dir: &Path, name: &str, bytes: &[u8]) -> Result<Published> {
    let path = dir.join(name);
    oe_core::write_new_bytes(&path, bytes)?;
    let identity = asset(root, &path, 64 * 1024 * 1024)?;
    ensure!(
        identity.sha256 == sha256(bytes) && identity.bytes == bytes.len() as u64,
        "page_content4_publish_identity"
    );
    Ok(Published {
        identity,
        original: bytes.to_vec(),
    })
}
impl Published {
    fn read(&self, root: &SkillRoot) -> Result<Vec<u8>> {
        let path = verify_asset(root, &self.identity, 64 * 1024 * 1024)?;
        let bytes = read_regular_bytes(&path, 64 * 1024 * 1024)?;
        ensure!(
            bytes == self.original,
            "page_content4_published_original_changed"
        );
        Ok(bytes)
    }
}
/// A borrowing adapter for the one producer-selected asset, not a new source owner.
struct SelectedImage<'a> {
    root: &'a SkillRoot,
    image: &'a Published,
}
impl SourceProvider for SelectedImage<'_> {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        ensure!(id == PNG_ID, "page_content4_unselected_asset");
        self.image.read(self.root)
    }
}

/// Non-deserializable same-call capability. Fields and constructors remain private.
pub struct PreparedPageContent4 {
    root: SkillRoot,
    provider: Provider,
    selected_row: Value,
    observed: poppler::ObservedRender4,
    streams: Vec<Published>,
    decoded: DecodedPageImage,
    report: KnowledgePreparedPageContent4,
}
impl fmt::Debug for PreparedPageContent4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedPageContent4")
            .field("scope", &"selected_source_content_only")
            .finish_non_exhaustive()
    }
}
impl PreparedPageContent4 {
    pub fn report(&self) -> &KnowledgePreparedPageContent4 {
        &self.report
    }
    pub fn pixels(&self) -> &[u8] {
        self.decoded.pixels()
    }
    pub(crate) fn decoded_image(&self) -> &DecodedPageImage {
        &self.decoded
    }
}
fn row_binding(row: &Value, request: &Request) -> Result<()> {
    ensure!(
        row["provider"] == "local_pdf"
            && row["id"] == request.page_id
            && row["source_sha256"] == request.document_sha256
            && row["text_sha256"] == request.text_sha256
            && row["raw_text_sha256"] == request.raw_text_sha256
            && row["pdf_page"].as_u64() == Some(request.physical_page),
        "page_content4_selected_row_binding"
    );
    let body = text(row, "body")?;
    ensure!(
        sha256(body) == request.text_sha256
            && number(row, "text_chars")? == body.chars().count() as u64,
        "page_content4_native_identity"
    );
    ensure!(
        truth(&row["native_text_missing"])? == body.trim().is_empty()
            && truth(&row["navigation_only"])? == body.trim().is_empty(),
        "page_content4_native_channel"
    );
    let locator = &row["source_locator"];
    ensure!(
        locator["schema"] == "ontology-engineering.source-page/v1"
            && locator["kind"] == "local_pdf"
            && locator["document_sha256"] == request.document_sha256
            && locator["physical_page"].as_u64() == Some(request.physical_page)
            && locator["printed_page"].is_null()
            && locator["printed_page_status"] == "unverified",
        "page_content4_locator"
    );
    Ok(())
}
fn binding(observed: &poppler::ObservedRender4) -> SemanticPageImageBinding3 {
    SemanticPageImageBinding3 {
        schema: "semantica.page-image-binding/3".into(),
        document_sha256: observed.document().sha256.clone(),
        document_bytes: observed.document().bytes,
        physical_page: observed.page(),
        page_count: observed.page_count(),
        dpi: observed.dpi() as u64,
        image_asset: SemanticContentAsset3 {
            logical_id: PNG_ID.into(),
            sha256: sha256(observed.png()),
            bytes: observed.png().len() as u64,
        },
    }
}
/// The output directory is new and exact. Its prefix never authorizes source reads.
pub fn prepare_content4(
    root: &SkillRoot,
    request: &Request,
    tools: &poppler::PopplerTools,
    output_leaf: &str,
) -> Result<PreparedPageContent4> {
    generated(request.validate())?;
    ensure!(
        request.schema == REQUEST_SCHEMA && request.dpi == 144 && request.physical_page > 0,
        "page_content4_request_domain"
    );
    ensure!(
        request.registration.path == "sources/.indexes/local-pdf/registration.json"
            && request.index.path == "sources/.indexes/local-pdf/catalog.sqlite3",
        "page_content4_exact_registered_paths"
    );
    verify_asset(root, &request.registration, index::MAX_VALUE_BYTES as u64)?;
    verify_asset(root, &request.index, index::MAX_INDEX_BYTES)?;
    let provider = Provider::open(root, "local_pdf", None)?;
    let row = provider.page(&request.page_id)?;
    row_binding(&row, request)?;
    ensure!(
        row["index_identity"]["sha256"] == request.index.sha256
            && row["index_identity"]["bytes"].as_u64() == Some(request.index.bytes),
        "page_content4_selected_index_identity"
    );
    let source = root.expand(Path::new(text(&row, "source_pdf")?))?;
    no_symlinks(&source)?;
    let document = asset(root, &source, crate::citations::MAX_SOURCE_BYTES)?;
    ensure!(
        document.sha256 == request.document_sha256
            && document.bytes == number(&row, "source_bytes")?,
        "page_content4_selected_document_identity"
    );
    ensure!(
        output_leaf.len() > 0
            && output_leaf.len() <= 96
            && output_leaf
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "page_content4_output_leaf"
    );
    let parent = root.path().join("var/cache/knowledge/page-content4");
    no_symlinks(&parent)?;
    std::fs::create_dir_all(&parent)?;
    no_symlinks(&parent)?;
    let dir = parent.join(output_leaf);
    std::fs::create_dir(&dir)?;
    no_symlinks(&dir)?;
    let observed = poppler::render_observed4(
        root.path(),
        &source,
        request.physical_page,
        number(&row["source_locator"], "page_count")?,
        144,
        tools,
        Path::new(&request.renderer.path),
        &request.renderer.sha256,
        request.renderer.bytes,
    )?;
    let mut streams = Vec::new();
    let mut processes = Vec::new();
    for (ordinal, (role, tool, captured)) in observed.processes().iter().enumerate() {
        let extension = if role == "page_render" { "png" } else { "bin" };
        let stdout = publish(
            root,
            &dir,
            &format!("{ordinal}-{role}-stdout.{extension}"),
            &captured.stdout,
        )?;
        let stderr = publish(
            root,
            &dir,
            &format!("{ordinal}-{role}-stderr.bin"),
            &captured.stderr,
        )?;
        let tool_asset = Asset {
            path: tool
                .path
                .to_str()
                .context("page_content4_tool_path")?
                .into(),
            sha256: tool.sha256.clone(),
            bytes: tool.bytes,
        };
        processes.push(KnowledgePageProcess4 {
            role: role.clone(),
            tool: tool_asset,
            argv: captured.argv.clone(),
            cwd: captured.cwd.to_str().context("page_content4_cwd")?.into(),
            lc_all: "C".into(),
            stdin: "null".into(),
            exit_code: i64::from(captured.exit_code.context("page_content4_exit_missing")?),
            timeout_millis: captured.timeout_millis,
            stdout_limit_bytes: captured.stdout_limit_bytes,
            stderr_limit_bytes: 1024 * 1024,
            elapsed_millis: captured.elapsed_millis,
            stdout: stdout.identity.clone(),
            stderr: stderr.identity.clone(),
            group_kill_return: i64::from(captured.group_kill_return),
            child_wait_completed: captured.child_wait_completed,
            child_wait_exit_code: captured.child_wait_exit_code.map(i64::from),
            scratch_removed: captured.scratch_removed,
        });
        streams.push(stdout);
        streams.push(stderr);
    }
    let png_index = streams
        .len()
        .checked_sub(2)
        .context("page_content4_render_streams")?;
    let reader = SelectedImage {
        root,
        image: &streams[png_index],
    };
    let decoded = read_png_source(&reader, &binding(&observed), &ContentLimits::default())?;
    let render = KnowledgePageRender4 {
        schema: "oev2.knowledge-page-render-observation/4".into(),
        document: document.clone(),
        document_sha256_after: observed.document_after().sha256.clone(),
        document_bytes_after: observed.document_after().bytes,
        physical_page: observed.page(),
        page_count: observed.page_count(),
        dpi: observed.dpi() as u64,
        whole_page: true,
        renderer_version: observed.tool().version.clone(),
        processes,
        png: streams[png_index].identity.clone(),
        relationship: "current_controlled_process_output".into(),
        unknown: strings(RENDER_UNKNOWN),
    };
    let body = text(&row, "body")?.to_owned();
    let report = KnowledgePreparedPageContent4 {
        schema: "oev2.knowledge-prepared-page-content/4".into(),
        request: request.clone(),
        registration: request.registration.clone(),
        index: request.index.clone(),
        document,
        page_id: request.page_id.clone(),
        native_body: body,
        text_sha256: request.text_sha256.clone(),
        raw_text_sha256: request.raw_text_sha256.clone(),
        raw_text_chars: number(&row, "raw_text_chars")?,
        native_text_chars: number(&row, "text_chars")?,
        native_text_missing: truth(&row["native_text_missing"])?,
        navigation_only: truth(&row["navigation_only"])?,
        native_text_sparse: truth(&row["native_text_sparse"])?,
        native_origin: "registered_index_poppler_native_text".into(),
        native_derivation: "indexed_digest_only_no_new_native_extraction".into(),
        printed_page_status: "unverified".into(),
        render,
        image_content: serde_json::to_value(decoded.content())?,
        availability: "decoded_image_available".into(),
        ocr: "not_run".into(),
        model_image_reading: "not_run".into(),
        engineering_adoption: "not_run".into(),
        aggregate_deadline: "unknown_not_implemented".into(),
        qualification: "selected_source_content_only".into(),
        unknown: strings(UNKNOWN),
    };
    generated(report.validate())?;
    let prepared = PreparedPageContent4 {
        root: root.clone(),
        provider,
        selected_row: row,
        observed,
        streams,
        decoded,
        report,
    };
    verify_prepared_content4(&prepared, &prepared.report)?;
    Ok(prepared)
}
/// Replays actual source and original same-call streams; JSON cannot recreate authority.
pub fn verify_prepared_content4(
    prepared: &PreparedPageContent4,
    candidate: &KnowledgePreparedPageContent4,
) -> Result<()> {
    generated(candidate.validate())?;
    ensure!(
        candidate == &prepared.report,
        "page_content4_public_projection_changed"
    );
    let root = &prepared.root;
    let request = &prepared.report.request;
    verify_asset(root, &request.registration, index::MAX_VALUE_BYTES as u64)?;
    verify_asset(root, &request.index, index::MAX_INDEX_BYTES)?;
    let current = prepared.provider.page(&request.page_id)?;
    ensure!(
        current == prepared.selected_row,
        "page_content4_selected_row_changed"
    );
    row_binding(&current, request)?;
    let actual = asset(
        root,
        prepared.observed.document_path(),
        crate::citations::MAX_SOURCE_BYTES,
    )?;
    ensure!(
        actual == prepared.report.document
            && actual.sha256 == prepared.observed.document().sha256
            && actual.bytes == prepared.observed.document().bytes,
        "page_content4_original_changed"
    );
    for (i, (_, tool, captured)) in prepared.observed.processes().iter().enumerate() {
        let actual = fingerprint_file(&tool.path, 128 * 1024 * 1024)?;
        ensure!(
            actual.sha256 == tool.sha256 && actual.bytes == tool.bytes,
            "poppler_tool_changed"
        );
        ensure!(
            captured.exit_code == Some(0),
            "page_content4_process_status"
        );
        ensure!(
            prepared.streams[2 * i].read(root)? == captured.stdout
                && prepared.streams[2 * i + 1].read(root)? == captured.stderr,
            "page_content4_same_call_raw_streams"
        );
    }
    let index = prepared.streams.len() - 2;
    let source = SelectedImage {
        root,
        image: &prepared.streams[index],
    };
    verify_png_content(
        &source,
        &binding(&prepared.observed),
        prepared.decoded.content(),
        &ContentLimits::default(),
    )?;
    ensure!(
        serde_json::to_value(prepared.decoded.content())? == prepared.report.image_content,
        "page_content4_image_projection"
    );
    Ok(())
}
