//! Provenance storage backends (port of `semantica/provenance/storage.py`).
//!
//! - [`ProvenanceStorage`]: the abstract storage interface (a trait; object safe, so the manager
//!   holds an `Arc<dyn ProvenanceStorage>` and custom backends plug in like Python subclasses).
//! - [`InMemoryStorage`]: fast in-memory storage for development and testing.
//! - [`SQLiteStorage`]: persistent SQLite storage (W3C PROV-O schema, WAL mode, batched BFS).
//!
//! Python's context-manager transactions (`with storage.transaction() as conn:`) become
//! closure-taking methods: the body runs with the active connection; returning `Ok` commits and
//! returning `Err` (Python: raising) rolls back and propagates the error. The connection a body
//! receives is Python's `conn` object: `None`, the in-memory marker, or an open SQLite
//! connection ([`StorageConnection`]).

use std::any::Any;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::thread::ThreadId;

use indexmap::{IndexMap, IndexSet};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, Row, params_from_iter};
use serde_json::{Map, Value};

use sem_utils::SemanticaError;
use sem_utils::logging::{Logger, get_logger};
use sem_utils::pycompat::fmt::float_repr;
use sem_utils::pycompat::json::dumps_default;

use crate::schemas::ProvenanceEntry;

// ---------------------------------------------------------------------------------------------
// connections and errors
// ---------------------------------------------------------------------------------------------

/// The connection object a storage transaction yields (Python's `conn`).
///
/// `None` (the base-class default transaction) is represented by `Option::None` wherever a
/// connection is accepted.
#[derive(Clone, Copy)]
pub enum StorageConnection<'a> {
    /// `InMemoryStorage`'s staging marker (`"IN_MEMORY_TX"`).
    InMemory,
    /// An open SQLite connection inside `BEGIN IMMEDIATE`.
    Sqlite(&'a Connection),
    /// A custom backend's own connection handle.
    Custom(&'a dyn Any),
}

impl std::fmt::Debug for StorageConnection<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageConnection::InMemory => f.write_str("'IN_MEMORY_TX'"),
            StorageConnection::Sqlite(_) => f.write_str("<sqlite3.Connection object>"),
            StorageConnection::Custom(_) => f.write_str("<connection>"),
        }
    }
}

