//! Opaque current-call OCR provenance in the existing knowledge owner.
//! Native Tesseract is a selected external tool, not a Rust OCR algorithm.
use crate::{
    page_content4::{PreparedPageContent4, prepare_content4, verify_prepared_content4},
    poppler,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    KnowledgeOcrRequest5 as Request, KnowledgePageAsset4 as Asset,
    KnowledgePageContentRequest4 as PageRequest, KnowledgePreparedOcrPage5,
};
use oe_core::process::{CommandSpec, ProcessObservation, run_command_with_deadline};
use oe_core::{SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256, write_new};
use semantica_contracts::{SemanticContentAsset3, SemanticOcrTsvSourceContent5};
use semantica_runtime::{
    books::SourceProvider,
    page_content3::ContentLimits,
    page_content5::{read_tsv_source5, verify_tsv_source5},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const PNG_ID: &str = "selected/render-stdout.png";
const TSV_ID: &str = "selected/ocr-stdout.tsv";
const MAX_SOURCE: u64 = 128 * 1024 * 1024;
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
    "all_original_13494_19modules_19protocols_22owners_formats_UI_SDK_server_platforms",
];
fn generated(r: std::result::Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|_| anyhow::anyhow!("ocr_page5_contract"))
}
fn time_left(deadline: Instant, phase: &str) -> Result<Duration> {
    let left = deadline
        .checked_duration_since(Instant::now())
        .context(format!("ocr_page5_deadline_expired:{phase}"))?;
    ensure!(!left.is_zero(), "ocr_page5_deadline_expired:{phase}");
    Ok(left)
}
fn millis(d: Duration) -> u64 {
    d.as_millis().min(u128::from(u64::MAX)) as u64
}
fn verify_asset(root: &SkillRoot, a: &Asset) -> Result<PathBuf> {
    generated(a.validate())?;
    let p = root.expand(Path::new(&a.path))?;
    no_symlinks(&p)?;
    let f = fingerprint_file(&p, MAX_SOURCE)?;
    ensure!(
        f.bytes == a.bytes && f.sha256 == a.sha256,
        "ocr_page5_asset_changed"
    );
    Ok(p)
}
fn bind(root: &SkillRoot, p: &Path) -> Result<Asset> {
    no_symlinks(p)?;
    let f = fingerprint_file(p, MAX_SOURCE)?;
    Ok(Asset {
        path: oe_core::portable_source_path(root.path(), p)?,
        bytes: f.bytes,
        sha256: f.sha256,
    })
}
fn exact_read(root: &SkillRoot, a: &Asset) -> Result<Vec<u8>> {
    let p = verify_asset(root, a)?;
    let b = read_regular_bytes(&p, usize::try_from(MAX_SOURCE)?)?;
    ensure!(
        b.len() as u64 == a.bytes && sha256(&b) == a.sha256,
        "ocr_page5_read_identity"
    );
    Ok(b)
}
/// Request selects original source files only. Caller-supplied receipts and
/// process/provenance extras are not accepted or deserialized as authority.
pub fn parse_request5(value: &Value) -> Result<Request> {
    let object = value.as_object().context("ocr_page5_request_object")?;
    ensure!(
        object.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == ["schema", "tool", "config", "languages"]
                .into_iter()
                .collect(),
        "ocr_page5_request_keys"
    );
    fn keys(v: &Value) -> Result<()> {
        ensure!(
            v.as_object()
                .context("ocr_page5_asset_object")?
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == ["path", "sha256", "bytes"].into_iter().collect(),
            "ocr_page5_asset_keys"
        );
        Ok(())
    }
    keys(&value["tool"])?;
    keys(&value["config"])?;
    for l in value["languages"]
        .as_array()
        .context("ocr_page5_languages_array")?
    {
        ensure!(
            l.as_object()
                .context("ocr_page5_language_object")?
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == ["name", "source"].into_iter().collect(),
            "ocr_page5_language_keys"
        );
        keys(&l["source"])?;
    }
    let r: Request = serde_json::from_value(value.clone())?;
    validate_request5(&r)?;
    Ok(r)
}
pub fn validate_request5(r: &Request) -> Result<()> {
    generated(r.validate())?;
    generated(r.tool.validate())?;
    generated(r.config.validate())?;
    ensure!(
        r.schema == "oev2.knowledge-ocr-source-request/5"
            && r.languages.len() == 2
            && r.languages[0].name == "chi_sim"
            && r.languages[1].name == "eng",
        "ocr_page5_requested_language_domain"
    );
    for l in &r.languages {
        generated(l.validate())?;
        generated(l.source.validate())?;
        ensure!(
            l.source.bytes > 0 && l.source.bytes <= 64 * 1024 * 1024,
            "ocr_page5_language_source_limit"
        );
    }
    ensure!(
        Path::new(&r.tool.path).is_absolute()
            && Path::new(&r.tool.path).file_name() == Some(std::ffi::OsStr::new("tesseract"))
            && r.tool.bytes > 0
            && r.tool.bytes <= MAX_SOURCE,
        "ocr_page5_selected_tool_domain"
    );
    ensure!(
        r.config.bytes == 22
            && r.config.sha256
                == "59d079bb75d8b3d7c839a3564580cb559e362c93a9d70f234e421c0c3e767e04",
        "ocr_page5_exact_tsv_config_domain"
    );
    Ok(())
}
/// Structural process-success predicate only. This public predicate cannot
/// create private OCR provenance from a fabricable ProcessObservation.
pub fn process_completed5(o: &ProcessObservation) -> Result<()> {
    ensure!(
        o.status == "completed"
            && o.exit_code == Some(0)
            && o.report["status"] == "completed"
            && o.report["observed_status"] == "completed"
            && o.report["exit_code"] == 0
            && o.report["capture_complete"] == true
            && o.report["streams_collected"] == 2
            && o.report["stdout"]["truncated"] == false
            && o.report["stderr"]["truncated"] == false
            && o.report["executable"]["unchanged"] == true,
        "ocr_page5_process_not_complete"
    );
    ensure!(
        o.report["stdout"]["bytes"].as_u64() == Some(o.stdout.len() as u64)
            && o.report["stderr"]["bytes"].as_u64() == Some(o.stderr.len() as u64)
            && o.report["stdout"]["sha256"] == sha256(&o.stdout)
            && o.report["stderr"]["sha256"] == sha256(&o.stderr),
        "ocr_page5_process_stream_identity"
    );
    ensure!(
        o.report["borrowed_deadline"].is_object()
            && o.report["borrowed_deadline"]["expired"] == false,
        "ocr_page5_borrowed_process_deadline"
    );
    Ok(())
}
/// Pure recorded-input join. Invocation spelling and canonical byte-identity
/// target stay separate, as in the sole core process owner. This predicate
/// cannot create opaque provenance or authorize a symlink/source by itself.
pub fn process_selected_tool5(
    o: &ProcessObservation,
    selected: &Asset,
    invocation: &Path,
    canonical: &Path,
) -> Result<()> {
    generated(selected.validate())?;
    ensure!(
        Path::new(&selected.path) == invocation
            && invocation.is_absolute()
            && canonical.is_absolute(),
        "ocr_page5_selected_tool_path_context"
    );
    ensure!(
        o.report["executable"]["identity"]["sha256"] == selected.sha256
            && o.report["executable"]["identity"]["bytes"].as_u64() == Some(selected.bytes)
            && o.report["executable"]["path"].as_str() == invocation.to_str()
            && o.report["executable"]["resolved"].as_str() == canonical.to_str(),
        "ocr_page5_process_selected_tool_identity"
    );
    Ok(())
}
struct RawOriginal {
    asset: Asset,
    bytes: Vec<u8>,
}
impl RawOriginal {
    fn verify(&self, root: &SkillRoot) -> Result<()> {
        ensure!(
            exact_read(root, &self.asset)? == self.bytes,
            "ocr_page5_raw_original_changed"
        );
        Ok(())
    }
}
struct SelectedOcrSources<'a> {
    root: &'a SkillRoot,
    image: &'a Asset,
    tsv: &'a RawOriginal,
}
impl SourceProvider for SelectedOcrSources<'_> {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        match id {
            PNG_ID => exact_read(self.root, self.image),
            TSV_ID => {
                self.tsv.verify(self.root)?;
                Ok(self.tsv.bytes.clone())
            }
            _ => anyhow::bail!("ocr_page5_unselected_source"),
        }
    }
}
/// The same existing knowledge/process owner stores one actual call. Only
/// crate-private factories may create it; neither version's public audit does.
pub(crate) struct ActualOcrCall {
    root: SkillRoot,
    page: PreparedPageContent4,
    request: Request,
    source_originals: Vec<RawOriginal>,
    copies: Vec<RawOriginal>,
    artifacts: Vec<RawOriginal>,
    observations: Vec<ProcessObservation>,
    tsv: RawOriginal,
    content: SemanticOcrTsvSourceContent5,
    deadline: Instant,
    output_dir: PathBuf,
    started: Instant,
    remaining_entry: u64,
    remaining_qualification: u64,
}
pub(crate) enum OcrCallDomain {
    Development5,
    Registered6,
}
/// Shared internal values have no wire schema or standalone authority.
pub(crate) struct OcrCallAudit {
    pub page: oe_contracts::KnowledgePreparedPageContent4,
    pub request: Request,
    pub selected_copies: Vec<Asset>,
    pub process_observations: Vec<Value>,
    pub process_artifacts: Vec<Asset>,
    pub raw_tsv: Asset,
    pub content: Value,
    pub elapsed_millis: u64,
    pub remaining_millis_at_entry: u64,
    pub remaining_millis_before_qualification: u64,
}
impl ActualOcrCall {
    pub(crate) fn content(&self) -> &SemanticOcrTsvSourceContent5 {
        &self.content
    }
    pub(crate) fn page(&self) -> &oe_contracts::KnowledgePreparedPageContent4 {
        self.page.report()
    }

