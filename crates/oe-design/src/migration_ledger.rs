//! Port ledger for the frozen Semantica scope.
//!
//! `semantica-rs/ledger/*.json` gives every frozen scope item one disposition in the Rust port
//! (`semantica-rs/`). Behavioural evidence is one recorded port test run in
//! `semantica-rs/evidence/`: the raw libtest log plus a receipt binding it to the digest of the
//! port sources. The audit re-parses the raw log itself; it never trusts a listed result.
//! Completion requires every item verified against fresh evidence whose ledger and evidence
//! digests were pinned when this program was built, so editing either after the build cannot
//! complete the migration. Without a ledger directory the audit is unchanged.
use anyhow::{Context, Result, ensure};
use oe_wire::json::strict_json;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use syn::visit::Visit;

#[path = "../../../build_support/migration_pins.rs"]
mod pins;
include!(concat!(env!("OUT_DIR"), "/migration-pins.rs"));

/// Port root relative to the skill root.
pub const PORT: &str = "semantica-rs";
const LEDGER_SCHEMA: &str = "semantica.migration-ledger/1";
const RECEIPT_SCHEMA: &str = "semantica.port-test-receipt/1";
const BASELINE_SCHEMA: &str = "semantica.original-test-baseline/1";
const DISPOSITIONS: &[&str] = &[
    "migrated",
    "ported_test",
    "migrated_file",
    "retained",
    "superseded",
];
const ENTRY_KEYS: &[&str] = &[
    "id",
    "disposition",
    "rust",
    "tests",
    "retained",
    "by",
    "note",
    "original_result",
];
const ORIGINAL_RESULTS: &[&str] = &["passed", "failed", "skipped", "error", "not_run"];
const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LOG_BYTES: u64 = 512 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PORT_FILES: usize = 250_000;
const MAX_PORT_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const PROBLEM_SAMPLE: usize = 60;

/// Outcome for the audit: the port report and each covered scope id's status.
pub struct LedgerAudit {
    pub report: Value,
    pub statuses: BTreeMap<String, &'static str>,
    pub complete: bool,
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).context("migration_ledger_file_missing")?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= limit,
        "migration_ledger_unsafe_or_oversized_file"
    );
    let raw = fs::read(path).context("migration_ledger_read_failed")?;
    ensure!(raw.len() as u64 <= limit, "migration_ledger_input_budget");
    Ok(raw)
}

fn parse_json(raw: &[u8]) -> Result<Value> {
    strict_json(raw).context("migration_ledger_invalid_or_duplicate_json")
}

/// A safe portable path inside the port, outside build output and the evidence/ledger folders.
fn port_path(value: &str) -> Result<()> {
    let rest = value
        .strip_prefix("semantica-rs/")
        .context("migration_ledger_reference_outside_port")?;
    ensure!(
        !rest.is_empty()
            && value.len() <= 4096
            && !value.contains(['\\', ':', '\0'])
            && !value.chars().any(char::is_control)
            && rest
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "migration_ledger_unsafe_reference"
    );
    let first = rest.split('/').next().unwrap_or("");
    ensure!(
        !first.starts_with("target") && first != "ledger" && first != "evidence",
        "migration_ledger_reference_to_build_or_evidence"
    );
    Ok(())
}

/// SHA-256 over sorted `<relative path>\0<sha256>\n` of every port file except build output
/// (`target*`), `ledger/`, `evidence/` and `.git`; with the file count.
pub fn port_source_digest(root: &Path) -> Result<(String, usize)> {
    let port = root.join(PORT);
    let mut rows = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![port.clone()];
    while let Some(dir) = stack.pop() {
        let meta = fs::symlink_metadata(&dir).context("migration_port_missing")?;
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "migration_port_unsafe_directory"
        );
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("migration_port_name_not_utf8"))?;
            ensure!(!kind.is_symlink(), "migration_port_symlink_refused");
            let path = entry.path();
            if kind.is_dir() {
                let top = dir == port;
                if name == ".git"
                    || (top
                        && (name.starts_with("target") || name == "ledger" || name == "evidence"))
                {
                    continue;
                }
                stack.push(path);
            } else {
                ensure!(kind.is_file(), "migration_port_special_file");
                let rel = path
                    .strip_prefix(root)?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let bytes = fs::read(&path)?;
                total += bytes.len() as u64;
                ensure!(total <= MAX_PORT_BYTES, "migration_port_byte_budget");
                rows.push(format!("{rel}\0{}\n", oe_core::sha256(&bytes)));
                ensure!(rows.len() <= MAX_PORT_FILES, "migration_port_file_budget");
            }
        }
    }
    rows.sort();
    let count = rows.len();
    Ok((oe_core::sha256(rows.concat().as_bytes()), count))
}

