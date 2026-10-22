//! Rebuild supplier retrieval indexes from original bytes. Poppler text is a new
//! extraction baseline, never represented as PyMuPDF text or engineering proof.
use crate::{
    poppler::{self, PopplerLimits, PopplerTools},
    source_pack::{self, file_hash, within_root},
};
use anyhow::{Context, Result, ensure};
use oe_core::json::{canonical_digest, canonical_json, strict_json};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const CATALOG_CODES: [&str; 2] = ["fabiaozhunpin202210", "fajingjixing202306"];
const LABELS: [&str; 2] = ["FA标准品（2022）", "FA经济型（2023）"];
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_PAGES: usize = 20000;
const MAX_TEXT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum BuildInput {
    Archive(PathBuf),
    SourcePack(PathBuf),
}
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub data_root: PathBuf,
    pub replace: bool,
    pub tools: PopplerTools,
    pub limits: PopplerLimits,
}
impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            data_root: "sources/.indexes/misumi".into(),
            replace: false,
            tools: Default::default(),
            limits: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MetadataFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Clone)]
pub(crate) struct ChapterMetadata {
    pub id: String,
    pub catalog: String,
    pub title: String,
    pub start_page: u64,
    pub end_page: u64,
    pub raw: Value,
}
#[derive(Debug, Clone)]
pub(crate) struct PageMetadata {
    pub id: String,
    pub page_id: i64,
    pub number: u64,
    pub printed_page: String,
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
    pub url: String,
    pub chapter_id: Option<String>,
    pub chapter_title: String,
    pub headings: Vec<Value>,
    pub series: Vec<Value>,
    pub hotspots: Vec<Value>,
    pub raw: Value,
}
#[derive(Debug, Clone)]
pub(crate) struct CatalogMetadata {
    pub code: String,
    pub label: String,
    pub title: String,
    pub page_count: u64,
    pub pdf_relative_path: String,
    pub pdf_sha256: String,
    pub metadata_sha256: String,
    pub manifest_sha256: String,
    pub chapters: Vec<ChapterMetadata>,
    pub pages: Vec<PageMetadata>,
}
#[derive(Debug, Clone)]
pub(crate) struct ArchiveMetadata {
    pub catalogs: Vec<CatalogMetadata>,
    pub files: Vec<MetadataFile>,
}
impl ArchiveMetadata {
    pub(crate) fn source_identity(&self) -> Value {
        json!(
            self.catalogs
                .iter()
                .map(|c| json!([c.code, c.metadata_sha256, c.manifest_sha256, c.pdf_sha256]))
                .collect::<Vec<_>>()
        )
    }
}

fn encoded(value: &Value) -> Result<String> {
    Ok(String::from_utf8(canonical_json(value)?)?)
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|x| !x.trim().is_empty() && x.len() <= MAX_METADATA_BYTES)
        .context("supplier_metadata_text_invalid")
}
fn scalar(value: &Value) -> Result<String> {
    Ok(match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        _ => anyhow::bail!("supplier_metadata_scalar_invalid"),
    })
}
fn positive(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)
        .context("supplier_metadata_positive_integer_required")
}
fn optional_text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    match value.get(field) {
        None => Ok(""),
        Some(v) => v.as_str().context("supplier_hotspot_text_invalid"),
    }
}
fn member(value: &str) -> Result<String> {
    source_pack::relative_path(value)?;
    Ok(value.into())
}
fn sha(value: &Value) -> Result<String> {
    let s = value.as_str().context("supplier_hash_invalid")?;
    ensure!(source_pack::valid_sha(s), "supplier_hash_invalid");
    Ok(s.into())
}

pub(crate) fn required_metadata_paths() -> Vec<String> {
    CATALOG_CODES
        .iter()
        .flat_map(|c| {
            [
                format!("tmp/pdfs/{c}/metadata.json"),
                format!("下载记录/{c}-manifest.json"),
                format!("下载记录/{c}-assembly.json"),
            ]
        })
        .collect()
}
pub(crate) fn parse_archive_metadata(root: &Path, archive: &Path) -> Result<ArchiveMetadata> {
    let archive = within_root(root, archive)?;
    ensure!(
        archive.starts_with(root.join("sources")),
        "supplier_originals_must_be_in_sources"
    );
    parse_metadata_directory(root, &archive)
}

/// Shared metadata decoding for originals and an installer's private staging
/// directory. This does not authorize using staging as a builder source.
pub(crate) fn parse_metadata_directory(root: &Path, archive: &Path) -> Result<ArchiveMetadata> {
    let archive = within_root(root, archive)?;
    ensure!(
        archive.starts_with(root.join("sources")) || archive.starts_with(root.join("var")),
        "supplier_metadata_directory_outside_sources_or_private_stage"
    );
    let mut files = BTreeMap::new();
    for relative in required_metadata_paths() {
        let path = within_root(root, &archive.join(&relative))?;
        files.insert(
            relative,
            oe_core::read_regular_bytes(&path, MAX_METADATA_BYTES)?,
        );
    }
    for code in CATALOG_CODES {
        let assembly = strict_json(&files[&format!("下载记录/{code}-assembly.json")])?;
        if let Some(qc) = assembly.get("qc_report").filter(|v| !v.is_null()) {
            let relative = member(qc.as_str().context("supplier_qc_path_invalid")?)?;
            ensure!(
                !files.contains_key(&relative),
                "supplier_duplicate_metadata_path"
            );
            files.insert(
                relative.clone(),
                oe_core::read_regular_bytes(
                    &within_root(root, &archive.join(relative))?,
                    MAX_METADATA_BYTES,
                )?,
            );
        }
    }
    parse_metadata_bytes(&files)
}

