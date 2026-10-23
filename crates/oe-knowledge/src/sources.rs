//! Common local-PDF and MISUMI providers over existing source-bound indexes.
use crate::{
    index::{MAX_VALUE_BYTES, ReadIndex},
    text::match_expression,
};
use anyhow::{Context, Result, ensure};
use oe_core::json::{canonical_digest, strict_json};
use oe_core::{FileIdentity, SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

pub struct Provider {
    root: SkillRoot,
    kind: String,
    directory: PathBuf,
    index: ReadIndex,
    registration: Option<(PathBuf, FileIdentity)>,
    source_manifest: Option<(PathBuf, FileIdentity)>,
    archive_root: Option<PathBuf>,
    sources: Vec<Value>,
    catalogs: BTreeMap<String, Value>,
    chapter_rows: Vec<Value>,
}

#[derive(Debug, Clone)]
pub enum RetrievalRequest {
    Lexical {
        query: String,
        chapters: Vec<String>,
        limit: usize,
    },
    Body {
        anchor: String,
        terms: String,
        chapters: Vec<String>,
        limit: usize,
    },
}

impl Provider {
    pub fn open(root: &SkillRoot, kind: &str, index_dir: Option<&Path>) -> Result<Self> {
        let kind = match kind {
            "local_pdf" | "local-pdf" => "local_pdf",
            "misumi" => "misumi",
            _ => anyhow::bail!("unknown_provider"),
        };
        let default = PathBuf::from("sources/.indexes").join(if kind == "local_pdf" {
            "local-pdf"
        } else {
            "misumi"
        });
        let directory = root.expand(index_dir.unwrap_or(&default))?;
        ensure!(
            directory.starts_with(root.path().join("sources/.indexes"))
                && directory != root.path().join("sources/.indexes"),
            "index_outside_registered_sources"
        );
        no_symlinks(&directory)?;
        if kind == "misumi" {
            let mut manifests = 0;
            for name in ["source.json", "source-pack.json", "data-manifest.json"] {
                manifests += usize::from(directory.join(name).try_exists()?);
            }
            ensure!(manifests <= 1, "misumi_ambiguous_source_registration");
        }
        let registration_path = directory.join(if kind == "local_pdf" {
            "registration.json"
        } else if directory.join("source.json").try_exists()? {
            "source.json"
        } else if directory.join("data-manifest.json").try_exists()? {
            "data-manifest.json"
        } else {
            "source-pack.json"
        });
        let registration = if kind == "local_pdf" || registration_path.try_exists()? {
            let raw = read_regular_bytes(&registration_path, MAX_VALUE_BYTES)?;
            let identity = FileIdentity {
                bytes: raw.len() as u64,
                sha256: oe_core::sha256(&raw),
            };
            Some((strict_json(&raw)?, registration_path, identity))
        } else {
            None
        };
        let location = registration.as_ref().map(|r| &r.0).filter(|v| {
            v["schema"] == "ontology-engineering.misumi-source-location/v1" && kind == "misumi"
        });
        let index = if let Some(location) = location {
            let path = resolve_registered(root, &directory, string(location, "index_path")?)?;
            ReadIndex::open_source(root, &path, kind)?
        } else {
            ReadIndex::open(root, &directory, kind)?
        };
        let mut source_manifest = None;
        let mut archive_root = None;
        let mut sources = Vec::new();
        if kind == "local_pdf" {
            let document = &registration
                .as_ref()
                .context("local_pdf_registration_missing")?
                .0;
            ensure!(
                document.as_object().is_some_and(|o| o.len() == 3)
                    && document["schema"] == "ontology-engineering.local-pdf-registration/v1",
                "local_pdf_registration_invalid"
            );
            ensure!(
                document["index_sha256"] == index.identity()["sha256"],
                "local_pdf_index_hash_mismatch"
            );
            let base = json!({"schema":document["schema"],"sources":document["sources"]});
            ensure!(
                index.meta["registration_sha256"] == canonical_digest(&base)?,
                "local_pdf_registration_identity_mismatch"
            );
            let entries = document["sources"]
                .as_array()
                .context("local_pdf_registration_sources_invalid")?;
            ensure!(
                !entries.is_empty() && entries.len() <= 256,
                "local_pdf_registration_sources_limit"
            );
            let mut seen = BTreeSet::new();
            for entry in entries {
                let id = string(entry, "id")?;
                ensure!(
                    valid_id(id) && seen.insert(id.to_owned()),
                    "local_pdf_source_id_invalid"
                );
                ensure!(
                    valid_sha(string(entry, "sha256")?)
                        && entry["page_count"].as_u64().is_some_and(|n| n > 0)
                        && entry["bytes"].as_u64().is_some_and(|n| n > 0),
                    "local_pdf_source_identity_invalid"
                );
                string(entry, "title")?;
                string(entry, "edition")?;
                resolve_original(root, &directory, string(entry, "path")?)?;
                sources.push(entry.clone());
            }
        } else if let Some(location) = location {
            ensure!(
                location["index_sha256"] == index.identity()["sha256"],
                "misumi_registered_index_hash_mismatch"
            );
            let recorded = &location["source_catalogs"];
            ensure!(
                recorded["index_fingerprint"] == index.meta["fingerprint"]
                    && recorded["counts"] == index.meta["counts"]
                    && recorded["source_identity"].is_array()
                    && recorded["source_identity"] == index.meta["source_identity"],
                "misumi_registered_catalog_identity_mismatch"
            );
            archive_root = Some(resolve_registered(
                root,
                &directory,
                string(location, "archive_root")?,
            )?);
            match (
                location.get("source_pack_manifest"),
                location.get("source_pack_manifest_sha256"),
            ) {
                (None, None) => {}
                (Some(path), Some(sha)) => {
                    let manifest = resolve_registered(
                        root,
                        &directory,
                        path.as_str()
                            .context("misumi_registered_pack_path_invalid")?,
                    )?;
                    let identity =
                        fingerprint_file(&manifest, crate::source_pack::MAX_MANIFEST_BYTES as u64)?;
                    ensure!(
                        sha.as_str() == Some(identity.sha256.as_str()),
                        "misumi_registered_pack_hash_mismatch"
                    );
                    crate::source_pack::load_manifest(root.path(), &manifest)?;
                    ensure!(
                        archive_root.as_ref()
                            == Some(
                                &manifest
                                    .parent()
                                    .context("source_pack_parent")?
                                    .join("cache/archive")
                            ),
                        "misumi_registered_pack_archive_mismatch"
                    );
                    source_manifest = Some((manifest, identity));
                }
                _ => anyhow::bail!("misumi_registered_pack_binding_incomplete"),
            }
        } else if let Some((document, _, _)) = &registration
            && document["schema"] == "ontology-engineering.misumi-data/v1"
        {
            let files = document["files"]
                .as_array()
                .context("misumi_data_inventory_missing")?;
            let entries = files
                .iter()
                .filter(|f| f["path"] == "catalog.sqlite3")
                .collect::<Vec<_>>();
            ensure!(
                entries.len() == 1
                    && entries[0]["sha256"] == index.identity()["sha256"]
                    && entries[0]["bytes"] == index.identity()["bytes"],
                "misumi_imported_index_hash_mismatch"
            );
            let recorded = &document["source_catalogs"];
            ensure!(
                recorded["source_identity"].is_array()
                    && recorded["source_identity"] == index.meta["source_identity"]
                    && recorded["counts"] == index.meta["counts"]
                    && recorded["index_fingerprint"] == index.meta["fingerprint"],
                "misumi_imported_catalog_identity_mismatch"
            );
            archive_root = Some(directory.join("archive"));
        } else if let Some((document, path, identity)) = &registration {
            ensure!(
                document["schema"] == "ontology-engineering.misumi-source-pack/v1"
                    && document["index"]["path"] == "catalog.sqlite3",
                "misumi_source_pack_invalid"
            );
            ensure!(
                document["index"]["sha256"] == index.identity()["sha256"]
                    && document["index"]["bytes"] == index.identity()["bytes"],
                "misumi_source_pack_index_mismatch"
            );
            source_manifest = Some((path.clone(), identity.clone()));
        } else if kind == "misumi" {
            // Legacy fully imported libraries keep originals beside their DB.
            archive_root = Some(directory.join("archive"));
        }
        if kind == "misumi"
            && let Some(binding) = index.meta.get("source_binding")
        {
            match binding["kind"].as_str() {
                Some("local_lossless_shards") => {
                    let (manifest, identity) = source_manifest
                        .as_ref()
                        .context("misumi_index_pack_binding_missing")?;
                    ensure!(
                        binding["source_pack_manifest"]
                            == manifest
                                .strip_prefix(root.path())?
                                .to_str()
                                .context("source_path_not_utf8")?
                            && binding["source_pack_manifest_sha256"] == identity.sha256,
                        "misumi_index_pack_binding_mismatch"
                    );
                }
                Some("expanded_archive") => {
                    // The recorded build path is provenance. Explicit library
                    // import/register may relocate the same hash-bound originals.
                    ensure!(
                        source_manifest.is_none() && archive_root.is_some(),
                        "misumi_index_archive_binding_mismatch"
                    );
                }
                _ => anyhow::bail!("misumi_index_source_binding_unknown"),
            }
        }
        let mut catalogs = BTreeMap::new();
        if kind == "misumi" {
            for row in index.rows("SELECT * FROM catalogs ORDER BY code LIMIT 65", &[], 64)? {
                catalogs.insert(string(&row, "code")?.to_owned(), row);
            }
        }
        let mut chapter_rows = index.rows(
            "SELECT * FROM chapters ORDER BY catalog,start_page LIMIT 4097",
            &[],
            4096,
        )?;
        for row in &mut chapter_rows {
            let source = strict_json(string(row, "source_json")?.as_bytes())?;
            ensure!(
                row["start_page"].as_u64().is_some_and(|p| p > 0)
                    && row["end_page"].as_u64() >= row["start_page"].as_u64(),
                "chapter_range_invalid"
            );
            if kind == "local_pdf" {
                ensure!(
                    source["aliases"].is_array(),
                    "local_pdf_chapter_aliases_invalid"
                );
                row["aliases"] = source["aliases"].clone();
            }
            row["source_kind"] = json!(kind);
        }
        let result = Self {
            root: root.clone(),
            kind: kind.to_owned(),
            directory,
            index,
            registration: registration.map(|(_, p, i)| (p, i)),
            source_manifest,
            archive_root,
            sources,
            catalogs,
            chapter_rows,
        };
        result.verify()?;
        Ok(result)
    }

    fn verify(&self) -> Result<()> {
        self.index.verify()?;
        if let Some((path, identity)) = &self.registration {
            ensure!(
                &fingerprint_file(path, MAX_VALUE_BYTES as u64)? == identity,
                "index_registration_changed"
            );
        }
        if let Some((path, identity)) = &self.source_manifest {
            ensure!(
                &fingerprint_file(path, crate::source_pack::MAX_MANIFEST_BYTES as u64)? == identity,
                "index_source_pack_changed"
            );
        }
        Ok(())
    }

    pub fn status(&self) -> Value {
        let ready = self.verify().is_ok();
        json!({"status":if ready {"ready"} else {"not_ready"},"provider":self.kind,"index":self.index.meta,"index_identity":self.index.identity(),
            "errors":if ready {vec![]} else {vec![json!({"code":"index_identity_changed_or_unavailable"})]},
            "source_verification":"index_and_registration_checked_original_hash_checked_at_return","ocr":"not_run","network_requests":0})
    }

    pub fn chapters(&self) -> Result<Vec<Value>> {
        self.verify()?;
        Ok(self.chapter_rows.clone())
    }

    pub fn page(&self, id: &str) -> Result<Value> {
        ensure!(!id.is_empty() && id.len() <= 512, "page_id_invalid");
        self.verify()?;
        let rows = self
            .index
            .rows("SELECT * FROM pages WHERE id=? LIMIT 2", &[&id], 1)?;
        let page = rows.into_iter().next().context("page_not_in_index")?;
        let page = self.decorate(page)?;
        self.verify()?;
        Ok(page)
    }

    pub fn lookup_page(&self, id: &str) -> Result<Value> {
        self.page(id)
    }

    pub fn source_result(
        &self,
        candidate: &Value,
        query: &str,
        tools: &crate::poppler::PopplerTools,
    ) -> Result<Value> {
        let mut row = self.page(string(candidate, "id")?)?;
        for field in [
            "body",
            "text_sha256",
            "pdf_page",
            "source_sha256",
            "catalog",
            "index_identity",
        ] {
            ensure!(
                candidate[field] == row[field],
                "candidate_identity_mismatch"
            );
        }
        row["lexical_score"] = candidate["lexical_score"].clone();
        let result = crate::citations::resolved_result(self.root.path(), &row, query, tools)?;
        self.verify()?;
        Ok(result)
    }

    pub fn retrieve(&self, query: &str, limit: usize, chapters: &[String]) -> Result<Vec<Value>> {
        self.verify()?;
        let result = self.retrieve_inner(query, limit, chapters)?;
        self.verify()?;
        Ok(result)
    }

    /// Keep each selected textbook's inquiry separate; another document's high
    /// lexical score cannot stand in for missing evidence from this source.
    pub fn retrieve_document(
        &self,
        query: &str,
        limit: usize,
        document: &str,
    ) -> Result<Vec<Value>> {
        ensure!(
            self.kind == "local_pdf",
            "document_scope_requires_local_pdf"
        );
        ensure!(
            self.sources
                .iter()
                .any(|s| s["id"].as_str() == Some(document)),
            "document_not_registered"
        );
        self.verify()?;
        let rows = self.retrieve_scoped_inner(query, limit, &[], &[document.to_owned()])?;
        self.verify()?;
        Ok(rows)
    }

    /// One controlled snapshot and one complete before/after source verification.
    /// Results remain separate so orchestration can retain each pool's provenance.
    pub fn retrieve_plan(&self, requests: &[RetrievalRequest]) -> Result<Vec<Vec<Value>>> {
        self.retrieve_plan_checked(requests, |_| Ok(()))
    }

    fn retrieve_plan_checked(
        &self,
        requests: &[RetrievalRequest],
        mut checkpoint: impl FnMut(usize) -> Result<()>,
    ) -> Result<Vec<Vec<Value>>> {
        ensure!(requests.len() <= 32, "retrieval_plan_limit");
        self.verify()?;
        let mut result = Vec::new();
        let mut bytes = 0;
        for (i, request) in requests.iter().enumerate() {
            let pool = match request {
                RetrievalRequest::Lexical {
                    query,
                    chapters,
                    limit,
                } => self.retrieve_inner(query, *limit, chapters)?,
                RetrievalRequest::Body {
                    anchor,
                    terms,
                    chapters,
                    limit,
                } => self.retrieve_body_inner(anchor, terms, *limit, chapters)?,
            };
            account_rows(&pool, &mut bytes)?;
            result.push(pool);
            checkpoint(i + 1)?;
        }
        self.verify()?;
        Ok(result)
    }

    fn retrieve_inner(&self, query: &str, limit: usize, chapters: &[String]) -> Result<Vec<Value>> {
        self.retrieve_scoped_inner(query, limit, chapters, &[])
    }
    fn retrieve_scoped_inner(
        &self,
        query: &str,
        limit: usize,
        chapters: &[String],
        documents: &[String],
    ) -> Result<Vec<Value>> {
        ensure!((1..=96).contains(&limit), "candidate_limit_invalid");
        ensure!(
            query.len() <= 64 * 1024 && chapters.len() <= 128,
            "query_limit_invalid"
        );
        let expr = match_expression(query);
        let routed: Vec<&Value> = chapters
            .iter()
            .map(|id| {
                self.chapter_rows
                    .iter()
                    .find(|c| c["id"].as_str() == Some(id))
                    .context("route_chapter_not_in_index")
            })
            .collect::<Result<_>>()?;
        let mut routed_pools = Vec::new();
        let mut candidate_bytes = 0usize;
        for chapter in &routed {
            let code = string(chapter, "catalog")?;
            let start = chapter["start_page"]
                .as_i64()
                .context("chapter_range_invalid")?;
            let end = chapter["end_page"]
                .as_i64()
                .context("chapter_range_invalid")?;
            let mut rows = self.index.rows("SELECT *,NULL AS lexical_score FROM pages WHERE catalog=? AND pdf_page BETWEEN ? AND ? ORDER BY native_text_sparse,pdf_page LIMIT ?", &[&code,&start,&end,&(limit as i64)],limit)?;
            account_rows(&rows, &mut candidate_bytes)?;
            for row in &mut rows {
                row["retrieval_pool"] = json!(if self.kind == "local_pdf" {
                    "local_pdf_selected_chapter"
                } else {
                    "misumi_selected_chapter"
                });
            }
            routed_pools.push(rows);
        }
        let mut document_pools = Vec::new();
        let codes: Vec<String> = if self.kind == "local_pdf" {
            self.sources
                .iter()
                .filter(|s| {
                    documents.is_empty()
                        || documents
                            .iter()
                            .any(|d| s["id"].as_str() == Some(d.as_str()))
                })
                .map(|s| format!("local_pdf:{}", s["id"].as_str().unwrap()))
                .collect()
        } else {
            vec![String::new()]
        };
        for code in &codes {
            let catalog_filter = if self.kind == "local_pdf" {
                Some(code.as_str())
            } else {
                None
            };
            let mut lexical = if expr.is_empty() {
                vec![]
            } else {
                self.index.rows("SELECT p.*,bm25(search,8,5,1,0.2) AS lexical_score FROM search JOIN pages p ON p.rowid=search.rowid WHERE search MATCH ? AND (? IS NULL OR p.catalog=?) ORDER BY lexical_score,p.catalog,p.pdf_page LIMIT ?", &[&expr,&catalog_filter,&catalog_filter,&(limit as i64)],limit)?
            };
            account_rows(&lexical, &mut candidate_bytes)?;
            for row in &mut lexical {
                row["retrieval_pool"] = json!(if self.kind == "local_pdf" {
                    "local_pdf_native_or_declared_navigation"
                } else {
                    "misumi_native_or_declared_navigation"
                });
            }
            let mut pools = vec![lexical];
            for (chapter, pool) in routed.iter().zip(&routed_pools) {
                if self.kind == "misumi" || chapter["catalog"].as_str() == Some(code) {
                    pools.push(pool.clone());
                }
            }
            document_pools.push(round_robin(&pools, limit));
        }
        let mut selected = round_robin(
            &routed_pools,
            limit.min(routed_pools.iter().filter(|p| !p.is_empty()).count()),
        );
        let mut seen: BTreeSet<String> = selected
            .iter()
            .filter_map(|p| p["id"].as_str().map(str::to_owned))
            .collect();
        for row in round_robin(&document_pools, limit) {
            if selected.len() == limit {
                break;
            }
            if seen.insert(string(&row, "id")?.to_owned()) {
                selected.push(row);
            }
        }
        let selected = selected
            .into_iter()
            .map(|p| self.decorate(p))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            selected
                .iter()
                .map(|p| p["body"].as_str().map_or(0, str::len))
                .sum::<usize>()
                <= crate::index::MAX_QUERY_BYTES,
            "candidate_text_bytes_limit"
        );
        Ok(selected)
    }

    /// Plain-text inputs only: callers cannot inject SQL or FTS operators.
    /// Match the prior topic/object body pool weights, retaining per-book fairness.
    pub fn retrieve_body(
        &self,
        anchor: &str,
        terms: &str,
        limit: usize,
        chapters: &[String],
    ) -> Result<Vec<Value>> {
        self.verify()?;
        let result = self.retrieve_body_inner(anchor, terms, limit, chapters)?;
        self.verify()?;
        Ok(result)
    }

    fn retrieve_body_inner(
        &self,
        anchor: &str,
        terms: &str,
        limit: usize,
        chapters: &[String],
    ) -> Result<Vec<Value>> {
        ensure!(
            (1..=96).contains(&limit)
                && anchor.len() <= 64 * 1024
                && terms.len() <= 64 * 1024
                && chapters.len() <= 128,
            "query_limit_invalid"
        );
        for id in chapters {
            ensure!(
                self.chapter_rows
                    .iter()
                    .any(|c| c["id"].as_str() == Some(id)),
                "route_chapter_not_in_index"
            );
        }
        let anchor = match_expression(anchor);
        let terms = match_expression(terms);
        ensure!(
            !anchor.is_empty() || !terms.is_empty(),
            "body_query_no_search_terms"
        );
        let expression = match (anchor.is_empty(), terms.is_empty()) {
            (false, false) => format!("body: ({anchor}) AND body: ({terms})"),
            (false, true) => format!("body: ({anchor})"),
            _ => format!("body: ({terms})"),
        };
        let codes: Vec<Option<String>> = if self.kind == "local_pdf" {
            self.sources
                .iter()
                .map(|s| Some(format!("local_pdf:{}", s["id"].as_str().unwrap())))
                .collect()
        } else {
            vec![None]
        };
        let mut pools = Vec::new();
        let mut total = 0;
        for code in codes {
            let mut sql="SELECT p.*,bm25(search,0.2,2,6,0.1) AS lexical_score FROM search JOIN pages p ON p.rowid=search.rowid WHERE search MATCH ?".to_owned();
            let mut values = vec![rusqlite::types::Value::Text(expression.clone())];
            if let Some(code) = code {
                sql.push_str(" AND p.catalog=?");
                values.push(code.into());
            }
            if !chapters.is_empty() {
                sql.push_str(" AND p.chapter_id IN (");
                sql.push_str(&vec!["?"; chapters.len()].join(","));
                sql.push(')');
                values.extend(chapters.iter().cloned().map(rusqlite::types::Value::Text));
            }
            sql.push_str(" ORDER BY lexical_score,p.catalog,p.pdf_page LIMIT ?");
            values.push((limit as i64).into());
            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            let rows = self.index.rows(&sql, &params, limit)?;
            account_rows(&rows, &mut total)?;
            pools.push(rows);
        }
        let selected = round_robin(&pools, limit)
            .into_iter()
            .map(|p| self.decorate(p))
            .collect::<Result<Vec<_>>>()?;
        Ok(selected)
    }

    fn decorate(&self, mut page: Value) -> Result<Value> {
        let body = string_allow_empty(&page, "body")?;
        ensure!(
            page["text_sha256"].as_str() == Some(oe_core::sha256(body).as_str())
                && page["text_chars"].as_u64() == Some(body.chars().count() as u64),
            "page_text_identity_mismatch"
        );
        let missing = body.trim().is_empty();
        let catalog = string(&page, "catalog")?.to_owned();
        let physical_page = page["pdf_page"]
            .as_u64()
            .filter(|p| *p > 0)
            .context("page_position_invalid")?;
        ensure!(
            valid_sha(string(&page, "source_sha256")?)
                && page["source_bytes"].as_u64().is_some_and(|n| n > 0),
            "page_source_identity_invalid"
        );
        let titles = strict_json(string(&page, "titles_json")?.as_bytes())?;
        let series = strict_json(string(&page, "series_json")?.as_bytes())?;
        ensure!(
            titles.is_array() && series.is_array(),
            "page_navigation_invalid"
        );
        let source_data = strict_json(string(&page, "source_json")?.as_bytes())?;
        ensure!(source_data.is_object(), "page_source_json_invalid");
        let chapter = self
            .chapter_rows
            .iter()
            .find(|c| c["id"] == page["chapter_id"]);
        page["chapter"] = chapter.map(|c| c["title"].clone()).unwrap_or_else(|| {
            json!(if self.kind == "local_pdf" {
                "未登记章节"
            } else {
                "封面及前附"
            })
        });
        page["headings"] = json!(
            titles
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["text"].clone())
                .collect::<Vec<_>>()
        );
        page["series_references"] = series;
        page["provider"] = json!(self.kind);
        page["source_kind"] = json!(self.kind);
        page["index_identity"] = self.index.identity();
        page["original_verified"] = json!(false);
        page["native_text_missing"] = json!(missing);
        page["navigation_only"] = json!(missing);
        if self.kind == "local_pdf" {
            let source = self
                .sources
                .iter()
                .find(|s| catalog == format!("local_pdf:{}", s["id"].as_str().unwrap()))
                .context("page_source_not_registered")?;
            ensure!(
                physical_page <= source["page_count"].as_u64().unwrap()
                    && page["source_sha256"] == source["sha256"]
                    && page["source_bytes"] == source["bytes"]
                    && source_data["document_id"] == source["id"],
                "page_registration_identity_mismatch"
            );
            page["source_registration"] = source.clone();
            // Navigation is returned once by chapters(), not copied into every
            // selected page. Keep the registered source identity fields here.
            page["source_registration"]
                .as_object_mut()
                .expect("validated source")
                .remove("chapters");
            page["source_pdf"] = json!(resolve_original(
                &self.root,
                &self.directory,
                string(source, "path")?
            )?);
            page["source_file_page"] = json!(physical_page);
            page["catalog_title"] = source["title"].clone();
            page["text_origin"] = json!("poppler_native_text");
            page["source_locator"] = json!({"schema":"ontology-engineering.source-page/v1","kind":"local_pdf","document_id":source["id"],"document_sha256":source["sha256"],"physical_page":physical_page,"page_count":source["page_count"],"title":source["title"],"edition":source["edition"],"printed_page":null,"printed_page_status":"unverified"});
        } else {
            let catalog_row = self
                .catalogs
                .get(&catalog)
                .context("page_catalog_not_registered")?;
            ensure!(
                catalog_row["page_count"]
                    .as_u64()
                    .is_some_and(|n| physical_page <= n),
                "page_position_invalid"
            );
            safe_relative(string(&page, "source_relative_path")?)?;
            page["catalog_registration"] = catalog_row.clone();
            page["catalog_title"] = catalog_row["label"].clone();
            if let Some((manifest, _)) = &self.source_manifest {
                page["source_pack_manifest"] = json!(manifest);
            } else {
                page["source_archive_root"] = json!(
                    self.archive_root
                        .as_ref()
                        .context("misumi_source_archive_missing")?
                );
            }
            page["source_file_page"] = json!(1);
            page["text_origin"] = self
                .index
                .meta
                .get("text_origin")
                .cloned()
                .unwrap_or(json!("pymupdf_native_text"));
            page["source_locator"] = json!({"schema":"ontology-engineering.source-page/v1","kind":"misumi","catalog":catalog,"physical_page":physical_page,"page_count":catalog_row["page_count"],"source_sha256":page["source_sha256"],"source_relative_path":page["source_relative_path"],"source_file_page":1,"printed_page":page["printed_page"],"printed_page_status":"supplier_declared_unverified"});
        }
        Ok(page)
    }
}

