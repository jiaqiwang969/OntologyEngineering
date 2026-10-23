//! Incremental registration reads a validated, read-only snapshot and reconstructs
//! a fresh fixed schema. No SQL/schema/FTS state from the input is made writable.
use super::*;
use crate::index::ReadIndex;
use std::collections::BTreeMap;

const SOURCE_KEYS: &[&str] = &["id", "title", "edition", "path", "sha256", "chapters"];
fn declaration(source: &Value) -> Value {
    Value::Object(
        SOURCE_KEYS
            .iter()
            .map(|k| ((*k).to_owned(), source[*k].clone()))
            .collect(),
    )
}

pub(super) struct Previous {
    pub snapshot: Value,
    pub read: ReadIndex,
    pub sources: Vec<Value>,
}
impl Previous {
    pub fn open(
        root: &oe_core::SkillRoot,
        target: &Path,
        options: &RegistrationOptions,
    ) -> Result<Option<Self>> {
        if !target.try_exists()? {
            return Ok(None);
        }
        ensure!(
            options.replace || options.merge,
            "local_pdf_data_root_exists"
        );
        let snapshot = owned_snapshot(target)?;
        let registration = oe_core::json::strict_json(&oe_core::read_regular_bytes(
            &target.join("registration.json"),
            8 * 1024 * 1024,
        )?)?;
        let read = ReadIndex::open(root, target, "local_pdf")?;
        ensure!(
            read.identity()["sha256"] == snapshot["index_sha256"]
                && read.meta["registration_sha256"]
                    == oe_core::json::canonical_digest(
                        &json!({"schema":registration["schema"],"sources":registration["sources"]})
                    )?,
            "local_pdf_existing_registration_binding_mismatch"
        );
        let stored = registration["sources"]
            .as_array()
            .context("local_pdf_existing_sources_invalid")?;
        for source in stored {
            keys(
                source,
                &[
                    "id",
                    "title",
                    "edition",
                    "path",
                    "sha256",
                    "chapters",
                    "bytes",
                    "page_count",
                ],
            )?;
            ensure!(
                source["page_count"]
                    .as_u64()
                    .is_some_and(|n| n > 0 && n <= 20000)
                    && source["bytes"]
                        .as_u64()
                        .is_some_and(|n| n > 0 && n <= 4 * 1024 * 1024 * 1024),
                "local_pdf_existing_source_bounds"
            );
        }
        // Stored paths are index-relative (normally ../../books/...). Resolve
        // them with the same no-follow source-root boundary used by readers;
        // new manifest inputs retain their stricter no-parent-components rule.
        let declarations = stored
            .iter()
            .map(|source| {
                let mut value = declaration(source);
                value["path"] = json!(crate::sources::resolve_original(
                    root,
                    target,
                    text(source, "path")?
                )?);
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?;
        let raw = oe_core::json::canonical_json(
            &json!({"schema":"ontology-engineering.local-pdf-manifest/v1","sources":declarations}),
        )?;
        let mut sources = manifest_sources(root.path(), &target.join("registration.json"), &raw)?;
        for (source, old) in sources.iter_mut().zip(stored) {
            source["page_count"] = old["page_count"].clone();
            source["bytes"] = old["bytes"].clone();
        }
        let chapters: usize = sources
            .iter()
            .map(|s| s["chapters"].as_array().expect("validated").len())
            .sum();
        let pages: u64 = sources
            .iter()
            .map(|s| s["page_count"].as_u64().expect("validated"))
            .sum();
        ensure!(
            chapters <= 4096
                && pages <= 20000
                && read.meta["counts"]["catalogs"] == sources.len()
                && read.meta["counts"]["chapters"] == chapters
                && read.meta["counts"]["pages"] == pages,
            "local_pdf_existing_counts_mismatch"
        );
        let actual = read.rows("SELECT (SELECT count(*) FROM pages) AS pages,(SELECT count(*) FROM chapters) AS chapters", &[], 1)?;
        ensure!(
            actual.len() == 1 && actual[0]["pages"] == pages && actual[0]["chapters"] == chapters,
            "local_pdf_existing_row_counts_mismatch"
        );
        let previous = Self {
            snapshot,
            read,
            sources,
        };
        previous.verify(target)?;
        Ok(Some(previous))
    }
    pub fn verify(&self, target: &Path) -> Result<()> {
        self.read.verify()?;
        ensure!(
            owned_snapshot(target)? == self.snapshot,
            "local_pdf_existing_snapshot_changed"
        );
        Ok(())
    }
}

pub(super) struct Plan {
    pub sources: Vec<Value>,
    pub update: Value,
    pub final_counts: Value,
    reuse: BTreeSet<String>,
}
impl Plan {
    pub fn new(
        incoming: Vec<Value>,
        previous: Option<&Previous>,
        options: &RegistrationOptions,
    ) -> Result<Self> {
        let old: BTreeMap<_, _> = previous
            .into_iter()
            .flat_map(|p| p.sources.iter())
            .map(|s| (s["id"].as_str().expect("validated"), s))
            .collect();
        let supplied: BTreeMap<_, _> = incoming
            .iter()
            .map(|s| (s["id"].as_str().expect("validated"), s))
            .collect();
        let (mut added, mut updated, mut unchanged, mut retained, mut removed) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut reuse = BTreeSet::new();
        for (id, source) in &supplied {
            match old.get(id) {
                None => added.push(*id),
                Some(prior) if declaration(prior) == **source => {
                    unchanged.push(*id);
                    if options.merge {
                        reuse.insert((*id).to_owned());
                    }
                }
                Some(_) => updated.push(*id),
            }
        }
        for id in old.keys().filter(|id| !supplied.contains_key(*id)) {
            if options.merge {
                retained.push(*id);
                reuse.insert((*id).to_owned());
            } else {
                removed.push(*id);
            }
        }
        // Existing order is stable in a merge; new IDs follow manifest order.
        let sources: Vec<Value> = if options.merge {
            previous
                .into_iter()
                .flat_map(|p| p.sources.iter())
                .map(|s| {
                    supplied
                        .get(s["id"].as_str().expect("validated"))
                        .map(|v| (*v).clone())
                        .unwrap_or_else(|| declaration(s))
                })
                .chain(
                    incoming
                        .iter()
                        .filter(|s| !old.contains_key(s["id"].as_str().expect("validated")))
                        .cloned(),
                )
                .collect()
        } else {
            incoming.clone()
        };
        ensure!(
            !sources.is_empty() && sources.len() <= 256,
            "local_pdf_source_count_invalid"
        );
        let chapters: usize = sources
            .iter()
            .map(|s| s["chapters"].as_array().expect("validated").len())
            .sum();
        ensure!(chapters <= 4096, "local_pdf_total_chapter_limit");
        let required: Vec<_> = sources
            .iter()
            .filter(|s| !reuse.contains(s["id"].as_str().expect("validated")))
            .map(|s| s["id"].clone())
            .collect();
        let known_pages: u64 = reuse
            .iter()
            .map(|id| old[id.as_str()]["page_count"].as_u64().expect("validated"))
            .sum();
        ensure!(known_pages <= 20000, "local_pdf_registration_total_limit");
        let mode = if options.merge {
            "merge"
        } else if options.replace {
            "replace"
        } else {
            "create"
        };
        let strings = |ids: Vec<&str>| ids.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let update = serde_json::to_value(oe_contracts::LocalPdfRegistrationUpdate {
            mode: mode.to_owned(),
            added: strings(added),
            updated: strings(updated),
            unchanged: strings(unchanged),
            retained: strings(retained),
            removed: strings(removed),
            final_source_count: sources.len() as u64,
            reused_source_ids: reuse.iter().cloned().collect(),
            extracted_source_ids: Vec::new(),
            extraction_required_ids: required
                .iter()
                .map(|v| v.as_str().expect("id").to_owned())
                .collect(),
        })?;
        let final_counts = json!({"catalogs":sources.len(),"chapters":chapters,"pages":if required.is_empty() {Some(known_pages)} else {None}});
        Ok(Self {
            sources,
            update,
            final_counts,
            reuse,
        })
    }
    pub fn reused<'a>(&self, source: &Value, previous: Option<&'a Previous>) -> Option<&'a Value> {
        let id = source["id"].as_str().expect("validated");
        if self.reuse.contains(id) {
            previous?.sources.iter().find(|s| s["id"] == id)
        } else {
            None
        }
    }
    pub fn is_noop(&self, options: &RegistrationOptions) -> bool {
        options.merge && self.reuse.len() == self.sources.len()
    }
}

