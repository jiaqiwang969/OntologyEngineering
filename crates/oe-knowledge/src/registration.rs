//! Explicit local-PDF registration. Originals remain in sources/; publication is
//! a two-file index snapshot with rollback, never a query-time rebuild.
use crate::{
    poppler::{self, PopplerLimits, PopplerTools},
    source_pack::{Stage, file_hash, valid_sha, within_root},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
mod incremental;

pub const TABLES:&str="CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE chapters(id TEXT PRIMARY KEY,catalog TEXT,title TEXT,start_page INTEGER,end_page INTEGER,source_json TEXT);
CREATE TABLE pages(id TEXT PRIMARY KEY,catalog TEXT,page_id INTEGER,pdf_page INTEGER,printed_page TEXT,chapter_id TEXT,source_relative_path TEXT,source_url TEXT,source_sha256 TEXT,source_bytes INTEGER,body TEXT,text_sha256 TEXT,raw_text_sha256 TEXT,raw_text_chars INTEGER,text_chars INTEGER,native_text_sparse INTEGER,titles_json TEXT,series_json TEXT,series_count INTEGER,source_json TEXT);
CREATE INDEX page_chapter ON pages(chapter_id,pdf_page);CREATE INDEX page_position ON pages(catalog,pdf_page);
CREATE VIRTUAL TABLE search USING fts5(models,titles,body,chapter);";

#[derive(Debug, Clone)]
pub struct RegistrationOptions {
    pub data_root: PathBuf,
    pub replace: bool,
    pub merge: bool,
    pub dry_run: bool,
    pub tools: PopplerTools,
    pub limits: PopplerLimits,
}
impl Default for RegistrationOptions {
    fn default() -> Self {
        Self {
            data_root: "sources/.indexes/local-pdf".into(),
            replace: false,
            merge: false,
            dry_run: false,
            tools: Default::default(),
            limits: Default::default(),
        }
    }
}
fn keys(value: &Value, expected: &[&str]) -> Result<()> {
    let object = value.as_object().context("local_pdf_object_required")?;
    ensure!(
        object.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == expected.iter().copied().collect(),
        "local_pdf_unexpected_fields"
    );
    Ok(())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let s = value[key].as_str().context("local_pdf_text_required")?;
    ensure!(
        !s.trim().is_empty() && s.chars().count() <= 2000,
        "local_pdf_text_invalid"
    );
    Ok(s)
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}
fn encoded(value: &Value) -> Result<String> {
    Ok(String::from_utf8(oe_core::json::canonical_json(value)?)?)
}
fn manifest_sources(root: &Path, path: &Path, raw: &[u8]) -> Result<Vec<Value>> {
    let path = within_root(root, path)?;
    let document = oe_core::json::strict_json(raw)?;
    keys(&document, &["schema", "sources"])?;
    ensure!(
        document["schema"] == "ontology-engineering.local-pdf-manifest/v1",
        "local_pdf_manifest_invalid"
    );
    let sources = document["sources"]
        .as_array()
        .context("local_pdf_sources_missing")?;
    ensure!(
        !sources.is_empty() && sources.len() <= 256,
        "local_pdf_source_count_invalid"
    );
    let mut ids = BTreeSet::new();
    let mut result = Vec::new();
    let source_root = oe_core::SkillRoot::new(root)?.path().join("sources");
    for source in sources {
        keys(
            source,
            &["id", "title", "edition", "path", "sha256", "chapters"],
        )?;
        let sid = text(source, "id")?;
        ensure!(
            id(sid) && ids.insert(sid),
            "local_pdf_source_id_invalid_or_duplicate"
        );
        text(source, "title")?;
        text(source, "edition")?;
        let hash = text(source, "sha256")?;
        ensure!(valid_sha(hash), "local_pdf_source_hash_invalid");
        let original = Path::new(text(source, "path")?);
        let absolute = if original.is_absolute() {
            original.to_path_buf()
        } else {
            path.parent().context("manifest_parent")?.join(original)
        };
        let original = within_root(root, &absolute)?;
        ensure!(
            original.starts_with(&source_root)
                && !original.starts_with(source_root.join(".indexes")),
            "local_pdf_original_must_be_in_sources_outside_indexes"
        );
        let (mut chapter_ids, mut chapters) = (BTreeSet::new(), Vec::new());
        for chapter in source["chapters"]
            .as_array()
            .context("local_pdf_chapters_invalid")?
        {
            keys(
                chapter,
                &["id", "title", "start_page", "end_page", "aliases"],
            )?;
            let cid = text(chapter, "id")?;
            ensure!(
                id(cid) && chapter_ids.insert(cid),
                "local_pdf_chapter_id_invalid_or_duplicate"
            );
            text(chapter, "title")?;
            let start = chapter["start_page"]
                .as_u64()
                .context("local_pdf_chapter_range_invalid")?;
            let end = chapter["end_page"]
                .as_u64()
                .context("local_pdf_chapter_range_invalid")?;
            ensure!(start > 0 && start <= end, "local_pdf_chapter_range_invalid");
            let aliases = chapter["aliases"]
                .as_array()
                .context("local_pdf_aliases_invalid")?;
            ensure!(aliases.len() <= 40, "local_pdf_aliases_invalid");
            for alias in aliases {
                ensure!(
                    alias
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 2000),
                    "local_pdf_alias_invalid"
                );
            }
            chapters.push(chapter.clone());
        }
        let mut source = source.clone();
        source["path"] = json!(original);
        source["chapters"] = json!(chapters);
        result.push(source);
    }
    Ok(result)
}
fn normalize_body(raw: &str) -> String {
    let mut body = String::new();
    let mut horizontal = false;
    for c in raw.chars() {
        if c == ' ' || c == '\t' {
            if !horizontal {
                body.push(' ')
            }
            horizontal = true
        } else {
            body.push(c);
            horizontal = false
        }
    }
    body
}
fn relative_original(original: &Path, index: &Path) -> Result<String> {
    let a: Vec<_> = original.components().collect();
    let b: Vec<_> = index.components().collect();
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let mut result = PathBuf::new();
    for _ in common..b.len() {
        result.push("..");
    }
    for c in &a[common..] {
        result.push(c.as_os_str());
    }
    Ok(result.to_str().context("source_path_utf8")?.to_owned())
}
fn owned_snapshot(target: &Path) -> Result<Value> {
    oe_core::no_symlinks(target)?;
    ensure!(
        target.is_dir(),
        "local_pdf_replace_requires_owned_registration"
    );
    let names = fs::read_dir(target)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    ensure!(
        names
            == ["catalog.sqlite3".into(), "registration.json".into()]
                .into_iter()
                .collect(),
        "local_pdf_replace_requires_owned_registration"
    );
    let raw = oe_core::read_regular_bytes(&target.join("registration.json"), 8 * 1024 * 1024)?;
    let registration = oe_core::json::strict_json(&raw)?;
    keys(&registration, &["schema", "sources", "index_sha256"])?;
    ensure!(
        registration["schema"] == "ontology-engineering.local-pdf-registration/v1",
        "local_pdf_replace_requires_owned_registration"
    );
    let (hash, bytes) = file_hash(&target.join("catalog.sqlite3"), 2 * 1024 * 1024 * 1024)?;
    ensure!(
        registration["index_sha256"] == hash,
        "local_pdf_existing_index_identity_mismatch"
    );
    Ok(json!({"registration_sha256":oe_core::sha256(raw),"index_sha256":hash,"index_bytes":bytes}))
}