/// The shared parser requires the six core metadata documents and referenced QC.
/// Compressed callers first verify their enclosing archive/member identities.
pub(crate) fn parse_metadata_bytes(input: &BTreeMap<String, Vec<u8>>) -> Result<ArchiveMetadata> {
    let mut files = Vec::new();
    let mut documents = BTreeMap::new();
    for path in required_metadata_paths() {
        let bytes = input
            .get(&path)
            .context("supplier_required_metadata_missing")?;
        ensure!(
            bytes.len() <= MAX_METADATA_BYTES,
            "supplier_metadata_byte_limit"
        );
        files.push(MetadataFile {
            path: path.clone(),
            sha256: oe_core::sha256(bytes),
            bytes: bytes.len() as u64,
        });
        documents.insert(path, strict_json(bytes)?);
    }
    for code in CATALOG_CODES {
        if let Some(qc) = documents[&format!("下载记录/{code}-assembly.json")]
            .get("qc_report")
            .filter(|v| !v.is_null())
        {
            let path = member(qc.as_str().context("supplier_qc_path_invalid")?)?;
            ensure!(
                !files.iter().any(|f| f.path == path),
                "supplier_duplicate_metadata_path"
            );
            let bytes = input.get(&path).context("supplier_required_qc_missing")?;
            ensure!(
                bytes.len() <= MAX_METADATA_BYTES,
                "supplier_metadata_byte_limit"
            );
            strict_json(bytes)?;
            files.push(MetadataFile {
                path,
                sha256: oe_core::sha256(bytes),
                bytes: bytes.len() as u64,
            });
        }
    }
    let mut catalogs = Vec::new();
    for (catalog_index, code) in CATALOG_CODES.iter().enumerate() {
        let metadata_path = format!("tmp/pdfs/{code}/metadata.json");
        let manifest_path = format!("下载记录/{code}-manifest.json");
        let ebook = &documents[&metadata_path]["ebook"];
        ensure!(
            ebook["catalogCode"] == *code,
            "supplier_catalog_identity_mismatch"
        );
        let pages = ebook["pages"]["info"]
            .as_array()
            .context("supplier_pages_invalid")?;
        let manifests = documents[&manifest_path]
            .as_array()
            .context("supplier_page_manifest_invalid")?;
        ensure!(
            !pages.is_empty() && pages.len() <= MAX_PAGES && pages.len() == manifests.len(),
            "supplier_source_manifest_identity_mismatch"
        );
        let mut page_by_id = BTreeMap::new();
        let mut numbers = BTreeSet::new();
        for page in pages {
            let id = positive(&page["id"])?;
            let n = positive(&page["pageNo"])?;
            ensure!(
                n <= pages.len() as u64
                    && numbers.insert(n)
                    && page_by_id.insert(id.to_string(), page).is_none(),
                "supplier_duplicate_or_invalid_page"
            );
        }
        let mut manifest_by_number = BTreeMap::new();
        for entry in manifests {
            let n = positive(&entry["page_number"])?;
            ensure!(
                numbers.contains(&n) && manifest_by_number.insert(n, entry).is_none(),
                "supplier_duplicate_or_invalid_manifest_page"
            );
        }
        let assembly = &documents[&format!("下载记录/{code}-assembly.json")];
        ensure!(
            assembly["book"] == *code && assembly["pages"].as_u64() == Some(pages.len() as u64),
            "supplier_assembly_identity_mismatch"
        );
        let pdf_relative_path = member(text(assembly, "file")?)?;
        let pdf_sha256 = sha(&assembly["sha256"])?;
        let mut class = ebook["class"]
            .as_array()
            .context("supplier_chapters_invalid")?
            .iter()
            .collect::<Vec<_>>();
        ensure!(class.len() <= 4096, "supplier_chapter_count_limit");
        for c in &class {
            ensure!(
                page_by_id.contains_key(&scalar(&c["pageId"])?),
                "supplier_chapter_page_missing"
            );
        }
        class.sort_by_key(|c| {
            page_by_id[&scalar(&c["pageId"]).expect("checked scalar")]["pageNo"]
                .as_u64()
                .expect("checked page")
        });
        let mut ids = BTreeSet::new();
        let mut starts = BTreeSet::new();
        let mut chapters = Vec::new();
        for (i, c) in class.iter().enumerate() {
            let source_id = scalar(c.get("id").context("supplier_chapter_id_missing")?)?;
            ensure!(
                !source_id.is_empty() && source_id.len() <= 2000 && ids.insert(source_id.clone()),
                "supplier_duplicate_chapter"
            );
            let start = positive(&page_by_id[&scalar(&c["pageId"])?]["pageNo"])?;
            ensure!(starts.insert(start), "supplier_overlapping_chapter_start");
            let end = if i + 1 < class.len() {
                positive(&page_by_id[&scalar(&class[i + 1]["pageId"])?]["pageNo"])? - 1
            } else {
                pages.len() as u64
            };
            ensure!(end >= start, "supplier_chapter_range_invalid");
            chapters.push(ChapterMetadata {
                id: format!("{code}:chapter:{source_id}"),
                catalog: (*code).into(),
                title: text(c, "title")?.into(),
                start_page: start,
                end_page: end,
                raw: (*c).clone(),
            });
        }
        let mut incoming: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for page in pages {
            if let Some(areas) = page.get("hotArea") {
                for h in areas.as_array().context("supplier_hotspots_invalid")? {
                    let target = match h.get("target") {
                        Some(v) => scalar(v)?,
                        None => String::new(),
                    };
                    let keyword = optional_text(h, "keyword")?;
                    if page_by_id.contains_key(&target)
                        && !keyword.is_empty()
                        && optional_text(h, "type")?.contains("目次")
                    {
                        let hotspot_id = h.get("id").context("supplier_hotspot_id_missing")?;
                        incoming.entry(target).or_default().push(json!({"text":keyword,"source_page_id":page["id"],"hotspot_id":hotspot_id}));
                    }
                }
            }
        }
        let mut page_rows = Vec::new();
        let mut source_paths = BTreeSet::new();
        for page in pages {
            let number = positive(&page["pageNo"])?;
            let id = positive(&page["id"])? as i64;
            let entry = manifest_by_number[&number];
            let filename = member(text(page, "pdfUrl")?)?;
            let printed_page = scalar(
                page.get("pageNoReal")
                    .context("supplier_printed_page_missing")?,
            )?;
            ensure!(
                entry["page_id"] == page["id"]
                    && entry["filename"] == filename
                    && scalar(
                        entry
                            .get("printed_page")
                            .context("supplier_printed_page_missing")?
                    )? == printed_page,
                "supplier_page_mapping_mismatch"
            );
            let relative_path = format!("tmp/pdfs/{code}/pages/{filename}");
            ensure!(
                source_paths.insert(relative_path.clone()),
                "supplier_duplicate_source_path"
            );
            let bytes = positive(&entry["bytes"])?;
            ensure!(
                bytes <= source_pack::MAX_SOURCE_BYTES,
                "supplier_page_byte_limit"
            );
            let hotspots = match page.get("hotArea") {
                Some(v) => v.as_array().context("supplier_hotspots_invalid")?.clone(),
                None => Vec::new(),
            };
            ensure!(hotspots.len() <= 20000, "supplier_page_hotspot_limit");
            let mut series = Vec::new();
            for h in &hotspots {
                ensure!(h.is_object(), "supplier_hotspot_invalid");
                let kind = optional_text(h, "type")?;
                let keyword = optional_text(h, "keyword")?;
                if let Some(target) = h.get("target") {
                    scalar(target)?;
                }
                if matches!(kind, "形式Type" | "形式Tpye") {
                    series.push(json!({"keyword":keyword,"url":h.get("target").cloned().unwrap_or(json!("")),"hotspot_id":h.get("id").context("supplier_hotspot_id_missing")?}));
                }
            }
            let chapter = chapters
                .iter()
                .find(|c| c.start_page <= number && number <= c.end_page);
            let mut raw = page.clone();
            raw.as_object_mut()
                .context("supplier_page_object_invalid")?
                .remove("hotArea");
            page_rows.push(PageMetadata {
                id: format!("{code}:page:{id}"),
                page_id: id,
                number,
                printed_page,
                relative_path,
                sha256: sha(&entry["sha256"])?,
                bytes,
                url: text(entry, "url")?.into(),
                chapter_id: chapter.map(|c| c.id.clone()),
                chapter_title: chapter
                    .map(|c| c.title.clone())
                    .unwrap_or_else(|| "封面及前附".into()),
                headings: incoming.get(&id.to_string()).cloned().unwrap_or_default(),
                series,
                hotspots,
                raw,
            });
        }
        // SQL row insertion follows the declared metadata order, matching the source.
        catalogs.push(CatalogMetadata {
            code: (*code).into(),
            label: LABELS[catalog_index].into(),
            title: text(ebook, "catalogTitle")?.into(),
            page_count: pages.len() as u64,
            pdf_relative_path,
            pdf_sha256,
            metadata_sha256: oe_core::sha256(&input[&metadata_path]),
            manifest_sha256: oe_core::sha256(&input[&manifest_path]),
            chapters,
            pages: page_rows,
        });
    }
    ensure!(
        catalogs.iter().map(|c| c.pages.len()).sum::<usize>() <= MAX_PAGES,
        "supplier_total_page_limit"
    );
    Ok(ArchiveMetadata { catalogs, files })
}