/// Test results parsed from raw libtest output of `cargo test`. Ids are
/// `<test binary target>::<test path>`; doc tests are `doc:<crate>::<name>`.
#[derive(Default)]
pub struct TestLog {
    pub passed: BTreeSet<String>,
    pub failed: BTreeSet<String>,
    pub ignored: BTreeSet<String>,
}

/// Target name of a `Running ... (<path>/deps/<target>-<hash>)` line.
fn running_target(line: &str) -> Option<String> {
    let inner = line.rsplit_once('(')?.1.strip_suffix(')')?;
    let file = inner.rsplit(['/', '\\']).next()?;
    let file = file.strip_suffix(".exe").unwrap_or(file);
    let (stem, hash) = file.rsplit_once('-')?;
    (hash.len() >= 8 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| stem.to_owned())
}

pub fn parse_test_log(log: &str) -> Result<TestLog> {
    let mut out = TestLog::default();
    let mut target: Option<String> = None;
    let mut seen_targets = BTreeSet::new();
    for line in log.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Running ") {
            let name = running_target(rest).context("migration_test_log_unrecognised_target")?;
            ensure!(
                seen_targets.insert(name.clone()),
                "migration_test_log_duplicate_target:{name}"
            );
            target = Some(name);
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("Doc-tests ") {
            let name = format!("doc:{}", name.trim());
            ensure!(
                seen_targets.insert(name.clone()),
                "migration_test_log_duplicate_target:{name}"
            );
            target = Some(name);
            continue;
        }
        let Some(rest) = line.strip_prefix("test ") else {
            continue;
        };
        let Some((name, result)) = rest.rsplit_once(" ... ") else {
            continue;
        };
        let Some(current) = &target else {
            anyhow::bail!("migration_test_log_result_without_target");
        };
        let id = format!("{current}::{name}");
        let set = match result.trim() {
            "ok" => &mut out.passed,
            "FAILED" => &mut out.failed,
            r if r == "ignored" || r.starts_with("ignored,") => &mut out.ignored,
            _ => continue,
        };
        ensure!(
            set.insert(id.clone()),
            "migration_test_log_duplicate_test:{id}"
        );
    }
    ensure!(
        out.passed.is_disjoint(&out.failed) && out.passed.is_disjoint(&out.ignored),
        "migration_test_log_conflicting_results"
    );
    Ok(out)
}

/// Names a Rust source file declares: functions and methods, types, traits, constants,
/// statics, modules and macros, plus the self types of its `impl` blocks.
#[derive(Default)]
struct Declared {
    names: BTreeSet<String>,
    impls: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for Declared {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.names.insert(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.names.insert(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
    }
    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.names.insert(node.sig.ident.to_string());
        syn::visit::visit_trait_item_fn(self, node);
    }
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.names.insert(node.ident.to_string());
        syn::visit::visit_item_struct(self, node);
    }
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.names.insert(node.ident.to_string());
        syn::visit::visit_item_enum(self, node);
    }
    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.names.insert(node.ident.to_string());
        syn::visit::visit_item_trait(self, node);
    }
    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.names.insert(node.ident.to_string());
    }
    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        self.names.insert(node.ident.to_string());
    }
    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.names.insert(node.ident.to_string());
    }
    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.names.insert(node.ident.to_string());
    }
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.names.insert(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
    }
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if let Some(ident) = &node.ident {
            self.names.insert(ident.to_string());
        }
    }
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if let syn::Type::Path(path) = node.self_ty.as_ref() {
            if let Some(last) = path.path.segments.last() {
                self.impls.insert(last.ident.to_string());
            }
        }
        syn::visit::visit_item_impl(self, node);
    }
}

