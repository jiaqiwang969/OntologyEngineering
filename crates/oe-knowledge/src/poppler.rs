//! Bounded native Poppler boundary. No OCR and no legacy business wrapper.
use crate::source_pack::{Stage, file_hash, within_root};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopplerTools {
    pub pdfinfo: PathBuf,
    pub pdftotext: PathBuf,
    pub pdftoppm: PathBuf,
}
impl Default for PopplerTools {
    fn default() -> Self {
        Self {
            pdfinfo: "pdfinfo".into(),
            pdftotext: "pdftotext".into(),
            pdftoppm: "pdftoppm".into(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct PopplerLimits {
    pub timeout: Duration,
    pub output_bytes: u64,
    pub page_bytes: usize,
    pub pages: usize,
}
impl Default for PopplerLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(180),
            output_bytes: 64 * 1024 * 1024,
            page_bytes: 2 * 1024 * 1024,
            pages: 20000,
        }
    }
}
impl PopplerLimits {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.timeout.is_zero()
                && self.timeout <= Duration::from_secs(180)
                && self.output_bytes > 0
                && self.output_bytes <= 64 * 1024 * 1024
                && self.page_bytes > 0
                && self.page_bytes <= 2 * 1024 * 1024
                && self.pages > 0
                && self.pages <= 20000,
            "poppler_invalid_limits"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolIdentity {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub version: String,
}
#[derive(Debug)]
pub struct Extraction {
    pub pages: Vec<String>,
    pub observations: Value,
}
#[derive(Debug)]
pub struct PdfInfo {
    pub page_count: u64,
    pub tool: ToolIdentity,
    pub output_sha256: String,
}
#[derive(Debug)]
pub struct Rendered {
    pub bytes: Vec<u8>,
    pub tool: ToolIdentity,
    pub width: u32,
    pub height: u32,
}

/// Private observations from the same owning process loop. Public JSON is data.
pub(crate) struct ProcessCapture4 {
    pub(crate) argv: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) timeout_millis: u64,
    pub(crate) stdout_limit_bytes: u64,
    pub(crate) elapsed_millis: u64,
    pub(crate) group_kill_return: i32,
    pub(crate) child_wait_completed: bool,
    pub(crate) child_wait_exit_code: Option<i32>,
    pub(crate) scratch_removed: bool,
}

/// Only the actual process factory below constructs this non-deserializable value.
pub(crate) struct ObservedRender4 {
    document: oe_core::FileIdentity,
    document_after: oe_core::FileIdentity,
    document_path: PathBuf,
    page: u64,
    page_count: u64,
    dpi: u32,
    tool: ToolIdentity,
    processes: Vec<(String, ToolIdentity, ProcessCapture4)>,
}
impl ObservedRender4 {
    pub(crate) fn document(&self) -> &oe_core::FileIdentity {
        &self.document
    }
    pub(crate) fn document_after(&self) -> &oe_core::FileIdentity {
        &self.document_after
    }
    pub(crate) fn document_path(&self) -> &Path {
        &self.document_path
    }
    pub(crate) fn page(&self) -> u64 {
        self.page
    }
    pub(crate) fn page_count(&self) -> u64 {
        self.page_count
    }
    pub(crate) fn dpi(&self) -> u32 {
        self.dpi
    }
    pub(crate) fn tool(&self) -> &ToolIdentity {
        &self.tool
    }
    pub(crate) fn processes(&self) -> &[(String, ToolIdentity, ProcessCapture4)] {
        &self.processes
    }
    pub(crate) fn png(&self) -> &[u8] {
        &self
            .processes
            .last()
            .expect("actual render process")
            .2
            .stdout
    }
}

fn tool_path(path: &Path) -> Result<PathBuf> {
    let found = if path.is_absolute() {
        path.to_path_buf()
    } else {
        ensure!(
            path.components().count() == 1,
            "poppler_tool_must_be_absolute_or_name"
        );
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(path))
            .find(|p| p.is_file())
            .context("poppler_tool_missing")?
    };
    let canonical = std::fs::canonicalize(&found).context("poppler_tool_missing")?;
    oe_core::no_symlinks(&canonical)?;
    Ok(canonical)
}