/// The body of a transaction or savepoint: runs with the active connection.
pub type TransactionBody<'b> =
    dyn FnMut(Option<&StorageConnection<'_>>) -> Result<(), SemanticaError> + 'b;

/// A Python exception of class `module.name`.
pub(crate) fn py_error(module: &str, name: &str, message: impl Into<String>) -> SemanticaError {
    SemanticaError::Other {
        module: module.to_string(),
        type_name: name.to_string(),
        message: message.into(),
    }
}

/// The `sqlite3` exception Python raises for a SQLite failure (CPython's result-code mapping in
/// `Modules/_sqlite/util.c`), carrying SQLite's own error message.
pub(crate) fn sqlite_error(err: rusqlite::Error) -> SemanticaError {
    match &err {
        rusqlite::Error::SqliteFailure(failure, message) => {
            let class = match failure.extended_code & 0xff {
                2 | 12 => "InternalError", // SQLITE_INTERNAL, SQLITE_NOTFOUND
                7 => {
                    return SemanticaError::Other {
                        module: "builtins".to_string(),
                        type_name: "MemoryError".to_string(),
                        message: String::new(),
                    };
                }
                1 | 3 | 4 | 5 | 6 | 8 | 9 | 10 | 13 | 14 | 15 | 16 | 17 => "OperationalError",
                11 => "DatabaseError", // SQLITE_CORRUPT
                18 => "DataError",     // SQLITE_TOOBIG
                19 | 20 => "IntegrityError",
                21 | 25 => "InterfaceError",
                _ => "DatabaseError",
            };
            let mut text = message.clone().unwrap_or_else(|| failure.to_string());
            // rusqlite appends the path to open failures; sqlite3 reports SQLite's message only.
            const CANTOPEN: &str = "unable to open database file";
            if text.starts_with(CANTOPEN) {
                text.truncate(CANTOPEN.len());
            }
            py_error("sqlite3", class, text)
        }
        rusqlite::Error::FromSqlConversionFailure(..)
        | rusqlite::Error::InvalidColumnType(..)
        | rusqlite::Error::IntegralValueOutOfRange(..) => SemanticaError::Type(err.to_string()),
        other => py_error("sqlite3", "InterfaceError", other.to_string()),
    }
}

fn is_operational_error(err: &SemanticaError) -> bool {
    matches!(err, SemanticaError::Other { module, type_name, .. }
        if module == "sqlite3" && type_name == "OperationalError")
}

/// Upcast support so callers can recover the concrete backend behind a `dyn ProvenanceStorage`
/// (Python's `isinstance(manager.storage, SQLiteStorage)`).
pub trait AsAny {
    /// `self` as `&dyn Any`.
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------------------------
// ProvenanceStorage
// ---------------------------------------------------------------------------------------------

/// Abstract storage interface for provenance tracking (`ProvenanceStorage(ABC)`).
///
/// `store`, `retrieve`, `retrieve_all`, `trace_lineage` and `clear` are the abstract methods;
/// the others have the base class's default behaviour. `store_with_conn`/`retrieve_with_conn`
/// are the original's `_store_with_conn`/`_retrieve_with_conn` hooks that the manager uses
/// inside a transaction; overriding them is how custom backends join a transaction.
pub trait ProvenanceStorage: AsAny + Send + Sync {
    /// Store a provenance entry.
    fn store(&self, entry: &ProvenanceEntry) -> Result<(), SemanticaError>;

    /// Retrieve the entry for `entity_id`, if any.
    fn retrieve(&self, entity_id: &str) -> Result<Option<ProvenanceEntry>, SemanticaError>;

    /// All entries, optionally filtered by `entity_type` (an empty filter means no filter).
    fn retrieve_all(
        &self,
        entity_type: Option<&str>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError>;

    /// The lineage of `entity_id` (BFS over `parent_entity_id` and `used_entities`), at most
    /// `max_depth` levels deep.
    fn trace_lineage(
        &self,
        entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError>;

    /// Clear all provenance data; returns the number of entries removed.
    fn clear(&self) -> Result<usize, SemanticaError>;

    /// Whether `trace_lineage` honours `max_depth`. Backends written against the original
    /// one-argument `trace_lineage(entity_id)` return `false`; the manager then calls them
    /// without a depth (Python inspects the method signature for this).
    fn supports_max_depth(&self) -> bool {
        true
    }

    /// Default transaction: runs `body` with no connection (`yield None`).
    fn transaction(&self, body: &mut TransactionBody<'_>) -> Result<(), SemanticaError> {
        body(None)
    }

    /// Default savepoint: runs `body` with the given connection (`yield conn`).
    fn savepoint(
        &self,
        conn: Option<&StorageConnection<'_>>,
        body: &mut TransactionBody<'_>,
    ) -> Result<(), SemanticaError> {
        body(conn)
    }

    /// `_store_with_conn`: store using an active connection/transaction (default: `store`).
    fn store_with_conn(
        &self,
        conn: Option<&StorageConnection<'_>>,
        entry: &ProvenanceEntry,
    ) -> Result<(), SemanticaError> {
        let _ = conn;
        self.store(entry)
    }

    /// `_retrieve_with_conn`: retrieve using an active connection/transaction (default:
    /// `retrieve`).
    fn retrieve_with_conn(
        &self,
        conn: Option<&StorageConnection<'_>>,
        entity_id: &str,
    ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        let _ = conn;
        self.retrieve(entity_id)
    }

    /// `(sequence_id, checksum)` of the most recently stored entry, or `None` when the store is
    /// empty or the backend does not chain entries (the default).
    fn get_chain_head(
        &self,
        conn: Option<&StorageConnection<'_>>,
    ) -> Result<Option<(i64, Option<String>)>, SemanticaError> {
        let _ = conn;
        Ok(None)
    }

    /// Downstream descendants of `entity_id` (reverse lineage) in BFS order. The default builds
    /// a reverse index over `retrieve_all()` (`parent_entity_id`, `previous_version_id`,
    /// `derived_from_id`, `used_entities`).
    fn trace_descendants(
        &self,
        entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        default_trace_descendants(self, entity_id, max_depth)
    }
}

impl<'s> dyn ProvenanceStorage + 's {
    /// The concrete backend, if it is a `T` (`isinstance(storage, T)`).
    pub fn downcast_ref<T: ProvenanceStorage + 'static>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }

    /// Whether the backend is a `T`.
    pub fn is<T: ProvenanceStorage + 'static>(&self) -> bool {
        self.as_any().is::<T>()
    }
}

/// Generic helpers over the closure-based transaction API.
pub trait ProvenanceStorageExt: ProvenanceStorage {
    /// Run `body` inside `transaction()` and return its value.
    fn with_transaction<T>(
        &self,
        body: impl FnOnce(Option<&StorageConnection<'_>>) -> Result<T, SemanticaError>,
    ) -> Result<T, SemanticaError> {
        let mut body = Some(body);
        let mut out = None;
        self.transaction(&mut |conn| {
            let f = body.take().expect("transaction body runs once");
            out = Some(f(conn)?);
            Ok(())
        })?;
        Ok(out.expect("transaction body ran"))
    }

    /// Run `body` inside `savepoint(conn)` and return its value.
    fn with_savepoint<T>(
        &self,
        conn: Option<&StorageConnection<'_>>,
        body: impl FnOnce(Option<&StorageConnection<'_>>) -> Result<T, SemanticaError>,
    ) -> Result<T, SemanticaError> {
        let mut body = Some(body);
        let mut out = None;
        self.savepoint(conn, &mut |conn| {
            let f = body.take().expect("savepoint body runs once");
            out = Some(f(conn)?);
            Ok(())
        })?;
        Ok(out.expect("savepoint body ran"))
    }
}

impl<S: ProvenanceStorage + ?Sized> ProvenanceStorageExt for S {}

/// The base class's generic `trace_descendants`, usable by any backend.
pub fn default_trace_descendants<S: ProvenanceStorage + ?Sized>(
    storage: &S,
    entity_id: &str,
    max_depth: Option<usize>,
) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
    let mut reverse_index: HashMap<String, Vec<usize>> = HashMap::new();
    let entries = storage.retrieve_all(None)?;
    for (position, entry) in entries.iter().enumerate() {
        let mut referenced: IndexSet<&str> = IndexSet::new();
        for link in [
            &entry.parent_entity_id,
            &entry.previous_version_id,
            &entry.derived_from_id,
        ]
        .into_iter()
        .flatten()
        {
            if !link.is_empty() {
                referenced.insert(link);
            }
        }
        for used in &entry.used_entities {
            referenced.insert(used);
        }
        for reference in referenced {
            reverse_index
                .entry(reference.to_string())
                .or_default()
                .push(position);
        }
    }
    let mut descendants = Vec::new();
    let mut visited: HashSet<String> = HashSet::from([entity_id.to_string()]);
    let mut queue: VecDeque<(String, usize)> = VecDeque::from([(entity_id.to_string(), 0)]);
    while let Some((current, depth)) = queue.pop_front() {
        if max_depth.is_some_and(|max| depth >= max) {
            continue;
        }
        for &position in reverse_index
            .get(&current)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let child = &entries[position];
            if visited.contains(&child.entity_id) {
                continue;
            }
            visited.insert(child.entity_id.clone());
            descendants.push(child.clone());
            queue.push_back((child.entity_id.clone(), depth + 1));
        }
    }
    Ok(descendants)
}

// ---------------------------------------------------------------------------------------------
// InMemoryStorage
// ---------------------------------------------------------------------------------------------

type PendingLevel = IndexMap<String, ProvenanceEntry>;

#[derive(Debug, Default)]
struct ChainState {
    counter: i64,
    head: Option<String>,
}

/// Fast in-memory storage for provenance tracking (development, testing, short-lived
/// processes). Entries keep insertion order; re-storing an entity keeps its position.
///
/// Transactions stage entries in a per-thread stack of pending levels (Python's
/// `threading.local`): nested transactions and savepoints merge into their parent on success,
/// the outermost one stores everything on success, and a failing level is discarded.
#[derive(Default)]
pub struct InMemoryStorage {
    entries: Mutex<IndexMap<String, ProvenanceEntry>>,
    chain: Mutex<ChainState>,
    pending: Mutex<HashMap<ThreadId, Vec<PendingLevel>>>,
}

impl std::fmt::Debug for InMemoryStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryStorage")
            .field("entries", &self.lock_entries().len())
            .finish()
    }
}

impl InMemoryStorage {
    /// `InMemoryStorage()`.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_entries(&self) -> std::sync::MutexGuard<'_, IndexMap<String, ProvenanceEntry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_chain(&self) -> std::sync::MutexGuard<'_, ChainState> {
        self.chain.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_pending(&self) -> std::sync::MutexGuard<'_, HashMap<ThreadId, Vec<PendingLevel>>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn with_stack<T>(&self, f: impl FnOnce(&mut Vec<PendingLevel>) -> T) -> T {
        let id = std::thread::current().id();
        let mut pending = self.lock_pending();
        let stack = pending.entry(id).or_default();
        let out = f(stack);
        if stack.is_empty() {
            pending.remove(&id);
        }
        out
    }

    /// `transaction()` whose outermost commit stores the staged entries through `this.store`
    /// (Python's `self.store` dispatches to the most derived class, so a wrapper or "subclass"
    /// that overrides `store` passes itself here).
    pub fn transaction_via(
        &self,
        this: &dyn ProvenanceStorage,
        body: &mut TransactionBody<'_>,
    ) -> Result<(), SemanticaError> {
        self.with_stack(|stack| stack.push(PendingLevel::new()));
        match body(Some(&StorageConnection::InMemory)) {
            Ok(()) => {
                let to_store = self.with_stack(|stack| {
                    let level = stack.pop().unwrap_or_default();
                    if let Some(parent) = stack.last_mut() {
                        for (key, entry) in level {
                            parent.insert(key, entry);
                        }
                        None
                    } else {
                        Some(level)
                    }
                });
                if let Some(level) = to_store {
                    for entry in level.values() {
                        this.store(entry)?;
                    }
                }
                Ok(())
            }
            Err(err) => {
                self.with_stack(|stack| {
                    stack.pop();
                });
                Err(err)
            }
        }
    }

    /// `savepoint()` with commit-time stores dispatched through `this` (see
    /// [`InMemoryStorage::transaction_via`]).
    pub fn savepoint_via(
        &self,
        this: &dyn ProvenanceStorage,
        body: &mut TransactionBody<'_>,
    ) -> Result<(), SemanticaError> {
        self.transaction_via(this, body)
    }
}

