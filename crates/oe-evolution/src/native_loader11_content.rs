//! Paged complete diagnostic CONTENT from selected raw Cargo stream files.
//! All lines use the unchanged original5 interpretation; the report is /11.
//! Reported PIDs/UUIDs/paths remain text, without native or owner authority.
use crate::native_runtime::{read, reference};
use crate::native_stream11::{VerifiedCargoStreamContent11, inspect_capture_content11};
use crate::*;

fn unknown11() -> Vec<String> {
    let mut values = crate::native_runtime5_content::unknown();
    values.push("segmented_capture_content_does_not_recreate_live_owner_or_deadline".into());
    values.push(
        "diagnostic_content_alone_does_not_qualify_full_native11_execution_or_registry_adoption"
            .into(),
    );
    values
}

fn visit_lines11(
    content: &VerifiedCargoStreamContent11,
    mut f: impl FnMut(CodeNativeLoaderDiagnosticLine5) -> Result<()>,
) -> Result<u64> {
    let limits = content.resources();
    let mut pending = Vec::new();
    let mut ordinal = 0usize;
    let mut offset = 0usize;
    let mut emit = |raw: &[u8]| -> Result<()> {
        ensure!(
            (ordinal as u64) < limits.maximum_lines,
            "loader11_line_count_limit"
        );
        ensure!(
            raw.len() as u64 <= limits.maximum_line_bytes && !raw.contains(&0),
            "loader11_line_limit_or_nul"
        );
        let text = std::str::from_utf8(raw).context("loader11_complete_line_utf8")?;
        let line = crate::native_runtime5_content::line(ordinal, offset, text)?;
        f(line)?;
        ordinal += 1;
        offset = offset
            .checked_add(raw.len())
            .context("loader11_offset_overflow")?;
        Ok(())
    };
    content.visit_stream("stderr", |_, chunk| {
        for part in chunk.split_inclusive(|b| *b == b'\n') {
            ensure!(
                pending
                    .len()
                    .checked_add(part.len())
                    .is_some_and(|n| n as u64 <= limits.maximum_line_bytes),
                "loader11_cross_segment_line_limit"
            );
            pending.extend_from_slice(part);
            if pending.last() == Some(&b'\n') {
                emit(&pending)?;
                pending.clear();
            }
        }
        Ok(())
    })?;
    if !pending.is_empty() {
        emit(&pending)?;
    }
    ensure!(
        offset as u64 == content.capture().stderr_bytes,
        "loader11_full_raw_coordinates"
    );
    Ok(ordinal as u64)
}

fn page11(
    content: &VerifiedCargoStreamContent11,
    first_ordinal: u64,
    first_byte_offset: u64,
) -> CodeNativeLoaderPage11 {
    CodeNativeLoaderPage11 {
        schema: "oev2.native-loader-page/11".into(),
        declared_domain: "host_compilation".into(),
        raw_sha256: content.capture().stderr_sha256.clone(),
        first_ordinal,
        first_byte_offset,
        lines: Vec::new(),
    }
}

/// Produce exact bounded page bytes in original order without collecting the
/// whole raw stream or a whole unbounded diagnostic report in memory.
fn visit_pages11(
    content: &VerifiedCargoStreamContent11,
    mut f: impl FnMut(usize, &[u8], &CodeNativeLoaderPage11) -> Result<()>,
) -> Result<(u64, usize)> {
    let mut page = page11(content, 0, 0);
    let mut bytes = serde_json::to_vec(&page)?.len() + 1;
    let mut total = 0u64;
    let mut page_count = 0usize;
    let mut flush = |page: &CodeNativeLoaderPage11, expected: usize| -> Result<()> {
        if page.lines.is_empty() {
            return Ok(());
        }
        page.validate()
            .map_err(|e| anyhow::anyhow!("loader11_page:{e:?}"))?;
        let mut serialized = serde_json::to_vec(page)?;
        serialized.push(b'\n');
        ensure!(
            serialized.len() == expected
                && expected as u64 <= content.resources().maximum_page_bytes,
            "loader11_page_bytes_limit"
        );
        total = total
            .checked_add(expected as u64)
            .context("loader11_derived_overflow")?;
        ensure!(
            total <= content.resources().maximum_derived_bytes,
            "loader11_derived_bytes_limit"
        );
        f(page_count, &serialized, page)?;
        page_count += 1;
        Ok(())
    };
    let count = visit_lines11(content, |line| {
        let line_bytes = serde_json::to_vec(&line)?.len();
        let added = line_bytes + usize::from(!page.lines.is_empty());
        if bytes
            .checked_add(added)
            .is_none_or(|n| n as u64 > content.resources().maximum_page_bytes)
        {
            flush(&page, bytes)?;
            page = page11(content, line.ordinal, line.byte_offset);
            bytes = serde_json::to_vec(&page)?.len() + 1;
        }
        let added = line_bytes + usize::from(!page.lines.is_empty());
        bytes = bytes.checked_add(added).context("loader11_page_overflow")?;
        ensure!(
            bytes as u64 <= content.resources().maximum_page_bytes,
            "loader11_single_line_page_limit"
        );
        page.lines.push(line);
        Ok(())
    })?;
    flush(&page, bytes)?;
    Ok((count, page_count))
}