fn round_robin(pools: &[Vec<Value>], limit: usize) -> Vec<Value> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for offset in 0..limit {
        for pool in pools {
            if let Some(row) = pool.get(offset)
                && seen.insert(row["id"].clone().to_string())
            {
                result.push(row.clone());
                if result.len() == limit {
                    return result;
                }
            }
        }
    }
    result
}

fn account_rows(rows: &[Value], total: &mut usize) -> Result<()> {
    for row in rows {
        for value in row.as_object().context("page_not_object")?.values() {
            if let Some(text) = value.as_str() {
                *total = total
                    .checked_add(text.len())
                    .context("candidate_text_bytes_limit")?;
            }
        }
    }
    ensure!(
        *total <= crate::index::MAX_QUERY_BYTES,
        "candidate_text_bytes_limit"
    );
    Ok(())
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let s = string_allow_empty(value, key)?;
    ensure!(!s.is_empty(), "index_empty_required_string");
    Ok(s)
}
fn string_allow_empty<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().context("index_expected_string")
}
fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-'))
}
fn safe_relative(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && !s.contains('\\')
            && !s.chars().any(char::is_control)
            && Path::new(s)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            && !s.contains("//")
            && !s.ends_with('/'),
        "invalid_source_relative_path"
    );
    Ok(())
}