impl ProvenanceStorage for InMemoryStorage {
    fn store(&self, entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
        {
            let mut chain = self.lock_chain();
            if let Some(sequence_id) = entry.sequence_id {
                chain.counter = chain.counter.max(sequence_id);
            }
            chain.head = entry.checksum.clone();
        }
        self.lock_entries()
            .insert(entry.entity_id.clone(), entry.clone());
        Ok(())
    }

    fn retrieve(&self, entity_id: &str) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        Ok(self.lock_entries().get(entity_id).cloned())
    }

    fn retrieve_all(
        &self,
        entity_type: Option<&str>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        let entries = self.lock_entries();
        Ok(match entity_type {
            Some(kind) if !kind.is_empty() => entries
                .values()
                .filter(|entry| entry.entity_type == kind)
                .cloned()
                .collect(),
            _ => entries.values().cloned().collect(),
        })
    }

    fn trace_lineage(
        &self,
        entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        let mut lineage = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::from([(entity_id.to_string(), 0)]);
        while let Some((current, depth)) = queue.pop_front() {
            if visited.contains(&current) || max_depth.is_some_and(|max| depth >= max) {
                continue;
            }
            visited.insert(current.clone());
            if let Some(entry) = self.retrieve(&current)? {
                if let Some(parent) = entry.parent_entity_id.as_deref().filter(|p| !p.is_empty()) {
                    queue.push_back((parent.to_string(), depth + 1));
                }
                for used in &entry.used_entities {
                    if !visited.contains(used) {
                        queue.push_back((used.clone(), depth + 1));
                    }
                }
                lineage.push(entry);
            }
        }
        Ok(lineage)
    }

    fn clear(&self) -> Result<usize, SemanticaError> {
        let count = {
            let mut entries = self.lock_entries();
            let count = entries.len();
            entries.clear();
            count
        };
        let mut chain = self.lock_chain();
        chain.counter = 0;
        chain.head = None;
        Ok(count)
    }

    fn transaction(&self, body: &mut TransactionBody<'_>) -> Result<(), SemanticaError> {
        self.transaction_via(self, body)
    }

    fn savepoint(
        &self,
        _conn: Option<&StorageConnection<'_>>,
        body: &mut TransactionBody<'_>,
    ) -> Result<(), SemanticaError> {
        self.transaction_via(self, body)
    }

    fn store_with_conn(
        &self,
        _conn: Option<&StorageConnection<'_>>,
        entry: &ProvenanceEntry,
    ) -> Result<(), SemanticaError> {
        let staged = self.with_stack(|stack| match stack.last_mut() {
            Some(level) => {
                level.insert(entry.entity_id.clone(), entry.clone());
                true
            }
            None => false,
        });
        if staged { Ok(()) } else { self.store(entry) }
    }

    fn retrieve_with_conn(
        &self,
        _conn: Option<&StorageConnection<'_>>,
        entity_id: &str,
    ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        let staged = self.with_stack(|stack| {
            stack
                .iter()
                .rev()
                .find_map(|level| level.get(entity_id).cloned())
        });
        match staged {
            Some(entry) => Ok(Some(entry)),
            None => self.retrieve(entity_id),
        }
    }

    /// `(sequence_id, checksum)` of the most recent entry, including entries staged in this
    /// thread's open transaction (compared against the committed head, since a transaction can
    /// stage an archived relabel whose `sequence_id` is older than the committed head).
    fn get_chain_head(
        &self,
        _conn: Option<&StorageConnection<'_>>,
    ) -> Result<Option<(i64, Option<String>)>, SemanticaError> {
        let (mut best_seq, mut best_checksum) = {
            let chain = self.lock_chain();
            (chain.counter, chain.head.clone())
        };
        self.with_stack(|stack| {
            for level in stack.iter() {
                for staged in level.values() {
                    if let Some(sequence_id) = staged.sequence_id
                        && (best_checksum.is_none() || sequence_id > best_seq)
                    {
                        best_seq = sequence_id;
                        best_checksum = staged.checksum.clone();
                    }
                }
            }
        });
        Ok(best_checksum.map(|checksum| (best_seq, Some(checksum))))
    }
}

