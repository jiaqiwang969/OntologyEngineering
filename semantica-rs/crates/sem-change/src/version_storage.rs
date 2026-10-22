//! Version Storage Module (`semantica/change_management/version_storage.py`).
//!
//! Storage backends for persistent version management:
//!
//! - [`VersionStorage`]: the storage contract (snapshots, tags, mutation audit trail);
//! - [`InMemoryVersionStorage`]: map-based storage, lost when the process ends;
//! - [`SQLiteVersionStorage`]: SQLite file storage with the original schema, so databases are
//!   interchangeable with the Python implementation;
//! - [`compute_checksum`] / [`verify_checksum`]: SHA-256 over Python's canonical
//!   `json.dumps(data, sort_keys=True, separators=(",", ":"))` text, so checksums match the
//!   original byte for byte;
//! - [`create_graph_snapshot_record`]: a checksummed metadata record for a named graph.
//!
//! Snapshots, mutations and records are free-form dicts and stay `serde_json` maps. All
//! storage methods take `&self` and are thread-safe (each backend serialises its operations
//! with a lock, like the original `threading.RLock`).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use indexmap::IndexMap;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params_from_iter};
use sem_utils::logging::{LogLevel, Logger};
use sem_utils::pycompat::fmt::{py_str, truthy, type_name};
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{ProcessingError, SemanticaError, ValidationError, get_logger};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::pycompat::{get, json_dumps_canonical, json_dumps_default, json_dumps_map_default};
use crate::pycompat::{json_loads, py_list_len};

/// `len(entities), len(relationships)` of `_snapshot_collections(snapshot)`: the entity and
/// relationship collections with the legacy (`entities`/`relationships`) keys preferred and
/// the node/edge keys as fallback.
fn snapshot_collection_counts(
    snapshot: &Map<String, Value>,
) -> Result<(usize, usize), SemanticaError> {
    let count = |primary: &str, fallback: &str| -> Result<usize, SemanticaError> {
        let mut items = get(snapshot, primary);
        if items.is_null() {
            items = get(snapshot, fallback);
        }
        if truthy(items) {
            py_list_len(items)
        } else {
            Ok(0)
        }
    };
    Ok((
        count("entities", "nodes")?,
        count("relationships", "edges")?,
    ))
}

/// The metadata row `list_all` returns for a snapshot (without the graph data).
fn snapshot_metadata(snapshot: &Map<String, Value>) -> Result<Map<String, Value>, SemanticaError> {
    let (entity_count, relationship_count) = snapshot_collection_counts(snapshot)?;
    let label = get(snapshot, "label").clone();
    let version_id = snapshot
        .get("version_id")
        .cloned()
        .unwrap_or_else(|| label.clone());
    let mut metadata = Map::new();
    metadata.insert("label".into(), label);
    metadata.insert("version_id".into(), version_id);
    for key in [
        "graph_uri",
        "timestamp",
        "author",
        "description",
        "checksum",
    ] {
        metadata.insert(key.into(), get(snapshot, key).clone());
    }
    metadata.insert("entity_count".into(), Value::from(entity_count));
    metadata.insert("relationship_count".into(), Value::from(relationship_count));
    Ok(metadata)
}

/// `create_graph_snapshot_record(version_id, graph_uri, author="system",
/// description="Graph snapshot", metadata=None)`: a standardized, checksummed snapshot
/// metadata record for a named graph.
pub fn create_graph_snapshot_record(
    version_id: &str,
    graph_uri: &str,
    author: Option<&str>,
    description: Option<&str>,
    metadata: Option<Map<String, Value>>,
) -> Map<String, Value> {
    let mut record = Map::new();
    record.insert("label".into(), Value::String(version_id.into()));
    record.insert("version_id".into(), Value::String(version_id.into()));
    record.insert("graph_uri".into(), Value::String(graph_uri.into()));
    record.insert(
        "timestamp".into(),
        Value::String(PyDateTime::now(false).isoformat('T')),
    );
    record.insert(
        "author".into(),
        Value::String(author.unwrap_or("system").into()),
    );
    record.insert(
        "description".into(),
        Value::String(description.unwrap_or("Graph snapshot").into()),
    );
    record.insert(
        "metadata".into(),
        Value::Object(metadata.unwrap_or_default()),
    );
    let checksum = compute_checksum(&record);
    record.insert("checksum".into(), Value::String(checksum));
    record
}