struct Port<'a> {
    root: &'a Path,
    declared: BTreeMap<String, std::result::Result<Declared, String>>,
    shas: BTreeMap<String, Option<String>>,
}
impl Port<'_> {
    fn sha(&mut self, rel: &str) -> Option<String> {
        if let Some(v) = self.shas.get(rel) {
            return v.clone();
        }
        let value = read_bounded(&self.root.join(rel), MAX_JSON_BYTES)
            .ok()
            .map(|b| oe_core::sha256(&b));
        self.shas.insert(rel.to_owned(), value.clone());
        value
    }
    /// `semantica-rs/<file>` or `semantica-rs/<file.rs>::<Name>[::<method>]`.
    fn check_rust(&mut self, reference: &str) -> std::result::Result<(), String> {
        let (file, symbol) = match reference.split_once(".rs::") {
            Some((f, s)) => (format!("{f}.rs"), Some(s)),
            None => (reference.to_owned(), None),
        };
        port_path(&file).map_err(|e| format!("{e}"))?;
        if self.sha(&file).is_none() {
            return Err("rust_reference_missing_file".into());
        }
        let Some(symbol) = symbol else {
            return Ok(());
        };
        let root = self.root;
        let declared = self.declared.entry(file.clone()).or_insert_with(|| {
            let raw =
                read_bounded(&root.join(&file), MAX_SOURCE_BYTES).map_err(|e| e.to_string())?;
            let text = std::str::from_utf8(&raw).map_err(|_| "rust_source_not_utf8".to_owned())?;
            let parsed = syn::parse_file(text).map_err(|_| "rust_source_unparsable".to_owned())?;
            let mut d = Declared::default();
            d.visit_file(&parsed);
            Ok(d)
        });
        let declared = declared.as_ref().map_err(Clone::clone)?;
        let parts: Vec<&str> = symbol.split("::").collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err("rust_reference_invalid_symbol".into());
        }
        let last = parts[parts.len() - 1];
        if !declared.names.contains(last) {
            return Err(format!("rust_symbol_not_declared:{last}"));
        }
        if parts.len() >= 2 {
            let owner = parts[parts.len() - 2];
            if !declared.names.contains(owner) && !declared.impls.contains(owner) {
                return Err(format!("rust_owner_not_declared:{owner}"));
            }
        }
        Ok(())
    }
}

fn strings<'a>(
    entry: &'a Map<String, Value>,
    key: &str,
) -> std::result::Result<Vec<&'a str>, String> {
    match entry.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|v| v.as_str().ok_or_else(|| format!("{key}_not_string")))
            .collect(),
        Some(_) => Err(format!("{key}_not_array")),
    }
}

struct Evidence {
    status: &'static str,
    tests: TestLog,
    detail: Value,
    baseline: Option<BTreeMap<String, String>>,
}

fn evidence(root: &Path, source_digest: &str) -> Result<Evidence> {
    let dir = root.join(pins::EVIDENCE_DIR);
    let receipt_path = dir.join("test-receipt.json");
    let mut baseline = None;
    let baseline_path = dir.join("original-baseline.json");
    if baseline_path.exists() {
        let b = parse_json(&read_bounded(&baseline_path, MAX_JSON_BYTES)?)?;
        ensure!(
            b["schema"] == BASELINE_SCHEMA && b["scope_sha256"] == crate::migration::SCOPE_SHA256,
            "migration_baseline_binding_mismatch"
        );
        let results = b["results"]
            .as_object()
            .context("migration_baseline_results_missing")?;
        let mut map = BTreeMap::new();
        for (id, result) in results {
            let result = result
                .as_str()
                .context("migration_baseline_result_not_string")?;
            ensure!(
                ORIGINAL_RESULTS.contains(&result),
                "migration_baseline_unknown_result"
            );
            map.insert(id.clone(), result.to_owned());
        }
        baseline = Some(map);
    }
    if !receipt_path.exists() {
        return Ok(Evidence {
            status: "absent",
            tests: TestLog::default(),
            detail: json!({"status": "absent"}),
            baseline,
        });
    }
    let receipt = parse_json(&read_bounded(&receipt_path, MAX_JSON_BYTES)?)?;
    ensure!(
        receipt["schema"] == RECEIPT_SCHEMA,
        "migration_receipt_schema_mismatch"
    );
    let log_name = receipt["log"]
        .as_str()
        .context("migration_receipt_log_missing")?;
    ensure!(
        !log_name.is_empty()
            && !log_name.contains(['/', '\\', '\0'])
            && log_name != "test-receipt.json",
        "migration_receipt_unsafe_log_name"
    );
    let log_raw = read_bounded(&dir.join(log_name), MAX_LOG_BYTES)?;
    ensure!(
        receipt["log_sha256"] == oe_core::sha256(&log_raw),
        "migration_receipt_log_digest_mismatch"
    );
    let tests =
        parse_test_log(std::str::from_utf8(&log_raw).context("migration_test_log_not_utf8")?)?;
    let recorded = receipt["source_sha256"]
        .as_str()
        .context("migration_receipt_source_missing")?;
    let status = if recorded == source_digest {
        "fresh"
    } else {
        "stale"
    };
    let detail = json!({"status": status, "recorded_source_sha256": recorded, "passed": tests.passed.len(),
        "failed": tests.failed.len(), "ignored": tests.ignored.len(), "toolchain": receipt["toolchain"],
        "failed_sample": tests.failed.iter().take(20).collect::<Vec<_>>()});
    Ok(Evidence {
        status,
        tests,
        detail,
        baseline,
    })
}