    pub(crate) fn audit(&self) -> Result<OcrCallAudit> {
        Ok(OcrCallAudit {
            page: self.page.report().clone(),
            request: self.request.clone(),
            selected_copies: self.copies.iter().map(|s| s.asset.clone()).collect(),
            process_observations: self.observations.iter().map(|o| o.report.clone()).collect(),
            process_artifacts: self.artifacts.iter().map(|s| s.asset.clone()).collect(),
            raw_tsv: self.tsv.asset.clone(),
            content: serde_json::to_value(&self.content)?,
            elapsed_millis: millis(self.started.elapsed()),
            remaining_millis_at_entry: self.remaining_entry,
            remaining_millis_before_qualification: self.remaining_qualification,
        })
    }
    pub(crate) fn publish(&self, name: &str, report: &Value) -> Result<()> {
        write_new(&self.output_dir.join(name), report)?;
        time_left(self.deadline, "audit_publish_return")?;
        Ok(())
    }
}
/// No Deserialize, Clone, Default, public fields or from-report constructor.
/// Only the real development-domain factory owns this version5 capability.
pub struct PreparedOcrPage5 {
    call: ActualOcrCall,
    report: KnowledgePreparedOcrPage5,
}
impl fmt::Debug for PreparedOcrPage5 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedOcrPage5")
            .field("scope", &"actual_process_source_observation_only")
            .finish_non_exhaustive()
    }
}
impl PreparedOcrPage5 {
    pub fn report(&self) -> &KnowledgePreparedOcrPage5 {
        &self.report
    }
    pub fn content(&self) -> &SemanticOcrTsvSourceContent5 {
        self.call.content()
    }
}
fn original(root: &SkillRoot, a: &Asset) -> Result<RawOriginal> {
    Ok(RawOriginal {
        asset: a.clone(),
        bytes: exact_read(root, a)?,
    })
}
fn save_copy(root: &SkillRoot, path: &Path, raw: &RawOriginal) -> Result<RawOriginal> {
    oe_core::write_new_bytes(path, &raw.bytes)?;
    let copy = original(root, &bind(root, path)?)?;
    ensure!(
        copy.asset.bytes == raw.asset.bytes && copy.asset.sha256 == raw.asset.sha256,
        "ocr_page5_source_copy_changed"
    );
    Ok(copy)
}
fn artifacts(root: &SkillRoot, dir: &Path, o: &ProcessObservation) -> Result<Vec<RawOriginal>> {
    let mut result = Vec::new();
    for (name, raw) in [("stdout", &o.stdout), ("stderr", &o.stderr)] {
        let artifact = original(root, &bind(root, &dir.join(name))?)?;
        ensure!(
            artifact.bytes == *raw,
            "ocr_page5_original_process_stream_changed"
        );
        result.push(artifact);
    }
    let receipt = original(root, &bind(root, &dir.join("process.json"))?)?;
    let json: Value = oe_core::json::strict_json(&receipt.bytes)?;
    ensure!(
        json == o.report,
        "ocr_page5_completed_process_receipt_mismatch"
    );
    result.push(receipt);
    Ok(result)
}
fn command(tool: &Path, arguments: Vec<OsString>, cwd: &Path) -> CommandSpec {
    CommandSpec {
        executable: tool.into(),
        arguments,
        cwd: cwd.into(),
        environment: BTreeMap::from([
            ("LC_ALL".into(), "C".into()),
            ("OMP_THREAD_LIMIT".into(), "1".into()),
        ]),
        remove_environment: vec!["TESSDATA_PREFIX".into()],
        timeout: Duration::from_secs(120),
        output_limit: 8 * 1024 * 1024,
    }
}
/// Borrow a deadline created by the caller BEFORE entering page/render prep.
/// Stage4 rendering is not preemptible by this clock: expiry after return
/// rejects OCR admission. Synchronous reads/CPU/cleanup remain explicit unknown.
pub(crate) fn prepare_ocr_call(
    root: &SkillRoot,
    page_request: &PageRequest,
    request: &Request,
    tools: &poppler::PopplerTools,
    deadline: Instant,
    output_leaf: &str,
    domain: OcrCallDomain,
) -> Result<ActualOcrCall> {
    let started = Instant::now();
    let remaining_entry = millis(time_left(deadline, "entry")?);
    validate_request5(request)?;
    if matches!(domain, OcrCallDomain::Development5) {
        ensure!(
            page_request.physical_page == 188
                && page_request.page_id == "local_pdf:interchangeability:page:188"
                && page_request.dpi == 144,
            "ocr_page5_development_page_domain"
        );
    }
    let version = match domain {
        OcrCallDomain::Development5 => "5",
        OcrCallDomain::Registered6 => "6",
    };
    ensure!(
        !output_leaf.is_empty()
            && output_leaf.len() <= 80
            && output_leaf
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "ocr_page5_output_leaf"
    );
    let mut sources = vec![
        original(root, &request.tool)?,
        original(root, &request.config)?,
    ];
    for l in &request.languages {
        sources.push(original(root, &l.source)?);
    }
    time_left(deadline, "selected_source_return")?;
    let page = prepare_content4(
        root,
        page_request,
        tools,
        &format!("ocr{version}-{output_leaf}"),
    )?;
    time_left(deadline, "page_render_return")?;
    verify_prepared_content4(&page, page.report())?;
    let parent = root
        .path()
        .join(format!("var/cache/knowledge/ocr-page{version}"));
    no_symlinks(&parent)?;
    std::fs::create_dir_all(&parent)?;
    no_symlinks(&parent)?;
    let dir = parent.join(output_leaf);
    std::fs::create_dir(&dir)?;
    no_symlinks(&dir)?;
    // Preserve current actual renderer/source observations even if OCR or TSV
    // later refuses. This public audit is never producer authority.
    write_new(
        &dir.join("prepared-page-audit.json"),
        &serde_json::to_value(page.report())?,
    )?;
    time_left(deadline, "page_audit_return")?;
    let data = dir.join("selected-tessdata");
    std::fs::create_dir(&data)?;
    let configs = data.join("configs");
    std::fs::create_dir(&configs)?;
    let mut copies = vec![save_copy(root, &configs.join("tsv"), &sources[1])?];
    for (i, l) in request.languages.iter().enumerate() {
        copies.push(save_copy(
            root,
            &data.join(format!("{}.traineddata", l.name)),
            &sources[i + 2],
        )?);
    }
    for s in sources.iter().chain(&copies) {
        s.verify(root)?;
    }
    time_left(deadline, "selected_copy_return")?;
    let tool = verify_asset(root, &request.tool)?;
    let canonical_tool = std::fs::canonicalize(&tool)?;
    let version_dir = dir.join("tool-version");
    let version = run_command_with_deadline(
        &command(&tool, vec!["--version".into()], root.path()),
        &version_dir,
        deadline,
    )?;
    process_completed5(&version)?;
    process_selected_tool5(&version, &request.tool, &tool, &canonical_tool)?;
    time_left(deadline, "version_return")?;
    let mut raw_artifacts = artifacts(root, &version_dir, &version)?;
    for s in sources.iter().chain(&copies) {
        s.verify(root)?;
    }
    verify_prepared_content4(&page, page.report())?;
    let image = verify_asset(root, &page.report().render.png)?;
    let config = verify_asset(root, &copies[0].asset)?;
    let args = vec![
        image.into_os_string(),
        "stdout".into(),
        "-l".into(),
        "chi_sim+eng".into(),
        "--tessdata-dir".into(),
        data.into_os_string(),
        config.into_os_string(),
    ];
    time_left(deadline, "ocr_admission")?;
    let ocr_dir = dir.join("ocr");
    // The only actual OCR invocation. No retries, alternate parameters or fallback.
    let ocr = run_command_with_deadline(&command(&tool, args, root.path()), &ocr_dir, deadline)?;
    process_completed5(&ocr)?;
    process_selected_tool5(&ocr, &request.tool, &tool, &canonical_tool)?;
    time_left(deadline, "ocr_return")?;
    raw_artifacts.extend(artifacts(root, &ocr_dir, &ocr)?);
    let tsv = original(root, &bind(root, &ocr_dir.join("stdout"))?)?;
    ensure!(tsv.bytes == ocr.stdout, "ocr_page5_actual_tsv_output");
    let reader = SelectedOcrSources {
        root,
        image: &page.report().render.png,
        tsv: &tsv,
    };
    let tsv_asset = SemanticContentAsset3 {
        logical_id: TSV_ID.into(),
        sha256: tsv.asset.sha256.clone(),
        bytes: tsv.asset.bytes,
    };
    let content = read_tsv_source5(
        &reader,
        page.decoded_image(),
        &tsv_asset,
        &ContentLimits::default(),
    )?;
    time_left(deadline, "tsv_parse_return")?;
    for s in sources.iter().chain(&copies).chain(&raw_artifacts) {
        s.verify(root)?;
    }
    verify_prepared_content4(&page, page.report())?;
    verify_tsv_source5(
        &reader,
        &page.decoded_image().content().binding,
        &tsv_asset,
        &content,
        &ContentLimits::default(),
    )?;
    let remaining = millis(time_left(deadline, "before_qualification")?);
    let call = ActualOcrCall {
        root: root.clone(),
        page,
        request: request.clone(),
        source_originals: sources,
        copies,
        artifacts: raw_artifacts,
        observations: vec![version, ocr],
        tsv,
        content,
        deadline,
        output_dir: dir,
        started,
        remaining_entry,
        remaining_qualification: remaining,
    };
    verify_actual_ocr_call(&call)?;
    Ok(call)
}
/// Version5 retains its original exact development188/144dpi admission. The
/// general registered-page domain has an explicit separate version6 wrapper.
pub fn prepare_page_and_ocr5(
    root: &SkillRoot,
    page_request: &PageRequest,
    request: &Request,
    tools: &poppler::PopplerTools,
    deadline: Instant,
    output_leaf: &str,
) -> Result<PreparedOcrPage5> {
    let call = prepare_ocr_call(
        root,
        page_request,
        request,
        tools,
        deadline,
        output_leaf,
        OcrCallDomain::Development5,
    )?;
    let a = call.audit()?;
    let report = KnowledgePreparedOcrPage5 {
        schema: "oev2.knowledge-prepared-ocr-page/5".into(),
        page: a.page,
        request: a.request,
        selected_copies: a.selected_copies,
        process_observations: a.process_observations,
        process_artifacts: a.process_artifacts,
        raw_tsv: a.raw_tsv,
        content: a.content,
        elapsed_millis: a.elapsed_millis,
        remaining_millis_at_entry: a.remaining_millis_at_entry,
        remaining_millis_before_qualification: a.remaining_millis_before_qualification,
        deadline_validation: "same_borrowed_instant_admission_postreturn".into(),
        language_loading: "unknown_not_attested".into(),
        transcription_accuracy: "unverified".into(),
        qualification: "actual_process_source_content_observation_only".into(),
        model: "not_run".into(),
        engineering_adoption: "not_run".into(),
        unknown: UNKNOWN.iter().map(|s| (*s).into()).collect(),
    };
    generated(report.validate())?;
    let prepared = PreparedOcrPage5 { call, report };
    verify_prepared_ocr5(&prepared, prepared.report())?;
    prepared.call.publish(
        "prepared-ocr-audit.json",
        &serde_json::to_value(prepared.report())?,
    )?;
    Ok(prepared)
}
/// A report is only a projection of this same-call capability, never its origin.
pub fn verify_prepared_ocr5(
    p: &PreparedOcrPage5,
    candidate: &KnowledgePreparedOcrPage5,
) -> Result<()> {
    time_left(p.call.deadline, "verify_entry")?;
    generated(candidate.validate())?;
    ensure!(
        candidate == &p.report && candidate.request == p.call.request,
        "ocr_page5_public_projection_changed"
    );
    ensure!(
        candidate.page == *p.call.page()
            && candidate.process_observations
                == p.call
                    .observations
                    .iter()
                    .map(|o| o.report.clone())
                    .collect::<Vec<_>>()
            && serde_json::to_value(p.call.content())? == candidate.content,
        "ocr_page5_content_projection"
    );
    verify_actual_ocr_call(&p.call)
}
/// Current-call source/process replay is shared; it creates no capability from
/// audit JSON and is never exposed outside this knowledge owner.
pub(crate) fn verify_actual_ocr_call(p: &ActualOcrCall) -> Result<()> {
    time_left(p.deadline, "verify_entry")?;
    validate_request5(&p.request)?;
    verify_prepared_content4(&p.page, p.page.report())?;
    for s in p
        .source_originals
        .iter()
        .chain(&p.copies)
        .chain(&p.artifacts)
    {
        s.verify(&p.root)?;
    }
    ensure!(
        p.copies.len() == 3 && p.source_originals.len() == 4 && p.observations.len() == 2,
        "ocr_page5_original_inventory"
    );
    for (copy, source) in p.copies.iter().zip(p.source_originals.iter().skip(1)) {
        ensure!(
            copy.bytes == source.bytes && copy.asset.sha256 == source.asset.sha256,
            "ocr_page5_source_copy_projection"
        );
    }
    let invocation = verify_asset(&p.root, &p.request.tool)?;
    let canonical = std::fs::canonicalize(&invocation)?;
    for o in &p.observations {
        process_completed5(o)?;
        process_selected_tool5(o, &p.request.tool, &invocation, &canonical)?;
    }
    ensure!(
        p.tsv.bytes == p.observations[1].stdout,
        "ocr_page5_same_call_process_output"
    );
    p.tsv.verify(&p.root)?;
    let reader = SelectedOcrSources {
        root: &p.root,
        image: &p.page.report().render.png,
        tsv: &p.tsv,
    };
    let asset = SemanticContentAsset3 {
        logical_id: TSV_ID.into(),
        sha256: p.tsv.asset.sha256.clone(),
        bytes: p.tsv.asset.bytes,
    };
    verify_tsv_source5(
        &reader,
        &p.page.decoded_image().content().binding,
        &asset,
        &p.content,
        &ContentLimits::default(),
    )?;
    time_left(p.deadline, "verify_return")?;
    Ok(())
}