pub(super) struct Counts {
    pub value: Value,
    text_bytes: usize,
    pages: usize,
}
impl Counts {
    pub fn new(catalogs: usize) -> Self {
        Self {
            value: json!({"catalogs":catalogs,"pages":0,"chapters":0,"native_text_sparse":0,"native_text_missing":0,"native_text_chars":0}),
            text_bytes: 0,
            pages: 0,
        }
    }
    pub fn add_source(&mut self, source: &Value, bytes: usize) -> Result<()> {
        let pages = usize::try_from(
            source["page_count"]
                .as_u64()
                .context("local_pdf_page_count_invalid")?,
        )?;
        self.pages = self
            .pages
            .checked_add(pages)
            .context("local_pdf_registration_total_limit")?;
        self.text_bytes = self
            .text_bytes
            .checked_add(bytes)
            .context("local_pdf_registration_total_limit")?;
        ensure!(
            self.pages <= 20000 && self.text_bytes <= 128 * 1024 * 1024,
            "local_pdf_registration_total_limit"
        );
        self.value["chapters"] = json!(
            self.value["chapters"].as_u64().unwrap()
                + source["chapters"].as_array().expect("validated").len() as u64
        );
        Ok(())
    }
    pub fn add_page(&mut self, row: &Value) -> Result<()> {
        for (key, n) in [
            ("pages", 1),
            (
                "native_text_sparse",
                row["native_text_sparse"]
                    .as_u64()
                    .context("local_pdf_page_sparse_invalid")?,
            ),
            (
                "native_text_missing",
                u64::from(
                    row["body"]
                        .as_str()
                        .context("local_pdf_page_body_invalid")?
                        .trim()
                        .is_empty(),
                ),
            ),
            (
                "native_text_chars",
                row["text_chars"]
                    .as_u64()
                    .context("local_pdf_page_chars_invalid")?,
            ),
        ] {
            self.value[key] = json!(
                self.value[key]
                    .as_u64()
                    .unwrap()
                    .checked_add(n)
                    .context("local_pdf_count_overflow")?
            );
        }
        Ok(())
    }
}