// ---------------------------------------------------------------------------------------------
// SQLiteStorage
// ---------------------------------------------------------------------------------------------

/// Replacement for `SQLiteStorage._configure_connection` (see
/// [`SQLiteStorage::set_configure_connection_hook`]).
pub type ConfigureConnectionHook =
    Box<dyn Fn(&Connection) -> Result<(), SemanticaError> + Send + Sync>;

/// Persistent SQLite storage for provenance tracking (production, audit trails).
///
/// Like the original, no connection is held between calls: every public method opens one
/// connection (WAL journal, `busy_timeout=5000`, `synchronous=NORMAL`), does all its work on
/// it, and closes it before returning, so the database file can be deleted as soon as a call
/// returns. Writes run under `BEGIN IMMEDIATE`; reads take no writer lock.
pub struct SQLiteStorage {
    db_path: String,
    logger: Logger,
    configure_hook: Mutex<Option<ConfigureConnectionHook>>,
}

impl std::fmt::Debug for SQLiteStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SQLiteStorage")
            .field("db_path", &self.db_path)
            .finish()
    }
}

const CREATE_TABLE: &str = "
                CREATE TABLE IF NOT EXISTS provenance (
                    entity_id TEXT PRIMARY KEY,
                    entity_type TEXT NOT NULL,
                    activity_id TEXT NOT NULL,
                    agent_id TEXT DEFAULT 'semantica',
                    source_document TEXT,
                    source_location TEXT,
                    source_quote TEXT,
                    timestamp TEXT NOT NULL,
                    first_seen TEXT,
                    last_updated TEXT,
                    confidence REAL DEFAULT 1.0,
                    checksum TEXT,
                    parent_entity_id TEXT,
                    used_entities TEXT,
                    start_index INTEGER,
                    end_index INTEGER,
                    credibility REAL,
                    metadata TEXT,
                    version TEXT DEFAULT '1.0',
                    agent_type TEXT DEFAULT 'software_agent',
                    is_automated INTEGER DEFAULT 1,
                    role TEXT,
                    sequence_id INTEGER,
                    previous_checksum TEXT,
                    previous_version_id TEXT,
                    derived_from_id TEXT,
                    invalidated INTEGER DEFAULT 0,
                    invalidated_at_time TEXT,
                    invalidated_by TEXT,
                    invalidation_reason TEXT,
                    activity_started_at_time TEXT,
                    activity_ended_at_time TEXT,
                    acted_on_behalf_of TEXT,
                    informed_by_activities TEXT,
                    valid_from TEXT,
                    valid_until TEXT,
                    revision_type TEXT,
                    supersedes TEXT,
                    bundle_id TEXT
                )
            ";

const INDEXES: [(&str, &str); 8] = [
    ("idx_entity_type", "entity_type"),
    ("idx_source_document", "source_document"),
    ("idx_parent_entity", "parent_entity_id"),
    ("idx_previous_version_id", "previous_version_id"),
    ("idx_derived_from_id", "derived_from_id"),
    ("idx_sequence_id", "sequence_id"),
    ("idx_invalidated", "invalidated"),
    ("idx_bundle_id", "bundle_id"),
];

const INSERT_ENTRY: &str = "
            INSERT OR REPLACE INTO provenance VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?, ?, ?
            )
        ";

const CHAIN_HEAD_QUERY: &str = "SELECT sequence_id, checksum FROM provenance \
     WHERE sequence_id IS NOT NULL \
     ORDER BY sequence_id DESC, rowid DESC LIMIT 1";

/// SQLite's default limit on variables per statement that the batched queries stay under.
const IN_CHUNK: usize = 999;

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(",")
}

fn sql_text(value: &str) -> SqlValue {
    SqlValue::Text(value.to_string())
}

fn sql_opt_text(value: &Option<String>) -> SqlValue {
    value
        .as_ref()
        .map(|s| SqlValue::Text(s.clone()))
        .unwrap_or(SqlValue::Null)
}

fn sql_opt_i64(value: Option<i64>) -> SqlValue {
    value.map(SqlValue::Integer).unwrap_or(SqlValue::Null)
}

fn sql_opt_f64(value: Option<f64>) -> SqlValue {
    value.map(SqlValue::Real).unwrap_or(SqlValue::Null)
}

fn json_text(value: &Value) -> SqlValue {
    SqlValue::Text(dumps_default(value))
}

fn str_list_json(values: &[String]) -> SqlValue {
    json_text(&Value::Array(
        values.iter().cloned().map(Value::String).collect(),
    ))
}

fn column_text(row: &Row<'_>, index: usize) -> Result<Option<String>, SemanticaError> {
    Ok(match row.get_ref(index).map_err(sqlite_error)? {
        ValueRef::Null => None,
        ValueRef::Text(text) | ValueRef::Blob(text) => {
            Some(String::from_utf8_lossy(text).into_owned())
        }
        ValueRef::Integer(n) => Some(n.to_string()),
        ValueRef::Real(f) => Some(float_repr(f)),
    })
}

