//! Append-only report delivery. PDF compilation is an explicit bounded lane.
use super::{MAX_INPUT_BYTES, pdf, render};
use anyhow::{Context, Result, ensure};
use oe_core::json::strict_json;
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
const DELIVERY: &str = "ontology-engineering.manufacturing-report-delivery/v1";
const ARTIFACT_LIMIT: usize = 64 * 1024 * 1024;
const PREAMBLE: &str = "skills/manufacturing-process-cost/assets/report-template/preamble.tex";
const CONTRACT: &str = "skills/manufacturing-process-cost/assets/report-template/contract.json";
const PREAMBLE_BYTES: &[u8] = include_bytes!(
    "../../../../skills/manufacturing-process-cost/assets/report-template/preamble.tex"
);
const CONTRACT_BYTES: &[u8] = include_bytes!(
    "../../../../skills/manufacturing-process-cost/assets/report-template/contract.json"
);
const FILES: &[&str] = &[
    "snapshot.json",
    "template.tex",
    "contract.json",
    "report.tex",
    "render-map.json",
];
const PDF_FILES: &[&str] = &[
    "report.pdf",
    "report.log",
    "report.txt",
    "fonts.txt",
    "pdfinfo.txt",
    "tool-runs.json",
];
// Compiled source bytes identify this adapter and the reused path/JSON boundary.
// Cargo/toolchain identities remain the package/build provenance, not a claim
// made by this report receipt. No Python generator executes in this lane.
const SOURCES: &[(&str, &str)] = &[
    (
        "crates/oe-engineering/src/manufacturing_report/mod.rs",
        include_str!("mod.rs"),
    ),
    (
        "crates/oe-engineering/src/manufacturing_report/decimal.rs",
        include_str!("decimal.rs"),
    ),
    (
        "crates/oe-engineering/src/manufacturing_report/render.rs",
        include_str!("render.rs"),
    ),
    (
        "crates/oe-engineering/src/manufacturing_report/publication.rs",
        include_str!("publication.rs"),
    ),
    (
        "crates/oe-engineering/src/manufacturing_report/pdf.rs",
        include_str!("pdf.rs"),
    ),
    (
        "crates/oe-engineering/src/lib.rs",
        include_str!("../lib.rs"),
    ),
    (
        "crates/oe-core/src/lib.rs",
        include_str!("../../../oe-core/src/lib.rs"),
    ),
    (
        "crates/oe-core/src/json.rs",
        include_str!("../../../oe-core/src/json.rs"),
    ),
    (
        "crates/oe-wire/src/lib.rs",
        include_str!("../../../oe-wire/src/lib.rs"),
    ),
    (
        "crates/oe-wire/src/json.rs",
        include_str!("../../../oe-wire/src/json.rs"),
    ),
];
/// Named, compiled adapter/path/parser identities. This is not a compiler or
/// dependency attestation, nor authentication of a declared report author.
pub fn source_identity() -> Value {
    let files: BTreeMap<_, _> = SOURCES
        .iter()
        .map(|(p, s)| (*p, sha256(s)))
        .chain([
            (PREAMBLE, sha256(PREAMBLE_BYTES)),
            (CONTRACT, sha256(CONTRACT_BYTES)),
        ])
        .collect();
    json!(files)
}
fn generator() -> String {
    sha256(oe_core::json::canonical_json(&source_identity()).expect("source identity JSON"))
}
fn policy() -> String {
    sha256(include_str!("../../../oe-core/src/lib.rs"))
}
fn normalized_text(text: &str, windows: bool) -> Result<()> {
    ensure!(
        !text.is_empty() && !text.chars().any(char::is_control),
        "noncanonical_report_path"
    );
    let rest = if windows {
        // Inspect the original spelling. Do not collapse separators or dot
        // components. Windows canonical roots may use a verbatim disk prefix.
        let drive = text.strip_prefix(r"\\?\").unwrap_or(text);
        if drive.as_bytes().get(1) == Some(&b':') {
            ensure!(
                drive.as_bytes()[0].is_ascii_alphabetic(),
                "invalid_drive_prefix"
            );
            let tail = &drive[2..];
            ensure!(
                tail.starts_with(['/', '\\']),
                "drive_relative_path_forbidden"
            );
            tail
        } else {
            ensure!(
                !drive.starts_with(['/', '\\']),
                "root_relative_or_UNC_path_forbidden"
            );
            drive
        }
    } else {
        ensure!(!text.contains('\\'), "noncanonical_report_path");
        text
    };
    ensure!(!rest.contains(':'), "noncanonical_report_path");
    let pieces: Vec<_> = if windows {
        rest.split(['/', '\\']).collect()
    } else {
        rest.split('/').collect()
    };
    for (index, part) in pieces.iter().enumerate() {
        ensure!(!part.is_empty() || index == 0, "noncanonical_report_path");
        ensure!(*part != "." && *part != "..", "noncanonical_report_path");
        if windows && !part.is_empty() {
            ensure!(!part.ends_with(['.', ' ']), "noncanonical_report_path");
        }
    }
    Ok(())
}
fn normalized(value: &Path) -> Result<()> {
    normalized_text(value.to_str().context("path_requires_UTF8")?, cfg!(windows))
}
fn input_path(root: &SkillRoot, value: &Path) -> Result<PathBuf> {
    normalized(value)?;
    let path = root.expand(value)?;
    ensure!(
        path.starts_with(root.path()),
        "report_input_outside_skill_root"
    );
    oe_core::no_symlinks(&path)?;
    Ok(path)
}
fn directory_path(root: &SkillRoot, value: &Path) -> Result<PathBuf> {
    normalized(value)?;
    let path = root.private_path(value)?;
    Ok(path)
}
fn check_sources(root: &SkillRoot) -> Result<()> {
    for (path, source) in SOURCES {
        ensure!(
            read_regular_bytes(&root.path().join(path), MAX_INPUT_BYTES)?.as_slice()
                == source.as_bytes(),
            "report_runtime_source_drift:{path}"
        );
    }
    Ok(())
}
fn assets(root: &SkillRoot) -> Result<(Vec<u8>, Vec<u8>)> {
    let template = read_regular_bytes(&root.path().join(PREAMBLE), MAX_INPUT_BYTES)?;
    let contract = read_regular_bytes(&root.path().join(CONTRACT), MAX_INPUT_BYTES)?;
    ensure!(
        template == PREAMBLE_BYTES && contract == CONTRACT_BYTES,
        "report_template_contract_source_drift"
    );
    Ok((template, contract))
}
fn encoded(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn exact_keys(value: &Value, keys: &[&str], scope: &str) -> Result<()> {
    let map = value
        .as_object()
        .with_context(|| format!("invalid_{scope}"))?;
    ensure!(
        map.len() == keys.len() && keys.iter().all(|k| map.contains_key(*k)),
        "invalid_{scope}_fields"
    );
    Ok(())
}
fn sha_text(v: &Value) -> Result<&str> {
    let s = v.as_str().context("invalid_sha256")?;
    ensure!(
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid_sha256"
    );
    Ok(s)
}
fn base_result(
    issues: Vec<String>,
    s: &Value,
    map: &Value,
    input_sha: &str,
    compiled: bool,
    pages: Option<u64>,
    pdf_sha: Option<String>,
) -> Value {
    let status = match (issues.is_empty(), compiled) {
        (true, true) => "pdf_technically_verified",
        (true, false) => "tex_verified",
        (false, true) => "pdf_verification_failed",
        (false, false) => "tex_verification_failed",
    };
    let scope = if compiled {
        "Frozen snapshot/render correspondence and PDF text, fonts, page count and diagnostics only. Engineering semantics, source truth and visual review are separate."
    } else {
        "Frozen snapshot/TeX/render-map correspondence and supplied arithmetic only. PDF compilation, engineering semantics, source truth and visual review are separate."
    };
    json!({
        "passed": issues.is_empty(), "status": status, "issues": issues,
        "views": map["views"].as_array().map_or(0, Vec::len),
        "text_expectations": map["expected_text"].as_array().map_or(0, Vec::len),
        "pages": pages, "compiled": compiled, "input_sha256": input_sha,
        "pdf_sha256": pdf_sha, "semantic_status": s["semantic_status"],
        "visual_review": "not_performed_by_this_module", "scope": scope,
    })
}
/// Publish a fresh TeX-only delivery beneath this skill's var directory. Input
/// bytes and trusted assets are captured before output creation. A failed
/// publication remains partial evidence; this function never cleans or retries.
pub fn generate(root: &SkillRoot, input: &Path, output: &Path) -> Result<Value> {
    generate_with_tools(root, input, output, None)
}

/// Compile a new report with bounded document tools. The optional lane leaves
/// the existing TeX-only command independent of any system document tools.
pub fn generate_with_tools(
    root: &SkillRoot,
    input: &Path,
    output: &Path,
    tools: Option<&pdf::PdfTools>,
) -> Result<Value> {
    check_sources(root)?;
    let input = input_path(root, input)?;
    let output = directory_path(root, output)?;
    let snapshot = read_regular_bytes(&input, MAX_INPUT_BYTES)?;
    let value = strict_json(&snapshot)?;
    let (template, contract) = assets(root)?;
    let rendered = render(
        &value,
        std::str::from_utf8(&template).context("template_not_UTF8")?,
    )?;
    let mut payload = vec![
        ("snapshot.json", snapshot),
        ("template.tex", template),
        ("contract.json", contract),
        ("report.tex", rendered.tex.into_bytes()),
        ("render-map.json", encoded(&rendered.map)?),
    ];
    ensure!(
        payload.iter().all(|(_, b)| b.len() <= ARTIFACT_LIMIT),
        "report_artifact_exceeds_limit"
    );
    // All interpretation and source checks precede this create-new side effect.
    check_sources(root)?;
    assets(root)?;
    root.new_private_dir(&output)?;
    for (name, bytes) in &payload {
        write_new_bytes(&output.join(name), bytes)?;
    }
    if let Some(tools) = tools {
        let compiled = pdf::compile(&output, &payload[3].1, &rendered.map, tools)?;
        for (name, bytes) in compiled {
            ensure!(
                bytes.len() <= ARTIFACT_LIMIT,
                "report_artifact_exceeds_limit"
            );
            write_new_bytes(&output.join(name), &bytes)?;
            payload.push((name, bytes));
        }
    }
    let manifest = json!({"format":DELIVERY,"generator_sha256":generator(),"local_paths_sha256":policy(),"generator_source_files":source_identity(),"input_sha256":sha256(&payload[0].1),"files":payload.iter().map(|(name,b)|json!({"path":name,"sha256":sha256(b)})).collect::<Vec<_>>(),"document_id":value["document"]["id"],"revision":value["document"]["revision"],"compiled":tools.is_some(),"semantic_status":value["semantic_status"],"scope":"Frozen report artifact, not engineering approval or a semantic receipt."});
    write_new(&output.join("manifest.json"), &manifest)?;
    let result = verify_with_tools(root, &output, tools.unwrap_or(&pdf::PdfTools::default()))?;
    ensure!(
        result["passed"] == true,
        "published_report_failed_verification"
    );
    write_new(&output.join("verification.json"), &result)?;
    Ok(result)
}
/// Recompute a captured delivery; compiled reports also require native PDF
/// inspection. Rehashed altered TeX/map bytes still fail recomputation.
pub fn verify(root: &SkillRoot, directory: &Path) -> Result<Value> {
    verify_with_tools(root, directory, &pdf::PdfTools::default())
}

pub fn verify_with_tools(
    root: &SkillRoot,
    directory: &Path,
    tools: &pdf::PdfTools,
) -> Result<Value> {
    check_sources(root)?;
    let directory = directory_path(root, directory)?;
    ensure!(
        fs::symlink_metadata(&directory)?.is_dir(),
        "report_directory_required"
    );
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("report_member_not_UTF8"))?;
        ensure!(
            entry.file_type()?.is_file(),
            "report_member_not_regular_file"
        );
        ensure!(names.insert(name), "duplicate_report_member");
    }
    let allowed: BTreeSet<String> = FILES
        .iter()
        .copied()
        .chain(PDF_FILES.iter().copied())
        .chain(["manifest.json", "verification.json"])
        .map(str::to_owned)
        .collect();
    ensure!(names.is_subset(&allowed), "unexpected_report_artifact");
    let manifest_bytes = read_regular_bytes(&directory.join("manifest.json"), MAX_INPUT_BYTES)?;
    let manifest = strict_json(&manifest_bytes)?;
    exact_keys(
        &manifest,
        &[
            "format",
            "generator_sha256",
            "local_paths_sha256",
            "generator_source_files",
            "input_sha256",
            "files",
            "document_id",
            "revision",
            "compiled",
            "semantic_status",
            "scope",
        ],
        "report_manifest",
    )?;
    ensure!(
        manifest["format"] == DELIVERY,
        "unsupported_delivery_format"
    );
    let compiled = manifest["compiled"]
        .as_bool()
        .context("compiled_must_be_boolean")?;
    ensure!(
        sha_text(&manifest["generator_sha256"])? == generator(),
        "use_archived_Rust_generator_version"
    );
    ensure!(
        sha_text(&manifest["local_paths_sha256"])? == policy(),
        "report_path_policy_identity_changed"
    );
    ensure!(
        manifest["generator_source_files"] == source_identity(),
        "report_source_closure_identity_changed"
    );
    let inventory = manifest["files"]
        .as_array()
        .context("manifest_files_required")?;
    let mut listed = BTreeSet::new();
    let mut payload = BTreeMap::new();
    let mut issues = Vec::new();
    for row in inventory {
        exact_keys(row, &["path", "sha256"], "manifest_file")?;
        let name = row["path"].as_str().context("manifest_member_name")?;
        ensure!(
            (FILES.contains(&name) || compiled && PDF_FILES.contains(&name)) && listed.insert(name),
            "delivery_file_inventory_differs"
        );
        let expected = sha_text(&row["sha256"])?;
        match read_regular_bytes(
            &directory.join(name),
            if name == "snapshot.json" {
                MAX_INPUT_BYTES
            } else {
                ARTIFACT_LIMIT
            },
        ) {
            Ok(bytes) => {
                if sha256(&bytes) != expected {
                    issues.push(format!("file changed: {name}"));
                }
                payload.insert(name, bytes);
            }
            Err(_) => issues.push(format!("file missing or unsafe: {name}")),
        }
    }
    ensure!(
        listed
            == FILES
                .iter()
                .copied()
                .chain(if compiled { PDF_FILES } else { &[] }.iter().copied())
                .collect(),
        "delivery_file_inventory_differs"
    );
    if !issues.is_empty() {
        return Ok(
            json!({"passed":false,"status":if compiled{"pdf_verification_failed"}else{"tex_verification_failed"},"issues":issues,"compiled":compiled,"scope":"Report artifact checks only; no engineering acceptance."}),
        );
    }
    let source = &payload["snapshot.json"];
    let input_sha = sha_text(&manifest["input_sha256"])?;
    ensure!(sha256(source) == input_sha, "snapshot_identity_mismatch");
    let snapshot = strict_json(source)?;
    ensure!(
        payload["template.tex"] == PREAMBLE_BYTES && payload["contract.json"] == CONTRACT_BYTES,
        "archived_template_contract_not_bound_to_generator"
    );
    strict_json(&payload["contract.json"])?;
    let rendered = render(
        &snapshot,
        std::str::from_utf8(&payload["template.tex"]).context("template_not_UTF8")?,
    )?;
    ensure!(
        manifest["document_id"] == snapshot["document"]["id"]
            && manifest["revision"] == snapshot["document"]["revision"]
            && manifest["semantic_status"] == snapshot["semantic_status"],
        "report_manifest_snapshot_metadata_mismatch"
    );
    ensure!(
        manifest["scope"]
            == "Frozen report artifact, not engineering approval or a semantic receipt.",
        "report_scope_changed"
    );
    if payload["report.tex"] != rendered.tex.as_bytes() {
        issues.push("generated TeX does not correspond to this snapshot".into());
    }
    if strict_json(&payload["render-map.json"])? != rendered.map {
        issues.push("render map does not correspond to this snapshot".into());
    }
    let mut pages = None;
    let mut pdf_sha = None;
    if compiled {
        let runs = strict_json(&payload["tool-runs.json"])?;
        ensure!(
            runs["schema"] == "ontology-engineering.report-tool-runs/v1"
                && runs["runs"]
                    .as_array()
                    .is_some_and(|r| r.len() == 7
                        && r.iter().all(|item| item["status"] == "completed"
                            && item["capture_complete"] == true)),
            "report_tool_run_receipts_invalid"
        );
        let (pdf_issues, count) = pdf::verify(root, &directory, &rendered.map, tools)?;
        issues.extend(pdf_issues);
        pages = count;
        pdf_sha = Some(sha256(&payload["report.pdf"]));
    }
    let mut result = base_result(
        issues,
        &snapshot,
        &rendered.map,
        input_sha,
        compiled,
        pages,
        pdf_sha,
    );
    if names.contains("verification.json") {
        let archived = strict_json(&read_regular_bytes(
            &directory.join("verification.json"),
            MAX_INPUT_BYTES,
        )?)?;
        if archived != result {
            result["passed"] = json!(false);
            result["status"] = json!(if compiled {
                "pdf_verification_failed"
            } else {
                "tex_verification_failed"
            });
            result["issues"].as_array_mut().unwrap().push(json!(
                "stored verification differs from independent recomputation"
            ));
        }
    }
    let current_names = fs::read_dir(&directory)?
        .map(|entry| {
            entry?
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("report_member_not_UTF8"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if current_names != names
        || read_regular_bytes(&directory.join("manifest.json"), MAX_INPUT_BYTES)? != manifest_bytes
        || payload.iter().any(|(name, original)| {
            read_regular_bytes(
                &directory.join(name),
                if *name == "snapshot.json" {
                    MAX_INPUT_BYTES
                } else {
                    ARTIFACT_LIMIT
                },
            )
            .map_or(true, |current| current != *original)
        })
    {
        result["passed"] = json!(false);
        result["status"] = json!(if compiled {
            "pdf_verification_failed"
        } else {
            "tex_verification_failed"
        });
        result["issues"]
            .as_array_mut()
            .unwrap()
            .push(json!("report changed during verification"));
    }
    // Current implementation and selected directory are checked again before a
    // success is returned; archived verification.json never grants authority.
    check_sources(root)?;
    Ok(result)
}

#[cfg(test)]
mod path_tests {
    use super::normalized_text;
    #[test]
    fn unix_input_spelling_rejects_aliases_without_normalizing_them() {
        for p in [
            "var/report",
            "/Volumes/fixture/skill/var/report",
            "var/中文报告",
        ] {
            normalized_text(p, false).unwrap();
        }
        for p in [
            "var/../report",
            "var/./report",
            "var//report",
            "var/report/",
            r"var\report",
            "C:/report",
            "var/report\n",
        ] {
            assert!(normalized_text(p, false).is_err(), "{p}");
        }
    }
    #[test]
    fn windows_native_separators_and_canonical_disk_prefix_remain_usable() {
        for p in [
            r"var\report",
            "var/report",
            r"C:\Work\example\skill\var\report",
            r"\\?\C:\Work\example\skill\var\report",
        ] {
            normalized_text(p, true).unwrap();
        }
        for p in [
            r"var\..\report",
            r"var\.\report",
            r"var\\report",
            r"C:relative",
            r"\root_relative",
            r"\\server\share\report",
            r"C:\Work\x\report.",
            r"C:\Work\x\report ",
            r"C:\Work\x\report:stream",
        ] {
            assert!(normalized_text(p, true).is_err(), "{p}");
        }
    }
}