/// Pipes are nonblocking and polled by the same bounded loop. Descendants
/// holding a pipe cannot stall a reader join after the deadline.
#[cfg(unix)]
fn process_observed4(
    root: &Path,
    tool: &Path,
    args: &[String],
    limits: &PopplerLimits,
) -> Result<ProcessCapture4> {
    use std::os::{fd::AsRawFd, unix::process::CommandExt};
    limits.validate()?;
    let scratch = Stage::new(root, "poppler")?;
    let outpath = scratch.path.join("stdout");
    let mut out = oe_core::create_new_regular_file(&outpath)?;
    let mut command = Command::new(tool);
    command
        .args(args)
        .env("LC_ALL", "C")
        .current_dir(&scratch.path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().context("poppler_process_spawn_failed")?;
    let mut stdout = child.stdout.take().context("poppler_stdout")?;
    let mut stderr = child.stderr.take().context("poppler_stderr")?;
    let start = Instant::now();
    let result = (|| -> Result<(Vec<u8>, Option<i32>)> {
        for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
            let old = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            ensure!(
                old >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, old | libc::O_NONBLOCK) } >= 0,
                "poppler_nonblocking_pipe_failed"
            );
        }
        let mut status = None;
        let (mut out_eof, mut err_eof) = (false, false);
        let mut error = Vec::new();
        let mut count = 0_u64;
        let mut buffer = [0; 65536];
        loop {
            ensure!(start.elapsed() < limits.timeout, "poppler_process_timeout");
            // One bounded read per pipe per iteration avoids starvation by an
            // output flood and keeps the timeout observable.
            if !out_eof {
                match stdout.read(&mut buffer) {
                    Ok(0) => out_eof = true,
                    Ok(n) => {
                        count += n as u64;
                        ensure!(count <= limits.output_bytes, "poppler_stdout_limit");
                        out.write_all(&buffer[..n])?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
            if !err_eof {
                match stderr.read(&mut buffer) {
                    Ok(0) => err_eof = true,
                    Ok(n) => {
                        ensure!(error.len() + n <= 1024 * 1024, "poppler_stderr_limit");
                        error.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
            if status.is_none() {
                status = child.try_wait()?;
            }
            if let Some(status) = status
                && out_eof
                && err_eof
            {
                out.sync_all()?;
                return Ok((error, status.code()));
            }
            thread::sleep(Duration::from_millis(1));
        }
    })();
    let group_kill_return = unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    let _ = child.kill();
    let waited = child.wait();
    drop(stdout);
    drop(stderr);
    let (errors, exit_code) = result?;
    let bytes = oe_core::read_regular_bytes(&outpath, usize::try_from(limits.output_bytes)?)?;
    let cwd = scratch.path.clone();
    let elapsed_millis = u64::try_from(start.elapsed().as_millis())?;
    let child_wait_completed = waited.is_ok();
    let child_wait_exit_code = waited.ok().and_then(|s| s.code());
    drop(out);
    drop(scratch);
    let scratch_removed = !cwd.try_exists()?;
    Ok(ProcessCapture4 {
        argv: std::iter::once(tool.to_string_lossy().into_owned())
            .chain(args.iter().cloned())
            .collect(),
        cwd,
        stdout: bytes,
        stderr: errors,
        exit_code,
        timeout_millis: u64::try_from(limits.timeout.as_millis())?,
        stdout_limit_bytes: limits.output_bytes,
        elapsed_millis,
        group_kill_return,
        child_wait_completed,
        child_wait_exit_code,
        scratch_removed,
    })
}
#[cfg(not(unix))]
fn process_observed4(
    _root: &Path,
    _tool: &Path,
    _args: &[String],
    _limits: &PopplerLimits,
) -> Result<ProcessCapture4> {
    anyhow::bail!("bounded_poppler_requires_unix")
}
fn process(
    root: &Path,
    tool: &Path,
    args: &[String],
    limits: &PopplerLimits,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let observed = process_observed4(root, tool, args, limits)?;
    ensure!(
        observed.exit_code == Some(0),
        "poppler_process_failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    Ok((observed.stdout, observed.stderr))
}
fn identify(root: &Path, path: &Path, limits: &PopplerLimits) -> Result<ToolIdentity> {
    let path = tool_path(path)?;
    let (hash, bytes) = file_hash(&path, 128 * 1024 * 1024)?;
    let version_limits = PopplerLimits {
        timeout: limits.timeout.min(Duration::from_secs(10)),
        output_bytes: 65536,
        ..limits.clone()
    };
    let (mut out, err) = process(root, &path, &["-v".into()], &version_limits)?;
    out.extend(err);
    let text = String::from_utf8(out)?;
    let version = text
        .lines()
        .find(|l| l.contains("version"))
        .context("poppler_version_missing")?
        .to_owned();
    ensure!(
        file_hash(&path, 128 * 1024 * 1024)? == (hash.clone(), bytes),
        "poppler_tool_changed"
    );
    Ok(ToolIdentity {
        path,
        sha256: hash,
        bytes,
        version,
    })
}
fn execute(
    root: &Path,
    identity: &ToolIdentity,
    args: &[String],
    limits: &PopplerLimits,
) -> Result<Vec<u8>> {
    ensure!(
        file_hash(&identity.path, 128 * 1024 * 1024)? == (identity.sha256.clone(), identity.bytes),
        "poppler_tool_changed"
    );
    let result = process(root, &identity.path, args, limits).map(|(out, _)| out);
    ensure!(
        file_hash(&identity.path, 128 * 1024 * 1024)? == (identity.sha256.clone(), identity.bytes),
        "poppler_tool_changed"
    );
    result
}
pub fn inspect(
    root: &Path,
    path: &Path,
    tools: &PopplerTools,
    limits: &PopplerLimits,
) -> Result<PdfInfo> {
    limits.validate()?;
    let path = within_root(root, path)?;
    let tool = identify(root, &tools.pdfinfo, limits)?;
    let mut info_limits = limits.clone();
    info_limits.output_bytes = 1024 * 1024;
    let raw = execute(
        root,
        &tool,
        &[
            "-enc".into(),
            "UTF-8".into(),
            path.to_string_lossy().into_owned(),
        ],
        &info_limits,
    )?;
    let text = std::str::from_utf8(&raw)?;
    let mut pages = None;
    let mut encryption = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Pages:") {
            ensure!(pages.is_none(), "pdfinfo_duplicate_page_count");
            pages = Some(v.trim().parse::<u64>()?);
        }
        if let Some(v) = line.strip_prefix("Encrypted:") {
            encryption = Some(v.trim().starts_with("yes"));
        }
    }
    let page_count = pages.context("source_pdf_page_count_unknown")?;
    ensure!(
        page_count > 0 && page_count <= limits.pages as u64 && encryption == Some(false),
        "local_pdf_page_count_or_encryption_invalid"
    );
    Ok(PdfInfo {
        page_count,
        tool,
        output_sha256: oe_core::sha256(&raw),
    })
}
pub fn extract(
    root: &Path,
    path: &Path,
    sha: &str,
    tools: &PopplerTools,
    limits: &PopplerLimits,
) -> Result<Extraction> {
    limits.validate()?;
    let path = within_root(root, path)?;
    let original = file_hash(&path, 4 * 1024 * 1024 * 1024)?;
    ensure!(original.0 == sha, "local_pdf_source_hash_mismatch");
    let info = inspect(root, &path, tools, limits)?;
    let tool = identify(root, &tools.pdftotext, limits)?;
    let raw = execute(
        root,
        &tool,
        &[
            "-layout".into(),
            "-enc".into(),
            "UTF-8".into(),
            path.to_string_lossy().into_owned(),
            "-".into(),
        ],
        limits,
    )?;
    let mut pages = Vec::new();
    let mut reader = BufReader::new(raw.as_slice());
    loop {
        let mut page = Vec::new();
        let n = read_page(&mut reader, &mut page, limits.page_bytes)?;
        if n == 0 {
            break;
        }
        ensure!(
            page.len() <= limits.page_bytes,
            "local_pdf_page_output_limit"
        );
        if page.last() == Some(&12) {
            page.pop();
        }
        pages.push(String::from_utf8(page)?);
        ensure!(pages.len() <= limits.pages, "local_pdf_page_limit");
    }
    ensure!(
        pages.len() == info.page_count as usize,
        "local_pdf_extracted_page_count_mismatch"
    );
    ensure!(
        file_hash(&path, 4 * 1024 * 1024 * 1024)? == original,
        "local_pdf_source_changed_during_registration"
    );
    Ok(Extraction {
        pages,
        observations: json!({"pdfinfo":info.tool,"pdftotext":tool,"pdfinfo_output_sha256":info.output_sha256,"text_output_sha256":oe_core::sha256(&raw),"stdout_bytes":raw.len(),"timeout_seconds_per_process":limits.timeout.as_secs_f64(),"stdout_limit_bytes":limits.output_bytes,"page_limit_bytes":limits.page_bytes,"max_physical_pages":limits.pages,"ocr":"not_run"}),
    })
}
pub fn render(
    root: &Path,
    path: &Path,
    page: u64,
    dpi: u32,
    tools: &PopplerTools,
) -> Result<Rendered> {
    ensure!(
        (72..=300).contains(&dpi) && page > 0,
        "source_view_dpi_or_page_invalid"
    );
    let limits = PopplerLimits {
        timeout: Duration::from_secs(120),
        ..Default::default()
    };
    let path = within_root(root, path)?;
    let tool = identify(root, &tools.pdftoppm, &limits)?;
    let raw = execute(
        root,
        &tool,
        &[
            "-f".into(),
            page.to_string(),
            "-l".into(),
            page.to_string(),
            "-singlefile".into(),
            "-r".into(),
            dpi.to_string(),
            "-png".into(),
            path.to_string_lossy().into_owned(),
        ],
        &limits,
    )?;
    ensure!(
        raw.len() >= 24 && raw[..8] == *b"\x89PNG\r\n\x1a\n" && raw[12..16] == *b"IHDR",
        "source_view_invalid_image"
    );
    let width = u32::from_be_bytes(raw[16..20].try_into()?);
    let height = u32::from_be_bytes(raw[20..24].try_into()?);
    ensure!(width > 0 && height > 0, "source_view_invalid_image");
    Ok(Rendered {
        bytes: raw,
        tool,
        width,
        height,
    })
}

fn read_page(reader: &mut impl BufRead, page: &mut Vec<u8>, limit: usize) -> Result<usize> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(page.len());
        }
        let end = available.iter().position(|x| *x == 12).map(|p| p + 1);
        let n = end.unwrap_or(available.len());
        ensure!(page.len() + n <= limit, "local_pdf_page_output_limit");
        page.extend_from_slice(&available[..n]);
        reader.consume(n);
        if end.is_some() {
            return Ok(page.len());
        }
    }
}