/// Verify the port ledger against the audited scope items (`tracked` maps each original file to
/// its SHA-256). `None` when the port has no ledger.
pub fn verify(
    root: &Path,
    items: &[Value],
    tracked: &BTreeMap<&str, &str>,
) -> Result<Option<LedgerAudit>> {
    let ledger_dir = root.join(pins::LEDGER_DIR);
    if !ledger_dir.exists() {
        return Ok(None);
    }
    let ledger_digest = pins::directory_digest(&ledger_dir)
        .context("migration_ledger_digest")?
        .context("migration_ledger_missing")?;
    let evidence_digest = pins::directory_digest(&root.join(pins::EVIDENCE_DIR))
        .context("migration_evidence_digest")?;
    let (source_digest, source_files) = port_source_digest(root)?;
    let ev = evidence(root, &source_digest)?;

    let by_id: BTreeMap<&str, &Value> = items
        .iter()
        .filter_map(|i| i["id"].as_str().map(|id| (id, i)))
        .collect();
    let mut names: Vec<_> = fs::read_dir(&ledger_dir)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    names.sort();
    let mut port = Port {
        root,
        declared: BTreeMap::new(),
        shas: BTreeMap::new(),
    };
    let mut statuses = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();
    let mut problem_count = 0usize;
    let mut by_disposition = BTreeMap::<String, u64>::new();
    let mut entries = 0usize;
    for name in &names {
        let module = name
            .strip_suffix(".json")
            .context("migration_ledger_non_json_file")?;
        let ledger = parse_json(&read_bounded(&ledger_dir.join(name), MAX_JSON_BYTES)?)?;
        ensure!(
            ledger["schema"] == LEDGER_SCHEMA
                && ledger["scope_sha256"] == crate::migration::SCOPE_SHA256
                && ledger["module"] == module,
            "migration_ledger_binding_mismatch:{name}"
        );
        ensure!(
            ledger.as_object().is_some_and(|o| o.keys().all(|k| [
                "schema",
                "scope_sha256",
                "module",
                "entries"
            ]
            .contains(&k.as_str()))),
            "migration_ledger_unknown_field:{name}"
        );
        for raw in ledger["entries"]
            .as_array()
            .with_context(|| format!("migration_ledger_entries_missing:{name}"))?
        {
            entries += 1;
            let entry = raw
                .as_object()
                .context("migration_ledger_entry_not_object")?;
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .context("migration_ledger_entry_id_missing")?;
            let item = by_id
                .get(id)
                .with_context(|| format!("migration_ledger_unknown_scope_id:{id}"))?;
            ensure!(
                !statuses.contains_key(id),
                "migration_ledger_duplicate_scope_id:{id}"
            );
            let result = check_entry(&mut port, &ev, tracked, item, entry);
            let disposition = entry
                .get("disposition")
                .and_then(Value::as_str)
                .unwrap_or("invalid");
            *by_disposition.entry(disposition.to_owned()).or_default() += 1;
            match result {
                Ok(()) => {
                    statuses.insert(id.to_owned(), "verified");
                }
                Err(problem) => {
                    statuses.insert(id.to_owned(), "unverified");
                    problem_count += 1;
                    if problems.len() < PROBLEM_SAMPLE {
                        problems.push(format!("{id}: {problem}"));
                    }
                }
            }
        }
    }
    let verified = statuses.values().filter(|s| **s == "verified").count();
    let unverified = statuses.len() - verified;
    let missing = items.len() - statuses.len();
    let pinned = COMPILED_LEDGER_SHA256 == Some(ledger_digest.as_str())
        && COMPILED_EVIDENCE_SHA256 == evidence_digest.as_deref();
    let complete = missing == 0
        && unverified == 0
        && ev.status == "fresh"
        && ev.tests.failed.is_empty()
        && pinned;
    let report = json!({
        "port_root": PORT, "ledger_files": names.len(), "entries": entries, "by_disposition": by_disposition,
        "verified": verified, "unverified": unverified, "missing": missing,
        "problem_count": problem_count, "problems": problems,
        "source_sha256": source_digest, "source_files": source_files, "evidence": ev.detail,
        "baseline": ev.baseline.as_ref().map(|b| b.len()),
        "ledger_sha256": ledger_digest, "evidence_sha256": evidence_digest,
        "compiled_pins": {"ledger": COMPILED_LEDGER_SHA256, "evidence": COMPILED_EVIDENCE_SHA256},
        "pinned": pinned,
        "completion_rule": "every scope item verified, fresh source-bound test evidence with no failures, ledger and evidence digests equal to this build's compiled pins"
    });
    Ok(Some(LedgerAudit {
        report,
        statuses,
        complete,
    }))
}