fn normalized_body(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut horizontal = false;
    for ch in raw.chars() {
        if matches!(ch, ' ' | '\t') {
            if !horizontal {
                result.push(' ')
            }
            horizontal = true;
        } else {
            result.push(ch);
            horizontal = false;
        }
    }
    result
}
struct IndexBuilder {
    db: Connection,
    counts: Value,
    tools: Option<Value>,
    seen: BTreeSet<String>,
    text_bytes: usize,
}
impl IndexBuilder {
    fn new(path: &Path, metadata: &ArchiveMetadata) -> Result<Self> {
        drop(oe_core::create_new_regular_file(path)?);
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA trusted_schema=OFF;")?;
        db.execute_batch(crate::registration::TABLES)?;
        db.execute_batch("CREATE TABLE catalogs(code TEXT PRIMARY KEY,label TEXT,title TEXT,page_count INTEGER,pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT);
CREATE TABLE hotspots(page_id TEXT,ordinal INTEGER,kind TEXT,keyword TEXT,keyword_normalized TEXT,target TEXT,source_json TEXT,PRIMARY KEY(page_id,ordinal));
CREATE INDEX hotspot_model ON hotspots(keyword_normalized,kind);BEGIN IMMEDIATE;")?;
        for c in &metadata.catalogs {
            db.execute(
                "INSERT INTO catalogs VALUES (?,?,?,?,?,?,?,?)",
                params![
                    c.code,
                    c.label,
                    c.title,
                    c.page_count,
                    c.pdf_relative_path,
                    c.pdf_sha256,
                    c.metadata_sha256,
                    c.manifest_sha256
                ],
            )?;
            for chapter in &c.chapters {
                db.execute(
                    "INSERT INTO chapters VALUES (?,?,?,?,?,?)",
                    params![
                        chapter.id,
                        chapter.catalog,
                        chapter.title,
                        chapter.start_page,
                        chapter.end_page,
                        encoded(&chapter.raw)?
                    ],
                )?;
            }
        }
        let counts = json!({"catalogs":metadata.catalogs.len(),"chapters":metadata.catalogs.iter().map(|c|c.chapters.len()).sum::<usize>(),"pages":0,"hotspots":0,"series_hotspots":0,"native_text_sparse":0,"native_text_missing":0,"native_text_chars":0});
        Ok(Self {
            db,
            counts,
            tools: None,
            seen: BTreeSet::new(),
            text_bytes: 0,
        })
    }
    fn insert_page(
        &mut self,
        root: &Path,
        catalog: &str,
        page: &PageMetadata,
        path: &Path,
        options: &BuildOptions,
    ) -> Result<()> {
        ensure!(
            self.seen.insert(page.id.clone()),
            "supplier_duplicate_extracted_page"
        );
        let original = file_hash(path, source_pack::MAX_SOURCE_BYTES)?;
        ensure!(
            original == (page.sha256.clone(), page.bytes),
            "supplier_source_page_hash_mismatch"
        );
        let mut limits = options.limits.clone();
        limits.pages = 1;
        let extracted = poppler::extract(root, path, &page.sha256, &options.tools, &limits)?;
        ensure!(
            extracted.pages.len() == 1,
            "supplier_source_pdf_requires_single_physical_page"
        );
        let tool_identity = json!({"pdfinfo":extracted.observations["pdfinfo"],"pdftotext":extracted.observations["pdftotext"]});
        if let Some(expected) = &self.tools {
            ensure!(
                *expected == tool_identity,
                "supplier_extractor_changed_during_build"
            );
        } else {
            self.tools = Some(tool_identity);
        }
        let raw = &extracted.pages[0];
        let body = normalized_body(raw);
        self.text_bytes = self
            .text_bytes
            .checked_add(body.len())
            .context("supplier_total_text_limit")?;
        ensure!(
            self.text_bytes <= MAX_TEXT_BYTES,
            "supplier_total_text_limit"
        );
        for (ordinal, h) in page.hotspots.iter().enumerate() {
            let kind = optional_text(h, "type")?;
            let keyword = optional_text(h, "keyword")?;
            let target = match h.get("target") {
                Some(v) => scalar(v)?,
                None => String::new(),
            };
            self.db.execute(
                "INSERT INTO hotspots VALUES (?,?,?,?,?,?,?)",
                params![
                    page.id,
                    ordinal as u64,
                    kind,
                    keyword,
                    crate::text::normalize(keyword),
                    target,
                    encoded(h)?
                ],
            )?;
        }
        let sparse = body.chars().filter(|c| !c.is_whitespace()).count() < 80;
        self.db.execute(
            "INSERT INTO pages VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                page.id,
                catalog,
                page.page_id,
                page.number,
                page.printed_page,
                page.chapter_id,
                page.relative_path,
                page.url,
                page.sha256,
                page.bytes,
                body,
                oe_core::sha256(&body),
                oe_core::sha256(raw),
                raw.chars().count() as u64,
                body.chars().count() as u64,
                sparse,
                encoded(&json!(page.headings))?,
                encoded(&json!(page.series))?,
                page.series.len() as u64,
                encoded(&page.raw)?
            ],
        )?;
        let models = page
            .series
            .iter()
            .map(|s| s["keyword"].as_str().expect("validated text"))
            .collect::<Vec<_>>()
            .join(" ");
        let titles = page
            .headings
            .iter()
            .map(|h| h["text"].as_str().expect("validated text"))
            .collect::<Vec<_>>()
            .join(" ");
        self.db.execute(
            "INSERT INTO search(rowid,models,titles,body,chapter) VALUES (?,?,?,?,?)",
            params![
                self.db.last_insert_rowid(),
                crate::text::tokens(&models).join(" "),
                crate::text::tokens(&titles).join(" "),
                crate::text::tokens(&body).join(" "),
                crate::text::tokens(&page.chapter_title).join(" ")
            ],
        )?;
        for (key, add) in [
            ("pages", 1),
            ("hotspots", page.hotspots.len() as u64),
            ("series_hotspots", page.series.len() as u64),
            ("native_text_sparse", u64::from(sparse)),
            ("native_text_missing", u64::from(body.trim().is_empty())),
            ("native_text_chars", body.chars().count() as u64),
        ] {
            self.counts[key] = json!(
                self.counts[key]
                    .as_u64()
                    .expect("initialized count")
                    .checked_add(add)
                    .context("supplier_count_overflow")?
            );
        }
        ensure!(
            file_hash(path, source_pack::MAX_SOURCE_BYTES)? == original,
            "supplier_page_changed_during_extraction"
        );
        Ok(())
    }
    fn finish(
        self,
        metadata: &ArchiveMetadata,
        source_binding: Value,
        assembly_observations: Value,
        options: &BuildOptions,
    ) -> Result<Value> {
        ensure!(
            self.seen.len()
                == metadata
                    .catalogs
                    .iter()
                    .map(|c| c.pages.len())
                    .sum::<usize>(),
            "supplier_extracted_page_inventory_mismatch"
        );
        let mut meta = json!({"schema":"misumi.catalog-index/v1","version":env!("CARGO_PKG_VERSION"),"built_at":chrono::DateTime::<chrono::Utc>::from(SystemTime::now()).to_rfc3339_opts(chrono::SecondsFormat::Secs,true),"source_identity":metadata.source_identity(),"counts":self.counts,
            "extraction":"Poppler pdftotext -layout -enc UTF-8; form-feed physical-page separator removed; horizontal whitespace collapsed; no OCR",
            "text_origin":"poppler_native_text","text_compatibility":"new extraction baseline; not PyMuPDF sort=True text or hash equivalence","repair_detection":"Poppler page-count and encryption checks; no PyMuPDF is_repaired equivalence claimed",
            "tools":self.tools.context("supplier_extractor_observation_missing")?,"limits":{"timeout_seconds_per_native_process":options.limits.timeout.as_secs_f64(),"native_stdout_bytes":options.limits.output_bytes,"page_text_bytes":options.limits.page_bytes,"total_text_bytes":MAX_TEXT_BYTES,"max_source_pages":MAX_PAGES},
            "source_binding":source_binding,"assembled_pdf_verification":assembly_observations,"sqlite_version":rusqlite::version(),"builder":"oe-knowledge Rust supplier index","builder_sha256":oe_core::sha256(include_bytes!("supplier_index.rs")),"tokenizer":"Rust Unicode NFKC + Unicode lowercase; ASCII/CJK ngrams","scope":"document_retrieval; series hotspots are not configured products; no parameter verification"});
        meta["fingerprint"] = json!(canonical_digest(&meta)?);
        ensure!(
            encoded(&meta)?.len() <= crate::index::MAX_VALUE_BYTES,
            "supplier_index_metadata_limit"
        );
        for (key, value) in meta.as_object().expect("metadata object") {
            self.db.execute(
                "INSERT INTO meta VALUES (?,?)",
                params![key, encoded(value)?],
            )?;
        }
        self.db
            .execute("INSERT INTO search(search) VALUES ('optimize')", [])?;
        self.db.execute_batch("COMMIT")?;
        ensure!(
            self.db
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))?
                == "ok",
            "supplier_index_integrity_failure"
        );
        self.db.close().map_err(|(_, e)| e)?;
        Ok(meta)
    }
}

