//! Offline browser checks of the review page. Every expected set is computed in the browser from the graph
//! embedded in the page; no answers are injected. A check that needs an element the drawing does not have (for
//! example a bounded shared contour source) is listed under facts.not_applicable instead of failing; where the
//! element exists the check runs unchanged. The complete structure draws each structural line once as one
//! element (inferred spans are part of their line, never a separate overlay). Chrome runs headless with a fresh
//! profile under a time limit; its result is read from the dumped page.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::SkillRoot;
use oe_core::process::{CommandSpec, ProcessObservation, run_command};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

pub const SCHEMA: &str = "oev2.drawing-review-page-check/1";
pub const REQUEST_SCHEMA: &str = "oev2.drawing-review-page-check-request/1";
pub const MODES: [&str; 7] = [
    "kind", "line", "view", "geometry", "focus", "complete", "asdrawn",
];
const CHECKS: &str = include_str!("../assets/review-page-checks.js");

pub struct ReviewPageChecker<'a> {
    pub root: &'a SkillRoot,
}
impl MdReviewPageCheckPort for ReviewPageChecker<'_> {
    fn check_page(
        &self,
        input: &MdReviewPageCheckRequest,
    ) -> std::result::Result<MdReviewPageCheck, FailureRecord> {
        check_page(self.root, input)
            .map_err(|e| crate::graph::failure("drawing_review_page_check", e))
    }
}

