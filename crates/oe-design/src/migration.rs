//! Whole-library task inventory gate. This is neither an engineering ontology nor
//! a package promotion ledger. Static identities and declarations cannot prove behavior.
use anyhow::{Context, Result, ensure};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub(crate) const SCOPE_SHA256: &str =
    "b1470d9980755811c815be24cdc041740de5f55e28232c327f0845d160455226";
const LOCK_SHA256: &str = "d874aecfa20e3753ea85725c0a9b24e2f4795e4b54df819b52a31ac56b93e1dc";
const COMMIT: &str = "8b2ed123c380fc8abc78507e6e6648e6f5c0052c";
const WHEEL_SHA256: &str = "142ddd6fe788b3a716646babc410628aa7606e44ed63b98f5227863f9ea7ba15";
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const EXPECTED: &[(&str, u64)] = &[
    ("tracked_files", 1469),
    ("package_python", 360),
    ("outside_python", 33),
    ("frontend_code", 80),
    ("public_symbols", 3915),
    ("outside_symbols", 125),
    ("python_exports", 1206),
    ("frontend_exports", 288),
    ("routes", 87),
    ("test_files", 307),
    ("original_test_symbols", 5268),
    ("python_test_sources", 303),
    ("placeholder_candidates", 32),
    ("empty_modules", 1),
    ("empty_files", 15),
    ("console_entrypoints", 5),
    ("public_classes", 862),
    ("public_functions", 607),
    ("public_methods", 2446),
    ("original_assertions", 7777),
];
const FILE_SECTIONS: &[(&str, &str)] = &[
    ("tracked_files", "tracked_file"),
    ("empty_files", "original_empty_file"),
    ("package_python", "package_source"),
    ("outside_python", "outside_source"),
    ("frontend_code", "frontend_source"),
    ("test_files", "original_test_file"),
    ("python_test_sources", "original_python_test_source"),
];
const SYMBOL_SECTIONS: &[&str] = &[
    "public_symbols",
    "outside_symbols",
    "python_exports",
    "frontend_exports",
    "routes",
    "original_test_symbols",
    "placeholder_candidates",
    "empty_modules",
    "console_entrypoints",
];

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("migration_scope_missing_string:{key}"))
}
fn rows<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v.get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("migration_scope_missing_rows:{key}"))
}
fn safe_path(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 4096
            && !value.starts_with('/')
            && !value.contains(['\\', ':'])
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "migration_scope_unsafe_path"
    );
    Ok(())
}
fn digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "migration_scope_invalid_sha256"
    );
    Ok(())
}
fn bounded_file(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).context("migration_scope_file_missing")?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= MAX_BYTES,
        "migration_scope_unsafe_or_oversized_file"
    );
    let raw = fs::read(path).context("migration_scope_read_failed")?;
    ensure!(
        raw.len() as u64 <= MAX_BYTES,
        "migration_scope_input_budget"
    );
    Ok(raw)
}
fn parse(raw: &[u8]) -> Result<Value> {
    strict_json(raw).context("migration_scope_invalid_or_duplicate_json")
}
fn identity(v: &Value) -> Result<()> {
    ensure!(
        text(v, "version")? == "0.6.5+oe.7"
            && text(v, "commit")? == COMMIT
            && text(v, "wheel_sha256")? == WHEEL_SHA256
            && text(v, "wheel_filename")? == "semantica-0.6.5+oe.7-py3-none-any.whl"
            && v["wheel_members"] == 810,
        "migration_scope_original_identity_mismatch"
    );
    Ok(())
}
fn unknown_status(v: &Value) -> Result<()> {
    ensure!(
        text(v, "behavior_status")? == "unknown" && text(v, "migration_status")? == "incomplete",
        "migration_scope_unearned_status"
    );
    Ok(())
}
fn file_identity(row: &Value) -> Result<(&str, &str, u64)> {
    let path = text(row, "path")?;
    safe_path(path)?;
    let hash = text(row, "sha256")?;
    digest(hash)?;
    let bytes = row["bytes"]
        .as_u64()
        .context("migration_scope_invalid_source_bytes")?;
    Ok((path, hash, bytes))
}
fn symbol_id(row: &Value, section: &str) -> Result<String> {
    let path = text(row, "path")?;
    safe_path(path)?;
    let symbol = text(row, "symbol")?;
    ensure!(
        !symbol.is_empty() && !symbol.contains('|') && !symbol.chars().any(char::is_control),
        "migration_scope_invalid_symbol"
    );
    let kind = match section {
        "placeholder_candidates" => "placeholder",
        "empty_modules" => "module_placeholder",
        _ => text(row, "kind")?,
    };
    ensure!(
        !kind.is_empty() && !kind.contains('|') && !kind.chars().any(char::is_control),
        "migration_scope_invalid_kind"
    );
    let class = row.get("class").and_then(Value::as_str).unwrap_or("");
    ensure!(
        !class.contains('|') && !class.chars().any(char::is_control),
        "migration_scope_invalid_class"
    );
    let line = row["line"]
        .as_u64()
        .filter(|v| *v > 0)
        .context("migration_scope_invalid_source_line")?;
    let target = if class.is_empty() {
        symbol.to_owned()
    } else {
        format!("{class}.{symbol}")
    };
    let expected = format!("{path}|{kind}|{target}|{line}");
    ensure!(
        text(row, "id")? == expected,
        "migration_scope_unstable_api_id"
    );
    ensure!(
        text(row, "behavior_status")? == "unknown",
        "migration_scope_unearned_api_status"
    );
    if section == "frontend_exports" {
        digest(text(row, "declaration_sha256")?)?;
    }
    if section == "routes" {
        digest(text(row, "decorator_sha256")?)?;
    }
    if section == "placeholder_candidates" {
        digest(text(row, "body_sha256")?)?;
    }
    Ok(expected)
}