pub fn prepare_loader_stream_content11(
    root: &SkillRoot,
    capture: &ArtifactReference,
) -> Result<CodeNativeLoaderStreamReport11> {
    let content = inspect_capture_content11(root, capture)?;
    let directory = content.evidence_directory().join("loader-pages11");
    oe_core::create_new_private_dir(&root.private_path(&directory)?)?;
    let mut pages = Vec::new();
    let (line_count, page_count) = visit_pages11(&content, |index, bytes, _| {
        let path = directory.join(format!("page-{index:05}.json"));
        write_new_bytes(&path, bytes)?;
        pages.push(reference(root, &path)?);
        Ok(())
    })?;
    ensure!(page_count == pages.len(), "loader11_page_count");
    content.verify_current()?;
    let report = CodeNativeLoaderStreamReport11 {
        schema: "oev2.native-loader-stream-content/11".into(),
        capture: capture.clone(),
        declared_domain: "host_compilation".into(),
        raw_sha256: content.capture().stderr_sha256.clone(),
        raw_bytes: content.capture().stderr_bytes,
        line_count,
        pages,
        qualification: "diagnostic_content_only".into(),
        unknown: unknown11(),
    };
    report
        .validate()
        .map_err(|e| anyhow::anyhow!("loader11_report:{e:?}"))?;
    write_new(
        &content.evidence_directory().join("loader-content11.json"),
        &serde_json::to_value(&report)?,
    )?;
    verify_loader_stream_content11(root, &report)?;
    Ok(report)
}