pub fn register(root: &Path, manifest: &Path, options: &RegistrationOptions) -> Result<Value> {
    register_with_guard(root, manifest, options, || Ok(()))
}

/// The owning intake layer may add a final source-document check. A preview is
/// informational: apply always rereads, hashes and validates all current inputs.
pub(crate) fn register_with_guard(
    root: &Path,
    manifest: &Path,
    options: &RegistrationOptions,
    guard: impl FnOnce() -> Result<()>,
) -> Result<Value> {
    ensure!(
        !(options.replace && options.merge),
        "local_pdf_replace_merge_conflict"
    );
    let root = oe_core::SkillRoot::new(root)?;
    let target = within_root(root.path(), &options.data_root)?;
    let indexes = root.path().join("sources/.indexes");
    ensure!(
        target.starts_with(&indexes) && target != indexes,
        "local_pdf_index_outside_sources_indexes"
    );
    let previous = incremental::Previous::open(&root, &target, options)?;
    let manifest = within_root(root.path(), manifest)?;
    let manifest_raw = oe_core::read_regular_bytes(&manifest, 8 * 1024 * 1024)?;
    let incoming = manifest_sources(root.path(), &manifest, &manifest_raw)?;
    let mut plan = incremental::Plan::new(incoming, previous.as_ref(), options)?;
    let mut originals = Vec::new();
    for source in &plan.sources {
        let path = PathBuf::from(text(source, "path")?);
        let actual = file_hash(&path, 4 * 1024 * 1024 * 1024)?;
        ensure!(
            actual.0 == text(source, "sha256")?,
            "local_pdf_source_hash_mismatch"
        );
        if let Some(old) = plan.reused(source, previous.as_ref()) {
            ensure!(
                old["bytes"] == actual.1,
                "local_pdf_existing_source_bytes_mismatch"
            );
        }
        originals.push((path, actual));
    }
    let recheck_inputs = || -> Result<()> {
        for (path, expected) in &originals {
            ensure!(
                file_hash(path, 4 * 1024 * 1024 * 1024)? == *expected,
                "local_pdf_source_changed_during_registration"
            );
        }
        ensure!(
            oe_core::read_regular_bytes(&manifest, 8 * 1024 * 1024)? == manifest_raw,
            "local_pdf_manifest_changed"
        );
        Ok(())
    };
    if options.dry_run || plan.is_noop(options) {
        // Validate reuse eligibility and bounded page rows even without a writer.
        let mut measured = incremental::Counts::new(plan.sources.len());
        for source in &plan.sources {
            if let Some(old) = plan.reused(source, previous.as_ref()) {
                let previous = previous.as_ref().expect("reuse has previous");
                let mut registered = source.clone();
                registered["bytes"] = old["bytes"].clone();
                registered["page_count"] = old["page_count"].clone();
                incremental::reuse(
                    &previous.read,
                    old,
                    &registered,
                    options,
                    &mut measured,
                    |_, _| Ok(()),
                )?;
            }
        }
        if plan.is_noop(options) {
            ensure!(
                measured.value == previous.as_ref().expect("noop has previous").read.meta["counts"],
                "local_pdf_existing_counts_mismatch"
            );
        }
        guard()?;
        recheck_inputs()?;
        if let Some(previous) = &previous {
            previous.verify(&target)?;
        }
        let status = if options.dry_run {
            "preview"
        } else {
            "unchanged"
        };
        let reuse_candidate_ids = if options.dry_run {
            std::mem::replace(&mut plan.update["reused_source_ids"], json!([]))
        } else {
            json!([])
        };
        return Ok(
            json!({"status":status,"provider":"local_pdf","data_root":target,
            "update":plan.update,"final_counts":plan.final_counts,"index":if status == "unchanged" {previous.as_ref().map(|p| &p.read.meta)} else {None},
            "previous":previous.as_ref().map(|p| &p.snapshot),
            "reuse_candidate_ids":reuse_candidate_ids,
            "published":false,"originals_modified":false,"network_requests":0}),
        );
    }
    let stage = Stage::new(root.path(), "registration")?;
    let built = stage.path.join("index");
    oe_core::create_new_private_dir(&built)?;
    let index = built.join("catalog.sqlite3");
    drop(oe_core::create_new_regular_file(&index)?);
    let db = Connection::open(&index)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA trusted_schema=OFF;")?;
    db.execute_batch(TABLES)?;
    db.execute_batch("BEGIN IMMEDIATE")?;
    let mut counts = incremental::Counts::new(plan.sources.len());
    let mut tools = Vec::new();
    let mut sources = Vec::new();
    let mut reused = Vec::new();
    let mut extracted_ids = Vec::new();
    for (source, (original, identity)) in plan.sources.iter().zip(&originals) {
        let mut registered = source.clone();
        registered["bytes"] = json!(identity.1);
        if let Some(old) = plan.reused(source, previous.as_ref()) {
            let previous = previous.as_ref().expect("reuse has previous");
            registered["page_count"] = old["page_count"].clone();
            incremental::insert_chapters(&db, &registered)?;
            let observation = incremental::reuse(
                &previous.read,
                old,
                &registered,
                options,
                &mut counts,
                |record, search| incremental::insert_page(&db, &record, &search),
            )?;
            tools.push(observation);
            reused.push(
                json!({"source_id":source["id"],"previous_index":previous.read.identity(),
                "previous_registration_sha256":previous.snapshot["registration_sha256"],
                "previous_index_fingerprint":previous.read.meta["fingerprint"],
                "navigation":"rebuilt_from_current_declaration",
                "previous_chapters_sha256":oe_core::json::canonical_digest(&old["chapters"])?,
                "current_chapters_sha256":oe_core::json::canonical_digest(&registered["chapters"])?,
                "extraction":"reused_verified_native_text; no Poppler invocation"}),
            );
        } else {
            let extracted = poppler::extract(
                root.path(),
                original,
                &identity.0,
                &options.tools,
                &options.limits,
            )?;
            registered["page_count"] = json!(extracted.pages.len());
            incremental::insert_chapters(&db, &registered)?;
            counts.add_source(&registered, extracted.pages.iter().map(String::len).sum())?;
            for (offset, raw) in extracted.pages.iter().enumerate() {
                let body = normalize_body(raw);
                let (record, search) = incremental::page_record(
                    &registered,
                    (offset + 1) as u64,
                    &body,
                    &oe_core::sha256(raw),
                    raw.chars().count() as u64,
                )?;
                counts.add_page(&record)?;
                incremental::insert_page(&db, &record, &search)?;
            }
            tools.push(json!({"source_id":source["id"],"tools":extracted.observations}));
            extracted_ids.push(source["id"].clone());
        }
        registered["path"] = json!(relative_original(original, &target)?);
        sources.push(registered);
    }
    plan.update["extracted_source_ids"] = json!(extracted_ids);
    plan.final_counts = counts.value.clone();
    let identity =
        json!({"schema":"ontology-engineering.local-pdf-registration/v1","sources":sources});
    let builder = [
        include_bytes!("registration.rs").as_slice(),
        include_bytes!("registration/incremental.rs").as_slice(),
    ]
    .concat();
    let mut meta = json!({"schema":"ontology-engineering.local-pdf-index/v1","built_at":chrono::DateTime::<chrono::Utc>::from(SystemTime::now()).to_rfc3339_opts(chrono::SecondsFormat::Secs,true),"counts":counts.value,"registration_sha256":oe_core::json::canonical_digest(&identity)?,"builder_sha256":oe_core::sha256(builder),"builder":"oe-knowledge Rust registration","extraction":"pdftotext -layout -enc UTF-8; horizontal whitespace collapsed; no OCR","scope":"native text retrieval plus declared chapter navigation; no claim extraction or semantic adoption","tools":tools,"reused_sources":reused,"sqlite_version":rusqlite::version(),"tokenizer":"Rust Unicode NFKC + Unicode lowercase; ASCII/CJK ngrams"});
    meta["fingerprint"] = json!(oe_core::json::canonical_digest(&meta)?);
    ensure!(
        encoded(&meta)?.len() <= 8 * 1024 * 1024,
        "local_pdf_metadata_limit"
    );
    for (key, value) in meta.as_object().expect("object") {
        db.execute(
            "INSERT INTO meta VALUES (?,?)",
            params![key, encoded(value)?],
        )?;
    }
    db.execute("INSERT INTO search(search) VALUES ('optimize')", [])?;
    db.execute_batch("COMMIT")?;
    ensure!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? == "ok",
        "local_pdf_index_integrity_failure"
    );
    db.close().map_err(|(_, e)| e)?;
    let (index_hash, _) = file_hash(&index, 2 * 1024 * 1024 * 1024)?;
    let mut registration = identity;
    registration["index_sha256"] = json!(index_hash);
    ensure!(
        serde_json::to_vec_pretty(&registration)?.len() < 8 * 1024 * 1024,
        "local_pdf_registration_limit"
    );
    oe_core::write_new(&built.join("registration.json"), &registration)?;
    if let Some(previous) = &previous {
        previous.verify(&target)?;
    }
    crate::publication::publish_directory(
        stage,
        &built,
        &target,
        previous.as_ref().map(|p| &p.snapshot),
        owned_snapshot,
        || {
            guard()?;
            recheck_inputs()
        },
    )?;
    Ok(
        json!({"status":"registered","provider":"local_pdf","data_root":target,
        "index":meta,"update":plan.update,"final_counts":plan.final_counts,"previous":previous.as_ref().map(|p| &p.snapshot),
        "published":true,"originals_modified":false,"network_requests":0}),
    )
}

/// Reuse the original registration transform without changing its source body.
pub fn normalized_native_body(raw: &str) -> String {
    normalize_body(raw)
}
