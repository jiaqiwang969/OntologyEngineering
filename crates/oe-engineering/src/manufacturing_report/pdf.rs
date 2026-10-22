//! Bounded XeLaTeX and Poppler boundary for an explicit report snapshot.
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command_memory};
use oe_core::{SkillRoot, create_new_private_dir, read_regular_bytes, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const LIMIT: usize = 64 * 1024 * 1024;
const DIAGNOSTICS: &[&str] = &[
    "Missing character",
    "Overfull \\hbox",
    "Overfull \\vbox",
    "Underfull \\hbox",
    "Underfull \\vbox",
    "Undefined control sequence",
    "LaTeX Error",
    "undefined references",
];
static VERIFY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct PdfTools {
    pub xelatex: PathBuf,
    pub pdftotext: PathBuf,
    pub pdffonts: PathBuf,
    pub pdfinfo: PathBuf,
}

impl Default for PdfTools {
    fn default() -> Self {
        Self {
            xelatex: "xelatex".into(),
            pdftotext: "pdftotext".into(),
            pdffonts: "pdffonts".into(),
            pdfinfo: "pdfinfo".into(),
        }
    }
}

fn checked_tools(tools: &PdfTools, compile: bool) -> Result<PdfTools> {
    let resolve = oe_core::process::resolve_executable;
    Ok(PdfTools {
        xelatex: if compile {
            resolve(&tools.xelatex)?
        } else {
            tools.xelatex.clone()
        },
        pdftotext: resolve(&tools.pdftotext)?,
        pdffonts: resolve(&tools.pdffonts)?,
        pdfinfo: resolve(&tools.pdfinfo)?,
    })
}

fn run(tool: &Path, args: &[&str], cwd: &Path, evidence: &Path) -> Result<(Vec<u8>, Value)> {
    let observation = run_command_memory(
        &CommandSpec {
            executable: tool.to_path_buf(),
            arguments: args.iter().map(OsString::from).collect(),
            cwd: cwd.to_path_buf(),
            environment: BTreeMap::new(),
            remove_environment: Vec::new(),
            timeout: Duration::from_secs(180),
            output_limit: 4 * 1024 * 1024,
        },
        evidence,
    )?;
    ensure!(
        observation.status == "completed"
            && observation.exit_code == Some(0)
            && observation.report["capture_complete"] == true,
        "report_document_tool_failed:{}:{}",
        tool.display(),
        evidence.display()
    );
    Ok((observation.stdout, observation.report))
}

fn normalized(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

pub fn technical_checks(
    map: &Value,
    extracted: &str,
    log: &str,
    fonts: &str,
    info: &str,
) -> (Vec<String>, Option<u64>) {
    let mut issues = Vec::new();
    let text = normalized(extracted);
    if let Some(items) = map["expected_text"].as_array() {
        for item in items {
            if let Some(expected) = item.as_str()
                && !text.contains(&normalized(expected))
            {
                issues.push(format!(
                    "expected text missing: {}",
                    expected.chars().take(100).collect::<String>()
                ));
            }
        }
    } else {
        issues.push("expected_text_missing_from_render_map".into());
    }
    for diagnostic in DIAGNOSTICS {
        if log.contains(diagnostic) {
            issues.push(format!("XeLaTeX diagnostic: {diagnostic}"));
        }
    }
    if extracted.contains("??") {
        issues.push("unresolved PDF cross reference".into());
    }
    let font_rows: Vec<_> = fonts
        .lines()
        .skip(2)
        .filter(|s| !s.trim().is_empty())
        .collect();
    if font_rows.is_empty()
        || font_rows.iter().any(|line| {
            let columns: Vec<_> = line.split_whitespace().collect();
            columns.len() < 7 || columns[columns.len() - 5] != "yes"
        })
    {
        issues.push("font not embedded".into());
    }
    let pages = info.lines().find_map(|line| {
        line.strip_prefix("Pages:")
            .and_then(|rest| rest.trim().parse::<u64>().ok())
    });
    if pages.is_none_or(|n| n == 0) {
        issues.push("PDF page count missing or zero".into());
    }
    (issues, pages)
}

/// Create compiled files from the already rendered TeX. A failed build leaves
/// its private work directory for diagnosis; no final manifest is written.
pub fn compile(
    directory: &Path,
    tex: &[u8],
    map: &Value,
    tools: &PdfTools,
) -> Result<Vec<(&'static str, Vec<u8>)>> {
    let tools = checked_tools(tools, true)?;
    let work = directory.join(".compile-work");
    create_new_private_dir(&work)?;
    write_new_bytes(&work.join("report.tex"), tex)?;
    let mut observations = Vec::new();
    for index in 1..=3 {
        let (_, receipt) = run(
            &tools.xelatex,
            &[
                "-no-shell-escape",
                "-interaction=nonstopmode",
                "-halt-on-error",
                "-file-line-error",
                "report.tex",
            ],
            &work,
            &work.join(format!("xelatex-{index}")),
        )?;
        observations.push(receipt);
    }
    let (layout, receipt) = run(
        &tools.pdftotext,
        &["-layout", "report.pdf", "-"],
        &work,
        &work.join("pdftotext-layout"),
    )?;
    observations.push(receipt);
    let (raw, receipt) = run(
        &tools.pdftotext,
        &["-raw", "report.pdf", "-"],
        &work,
        &work.join("pdftotext-raw"),
    )?;
    observations.push(receipt);
    let (fonts, receipt) = run(
        &tools.pdffonts,
        &["report.pdf"],
        &work,
        &work.join("pdffonts"),
    )?;
    observations.push(receipt);
    let (info, receipt) = run(
        &tools.pdfinfo,
        &["report.pdf"],
        &work,
        &work.join("pdfinfo"),
    )?;
    observations.push(receipt);
    let raw = String::from_utf8(raw).context("report_pdf_text_not_UTF8")?;
    let fonts = String::from_utf8(fonts).context("report_fonts_not_UTF8")?;
    let info = String::from_utf8(info).context("report_info_not_UTF8")?;
    let log = read_regular_bytes(&work.join("report.log"), LIMIT)?;
    let log_text = String::from_utf8_lossy(&log);
    let (issues, _) = technical_checks(map, &raw, &log_text, &fonts, &info);
    ensure!(
        issues.is_empty(),
        "report_PDF_technical_check_failed:{issues:?}"
    );
    let pdf = read_regular_bytes(&work.join("report.pdf"), LIMIT)?;
    ensure!(pdf.starts_with(b"%PDF-"), "report_PDF_header_missing");
    let runs = serde_json::to_vec_pretty(
        &json!({"schema":"ontology-engineering.report-tool-runs/v1","runs":observations}),
    )?;
    let result = vec![
        ("report.pdf", pdf),
        ("report.log", log),
        ("report.txt", layout),
        ("fonts.txt", fonts.into_bytes()),
        ("pdfinfo.txt", info.into_bytes()),
        ("tool-runs.json", runs),
    ];
    fs::remove_dir_all(&work)?;
    Ok(result)
}

fn verify_scratch(root: &SkillRoot) -> Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let sequence = VERIFY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    root.new_private_dir(&root.path().join(format!(
        "var/maintenance/report-pdf-verify-{}-{nanos}-{sequence}",
        std::process::id()
    )))
}

/// Re-run Poppler against the captured PDF. Stored tool receipts are evidence,
/// not authority for this check.
pub fn verify(
    root: &SkillRoot,
    directory: &Path,
    map: &Value,
    tools: &PdfTools,
) -> Result<(Vec<String>, Option<u64>)> {
    let tools = checked_tools(tools, false)?;
    let scratch = verify_scratch(root)?;
    let pdf = directory.join("report.pdf");
    let pdf = pdf.to_str().context("report_PDF_path_not_UTF8")?;
    let result = (|| {
        let (raw, _) = run(
            &tools.pdftotext,
            &["-raw", pdf, "-"],
            &scratch,
            &scratch.join("raw"),
        )?;
        let (layout, _) = run(
            &tools.pdftotext,
            &["-layout", pdf, "-"],
            &scratch,
            &scratch.join("layout"),
        )?;
        let (fonts, _) = run(&tools.pdffonts, &[pdf], &scratch, &scratch.join("fonts"))?;
        let (info, _) = run(&tools.pdfinfo, &[pdf], &scratch, &scratch.join("info"))?;
        let raw = String::from_utf8(raw).context("report_pdf_text_not_UTF8")?;
        let fonts = String::from_utf8(fonts).context("report_fonts_not_UTF8")?;
        let info = String::from_utf8(info).context("report_info_not_UTF8")?;
        let log = read_regular_bytes(&directory.join("report.log"), LIMIT)?;
        let log = String::from_utf8_lossy(&log);
        let (mut issues, pages) = technical_checks(map, &raw, &log, &fonts, &info);
        if read_regular_bytes(&directory.join("report.txt"), LIMIT)? != layout {
            issues.push("stored PDF text differs from current extraction".into());
        }
        let archived = read_regular_bytes(&directory.join("report.pdf"), LIMIT)?;
        if !archived.starts_with(b"%PDF-") {
            issues.push("report PDF header missing".into());
        }
        Ok((issues, pages))
    })();
    fs::remove_dir_all(&scratch)?;
    result
}