/// The storage contract every version backend implements (`class VersionStorage(ABC)`).
///
/// Labels, tags and entity ids are strings. Failures carry the original exception kinds:
/// `ValidationError` for invalid snapshots, `ProcessingError` for database failures.
pub trait VersionStorage: Send + Sync {
    /// Save a version snapshot (`ValidationError` without a `label` or for a duplicate label).
    fn save(&self, snapshot: &Map<String, Value>) -> Result<(), SemanticaError>;

    /// Retrieve a version snapshot by label (a copy; `None` if absent).
    fn get(&self, label: &str) -> Result<Option<Map<String, Value>>, SemanticaError>;

    /// List all version snapshots as metadata rows (`label`, `version_id`, `graph_uri`,
    /// `timestamp`, `author`, `description`, `checksum`, `entity_count`,
    /// `relationship_count`).
    fn list_all(&self) -> Result<Vec<Map<String, Value>>, SemanticaError>;

    /// Check if a version exists.
    fn exists(&self, label: &str) -> Result<bool, SemanticaError>;

    /// Delete a version snapshot; `true` if one was deleted.
    fn delete(&self, label: &str) -> Result<bool, SemanticaError>;

    /// Save a named tag pointing to a specific version.
    fn save_tag(&self, tag_name: &str, version_label: &str) -> Result<(), SemanticaError>;

    /// Retrieve the version label associated with a tag.
    fn get_tag(&self, tag_name: &str) -> Result<Option<String>, SemanticaError>;

    /// List all tags as a mapping of tag name to version label.
    fn list_tags(&self) -> Result<IndexMap<String, String>, SemanticaError>;

    /// Save a granular mutation record for the audit trail.
    fn save_mutation(&self, mutation: &Map<String, Value>) -> Result<(), SemanticaError>;

    /// Retrieve the chronological mutation history for a specific entity.
    fn get_entity_history(
        &self,
        entity_id: &str,
    ) -> Result<Vec<Map<String, Value>>, SemanticaError>;

    /// Attach a version label to the unlabeled mutations recorded since the last snapshot.
    fn assign_version_to_unlabeled_mutations(
        &self,
        version_label: &str,
    ) -> Result<(), SemanticaError>;
}

/// The key a snapshot's `label` field takes in storage: the string itself, or `str(label)` for
/// other truthy scalars (see PORT-NOTES.md); lists and dicts are unhashable.
fn memory_label(label: &Value) -> Result<String, SemanticaError> {
    match label {
        Value::String(s) => Ok(s.clone()),
        Value::Array(_) | Value::Object(_) => Err(SemanticaError::Type(format!(
            "unhashable type: '{}'",
            type_name(label)
        ))),
        other => Ok(py_str(other)),
    }
}

fn missing_label() -> SemanticaError {
    ValidationError::new("Snapshot must have a 'label' field").into()
}

fn duplicate_label(label: &Value) -> SemanticaError {
    ValidationError::new(format!("Version '{}' already exists", py_str(label))).into()
}

/// A deep copy through JSON text: the original's `json.loads(json.dumps(value))`. For JSON
/// values the round trip equals a clone.
fn deep_copy(map: &Map<String, Value>) -> Map<String, Value> {
    match json_loads(&json_dumps_map_default(map)) {
        Ok(Value::Object(copy)) => copy,
        _ => map.clone(),
    }
}

