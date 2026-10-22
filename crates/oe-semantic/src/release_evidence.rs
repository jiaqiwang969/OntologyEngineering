//! Complete, source-bound local book evidence. No owned Python collector runs.
//! Raw observations are retained privately; these documents do not grant release.
use anyhow::{Context, Result, bail, ensure};
use oe_core::process::{CommandSpec, ProcessObservation, resolve_executable, run_command};
use oe_core::{
    SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256, write_new,
    write_new_bytes,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::book_release::{
    self as release, BOOK_SPECS, EVIDENCE_PATHS, REGRESSION_LOG_PATHS, SOURCE_LOCK_PATH,
    VISUAL_REVIEW_PATH, artifact_ref, exact_keys, verify_artifact_ref,
};
const SCHEMA: &str = "ontology-engineering.release-evidence/v2";
const REGRESSION_SCHEMA: &str = "ontology-engineering.regression-evidence/v3";
const RAW_SCHEMA: &str = "ontology-engineering.release-observation/v2";
const LIMIT: usize = 32 * 1024 * 1024;
const CODE_PATH: &str = "crates/oe-semantic/src/release_evidence.rs";
const CRITERIA_SOURCES: &[(&str, &[u8])] = &[
    (CODE_PATH, include_bytes!("release_evidence.rs")),
    (
        "crates/oe-semantic/src/book_sources.rs",
        include_bytes!("book_sources.rs"),
    ),
    (
        "crates/oe-semantic/src/books.rs",
        include_bytes!("books.rs"),
    ),
    (
        "crates/oe-semantic/src/native.rs",
        include_bytes!("native.rs"),
    ),
    (
        "crates/oe-semantic/src/runtime.rs",
        include_bytes!("runtime.rs"),
    ),
    (
        "crates/oe-semantic/src/release_sources.rs",
        include_bytes!("release_sources.rs"),
    ),
    (
        "crates/oe-semantic/src/book_release.rs",
        include_bytes!("book_release.rs"),
    ),
    (
        "crates/oe-core/src/process.rs",
        include_bytes!("../../oe-core/src/process.rs"),
    ),
    (
        "crates/oe-core/src/privacy.rs",
        include_bytes!("../../oe-core/src/privacy.rs"),
    ),
    (
        "crates/oe-core/src/json.rs",
        include_bytes!("../../oe-core/src/json.rs"),
    ),
    (
        "crates/oe-core/src/lib.rs",
        include_bytes!("../../oe-core/src/lib.rs"),
    ),
    (
        "crates/oe-wire/Cargo.toml",
        include_bytes!("../../oe-wire/Cargo.toml"),
    ),
    (
        "crates/oe-wire/src/lib.rs",
        include_bytes!("../../oe-wire/src/lib.rs"),
    ),
    (
        "crates/oe-wire/src/json.rs",
        include_bytes!("../../oe-wire/src/json.rs"),
    ),
    (
        "crates/oe-wire/src/unicode_casefold.rs",
        include_bytes!("../../oe-wire/src/unicode_casefold.rs"),
    ),
];
const BOOK_LOCKS: &[(&str, &str)] = &[
    (
        "references/ontology-engineering-book/",
        "authoring-sources.sha256",
    ),
    (
        "references/product-trustworthiness-book/",
        "handbook/current-source.sha256",
    ),
    (
        "references/product-trustworthiness-book/",
        "handbook/authoring-assets.sha256",
    ),
    (
        "references/product-trustworthiness-book/",
        "handbook/formal-search-guides.sha256",
    ),
];

#[derive(Debug, Clone)]
pub struct EvidenceOptions {
    pub output: PathBuf,
    pub git: PathBuf,
    pub python: PathBuf,
    pub cargo: PathBuf,
    pub pdfinfo: PathBuf,
    pub pdffonts: PathBuf,
    pub pdftotext: PathBuf,
    pub semantica_root: Option<PathBuf>,
    pub visual_review: Option<PathBuf>,
    pub oe_source_commit: Option<String>,
    pub timeout: Duration,
    pub stream_limit: usize,
}
impl EvidenceOptions {
    pub fn new(output: PathBuf) -> Self {
        Self {
            output,
            git: "git".into(),
            python: "runtime/.venv/bin/python".into(),
            cargo: "cargo".into(),
            pdfinfo: "pdfinfo".into(),
            pdffonts: "pdffonts".into(),
            pdftotext: "pdftotext".into(),
            semantica_root: None,
            visual_review: None,
            oe_source_commit: None,
            timeout: Duration::from_secs(7200),
            stream_limit: LIMIT,
        }
    }
}

fn relative(root: &SkillRoot, path: &Path) -> Result<String> {
    let value = path
        .strip_prefix(root.path())
        .context("evidence_path_outside_root")?
        .to_str()
        .context("evidence_path_not_utf8")?;
    crate::safe_relative(value)?;
    Ok(value.to_owned())
}
fn reference(root: &SkillRoot, path: &Path) -> Result<Value> {
    artifact_ref(root, &relative(root, path)?)
}
fn load_ref(root: &SkillRoot, value: &Value) -> Result<Vec<u8>> {
    let path = verify_artifact_ref(root, value, "evidence_artifact")?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(
        sha256(&bytes) == value["sha256"],
        "evidence_read_identity_changed"
    );
    Ok(bytes)
}
fn tool(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    resolve_executable(&if path.components().count() > 1 {
        root.expand(path)?
    } else {
        path.to_owned()
    })
}
fn tool_record(root: &SkillRoot, path: &Path) -> Result<Value> {
    let invocation = tool(root, path)?;
    let target = fs::canonicalize(&invocation)?;
    let identity = fingerprint_file(&target, 512 * 1024 * 1024)?;
    Ok(
        json!({"invocation":invocation,"target":target,"sha256":identity.sha256,"size_bytes":identity.bytes}),
    )
}
fn check_tool(record: &Value) -> Result<()> {
    exact_keys(
        record,
        &["invocation", "target", "sha256", "size_bytes"],
        "evidence_tool",
    )?;
    let invocation = Path::new(record["invocation"].as_str().context("tool_path_missing")?);
    let target = Path::new(record["target"].as_str().context("tool_target_missing")?);
    ensure!(
        fs::canonicalize(invocation)? == target,
        "evidence_tool_target_changed"
    );
    let identity = fingerprint_file(target, 512 * 1024 * 1024)?;
    ensure!(
        record["sha256"] == identity.sha256 && record["size_bytes"] == identity.bytes,
        "evidence_tool_bytes_changed"
    );
    Ok(())
}
fn tools_identity(records: &BTreeMap<String, Value>) -> Value {
    json!(
        records
            .iter()
            .map(|(k, v)| (
                k.clone(),
                json!({"sha256":v["sha256"],"size_bytes":v["size_bytes"]})
            ))
            .collect::<BTreeMap<_, _>>()
    )
}
fn pin(root: &SkillRoot, paths: &BTreeSet<String>) -> Result<Value> {
    let mut result = Map::new();
    for path in paths {
        crate::safe_relative(path)?;
        let full = root.path().join(path);
        no_symlinks(&full)?;
        let value = if full.try_exists()? {
            artifact_ref(root, path)?
        } else {
            Value::Null
        };
        result.insert(path.clone(), value);
    }
    Ok(Value::Object(result))
}
fn check_pin(root: &SkillRoot, value: &Value) -> Result<()> {
    for (path, reference) in value.as_object().context("evidence_inputs_not_object")? {
        crate::safe_relative(path)?;
        if reference.is_null() {
            no_symlinks(&root.path().join(path))?;
            ensure!(
                !root.path().join(path).try_exists()?,
                "evidence_missing_input_created"
            );
        } else {
            ensure!(reference["path"] == *path, "evidence_input_path_mismatch");
            verify_artifact_ref(root, reference, "evidence_source")?;
        }
    }
    Ok(())
}
fn base_paths(root: &SkillRoot, authoring: bool) -> Result<BTreeSet<String>> {
    let lock = crate::runtime::read_runtime_source_lock(root.path(), false)?;
    let mut paths = BTreeSet::from([
        SOURCE_LOCK_PATH.to_owned(),
        format!("runtime/vendor/{}", lock.artifact_filename),
        CODE_PATH.to_owned(),
    ]);
    for (path, bytes) in CRITERIA_SOURCES {
        ensure!(
            read_regular_bytes(&root.path().join(path), LIMIT)? == *bytes,
            "compiled_release_evidence_source_mismatch"
        );
        paths.insert((*path).to_owned());
    }
    if authoring {
        for (prefix, name) in BOOK_LOCKS {
            let lock_path = format!("{prefix}{name}");
            paths.insert(lock_path.clone());
            if !root.path().join(&lock_path).try_exists()? {
                continue;
            }
            let bytes = read_regular_bytes(&root.path().join(&lock_path), LIMIT)?;
            for line in std::str::from_utf8(&bytes)?
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
            {
                let (digest, path) = line
                    .split_once("  ")
                    .context("authoring_lock_line_invalid")?;
                ensure!(
                    digest.len() == 64
                        && digest
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "authoring_lock_digest_invalid"
                );
                crate::safe_relative(path)?;
                paths.insert(format!("{prefix}{path}"));
            }
        }
    }
    Ok(paths)
}
fn criteria(kind: &str) -> Result<Value> {
    ensure!(
        EVIDENCE_PATHS.iter().any(|(k, _)| *k == kind),
        "unknown_release_evidence_kind"
    );
    Ok(
        json!({"schema":"ontology-engineering.release-criteria/v2","implementation_sha256":sha256(include_bytes!("release_evidence.rs")),"implementation_sources":CRITERIA_SOURCES.iter().map(|(path,bytes)|((*path).to_owned(),sha256(bytes))).collect::<BTreeMap<_,_>>(),"kind":kind,
        "authority":"local_candidate_only_no_fact_adoption_or_publication",
        "regression_contract":"owned_cargo_workspace_locked_offline_and_external_semantica_two_suites",
        "pdf_contract":"two_fixed_books_positive_pages_all_fonts_embedded_text_gt_1000_advisory_visual_only",
        "replay_contract":"same_sources_tools_criteria_and_complete_domain_outcomes"}),
    )
}
fn portable(value: &Value) -> Result<()> {
    match value {
        Value::String(s) => ensure!(
            ![
                "/Users/",
                "/home/",
                "\\Users\\",
                "PRIVATE KEY-----",
                "apikey_"
            ]
            .iter()
            .any(|v| s.contains(v)),
            "fixed_evidence_contains_private_or_credential_text"
        ),
        Value::Array(a) => {
            for v in a {
                portable(v)?;
            }
        }
        Value::Object(o) => {
            for (k, v) in o {
                portable(&Value::String(k.clone()))?;
                portable(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

struct Session<'a> {
    root: &'a SkillRoot,
    options: &'a EvidenceOptions,
    output: PathBuf,
    inputs: Value,
    runtime: Value,
    tools: BTreeMap<String, Value>,
    processes: Vec<Value>,
    errors: Vec<Value>,
    runtime_access: crate::runtime::RuntimeAccessGuard,
}
impl<'a> Session<'a> {
    fn new(
        root: &'a SkillRoot,
        options: &'a EvidenceOptions,
        paths: BTreeSet<String>,
        tools: &[(&str, &Path)],
    ) -> Result<Self> {
        ensure!(
            options.timeout > Duration::ZERO && options.timeout <= Duration::from_secs(43200),
            "evidence_timeout_invalid"
        );
        ensure!(
            (1..=LIMIT).contains(&options.stream_limit),
            "evidence_stream_limit_invalid"
        );
        let runtime_access = crate::runtime::RuntimeAccessGuard::shared(root.path())?;
        let inputs = pin(root, &paths)?;
        let runtime = serde_json::to_value(crate::runtime::read_runtime_source_lock(
            root.path(),
            false,
        )?)?;
        check_pin(root, &inputs)?;
        let mut records = BTreeMap::new();
        for (name, path) in tools {
            records.insert((*name).into(), tool_record(root, path)?);
        }
        let output = root.private_path(&options.output)?;
        for path in paths {
            let full = root.path().join(path);
            ensure!(
                !full.starts_with(&output) && !output.starts_with(&full),
                "evidence_output_overlaps_input"
            );
        }
        let output = root.new_private_dir(&output)?;
        write_new(&output.join("input-snapshot.json"), &inputs)?;
        write_new(&output.join("tool-snapshot.json"), &json!(records))?;
        Ok(Self {
            root,
            options,
            output,
            inputs,
            runtime,
            tools: records,
            processes: Vec::new(),
            errors: Vec::new(),
            runtime_access,
        })
    }
    fn error(&mut self, phase: &str, error: anyhow::Error) {
        self.errors
            .push(json!({"phase":phase,"code":"observation_failed","detail":error.to_string()}));
    }
    fn run(
        &mut self,
        id: &str,
        name: &str,
        arguments: Vec<OsString>,
        cwd: &Path,
        environment: BTreeMap<OsString, OsString>,
    ) -> Result<ProcessObservation> {
        let record = self.tools.get(name).context("evidence_tool_not_selected")?;
        check_tool(record)?;
        let spec = CommandSpec {
            executable: record["invocation"]
                .as_str()
                .context("tool_path_missing")?
                .into(),
            arguments,
            cwd: cwd.to_owned(),
            environment,
            remove_environment: [
                "PYTHONHOME",
                "PYTEST_ADDOPTS",
                "PYTEST_PLUGINS",
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "RUSTDOCFLAGS",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            timeout: self.options.timeout,
            output_limit: self.options.stream_limit,
        };
        let path = self.output.join(id);
        let observed = run_command(&spec, &path)?;
        self.processes.push(json!({"id":id,"tool":name,"report":reference(self.root,&path.join("process.json"))?,"stdout":reference(self.root,&path.join("stdout"))?,"stderr":reference(self.root,&path.join("stderr"))?}));
        Ok(observed)
    }
    fn finish(&mut self, kind: &str, data: Value, git: Value) -> Result<Value> {
        if let Err(error) = self.runtime_access.ensure_current() {
            self.error("runtime_access", error);
        }
        let source_current = check_pin(self.root, &self.inputs).is_ok();
        let tools_current = self.tools.values().all(|t| check_tool(t).is_ok());
        let raw = json!({"schema":RAW_SCHEMA,"kind":kind,"origin":"rust_native","criteria":criteria(kind)?,
            "source_identity":{"runtime":self.runtime,"inputs":self.inputs,"tools":tools_identity(&self.tools),"git":git},
            "source_current":source_current,"tools_current":tools_current,"tools":self.tools,"data":data,"processes":self.processes,"errors":self.errors});
        let path = self.output.join(format!("{kind}-raw.json"));
        write_new(&path, &raw)?;
        let mut refs = Vec::new();
        inventory(self.root, &self.output, &path, &mut refs)?;
        let execution =
            json!({"raw":reference(self.root,&path)?,"artifacts":refs,"published_log":null});
        let (observation, domain_passed) = observe_raw(self.root, &raw)?;
        let doc = json!({"$schema":if kind.ends_with("_regression"){REGRESSION_SCHEMA}else{SCHEMA},"kind":kind,"passed":domain_passed&&source_current&&tools_current&&self.errors.is_empty(),
            "origin":"rust_native","criteria":criteria(kind)?,"source_identity":raw["source_identity"],"observation":observation,"execution":execution});
        portable(&doc)?;
        write_new(&self.output.join(format!("{kind}-evidence.json")), &doc)?;
        Ok(doc)
    }
}
fn inventory(root: &SkillRoot, dir: &Path, raw: &Path, out: &mut Vec<Value>) -> Result<()> {
    let mut entries = fs::read_dir(dir)?
        .map(|e| e.map(|v| v.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    for p in entries {
        if p == raw
            || p.file_name()
                .is_some_and(|n| n == "cargo-target" || n == "pytest-temp" || n == "tmp")
        {
            continue;
        }
        no_symlinks(&p)?;
        if p.is_dir() {
            inventory(root, &p, raw, out)?;
        } else {
            out.push(reference(root, &p)?);
        }
        ensure!(out.len() <= 10000, "evidence_artifact_count_limit");
    }
    Ok(())
}
fn command(root: &SkillRoot, raw: &Value, id: &str) -> Result<(Value, Vec<u8>, Vec<u8>)> {
    let rows = raw["processes"]
        .as_array()
        .context("evidence_processes_missing")?;
    let matches = rows.iter().filter(|r| r["id"] == id).collect::<Vec<_>>();
    ensure!(matches.len() == 1, "evidence_process_missing_or_duplicate");
    let row = matches[0];
    exact_keys(
        row,
        &["id", "tool", "report", "stdout", "stderr"],
        "process_reference",
    )?;
    let report_path = verify_artifact_ref(root, &row["report"], "process_report")?;
    root.private_path(&report_path)?;
    ensure!(
        report_path.file_name().is_some_and(|n| n == "process.json"),
        "process_report_name_invalid"
    );
    let directory = report_path.parent().context("process_directory_missing")?;
    ensure!(
        row["stdout"]["path"] == relative(root, &directory.join("stdout"))?
            && row["stderr"]["path"] == relative(root, &directory.join("stderr"))?,
        "process_stream_path_mismatch"
    );
    let report = crate::strict_json(&load_ref(root, &row["report"])?)?;
    let stdout = load_ref(root, &row["stdout"])?;
    let stderr = load_ref(root, &row["stderr"])?;
    ensure!(
        report["schema"] == "ontology-engineering.external-process/v1",
        "process_schema_invalid"
    );
    for (key, bytes) in [("stdout", &stdout), ("stderr", &stderr)] {
        ensure!(
            report[key]["path"] == key
                && report[key]["sha256"] == sha256(bytes)
                && report[key]["bytes"] == bytes.len(),
            "process_stream_identity_mismatch"
        );
    }
    let name = row["tool"].as_str().context("process_tool_missing")?;
    let expected = &raw["tools"][name];
    ensure!(
        !expected.is_null()
            && report["executable"]["path"] == expected["invocation"]
            && report["executable"]["resolved"] == expected["target"]
            && report["executable"]["identity"]["sha256"] == expected["sha256"]
            && report["executable"]["identity"]["bytes"] == expected["size_bytes"],
        "process_tool_identity_mismatch"
    );
    Ok((report, stdout, stderr))
}
fn command_passed(report: &Value) -> bool {
    report["status"] == "completed"
        && report["exit_code"] == 0
        && report["capture_complete"] == true
        && report["executable"]["unchanged"] == true
        && report["stdout"]["truncated"] == false
        && report["stderr"]["truncated"] == false
}
fn exact_command(report: &Value, args: &[String], cwd: &Path) -> Result<()> {
    ensure!(
        report["arguments"] == json!(args) && report["cwd"] == json!(cwd),
        "evidence_command_contract_mismatch"
    );
    Ok(())
}

pub fn observe_static(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    let mut s = Session::new(
        root,
        options,
        base_paths(root, true)?,
        &[("python", &options.python), ("git", &options.git)],
    )?;
    let authoring = match crate::book_sources::check_authoring_locks(root.path(), false) {
        Ok(v) => v,
        Err(e) => {
            s.error("authoring_locks", e);
            Value::Null
        }
    };
    let authoring_doc = s.finish("authoring_locks", json!({"report":authoring}), Value::Null)?;
    s.errors.clear();
    let identity = (|| -> Result<Value> {
        let guard = crate::runtime::RuntimeAccessGuard::shared(root.path())?;
        let python = PathBuf::from(
            s.tools["python"]["invocation"]
                .as_str()
                .context("python_missing")?,
        );
        let identity = serde_json::to_value(crate::runtime::verify_runtime_source_identity(
            root.path(),
            None,
            &python,
        )?)?;
        let probe = crate::runtime::probe_external_runtime(root.path(), &python)?;
        let checks = crate::runtime::doctor_preflight(root.path(), probe.python_version);
        let ok = !checks.iter().any(|check| check.level == "error");
        guard.ensure_current()?;
        Ok(json!({"mode":"doctor","ok":ok,"checks":checks,"identity":identity,"probe":probe}))
    })();
    let identity = match identity {
        Ok(v) => v,
        Err(e) => {
            s.error("runtime_identity", e);
            Value::Null
        }
    };
    let runtime_doc = s.finish("runtime_identity", json!({"report":identity}), Value::Null)?;
    s.errors.clear();
    let scan_options = oe_core::privacy::ScanOptions::new(
        PathBuf::from(
            s.tools["git"]["invocation"]
                .as_str()
                .context("git_missing")?,
        ),
        s.output.join("privacy-scan"),
    );
    let scan = match oe_core::privacy::scan(root, &scan_options) {
        Ok(v) => v,
        Err(e) => {
            s.error("privacy", e);
            Value::Null
        }
    };
    let privacy_doc = s.finish("privacy", json!({"report":scan}), Value::Null)?;
    Ok(
        json!({"authoring_locks":authoring_doc,"runtime_identity":runtime_doc,"privacy":privacy_doc}),
    )
}
pub fn observe_book_bindings(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    let execution = crate::books::verify_book_sources(root);
    let mut paths = base_paths(root, true)?;
    paths.extend(
        crate::books::rust::source_execution::IMPLEMENTATION_SOURCES
            .iter()
            .map(|(path, _)| (*path).to_owned()),
    );
    paths.extend([
        crate::books::rust::ARCHIVE.into(),
        "sources/continuity.json".into(),
    ]);
    if let Ok(response) = &execution {
        paths.extend(crate::books::rust::source_execution::required_paths(
            response,
        )?);
    }
    let mut s = Session::new(root, options, paths, &[])?;
    let response = match execution {
        Ok(v) => v,
        Err(e) => {
            s.error("book_source_bindings", e);
            Value::Null
        }
    };
    Ok(
        json!({"book_source_bindings":s.finish("book_source_bindings",json!({"report":response}),Value::Null)?}),
    )
}

fn visual_document(root: &SkillRoot, options: &EvidenceOptions) -> Result<(Value, Value)> {
    let path = root.expand(
        options
            .visual_review
            .as_deref()
            .unwrap_or(Path::new(VISUAL_REVIEW_PATH)),
    )?;
    let rel = relative(root, &path)?;
    let reference = artifact_ref(root, &rel)?;
    let document = release::read_document(root, &rel, true)?;
    validate_visual(&document)?;
    Ok((document, reference))
}
fn validate_visual(document: &Value) -> Result<()> {
    exact_keys(
        document,
        &[
            "$schema",
            "advisory_only",
            "books",
            "passed",
            "review_basis",
        ],
        "visual_review",
    )?;
    ensure!(
        document["$schema"] == "ontology-engineering.pdf-visual-review/v1"
            && document["advisory_only"] == true
            && document["passed"].is_boolean(),
        "visual_review_authority_or_schema_invalid"
    );
    ensure!(
        document["review_basis"]
            .as_str()
            .is_some_and(|v| !v.trim().is_empty()),
        "visual_review_basis_missing"
    );
    let books = document["books"]
        .as_array()
        .context("visual_books_missing")?;
    ensure!(books.len() == 2, "visual_book_count_invalid");
    for (entry, spec) in books.iter().zip(BOOK_SPECS) {
        exact_keys(
            entry,
            &["pages_reviewed", "passed", "volume"],
            "visual_book",
        )?;
        ensure!(
            entry["volume"] == spec.volume && entry["passed"].is_boolean(),
            "visual_book_identity_invalid"
        );
        let pages = entry["pages_reviewed"]
            .as_array()
            .context("visual_pages_missing")?;
        ensure!(!pages.is_empty(), "visual_pages_empty");
        let mut previous = 0;
        for page in pages {
            let number = page.as_u64().context("visual_page_invalid")?;
            ensure!(number > previous, "visual_pages_unsorted_or_duplicate");
            previous = number;
        }
    }
    Ok(())
}
pub fn observe_pdf_qa(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    let (visual, visual_ref) = visual_document(root, options)?;
    let mut paths = base_paths(root, false)?;
    paths.insert(visual_ref["path"].as_str().unwrap().to_owned());
    for book in BOOK_SPECS {
        paths.insert(book.pdf.into());
    }
    let mut s = Session::new(
        root,
        options,
        paths,
        &[
            ("pdfinfo", &options.pdfinfo),
            ("pdffonts", &options.pdffonts),
            ("pdftotext", &options.pdftotext),
        ],
    )?;
    for book in BOOK_SPECS {
        for name in ["pdfinfo", "pdffonts", "pdftotext"] {
            let mut args = vec![root.path().join(book.pdf).into_os_string()];
            if name == "pdftotext" {
                args.push("-".into());
            }
            if let Err(e) = s.run(
                &format!("{}-{name}", book.volume),
                name,
                args,
                root.path(),
                BTreeMap::new(),
            ) {
                s.error(name, e);
            }
        }
    }
    Ok(
        json!({"pdf_qa":s.finish("pdf_qa",json!({"visual":visual,"visual_ref":visual_ref}),Value::Null)?}),
    )
}

fn git_identity(report: &Value) -> Result<Value> {
    ensure!(
        report["status"] == "passed"
            || report["status"] == "verified"
            || report["status"] == "completed",
        "regression_source_boundary_not_verified"
    );
    let identity = report
        .get("source_identity")
        .context("git_source_identity_missing")?;
    exact_keys(
        identity,
        &["source_commit", "source_tree", "head", "files"],
        "git_source_identity",
    )?;
    Ok(identity.clone())
}
fn regression_args(kind: &str, output: &Path) -> Vec<String> {
    if kind == "semantica_regression" {
        vec![
            "-m".into(),
            "pytest".into(),
            "-q".into(),
            "tests/ontology".into(),
            "tests/chapter_packages".into(),
            "--basetemp".into(),
            output.join("pytest-temp").to_string_lossy().into_owned(),
        ]
    } else {
        vec![
            "test".into(),
            "--workspace".into(),
            "--locked".into(),
            "--offline".into(),
        ]
    }
}
pub fn observe_regressions(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    let commit = options
        .oe_source_commit
        .as_deref()
        .context("regression_oe_source_commit_required")?;
    let checkout = root.expand(
        options
            .semantica_root
            .as_deref()
            .context("regression_semantica_checkout_required")?,
    )?;
    no_symlinks(&checkout)?;
    ensure!(checkout.is_dir(), "regression_checkout_missing");
    let mut paths = base_paths(root, false)?;
    paths.extend(["Cargo.toml".into(), "Cargo.lock".into()]);
    let mut s = Session::new(
        root,
        options,
        paths,
        &[
            ("git", &options.git),
            ("python", &options.python),
            ("cargo", &options.cargo),
        ],
    )?;
    ensure!(
        !checkout.starts_with(&s.output) && !s.output.starts_with(&checkout),
        "regression_output_overlaps_checkout"
    );
    let git = PathBuf::from(
        s.tools["git"]["invocation"]
            .as_str()
            .context("git_missing")?,
    );
    let lock = crate::runtime::read_runtime_source_lock(root.path(), false)?;
    let before_oe = crate::release_sources::verify_source_boundary(
        root,
        commit,
        &git,
        &s.output.join("oe-source-before"),
    )?;
    let before_sem = crate::release_sources::verify_semantica_checkout(
        root,
        &checkout,
        &lock.commit,
        &git,
        &s.output.join("semantica-source-before"),
    )?;
    let before = json!({"ontology_engineering":git_identity(&before_oe)?,"semantica":git_identity(&before_sem)?});
    let tmp = s.output.join("tmp");
    oe_core::create_new_private_dir(&tmp)?;
    for (kind, name, cwd) in [
        ("semantica_regression", "python", checkout.as_path()),
        ("ontology_engineering_regression", "cargo", root.path()),
    ] {
        let mut env = BTreeMap::from([(OsString::from("TMPDIR"), tmp.clone().into_os_string())]);
        if kind == "semantica_regression" {
            env.insert("PYTHONPATH".into(), checkout.clone().into_os_string());
        } else {
            env.insert(
                "CARGO_TARGET_DIR".into(),
                s.output.join("cargo-target").into_os_string(),
            );
        }
        let args = regression_args(kind, &s.output)
            .into_iter()
            .map(OsString::from)
            .collect();
        if let Err(e) = s.run(kind, name, args, cwd, env) {
            s.error(kind, e);
        }
    }
    let after = (|| -> Result<Value> {
        let oe = crate::release_sources::verify_source_boundary(
            root,
            commit,
            &git,
            &s.output.join("oe-source-after"),
        )?;
        let sem = crate::release_sources::verify_semantica_checkout(
            root,
            &checkout,
            &lock.commit,
            &git,
            &s.output.join("semantica-source-after"),
        )?;
        Ok(json!({"ontology_engineering":git_identity(&oe)?,"semantica":git_identity(&sem)?}))
    })();
    let after = match after {
        Ok(v) => v,
        Err(e) => {
            s.error("source_after", e);
            Value::Null
        }
    };
    if before != after {
        s.errors.push(json!({"phase":"source_after","code":"source_changed","detail":"regression source baseline changed"}));
    }
    let mut source_reports = Map::new();
    for phase in ["before", "after"] {
        let mut reports = Map::new();
        for (kind, prefix) in [("ontology_engineering", "oe"), ("semantica", "semantica")] {
            let path = s
                .output
                .join(format!("{prefix}-source-{phase}/source-boundary.json"));
            reports.insert(
                kind.into(),
                if path.try_exists()? {
                    reference(root, &path)?
                } else {
                    Value::Null
                },
            );
        }
        source_reports.insert(phase.into(), Value::Object(reports));
    }
    let mut docs = Map::new();
    for kind in ["semantica_regression", "ontology_engineering_regression"] {
        let raw = json!({"processes":s.processes,"tools":s.tools});
        let normalized = match command(root, &raw, kind) {
            Ok((_, out, err)) => normalize_logs(&out, &err, root.path(), &checkout)?,
            Err(_) => Vec::new(),
        };
        let log = s.output.join(format!("{kind}.log"));
        write_new_bytes(&log, &normalized)?;
        let data = json!({"checkout":checkout,"output":s.output,"before":before,"after":after,"source_reports":source_reports,"log":reference(root,&log)?,"oe_source_commit":commit});
        docs.insert(kind.into(), s.finish(kind, data, before.clone())?);
    }
    Ok(Value::Object(docs))
}

fn normalize_logs(stdout: &[u8], stderr: &[u8], root: &Path, semantica: &Path) -> Result<Vec<u8>> {
    // Strict UTF-8 is required for published logs; raw streams remain retained.
    let a = std::str::from_utf8(stdout)?.trim_end_matches(['\r', '\n']);
    let b = std::str::from_utf8(stderr)?.trim_end_matches(['\r', '\n']);
    let mut output = [a, b]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    let mut replacements = [
        (
            root.to_str().context("root_not_utf8")?,
            "<ontology-engineering-root>",
        ),
        (
            semantica.to_str().context("checkout_not_utf8")?,
            "<semantica-root>",
        ),
    ];
    replacements.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    for (prefix, token) in replacements {
        let mut result = String::new();
        let mut start = 0;
        while let Some(index) = output[start..].find(prefix) {
            let index = start + index;
            let end = index + prefix.len();
            result.push_str(&output[start..index]);
            if end == output.len() || matches!(output.as_bytes().get(end), Some(b'/') | Some(b'\\'))
            {
                result.push_str(token);
            } else {
                result.push_str(prefix);
            }
            start = end;
        }
        result.push_str(&output[start..]);
        output = result;
    }
    Ok(output.into_bytes())
}
fn pytest_summary(text: &str) -> Result<Value> {
    let mut summary = None;
    for line in text.lines() {
        let line = line.trim_matches(|c: char| c == '=' || c.is_whitespace());
        let Some((counts, time)) = line.rsplit_once(" in ") else {
            continue;
        };
        // Pytest adds a human-readable timedelta after the seconds token for
        // runs lasting at least a minute. Neither representation is a verdict.
        let Some(seconds) = time
            .split_whitespace()
            .next()
            .and_then(|s| s.strip_suffix('s'))
        else {
            continue;
        };
        if seconds
            .parse::<f64>()
            .ok()
            .is_none_or(|v| !v.is_finite() || v < 0.0)
        {
            continue;
        }
        let mut tally = BTreeMap::new();
        let mut valid = true;
        for field in counts.split(", ") {
            let Some((number, label)) = field.split_once(' ') else {
                valid = false;
                break;
            };
            let Ok(number) = number.parse::<u64>() else {
                valid = false;
                break;
            };
            if tally.insert(label.to_owned(), number).is_some() {
                valid = false;
                break;
            }
        }
        if valid && tally.contains_key("passed") {
            summary = Some(tally);
        }
    }
    let tally = summary.context("pytest_pass_summary_missing")?;
    ensure!(
        tally.get("passed").copied().unwrap_or(0) > 0
            && tally.get("deselected").copied().unwrap_or(0) == 0
            && tally.get("failed").copied().unwrap_or(0) == 0
            && tally
                .get("error")
                .or(tally.get("errors"))
                .copied()
                .unwrap_or(0)
                == 0,
        "pytest_failed_summary"
    );
    Ok(
        json!({"passed_count":tally["passed"],"failed_count":0,"ignored_count":tally.get("skipped").copied().unwrap_or(0),"suite_count":1,"counts":tally}),
    )
}
fn cargo_summary(text: &str) -> Result<Value> {
    let mut suites = Vec::new();
    let mut passed = 0u64;
    let mut ignored = 0u64;
    for line in text
        .lines()
        .filter_map(|line| line.strip_prefix("test result: "))
    {
        let line = line
            .strip_prefix("ok. ")
            .context("cargo_test_suite_failed")?;
        let fields = line.split(';').map(str::trim).collect::<Vec<_>>();
        let count = |index: usize, suffix: &str| -> Result<u64> {
            fields
                .get(index)
                .and_then(|s| s.strip_suffix(suffix))
                .context("cargo_summary_invalid")?
                .parse()
                .context("cargo_summary_number_invalid")
        };
        let p = count(0, " passed")?;
        let f = count(1, " failed")?;
        let i = count(2, " ignored")?;
        let m = count(3, " measured")?;
        let filtered = count(4, " filtered out")?;
        ensure!(f == 0 && filtered == 0, "cargo_suite_failed_or_filtered");
        passed = passed.checked_add(p).context("cargo_count_overflow")?;
        ignored = ignored.checked_add(i).context("cargo_count_overflow")?;
        suites.push(json!({"passed":p,"failed":f,"ignored":i,"measured":m,"filtered":filtered}));
    }
    ensure!(
        !suites.is_empty() && passed > 0,
        "cargo_workspace_summary_missing"
    );
    Ok(
        json!({"passed_count":passed,"failed_count":0,"ignored_count":ignored,"suite_count":suites.len(),"suites":suites}),
    )
}
fn fonts(text: &str) -> Result<(bool, usize)> {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() < 3 {
        return Ok((false, 0));
    }
    let header = lines[0].to_lowercase();
    ensure!(
        header.split_whitespace().any(|s| s == "emb")
            && header.split_whitespace().any(|s| s == "sub"),
        "pdffonts_embedding_column_missing"
    );
    let mut embedded = true;
    let mut count = 0;
    for row in lines[2..].iter().filter(|s| !s.trim().is_empty()) {
        let fields = row.split_whitespace().collect::<Vec<_>>();
        ensure!(fields.len() >= 6, "pdffonts_row_invalid");
        let n = fields.len();
        for flag in &fields[n - 5..n - 2] {
            ensure!(
                matches!(flag.to_ascii_lowercase().as_str(), "yes" | "no"),
                "pdffonts_flag_invalid"
            );
        }
        fields[n - 2]
            .parse::<u64>()
            .context("pdffonts_object_invalid")?;
        fields[n - 1]
            .parse::<u64>()
            .context("pdffonts_generation_invalid")?;
        embedded &= fields[n - 5].eq_ignore_ascii_case("yes");
        count += 1;
    }
    Ok((embedded && count > 0, count))
}

fn observe_raw(root: &SkillRoot, raw: &Value) -> Result<(Value, bool)> {
    let kind = raw["kind"].as_str().context("raw_kind_missing")?;
    let data = &raw["data"];
    if kind == "authoring_locks" {
        let report = data["report"].clone();
        if !report.is_null() {
            exact_keys(
                &report,
                &["schema_version", "mode", "passed", "locks"],
                "authoring_report",
            )?;
            ensure!(
                report == crate::book_sources::check_authoring_locks(root.path(), false)?,
                "authoring_report_not_reproduced"
            );
        }
        return Ok((
            report.clone(),
            report["passed"] == true && report["mode"] == "check",
        ));
    }
    if kind == "runtime_identity" {
        let report = data["report"].clone();
        if !report.is_null() {
            exact_keys(
                &report,
                &["mode", "ok", "checks", "identity", "probe"],
                "runtime_doctor_report",
            )?;
            let probe: crate::runtime::InstalledRuntimeObservation =
                serde_json::from_value(report["probe"].clone())?;
            ensure!(
                report["checks"]
                    == serde_json::to_value(crate::runtime::doctor_preflight(
                        root.path(),
                        probe.python_version
                    ))?,
                "runtime_doctor_checks_changed"
            );
        }
        let identity = &report["identity"];
        let checks = report["checks"].as_array();
        let passed = report["mode"] == "doctor"
            && report["ok"] == true
            && checks.is_some_and(|checks| {
                !checks.is_empty()
                    && checks.iter().all(|check| {
                        matches!(check["level"].as_str(), Some("ok" | "warning" | "info"))
                    })
            })
            && identity["source_lock"] == raw["source_identity"]["runtime"]
            && identity["record_files"].as_u64().is_some_and(|v| v > 0)
            && identity["semantic_execution"] == "not_run"
            && identity["staging_descriptor_sha256"].is_null()
            && identity["observation_kind"] == "isolated_external_runtime_identity_probe";
        let checks = checks.map(|checks| {
            checks
                .iter()
                .map(|check| json!({"level":check["level"],"code":check["code"]}))
                .collect::<Vec<_>>()
        });
        return Ok((
            json!({"mode":"doctor","checks":checks,"identity":identity,"semantic_execution":"not_run","ok":passed,"full_diagnostics_retained_in_raw":true}),
            passed,
        ));
    }
    if kind == "book_source_bindings" {
        let wrapper = &data["report"];
        // Old native receipts retain their original identity. They cannot
        // satisfy a current Rust book-source gate by inheriting a verdict.
        if !wrapper.is_null() && wrapper["engine"] != "semantica.rust-runtime" {
            return Ok((
                json!({"report":wrapper["result"],"status":"historical",
                "input_sha256":wrapper["input_sha256"],
                "native_execution":wrapper["native_execution"]["semantic_execution"],
                "semantic_execution":"not_run","engine":"historical-semantica",
                "current_rust_acceptance":false}),
                false,
            ));
        }
        let passed = if wrapper.is_null() {
            false
        } else {
            crate::books::rust::source_execution::validate_response(root, wrapper)?
        };
        return Ok((
            json!({"report":wrapper["result"],"status":wrapper["status"],
                "input_sha256":wrapper["input_sha256"],
                "semantic_execution":wrapper["semantic_execution"],
                "engine":wrapper["engine"],"rust_execution":wrapper["rust_execution"]}),
            passed,
        ));
    }
    if kind == "privacy" {
        let report = &data["report"];
        let passed = report["schema"] == "ontology-engineering.public-privacy-scan/v1"
            && report["status"] == "completed"
            && report["ok"] == true
            && report["findings"].as_array().is_some_and(Vec::is_empty)
            && report["candidates"].as_array().is_some_and(|items| {
                report["checked_files"] == items.len()
                    && report["selected_files"] == items.len()
                    && items.iter().all(|item| item["unchanged"] == true)
            })
            && report["criteria_identity"]["source_sha256"]
                == sha256(include_bytes!("../../oe-core/src/privacy.rs"))
            && report["semantic_authority"] == false
            && report["error_code"].is_null()
            && report["inputs_unchanged"] == true
            && report["selection_unchanged"] == true
            && report["effective_scope"] == "git_tracked_and_unignored"
            && report["selection_reason"] == "required_git_selection_no_fallback";
        return Ok((
            json!({"ok":report["ok"],"scope":report["effective_scope"],"findings":report["findings"],"criteria_identity":report["criteria_identity"],
            "inventory_timing":"observed_before_fixed_evidence_publication","full_scan_retained_in_raw":true}),
            passed,
        ));
    }
    if kind.ends_with("_regression") {
        let output = Path::new(
            data["output"]
                .as_str()
                .context("regression_output_missing")?,
        );
        let checkout = Path::new(
            data["checkout"]
                .as_str()
                .context("regression_checkout_missing")?,
        );
        let observed = (|| -> Result<(Value, bool)> {
            for phase in ["before", "after"] {
                for kind in ["ontology_engineering", "semantica"] {
                    if data[phase].is_null() {
                        continue;
                    }
                    let report =
                        crate::strict_json(&load_ref(root, &data["source_reports"][phase][kind])?)?;
                    ensure!(
                        git_identity(&report)? == data[phase][kind],
                        "regression_source_report_mismatch"
                    );
                }
            }
            let (report, stdout, stderr) = command(root, raw, kind)?;
            let cwd = if kind == "semantica_regression" {
                checkout
            } else {
                root.path()
            };
            exact_command(&report, &regression_args(kind, output), cwd)?;
            let log = load_ref(root, &data["log"])?;
            ensure!(
                log == normalize_logs(&stdout, &stderr, root.path(), checkout)?,
                "normalized_regression_log_mismatch"
            );
            let combined = format!(
                "{}\n{}",
                std::str::from_utf8(&stdout)?,
                std::str::from_utf8(&stderr)?
            );
            let counts = if kind == "semantica_regression" {
                pytest_summary(&combined)
            } else {
                cargo_summary(&combined)
            };
            let passed =
                command_passed(&report) && counts.is_ok() && data["before"] == data["after"];
            let counts =
                counts.unwrap_or_else(|_| json!({"status":"unrecognized_or_failed_summary"}));
            let mut logical = regression_args(kind, Path::new("<audit-output>"));
            logical.insert(
                0,
                if kind == "semantica_regression" {
                    "runtime-python".into()
                } else {
                    "cargo".into()
                },
            );
            Ok((
                json!({"command":logical,"repository":if kind=="semantica_regression"{"https://github.com/jiaqiwang969/semantica.git"}else{"https://github.com/jiaqiwang969/OntologyEngineering.git"},
                "commit":if kind=="semantica_regression"{&data["before"]["semantica"]["source_commit"]}else{&data["before"]["ontology_engineering"]["source_commit"]},
                "process_status":report["status"],"return_code":report["exit_code"],"counts":counts,"log":data["log"],"passed":passed,
                "ignored_cases":"reported_not_executed","source_unchanged":data["before"]==data["after"]}),
                passed,
            ))
        })();
        return match observed {
            Ok(v) => Ok(v),
            Err(_) => Ok((
                json!({"passed":false,"error_code":"regression_observation_invalid","log":data["log"]}),
                false,
            )),
        };
    }
    if kind == "pdf_qa" {
        let visual_bytes = load_ref(root, &data["visual_ref"])?;
        let visual = crate::strict_json(&visual_bytes)?;
        ensure!(
            release::canonical_bytes(&visual)? == visual_bytes && visual == data["visual"],
            "visual_review_original_mismatch"
        );
        validate_visual(&visual)?;
        let mut rows = Vec::new();
        let mut passed = data["visual"]["passed"] == true;
        for (index, book) in BOOK_SPECS.iter().enumerate() {
            let result = (|| -> Result<Value> {
                let (info, info_out, _) = command(root, raw, &format!("{}-pdfinfo", book.volume))?;
                let (font, font_out, _) = command(root, raw, &format!("{}-pdffonts", book.volume))?;
                let (text, text_out, _) =
                    command(root, raw, &format!("{}-pdftotext", book.volume))?;
                let file = root.path().join(book.pdf).to_string_lossy().into_owned();
                exact_command(&info, std::slice::from_ref(&file), root.path())?;
                exact_command(&font, std::slice::from_ref(&file), root.path())?;
                exact_command(&text, &[file, "-".into()], root.path())?;
                ensure!(
                    command_passed(&info) && command_passed(&font) && command_passed(&text),
                    "pdf_tool_failed"
                );
                let pages = std::str::from_utf8(&info_out)?
                    .lines()
                    .find_map(|l| l.strip_prefix("Pages:"))
                    .context("pdf_page_count_missing")?
                    .trim()
                    .parse::<u64>()?;
                ensure!(pages > 0, "pdf_page_count_invalid");
                let (embedded, font_count) = fonts(std::str::from_utf8(&font_out)?)?;
                let characters = std::str::from_utf8(&text_out)?.trim().chars().count();
                let visual = &data["visual"]["books"][index];
                ensure!(
                    visual["pages_reviewed"]
                        .as_array()
                        .context("visual_pages_missing")?
                        .iter()
                        .all(|p| p.as_u64().is_some_and(|n| n > 0 && n <= pages)),
                    "visual_page_out_of_range"
                );
                Ok(
                    json!({"font_count":font_count,"fonts_embedded":embedded,"page_count":pages,"pdf":raw["source_identity"]["inputs"][book.pdf],"text_characters":characters,"visual_passed":visual["passed"],"volume":book.volume}),
                )
            })();
            match result {
                Ok(row) => {
                    passed &= row["fonts_embedded"] == true
                        && row["text_characters"].as_u64().is_some_and(|n| n > 1000)
                        && row["visual_passed"] == true;
                    rows.push(row);
                }
                Err(_) => {
                    passed = false;
                    rows.push(json!({"volume":book.volume,"pdf":raw["source_identity"]["inputs"][book.pdf],"status":"blocked","error_code":"pdf_observation_invalid"}));
                }
            }
        }
        return Ok((
            json!({"books":rows,"passed":passed,"visual_review":data["visual_ref"]}),
            passed,
        ));
    }
    bail!("unknown_release_evidence_kind")
}

/// Verify original receipts and recompute the domain conclusion. This checks
/// internal consistency; fresh replay remains mandatory for candidate acceptance.
pub fn validate_document(root: &SkillRoot, kind: &str, document: &Value) -> Result<()> {
    exact_keys(
        document,
        &[
            "$schema",
            "kind",
            "passed",
            "origin",
            "criteria",
            "source_identity",
            "observation",
            "execution",
        ],
        "release_evidence",
    )?;
    ensure!(
        document["kind"] == kind
            && document["origin"] == "rust_native"
            && document["passed"].is_boolean(),
        "release_evidence_identity_invalid"
    );
    ensure!(
        document["$schema"]
            == if kind.ends_with("_regression") {
                REGRESSION_SCHEMA
            } else {
                SCHEMA
            },
        "release_evidence_schema_invalid"
    );
    ensure!(
        document["criteria"] == criteria(kind)?,
        "release_evidence_criteria_changed"
    );
    portable(document)?;
    let execution = &document["execution"];
    exact_keys(
        execution,
        &["raw", "artifacts", "published_log"],
        "evidence_execution",
    )?;
    let raw_path = verify_artifact_ref(root, &execution["raw"], "evidence_raw")?;
    root.private_path(&raw_path)?;
    for artifact in execution["artifacts"]
        .as_array()
        .context("evidence_artifacts_missing")?
    {
        let path = verify_artifact_ref(root, artifact, "execution_artifact")?;
        root.private_path(&path)?;
    }
    let raw = crate::strict_json(&load_ref(root, &execution["raw"])?)?;
    exact_keys(
        &raw,
        &[
            "schema",
            "kind",
            "origin",
            "criteria",
            "source_identity",
            "source_current",
            "tools_current",
            "tools",
            "data",
            "processes",
            "errors",
        ],
        "raw_evidence",
    )?;
    ensure!(
        raw["schema"] == RAW_SCHEMA
            && raw["kind"] == kind
            && raw["origin"] == "rust_native"
            && raw["criteria"] == document["criteria"]
            && raw["source_identity"] == document["source_identity"],
        "raw_evidence_identity_mismatch"
    );
    exact_keys(
        &raw["source_identity"],
        &["runtime", "inputs", "tools", "git"],
        "evidence_source_identity",
    )?;
    ensure!(
        raw["source_current"].is_boolean()
            && raw["tools_current"].is_boolean()
            && raw["errors"].is_array(),
        "evidence_freshness_invalid"
    );
    let tools: BTreeMap<String, Value> = serde_json::from_value(raw["tools"].clone())?;
    ensure!(
        tools_identity(&tools) == raw["source_identity"]["tools"],
        "evidence_tools_projection_mismatch"
    );
    exact_keys(
        &raw["data"],
        if kind.ends_with("_regression") {
            &[
                "checkout",
                "output",
                "before",
                "after",
                "source_reports",
                "log",
                "oe_source_commit",
            ]
        } else if kind == "pdf_qa" {
            &["visual", "visual_ref"]
        } else {
            &["report"]
        },
        "evidence_data",
    )?;
    let mut required = base_paths(
        root,
        matches!(
            kind,
            "authoring_locks" | "runtime_identity" | "privacy" | "book_source_bindings"
        ),
    )?;
    if kind.ends_with("_regression") {
        required.extend(["Cargo.toml".into(), "Cargo.lock".into()]);
    }
    if kind == "book_source_bindings" {
        required.extend(
            crate::books::rust::source_execution::IMPLEMENTATION_SOURCES
                .iter()
                .map(|(path, _)| (*path).to_owned()),
        );
        required.extend([
            crate::books::rust::ARCHIVE.into(),
            "sources/continuity.json".into(),
        ]);
        if raw["data"]["report"]["engine"] == "semantica.rust-runtime" {
            required.extend(crate::books::rust::source_execution::required_paths(
                &raw["data"]["report"],
            )?);
        }
    }
    if kind == "pdf_qa" {
        required.extend(BOOK_SPECS.iter().map(|b| b.pdf.to_owned()));
        required.insert(
            raw["data"]["visual_ref"]["path"]
                .as_str()
                .context("visual_path_missing")?
                .into(),
        );
    }
    ensure!(
        raw["source_identity"]["inputs"]
            .as_object()
            .context("source_inputs_missing")?
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            == required,
        "evidence_source_inventory_incomplete"
    );
    check_pin(root, &raw["source_identity"]["inputs"])?;
    ensure!(
        serde_json::to_value(crate::runtime::read_runtime_source_lock(
            root.path(),
            false
        )?)? == raw["source_identity"]["runtime"],
        "evidence_runtime_lock_changed"
    );
    for record in raw["tools"]
        .as_object()
        .context("raw_tools_missing")?
        .values()
    {
        check_tool(record)?;
    }
    let (observation, passed) = observe_raw(root, &raw)?;
    ensure!(
        observation == document["observation"]
            && document["passed"]
                == json!(
                    passed
                        && raw["source_current"] == true
                        && raw["tools_current"] == true
                        && raw["errors"].as_array().is_some_and(Vec::is_empty)
                ),
        "evidence_conclusion_not_reproduced"
    );
    if !execution["published_log"].is_null() {
        let path = REGRESSION_LOG_PATHS
            .iter()
            .find_map(|(k, p)| (*k == kind).then_some(*p))
            .context("unexpected_published_log")?;
        ensure!(
            execution["published_log"]["path"] == path,
            "published_log_path_invalid"
        );
        let published = load_ref(root, &execution["published_log"])?;
        ensure!(
            published == load_ref(root, &document["observation"]["log"])?,
            "published_log_bytes_differ"
        );
    }
    Ok(())
}

/// Replay compares complete domain outcomes and bound sources/criteria/tools;
/// private run directories, elapsed time and diagnostic warnings are not verdicts.
pub fn compare_replay(root: &SkillRoot, kind: &str, stored: &Value, fresh: &Value) -> Result<()> {
    validate_document(root, kind, stored)?;
    validate_document(root, kind, fresh)?;
    ensure!(
        stored["criteria"] == fresh["criteria"]
            && stored["source_identity"] == fresh["source_identity"],
        "evidence_replay_source_or_criteria_changed"
    );
    let mut old = stored["observation"].clone();
    let mut new = fresh["observation"].clone();
    if kind.ends_with("_regression") {
        old.as_object_mut()
            .context("regression_observation_missing")?
            .remove("log");
        new.as_object_mut()
            .context("regression_observation_missing")?
            .remove("log");
    }
    if kind == "privacy" {
        let ordinary = |document: &Value| -> Result<Value> {
            let raw = crate::strict_json(&load_ref(root, &document["execution"]["raw"])?)?;
            let candidates = raw["data"]["report"]["candidates"]
                .as_array()
                .context("privacy_candidates_missing")?;
            let mut kept = Vec::new();
            for candidate in candidates {
                let path = candidate["path"]
                    .as_str()
                    .context("privacy_candidate_path_missing")?;
                let generated = path == release::MANIFEST_PATH
                    || path == format!("{}.sha256", release::MANIFEST_PATH)
                    || EVIDENCE_PATHS
                        .iter()
                        .chain(release::GOVERNANCE_PATHS)
                        .chain(REGRESSION_LOG_PATHS)
                        .any(|(_, p)| path == *p);
                if !generated {
                    kept.push(candidate.clone());
                }
            }
            Ok(json!(kept))
        };
        ensure!(
            ordinary(stored)? == ordinary(fresh)?,
            "privacy_ordinary_source_inventory_changed"
        );
    }
    ensure!(
        old == new && stored["passed"] == fresh["passed"],
        "evidence_differs_from_fresh_replay"
    );
    Ok(())
}
fn publish(root: &SkillRoot, documents: Value) -> Result<Value> {
    let mut results = Map::new();
    for (kind, document) in documents.as_object().context("evidence_map_missing")? {
        let mut document = document.clone();
        if kind.ends_with("_regression") {
            let log = load_ref(root, &document["observation"]["log"])?;
            let path = REGRESSION_LOG_PATHS
                .iter()
                .find_map(|(k, p)| (*k == kind).then_some(*p))
                .context("regression_log_kind_invalid")?;
            release::write_artifact_bytes(root, path, &log)?;
            document["execution"]["published_log"] = artifact_ref(root, path)?;
        }
        release::write_document(root, release::evidence_path(kind)?, &document)?;
        results.insert(kind.clone(), document);
    }
    Ok(Value::Object(results))
}
pub fn collect_static(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    publish(root, observe_static(root, options)?)
}
pub fn collect_book_bindings(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    publish(root, observe_book_bindings(root, options)?)
}
pub fn collect_regressions(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    publish(root, observe_regressions(root, options)?)
}
pub fn collect_pdf_qa(root: &SkillRoot, options: &EvidenceOptions) -> Result<Value> {
    publish(root, observe_pdf_qa(root, options)?)
}