#[derive(Clone)]
struct Observed {
    path: PathBuf,
    sha256: String,
    bytes: u64,
    limit: u64,
}
impl Observed {
    fn check(&self) -> Result<()> {
        ensure!(
            file_hash(&self.path, self.limit)? == (self.sha256.clone(), self.bytes),
            "supplier_source_changed_during_build"
        );
        Ok(())
    }
}
fn check_all(files: &[Observed]) -> Result<()> {
    for file in files {
        file.check()?;
    }
    Ok(())
}
fn relative_to(path: &Path, directory: &Path) -> Result<String> {
    let a = path.components().collect::<Vec<_>>();
    let b = directory.components().collect::<Vec<_>>();
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for part in &a[common..] {
        out.push(part.as_os_str());
    }
    Ok(out
        .to_str()
        .context("supplier_path_utf8")?
        .replace('\\', "/"))
}
fn owned_snapshot(path: &Path) -> Result<Value> {
    oe_core::no_symlinks(path)?;
    ensure!(path.is_dir(), "supplier_replace_requires_owned_snapshot");
    let names = std::fs::read_dir(path)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    ensure!(
        names
            == ["catalog.sqlite3".into(), "source.json".into()]
                .into_iter()
                .collect(),
        "supplier_replace_requires_owned_snapshot"
    );
    let raw =
        oe_core::read_regular_bytes(&path.join("source.json"), crate::index::MAX_VALUE_BYTES)?;
    let location = strict_json(&raw)?;
    ensure!(
        location["schema"] == "ontology-engineering.misumi-source-location/v1"
            && location["index_path"] == "catalog.sqlite3",
        "supplier_replace_requires_owned_snapshot"
    );
    let identity = file_hash(&path.join("catalog.sqlite3"), crate::index::MAX_INDEX_BYTES)?;
    ensure!(
        location["index_sha256"] == identity.0,
        "supplier_existing_index_identity_mismatch"
    );
    Ok(
        json!({"registration_sha256":oe_core::sha256(raw),"index_sha256":identity.0,"index_bytes":identity.1}),
    )
}
fn archive_observations(
    root: &Path,
    archive: &Path,
    metadata: &ArchiveMetadata,
    options: &BuildOptions,
    required: bool,
    observed: &mut Vec<Observed>,
) -> Result<Value> {
    let mut result = Vec::new();
    for catalog in &metadata.catalogs {
        let path = within_root(root, &archive.join(&catalog.pdf_relative_path))?;
        if path.try_exists()? {
            let identity = file_hash(&path, crate::citations::MAX_SOURCE_BYTES)?;
            ensure!(
                identity.0 == catalog.pdf_sha256,
                "supplier_assembled_pdf_hash_mismatch"
            );
            let info = poppler::inspect(root, &path, &options.tools, &options.limits)?;
            ensure!(
                info.page_count == catalog.page_count,
                "supplier_assembled_physical_page_count_mismatch"
            );
            observed.push(Observed {
                path,
                sha256: identity.0,
                bytes: identity.1,
                limit: crate::citations::MAX_SOURCE_BYTES,
            });
            result.push(json!({"catalog":catalog.code,"status":"sha256_and_physical_page_count_verified","sha256":catalog.pdf_sha256,"page_count":info.page_count,"pdfinfo":info.tool,"pdfinfo_output_sha256":info.output_sha256}));
        } else {
            ensure!(!required, "supplier_assembled_pdf_missing");
            result.push(json!({"catalog":catalog.code,"status":"not_installed_not_verified","declared_sha256":catalog.pdf_sha256,"declared_page_count":catalog.page_count}));
        }
    }
    Ok(json!(result))
}