pub(crate) fn resolve_original(root: &SkillRoot, directory: &Path, value: &str) -> Result<PathBuf> {
    let result = resolve_registered(root, directory, value)?;
    ensure!(
        !result.starts_with(root.path().join("sources/.indexes")),
        "original_outside_source_root"
    );
    Ok(result)
}

fn resolve_registered(root: &SkillRoot, directory: &Path, value: &str) -> Result<PathBuf> {
    ensure!(
        !value.is_empty() && !value.contains('\\') && !value.chars().any(char::is_control),
        "invalid_original_path"
    );
    let input = Path::new(value);
    let input = if input.is_absolute() {
        input.to_path_buf()
    } else {
        directory.join(input)
    };
    let mut result = PathBuf::new();
    for part in input.components() {
        match part {
            Component::ParentDir => {
                ensure!(result.pop(), "invalid_original_path");
            }
            Component::CurDir => {}
            other => {
                result.push(other.as_os_str());
                no_symlinks(&result)?;
            }
        }
    }
    ensure!(
        result.starts_with(root.path().join("sources")) && result != root.path().join("sources"),
        "original_outside_source_root"
    );
    no_symlinks(&result)?;
    Ok(result)
}

#[cfg(test)]
mod plan_boundary_tests {
    use super::*;
    use rusqlite::{Connection, params};
    use std::fs;

