//! Bounded, read-only snapshots of the existing registered SQLite indexes.
use anyhow::{Context, Result, ensure};
use oe_core::json::strict_json;
use oe_core::{
    FileIdentity, SkillRoot, create_new_private_dir, create_new_regular_file, fingerprint_file,
    no_symlinks, open_regular_file,
};
use rusqlite::{Connection, OpenFlags, config::DbConfig, limits::Limit, types::ValueRef};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_INDEX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_VALUE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_QUERY_BYTES: usize = 32 * 1024 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(0);

pub struct ReadIndex {
    db: Option<Connection>,
    source: PathBuf,
    snapshot: PathBuf,
    identity: FileIdentity,
    pub meta: Value,
}

struct PendingSnapshot {
    directory: PathBuf,
    armed: bool,
}
impl Drop for PendingSnapshot {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(self.directory.join("catalog.sqlite3"));
            let _ = fs::remove_dir(&self.directory);
        }
    }
}

impl ReadIndex {
    pub fn open(root: &SkillRoot, directory: &Path, kind: &str) -> Result<Self> {
        ensure!(matches!(kind, "local_pdf" | "misumi"), "unknown_provider");
        let directory = root.expand(directory)?;
        let indexes = root.path().join("sources/.indexes");
        ensure!(
            directory.starts_with(&indexes) && directory != indexes,
            "index_outside_registered_sources"
        );
        no_symlinks(&directory)?;
        Self::open_source(root, &directory.join("catalog.sqlite3"), kind)
    }