/// A snapshot held by [`InMemoryVersionStorage`].
///
/// The original stores `json.loads(json.dumps(snapshot))` and returns
/// `json.loads(json.dumps(stored))`; this keeps the `json.dumps(snapshot)` text instead (an
/// isolated, immutable copy) and decodes it on `get`, which yields the same values with one
/// conversion per call. The `list_all` row is a function of that immutable copy, so it is
/// computed once (an error computing it is raised by `list_all`, as in the original).
#[derive(Debug)]
struct StoredSnapshot {
    text: String,
    is_empty: bool,
    metadata: Result<Map<String, Value>, SemanticaError>,
}

#[derive(Debug, Default)]
struct InMemoryState {
    storage: IndexMap<String, StoredSnapshot>,
    tags: IndexMap<String, String>,
    mutations: Vec<Map<String, Value>>,
}

/// In-memory version storage (`class InMemoryVersionStorage(VersionStorage)`): snapshots in
/// insertion order, deep-copied (through JSON text, like the original) on the way in and out.
/// Data is lost when the process ends.
#[derive(Debug)]
pub struct InMemoryVersionStorage {
    state: Mutex<InMemoryState>,
    /// `self.logger` (`semantica.in_memory_storage`).
    pub logger: Logger,
}

impl Default for InMemoryVersionStorage {
    fn default() -> Self {
        InMemoryVersionStorage::new()
    }
}

impl InMemoryVersionStorage {
    /// `InMemoryVersionStorage()`.
    pub fn new() -> InMemoryVersionStorage {
        InMemoryVersionStorage {
            state: Mutex::new(InMemoryState::default()),
            logger: get_logger("in_memory_storage"),
        }
    }

    fn lock(&self) -> MutexGuard<'_, InMemoryState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl VersionStorage for InMemoryVersionStorage {
    /// Save snapshot to memory.
    fn save(&self, snapshot: &Map<String, Value>) -> Result<(), SemanticaError> {
        let label = get(snapshot, "label");
        if !truthy(label) {
            return Err(missing_label());
        }
        let mut state = self.lock();
        let key = memory_label(label)?;
        if state.storage.contains_key(&key) {
            return Err(duplicate_label(label));
        }
        // Deep copy to prevent external modifications.
        let stored = StoredSnapshot {
            text: json_dumps_map_default(snapshot),
            is_empty: snapshot.is_empty(),
            metadata: snapshot_metadata(snapshot),
        };
        state.storage.insert(key, stored);
        if self.logger.is_enabled_for(LogLevel::DEBUG) {
            self.logger
                .debug(format!("Saved version '{}' to memory", py_str(label)));
        }
        Ok(())
    }