fn column_i64(row: &Row<'_>, index: usize) -> Result<Option<i64>, SemanticaError> {
    match row.get_ref(index).map_err(sqlite_error)? {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(n) => Ok(Some(n)),
        ValueRef::Real(f) if f.fract() == 0.0 => Ok(Some(f as i64)),
        ValueRef::Text(text) => std::str::from_utf8(text)
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map(Some)
            .ok_or_else(|| {
                SemanticaError::Type(format!("column {index} does not hold an integer"))
            }),
        _ => Err(SemanticaError::Type(format!(
            "column {index} does not hold an integer"
        ))),
    }
}

fn column_f64(row: &Row<'_>, index: usize) -> Result<Option<f64>, SemanticaError> {
    match row.get_ref(index).map_err(sqlite_error)? {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(n) => Ok(Some(n as f64)),
        ValueRef::Real(f) => Ok(Some(f)),
        ValueRef::Text(text) => std::str::from_utf8(text)
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(Some)
            .ok_or_else(|| SemanticaError::Type(format!("column {index} does not hold a float"))),
        ValueRef::Blob(_) => Err(SemanticaError::Type(format!(
            "column {index} does not hold a float"
        ))),
    }
}

/// `bool(row[i]) if row[i] is not None else default`.
fn column_bool(row: &Row<'_>, index: usize, default: bool) -> Result<bool, SemanticaError> {
    Ok(match row.get_ref(index).map_err(sqlite_error)? {
        ValueRef::Null => default,
        ValueRef::Integer(n) => n != 0,
        ValueRef::Real(f) => f != 0.0,
        ValueRef::Text(text) | ValueRef::Blob(text) => !text.is_empty(),
    })
}

/// `json.loads(row[i]) if row[i] else default`.
fn column_json(row: &Row<'_>, index: usize) -> Result<Option<Value>, SemanticaError> {
    match column_text(row, index)? {
        Some(text) if !text.is_empty() => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| SemanticaError::JsonDecode(e.to_string())),
        _ => Ok(None),
    }
}

fn json_str_list(value: Option<Value>, field: &str) -> Result<Vec<String>, SemanticaError> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .into_iter()
            .map(|item| match item {
                Value::String(s) => Ok(s),
                other => Err(SemanticaError::Type(format!(
                    "{field} holds a non-string item: {other}"
                ))),
            })
            .collect(),
        Some(other) => Err(SemanticaError::Type(format!(
            "{field} is not a JSON list: {other}"
        ))),
    }
}

fn sqlite_conn<'c>(conn: Option<&StorageConnection<'c>>) -> Result<&'c Connection, SemanticaError> {
    match conn {
        Some(StorageConnection::Sqlite(c)) => Ok(c),
        None => Err(SemanticaError::Attribute(
            "'NoneType' object has no attribute 'cursor'".to_string(),
        )),
        Some(StorageConnection::InMemory) => Err(SemanticaError::Attribute(
            "'str' object has no attribute 'cursor'".to_string(),
        )),
        Some(StorageConnection::Custom(_)) => Err(SemanticaError::Attribute(
            "connection object has no attribute 'cursor'".to_string(),
        )),
    }
}

impl SQLiteStorage {
    /// The `db_path` default of `SQLiteStorage()`.
    pub const DEFAULT_DB_PATH: &'static str = "provenance.db";

    /// Columns added after the table first shipped (`_MIGRATION_COLUMNS`); a pre-existing
    /// table missing any of them gets them added on open.
    pub const MIGRATION_COLUMNS: [(&'static str, &'static str); 20] = [
        ("agent_type", "TEXT DEFAULT 'software_agent'"),
        ("is_automated", "INTEGER DEFAULT 1"),
        ("role", "TEXT"),
        ("sequence_id", "INTEGER"),
        ("previous_checksum", "TEXT"),
        ("previous_version_id", "TEXT"),
        ("derived_from_id", "TEXT"),
        ("invalidated", "INTEGER DEFAULT 0"),
        ("invalidated_at_time", "TEXT"),
        ("invalidated_by", "TEXT"),
        ("invalidation_reason", "TEXT"),
        ("activity_started_at_time", "TEXT"),
        ("activity_ended_at_time", "TEXT"),
        ("acted_on_behalf_of", "TEXT"),
        ("informed_by_activities", "TEXT"),
        ("valid_from", "TEXT"),
        ("valid_until", "TEXT"),
        ("revision_type", "TEXT"),
        ("supersedes", "TEXT"),
        ("bundle_id", "TEXT"),
    ];

    /// `SQLiteStorage(db_path)`: open (creating if needed) and migrate the database.
    pub fn new(db_path: impl Into<String>) -> Result<Self, SemanticaError> {
        let storage = SQLiteStorage {
            db_path: db_path.into(),
            logger: get_logger("sqlite_storage"),
            configure_hook: Mutex::new(None),
        };
        storage.init_db()?;
        Ok(storage)
    }

    /// Path of the SQLite database file.
    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    /// The `semantica.sqlite_storage` logger.
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// Replace the connection configuration step (`_configure_connection`) for this storage, or
    /// restore the default with `None`. This is the seam the original tests patch
    /// (`patch.object(storage, "_configure_connection", ...)`).
    pub fn set_configure_connection_hook(&self, hook: Option<ConfigureConnectionHook>) {
        *self
            .configure_hook
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = hook;
    }

    fn connect(&self) -> Result<Connection, SemanticaError> {
        Connection::open(&self.db_path).map_err(sqlite_error)
    }