    /// Internal source operations also verify staged or separately registered
    /// indexes. They use the same snapshot, schema and identity boundary.
    pub(crate) fn open_source(root: &SkillRoot, path: &Path, kind: &str) -> Result<Self> {
        ensure!(matches!(kind, "local_pdf" | "misumi"), "unknown_provider");
        let source = root.expand(path)?;
        ensure!(
            source.starts_with(root.path().join("sources"))
                || source.starts_with(root.path().join("var")),
            "index_outside_skill_sources_or_staging"
        );
        no_symlinks(&source)?;
        reject_sidecars(&source)?;
        let mut input = open_regular_file(&source, MAX_INDEX_BYTES)?;
        let before = input.metadata()?;
        ensure!(before.len() >= 100, "index_header_missing");
        // Each open owns a fresh directory; no unverified cache file is adopted.
        let name = format!(
            "snapshot-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let directory =
            root.private_path(&PathBuf::from("var/cache/knowledge/indexes").join(name))?;
        create_new_private_dir(&directory)?;
        let mut pending = PendingSnapshot {
            directory,
            armed: true,
        };
        let snapshot = pending.directory.join("catalog.sqlite3");
        let mut output = create_new_regular_file(&snapshot)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut bytes = 0u64;
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes = bytes.checked_add(n as u64).context("index_size_limit")?;
            ensure!(bytes <= MAX_INDEX_BYTES, "index_size_limit");
            hasher.update(&buffer[..n]);
            output.write_all(&buffer[..n])?;
        }
        output.sync_all()?;
        drop(output);
        let after = input.metadata()?;
        ensure!(
            bytes == before.len()
                && bytes == after.len()
                && before.modified()? == after.modified()?,
            "index_changed_during_copy"
        );
        let identity = FileIdentity {
            bytes,
            sha256: format!("{:x}", hasher.finalize()),
        };
        ensure!(
            fingerprint_file(&source, MAX_INDEX_BYTES)? == identity,
            "index_changed_during_copy"
        );
        reject_sidecars(&source)?;
        no_symlinks(&snapshot)?;
        let db = Connection::open_with_flags(
            &snapshot,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.set_limit(Limit::SQLITE_LIMIT_LENGTH, MAX_VALUE_BYTES as i32);
        db.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 64 * 1024);
        db.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0);
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)?;
        // Extension APIs are never exposed. Explicitly disable loading as well.
        let rc = unsafe { rusqlite::ffi::sqlite3_enable_load_extension(db.handle(), 0) };
        ensure!(
            rc == rusqlite::ffi::SQLITE_OK,
            "sqlite_extension_disable_failed"
        );
        db.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA mmap_size=0;")?;
        validate_schema(&db, kind)?;
        let mut meta = Map::new();
        let mut statement = db.prepare("SELECT key,value FROM meta LIMIT 129")?;
        let mut rows = statement.query([])?;
        let mut total = 0;
        while let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            let raw: String = row.get(1)?;
            total += key.len() + raw.len();
            ensure!(
                meta.len() < 128 && key.len() < 256 && total <= MAX_VALUE_BYTES,
                "index_metadata_limit"
            );
            ensure!(
                meta.insert(key, strict_json(raw.as_bytes())?).is_none(),
                "index_duplicate_metadata"
            );
        }
        drop(rows);
        drop(statement);
        let expected = match kind {
            "local_pdf" => "ontology-engineering.local-pdf-index/v1",
            "misumi" => "misumi.catalog-index/v1",
            _ => anyhow::bail!("unknown_provider"),
        };
        ensure!(
            meta.get("schema").and_then(Value::as_str) == Some(expected),
            "index_schema_mismatch"
        );
        let mut unsigned = meta.clone();
        let fingerprint = unsigned
            .remove("fingerprint")
            .context("index_fingerprint_missing")?;
        ensure!(
            fingerprint == oe_core::json::canonical_digest(&Value::Object(unsigned))?,
            "index_fingerprint_mismatch"
        );
        ensure!(
            fingerprint_file(&snapshot, MAX_INDEX_BYTES)? == identity,
            "snapshot_identity_mismatch"
        );
        // Disarm temporary cleanup only once every check passed.
        pending.armed = false;
        Ok(Self {
            db: Some(db),
            source,
            snapshot,
            identity,
            meta: Value::Object(meta),
        })
    }

    pub fn identity(&self) -> Value {
        json!({"sha256":self.identity.sha256,"bytes":self.identity.bytes})
    }
    pub fn verify(&self) -> Result<()> {
        reject_sidecars(&self.source)?;
        ensure!(
            fingerprint_file(&self.source, MAX_INDEX_BYTES)? == self.identity,
            "index_source_changed"
        );
        ensure!(
            fingerprint_file(&self.snapshot, MAX_INDEX_BYTES)? == self.identity,
            "index_snapshot_changed"
        );
        Ok(())
    }

    pub(crate) fn rows(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        limit: usize,
    ) -> Result<Vec<Value>> {
        let mut statement = self.db.as_ref().context("index_closed")?.prepare(sql)?;
        let names: Vec<String> = statement
            .column_names()
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let mut rows = statement.query(params)?;
        let mut result = Vec::new();
        let mut total = 0;
        while let Some(row) = rows.next()? {
            ensure!(result.len() < limit, "index_result_count_limit");
            let mut object = Map::new();
            for (i, name) in names.iter().enumerate() {
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => {
                        ensure!(n.is_finite(), "index_nonfinite_value");
                        json!(n)
                    }
                    ValueRef::Text(raw) => {
                        total += raw.len();
                        ensure!(total <= MAX_QUERY_BYTES, "index_result_bytes_limit");
                        Value::String(
                            std::str::from_utf8(raw)
                                .context("index_invalid_utf8")?
                                .to_owned(),
                        )
                    }
                    ValueRef::Blob(_) => anyhow::bail!("index_unexpected_blob"),
                };
                object.insert(name.clone(), value);
            }
            result.push(Value::Object(object));
        }
        Ok(result)
    }
}

fn reject_sidecars(source: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = source.as_os_str().to_os_string();
        name.push(suffix);
        ensure!(
            matches!(fs::symlink_metadata(PathBuf::from(name)), Err(error) if error.kind()==std::io::ErrorKind::NotFound),
            "index_transaction_sidecar_present"
        );
    }
    Ok(())
}

impl Drop for ReadIndex {
    fn drop(&mut self) {
        // Close SQLite first so Windows can unlink the owned snapshot as well.
        if let Some(db) = self.db.take() {
            let _ = db.close();
        }
        // Unlink only this instance's two known paths. Never traverse a cache tree.
        let _ = fs::remove_file(&self.snapshot);
        if let Some(parent) = self.snapshot.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

fn normalized_sql(sql: &str) -> String {
    sql.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\'' | '"' | '`' | '[' | ']' | ';'))
        .flat_map(char::to_lowercase)
        .collect()
}