    /// Retrieve snapshot from memory (a deep copy).
    fn get(&self, label: &str) -> Result<Option<Map<String, Value>>, SemanticaError> {
        let state = self.lock();
        match state.storage.get(label) {
            Some(stored) if !stored.is_empty => match json_loads(&stored.text)? {
                Value::Object(snapshot) => Ok(Some(snapshot)),
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// List all snapshots in memory (metadata only, insertion order).
    fn list_all(&self) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        let state = self.lock();
        state
            .storage
            .values()
            .map(|stored| stored.metadata.clone())
            .collect()
    }

    /// Check if version exists in memory.
    fn exists(&self, label: &str) -> Result<bool, SemanticaError> {
        Ok(self.lock().storage.contains_key(label))
    }

    /// Delete version from memory.
    fn delete(&self, label: &str) -> Result<bool, SemanticaError> {
        let mut state = self.lock();
        if state.storage.shift_remove(label).is_some() {
            self.logger
                .debug(format!("Deleted version '{label}' from memory"));
            return Ok(true);
        }
        Ok(false)
    }

    fn save_tag(&self, tag_name: &str, version_label: &str) -> Result<(), SemanticaError> {
        self.lock()
            .tags
            .insert(tag_name.to_string(), version_label.to_string());
        Ok(())
    }

    fn get_tag(&self, tag_name: &str) -> Result<Option<String>, SemanticaError> {
        Ok(self.lock().tags.get(tag_name).cloned())
    }

    fn list_tags(&self) -> Result<IndexMap<String, String>, SemanticaError> {
        Ok(self.lock().tags.clone())
    }

    fn save_mutation(&self, mutation: &Map<String, Value>) -> Result<(), SemanticaError> {
        self.lock().mutations.push(deep_copy(mutation));
        Ok(())
    }

    fn get_entity_history(
        &self,
        entity_id: &str,
    ) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        Ok(self
            .lock()
            .mutations
            .iter()
            .filter(|m| matches!(m.get("entity_id"), Some(Value::String(id)) if id == entity_id))
            .cloned()
            .collect())
    }

    fn assign_version_to_unlabeled_mutations(
        &self,
        version_label: &str,
    ) -> Result<(), SemanticaError> {
        let mut state = self.lock();
        for mutation in &mut state.mutations {
            if get(mutation, "version_label").is_null() {
                mutation.insert(
                    "version_label".into(),
                    Value::String(version_label.to_string()),
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// SQLite
// ---------------------------------------------------------------------------------------------

// The statements are the original's, byte for byte (the CREATE TABLE text is what SQLite stores
// in `sqlite_master`).
const CREATE_VERSIONS: &str = "\n                    CREATE TABLE IF NOT EXISTS versions (\n                        label TEXT PRIMARY KEY,\n                        timestamp TEXT NOT NULL,\n                        author TEXT NOT NULL,\n                        description TEXT NOT NULL,\n                        checksum TEXT NOT NULL,\n                        snapshot_data TEXT NOT NULL,\n                        created_at TEXT NOT NULL\n                    )\n                ";
const CREATE_VERSION_TAGS: &str = "\n                    CREATE TABLE IF NOT EXISTS version_tags (\n                        tag_name TEXT PRIMARY KEY,\n                        version_label TEXT NOT NULL\n                    )\n                ";
const CREATE_MUTATION_LOG: &str = "\n                    CREATE TABLE IF NOT EXISTS mutation_log (\n                        id INTEGER PRIMARY KEY AUTOINCREMENT,\n                        timestamp TEXT,\n                        operation TEXT,\n                        entity_id TEXT,\n                        payload TEXT,\n                        version_label TEXT\n                    )\n                ";
const SELECT_LABEL: &str = "SELECT label FROM versions WHERE label = ?";
const INSERT_VERSION: &str = "\n                    INSERT INTO versions \n                    (label, timestamp, author, description, checksum, snapshot_data, created_at)\n                    VALUES (?, ?, ?, ?, ?, ?, ?)\n                ";
const SELECT_SNAPSHOT: &str =
    "\n                    SELECT snapshot_data FROM versions WHERE label = ?\n                ";
const SELECT_ALL_SNAPSHOTS: &str = "\n                    SELECT snapshot_data FROM versions ORDER BY timestamp DESC\n                ";
const SELECT_EXISTS: &str = "SELECT 1 FROM versions WHERE label = ?";
const DELETE_VERSION: &str = "DELETE FROM versions WHERE label = ?";
const UPSERT_TAG: &str = "\n                    INSERT OR REPLACE INTO version_tags (tag_name, version_label)\n                    VALUES (?, ?)\n                    ";
const SELECT_TAG: &str = "SELECT version_label FROM version_tags WHERE tag_name = ?";
const SELECT_TAGS: &str = "SELECT tag_name, version_label FROM version_tags";
const INSERT_MUTATION: &str = "\n                    INSERT INTO mutation_log \n                    (timestamp, operation, entity_id, payload, version_label)\n                    VALUES (?, ?, ?, ?, ?)\n                    ";
const SELECT_HISTORY: &str = "\n                    SELECT timestamp, operation, entity_id, payload, version_label\n                    FROM mutation_log \n                    WHERE entity_id = ? \n                    ORDER BY id ASC\n                    ";
const ASSIGN_LABEL: &str = "\n                    UPDATE mutation_log\n                    SET version_label = ?\n                    WHERE version_label IS NULL\n                    ";

/// A failure inside one of the original's `try: ... except sqlite3.Error` blocks: either an
/// `sqlite3.Error` (its `str()`), which the method wraps in a `ProcessingError`, or any other
/// exception, which propagates unchanged.
enum DbError {
    Sqlite(String),
    Other(SemanticaError),
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        DbError::Sqlite(sqlite_message(&err))
    }
}

impl From<SemanticaError> for DbError {
    fn from(err: SemanticaError) -> Self {
        DbError::Other(err)
    }
}

/// `str(e)` of the `sqlite3` exception Python raises for this failure.
fn sqlite_message(err: &rusqlite::Error) -> String {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message.clone(),
        other => other.to_string(),
    }
}

/// The `sqlite3` exception class Python raises for this failure (`get_exception_class` in
/// CPython's `Modules/_sqlite/util.c`, keyed by the primary result code).
fn sqlite_class(err: &rusqlite::Error) -> &'static str {
    match err {
        rusqlite::Error::SqliteFailure(e, _) => match e.extended_code & 0xff {
            // SQLITE_INTERNAL, SQLITE_NOTFOUND
            2 | 12 => "InternalError",
            // SQLITE_NOMEM
            7 => "MemoryError",
            // SQLITE_ERROR, PERM, ABORT, BUSY, LOCKED, READONLY, INTERRUPT, IOERR, FULL,
            // CANTOPEN, PROTOCOL, EMPTY, SCHEMA
            1 | 3 | 4 | 5 | 6 | 8 | 9 | 10 | 13 | 14 | 15 | 16 | 17 => "OperationalError",
            // SQLITE_TOOBIG
            18 => "DataError",
            // SQLITE_CONSTRAINT, SQLITE_MISMATCH
            19 | 20 => "IntegrityError",
            // SQLITE_MISUSE, SQLITE_RANGE
            21 | 25 => "InterfaceError",
            // SQLITE_CORRUPT, NOTADB and everything else
            _ => "DatabaseError",
        },
        _ => "InterfaceError",
    }
}

/// A raw `sqlite3` exception (raised outside the original's `try` blocks).
fn raw_sqlite_error(err: rusqlite::Error) -> SemanticaError {
    let type_name = sqlite_class(&err);
    SemanticaError::Other {
        module: if type_name == "MemoryError" {
            "builtins".to_string()
        } else {
            "sqlite3".to_string()
        },
        type_name: type_name.to_string(),
        message: sqlite_message(&err),
    }
}

/// Bind a JSON-like value as Python's `sqlite3` binds the equivalent Python object (parameter
/// numbers are 1-based, as in its error messages).
fn bind(value: &Value, index: usize) -> Result<SqlValue, DbError> {
    match value {
        Value::Null => Ok(SqlValue::Null),
        Value::Bool(b) => Ok(SqlValue::Integer(i64::from(*b))),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(SqlValue::Integer(i))
            } else if n.as_u64().is_some() {
                Err(DbError::Other(SemanticaError::Overflow(
                    "Python int too large to convert to SQLite INTEGER".to_string(),
                )))
            } else {
                Ok(SqlValue::Real(n.as_f64().unwrap_or(f64::NAN)))
            }
        }
        Value::String(s) => Ok(SqlValue::Text(s.clone())),
        Value::Array(_) | Value::Object(_) => Err(DbError::Sqlite(format!(
            "Error binding parameter {index}: type '{}' is not supported",
            type_name(value)
        ))),
    }
}

fn bind_all(values: &[&Value]) -> Result<Vec<SqlValue>, DbError> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| bind(v, i + 1))
        .collect()
}