    /// `_configure_connection`: WAL journal (falling back to DELETE), `busy_timeout=5000`,
    /// `synchronous=NORMAL`; unsupported PRAGMAs only log a warning.
    pub fn configure_connection(&self, conn: &Connection) -> Result<(), SemanticaError> {
        if let Some(hook) = self
            .configure_hook
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return hook(conn);
        }
        if let Err(err) = conn
            .execute_batch("PRAGMA journal_mode=WAL")
            .map_err(sqlite_error)
        {
            if !is_operational_error(&err) {
                return Err(err);
            }
            self.logger.warning(format!(
                "WAL journal mode not supported ({err}), falling back to DELETE journal mode."
            ));
            let _ = conn.execute_batch("PRAGMA journal_mode=DELETE");
        }
        if let Err(err) = conn
            .execute_batch("PRAGMA busy_timeout=5000")
            .map_err(sqlite_error)
        {
            if !is_operational_error(&err) {
                return Err(err);
            }
            self.logger
                .warning(format!("busy_timeout PRAGMA not supported ({err})"));
        }
        if let Err(err) = conn
            .execute_batch("PRAGMA synchronous=NORMAL")
            .map_err(sqlite_error)
        {
            if !is_operational_error(&err) {
                return Err(err);
            }
            self.logger
                .warning(format!("synchronous PRAGMA not supported ({err})"));
        }
        Ok(())
    }

    /// `_read_connection`: a configured connection without an explicit transaction, so reads
    /// do not wait for the writer lock.
    fn with_read_connection<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, SemanticaError>,
    ) -> Result<T, SemanticaError> {
        let conn = self.connect()?;
        self.configure_connection(&conn)?;
        f(&conn)
    }

    fn migrate_schema(&self, conn: &Connection) -> Result<(), SemanticaError> {
        let existing: HashSet<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(provenance)")
                .map_err(sqlite_error)?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(sqlite_error)?;
            rows.collect::<Result<_, _>>().map_err(sqlite_error)?
        };
        for (column, definition) in Self::MIGRATION_COLUMNS {
            if !existing.contains(column) {
                conn.execute_batch(&format!(
                    "ALTER TABLE provenance ADD COLUMN {column} {definition}"
                ))
                .map_err(sqlite_error)?;
            }
        }
        Ok(())
    }

    fn init_db(&self) -> Result<(), SemanticaError> {
        let conn = self.connect()?;
        self.configure_connection(&conn)?;
        conn.execute_batch(CREATE_TABLE).map_err(sqlite_error)?;
        self.migrate_schema(&conn)?;
        for (name, column) in INDEXES {
            conn.execute_batch(&format!(
                "
                CREATE INDEX IF NOT EXISTS {name}
                ON provenance({column})
            "
            ))
            .map_err(sqlite_error)?;
        }
        Ok(())
    }

    fn entry_params(entry: &ProvenanceEntry) -> Vec<SqlValue> {
        vec![
            sql_text(&entry.entity_id),
            sql_text(&entry.entity_type),
            sql_text(&entry.activity_id),
            sql_text(&entry.agent_id),
            sql_text(&entry.source_document),
            sql_opt_text(&entry.source_location),
            sql_opt_text(&entry.source_quote),
            sql_text(&entry.timestamp),
            sql_opt_text(&entry.first_seen),
            sql_opt_text(&entry.last_updated),
            SqlValue::Real(entry.confidence),
            sql_opt_text(&entry.checksum),
            sql_opt_text(&entry.parent_entity_id),
            str_list_json(&entry.used_entities),
            sql_opt_i64(entry.start_index),
            sql_opt_i64(entry.end_index),
            sql_opt_f64(entry.credibility),
            json_text(&Value::Object(entry.metadata.clone())),
            sql_text(&entry.version),
            sql_text(&entry.agent_type),
            SqlValue::Integer(i64::from(entry.is_automated)),
            sql_opt_text(&entry.role),
            sql_opt_i64(entry.sequence_id),
            sql_opt_text(&entry.previous_checksum),
            sql_opt_text(&entry.previous_version_id),
            sql_opt_text(&entry.derived_from_id),
            SqlValue::Integer(i64::from(entry.invalidated)),
            sql_opt_text(&entry.invalidated_at_time),
            sql_opt_text(&entry.invalidated_by),
            sql_opt_text(&entry.invalidation_reason),
            sql_opt_text(&entry.activity_started_at_time),
            sql_opt_text(&entry.activity_ended_at_time),
            sql_opt_text(&entry.acted_on_behalf_of),
            str_list_json(&entry.informed_by_activities),
            sql_opt_text(&entry.valid_from),
            sql_opt_text(&entry.valid_until),
            sql_opt_text(&entry.revision_type),
            sql_opt_text(&entry.supersedes),
            sql_opt_text(&entry.bundle_id),
        ]
    }

    /// `_row_to_entry`: a `SELECT *` row (column positions as created/migrated) to an entry.
    fn row_to_entry(row: &Row<'_>) -> Result<ProvenanceEntry, SemanticaError> {
        let text = |i: usize| column_text(row, i);
        let required = |i: usize| -> Result<String, SemanticaError> {
            Ok(column_text(row, i)?.unwrap_or_default())
        };
        let mut entry =
            ProvenanceEntry::with_timestamp(required(0)?, required(1)?, required(2)?, required(7)?);
        entry.agent_id = required(3)?;
        entry.source_document = text(4)?.filter(|s| !s.is_empty()).unwrap_or_default();
        entry.source_location = text(5)?;
        entry.source_quote = text(6)?;
        entry.first_seen = text(8)?;
        entry.last_updated = text(9)?;
        entry.confidence = column_f64(row, 10)?.unwrap_or(1.0);
        entry.checksum = text(11)?;
        entry.parent_entity_id = text(12)?;
        entry.used_entities = json_str_list(column_json(row, 13)?, "used_entities")?;
        entry.start_index = column_i64(row, 14)?;
        entry.end_index = column_i64(row, 15)?;
        entry.credibility = column_f64(row, 16)?;
        entry.metadata = match column_json(row, 17)? {
            None => Map::new(),
            Some(Value::Object(map)) => map,
            Some(other) => {
                return Err(SemanticaError::Type(format!(
                    "metadata is not a JSON object: {other}"
                )));
            }
        };
        entry.version = text(18)?.unwrap_or_else(|| "1.0".to_string());
        entry.agent_type = text(19)?
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "software_agent".to_string());
        entry.is_automated = column_bool(row, 20, true)?;
        entry.role = text(21)?;
        entry.sequence_id = column_i64(row, 22)?;
        entry.previous_checksum = text(23)?;
        entry.previous_version_id = text(24)?;
        entry.derived_from_id = text(25)?;
        entry.invalidated = column_bool(row, 26, false)?;
        entry.invalidated_at_time = text(27)?;
        entry.invalidated_by = text(28)?;
        entry.invalidation_reason = text(29)?;
        entry.activity_started_at_time = text(30)?;
        entry.activity_ended_at_time = text(31)?;
        entry.acted_on_behalf_of = text(32)?;
        entry.informed_by_activities =
            json_str_list(column_json(row, 33)?, "informed_by_activities")?;
        entry.valid_from = text(34)?;
        entry.valid_until = text(35)?;
        entry.revision_type = text(36)?;
        entry.supersedes = text(37)?;
        entry.bundle_id = text(38)?;
        Ok(entry)
    }

    fn query_entries(
        conn: &Connection,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        let mut stmt = conn.prepare(sql).map_err(sqlite_error)?;
        let mut rows = stmt.query(params_from_iter(params)).map_err(sqlite_error)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_error)? {
            out.push(Self::row_to_entry(row)?);
        }
        Ok(out)
    }

    fn query_ids(
        conn: &Connection,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<Vec<String>, SemanticaError> {
        let mut stmt = conn.prepare(sql).map_err(sqlite_error)?;
        let mut rows = stmt.query(params_from_iter(params)).map_err(sqlite_error)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_error)? {
            out.push(column_text(row, 0)?.unwrap_or_default());
        }
        Ok(out)
    }

    fn chain_head_on(conn: &Connection) -> Result<Option<(i64, Option<String>)>, SemanticaError> {
        let mut stmt = conn.prepare(CHAIN_HEAD_QUERY).map_err(sqlite_error)?;
        let mut rows = stmt.query([]).map_err(sqlite_error)?;
        match rows.next().map_err(sqlite_error)? {
            Some(row) => Ok(Some((
                column_i64(row, 0)?.unwrap_or_default(),
                column_text(row, 1)?,
            ))),
            None => Ok(None),
        }
    }
}