fn validate_schema(db: &Connection, kind: &str) -> Result<()> {
    let mut expected: BTreeMap<&str, String> = BTreeMap::new();
    for (name, sql) in [
        (
            "meta",
            "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
        ),
        (
            "chapters",
            "CREATE TABLE chapters(id TEXT PRIMARY KEY,catalog TEXT,title TEXT,start_page INTEGER,end_page INTEGER,source_json TEXT)",
        ),
        (
            "pages",
            "CREATE TABLE pages(id TEXT PRIMARY KEY,catalog TEXT,page_id INTEGER,pdf_page INTEGER,printed_page TEXT,chapter_id TEXT,source_relative_path TEXT,source_url TEXT,source_sha256 TEXT,source_bytes INTEGER,body TEXT,text_sha256 TEXT,raw_text_sha256 TEXT,raw_text_chars INTEGER,text_chars INTEGER,native_text_sparse INTEGER,titles_json TEXT,series_json TEXT,series_count INTEGER,source_json TEXT)",
        ),
        (
            "search",
            "CREATE VIRTUAL TABLE search USING fts5(models,titles,body,chapter)",
        ),
        (
            "search_data",
            "CREATE TABLE search_data(id INTEGER PRIMARY KEY, block BLOB)",
        ),
        (
            "search_idx",
            "CREATE TABLE search_idx(segid, term, pgno, PRIMARY KEY(segid, term)) WITHOUT ROWID",
        ),
        (
            "search_content",
            "CREATE TABLE search_content(id INTEGER PRIMARY KEY, c0, c1, c2, c3)",
        ),
        (
            "search_docsize",
            "CREATE TABLE search_docsize(id INTEGER PRIMARY KEY, sz BLOB)",
        ),
        (
            "search_config",
            "CREATE TABLE search_config(k PRIMARY KEY, v) WITHOUT ROWID",
        ),
        (
            "page_chapter",
            "CREATE INDEX page_chapter ON pages(chapter_id,pdf_page)",
        ),
        (
            "page_position",
            "CREATE INDEX page_position ON pages(catalog,pdf_page)",
        ),
    ] {
        expected.insert(name, normalized_sql(sql));
    }
    if kind == "misumi" {
        for (name, sql) in [
            (
                "catalogs",
                "CREATE TABLE catalogs(code TEXT PRIMARY KEY,label TEXT,title TEXT,page_count INTEGER,pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT)",
            ),
            (
                "hotspots",
                "CREATE TABLE hotspots(page_id TEXT,ordinal INTEGER,kind TEXT,keyword TEXT,keyword_normalized TEXT,target TEXT,source_json TEXT,PRIMARY KEY(page_id,ordinal))",
            ),
            (
                "hotspot_model",
                "CREATE INDEX hotspot_model ON hotspots(keyword_normalized,kind)",
            ),
        ] {
            expected.insert(name, normalized_sql(sql));
        }
    }
    let mut statement = db.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema LIMIT 65")?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        ensure!(count <= 64, "index_schema_limit");
        let kind: String = row.get(0)?;
        let name: String = row.get(1)?;
        let table: String = row.get(2)?;
        let sql: Option<String> = row.get(3)?;
        if let Some(sql) = sql {
            ensure!(
                matches!(kind.as_str(), "table" | "index"),
                "index_unsafe_schema_object"
            );
            let wanted = expected
                .remove(name.as_str())
                .context("index_unexpected_schema_object")?;
            ensure!(
                normalized_sql(&sql) == wanted,
                "index_table_schema_mismatch"
            );
        } else {
            ensure!(
                kind == "index"
                    && ["meta", "chapters", "pages", "catalogs", "hotspots"]
                        .contains(&table.as_str())
                    && name == format!("sqlite_autoindex_{table}_1"),
                "index_unexpected_autoindex"
            );
        }
    }
    ensure!(expected.is_empty(), "index_schema_incomplete");
    Ok(())
}