/// A local file as a file URL, bytes outside the unreserved set percent-encoded.
pub fn file_url(path: &Path) -> String {
    let mut out = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn escape_attribute(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// The harness page: the review page in a frame, checked when it has loaded. Only the page location, the
/// display mode, the focus label and the screenshot region are substituted.
pub fn harness(
    page_url: &str,
    mode: &str,
    focus_label: Option<&str>,
    region: Option<&[f64]>,
) -> Result<String> {
    ensure!(MODES.contains(&mode), "review_page_check_mode:{mode}");
    let focus = serde_json::to_string(focus_label.unwrap_or(""))?;
    let region = match region {
        Some(r) => {
            ensure!(
                r.len() == 4 && r.iter().all(|x| x.is_finite()),
                "review_page_check_region"
            );
            serde_json::to_string(r)?
        }
        None => "null".into(),
    };
    let checks = CHECKS
        .replace("__MODE__", mode)
        .replace("__FOCUS__", &focus)
        .replace("__BOX__", &region);
    Ok(format!(
        "<!doctype html><meta charset=\"utf-8\"><style>body{{margin:0}}iframe{{border:0;width:100%;height:100vh}}pre{{display:none}}</style><pre id=\"checks\">pending</pre><iframe id=\"f\" src=\"{}\"></iframe><script>{checks}</script>",
        escape_attribute(page_url)
    ))
}

/// Text of an HTML text node as a serializer writes it.
fn unescape(s: &str) -> Result<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').context("review_page_check_entity")?;
        let entity = &tail[1..end];
        let c = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => '\u{a0}',
            e if e.starts_with("#x") || e.starts_with("#X") => {
                char::from_u32(u32::from_str_radix(&e[2..], 16)?)
                    .context("review_page_check_entity")?
            }
            e if e.starts_with('#') => {
                char::from_u32(e[1..].parse()?).context("review_page_check_entity")?
            }
            e => anyhow::bail!("review_page_check_entity_unknown:{e}"),
        };
        out.push(c);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The browser result written into the harness: present, well formed and passed, or an error that keeps the
/// browser's own message.
pub fn read_result(dom: &str) -> Result<Value> {
    let open = "<pre id=\"checks\">";
    let start = dom.find(open).context("review_page_check_result_missing")? + open.len();
    let end = start
        + dom[start..]
            .find("</pre>")
            .context("review_page_check_result_unterminated")?;
    let result: Value = serde_json::from_str(&unescape(&dom[start..end])?)
        .context("review_page_check_result_not_json")?;
    ensure!(
        result["status"] == "passed",
        "review_page_check_failed:{}",
        result["error"].as_str().unwrap_or("no result")
    );
    ensure!(
        result["count"].as_u64().is_some_and(|n| n > 0)
            && result["checks"]
                .as_array()
                .is_some_and(|c| c.len() as u64 == result["count"].as_u64().unwrap_or(0)),
        "review_page_check_result_count"
    );
    Ok(result)
}

fn run(
    program: &Path,
    arguments: Vec<OsString>,
    evidence: &Path,
    cwd: &Path,
    seconds: u64,
) -> Result<ProcessObservation> {
    let environment: BTreeMap<OsString, OsString> = [
        ("PATH".into(), "/usr/bin:/bin".into()),
        (
            "HOME".into(),
            std::env::var_os("HOME").unwrap_or_else(|| "/".into()),
        ),
    ]
    .into_iter()
    .collect();
    let remove_environment = std::env::vars_os()
        .map(|(k, _)| k)
        .filter(|k| !environment.contains_key(k))
        .collect();
    let spec = CommandSpec {
        executable: program.to_owned(),
        arguments,
        cwd: cwd.to_owned(),
        environment,
        remove_environment,
        timeout: Duration::from_secs(seconds),
        output_limit: 64 * 1024 * 1024,
    };
    let observed = run_command(&spec, evidence)?;
    ensure!(
        observed.status == "completed" && observed.exit_code == Some(0),
        "review_page_check_browser_exited:{:?}:{}",
        observed.exit_code,
        observed.status
    );
    Ok(observed)
}

pub fn check_page(root: &SkillRoot, q: &MdReviewPageCheckRequest) -> Result<MdReviewPageCheck> {
    ensure!(
        q.validate().is_ok() && q.schema == REQUEST_SCHEMA,
        "review_page_check_contract"
    );
    let page = std::fs::canonicalize(root.private_path(Path::new(&q.page))?)?;
    let out = root.new_private_dir(Path::new(&q.output_directory))?;
    let chrome = root.data_expand(Path::new(&q.chrome))?;
    let base = root.data_path();
    let version = run(
        &chrome,
        vec!["--version".into()],
        &out.join("browser-version"),
        &out,
        60,
    )?;
    let browser = String::from_utf8_lossy(&version.stdout).trim().to_owned();
    let harness_path = out.join("harness.html");
    let text = harness(
        &file_url(&page),
        &q.mode,
        q.focus_label.as_deref(),
        q.region.as_deref(),
    )?;
    oe_core::write_new_bytes(&harness_path, text.as_bytes())?;
    let arguments: Vec<OsString> = vec![
        "--headless=new".into(),
        "--disable-gpu".into(),
        "--no-first-run".into(),
        "--disable-background-networking".into(),
        "--allow-file-access-from-files".into(),
        format!("--user-data-dir={}", out.join("chrome-profile").display()).into(),
        "--window-size=1800,1250".into(),
        "--virtual-time-budget=40000".into(),
        format!("--screenshot={}", out.join("screenshot.png").display()).into(),
        "--dump-dom".into(),
        file_url(&harness_path).into(),
    ];
    let observed = run(&chrome, arguments, &out.join("browser-process"), &out, 120)?;
    oe_core::write_new_bytes(&out.join("dom.html"), &observed.stdout)?;
    let result = read_result(&String::from_utf8_lossy(&observed.stdout))?;
    oe_core::write_new_bytes(
        &out.join("checks.json"),
        &serde_json::to_vec_pretty(&result)?,
    )?;
    ensure!(
        result["mode"] == q.mode.as_str(),
        "review_page_check_mode_mismatch"
    );
    let report = MdReviewPageCheck {
        schema: SCHEMA.into(),
        status: "passed".into(),
        mode: q.mode.clone(),
        count: result["count"].as_u64().unwrap_or(0),
        browser,
        page: crate::file(Some(base), &page)?,
        result: crate::file(Some(base), &out.join("checks.json"))?,
        screenshot: crate::file(Some(base), &out.join("screenshot.png"))?,
        facts: result["facts"].clone(),
    };
    ensure!(
        report.validate().is_ok(),
        "review_page_check_report_contract"
    );
    oe_core::write_new_bytes(
        &out.join("report.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