fn identify_observed4(
    root: &Path,
    path: &Path,
    limits: &PopplerLimits,
) -> Result<(ToolIdentity, ProcessCapture4)> {
    let path = tool_path(path)?;
    path.to_str().context("page_content4_tool_path_utf8")?;
    let original = file_hash(&path, 128 * 1024 * 1024)?;
    let version_limits = PopplerLimits {
        timeout: limits.timeout.min(Duration::from_secs(10)),
        output_bytes: 65536,
        ..limits.clone()
    };
    let captured = process_observed4(root, &path, &["-v".into()], &version_limits)?;
    ensure!(
        captured.exit_code == Some(0),
        "poppler_process_failed: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    let mut all = captured.stdout.clone();
    all.extend_from_slice(&captured.stderr);
    let text = String::from_utf8(all)?;
    let version = text
        .lines()
        .find(|l| l.contains("version"))
        .context("poppler_version_missing")?
        .to_owned();
    ensure!(
        file_hash(&path, 128 * 1024 * 1024)? == original,
        "poppler_tool_changed"
    );
    Ok((
        ToolIdentity {
            path,
            sha256: original.0,
            bytes: original.1,
            version,
        },
        captured,
    ))
}
fn execute_observed4(
    root: &Path,
    tool: &ToolIdentity,
    args: &[String],
    limits: &PopplerLimits,
) -> Result<ProcessCapture4> {
    let original = (tool.sha256.clone(), tool.bytes);
    ensure!(
        file_hash(&tool.path, 128 * 1024 * 1024)? == original,
        "poppler_tool_changed"
    );
    let observed = process_observed4(root, &tool.path, args, limits);
    ensure!(
        file_hash(&tool.path, 128 * 1024 * 1024)? == original,
        "poppler_tool_changed"
    );
    let observed = observed?;
    ensure!(
        observed.exit_code == Some(0),
        "poppler_process_failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    Ok(observed)
}

/// Called only by the registered-page adapter. Actual same-call observations
/// create authority; a public recipe, cache sidecar or DTO cannot create it.
pub(crate) fn render_observed4(
    root: &Path,
    path: &Path,
    page: u64,
    registered_page_count: u64,
    dpi: u32,
    tools: &PopplerTools,
    expected_renderer_path: &Path,
    expected_renderer_sha: &str,
    expected_renderer_bytes: u64,
) -> Result<ObservedRender4> {
    ensure!(
        dpi == 144 && page > 0 && page <= registered_page_count,
        "page_content4_recipe_domain"
    );
    let path = within_root(root, path)?;
    let source = path
        .to_str()
        .context("page_content4_source_path_utf8")?
        .to_owned();
    let original = file_hash(&path, crate::citations::MAX_SOURCE_BYTES)?;
    let limits = PopplerLimits {
        timeout: Duration::from_secs(120),
        ..Default::default()
    };
    let selected_renderer = tool_path(&tools.pdftoppm)?;
    ensure!(
        selected_renderer == expected_renderer_path
            && file_hash(&selected_renderer, 128 * 1024 * 1024)?
                == (expected_renderer_sha.to_owned(), expected_renderer_bytes),
        "page_content4_designated_renderer_identity"
    );
    let (info_tool, info_version) = identify_observed4(root, &tools.pdfinfo, &limits)?;
    let info_limits = PopplerLimits {
        output_bytes: 1024 * 1024,
        ..limits.clone()
    };
    let info = execute_observed4(
        root,
        &info_tool,
        &["-enc".into(), "UTF-8".into(), source.clone()],
        &info_limits,
    )?;
    let info_text = std::str::from_utf8(&info.stdout)?;
    let (mut count, mut encrypted) = (None, None);
    for line in info_text.lines() {
        if let Some(v) = line.strip_prefix("Pages:") {
            ensure!(count.is_none(), "pdfinfo_duplicate_page_count");
            count = Some(v.trim().parse::<u64>()?);
        }
        if let Some(v) = line.strip_prefix("Encrypted:") {
            encrypted = Some(v.trim().starts_with("yes"));
        }
    }
    let count = count.context("source_pdf_page_count_unknown")?;
    ensure!(
        count == registered_page_count
            && count > 0
            && count <= limits.pages as u64
            && encrypted == Some(false),
        "page_content4_registered_page_count_or_encryption"
    );
    let (tool, version) = identify_observed4(root, &tools.pdftoppm, &limits)?;
    ensure!(
        tool.path == expected_renderer_path
            && tool.sha256 == expected_renderer_sha
            && tool.bytes == expected_renderer_bytes,
        "page_content4_designated_renderer_identity"
    );
    let args = vec![
        "-f".into(),
        page.to_string(),
        "-l".into(),
        page.to_string(),
        "-singlefile".into(),
        "-r".into(),
        dpi.to_string(),
        "-png".into(),
        source,
    ];
    let rendered = execute_observed4(root, &tool, &args, &limits)?;
    ensure!(
        rendered.stdout.len() >= 24
            && rendered.stdout[..8] == *b"\x89PNG\r\n\x1a\n"
            && rendered.stdout[12..16] == *b"IHDR",
        "source_view_invalid_image"
    );
    let after = file_hash(&path, crate::citations::MAX_SOURCE_BYTES)?;
    ensure!(
        after == original,
        "page_content4_original_changed_during_render"
    );
    Ok(ObservedRender4 {
        document: oe_core::FileIdentity {
            sha256: original.0,
            bytes: original.1,
        },
        document_after: oe_core::FileIdentity {
            sha256: after.0,
            bytes: after.1,
        },
        document_path: path,
        page,
        page_count: count,
        dpi,
        tool: tool.clone(),
        processes: vec![
            ("tool_version".into(), info_tool.clone(), info_version),
            ("page_count".into(), info_tool, info),
            ("tool_version".into(), tool.clone(), version),
            ("page_render".into(), tool, rendered),
        ],
    })
}