/// A column value as the Python object `sqlite3` returns for it.
fn column_value(value: SqlValue) -> Value {
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => Value::from(i),
        SqlValue::Real(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        SqlValue::Text(s) => Value::String(s),
        SqlValue::Blob(b) => Value::String(String::from_utf8_lossy(&b).into_owned()),
    }
}

/// `json.loads(snapshot_data)` for a stored snapshot.
fn load_snapshot(value: SqlValue) -> Result<Map<String, Value>, SemanticaError> {
    let text = match value {
        SqlValue::Text(text) => text,
        other => {
            let name = match other {
                SqlValue::Null => "NoneType",
                SqlValue::Integer(_) => "int",
                SqlValue::Real(_) => "float",
                _ => "bytes",
            };
            return Err(SemanticaError::Type(format!(
                "the JSON object must be str, bytes or bytearray, not {name}"
            )));
        }
    };
    match json_loads(&text)? {
        Value::Object(map) => Ok(map),
        other => Err(SemanticaError::Type(format!(
            "stored snapshot is a {}, not a dict",
            type_name(&other)
        ))),
    }
}

/// SQLite-based persistent version storage (`class SQLiteVersionStorage(VersionStorage)`).
///
/// Uses the original schema (`versions`, `version_tags`, `mutation_log`) and stores each
/// snapshot as Python's `json.dumps(snapshot)` text, so files are interchangeable with the
/// original. Like the original, every operation opens its own connection (5 s busy timeout).
#[derive(Debug)]
pub struct SQLiteVersionStorage {
    /// `self.storage_path`: the database file.
    pub storage_path: PathBuf,
    lock: Mutex<()>,
    /// `self.logger` (`semantica.sqlite_storage`).
    pub logger: Logger,
}