fn check_entry(
    port: &mut Port<'_>,
    ev: &Evidence,
    tracked: &BTreeMap<&str, &str>,
    item: &Value,
    entry: &Map<String, Value>,
) -> std::result::Result<(), String> {
    if let Some(key) = entry.keys().find(|k| !ENTRY_KEYS.contains(&k.as_str())) {
        return Err(format!("unknown_field:{key}"));
    }
    let disposition = entry
        .get("disposition")
        .and_then(Value::as_str)
        .ok_or("disposition_missing")?;
    if !DISPOSITIONS.contains(&disposition) {
        return Err(format!("unknown_disposition:{disposition}"));
    }
    let rust = strings(entry, "rust")?;
    let tests = strings(entry, "tests")?;
    let by = strings(entry, "by")?;
    let note = entry.get("note").and_then(Value::as_str).unwrap_or("");
    let check_tests = |tests: &[&str]| -> std::result::Result<(), String> {
        if tests.is_empty() {
            return Err("tests_missing".into());
        }
        if ev.status != "fresh" {
            return Err(format!("evidence_{}", ev.status));
        }
        for t in tests {
            if !ev.tests.passed.contains(*t) {
                return Err(format!("test_not_passed:{t}"));
            }
        }
        Ok(())
    };
    match disposition {
        "migrated" => {
            if rust.is_empty() {
                return Err("rust_missing".into());
            }
            for r in &rust {
                port.check_rust(r)?;
            }
            check_tests(&tests)?;
        }
        "ported_test" => {
            for r in &rust {
                port.check_rust(r)?;
            }
            check_tests(&tests)?;
            let declared = entry
                .get("original_result")
                .map(|v| v.as_str().ok_or("original_result_not_string"))
                .transpose()?
                .unwrap_or("passed");
            if !ORIGINAL_RESULTS.contains(&declared) {
                return Err("original_result_unknown".into());
            }
            let id = item["id"].as_str().unwrap_or("");
            match ev.baseline.as_ref().and_then(|b| b.get(id)) {
                Some(recorded) if recorded != declared => {
                    return Err(format!("original_result_mismatch:{recorded}"));
                }
                None if declared != "passed" => {
                    return Err("original_result_without_baseline".into());
                }
                _ => {}
            }
        }
        "migrated_file" => {
            if rust.is_empty() {
                return Err("rust_missing".into());
            }
            for r in &rust {
                port.check_rust(r)?;
            }
        }
        "retained" => {
            let rows = entry
                .get("retained")
                .and_then(Value::as_array)
                .filter(|r| !r.is_empty())
                .ok_or("retained_missing")?;
            // A file item names its own bytes; a symbol item is retained with its source file.
            let original = item["sha256"]
                .as_str()
                .or_else(|| item["path"].as_str().and_then(|p| tracked.get(p).copied()));
            let mut identical = false;
            for row in rows {
                let path = row["path"].as_str().ok_or("retained_path_missing")?;
                let sha = row["sha256"].as_str().ok_or("retained_sha256_missing")?;
                port_path(path).map_err(|e| e.to_string())?;
                if port.sha(path).as_deref() != Some(sha) {
                    return Err(format!("retained_sha256_mismatch:{path}"));
                }
                identical |= original == Some(sha);
            }
            if !identical {
                return Err("retained_not_byte_identical_to_original".into());
            }
            if !tests.is_empty() {
                check_tests(&tests)?;
            }
        }
        "superseded" => {
            if by.is_empty() {
                return Err("by_missing".into());
            }
            for path in &by {
                port.check_rust(path)?;
            }
            if note.trim().chars().count() < 12 {
                return Err("note_missing".into());
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