pub(super) fn insert_chapters(db: &Connection, source: &Value) -> Result<()> {
    let code = format!("local_pdf:{}", text(source, "id")?);
    for chapter in source["chapters"].as_array().expect("validated") {
        ensure!(
            chapter["end_page"].as_u64().expect("validated")
                <= source["page_count"]
                    .as_u64()
                    .context("local_pdf_page_count_invalid")?,
            "local_pdf_chapter_outside_document"
        );
        db.execute(
            "INSERT INTO chapters VALUES (?,?,?,?,?,?)",
            params![
                format!("{code}:chapter:{}", text(chapter, "id")?),
                code,
                text(chapter, "title")?,
                chapter["start_page"].as_u64(),
                chapter["end_page"].as_u64(),
                encoded(chapter)?
            ],
        )?;
    }
    Ok(())
}

pub(super) fn page_record(
    source: &Value,
    n: u64,
    body: &str,
    raw_sha: &str,
    raw_chars: u64,
) -> Result<(Value, Value)> {
    let code = format!("local_pdf:{}", text(source, "id")?);
    let relevant: Vec<_> = source["chapters"]
        .as_array()
        .expect("validated")
        .iter()
        .filter(|c| c["start_page"].as_u64().unwrap() <= n && c["end_page"].as_u64().unwrap() >= n)
        .collect();
    let chapter = relevant
        .iter()
        .min_by_key(|c| c["end_page"].as_u64().unwrap() - c["start_page"].as_u64().unwrap());
    let headings: Vec<_> = relevant
        .iter()
        .map(|c| json!({"text":c["title"],"origin":"declared_chapter_navigation"}))
        .collect();
    let terms = relevant
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                c["title"].as_str().unwrap(),
                c["aliases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let sparse = body.chars().filter(|c| !c.is_whitespace()).count() < 80;
    let row = json!({"id":format!("{code}:page:{n}"),"catalog":code,"page_id":n,"pdf_page":n,"printed_page":null,
        "chapter_id":chapter.map(|c|format!("{code}:chapter:{}",c["id"].as_str().unwrap())),"source_relative_path":"","source_url":"",
        "source_sha256":source["sha256"],"source_bytes":source["bytes"],"body":body,"text_sha256":oe_core::sha256(body),
        "raw_text_sha256":raw_sha,"raw_text_chars":raw_chars,"text_chars":body.chars().count(),"native_text_sparse":u64::from(sparse),
        "titles_json":encoded(&json!(headings))?,"series_json":"[]","series_count":0,
        "source_json":encoded(&json!({"document_id":source["id"],"text_origin":"poppler_native_text"}))?});
    let search = json!([
        "",
        crate::text::tokens(&terms).join(" "),
        crate::text::tokens(body).join(" "),
        crate::text::tokens(chapter.map(|c| c["title"].as_str().unwrap()).unwrap_or("")).join(" ")
    ]);
    Ok((row, search))
}
const PAGE_COLUMNS: &[&str] = &[
    "id",
    "catalog",
    "page_id",
    "pdf_page",
    "printed_page",
    "chapter_id",
    "source_relative_path",
    "source_url",
    "source_sha256",
    "source_bytes",
    "body",
    "text_sha256",
    "raw_text_sha256",
    "raw_text_chars",
    "text_chars",
    "native_text_sparse",
    "titles_json",
    "series_json",
    "series_count",
    "source_json",
];
fn sql_value(value: &Value) -> Result<rusqlite::types::Value> {
    Ok(match value {
        Value::Null => rusqlite::types::Value::Null,
        Value::String(v) => rusqlite::types::Value::Text(v.clone()),
        Value::Number(v) => {
            rusqlite::types::Value::Integer(v.as_i64().context("local_pdf_sql_integer_invalid")?)
        }
        _ => anyhow::bail!("local_pdf_sql_value_invalid"),
    })
}
pub(super) fn insert_page(db: &Connection, row: &Value, search: &Value) -> Result<()> {
    let values = PAGE_COLUMNS
        .iter()
        .map(|k| sql_value(&row[*k]))
        .collect::<Result<Vec<_>>>()?;
    db.execute(
        "INSERT INTO pages VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        rusqlite::params_from_iter(values),
    )?;
    db.execute(
        "INSERT INTO search(rowid,models,titles,body,chapter) VALUES (?,?,?,?,?)",
        params![
            db.last_insert_rowid(),
            search[0].as_str(),
            search[1].as_str(),
            search[2].as_str(),
            search[3].as_str()
        ],
    )?;
    Ok(())
}

/// Batches keep the read adapter's per-query budget. Rebuild rowids and FTS from
/// verified native page text and the unchanged declared navigation.
pub(super) fn reuse(
    read: &ReadIndex,
    source: &Value,
    options: &RegistrationOptions,
    counts: &mut Counts,
    mut accept: impl FnMut(Value, Value) -> Result<()>,
) -> Result<Value> {
    ensure!(
        read.meta["extraction"]
            == "pdftotext -layout -enc UTF-8; horizontal whitespace collapsed; no OCR"
            && read.meta["tokenizer"] == "Rust Unicode NFKC + Unicode lowercase; ASCII/CJK ngrams",
        "local_pdf_reuse_transform_unsupported"
    );
    let observations: Vec<_> = read.meta["tools"]
        .as_array()
        .context("local_pdf_reuse_tools_missing")?
        .iter()
        .filter(|t| t["source_id"] == source["id"])
        .collect();
    ensure!(observations.len() == 1, "local_pdf_reuse_tools_invalid");
    let observation = observations[0];
    keys(observation, &["source_id", "tools"])?;
    let tools = &observation["tools"];
    for name in ["pdfinfo", "pdftotext"] {
        let identity: &Value = &tools[name];
        ensure!(
            identity["sha256"].as_str().is_some_and(valid_sha)
                && identity["path"]
                    .as_str()
                    .is_some_and(|s| Path::new(s).is_absolute())
                && identity["version"].as_str().is_some_and(|s| !s.is_empty())
                && identity["bytes"].as_u64().is_some_and(|n| n > 0),
            "local_pdf_reuse_tool_identity_invalid"
        );
    }
    ensure!(
        tools["ocr"] == "not_run"
            && tools["pdfinfo_output_sha256"]
                .as_str()
                .is_some_and(valid_sha)
            && tools["text_output_sha256"].as_str().is_some_and(valid_sha),
        "local_pdf_reuse_observation_invalid"
    );
    let page_count = source["page_count"]
        .as_u64()
        .context("local_pdf_page_count_invalid")?;
    let bytes = tools["stdout_bytes"]
        .as_u64()
        .context("local_pdf_reuse_text_size_missing")?;
    ensure!(
        options.limits.pages > 0
            && options.limits.pages <= 20000
            && options.limits.output_bytes > 0
            && options.limits.output_bytes <= 64 * 1024 * 1024
            && options.limits.page_bytes > 0
            && options.limits.page_bytes <= 2 * 1024 * 1024
            && page_count <= options.limits.pages as u64
            && bytes <= options.limits.output_bytes
            && tools["page_limit_bytes"]
                .as_u64()
                .is_some_and(|n| n <= options.limits.page_bytes as u64),
        "local_pdf_reuse_exceeds_requested_limits"
    );
    counts.add_source(source, usize::try_from(bytes)?)?;
    let code = format!("local_pdf:{}", text(source, "id")?);
    let actual_count = read.rows(
        "SELECT count(*) AS pages FROM pages WHERE catalog=?",
        &[&code],
        1,
    )?;
    ensure!(
        actual_count.len() == 1 && actual_count[0]["pages"] == page_count,
        "local_pdf_reuse_page_count_mismatch"
    );
    let expected_chapters = source["chapters"].as_array().expect("validated");
    let actual_chapters = read.rows(
        "SELECT * FROM chapters WHERE catalog=? ORDER BY id",
        &[&code],
        4097,
    )?;
    ensure!(
        actual_chapters.len() == expected_chapters.len(),
        "local_pdf_reuse_chapter_count_mismatch"
    );
    for chapter in expected_chapters {
        let cid = format!("{code}:chapter:{}", text(chapter, "id")?);
        let expected = json!({"id":cid,"catalog":code,"title":chapter["title"],"start_page":chapter["start_page"],"end_page":chapter["end_page"],"source_json":encoded(chapter)?});
        ensure!(
            actual_chapters.iter().any(|c| c == &expected)
                && chapter["end_page"].as_u64().expect("validated") <= page_count,
            "local_pdf_reuse_chapter_mismatch"
        );
    }
    let mut offset = 0_u64;
    let mut body_bytes = 0_u64;
    let mut source_raw_chars = 0_u64;
    loop {
        let batch = read.rows(
            "SELECT * FROM pages WHERE catalog=? AND pdf_page>? ORDER BY pdf_page,id LIMIT 8",
            &[&code, &offset],
            8,
        )?;
        if batch.is_empty() {
            break;
        }
        for row in batch {
            offset += 1;
            ensure!(offset <= page_count, "local_pdf_reuse_page_count_mismatch");
            let body = row["body"]
                .as_str()
                .context("local_pdf_reuse_body_invalid")?;
            let raw_sha = row["raw_text_sha256"]
                .as_str()
                .context("local_pdf_reuse_raw_identity_invalid")?;
            let raw_chars = row["raw_text_chars"]
                .as_u64()
                .context("local_pdf_reuse_raw_identity_invalid")?;
            body_bytes += body.len() as u64;
            source_raw_chars = source_raw_chars
                .checked_add(raw_chars)
                .context("local_pdf_reuse_text_size_mismatch")?;
            ensure!(
                valid_sha(raw_sha)
                    && body_bytes <= bytes
                    && source_raw_chars <= bytes
                    && body.len() <= options.limits.page_bytes
                    && raw_chars >= body.chars().count() as u64
                    && raw_chars <= options.limits.page_bytes as u64,
                "local_pdf_reuse_page_bounds_invalid"
            );
            let (expected, search) = page_record(source, offset, body, raw_sha, raw_chars)?;
            ensure!(row == expected, "local_pdf_reuse_page_binding_mismatch");
            counts.add_page(&row)?;
            accept(row, search)?;
        }
    }
    ensure!(offset == page_count, "local_pdf_reuse_page_count_mismatch");
    Ok(observation.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_pdf() -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>",
            "<< /Length 0 >>\nstream\n\nendstream",
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend(b"xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend(
            format!("trailer << /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        out
    }
    fn fixture() -> (Stage, PathBuf, RegistrationOptions) {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let stage = Stage::new(project, "registration-final-guard-test").unwrap();
        fs::write(stage.path.join("SKILL.md"), "owned native fixture").unwrap();
        fs::create_dir(stage.path.join("sources")).unwrap();
        let original = stage.path.join("sources/original.pdf");
        let bytes = tiny_pdf();
        fs::write(&original, &bytes).unwrap();
        let manifest = stage.path.join("sources/manifest.json");
        oe_core::write_new(&manifest,&json!({"schema":"ontology-engineering.local-pdf-manifest/v1","sources":[{"id":"one","title":"Fixture","edition":"one","path":"original.pdf","sha256":oe_core::sha256(bytes),"chapters":[]}]})).unwrap();
        let options = RegistrationOptions {
            tools: PopplerTools {
                pdfinfo: "/opt/homebrew/bin/pdfinfo".into(),
                pdftotext: "/opt/homebrew/bin/pdftotext".into(),
                pdftoppm: "/opt/homebrew/bin/pdftoppm".into(),
            },
            ..Default::default()
        };
        (stage, manifest, options)
    }
    #[test]
    fn preview_and_noop_run_final_intake_guard_and_preserve_index_bytes() {
        let (stage, manifest, mut options) = fixture();
        options.dry_run = true;
        let error = register_with_guard(&stage.path, &manifest, &options, || {
            anyhow::bail!("delivery_guard_rejected")
        })
        .unwrap_err();
        assert!(error.to_string().contains("delivery_guard_rejected"));
        assert!(!stage.path.join("sources/.indexes/local-pdf").exists());
        options.dry_run = false;
        register(&stage.path, &manifest, &options).unwrap();
        let index = stage.path.join("sources/.indexes/local-pdf");
        let before = owned_snapshot(&index).unwrap();
        options.merge = true;
        options.tools.pdfinfo = stage.path.join("absent-tool");
        options.tools.pdftotext = stage.path.join("absent-tool");
        for dry_run in [false, true] {
            options.dry_run = dry_run;
            let error = register_with_guard(&stage.path, &manifest, &options, || {
                anyhow::bail!("delivery_guard_rejected")
            })
            .unwrap_err();
            assert!(error.to_string().contains("delivery_guard_rejected"));
            assert_eq!(owned_snapshot(&index).unwrap(), before);
        }
    }
    #[test]
    fn final_publication_rechecks_original_after_owner_guard_and_rolls_back() {
        let (stage, manifest, mut options) = fixture();
        register(&stage.path, &manifest, &options).unwrap();
        let index = stage.path.join("sources/.indexes/local-pdf");
        let before = owned_snapshot(&index).unwrap();
        options.replace = true;
        let error = register_with_guard(&stage.path, &manifest, &options, || {
            fs::write(
                stage.path.join("sources/original.pdf"),
                b"changed at final guard",
            )?;
            Ok(())
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("source_changed_during_registration")
        );
        assert_eq!(owned_snapshot(&index).unwrap(), before);
    }
}