impl SQLiteVersionStorage {
    /// `SQLiteVersionStorage(storage_path)`: create the parent directories and the schema.
    ///
    /// Directory failures are `OSError`s; failures to open or initialise the database are the
    /// raw `sqlite3` exceptions (`SemanticaError::Other { module: "sqlite3", .. }`).
    pub fn new(storage_path: impl AsRef<Path>) -> Result<SQLiteVersionStorage, SemanticaError> {
        let storage_path = storage_path.as_ref().to_path_buf();
        let logger = get_logger("sqlite_storage");
        if let Some(parent) = storage_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SemanticaError::from_io(&e, Some(parent)))?;
        }
        let storage = SQLiteVersionStorage {
            storage_path,
            lock: Mutex::new(()),
            logger,
        };
        storage.init_database()?;
        Ok(storage)
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `sqlite3.connect(str(self.storage_path))`.
    fn connect(&self) -> Result<Connection, SemanticaError> {
        let conn = Connection::open_with_flags(
            &self.storage_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|err| match err {
            // rusqlite appends ": <path>" to SQLite's message; Python reports it bare.
            rusqlite::Error::SqliteFailure(code, Some(message)) => {
                let suffix = format!(": {}", self.storage_path.to_string_lossy());
                let message = message
                    .strip_suffix(&suffix)
                    .map(str::to_string)
                    .unwrap_or(message);
                raw_sqlite_error(rusqlite::Error::SqliteFailure(code, Some(message)))
            }
            other => raw_sqlite_error(other),
        })?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(raw_sqlite_error)?;
        Ok(conn)
    }

    /// `_init_database()`: create the schema if it does not exist.
    fn init_database(&self) -> Result<(), SemanticaError> {
        let _guard = self.lock();
        let conn = self.connect()?;
        for statement in [CREATE_VERSIONS, CREATE_VERSION_TAGS, CREATE_MUTATION_LOG] {
            conn.execute(statement, []).map_err(raw_sqlite_error)?;
        }
        self.logger.debug(format!(
            "Initialized SQLite database at {}",
            self.storage_path.display()
        ));
        Ok(())
    }

    /// Run `body` on a fresh connection, wrapping `sqlite3` errors as
    /// `ProcessingError("{context}: {e}")`.
    fn with_connection<T>(
        &self,
        context: &str,
        body: impl FnOnce(&Connection) -> Result<T, DbError>,
    ) -> Result<T, SemanticaError> {
        let _guard = self.lock();
        let conn = self.connect()?;
        match body(&conn) {
            Ok(value) => Ok(value),
            Err(DbError::Sqlite(message)) => {
                Err(ProcessingError::new(format!("{context}: {message}")).into())
            }
            Err(DbError::Other(err)) => Err(err),
        }
    }
}