    #[test]
    fn changes_between_pools_or_after_last_pool_cannot_escape_final_verification() {
        for change_after in [1, 2] {
            let directory=Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join(format!("var/maintenance/rust-rewrite-20260927/phase5-provider/plan-boundary-{}-{change_after}",std::process::id()));
            fs::create_dir_all(directory.join("sources/.indexes/local-pdf")).unwrap();
            fs::create_dir_all(directory.join("sources/books")).unwrap();
            fs::write(directory.join("SKILL.md"), "fixture").unwrap();
            fs::write(directory.join("sources/books/test.pdf"), "source").unwrap();
            let root = SkillRoot::new(&directory).unwrap();
            let index = directory.join("sources/.indexes/local-pdf");
            let mut registration = json!({"schema":"ontology-engineering.local-pdf-registration/v1","sources":[{"id":"test","title":"Test","edition":"fixture","path":"../../books/test.pdf","sha256":oe_core::sha256("source"),"bytes":6,"page_count":1,"chapters":[]}]});
            let db = Connection::open(index.join("catalog.sqlite3")).unwrap();
            db.execute_batch(crate::registration::TABLES).unwrap();
            db.execute(
                "INSERT INTO pages VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    "local_pdf:test:page:1",
                    "local_pdf:test",
                    1,
                    1,
                    Option::<String>::None,
                    Option::<String>::None,
                    "",
                    "",
                    oe_core::sha256("source"),
                    6,
                    "shaft",
                    oe_core::sha256("shaft"),
                    oe_core::sha256("shaft"),
                    5,
                    5,
                    1,
                    "[]",
                    "[]",
                    0,
                    "{\"document_id\":\"test\"}"
                ],
            )
            .unwrap();
            db.execute(
                "INSERT INTO search(rowid,models,titles,body,chapter) VALUES(1,'','','shaft','')",
                [],
            )
            .unwrap();
            let mut meta = json!({"schema":"ontology-engineering.local-pdf-index/v1","registration_sha256":canonical_digest(&registration).unwrap()});
            meta["fingerprint"] = json!(canonical_digest(&meta).unwrap());
            for (k, v) in meta.as_object().unwrap() {
                db.execute("INSERT INTO meta VALUES(?,?)", params![k, v.to_string()])
                    .unwrap();
            }
            drop(db);
            registration["index_sha256"] = json!(
                fingerprint_file(&index.join("catalog.sqlite3"), u64::MAX)
                    .unwrap()
                    .sha256
            );
            fs::write(index.join("registration.json"), registration.to_string()).unwrap();
            let provider = Provider::open(&root, "local_pdf", None).unwrap();
            let scoped = provider.retrieve_document("shaft", 1, "test").unwrap();
            assert_eq!(scoped.len(), 1);
            assert_eq!(scoped[0]["catalog"], "local_pdf:test");
            assert!(
                provider
                    .retrieve_document("shaft", 1, "missing")
                    .unwrap_err()
                    .to_string()
                    .contains("document_not_registered")
            );
            let requests = vec![
                RetrievalRequest::Lexical {
                    query: "shaft".into(),
                    chapters: vec![],
                    limit: 1
                };
                2
            ];
            let error = provider
                .retrieve_plan_checked(&requests, |completed| {
                    if completed == change_after {
                        fs::write(index.join("registration.json"), "changed during batch")?;
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(error.to_string().contains("index_registration_changed"));
            drop(provider);
            fs::remove_dir_all(directory).unwrap();
        }
    }
}
