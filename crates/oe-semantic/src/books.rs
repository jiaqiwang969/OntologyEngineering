//! Rust book facade. Legacy backend fixtures remain explicit historical tests;
//! the default entry uses the neutral Semantica library and v2 source adapter.
pub mod mcp;
pub mod rust;
use crate::native::{NativeExecution, NativeRuntime};
use crate::{canonical_json, digest, safe_relative, strict_json};
use anyhow::{Context, Result, ensure};
use oe_core::{
    SkillRoot, create_new_private_dir, fingerprint_file, no_symlinks, read_regular_bytes,
    write_new_bytes,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SCHEMA: &str = "ontology-engineering.book-native-response/v1";
const MAX_FILE: usize = 16 * 1024 * 1024;
const MAX_TOTAL: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 8192;
const BOOKS: [&str; 2] = [
    "references/ontology-engineering-book",
    "references/product-trustworthiness-book",
];
const PART1: &str =
    "part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json";
const PART3: &str =
    "part-03-concept-phase/native-full/ISO 26262-3-2018/auto/ISO 26262-3-2018_content_list_v2.json";
const GLOSSARY: &str = "appendices/appendix-c-glossary.md";

#[derive(Debug, Clone)]
pub struct EngraveOptions {
    pub source_root: PathBuf,
    pub book_root: PathBuf,
    pub output: PathBuf,
    pub parts: Vec<u8>,
    pub overwrite: bool,
}
#[derive(Debug, Clone)]
pub enum BookRequest {
    MigrationMap {
        volume: String,
    },
    ResolveSuccessor {
        old_path: String,
        volume: Option<String>,
    },
    AssetText {
        package_id: String,
        asset_id: String,
    },
    ChapterAssetText {
        volume: String,
        chapter: String,
        asset_id: String,
    },
    VerifyBookSources,
    Engrave(EngraveOptions),
}
/// Fixtures replace only external execution. Production uses NativeRuntime's
/// per-call source lock, RECORD and exact-child identity verification.
pub trait BookBackend {
    fn execute_book(&self, request: &Value) -> NativeExecution;
}
impl BookBackend for NativeRuntime {
    fn execute_book(&self, request: &Value) -> NativeExecution {
        self.book_api(request)
    }
}
pub fn read_migration_map(root: &SkillRoot, volume: &str) -> Result<Value> {
    execute(
        root,
        &BookRequest::MigrationMap {
            volume: volume.into(),
        },
    )
}
pub fn resolve_successor(root: &SkillRoot, old_path: &str, volume: Option<&str>) -> Result<Value> {
    execute(
        root,
        &BookRequest::ResolveSuccessor {
            old_path: old_path.into(),
            volume: volume.map(str::to_owned),
        },
    )
}
pub fn asset_text(root: &SkillRoot, package_id: &str, asset_id: &str) -> Result<Value> {
    execute(
        root,
        &BookRequest::AssetText {
            package_id: package_id.into(),
            asset_id: asset_id.into(),
        },
    )
}
pub fn chapter_asset_text(
    root: &SkillRoot,
    volume: &str,
    chapter: &str,
    asset_id: &str,
) -> Result<Value> {
    execute(
        root,
        &BookRequest::ChapterAssetText {
            volume: volume.into(),
            chapter: chapter.into(),
            asset_id: asset_id.into(),
        },
    )
}
pub fn verify_book_sources(root: &SkillRoot) -> Result<Value> {
    execute(root, &BookRequest::VerifyBookSources)
}
pub fn engrave(root: &SkillRoot, options: &EngraveOptions) -> Result<Value> {
    execute(root, &BookRequest::Engrave(options.clone()))
}
pub fn execute(root: &SkillRoot, request: &BookRequest) -> Result<Value> {
    rust::execute(root, request)
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    oe_core::text(&value[key], key)
}
fn id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control),
        "book_identifier_invalid"
    );
    Ok(())
}
fn volume(value: &str) -> Result<()> {
    ensure!(matches!(value, "vol1" | "vol2"), "book_volume_invalid");
    Ok(())
}
fn hash(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "book_digest_invalid"
    );
    Ok(())
}
fn exact(value: &Value, fields: &[&str]) -> Result<()> {
    let object = value.as_object().context("book_request_not_object")?;
    ensure!(
        object.len() == fields.len() && fields.iter().all(|k| object.contains_key(*k)),
        "book_request_fields_invalid"
    );
    Ok(())
}
fn rooted(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.expand(path)?;
    ensure!(path.starts_with(root.path()), "book_path_outside_root");
    no_symlinks(&path)?;
    Ok(path)
}
fn directory(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = rooted(root, path)?;
    ensure!(path.is_dir(), "book_directory_missing");
    Ok(path)
}
fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
/// Kept at the native boundary too: direct Rust callers cannot bypass argument
/// or rooted filesystem policy by constructing their own fixed-operation DTO.
pub(crate) fn validate_native_request(root: &SkillRoot, value: &Value) -> Result<()> {
    match field(value, "operation")? {
        "book_migration_map" => {
            exact(value, &["operation", "volume"])?;
            volume(field(value, "volume")?)?;
        }
        "book_resolve_successor" => {
            exact(value, &["operation", "old_path", "volume"])?;
            safe_relative(field(value, "old_path")?)?;
            if !value["volume"].is_null() {
                volume(field(value, "volume")?)?;
            }
        }
        "book_asset_text" => {
            exact(value, &["operation", "package_id", "asset_id"])?;
            id(field(value, "package_id")?)?;
            id(field(value, "asset_id")?)?;
        }
        "book_chapter_asset_text" => {
            exact(value, &["operation", "volume", "chapter", "asset_id"])?;
            volume(field(value, "volume")?)?;
            id(field(value, "chapter")?)?;
            id(field(value, "asset_id")?)?;
        }
        "book_verify_sources" => {
            exact(value, &["operation", "book_root"])?;
            directory(root, Path::new(field(value, "book_root")?))?;
        }
        "book_engrave" => {
            exact(
                value,
                &[
                    "operation",
                    "source_root",
                    "book_root",
                    "output",
                    "parts",
                    "overwrite",
                ],
            )?;
            let source = directory(root, Path::new(field(value, "source_root")?))?;
            ensure!(
                source.starts_with(root.path().join("var"))
                    || source.starts_with(root.path().join("sources")),
                "controlled_sources_must_be_private_or_source_library"
            );
            let book = directory(root, Path::new(field(value, "book_root")?))?;
            let output = root.private_path(Path::new(field(value, "output")?))?;
            ensure!(
                !overlap(&output, &source) && !overlap(&output, &book),
                "engraving_output_overlaps_input"
            );
            ensure!(
                output.parent().is_some_and(Path::is_dir),
                "engraving_output_parent_missing"
            );
            let overwrite = value["overwrite"]
                .as_bool()
                .context("engraving_overwrite_not_boolean")?;
            ensure!(
                !output.exists() || (overwrite && output.is_dir()),
                "engraving_existing_output_requires_overwrite"
            );
            let parts = value["parts"]
                .as_array()
                .context("engraving_parts_invalid")?;
            let mut seen = BTreeSet::new();
            ensure!(!parts.is_empty(), "engraving_parts_empty");
            for part in parts {
                let part = part.as_u64().context("engraving_part_not_integer")?;
                ensure!(
                    matches!(part, 1 | 3) && seen.insert(part),
                    "engraving_part_invalid_or_duplicate"
                );
            }
        }
        _ => anyhow::bail!("unsupported_book_api"),
    }
    Ok(())
}
fn request_value(root: &SkillRoot, request: &BookRequest) -> Result<Value> {
    let value = match request {
        BookRequest::MigrationMap { volume } => {
            json!({"operation":"book_migration_map","volume":volume})
        }
        BookRequest::ResolveSuccessor { old_path, volume } => {
            json!({"operation":"book_resolve_successor","old_path":old_path,"volume":volume})
        }
        BookRequest::AssetText {
            package_id,
            asset_id,
        } => json!({"operation":"book_asset_text","package_id":package_id,"asset_id":asset_id}),
        BookRequest::ChapterAssetText {
            volume,
            chapter,
            asset_id,
        } => {
            json!({"operation":"book_chapter_asset_text","volume":volume,"chapter":chapter,"asset_id":asset_id})
        }
        BookRequest::VerifyBookSources => {
            json!({"operation":"book_verify_sources","book_root":root.path()})
        }
        BookRequest::Engrave(o) => {
            json!({"operation":"book_engrave","source_root":rooted(root,&o.source_root)?,"book_root":rooted(root,&o.book_root)?,"output":root.private_path(&o.output)?,"parts":o.parts,"overwrite":o.overwrite})
        }
    };
    validate_native_request(root, &value)?;
    Ok(value)
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Snapshot {
    path: PathBuf,
    inputs: BTreeMap<PathBuf, String>,
    bytes: usize,
}
impl Snapshot {
    fn new(root: &SkillRoot) -> Result<Self> {
        let path = root.new_private_dir(Path::new(&format!(
            "var/book-input-{}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))?;
        Ok(Self {
            path,
            inputs: BTreeMap::new(),
            bytes: 0,
        })
    }
    fn copy(&mut self, source: &Path, target: &Path) -> Result<()> {
        let bytes = read_regular_bytes(source, MAX_FILE)?;
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .context("book_input_size_overflow")?;
        ensure!(
            self.bytes <= MAX_TOTAL && self.inputs.len() < MAX_FILES,
            "book_input_budget"
        );
        if !target.parent().context("book_snapshot_parent")?.exists() {
            create_new_private_dir(target.parent().unwrap())?;
        }
        write_new_bytes(target, &bytes)?;
        self.inputs.insert(source.to_owned(), digest(&bytes));
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        for (path, expected) in &self.inputs {
            ensure!(
                &fingerprint_file(path, MAX_FILE as u64)?.sha256 == expected,
                "book_source_changed_during_native_call"
            );
        }
        Ok(())
    }
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn book_tree(
    snapshot: &mut Snapshot,
    path: &Path,
    root: &Path,
    target: &Path,
    count: &mut usize,
) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    *count += 1;
    ensure!(*count <= MAX_FILES, "book_tree_entry_budget");
    ensure!(!meta.file_type().is_symlink(), "book_source_symlink");
    if meta.is_dir() {
        for item in fs::read_dir(path)? {
            book_tree(snapshot, &item?.path(), root, target, count)?;
        }
    } else {
        ensure!(meta.is_file(), "book_source_special_file");
        // The locked book source contract uses Markdown and TeX anchors. Large
        // rendered PDFs/images are neither read nor copied as source inputs.
        if path.extension().is_some_and(|s| s == "md" || s == "tex") {
            snapshot.copy(path, &target.join(path.strip_prefix(root)?))?;
        }
    }
    Ok(())
}
fn output_inventory(path: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    if !path.exists() {
        return Ok(result);
    }
    for item in fs::read_dir(path)? {
        let path = item?.path();
        no_symlinks(&path)?;
        let meta = fs::symlink_metadata(&path)?;
        // Unrelated directories are preserved by the external implementation;
        // their entries are not read as potential generated artifacts.
        ensure!(
            meta.is_file() || meta.is_dir(),
            "engraving_output_special_file"
        );
        let name = path
            .file_name()
            .context("engraving_name")?
            .to_str()
            .context("engraving_name_utf8")?
            .to_owned();
        let value = if meta.is_file() {
            fingerprint_file(&path, MAX_FILE as u64)?.sha256
        } else {
            "directory".into()
        };
        ensure!(result.len() < MAX_FILES, "engraving_output_entry_budget");
        result.insert(name, value);
    }
    Ok(result)
}

pub fn execute_with(
    root: &SkillRoot,
    request: &BookRequest,
    backend: &dyn BookBackend,
) -> Result<Value> {
    let original = request_value(root, request)?;
    let operation = field(&original, "operation")?.to_owned();
    let lock = crate::runtime::read_runtime_source_lock(root.path(), false)?;
    let mut call = original.clone();
    let mut snapshot = None;
    let mut previous = BTreeMap::new();
    if matches!(
        request,
        BookRequest::VerifyBookSources | BookRequest::Engrave(_)
    ) {
        let mut captured = Snapshot::new(root)?;
        let book = captured.path.join("book");
        create_new_private_dir(&book)?;
        match request {
            BookRequest::VerifyBookSources => {
                let mut count = 0;
                for name in BOOKS {
                    let source = root.path().join(name);
                    if fs::symlink_metadata(&source).is_ok() {
                        book_tree(&mut captured, &source, root.path(), &book, &mut count)?;
                    }
                }
                call["book_root"] = json!(book);
            }
            BookRequest::Engrave(options) => {
                let source = captured.path.join("source");
                create_new_private_dir(&source)?;
                for part in &options.parts {
                    let rel = if *part == 1 { PART1 } else { PART3 };
                    captured.copy(
                        &Path::new(field(&original, "source_root")?).join(rel),
                        &source.join(rel),
                    )?;
                }
                if options.parts.contains(&1) {
                    captured.copy(
                        &Path::new(field(&original, "book_root")?).join(GLOSSARY),
                        &book.join(GLOSSARY),
                    )?;
                }
                previous = output_inventory(Path::new(field(&original, "output")?))?;
                call["source_root"] = json!(source);
                call["book_root"] = json!(book);
            }
            _ => unreachable!(),
        }
        captured.verify()?;
        snapshot = Some(captured);
    }
    let execution = backend.execute_book(&call);
    let mut response = json!({"schema":SCHEMA,"operation":operation,"status":"blocked","scope":"source-locked native book APIs; no project fact adoption or publication authority","fact_admitted":false,"ontology_promotion":"not_performed","publication":"not_performed","result":execution.result,"native_execution":execution});
    if let Some(captured) = &snapshot {
        response["input_sha256"] = json!(
            captured
                .inputs
                .iter()
                .map(|(p, h)| Ok((
                    p.strip_prefix(root.path())?
                        .to_str()
                        .context("book_source_path_utf8")?
                        .to_owned(),
                    h.clone()
                )))
                .collect::<Result<BTreeMap<_, _>>>()?
        );
    }
    let checked = (|| -> Result<()> {
        ensure!(
            execution.semantic_execution == "completed",
            "native_book_api_not_completed"
        );
        let proof = execution
            .runtime_identity
            .as_ref()
            .context("native_book_runtime_identity_missing")?;
        ensure!(
            proof.source_lock == lock
                && proof.record_files > 0
                && proof.staging_descriptor_sha256.is_none(),
            "native_book_runtime_identity_mismatch"
        );
        ensure!(
            crate::runtime::read_runtime_source_lock(root.path(), false)? == lock,
            "native_book_source_lock_changed"
        );
        if let Some(captured) = &snapshot {
            captured.verify()?;
        }
        let value = execution
            .result
            .as_ref()
            .context("native_book_result_missing")?;
        validate_result(root, &original, value, snapshot.as_ref(), &previous)?;
        response["status"] = if operation == "book_verify_sources" && value["passed"] != true {
            json!("blocked")
        } else {
            json!("completed")
        };
        if matches!(
            request,
            BookRequest::AssetText { .. } | BookRequest::ChapterAssetText { .. }
        ) {
            response["text_sha256"] = json!(digest(
                value["text"]
                    .as_str()
                    .context("book_asset_not_text")?
                    .as_bytes()
            ));
        }
        Ok(())
    })();
    if checked.is_err() {
        response["status"] = json!("blocked");
        response["validation_error"] = json!("native_book_result_or_source_identity_not_verified");
    }
    if let BookRequest::Engrave(_) = request {
        response["output"] = original["output"].clone();
        response["output_state"] = json!(if Path::new(field(&original, "output")?).exists() {
            "artifacts_may_exist_inspect_native_result"
        } else {
            "not_created"
        });
    }
    Ok(response)
}
fn validate_successor(value: &Value) -> Result<()> {
    volume(field(value, "volume")?)?;
    safe_relative(field(value, "old_path")?)?;
    for k in ["old_sha256", "successor_sha256"] {
        hash(field(value, k)?)?;
    }
    for k in ["package_id", "asset_id", "relation"] {
        id(field(value, k)?)?;
    }
    Ok(())
}
fn validate_result(
    root: &SkillRoot,
    request: &Value,
    value: &Value,
    snapshot: Option<&Snapshot>,
    previous: &BTreeMap<String, String>,
) -> Result<()> {
    match field(request, "operation")? {
        "book_migration_map" => {
            ensure!(
                value["schema_version"] == "1.0" && value["volume"] == request["volume"],
                "book_map_identity"
            );
            hash(field(value, "source_sha256")?)?;
            let entries = value["entries"].as_array().context("book_map_entries")?;
            ensure!(
                value["entry_count"].as_u64() == Some(entries.len() as u64),
                "book_map_count"
            );
            for entry in entries {
                validate_successor(entry)?;
                ensure!(entry["volume"] == request["volume"], "book_map_volume");
            }
            let retired = value["retired_runtime_entries"]
                .as_array()
                .context("book_retired_entries")?;
            ensure!(
                value["retired_runtime_entry_count"].as_u64() == Some(retired.len() as u64),
                "book_retired_count"
            );
        }
        "book_resolve_successor" => {
            ensure!(
                value["old_path"] == request["old_path"],
                "book_resolution_path"
            );
            let candidates = value["candidates"]
                .as_array()
                .context("book_resolution_candidates")?;
            ensure!(
                value["resolved"] == !candidates.is_empty()
                    && value["ambiguous"] == (candidates.len() > 1),
                "book_resolution_ambiguity"
            );
            for candidate in candidates {
                validate_successor(candidate)?;
                ensure!(
                    candidate["old_path"] == request["old_path"]
                        && (request["volume"].is_null()
                            || candidate["volume"] == request["volume"]),
                    "book_resolution_identity"
                );
            }
        }
        "book_asset_text" | "book_chapter_asset_text" => {
            for k in if request["operation"] == "book_asset_text" {
                vec!["package_id", "asset_id"]
            } else {
                vec!["volume", "chapter", "asset_id"]
            } {
                ensure!(value[k] == request[k], "book_asset_identity");
            }
            let text = value["text"].as_str().context("book_asset_not_text")?;
            ensure!(!text.contains('\0'), "book_asset_binary");
        }
        "book_verify_sources" => {
            let checks = value["checks"].as_array().context("book_checks_missing")?;
            ensure!(
                value["schema_version"] == "1.0"
                    && value["check_count"].as_u64() == Some(checks.len() as u64),
                "book_check_count"
            );
            let passed = !checks.is_empty() && checks.iter().all(|c| c["status"] == "passed");
            ensure!(
                value["passed"] == passed
                    && value["status"] == if passed { "passed" } else { "blocked" },
                "book_check_status"
            );
            let snapshot = snapshot.context("book_input_snapshot_missing")?;
            for check in checks {
                ensure!(
                    matches!(check["status"].as_str(), Some("passed" | "blocked")),
                    "book_check_invalid_status"
                );
                id(field(check, "check_id")?)?;
                id(field(check, "package_id")?)?;
                if check["status"] == "passed" {
                    hash(field(check, "expected_sha256")?)?;
                    hash(field(check, "actual_sha256")?)?;
                    ensure!(
                        check["actual_sha256"] == check["expected_sha256"],
                        "book_passed_check_digest_mismatch"
                    );
                }
                if field(check, "check_id")?.ends_with(".digest")
                    && let Some(actual) = check["actual_sha256"].as_str()
                {
                    hash(actual)?;
                    let logical = safe_relative(field(check, "source_anchor")?)?;
                    // The native Vol.2 source contract carries the repository
                    // name as a logical prefix; it is not another directory.
                    let anchor = logical
                        .strip_prefix("ontology-engineering/")
                        .unwrap_or(logical);
                    ensure!(
                        BOOKS.iter().any(|p| anchor.starts_with(&format!("{p}/"))),
                        "book_anchor_outside_corpus"
                    );
                    let source = rooted(root, &root.path().join(anchor))?;
                    ensure!(
                        snapshot.inputs.get(&source).is_some_and(|h| h == actual),
                        "book_source_observation_mismatch"
                    );
                }
            }
        }
        "book_engrave" => validate_engraving(
            request,
            value,
            snapshot.context("engraving_snapshot_missing")?,
            previous,
        )?,
        _ => anyhow::bail!("unsupported_book_result"),
    }
    Ok(())
}
fn validate_engraving(
    request: &Value,
    value: &Value,
    snapshot: &Snapshot,
    previous: &BTreeMap<String, String>,
) -> Result<()> {
    ensure!(
        value["schema_version"] == "1.0"
            && value["restricted_standard_text_included"] == false
            && value["output_directory"] == request["output"],
        "engraving_identity"
    );
    for k in ["package_id", "package_version"] {
        id(field(value, k)?)?;
    }
    for k in ["package_digest", "bundle_sha256", "semantic_dataset_sha256"] {
        hash(field(value, k)?)?;
    }
    let parts = value["parts"]
        .as_array()
        .context("engraving_parts_missing")?;
    let mut requested = request["parts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_u64)
        .collect::<Vec<_>>();
    requested.sort();
    ensure!(
        parts
            .iter()
            .map(|p| p["part"].as_u64())
            .collect::<Option<Vec<_>>>()
            == Some(requested.clone()),
        "engraving_parts_mismatch"
    );
    let sources = value["source_hashes"]
        .as_object()
        .context("engraving_source_hashes_missing")?;
    for part in &requested {
        let suffix = if *part == 1 { PART1 } else { PART3 };
        let expected = snapshot
            .inputs
            .iter()
            .find(|(p, _)| p.ends_with(suffix))
            .map(|(_, h)| h)
            .context("engraving_input_missing")?;
        ensure!(
            sources.get(&format!("controlled.part{part}.extract")) == Some(&json!(expected)),
            "engraving_input_hash_mismatch"
        );
    }
    if requested.contains(&1) {
        let expected = snapshot
            .inputs
            .iter()
            .find(|(p, _)| p.ends_with(GLOSSARY))
            .map(|(_, h)| h)
            .context("engraving_glossary_missing")?;
        ensure!(
            sources.get("book.appendix-c-glossary") == Some(&json!(expected)),
            "engraving_glossary_hash_mismatch"
        );
    }
    let output = Path::new(field(request, "output")?);
    no_symlinks(output)?;
    let artifacts = value["artifacts"]
        .as_array()
        .context("engraving_artifacts_missing")?;
    ensure!(
        !artifacts.is_empty() && artifacts.len() <= 64,
        "engraving_artifact_budget"
    );
    let mut names = BTreeSet::new();
    let mut report = None;
    for artifact in artifacts {
        let relative = safe_relative(field(artifact, "relative_path")?)?;
        ensure!(
            !relative.contains('/') && names.insert(relative.to_owned()),
            "engraving_artifact_path_or_duplicate"
        );
        let raw = read_regular_bytes(&output.join(relative), MAX_FILE)?;
        ensure!(
            artifact["size_bytes"].as_u64() == Some(raw.len() as u64)
                && artifact["sha256"] == digest(&raw),
            "engraving_artifact_identity"
        );
        if relative == "engraving-report.json" {
            report = Some(strict_json(&raw)?);
        }
    }
    let report = report.context("engraving_report_missing")?;
    let mut content = report.clone();
    content
        .as_object_mut()
        .context("engraving_report_not_object")?
        .remove("bundle_sha256");
    ensure!(
        report["bundle_sha256"] == digest(canonical_json(&content)?)
            && report["bundle_sha256"] == value["bundle_sha256"],
        "engraving_bundle_hash"
    );
    for k in [
        "schema_version",
        "package_id",
        "package_version",
        "package_digest",
        "parts",
        "source_hashes",
        "semantic_dataset_sha256",
        "restricted_standard_text_included",
    ] {
        ensure!(report[k] == value[k], "engraving_report_dto_mismatch");
    }
    let without_report = artifacts
        .iter()
        .filter(|a| a["relative_path"] != "engraving-report.json")
        .cloned()
        .collect::<Vec<_>>();
    ensure!(
        report["artifacts"] == json!(without_report),
        "engraving_report_artifact_mismatch"
    );
    let after = output_inventory(output)?;
    ensure!(
        after
            .keys()
            .all(|p| names.contains(p) || previous.contains_key(p)),
        "engraving_undeclared_output"
    );
    for (name, expected) in previous {
        if !names.contains(name) {
            ensure!(
                after.get(name) == Some(expected),
                "engraving_unrelated_output_changed"
            );
        }
    }
    Ok(())
}