impl VersionStorage for SQLiteVersionStorage {
    /// Save snapshot to the SQLite database.
    fn save(&self, snapshot: &Map<String, Value>) -> Result<(), SemanticaError> {
        let label = get(snapshot, "label");
        if !truthy(label) {
            return Err(missing_label());
        }
        let empty = Value::String(String::new());
        let or_empty = |key: &str| snapshot.get(key).unwrap_or(&empty);
        self.with_connection("Failed to save version to database", |conn| {
            // Check if version already exists.
            let exists = conn
                .query_row(SELECT_LABEL, params_from_iter(bind_all(&[label])?), |_| {
                    Ok(())
                })
                .optional()?
                .is_some();
            if exists {
                return Err(DbError::Other(duplicate_label(label)));
            }
            let snapshot_data = Value::String(json_dumps_map_default(snapshot));
            let created_at = Value::String(PyDateTime::now(false).isoformat('T'));
            let params = bind_all(&[
                label,
                or_empty("timestamp"),
                or_empty("author"),
                or_empty("description"),
                or_empty("checksum"),
                &snapshot_data,
                &created_at,
            ])?;
            conn.execute(INSERT_VERSION, params_from_iter(params))?;
            self.logger.debug(format!(
                "Saved version '{}' to SQLite database",
                py_str(label)
            ));
            Ok(())
        })
    }

    /// Retrieve snapshot from the SQLite database.
    fn get(&self, label: &str) -> Result<Option<Map<String, Value>>, SemanticaError> {
        let row = self.with_connection("Failed to retrieve version from database", |conn| {
            Ok(conn
                .query_row(SELECT_SNAPSHOT, [label], |r| r.get::<_, SqlValue>(0))
                .optional()?)
        })?;
        row.map(load_snapshot).transpose()
    }