/// Audit the exact portable whole-library baseline and report task declarations.
/// A matching inventory always leaves behavior unknown and migration incomplete.
/// Changing the approved baseline requires a new explicit compiled pin, not merely
/// re-signing a smaller mutable manifest beside it.
pub fn audit(root: &Path) -> Result<Value> {
    ensure!(root.is_dir(), "migration_scope_root_missing");
    let runtime =
        fs::symlink_metadata(root.join("runtime")).context("migration_scope_runtime_missing")?;
    ensure!(
        runtime.is_dir() && !runtime.file_type().is_symlink(),
        "migration_scope_unsafe_runtime_path"
    );
    let lock_raw = bounded_file(&root.join("runtime/semantica-scope-lock.json"))?;
    let scope_raw = bounded_file(&root.join("runtime/semantica-scope.json"))?;
    let lock = parse(&lock_raw)?;
    let scope = parse(&scope_raw)?;
    ensure!(
        text(&lock, "schema")? == "semantica.migration-scope-lock/1"
            && text(&scope, "schema")? == "semantica.migration-scope/1",
        "migration_scope_schema_mismatch"
    );
    ensure!(
        text(&lock, "scope_path")? == "runtime/semantica-scope.json",
        "migration_scope_wrong_baseline_path"
    );
    unknown_status(&lock)?;
    unknown_status(&scope)?;
    identity(&lock["identity"])?;
    identity(&scope["identity"])?;
    ensure!(
        lock["identity"] == scope["identity"]
            && lock["inputs"] == scope["inputs"]
            && lock["counts"] == scope["counts"],
        "migration_scope_lock_content_mismatch"
    );
    ensure!(
        text(&scope, "kind")? == "full_codebase_task_inventory"
            && text(&scope, "evidence_state")? == "static_inventory"
            && text(&lock, "inventory_status")? == "frozen",
        "migration_scope_inventory_semantics_mismatch"
    );
    let expected: BTreeMap<_, _> = EXPECTED.iter().map(|(k, v)| (*k, *v)).collect();
    let counts = scope["counts"]
        .as_object()
        .context("migration_scope_missing_counts")?;
    ensure!(
        counts.len() == expected.len(),
        "migration_scope_count_keys_mismatch"
    );
    for (section, count) in EXPECTED {
        ensure!(
            counts.get(*section).and_then(Value::as_u64) == Some(*count),
            "migration_scope_count_mismatch:{section}"
        );
        if let Some(array) = scope.get(*section).and_then(Value::as_array) {
            ensure!(
                array.len() as u64 == *count,
                "migration_scope_missing_full_rows:{section}"
            );
        }
    }
    let mut input_paths = BTreeSet::new();
    ensure!(
        rows(&scope, "inputs")?.len() == 4,
        "migration_scope_frozen_inputs_missing"
    );
    for row in rows(&scope, "inputs")? {
        let p = text(row, "path")?;
        safe_path(p)?;
        digest(text(row, "sha256")?)?;
        ensure!(input_paths.insert(p), "migration_scope_duplicate_input");
    }
    let mut tracked = BTreeMap::new();
    for row in rows(&scope, "tracked_files")? {
        let (path, sha, bytes) = file_identity(row)?;
        ensure!(
            tracked.insert(path, (sha, bytes)).is_none(),
            "migration_scope_duplicate_tracked_path"
        );
    }
    let mut items = Vec::new();
    let mut all_ids = BTreeSet::new();
    for (section, kind) in FILE_SECTIONS {
        let mut paths = BTreeSet::new();
        for row in rows(&scope, section)? {
            let (path, sha, bytes) = file_identity(row)?;
            ensure!(
                paths.insert(path),
                "migration_scope_duplicate_source_path:{section}"
            );
            ensure!(
                tracked.get(path) == Some(&(sha, bytes)),
                "migration_scope_source_identity_mismatch"
            );
            let id = format!("{path}|{kind}||0");
            ensure!(
                all_ids.insert(id.clone()),
                "migration_scope_duplicate_scope_id"
            );
            items.push(json!({"id":id,"path":path,"kind":kind,"sha256":sha,"bytes":bytes,"migration_status":"unknown","behavior_status":"unknown","evidence_state":"static_identity"}));
        }
    }
    let mut kinds = BTreeMap::<String, u64>::new();
    for section in SYMBOL_SECTIONS {
        for row in rows(&scope, section)? {
            let id = symbol_id(row, section)?;
            let path = text(row, "path")?;
            ensure!(
                tracked.contains_key(path),
                "migration_scope_api_source_not_tracked"
            );
            ensure!(all_ids.insert(id), "migration_scope_duplicate_scope_id");
            if *section == "public_symbols" {
                *kinds.entry(text(row, "kind")?.to_owned()).or_default() += 1;
            }
            let mut item = row.clone();
            item.as_object_mut()
                .context("migration_scope_invalid_api_row")?
                .insert("migration_status".into(), json!("unknown"));
            items.push(item);
        }
    }
    ensure!(
        kinds.get("class") == Some(&862)
            && kinds.get("function") == Some(&607)
            && kinds.get("method") == Some(&2446)
            && kinds.len() == 3,
        "migration_scope_public_symbol_kinds_mismatch"
    );
    let assertions: u64 = rows(&scope, "python_test_sources")?
        .iter()
        .map(|r| {
            r["assertions"]
                .as_u64()
                .context("migration_scope_test_assertions_missing")
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum();
    ensure!(
        assertions == 7777,
        "migration_scope_original_test_assertions_mismatch"
    );
    let scope_sha = oe_core::sha256(&scope_raw);
    ensure!(
        scope_sha == text(&lock, "scope_sha256")?
            && scope_sha == SCOPE_SHA256
            && lock["scope_bytes"].as_u64() == Some(scope_raw.len() as u64),
        "migration_scope_frozen_sha_mismatch"
    );
    ensure!(
        oe_core::sha256(&lock_raw) == LOCK_SHA256,
        "migration_scope_compiled_lock_mismatch"
    );
    let design_mappings = design_declarations(root)?;
    let declared_mappings = explicit_declarations(root, &all_ids)?;
    // The port ledger may give items a verified status; only complete, fresh and pinned
    // recorded port evidence changes the whole-scope status (migration_ledger.rs).
    let tracked_shas: BTreeMap<&str, &str> = tracked.iter().map(|(p, (s, _))| (*p, *s)).collect();
    let ledger = crate::migration_ledger::verify(root, &items, &tracked_shas)?;
    let (migration_status, behavior_status) = match &ledger {
        Some(l) if l.complete => ("complete", "verified_by_recorded_port_tests"),
        _ => ("incomplete", "unknown"),
    };
    if let Some(l) = &ledger {
        for item in &mut items {
            if let Some(status) = item["id"].as_str().and_then(|id| l.statuses.get(id)) {
                item["migration_status"] = json!(status);
            }
        }
    }
    let mut report = json!({"schema":"semantica.migration-audit/1","scope_sha256":scope_sha,"scope_lock_sha256":LOCK_SHA256,"identity":scope["identity"],"inputs":scope["inputs"],"counts":scope["counts"],"inventory_integrity":"matched","migration_status":migration_status,"behavior_status":behavior_status,"items":items,"design_mappings":design_mappings,"declared_mappings":declared_mappings,"verification_policy":"static candidates and mapping declarations require separate source-bound behavioral and transfer evidence; only the pinned port ledger with fresh recorded port tests can verify items"});
    if let Some(l) = ledger {
        report["port"] = l.report;
    }
    Ok(report)
}

fn design_declarations(root: &Path) -> Result<Vec<Value>> {
    let path = root.join("specs/engineering.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let specs =
        fs::symlink_metadata(root.join("specs")).context("migration_scope_specs_missing")?;
    ensure!(
        specs.is_dir() && !specs.file_type().is_symlink(),
        "migration_scope_unsafe_specs_path"
    );
    let raw = bounded_file(&path)?;
    let source = std::str::from_utf8(&raw).context("migration_scope_design_not_utf8")?;
    let design: toml::Value = toml::from_str(source).context("migration_scope_invalid_design")?;
    let mut references = BTreeSet::new();
    for row in design
        .get("sources")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let identity = row
            .get("identity")
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        if identity.contains(COMMIT) || identity.contains(WHEEL_SHA256) {
            if let Some(id) = row.get("id").and_then(toml::Value::as_str) {
                references.insert(id);
            }
        }
    }
    let mut out = Vec::new();
    for row in design
        .get("modules")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let refs: Vec<_> = row
            .get("source_refs")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .filter(|id| references.contains(id))
            .collect();
        if refs.is_empty() {
            continue;
        }
        let module = row
            .get("id")
            .and_then(toml::Value::as_str)
            .context("migration_scope_invalid_design_module")?;
        let path = row
            .get("rust_path")
            .and_then(toml::Value::as_str)
            .context("migration_scope_missing_design_path")?;
        safe_path(path)?;
        out.push(json!({"module":module,"rust_path":path,"source_refs":refs,"claimed_status":row.get("status").and_then(toml::Value::as_str),"status":"declared","behavior_status":"unknown","scope_binding":"whole_source_reference_without_per_api_verification"}));
    }
    Ok(out)
}
fn explicit_declarations(root: &Path, ids: &BTreeSet<String>) -> Result<Vec<Value>> {
    let path = root.join("runtime/semantica-migration-mappings.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let v = parse(&bounded_file(&path)?)?;
    ensure!(
        text(&v, "schema")? == "semantica.migration-mappings/1"
            && text(&v, "scope_sha256")? == SCOPE_SHA256,
        "migration_scope_mapping_binding_mismatch"
    );
    let object = v
        .as_object()
        .context("migration_scope_mapping_not_object")?;
    ensure!(
        object
            .keys()
            .all(|k| ["schema", "scope_sha256", "claims"].contains(&k.as_str())),
        "migration_scope_mapping_unknown_field"
    );
    let mut out = Vec::new();
    let mut declared = BTreeSet::new();
    for row in rows(&v, "claims")? {
        ensure!(
            row.as_object().is_some_and(|o| o.keys().all(|k| [
                "scope_ids",
                "module",
                "rust_path",
                "status"
            ]
            .contains(&k.as_str()))),
            "migration_scope_mapping_unknown_claim"
        );
        ensure!(
            text(row, "status")? == "declared",
            "migration_scope_mapping_unearned_status"
        );
        let path = text(row, "rust_path")?;
        safe_path(path)?;
        let module = text(row, "module")?;
        ensure!(
            !module.is_empty() && !module.chars().any(char::is_control),
            "migration_scope_mapping_invalid_module"
        );
        let scope_ids = rows(row, "scope_ids")?;
        ensure!(!scope_ids.is_empty(), "migration_scope_mapping_empty_claim");
        for id in scope_ids {
            let id = id.as_str().context("migration_scope_mapping_invalid_id")?;
            ensure!(ids.contains(id), "migration_scope_mapping_unknown_id");
            ensure!(
                declared.insert((id.to_owned(), module.to_owned(), path.to_owned())),
                "migration_scope_mapping_duplicate_claim"
            );
        }
        out.push(json!({"scope_ids":scope_ids,"module":module,"rust_path":path,"status":"declared","behavior_status":"unknown"}));
    }
    Ok(out)
}