impl ProvenanceStorage for SQLiteStorage {
    fn store(&self, entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
        self.transaction(&mut |conn| self.store_with_conn(conn, entry))
    }

    fn retrieve(&self, entity_id: &str) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        self.with_read_connection(|conn| {
            self.retrieve_with_conn(Some(&StorageConnection::Sqlite(conn)), entity_id)
        })
    }

    fn retrieve_all(
        &self,
        entity_type: Option<&str>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        self.with_read_connection(|conn| match entity_type {
            Some(kind) if !kind.is_empty() => Self::query_entries(
                conn,
                "
                    SELECT * FROM provenance WHERE entity_type = ?
                ",
                vec![sql_text(kind)],
            ),
            _ => Self::query_entries(conn, "SELECT * FROM provenance", Vec::new()),
        })
    }

    /// Batched BFS: each frontier is fetched with `IN (...)` queries of at most 999 ids, in
    /// exact BFS order.
    fn trace_lineage(
        &self,
        entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        self.with_read_connection(|conn| {
            let mut lineage = Vec::new();
            let mut visited: HashSet<String> = HashSet::new();
            let mut frontier: Vec<String> = vec![entity_id.to_string()];
            let mut depth = 0usize;
            while !frontier.is_empty() && max_depth.is_none_or(|max| depth < max) {
                let current: Vec<String> = frontier
                    .iter()
                    .cloned()
                    .collect::<IndexSet<String>>()
                    .into_iter()
                    .filter(|id| !visited.contains(id))
                    .collect();
                if current.is_empty() {
                    break;
                }
                visited.extend(current.iter().cloned());
                let mut found: HashMap<String, ProvenanceEntry> = HashMap::new();
                for chunk in current.chunks(IN_CHUNK) {
                    let sql = format!(
                        "
                        SELECT * FROM provenance WHERE entity_id IN ({})
                    ",
                        placeholders(chunk.len())
                    );
                    let params = chunk.iter().map(|id| sql_text(id)).collect();
                    for entry in Self::query_entries(conn, &sql, params)? {
                        found.insert(entry.entity_id.clone(), entry);
                    }
                }
                let mut next = Vec::new();
                for id in &current {
                    let Some(entry) = found.remove(id) else {
                        continue;
                    };
                    if let Some(parent) = entry.parent_entity_id.as_deref()
                        && !parent.is_empty()
                        && !visited.contains(parent)
                    {
                        next.push(parent.to_string());
                    }
                    for used in &entry.used_entities {
                        if !visited.contains(used) {
                            next.push(used.clone());
                        }
                    }
                    lineage.push(entry);
                }
                frontier = next;
                depth += 1;
            }
            Ok(lineage)
        })
    }

    fn clear(&self) -> Result<usize, SemanticaError> {
        let conn = self.connect()?;
        self.configure_connection(&conn)?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM provenance", [], |row| row.get(0))
            .map_err(sqlite_error)?;
        conn.execute_batch("DELETE FROM provenance")
            .map_err(sqlite_error)?;
        Ok(count as usize)
    }

    /// `BEGIN IMMEDIATE` on a fresh connection; commit when `body` succeeds, roll back when it
    /// fails (or the commit fails), and always close the connection.
    fn transaction(&self, body: &mut TransactionBody<'_>) -> Result<(), SemanticaError> {
        let conn = self.connect()?;
        let result = (|| {
            self.configure_connection(&conn)?;
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(sqlite_error)?;
            body(Some(&StorageConnection::Sqlite(&conn)))?;
            if !conn.is_autocommit() {
                conn.execute_batch("COMMIT").map_err(sqlite_error)?;
            }
            Ok(())
        })();
        if result.is_err() && !conn.is_autocommit() {
            let _ = conn.execute_batch("ROLLBACK");
        }
        drop(conn);
        result
    }

    /// A `SAVEPOINT` on the given connection for per-item rollback; without a connection, a
    /// full transaction.
    fn savepoint(
        &self,
        conn: Option<&StorageConnection<'_>>,
        body: &mut TransactionBody<'_>,
    ) -> Result<(), SemanticaError> {
        let Some(given) = conn else {
            return self.transaction(body);
        };
        let c = sqlite_conn(Some(given))?;
        let name = format!("sp_{}", uuid::Uuid::new_v4().simple());
        c.execute_batch(&format!("SAVEPOINT {name}"))
            .map_err(sqlite_error)?;
        let result = body(conn).and_then(|()| {
            c.execute_batch(&format!("RELEASE {name}"))
                .map_err(sqlite_error)
        });
        if result.is_err() {
            let _ = c
                .execute_batch(&format!("ROLLBACK TO {name}"))
                .and_then(|()| c.execute_batch(&format!("RELEASE {name}")));
        }
        result
    }

    fn store_with_conn(
        &self,
        conn: Option<&StorageConnection<'_>>,
        entry: &ProvenanceEntry,
    ) -> Result<(), SemanticaError> {
        let c = sqlite_conn(conn)?;
        c.execute(INSERT_ENTRY, params_from_iter(Self::entry_params(entry)))
            .map_err(sqlite_error)?;
        Ok(())
    }

    fn retrieve_with_conn(
        &self,
        conn: Option<&StorageConnection<'_>>,
        entity_id: &str,
    ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        let c = sqlite_conn(conn)?;
        Ok(Self::query_entries(
            c,
            "
            SELECT * FROM provenance WHERE entity_id = ?
        ",
            vec![sql_text(entity_id)],
        )?
        .into_iter()
        .next())
    }

    /// Reuses the caller's transaction connection when given, so reading the head and
    /// inserting the next entry happen under the same writer lock. Ties on `sequence_id` go to
    /// the most recently written row (`rowid DESC`).
    fn get_chain_head(
        &self,
        conn: Option<&StorageConnection<'_>>,
    ) -> Result<Option<(i64, Option<String>)>, SemanticaError> {
        match conn {
            Some(given) => Self::chain_head_on(sqlite_conn(Some(given))?),
            None => self.with_read_connection(Self::chain_head_on),
        }
    }

    /// Indexed reverse lookups on `parent_entity_id`/`previous_version_id`/`derived_from_id`
    /// plus an in-memory reverse index over the `used_entities` JSON column. Children of a
    /// level come in row order (the original iterates a `set`, whose order is arbitrary).
    fn trace_descendants(
        &self,
        entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        self.with_read_connection(|conn| {
            let mut used_reverse: HashMap<String, Vec<String>> = HashMap::new();
            {
                let mut stmt = conn
                    .prepare(
                        "SELECT entity_id, used_entities FROM provenance \
                         WHERE used_entities IS NOT NULL AND used_entities != '[]' ORDER BY rowid",
                    )
                    .map_err(sqlite_error)?;
                let mut rows = stmt.query([]).map_err(sqlite_error)?;
                while let Some(row) = rows.next().map_err(sqlite_error)? {
                    let child = column_text(row, 0)?.unwrap_or_default();
                    let used: Vec<Value> = match column_text(row, 1)? {
                        Some(text) if !text.is_empty() => {
                            match serde_json::from_str::<Value>(&text) {
                                Ok(Value::Array(items)) => items,
                                _ => Vec::new(),
                            }
                        }
                        _ => Vec::new(),
                    };
                    for uid in used {
                        if let Value::String(uid) = uid {
                            used_reverse.entry(uid).or_default().push(child.clone());
                        }
                    }
                }
            }

            let mut descendants = Vec::new();
            let mut visited: HashSet<String> = HashSet::from([entity_id.to_string()]);
            let mut frontier: Vec<String> = vec![entity_id.to_string()];
            let mut depth = 0usize;
            while !frontier.is_empty() && max_depth.is_none_or(|max| depth < max) {
                let current: Vec<String> = frontier
                    .iter()
                    .cloned()
                    .collect::<IndexSet<String>>()
                    .into_iter()
                    .collect();
                let mut child_ids: IndexSet<String> = IndexSet::new();
                for chunk in current.chunks(IN_CHUNK) {
                    let marks = placeholders(chunk.len());
                    let sql = format!(
                        "
                        SELECT entity_id FROM provenance
                        WHERE parent_entity_id IN ({marks})
                           OR previous_version_id IN ({marks})
                           OR derived_from_id IN ({marks})
                        ORDER BY rowid
                        "
                    );
                    let mut params: Vec<SqlValue> = Vec::with_capacity(chunk.len() * 3);
                    for _ in 0..3 {
                        params.extend(chunk.iter().map(|id| sql_text(id)));
                    }
                    child_ids.extend(Self::query_ids(conn, &sql, params)?);
                }
                for reference in &current {
                    if let Some(children) = used_reverse.get(reference) {
                        child_ids.extend(children.iter().cloned());
                    }
                }
                let next: Vec<String> = child_ids
                    .into_iter()
                    .filter(|id| !visited.contains(id))
                    .collect();
                if next.is_empty() {
                    break;
                }
                visited.extend(next.iter().cloned());
                let mut found: HashMap<String, ProvenanceEntry> = HashMap::new();
                for chunk in next.chunks(IN_CHUNK) {
                    let sql = format!(
                        "SELECT * FROM provenance WHERE entity_id IN ({})",
                        placeholders(chunk.len())
                    );
                    let params = chunk.iter().map(|id| sql_text(id)).collect();
                    for entry in Self::query_entries(conn, &sql, params)? {
                        found.insert(entry.entity_id.clone(), entry);
                    }
                }
                for id in &next {
                    if let Some(entry) = found.remove(id) {
                        descendants.push(entry);
                    }
                }
                frontier = next;
                depth += 1;
            }
            Ok(descendants)
        })
    }
}