    /// List all snapshots in the SQLite database, newest timestamp first.
    fn list_all(&self) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        self.with_connection("Failed to list versions from database", |conn| {
            let mut stmt = conn.prepare(SELECT_ALL_SNAPSHOTS)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, SqlValue>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut metadata_list = Vec::with_capacity(rows.len());
            for row in rows {
                let snapshot = load_snapshot(row)?;
                metadata_list.push(snapshot_metadata(&snapshot)?);
            }
            Ok(metadata_list)
        })
    }

    /// Check if version exists in the SQLite database.
    fn exists(&self, label: &str) -> Result<bool, SemanticaError> {
        self.with_connection("Failed to check version existence", |conn| {
            Ok(conn
                .query_row(SELECT_EXISTS, [label], |_| Ok(()))
                .optional()?
                .is_some())
        })
    }

    /// Delete version from the SQLite database.
    fn delete(&self, label: &str) -> Result<bool, SemanticaError> {
        self.with_connection("Failed to delete version from database", |conn| {
            let deleted = conn.execute(DELETE_VERSION, [label])? > 0;
            if deleted {
                self.logger
                    .debug(format!("Deleted version '{label}' from SQLite database"));
            }
            Ok(deleted)
        })
    }

    fn save_tag(&self, tag_name: &str, version_label: &str) -> Result<(), SemanticaError> {
        self.with_connection("Failed to save tag", |conn| {
            conn.execute(UPSERT_TAG, [tag_name, version_label])?;
            Ok(())
        })
    }

    fn get_tag(&self, tag_name: &str) -> Result<Option<String>, SemanticaError> {
        let row = self.with_connection("Failed to get tag", |conn| {
            Ok(conn
                .query_row(SELECT_TAG, [tag_name], |r| r.get::<_, SqlValue>(0))
                .optional()?)
        })?;
        Ok(row.map(column_value).map(|v| match v {
            Value::String(s) => s,
            other => py_str(&other),
        }))
    }

    fn list_tags(&self) -> Result<IndexMap<String, String>, SemanticaError> {
        self.with_connection("Failed to list tags", |conn| {
            let mut stmt = conn.prepare(SELECT_TAGS)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, SqlValue>(0)?, r.get::<_, SqlValue>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let text = |v: SqlValue| match column_value(v) {
                Value::String(s) => s,
                other => py_str(&other),
            };
            Ok(rows
                .into_iter()
                .map(|(tag, label)| (text(tag), text(label)))
                .collect())
        })
    }

    fn save_mutation(&self, mutation: &Map<String, Value>) -> Result<(), SemanticaError> {
        self.with_connection("Failed to save mutation", |conn| {
            let payload = Value::String(match mutation.get("payload") {
                Some(payload) => json_dumps_default(payload),
                None => "{}".to_string(),
            });
            let params = bind_all(&[
                get(mutation, "timestamp"),
                get(mutation, "operation"),
                get(mutation, "entity_id"),
                &payload,
                get(mutation, "version_label"),
            ])?;
            conn.execute(INSERT_MUTATION, params_from_iter(params))?;
            Ok(())
        })
    }

    fn get_entity_history(
        &self,
        entity_id: &str,
    ) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        self.with_connection("Failed to get entity history", |conn| {
            let mut stmt = conn.prepare(SELECT_HISTORY)?;
            let rows = stmt
                .query_map([entity_id], |r| {
                    Ok([
                        r.get::<_, SqlValue>(0)?,
                        r.get::<_, SqlValue>(1)?,
                        r.get::<_, SqlValue>(2)?,
                        r.get::<_, SqlValue>(3)?,
                        r.get::<_, SqlValue>(4)?,
                    ])
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut history = Vec::with_capacity(rows.len());
            for [timestamp, operation, entity, payload, version_label] in rows {
                let payload = match column_value(payload) {
                    Value::String(text) if !text.is_empty() => json_loads(&text)?,
                    Value::String(_) | Value::Null => Value::Object(Map::new()),
                    other => other,
                };
                let mut record = Map::new();
                record.insert("timestamp".into(), column_value(timestamp));
                record.insert("operation".into(), column_value(operation));
                record.insert("entity_id".into(), column_value(entity));
                record.insert("payload".into(), payload);
                record.insert("version_label".into(), column_value(version_label));
                history.push(record);
            }
            Ok(history)
        })
    }

    fn assign_version_to_unlabeled_mutations(
        &self,
        version_label: &str,
    ) -> Result<(), SemanticaError> {
        self.with_connection("Failed to assign version label to mutations", |conn| {
            conn.execute(ASSIGN_LABEL, [version_label])?;
            Ok(())
        })
    }
}

/// `compute_checksum(data)`: SHA-256 (lower-case hex) of
/// `json.dumps(data, sort_keys=True, separators=(",", ":"))`.
pub fn compute_checksum(data: &Map<String, Value>) -> String {
    checksum_of(data, None)
}

fn checksum_of(data: &Map<String, Value>, skip: Option<&str>) -> String {
    let text = json_dumps_canonical(data, skip);
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// `verify_checksum(snapshot)`: `true` when the snapshot's `checksum` field equals the checksum
/// of the snapshot without that field; `false` when the field is missing or falsy.
pub fn verify_checksum(snapshot: &Map<String, Value>) -> bool {
    let stored = get(snapshot, "checksum");
    if !truthy(stored) {
        return false;
    }
    let computed = checksum_of(snapshot, Some("checksum"));
    matches!(stored, Value::String(s) if *s == computed)
}