pub fn verify_loader_stream_content11(
    root: &SkillRoot,
    report: &CodeNativeLoaderStreamReport11,
) -> Result<()> {
    report
        .validate()
        .map_err(|e| anyhow::anyhow!("loader11_report:{e:?}"))?;
    let content = inspect_capture_content11(root, &report.capture)?;
    ensure!(
        report.schema == "oev2.native-loader-stream-content/11"
            && report.qualification == "diagnostic_content_only"
            && report.unknown == unknown11()
            && report.declared_domain == "host_compilation"
            && report.raw_sha256 == content.capture().stderr_sha256
            && report.raw_bytes == content.capture().stderr_bytes,
        "loader11_report_binding_or_qualification"
    );
    let directory = content.evidence_directory().join("loader-pages11");
    no_symlinks(&directory)?;
    let actual = std::fs::read_dir(&directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let wanted: BTreeSet<_> = (0..report.pages.len())
        .map(|i| directory.join(format!("page-{i:05}.json")))
        .collect();
    ensure!(actual == wanted, "loader11_page_inventory");
    let mut observed = 0usize;
    let (lines, count) = visit_pages11(&content, |index, bytes, _| {
        let selected = report.pages.get(index).context("loader11_page_missing")?;
        let path = directory.join(format!("page-{index:05}.json"));
        ensure!(
            *selected == reference(root, &path)?,
            "loader11_page_reference_changed"
        );
        let actual = read_regular_bytes(&path, content.resources().maximum_page_bytes as usize)?;
        ensure!(
            actual == bytes,
            "loader11_original_line_bytes_or_order_changed"
        );
        observed += 1;
        Ok(())
    })?;
    ensure!(
        lines == report.line_count && count == report.pages.len() && observed == count,
        "loader11_complete_page_or_line_count"
    );
    content.verify_current()?;
    Ok(())
}

/// Reading reported PID groups gives no actual process admission. The complete
/// original compiler-intent/parent/SDK checks remain the caller's obligation.
pub fn reported_line_ordinals11(
    root: &SkillRoot,
    report: &CodeNativeLoaderStreamReport11,
    pid: u64,
) -> Result<Vec<u64>> {
    verify_loader_stream_content11(root, report)?;
    let mut ordinals = Vec::new();
    for reference in &report.pages {
        let page: CodeNativeLoaderPage11 = serde_json::from_value(read(root, reference)?)?;
        for line in page.lines {
            if line.reported_pid == Some(pid) && line.event == "uuid_path_report" {
                ordinals.push(line.ordinal);
            }
        }
    }
    Ok(ordinals)
}

/// Verified retained CONTENT index, deliberately neither Clone nor Deserialize.
/// The keys are reported PID text. Actual process association is a separate
/// obligation of native_loader6::retained_call_association.
pub(crate) struct VerifiedReportedPidIndex11 {
    root: SkillRoot,
    report_reference: ArtifactReference,
    report: CodeNativeLoaderStreamReport11,
    ordinals: BTreeMap<u64, Vec<u64>>,
}
impl VerifiedReportedPidIndex11 {
    pub(crate) fn inspect(root: &SkillRoot, reference: &ArtifactReference) -> Result<Self> {
        let report: CodeNativeLoaderStreamReport11 =
            serde_json::from_value(read(root, reference)?)?;
        verify_loader_stream_content11(root, &report)?;
        let mut ordinals = BTreeMap::<u64, Vec<u64>>::new();
        for page_reference in &report.pages {
            let page: CodeNativeLoaderPage11 = serde_json::from_value(read(root, page_reference)?)?;
            for line in page.lines {
                if line.event == "uuid_path_report" {
                    if let Some(pid) = line.reported_pid {
                        ordinals.entry(pid).or_default().push(line.ordinal);
                    }
                }
            }
        }
        let result = Self {
            root: SkillRoot::new(root.path())?,
            report_reference: reference.clone(),
            report,
            ordinals,
        };
        result.verify_current()?;
        Ok(result)
    }
    pub(crate) fn capture(&self) -> &ArtifactReference {
        &self.report.capture
    }
    pub(crate) fn reported_lines(&self, pid: u64) -> &[u64] {
        self.ordinals.get(&pid).map(Vec::as_slice).unwrap_or(&[])
    }
    pub(crate) fn verify_current(&self) -> Result<()> {
        ensure!(
            read(&self.root, &self.report_reference)? == serde_json::to_value(&self.report)?,
            "loader11_index_report_changed"
        );
        verify_loader_stream_content11(&self.root, &self.report)
    }
}

pub struct CargoStreamContent11Inspector {
    pub root: SkillRoot,
}
impl CodeNativeCargoStreamContent11Port for CargoStreamContent11Inspector {
    fn inspect_cargo_stream_content(
        &self,
        input: &CodeNativeCargoStreamCapture11,
    ) -> std::result::Result<CodeNativeLoaderStreamReport11, FailureRecord> {
        let result = (|| -> Result<CodeNativeLoaderStreamReport11> {
            let folder = Path::new(&input.process.path)
                .parent()
                .and_then(Path::parent)
                .context("loader11_capture_path")?;
            let capture = reference(&self.root, &folder.join("stream-capture11.json"))?;
            ensure!(
                read(&self.root, &capture)? == serde_json::to_value(input)?,
                "loader11_caller_capture_changed"
            );
            let report_path = folder.join("loader-content11.json");
            if report_path.exists() {
                let report = serde_json::from_value(read(
                    &self.root,
                    &reference(&self.root, &report_path)?,
                )?)?;
                verify_loader_stream_content11(&self.root, &report)?;
                Ok(report)
            } else {
                prepare_loader_stream_content11(&self.root, &capture)
            }
        })();
        result.map_err(|e| FailureRecord {
            code: ErrorCode::InvalidContract, phase: "cargo11_stream_content".into(), retryable: false,
            partial_result: Some(json!({"execution":"content_check_failed","reason":format!("{e:#}"),
                "native_qualification":"unknown","live_authority":"not_granted","engineering_adoption":"unknown"})),
        })
    }
}