/// Build a fresh compatible retrieval database. Replacement requires an explicit
/// flag and an owned two-file source.json snapshot; originals are never changed.
pub fn build(root: &Path, input: &BuildInput, options: &BuildOptions) -> Result<Value> {
    let skill = oe_core::SkillRoot::new(root)?;
    let root = skill.path();
    let target = within_root(root, &options.data_root)?;
    let indexes = root.join("sources/.indexes");
    ensure!(
        target.starts_with(&indexes) && target != indexes,
        "supplier_index_outside_sources_indexes"
    );
    let previous = if target.try_exists()? {
        ensure!(options.replace, "supplier_index_destination_exists");
        let snapshot = owned_snapshot(&target)?;
        let existing = crate::index::ReadIndex::open(&skill, &target, "misumi")?;
        let location = strict_json(&oe_core::read_regular_bytes(
            &target.join("source.json"),
            crate::index::MAX_VALUE_BYTES,
        )?)?;
        ensure!(
            location["source_catalogs"]
                == json!({"source_identity":existing.meta["source_identity"],"counts":existing.meta["counts"],"index_fingerprint":existing.meta["fingerprint"]}),
            "supplier_existing_registration_mismatch"
        );
        existing.verify()?;
        Some(snapshot)
    } else {
        None
    };
    let stage = source_pack::Stage::new(root, "supplier-index")?;
    let built = stage.path.join("index");
    oe_core::create_new_private_dir(&built)?;
    let index_path = built.join("catalog.sqlite3");
    let mut observed = Vec::new();
    let (metadata, archive_root, pack_manifest, source_binding, assembly, meta) = match input {
        BuildInput::Archive(path) => {
            let archive = within_root(root, path)?;
            ensure!(
                !archive.starts_with(&target) && !target.starts_with(&archive),
                "supplier_output_overlaps_archive"
            );
            let metadata = parse_archive_metadata(root, &archive)?;
            for file in &metadata.files {
                observed.push(Observed {
                    path: archive.join(&file.path),
                    sha256: file.sha256.clone(),
                    bytes: file.bytes,
                    limit: MAX_METADATA_BYTES as u64,
                });
            }
            let assembly =
                archive_observations(root, &archive, &metadata, options, true, &mut observed)?;
            let mut builder = IndexBuilder::new(&index_path, &metadata)?;
            for catalog in &metadata.catalogs {
                for page in &catalog.pages {
                    let path = within_root(root, &archive.join(&page.relative_path))?;
                    builder.insert_page(root, &catalog.code, page, &path, options)?;
                    observed.push(Observed {
                        path,
                        sha256: page.sha256.clone(),
                        bytes: page.bytes,
                        limit: source_pack::MAX_SOURCE_BYTES,
                    });
                }
            }
            let source_binding = json!({"kind":"expanded_archive","archive_root":archive.strip_prefix(root)?.to_str().context("supplier_path_utf8")?});
            let meta =
                builder.finish(&metadata, source_binding.clone(), assembly.clone(), options)?;
            (metadata, archive, None, source_binding, assembly, meta)
        }
        BuildInput::SourcePack(path) => {
            let manifest_path = within_root(root, path)?;
            ensure!(
                manifest_path.starts_with(root.join("sources")),
                "supplier_originals_must_be_in_sources"
            );
            let pack_root = manifest_path
                .parent()
                .context("supplier_pack_parent_missing")?;
            ensure!(
                !manifest_path.starts_with(&target),
                "supplier_output_overlaps_pack"
            );
            let raw = oe_core::read_regular_bytes(&manifest_path, source_pack::MAX_MANIFEST_BYTES)?;
            let manifest_sha = oe_core::sha256(&raw);
            let manifest = source_pack::load_manifest(root, &manifest_path)?;
            ensure!(strict_json(&raw)? == manifest, "supplier_manifest_changed");
            observed.push(Observed {
                path: manifest_path.clone(),
                sha256: manifest_sha.clone(),
                bytes: raw.len() as u64,
                limit: source_pack::MAX_MANIFEST_BYTES as u64,
            });
            let descriptor = &manifest["metadata"];
            let archive_path =
                within_root(root, &pack_root.join(member(text(descriptor, "path")?)?))?;
            let declared = descriptor["files"]
                .as_array()
                .context("supplier_metadata_inventory_missing")?;
            let selected = declared
                .iter()
                .filter_map(|x| {
                    x["path"]
                        .as_str()
                        .filter(|p| p.starts_with("archive/") && p.ends_with(".json"))
                        .map(str::to_owned)
                })
                .collect::<BTreeSet<_>>();
            ensure!(
                selected.len() >= 6 && selected.len() <= 128,
                "supplier_metadata_inventory_invalid"
            );
            let mut bytes = BTreeMap::new();
            let metadata_transport = if archive_path.try_exists()? {
                source_pack::visit_archive(
                    root,
                    &archive_path,
                    descriptor,
                    &source_pack::ArchiveLimits {
                        max_file_bytes: crate::index::MAX_INDEX_BYTES,
                        max_members: 128,
                        max_uncompressed_bytes: crate::index::MAX_INDEX_BYTES + 256 * 1024 * 1024,
                    },
                    Some(&selected),
                    |entry| {
                        let path = entry
                            .relative_path
                            .strip_prefix("archive/")
                            .context("supplier_metadata_member_invalid")?;
                        ensure!(
                            bytes
                                .insert(
                                    path.into(),
                                    oe_core::read_regular_bytes(&entry.path, MAX_METADATA_BYTES)?
                                )
                                .is_none(),
                            "supplier_duplicate_metadata_path"
                        );
                        Ok(())
                    },
                )?;
                observed.push(Observed {
                    path: archive_path,
                    sha256: sha(&descriptor["sha256"])?,
                    bytes: positive(&descriptor["bytes"])?,
                    limit: crate::index::MAX_INDEX_BYTES + 256 * 1024 * 1024,
                });
                "compressed_archive"
            } else {
                // Installed packs can retain verified expanded metadata instead
                // of its transport archive. Verify every declared member,
                // including the old DB without opening it or reusing its text.
                let mut seen = BTreeSet::new();
                for entry in declared {
                    let relative = member(text(entry, "path")?)?;
                    ensure!(
                        seen.insert(relative.clone()),
                        "supplier_duplicate_metadata_path"
                    );
                    let path = within_root(root, &pack_root.join(&relative))?;
                    let limit = if relative == "catalog.sqlite3" {
                        crate::index::MAX_INDEX_BYTES
                    } else {
                        MAX_METADATA_BYTES as u64
                    };
                    let identity = file_hash(&path, limit)?;
                    ensure!(
                        identity.0 == sha(&entry["sha256"])?
                            && identity.1 == positive(&entry["bytes"])?,
                        "supplier_installed_metadata_identity_mismatch"
                    );
                    observed.push(Observed {
                        path: path.clone(),
                        sha256: identity.0,
                        bytes: identity.1,
                        limit,
                    });
                    if selected.contains(&relative) {
                        let key = relative
                            .strip_prefix("archive/")
                            .context("supplier_metadata_member_invalid")?;
                        bytes.insert(
                            key.to_owned(),
                            oe_core::read_regular_bytes(&path, MAX_METADATA_BYTES)?,
                        );
                    }
                }
                "installed_verified_members"
            };
            let metadata = parse_metadata_bytes(&bytes)?;
            let mut expected = metadata
                .files
                .iter()
                .map(|f| (format!("archive/{}", f.path), (f.sha256.clone(), f.bytes)))
                .collect::<BTreeMap<_, _>>();
            expected.insert(
                "catalog.sqlite3".into(),
                (
                    sha(&manifest["index"]["sha256"])?,
                    positive(&manifest["index"]["bytes"])?,
                ),
            );
            ensure!(
                declared.len() == expected.len(),
                "supplier_metadata_closure_mismatch"
            );
            for member in declared {
                ensure!(
                    expected.get(text(member, "path")?)
                        == Some(&(sha(&member["sha256"])?, positive(&member["bytes"])?)),
                    "supplier_metadata_closure_mismatch"
                );
            }
            let catalog_identity = &manifest["source_catalogs"]["source_identity"];
            ensure!(
                *catalog_identity == metadata.source_identity(),
                "supplier_pack_catalog_identity_mismatch"
            );
            let assemblies = manifest["assembled_pdfs"]
                .as_array()
                .context("supplier_assembled_inventory_missing")?;
            ensure!(
                assemblies.len() == metadata.catalogs.len(),
                "supplier_assembled_inventory_mismatch"
            );
            for catalog in &metadata.catalogs {
                let entries = assemblies
                    .iter()
                    .filter(|a| a["catalog"] == catalog.code)
                    .collect::<Vec<_>>();
                ensure!(
                    entries.len() == 1
                        && entries[0]["path"] == catalog.pdf_relative_path
                        && entries[0]["sha256"] == catalog.pdf_sha256,
                    "supplier_assembled_inventory_mismatch"
                );
            }
            let mut page_map = BTreeMap::new();
            for (ci, catalog) in metadata.catalogs.iter().enumerate() {
                for (pi, page) in catalog.pages.iter().enumerate() {
                    ensure!(
                        page_map
                            .insert(page.relative_path.clone(), (ci, pi))
                            .is_none(),
                        "supplier_duplicate_source_path"
                    );
                }
            }
            let segments = manifest["segments"]
                .as_array()
                .context("supplier_segments_missing")?;
            let mut declared_pages = BTreeSet::new();
            for segment in segments {
                for member in segment["files"]
                    .as_array()
                    .context("supplier_segment_members_missing")?
                {
                    let relative = text(member, "path")?;
                    let (ci, pi) = *page_map
                        .get(relative)
                        .context("supplier_unexpected_page_member")?;
                    let page = &metadata.catalogs[ci].pages[pi];
                    ensure!(
                        declared_pages.insert(relative.to_owned())
                            && member["sha256"] == page.sha256
                            && member["bytes"].as_u64() == Some(page.bytes),
                        "supplier_page_member_identity_mismatch"
                    );
                }
            }
            ensure!(
                declared_pages.len() == page_map.len(),
                "supplier_page_member_inventory_mismatch"
            );
            let mut builder = IndexBuilder::new(&index_path, &metadata)?;
            for segment in segments {
                let shard = within_root(root, &pack_root.join(member(text(segment, "path")?)?))?;
                source_pack::visit_archive(
                    root,
                    &shard,
                    segment,
                    &source_pack::ArchiveLimits {
                        max_file_bytes: source_pack::MAX_SOURCE_BYTES,
                        max_members: source_pack::MAX_SEGMENT_MEMBERS,
                        max_uncompressed_bytes: source_pack::MAX_SOURCE_BYTES
                            * source_pack::MAX_SEGMENT_MEMBERS as u64,
                    },
                    None,
                    |entry| {
                        let (ci, pi) = page_map[&entry.relative_path];
                        let catalog = &metadata.catalogs[ci];
                        builder.insert_page(
                            root,
                            &catalog.code,
                            &catalog.pages[pi],
                            &entry.path,
                            options,
                        )
                    },
                )?;
                observed.push(Observed {
                    path: shard,
                    sha256: sha(&segment["sha256"])?,
                    bytes: positive(&segment["bytes"])?,
                    limit: 34 * 1024 * 1024 * 1024,
                });
            }
            let archive = pack_root.join("cache/archive");
            let assembly =
                archive_observations(root, &archive, &metadata, options, false, &mut observed)?;
            let source_binding = json!({"kind":"local_lossless_shards","source_pack_manifest":manifest_path.strip_prefix(root)?.to_str().context("supplier_path_utf8")?,"source_pack_manifest_sha256":manifest_sha,"metadata_transport":metadata_transport,"segments_verified":segments.len(),"page_members_verified":declared_pages.len(),"old_index_text_reused":false});
            let meta =
                builder.finish(&metadata, source_binding.clone(), assembly.clone(), options)?;
            (
                metadata,
                archive,
                Some((manifest_path, manifest_sha)),
                source_binding,
                assembly,
                meta,
            )
        }
    };
    let (index_sha256, index_bytes) = file_hash(&index_path, crate::index::MAX_INDEX_BYTES)?;
    let source_catalogs = json!({"source_identity":metadata.source_identity(),"counts":meta["counts"],"index_fingerprint":meta["fingerprint"]});
    let mut registration = json!({"schema":"ontology-engineering.misumi-source-location/v1","archive_root":relative_to(&archive_root,&target)?,"index_path":"catalog.sqlite3","index_sha256":index_sha256,"source_catalogs":source_catalogs,"verification":"source metadata and every original page SHA256/native physical page count verified; no engineering parameter verification"});
    if let Some((path, hash)) = pack_manifest {
        registration["source_pack_manifest"] = json!(relative_to(&path, &target)?);
        registration["source_pack_manifest_sha256"] = json!(hash);
    }
    oe_core::write_new(&built.join("source.json"), &registration)?;
    check_all(&observed)?;
    crate::publication::publish_directory(
        stage,
        &built,
        &target,
        previous.as_ref(),
        owned_snapshot,
        || check_all(&observed),
    )?;
    Ok(
        json!({"status":"indexed","provider":"misumi","data_root":target.strip_prefix(root)?.to_str().context("supplier_path_utf8")?,"index_sha256":index_sha256,"index_bytes":index_bytes,"index":meta,"source_binding":source_binding,"assembled_pdf_verification":assembly,"originals_modified":false,"network_requests":0,"engineering_parameter_verification":"not_run"}),
    )
}

#[cfg(test)]
mod installed_metadata_test {
    #[test]
    #[ignore = "reads only the optional installed supplier metadata; no PDF/text scan"]
    fn actual_installed_metadata_contract() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let value =
            super::parse_archive_metadata(root, &root.join("sources/.indexes/misumi/archive"))
                .unwrap();
        assert_eq!(value.files.len(), 8);
        for (i, (pages, chapters, hotspots)) in [(3032, 54, 35456), (1620, 22, 13409)]
            .into_iter()
            .enumerate()
        {
            let catalog = &value.catalogs[i];
            assert_eq!(catalog.pages.len(), pages);
            assert_eq!(catalog.chapters.len(), chapters);
            assert_eq!(
                catalog
                    .pages
                    .iter()
                    .map(|p| p.hotspots.len())
                    .sum::<usize>(),
                hotspots
            );
            eprintln!(
                "verified metadata {}: pages={},chapters={},hotspots={}",
                catalog.code, pages, chapters, hotspots
            );
        }
    }
}
