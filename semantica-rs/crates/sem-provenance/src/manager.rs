//! Unified provenance manager (port of `semantica/provenance/manager.py`).
//!
//! [`ProvenanceManager`] consolidates the provenance tracking of `kg.ProvenanceTracker`
//! (entities/relationships), `split.ProvenanceTracker` (chunks) and `conflicts.SourceTracker`
//! (property sources) behind one API: W3C PROV-O entries, lineage and descendant tracing,
//! hash-chained integrity, tombstone invalidation, batch tracking and PROV-O RDF export.
//!
//! Python keyword arguments become typed option structs: [`TrackEntityOptions`] for the
//! `**kwargs` of `track_entity`/`track_relationship`/`track_entities_batch`,
//! [`TrackChunkOptions`] for `track_chunk`, and [`TrackKwargs`] (an `agent`/`activity` record
//! plus the free-form `**metadata` map) for `track_property_source`/`track_chunks_batch`.
//! The private `_conn` argument is the `*_with_conn` variant of a method.

use std::marker::PhantomData;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;

use serde_json::{Map, Value};

use sem_utils::SemanticaError;
use sem_utils::logging::{Logger, get_logger};
use sem_utils::pycompat::fmt::{py_str, str_repr, truthy, type_name};

use crate::clock;
use crate::integrity::{compute_checksum, verify_checksum};
use crate::rdf::{self, ProvGraph, RDF_TYPE, datetime_literal};
use crate::schemas::{ActivityRecord, AgentRecord, ProvenanceEntry, SourceReference};
use crate::storage::{
    InMemoryStorage, ProvenanceStorage, SQLiteStorage, StorageConnection, TransactionBody,
};

/// Base URI that `export_prov` mints entity/agent/activity IRIs under, shared with the RDF
/// exporter's `semantica` namespace so both exports of the same `entity_id` co-resolve.
pub const DEFAULT_BASE_URI: &str = "https://semantica.dev/ns#";

// ---------------------------------------------------------------------------------------------
// class-level default storage path
// ---------------------------------------------------------------------------------------------

/// `threading.RLock()`: the class lock guarding `_default_storage_path`.
struct ReentrantLock {
    state: Mutex<(Option<ThreadId>, usize)>,
    released: Condvar,
}

impl ReentrantLock {
    const fn new() -> Self {
        ReentrantLock {
            state: Mutex::new((None, 0)),
            released: Condvar::new(),
        }
    }

    fn acquire(&self) {
        let me = std::thread::current().id();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            match state.0 {
                None => {
                    *state = (Some(me), 1);
                    return;
                }
                Some(owner) if owner == me => {
                    state.1 += 1;
                    return;
                }
                Some(_) => {
                    state = self.released.wait(state).unwrap_or_else(|e| e.into_inner());
                }
            }
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.1 > 0 {
            state.1 -= 1;
        }
        if state.1 == 0 {
            state.0 = None;
            self.released.notify_all();
        }
    }
}

struct DefaultPathState {
    path: Option<String>,
    stack: Vec<Option<String>>,
}

static CLASS_LOCK: ReentrantLock = ReentrantLock::new();
static DEFAULT_PATH: Mutex<DefaultPathState> = Mutex::new(DefaultPathState {
    path: None,
    stack: Vec::new(),
});

fn default_path_state() -> std::sync::MutexGuard<'static, DefaultPathState> {
    DEFAULT_PATH.lock().unwrap_or_else(|e| e.into_inner())
}

/// Guard returned by [`default_storage_path`]: while alive, new managers default to the given
/// SQLite path; dropping it restores the previous default (also when unwinding).
///
/// Like the original's context manager it holds the class lock for its whole lifetime, so
/// concurrent contexts in other threads (and `ProvenanceManager` construction there) wait.
/// Nested guards on one thread restore in LIFO order.
#[must_use = "the default storage path is restored when the guard is dropped"]
pub struct DefaultStoragePathGuard {
    _not_send: PhantomData<*const ()>,
}

impl DefaultStoragePathGuard {
    fn enter(path: Option<&str>) -> Self {
        CLASS_LOCK.acquire();
        let mut state = default_path_state();
        let previous = state.path.take();
        state.stack.push(previous);
        state.path = path.map(str::to_string);
        DefaultStoragePathGuard {
            _not_send: PhantomData,
        }
    }
}

impl Drop for DefaultStoragePathGuard {
    fn drop(&mut self) {
        {
            let mut state = default_path_state();
            state.path = state.stack.pop().unwrap_or(None);
        }
        CLASS_LOCK.release();
    }
}

/// `default_storage_path(path)`: temporarily set the global default storage path for new
/// managers; the previous value comes back when the returned guard is dropped.
pub fn default_storage_path(path: Option<&str>) -> DefaultStoragePathGuard {
    ProvenanceManager::default_storage_path(path)
}

// ---------------------------------------------------------------------------------------------
// keyword-argument structs
// ---------------------------------------------------------------------------------------------

/// Constructor arguments of `ProvenanceManager(storage=None, storage_path=None, config=None)`.
#[derive(Clone, Default)]
pub struct ProvenanceManagerOptions {
    /// A custom storage backend (takes precedence over everything else).
    pub storage: Option<Arc<dyn ProvenanceStorage>>,
    /// Path of a SQLite database (in-memory storage when unset and no default applies).
    pub storage_path: Option<String>,
    /// Configuration mapping: `provenance.storage_path`, else a top-level `storage_path`.
    pub config: Option<Value>,
}

/// The typed `**kwargs` of `track_entity`, `track_relationship` and `track_entities_batch`.
///
/// Unset fields take the original defaults (`entity_type="entity"`, `agent_id="semantica"`,
/// `agent_type="software_agent"`, `is_automated=True`, `confidence=1.0`, the method's default
/// `activity_id`). An `agent` record overrides `agent_id`/`agent_type`/`is_automated`; an
/// `activity` record overrides `activity_id` and the activity times.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackEntityOptions {
    pub entity_type: Option<String>,
    pub activity_id: Option<String>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub is_automated: Option<bool>,
    pub role: Option<String>,
    pub agent: Option<AgentRecord>,
    pub source_location: Option<String>,
    pub source_quote: Option<String>,
    pub confidence: Option<f64>,
    pub parent_entity_id: Option<String>,
    pub used_entities: Option<Vec<String>>,
    pub activity: Option<ActivityRecord>,
    pub activity_started_at_time: Option<String>,
    pub activity_ended_at_time: Option<String>,
    pub acted_on_behalf_of: Option<String>,
    pub informed_by: Option<Vec<String>>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub revision_type: Option<String>,
    pub supersedes: Option<String>,
    pub bundle_id: Option<String>,
}

macro_rules! string_setters {
    ($($name:ident),* $(,)?) => {
        $(
            #[doc = concat!("Set `", stringify!($name), "`.")]
            pub fn $name(mut self, value: impl Into<String>) -> Self {
                self.$name = Some(value.into());
                self
            }
        )*
    };
}

macro_rules! list_setters {
    ($($name:ident),* $(,)?) => {
        $(
            #[doc = concat!("Set `", stringify!($name), "`.")]
            pub fn $name<I, S>(mut self, values: I) -> Self
            where
                I: IntoIterator<Item = S>,
                S: Into<String>,
            {
                self.$name = Some(values.into_iter().map(Into::into).collect());
                self
            }
        )*
    };
}

fn kw_str(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(py_str(other)),
    }
}

fn kw_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(*b),
        other => Some(truthy(other)),
    }
}

fn kw_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// `list(value)` for a JSON value: list items, string characters or dict keys.
fn kw_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Array(items) => Some(
            items
                .iter()
                .map(|item| match item {
                    Value::String(s) => s.clone(),
                    other => py_str(other),
                })
                .collect(),
        ),
        Value::String(s) => Some(s.chars().map(String::from).collect()),
        Value::Object(map) => Some(map.keys().cloned().collect()),
        _ => None,
    }
}

impl TrackEntityOptions {
    /// All options unset.
    pub fn new() -> Self {
        Self::default()
    }

    string_setters!(
        entity_type,
        activity_id,
        agent_id,
        agent_type,
        role,
        source_location,
        source_quote,
        parent_entity_id,
        activity_started_at_time,
        activity_ended_at_time,
        acted_on_behalf_of,
        valid_from,
        valid_until,
        revision_type,
        supersedes,
        bundle_id,
    );

    list_setters!(used_entities, informed_by);

    /// Set `is_automated`.
    pub fn is_automated(mut self, value: bool) -> Self {
        self.is_automated = Some(value);
        self
    }

    /// Set `confidence`.
    pub fn confidence(mut self, value: f64) -> Self {
        self.confidence = Some(value);
        self
    }

    /// Set the `agent` record.
    pub fn agent(mut self, agent: AgentRecord) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Set the `activity` record.
    pub fn activity(mut self, activity: ActivityRecord) -> Self {
        self.activity = Some(activity);
        self
    }

    /// Fill an unset option from a Python-style keyword argument. Returns whether `key` is one
    /// of [`ProvenanceManager::TRACK_ENTITY_KWARGS`]. `agent`/`activity` values in a JSON map
    /// are never records, so (as in Python's `isinstance` check) they are ignored.
    pub fn apply_kwarg(&mut self, key: &str, value: &Value) -> bool {
        fn fill<T>(slot: &mut Option<T>, value: Option<T>) {
            if slot.is_none() {
                *slot = value;
            }
        }
        match key {
            "entity_type" => fill(&mut self.entity_type, kw_str(value)),
            "activity_id" => fill(&mut self.activity_id, kw_str(value)),
            "agent_id" => fill(&mut self.agent_id, kw_str(value)),
            "agent_type" => fill(&mut self.agent_type, kw_str(value)),
            "is_automated" => fill(&mut self.is_automated, kw_bool(value)),
            "role" => fill(&mut self.role, kw_str(value)),
            "source_location" => fill(&mut self.source_location, kw_str(value)),
            "source_quote" => fill(&mut self.source_quote, kw_str(value)),
            "confidence" => fill(&mut self.confidence, kw_f64(value)),
            "parent_entity_id" => fill(&mut self.parent_entity_id, kw_str(value)),
            "used_entities" => fill(&mut self.used_entities, kw_list(value)),
            "activity_started_at_time" => fill(&mut self.activity_started_at_time, kw_str(value)),
            "activity_ended_at_time" => fill(&mut self.activity_ended_at_time, kw_str(value)),
            "acted_on_behalf_of" => fill(&mut self.acted_on_behalf_of, kw_str(value)),
            "informed_by" => fill(&mut self.informed_by, kw_list(value)),
            "valid_from" => fill(&mut self.valid_from, kw_str(value)),
            "valid_until" => fill(&mut self.valid_until, kw_str(value)),
            "revision_type" => fill(&mut self.revision_type, kw_str(value)),
            "supersedes" => fill(&mut self.supersedes, kw_str(value)),
            "bundle_id" => fill(&mut self.bundle_id, kw_str(value)),
            "agent" | "activity" => {}
            _ => return false,
        }
        true
    }

    /// Split a Python-style `**kwargs` map into the typed options and the remaining free-form
    /// keys (what `track_entities_batch` does with its `**metadata`).
    pub fn from_kwargs(kwargs: &Map<String, Value>) -> (TrackEntityOptions, Map<String, Value>) {
        let mut options = TrackEntityOptions::default();
        let mut rest = Map::new();
        for (key, value) in kwargs {
            if !options.apply_kwarg(key, value) {
                rest.insert(key.clone(), value.clone());
            }
        }
        (options, rest)
    }
}

/// `agent=` / `activity=` records plus the free-form `**metadata` keyword arguments of
/// `track_property_source` and `track_chunks_batch`.
///
/// The scalar agent/activity keys (`agent_id`, `agent_type`, `is_automated`, `role`,
/// `activity_id`, `activity_started_at_time`, `activity_ended_at_time`) may be given in
/// `metadata`, exactly as Python callers pass them; they populate the entry's fields and
/// (except `activity_id`, which the original keeps) are removed from the stored metadata.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackKwargs {
    pub agent: Option<AgentRecord>,
    pub activity: Option<ActivityRecord>,
    pub metadata: Map<String, Value>,
}

impl TrackKwargs {
    /// Only free-form keyword arguments.
    pub fn from_metadata(metadata: Map<String, Value>) -> Self {
        TrackKwargs {
            metadata,
            ..TrackKwargs::default()
        }
    }
}

/// Arguments of `track_chunk(chunk_id, source_document, source_path=None, start_index=0,
/// end_index=0, parent_chunk_id=None, **metadata)` after the two positional ones.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackChunkOptions {
    pub source_path: Option<String>,
    /// Default `Some(0)`.
    pub start_index: Option<i64>,
    /// Default `Some(0)`.
    pub end_index: Option<i64>,
    pub parent_chunk_id: Option<String>,
    pub agent: Option<AgentRecord>,
    pub activity: Option<ActivityRecord>,
    /// The `**metadata` keyword arguments (see [`TrackKwargs`] for the agent/activity keys).
    pub metadata: Map<String, Value>,
}

impl Default for TrackChunkOptions {
    fn default() -> Self {
        TrackChunkOptions {
            source_path: None,
            start_index: Some(0),
            end_index: Some(0),
            parent_chunk_id: None,
            agent: None,
            activity: None,
            metadata: Map::new(),
        }
    }
}

impl TrackChunkOptions {
    /// Defaults (`start_index=0`, `end_index=0`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `source_path`.
    pub fn source_path(mut self, value: impl Into<String>) -> Self {
        self.source_path = Some(value.into());
        self
    }

    /// Set `start_index` and `end_index`.
    pub fn span(mut self, start_index: i64, end_index: i64) -> Self {
        self.start_index = Some(start_index);
        self.end_index = Some(end_index);
        self
    }

    /// Set `parent_chunk_id`.
    pub fn parent_chunk_id(mut self, value: impl Into<String>) -> Self {
        self.parent_chunk_id = Some(value.into());
        self
    }

    /// Set the free-form `**metadata`.
    pub fn metadata(mut self, metadata: Map<String, Value>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// `audit_log()`'s result: text for `table`/`csv`, records for `json`.
#[derive(Debug, Clone, PartialEq)]
pub enum AuditLog {
    /// The `table` or `csv` rendering.
    Text(String),
    /// The `json` format: one `to_dict()` per entry.
    Records(Vec<Map<String, Value>>),
}

impl AuditLog {
    /// The text, if this is a `table`/`csv` log.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            AuditLog::Text(text) => Some(text),
            AuditLog::Records(_) => None,
        }
    }

    /// The records, if this is a `json` log.
    pub fn as_records(&self) -> Option<&[Map<String, Value>]> {
        match self {
            AuditLog::Records(records) => Some(records),
            AuditLog::Text(_) => None,
        }
    }

    /// As a JSON value (a string or a list of dicts).
    pub fn to_value(&self) -> Value {
        match self {
            AuditLog::Text(text) => Value::String(text.clone()),
            AuditLog::Records(records) => {
                Value::Array(records.iter().cloned().map(Value::Object).collect())
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// ProvenanceManager
// ---------------------------------------------------------------------------------------------

/// Unified provenance tracking manager (`semantica.provenance.ProvenanceManager`).
///
/// Cloning is cheap and shares the storage backend (Python passes the object by reference).
#[derive(Clone)]
pub struct ProvenanceManager {
    storage: Arc<dyn ProvenanceStorage>,
    logger: Logger,
}

impl std::fmt::Debug for ProvenanceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = if self.storage().is::<InMemoryStorage>() {
            "InMemoryStorage"
        } else if self.storage().is::<SQLiteStorage>() {
            "SQLiteStorage"
        } else {
            "custom"
        };
        f.debug_struct("ProvenanceManager")
            .field("storage", &kind)
            .finish()
    }
}

/// Keys `track_chunk`/`track_property_source` pop from `**metadata` after resolving the agent
/// and activity (note: not `activity_id`).
const POPPED_KWARGS: [&str; 8] = [
    "agent_id",
    "agent_type",
    "is_automated",
    "role",
    "agent",
    "activity",
    "activity_started_at_time",
    "activity_ended_at_time",
];

struct AgentInfo {
    agent_id: String,
    agent_type: String,
    is_automated: bool,
    role: Option<String>,
}

struct ActivityInfo {
    activity_id: String,
    started: Option<String>,
    ended: Option<String>,
}

/// `_resolve_agent_kwargs`.
fn resolve_agent(options: &TrackEntityOptions) -> AgentInfo {
    match &options.agent {
        Some(agent) => AgentInfo {
            agent_id: agent.id.clone(),
            agent_type: agent.agent_type.clone(),
            is_automated: agent.is_automated,
            role: options.role.clone(),
        },
        None => AgentInfo {
            agent_id: options
                .agent_id
                .clone()
                .unwrap_or_else(|| "semantica".to_string()),
            agent_type: options
                .agent_type
                .clone()
                .unwrap_or_else(|| "software_agent".to_string()),
            is_automated: options.is_automated.unwrap_or(true),
            role: options.role.clone(),
        },
    }
}

/// `_resolve_activity_kwargs`.
fn resolve_activity(options: &TrackEntityOptions, default_activity_id: &str) -> ActivityInfo {
    match &options.activity {
        Some(activity) => ActivityInfo {
            activity_id: activity.id.clone(),
            started: activity.started_at_time.clone(),
            ended: activity.ended_at_time.clone(),
        },
        None => ActivityInfo {
            activity_id: options
                .activity_id
                .clone()
                .unwrap_or_else(|| default_activity_id.to_string()),
            started: options.activity_started_at_time.clone(),
            ended: options.activity_ended_at_time.clone(),
        },
    }
}

/// Resolve agent/activity from `**metadata`-style keyword arguments and pop the agent keys.
fn resolve_from_kwargs(
    agent: Option<AgentRecord>,
    activity: Option<ActivityRecord>,
    metadata: &mut Map<String, Value>,
    default_activity_id: &str,
) -> (AgentInfo, ActivityInfo) {
    let mut options = TrackEntityOptions {
        agent,
        activity,
        ..TrackEntityOptions::default()
    };
    for key in [
        "agent_id",
        "agent_type",
        "is_automated",
        "role",
        "activity_id",
        "activity_started_at_time",
        "activity_ended_at_time",
    ] {
        if let Some(value) = metadata.get(key) {
            options.apply_kwarg(key, value);
        }
    }
    for key in POPPED_KWARGS {
        metadata.shift_remove(key);
    }
    (
        resolve_agent(&options),
        resolve_activity(&options, default_activity_id),
    )
}

/// `if not value` for an optional string.
fn falsy(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}

/// Python's `entries[:end]` bound for a list of `len` items.
fn slice_end(len: usize, end: i64) -> usize {
    if end >= 0 {
        (end as usize).min(len)
    } else {
        len.saturating_sub(end.unsigned_abs() as usize)
    }
}

/// `{**a, **b}` where `b` must be a mapping (Python raises `TypeError` otherwise).
fn merge_mapping(
    base: &Map<String, Value>,
    extra: Option<&Value>,
) -> Result<Map<String, Value>, SemanticaError> {
    let mut merged = base.clone();
    match extra {
        None => {}
        Some(Value::Object(map)) => {
            for (key, value) in map {
                merged.insert(key.clone(), value.clone());
            }
        }
        Some(other) => {
            return Err(SemanticaError::Type(format!(
                "'{}' object is not a mapping",
                type_name(other)
            )));
        }
    }
    Ok(merged)
}

fn entry_dicts(entries: &[ProvenanceEntry]) -> Value {
    Value::Array(entries.iter().map(|e| Value::Object(e.to_dict())).collect())
}

impl ProvenanceManager {
    /// The keyword arguments `track_entity` recognises (`_TRACK_ENTITY_KWARGS`); in
    /// `track_entities_batch` these are forwarded as typed options, every other key is
    /// free-form metadata.
    pub const TRACK_ENTITY_KWARGS: [&'static str; 22] = [
        "entity_type",
        "activity_id",
        "agent_id",
        "agent_type",
        "is_automated",
        "role",
        "agent",
        "source_location",
        "source_quote",
        "confidence",
        "parent_entity_id",
        "used_entities",
        "activity",
        "activity_started_at_time",
        "activity_ended_at_time",
        "acted_on_behalf_of",
        "informed_by",
        "valid_from",
        "valid_until",
        "revision_type",
        "supersedes",
        "bundle_id",
    ];

    /// Number of entries tracked per shared transaction in the batch methods.
    pub const BATCH_SIZE: usize = 1000;

    /// `ProvenanceManager()`: SQLite storage at the global default path when one is set,
    /// in-memory storage otherwise.
    pub fn new() -> Result<Self, SemanticaError> {
        Self::with_options(ProvenanceManagerOptions::default())
    }

    /// `ProvenanceManager(storage=..., storage_path=..., config=...)`.
    pub fn with_options(options: ProvenanceManagerOptions) -> Result<Self, SemanticaError> {
        let logger = get_logger("provenance_manager");
        if let Some(storage) = options.storage {
            return Ok(ProvenanceManager { storage, logger });
        }
        let mut storage_path: Option<Value> = options.storage_path.map(Value::String);
        let path_given = storage_path.as_ref().is_some_and(truthy);
        if let Some(Value::Object(config)) = &options.config
            && !path_given
            && !config.is_empty()
        {
            let section = config.get("provenance");
            let section_has_path =
                matches!(section, Some(Value::Object(s)) if s.contains_key("storage_path"));
            if section_has_path {
                storage_path = section.and_then(|s| s.get("storage_path")).cloned();
            } else if config.contains_key("storage_path") {
                storage_path = config.get("storage_path").cloned();
            }
        }
        if !storage_path.as_ref().is_some_and(truthy) {
            CLASS_LOCK.acquire();
            storage_path = default_path_state().path.clone().map(Value::String);
            CLASS_LOCK.release();
        }
        let storage: Arc<dyn ProvenanceStorage> = match storage_path {
            Some(Value::String(path)) if !path.is_empty() => Arc::new(SQLiteStorage::new(path)?),
            Some(other) if truthy(&other) => {
                return Err(SemanticaError::Type(format!(
                    "expected str, bytes or os.PathLike object, not {}",
                    type_name(&other)
                )));
            }
            _ => Arc::new(InMemoryStorage::new()),
        };
        Ok(ProvenanceManager { storage, logger })
    }

    /// `ProvenanceManager(storage=storage)`.
    pub fn with_storage(storage: impl ProvenanceStorage + 'static) -> Self {
        Self::with_shared_storage(Arc::new(storage))
    }

    /// `ProvenanceManager(storage=storage)` for a backend shared with other owners.
    pub fn with_shared_storage(storage: Arc<dyn ProvenanceStorage>) -> Self {
        ProvenanceManager {
            storage,
            logger: get_logger("provenance_manager"),
        }
    }

    /// `ProvenanceManager(storage_path=path)`.
    pub fn with_storage_path(path: impl Into<String>) -> Result<Self, SemanticaError> {
        Self::with_options(ProvenanceManagerOptions {
            storage_path: Some(path.into()),
            ..ProvenanceManagerOptions::default()
        })
    }

    /// `ProvenanceManager(config=config)`.
    pub fn with_config(config: Value) -> Result<Self, SemanticaError> {
        Self::with_options(ProvenanceManagerOptions {
            config: Some(config),
            ..ProvenanceManagerOptions::default()
        })
    }

    /// `ProvenanceManager.set_default_storage_path(path)`: the global default for new managers.
    pub fn set_default_storage_path(path: Option<&str>) {
        CLASS_LOCK.acquire();
        default_path_state().path = path.map(str::to_string);
        CLASS_LOCK.release();
    }

    /// `with ProvenanceManager.default_storage_path(path):` — see [`DefaultStoragePathGuard`].
    pub fn default_storage_path(path: Option<&str>) -> DefaultStoragePathGuard {
        DefaultStoragePathGuard::enter(path)
    }

    /// The current global default storage path (`ProvenanceManager._default_storage_path`).
    pub fn current_default_storage_path() -> Option<String> {
        default_path_state().path.clone()
    }

    /// The storage backend (`manager.storage`).
    pub fn storage(&self) -> &(dyn ProvenanceStorage + 'static) {
        &*self.storage
    }

    /// The shared handle to the storage backend.
    pub fn storage_arc(&self) -> Arc<dyn ProvenanceStorage> {
        Arc::clone(&self.storage)
    }

    /// Replace the storage backend (`manager.storage = ...`).
    pub fn set_storage(&mut self, storage: Arc<dyn ProvenanceStorage>) {
        self.storage = storage;
    }

    /// The `semantica.provenance_manager` logger.
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `_save_entry`: link the entry to the chain head, compute its checksum and store it
    /// (with `conn` inside a transaction). On a storage failure the error is logged; it is
    /// returned when `raise_on_error`, otherwise the result is `Ok(None)`.
    pub fn save_entry(
        &self,
        entry: &mut ProvenanceEntry,
        conn: Option<&StorageConnection<'_>>,
        raise_on_error: bool,
    ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        let head = self.storage.get_chain_head(conn).unwrap_or(None);
        match head {
            Some((sequence_id, checksum)) => {
                entry.sequence_id = Some(sequence_id + 1);
                entry.previous_checksum = checksum;
            }
            None => {
                entry.sequence_id = Some(1);
                entry.previous_checksum = None;
            }
        }
        entry.checksum = Some(compute_checksum(entry));
        let stored = match conn {
            Some(c) => self.storage.store_with_conn(Some(c), entry),
            None => self.storage.store(entry),
        };
        if let Err(err) = stored {
            self.logger.error(format!(
                "Failed to save provenance entry for entity '{}': {}",
                entry.entity_id, err
            ));
            if raise_on_error {
                return Err(err);
            }
            return Ok(None);
        }
        Ok(Some(entry.clone()))
    }

    /// `_get_or_create_transaction(conn)`: run `body` with `conn` when one is given, otherwise
    /// inside a new storage transaction.
    pub fn get_or_create_transaction(
        &self,
        conn: Option<&StorageConnection<'_>>,
        body: &mut TransactionBody<'_>,
    ) -> Result<(), SemanticaError> {
        match conn {
            Some(c) => body(Some(c)),
            None => self.storage.transaction(body),
        }
    }

    // === Entity tracking (kg.ProvenanceTracker compatible) ===

    /// Track entity provenance.
    ///
    /// Re-tracking an entity archives the previous version under `"<id>:v:<last_updated>"`
    /// (`previous_version_id`); an explicit parent (`parent_entity_id`, then
    /// `metadata["derived_from"]`, then `source` when it names a tracked entity) becomes
    /// `derived_from_id`. All reads and writes share one transaction. On failure the error is
    /// logged and the pre-failure state is returned: a copy of the previous entry, or `None`
    /// for a new entity.
    pub fn track_entity(
        &self,
        entity_id: &str,
        source: &str,
        metadata: Option<Map<String, Value>>,
        options: TrackEntityOptions,
    ) -> Option<ProvenanceEntry> {
        self.track_entity_with_conn(entity_id, source, metadata, options, None)
            .ok()
            .flatten()
    }

    /// `track_entity(..., _conn=conn)`: with a connection, failures propagate as errors (the
    /// batch methods count only items that did not fail); without one this is
    /// [`ProvenanceManager::track_entity`].
    pub fn track_entity_with_conn(
        &self,
        entity_id: &str,
        source: &str,
        metadata: Option<Map<String, Value>>,
        options: TrackEntityOptions,
        conn: Option<&StorageConnection<'_>>,
    ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        let storage = &*self.storage;
        let mut existing: Option<ProvenanceEntry> = None;
        let mut saved: Option<ProvenanceEntry> = None;
        let result = self.get_or_create_transaction(conn, &mut |conn| {
            existing = storage.retrieve_with_conn(conn, entity_id)?;
            let mut parent_id = options.parent_entity_id.clone();

            if falsy(&parent_id)
                && let Some(meta) = metadata.as_ref().filter(|m| !m.is_empty())
                && let Some(Value::String(derived)) = meta.get("derived_from")
                && !derived.is_empty()
            {
                parent_id = Some(derived.clone());
            }
            if falsy(&parent_id)
                && !source.is_empty()
                && let Ok(Some(_)) = storage.retrieve_with_conn(conn, source)
            {
                parent_id = Some(source.to_string());
            }
            let explicit_parent_supplied = parent_id.is_some();

            let mut archived_history_id: Option<String> = None;
            if let Some(previous) = &existing {
                let mut history = previous.clone();
                let base = format!(
                    "{entity_id}:v:{}",
                    previous.last_updated.as_deref().unwrap_or("None")
                );
                let mut history_id = base.clone();
                let mut counter = 1;
                while storage.retrieve_with_conn(conn, &history_id)?.is_some() {
                    history_id = format!("{base}:{counter}");
                    counter += 1;
                }
                // Pure relabel: checksum, sequence_id and previous_checksum stay as they were.
                history.entity_id = history_id.clone();
                storage.store_with_conn(conn, &history)?;
                archived_history_id = Some(history_id.clone());
                if !explicit_parent_supplied {
                    parent_id = Some(history_id);
                }
            }

            let agent = resolve_agent(&options);
            let activity = resolve_activity(&options, "entity_tracking");
            let first_seen = match &existing {
                Some(previous) => previous.first_seen.clone(),
                None => Some(clock::utcnow_isoformat()),
            };
            let last_updated = Some(clock::utcnow_isoformat());
            let timestamp = clock::utcnow_isoformat();

            let mut entry = ProvenanceEntry::with_timestamp(
                entity_id,
                options
                    .entity_type
                    .clone()
                    .unwrap_or_else(|| "entity".to_string()),
                activity.activity_id,
                timestamp,
            );
            entry.agent_id = agent.agent_id;
            entry.agent_type = agent.agent_type;
            entry.is_automated = agent.is_automated;
            entry.role = agent.role;
            entry.source_document = source.to_string();
            entry.source_location = options.source_location.clone();
            entry.source_quote = options.source_quote.clone();
            entry.confidence = options.confidence.unwrap_or(1.0);
            entry.metadata = metadata.clone().unwrap_or_default();
            entry.first_seen = first_seen;
            entry.last_updated = last_updated;
            entry.parent_entity_id = parent_id.clone();
            entry.used_entities = options.used_entities.clone().unwrap_or_default();
            entry.activity_started_at_time = activity.started;
            entry.activity_ended_at_time = activity.ended;
            entry.acted_on_behalf_of = options.acted_on_behalf_of.clone();
            entry.informed_by_activities = options.informed_by.clone().unwrap_or_default();
            entry.valid_from = options.valid_from.clone();
            entry.valid_until = options.valid_until.clone();
            entry.revision_type = options.revision_type.clone();
            entry.supersedes = options.supersedes.clone();
            entry.bundle_id = options.bundle_id.clone();

            entry.previous_version_id = archived_history_id.clone();
            if explicit_parent_supplied {
                entry.derived_from_id = parent_id.clone();
                if let Some(history_id) = &archived_history_id {
                    entry.used_entities.push(history_id.clone());
                }
            }

            self.save_entry(&mut entry, conn, true)?;
            saved = Some(entry);
            Ok(())
        });
        match result {
            Ok(()) => Ok(saved),
            Err(err) => {
                if conn.is_some() {
                    return Err(err);
                }
                self.logger.error(format!(
                    "Failed to track entity '{}' (transaction rolled back): {}. Returning pre-failure state ({}).",
                    entity_id,
                    err,
                    if existing.is_some() {
                        "existing entry"
                    } else {
                        "None (no prior entry existed)"
                    }
                ));
                Ok(existing)
            }
        }
    }

    /// Track relationship provenance (`entity_type="relationship"`, default activity
    /// `relationship_tracking`). Reads the agent/activity options, `source_location`,
    /// `confidence` and the Tier 2/3 fields; returns `None` when storage fails.
    pub fn track_relationship(
        &self,
        relationship_id: &str,
        source: &str,
        metadata: Option<Map<String, Value>>,
        options: TrackEntityOptions,
    ) -> Option<ProvenanceEntry> {
        let agent = resolve_agent(&options);
        let activity = resolve_activity(&options, "relationship_tracking");
        let first_seen = clock::utcnow_isoformat();
        let last_updated = clock::utcnow_isoformat();
        let timestamp = clock::utcnow_isoformat();
        let mut entry = ProvenanceEntry::with_timestamp(
            relationship_id,
            "relationship",
            activity.activity_id,
            timestamp,
        );
        entry.agent_id = agent.agent_id;
        entry.agent_type = agent.agent_type;
        entry.is_automated = agent.is_automated;
        entry.role = agent.role;
        entry.source_document = source.to_string();
        entry.source_location = options.source_location.clone();
        entry.confidence = options.confidence.unwrap_or(1.0);
        entry.metadata = metadata.unwrap_or_default();
        entry.first_seen = Some(first_seen);
        entry.last_updated = Some(last_updated);
        entry.activity_started_at_time = activity.started;
        entry.activity_ended_at_time = activity.ended;
        entry.acted_on_behalf_of = options.acted_on_behalf_of.clone();
        entry.informed_by_activities = options.informed_by.clone().unwrap_or_default();
        entry.valid_from = options.valid_from.clone();
        entry.valid_until = options.valid_until.clone();
        entry.revision_type = options.revision_type.clone();
        entry.supersedes = options.supersedes.clone();
        entry.bundle_id = options.bundle_id.clone();
        self.save_entry(&mut entry, None, false).ok().flatten()
    }

    // === Chunk tracking (split.ProvenanceTracker compatible) ===

    /// Track chunk provenance (`entity_type="chunk"`, default activity `chunking`). A parent
    /// chunk becomes both `parent_entity_id` and `derived_from_id`. Returns `None` when
    /// storage fails.
    pub fn track_chunk(
        &self,
        chunk_id: &str,
        source_document: &str,
        options: TrackChunkOptions,
    ) -> Option<ProvenanceEntry> {
        self.track_chunk_with_conn(chunk_id, source_document, options, None)
            .ok()
            .flatten()
    }

    /// `track_chunk(..., _conn=conn)`: with a connection, storage failures propagate.
    pub fn track_chunk_with_conn(
        &self,
        chunk_id: &str,
        source_document: &str,
        options: TrackChunkOptions,
        conn: Option<&StorageConnection<'_>>,
    ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        let TrackChunkOptions {
            source_path,
            start_index,
            end_index,
            parent_chunk_id,
            agent,
            activity,
            mut metadata,
        } = options;
        let (agent, activity) = resolve_from_kwargs(agent, activity, &mut metadata, "chunking");
        let timestamp = clock::utcnow_isoformat();
        let mut entry =
            ProvenanceEntry::with_timestamp(chunk_id, "chunk", activity.activity_id, timestamp);
        entry.agent_id = agent.agent_id;
        entry.agent_type = agent.agent_type;
        entry.is_automated = agent.is_automated;
        entry.role = agent.role;
        entry.source_document = source_document.to_string();
        entry.source_location = source_path;
        entry.start_index = start_index;
        entry.end_index = end_index;
        entry.parent_entity_id = parent_chunk_id.clone();
        // A chunk split from a parent chunk is a derivation, not a correction.
        entry.derived_from_id = parent_chunk_id;
        entry.metadata = metadata;
        entry.activity_started_at_time = activity.started;
        entry.activity_ended_at_time = activity.ended;
        self.save_entry(&mut entry, conn, conn.is_some())
    }

    // === Source tracking (conflicts.SourceTracker compatible) ===

    /// Track a property's source: entry `"{entity_id}_{property_name}"` of type `property`
    /// (default activity `property_tracking`), located at `page_<n>` or the source's section,
    /// with the source's confidence and `credibility` metadata. Returns `None` when storage
    /// fails.
    pub fn track_property_source(
        &self,
        entity_id: &str,
        property_name: &str,
        value: impl Into<Value>,
        source: &SourceReference,
        kwargs: TrackKwargs,
    ) -> Option<ProvenanceEntry> {
        let TrackKwargs {
            agent,
            activity,
            mut metadata,
        } = kwargs;
        let (agent, activity) =
            resolve_from_kwargs(agent, activity, &mut metadata, "property_tracking");
        let source_location = match source.page {
            Some(page) if page != 0 => Some(format!("page_{page}")),
            _ => source.section.clone(),
        };
        let credibility = source.metadata.get("credibility").and_then(Value::as_f64);
        let mut merged = Map::new();
        merged.insert("entity_id".into(), Value::String(entity_id.to_string()));
        merged.insert(
            "property_name".into(),
            Value::String(property_name.to_string()),
        );
        merged.insert("value".into(), value.into());
        for (key, value) in metadata.into_iter().chain(source.metadata.clone()) {
            merged.insert(key, value);
        }
        let timestamp = clock::utcnow_isoformat();
        let mut entry = ProvenanceEntry::with_timestamp(
            format!("{entity_id}_{property_name}"),
            "property",
            activity.activity_id,
            timestamp,
        );
        entry.agent_id = agent.agent_id;
        entry.agent_type = agent.agent_type;
        entry.is_automated = agent.is_automated;
        entry.role = agent.role;
        entry.source_document = source.document.clone();
        entry.source_location = source_location;
        entry.confidence = source.confidence;
        entry.credibility = credibility;
        entry.metadata = merged;
        entry.activity_started_at_time = activity.started;
        entry.activity_ended_at_time = activity.ended;
        self.save_entry(&mut entry, None, false).ok().flatten()
    }

    // === Batch operations ===

    /// Track many entities (`{"id": ...}` or `{"entity_id": ...}` dicts, with an optional
    /// `"metadata"` dict each) from one source. Every 1,000 entities share one transaction and
    /// each entity gets its own savepoint; the count includes only entities whose savepoint
    /// committed inside a transaction that committed.
    ///
    /// `metadata` is the original `**metadata`: keys in [`Self::TRACK_ENTITY_KWARGS`] fill the
    /// typed `options`, the others are merged into each entity's metadata.
    pub fn track_entities_batch(
        &self,
        entities: &[Map<String, Value>],
        source: &str,
        options: TrackEntityOptions,
        metadata: Map<String, Value>,
    ) -> usize {
        let mut options = options;
        let mut free_metadata = Map::new();
        for (key, value) in metadata {
            if !options.apply_kwarg(&key, &value) {
                free_metadata.insert(key, value);
            }
        }
        let storage = &*self.storage;
        let mut tracked = 0usize;
        for batch in entities.chunks(Self::BATCH_SIZE) {
            let mut batch_count = 0usize;
            let result = storage.transaction(&mut |conn| {
                batch_count = 0;
                for entity in batch {
                    let id_value = match entity.get("id") {
                        Some(value) if truthy(value) => Some(value),
                        _ => entity.get("entity_id"),
                    };
                    let Some(id_value) = id_value.filter(|v| truthy(v)) else {
                        continue;
                    };
                    let entity_metadata = merge_mapping(&free_metadata, entity.get("metadata"))?;
                    let item = storage.savepoint(conn, &mut |_| {
                        let Value::String(entity_id) = id_value else {
                            return Err(SemanticaError::Type(format!(
                                "entity_id must be a string, got {}",
                                type_name(id_value)
                            )));
                        };
                        self.track_entity_with_conn(
                            entity_id,
                            source,
                            Some(entity_metadata.clone()),
                            options.clone(),
                            conn,
                        )?;
                        Ok(())
                    });
                    if item.is_ok() {
                        batch_count += 1;
                    }
                }
                Ok(())
            });
            match result {
                Ok(()) => tracked += batch_count,
                Err(err) => self.logger.error(format!(
                    "Block-level storage transaction failed in track_entities_batch: {err}"
                )),
            }
        }
        tracked
    }

    /// Track many chunks (`{"id"|"chunk_id", "start_index", "end_index", "parent_chunk_id",
    /// "metadata"}` dicts) of one document, with the same transaction/savepoint/count
    /// semantics as [`Self::track_entities_batch`]. `kwargs` (agent/activity records and the
    /// `**metadata` keys) apply to every chunk, merged under each chunk's own `metadata`.
    pub fn track_chunks_batch(
        &self,
        chunks: &[Map<String, Value>],
        source_document: &str,
        source_path: Option<&str>,
        kwargs: TrackKwargs,
    ) -> usize {
        const NAMED: [&str; 8] = [
            "self",
            "chunk_id",
            "source_document",
            "source_path",
            "start_index",
            "end_index",
            "parent_chunk_id",
            "_conn",
        ];
        let storage = &*self.storage;
        let mut tracked = 0usize;
        for batch in chunks.chunks(Self::BATCH_SIZE) {
            let mut batch_count = 0usize;
            let result = storage.transaction(&mut |conn| {
                batch_count = 0;
                for chunk in batch {
                    let id_value = match chunk.get("id") {
                        Some(value) if truthy(value) => Some(value),
                        _ => chunk.get("chunk_id"),
                    };
                    let Some(id_value) = id_value.filter(|v| truthy(v)) else {
                        continue;
                    };
                    let item = storage.savepoint(conn, &mut |_| {
                        let merged = merge_mapping(&kwargs.metadata, chunk.get("metadata"))?;
                        if let Some(name) = merged.keys().find(|k| NAMED.contains(&k.as_str())) {
                            return Err(SemanticaError::Type(format!(
                                "ProvenanceManager.track_chunk() got multiple values for keyword argument '{name}'"
                            )));
                        }
                        let index = |key: &str| -> Result<Option<i64>, SemanticaError> {
                            match chunk.get(key) {
                                None => Ok(Some(0)),
                                Some(Value::Null) => Ok(None),
                                Some(Value::Number(n)) if n.as_i64().is_some() => Ok(n.as_i64()),
                                Some(other) => Err(SemanticaError::Type(format!(
                                    "{key} must be an int, got {}",
                                    type_name(other)
                                ))),
                            }
                        };
                        let options = TrackChunkOptions {
                            source_path: source_path.map(str::to_string),
                            start_index: index("start_index")?,
                            end_index: index("end_index")?,
                            parent_chunk_id: chunk.get("parent_chunk_id").and_then(kw_str),
                            agent: kwargs.agent.clone(),
                            activity: kwargs.activity.clone(),
                            metadata: merged,
                        };
                        let chunk_id = match id_value {
                            Value::String(s) => s.clone(),
                            other => py_str(other),
                        };
                        self.track_chunk_with_conn(&chunk_id, source_document, options, conn)?;
                        Ok(())
                    });
                    if item.is_ok() {
                        batch_count += 1;
                    }
                }
                Ok(())
            });
            match result {
                Ok(()) => tracked += batch_count,
                Err(err) => self.logger.error(format!(
                    "Block-level storage transaction failed in track_chunks_batch: {err}"
                )),
            }
        }
        tracked
    }

    // === Lineage retrieval ===

    /// Complete lineage of an entity (empty map when it was never tracked): `entity_id`,
    /// `lineage_chain`/`entries` (BFS from the entity through parents and used entities),
    /// `source_documents`, `first_seen`, `last_updated`, `entity_count`, the aggregated
    /// `metadata` (the queried entity's keys win) and `integrity_verified`.
    pub fn get_lineage(&self, entity_id: &str) -> Result<Map<String, Value>, SemanticaError> {
        let lineage = self.storage.trace_lineage(entity_id, None)?;
        if lineage.is_empty() {
            return Ok(Map::new());
        }
        let mut aggregated = Map::new();
        for entry in lineage.iter().rev() {
            for (key, value) in &entry.metadata {
                aggregated.insert(key.clone(), value.clone());
            }
        }
        let integrity_verified = lineage.iter().all(|e| verify_checksum(e, None));
        let chain = entry_dicts(&lineage);
        let mut sources: Vec<Value> = Vec::new();
        for entry in &lineage {
            let doc = Value::String(entry.source_document.clone());
            if !entry.source_document.is_empty() && !sources.contains(&doc) {
                sources.push(doc);
            }
        }
        let first_seen = lineage
            .iter()
            .filter_map(|e| e.first_seen.as_ref().filter(|s| !s.is_empty()))
            .min()
            .cloned();
        let last_updated = lineage
            .iter()
            .filter_map(|e| e.last_updated.as_ref().filter(|s| !s.is_empty()))
            .max()
            .cloned();
        let mut out = Map::new();
        out.insert("entity_id".into(), Value::String(entity_id.to_string()));
        out.insert("lineage_chain".into(), chain.clone());
        out.insert("entries".into(), chain);
        out.insert("source_documents".into(), Value::Array(sources));
        out.insert(
            "first_seen".into(),
            first_seen.map(Value::String).unwrap_or(Value::Null),
        );
        out.insert(
            "last_updated".into(),
            last_updated.map(Value::String).unwrap_or(Value::Null),
        );
        out.insert("entity_count".into(), Value::from(lineage.len()));
        out.insert("metadata".into(), Value::Object(aggregated));
        out.insert("integrity_verified".into(), Value::Bool(integrity_verified));
        Ok(out)
    }

    /// Raw lineage entries, optionally limited to `max_depth` BFS levels (backends whose
    /// `supports_max_depth()` is false are called without a depth).
    pub fn trace_lineage(
        &self,
        entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        match max_depth {
            Some(depth) if self.storage.supports_max_depth() => {
                self.storage.trace_lineage(entity_id, Some(depth))
            }
            _ => self.storage.trace_lineage(entity_id, None),
        }
    }

    /// Downstream descendants (reverse lineage) in BFS order: "entity X was wrong — what
    /// downstream facts used it?".
    pub fn trace_descendants(
        &self,
        entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        self.storage.trace_descendants(entity_id, max_depth)
    }

    /// `get_lineage`'s shape for descendants: `entity_id`, `descendant_chain`/`entries`,
    /// `entity_count`, `integrity_verified` (empty map when there are none).
    pub fn get_descendants(&self, entity_id: &str) -> Result<Map<String, Value>, SemanticaError> {
        let descendants = self.trace_descendants(entity_id, None)?;
        if descendants.is_empty() {
            return Ok(Map::new());
        }
        let integrity_verified = descendants.iter().all(|e| verify_checksum(e, None));
        let chain = entry_dicts(&descendants);
        let mut out = Map::new();
        out.insert("entity_id".into(), Value::String(entity_id.to_string()));
        out.insert("descendant_chain".into(), chain.clone());
        out.insert("entries".into(), chain);
        out.insert("entity_count".into(), Value::from(descendants.len()));
        out.insert("integrity_verified".into(), Value::Bool(integrity_verified));
        Ok(out)
    }

    /// Version history of an entity, oldest first, following its own `previous_version_id`
    /// chain: `{version, valid_from, valid_until, recorded_at, author[, revision_type]
    /// [, supersedes]}`. `valid_from`/`valid_until` default to the recording time and the next
    /// version's recording time.
    pub fn revision_history(
        &self,
        entity_id: &str,
    ) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        let Some(current) = self.storage.retrieve(entity_id)? else {
            return Ok(Vec::new());
        };
        let mut chain = vec![current];
        let mut visited: std::collections::HashSet<String> =
            std::collections::HashSet::from([entity_id.to_string()]);
        while let Some(previous_id) = chain
            .last()
            .and_then(|e| e.previous_version_id.clone())
            .filter(|id| !id.is_empty())
        {
            if visited.contains(&previous_id) {
                break;
            }
            let Some(previous) = self.storage.retrieve(&previous_id)? else {
                break;
            };
            chain.push(previous);
            visited.insert(previous_id);
        }
        chain.reverse();
        let mut history = Vec::with_capacity(chain.len());
        for (i, entry) in chain.iter().enumerate() {
            let default_valid_until = chain.get(i + 1).map(|next| next.timestamp.clone());
            let valid_from = match &entry.valid_from {
                Some(v) if !v.is_empty() => v.clone(),
                _ => entry.timestamp.clone(),
            };
            let valid_until = match &entry.valid_until {
                Some(v) if !v.is_empty() => Some(v.clone()),
                _ => default_valid_until,
            };
            let mut version = Map::new();
            version.insert("version".into(), Value::from(i + 1));
            version.insert("valid_from".into(), Value::String(valid_from));
            version.insert(
                "valid_until".into(),
                valid_until.map(Value::String).unwrap_or(Value::Null),
            );
            version.insert("recorded_at".into(), Value::String(entry.timestamp.clone()));
            version.insert("author".into(), Value::String(entry.agent_id.clone()));
            if let Some(kind) = entry.revision_type.as_ref().filter(|s| !s.is_empty()) {
                version.insert("revision_type".into(), Value::String(kind.clone()));
            }
            if let Some(old) = entry.supersedes.as_ref().filter(|s| !s.is_empty()) {
                version.insert("supersedes".into(), Value::String(old.clone()));
            }
            history.push(version);
        }
        Ok(history)
    }

    /// Entries whose `timestamp` lies within `[start, end]` (ISO strings, compared as text),
    /// sorted by timestamp, as dicts.
    pub fn query_recorded_between(
        &self,
        start: &str,
        end: &str,
    ) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        let mut matches: Vec<ProvenanceEntry> = self
            .storage
            .retrieve_all(None)?
            .into_iter()
            .filter(|e| {
                !e.timestamp.is_empty()
                    && start <= e.timestamp.as_str()
                    && e.timestamp.as_str() <= end
            })
            .collect();
        matches.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(matches.iter().map(ProvenanceEntry::to_dict).collect())
    }

    /// All sources along an entity's lineage: `{source, location, timestamp, confidence,
    /// metadata}` for every entry with a source document.
    pub fn get_all_sources(
        &self,
        entity_id: &str,
    ) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        let lineage = self.storage.trace_lineage(entity_id, None)?;
        Ok(lineage
            .iter()
            .filter(|e| !e.source_document.is_empty())
            .map(|e| {
                let dict = e.to_dict();
                let mut source = Map::new();
                source.insert("source".into(), dict["source_document"].clone());
                source.insert("location".into(), dict["source_location"].clone());
                source.insert("timestamp".into(), dict["timestamp"].clone());
                source.insert("confidence".into(), dict["confidence"].clone());
                source.insert("metadata".into(), dict["metadata"].clone());
                source
            })
            .collect())
    }

    /// The entry of an entity as a dict, if tracked.
    pub fn get_provenance(
        &self,
        entity_id: &str,
    ) -> Result<Option<Map<String, Value>>, SemanticaError> {
        Ok(self.storage.retrieve(entity_id)?.map(|e| e.to_dict()))
    }

    // === Invalidation (tombstone, not hard delete) ===

    /// Mark an entity invalidated (`prov:Invalidation`) instead of deleting it: the
    /// pre-invalidation state is archived under a versioned key and the invalidated entry is
    /// appended to the hash chain. Raises `ValueError` when the entity was never tracked.
    pub fn invalidate(
        &self,
        entity_id: &str,
        agent_id: &str,
        reason: Option<&str>,
        metadata: Option<Map<String, Value>>,
    ) -> Result<ProvenanceEntry, SemanticaError> {
        let storage = &*self.storage;
        let mut result_entry: Option<ProvenanceEntry> = None;
        storage.transaction(&mut |conn| {
            let Some(existing) = storage.retrieve_with_conn(conn, entity_id)? else {
                return Err(SemanticaError::Value(format!(
                    "Cannot invalidate: no provenance entry found for entity_id={}",
                    str_repr(entity_id)
                )));
            };
            let mut history = existing.clone();
            let base = format!(
                "{entity_id}:v:{}",
                existing.last_updated.as_deref().unwrap_or("None")
            );
            let mut history_id = base.clone();
            let mut counter = 1;
            while storage.retrieve_with_conn(conn, &history_id)?.is_some() {
                history_id = format!("{base}:{counter}");
                counter += 1;
            }
            history.entity_id = history_id.clone();
            storage.store_with_conn(conn, &history)?;

            let mut entry = existing.clone();
            entry.invalidated = true;
            entry.invalidated_at_time = Some(clock::utcnow_isoformat());
            entry.invalidated_by = Some(agent_id.to_string());
            entry.invalidation_reason = reason.map(str::to_string);
            entry.previous_version_id = Some(history_id);
            if let Some(extra) = metadata.as_ref().filter(|m| !m.is_empty()) {
                for (key, value) in extra {
                    entry.metadata.insert(key.clone(), value.clone());
                }
            }
            self.save_entry(&mut entry, conn, true)?;
            result_entry = Some(entry);
            Ok(())
        })?;
        Ok(result_entry.expect("transaction body produced the entry"))
    }

    // === Utility methods ===

    /// Clear all provenance data (a bulk reset; use `invalidate` to retract one fact).
    pub fn clear(&self) -> Result<usize, SemanticaError> {
        self.storage.clear()
    }

    /// `{total_entries, entity_types: {type: count}, unique_sources}`.
    pub fn get_statistics(&self) -> Result<Map<String, Value>, SemanticaError> {
        let entries = self.storage.retrieve_all(None)?;
        let mut types: Map<String, Value> = Map::new();
        for entry in &entries {
            let count = types
                .get(&entry.entity_type)
                .and_then(Value::as_u64)
                .unwrap_or(0);
            types.insert(entry.entity_type.clone(), Value::from(count + 1));
        }
        let unique: std::collections::HashSet<&str> = entries
            .iter()
            .map(|e| e.source_document.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        let mut out = Map::new();
        out.insert("total_entries".into(), Value::from(entries.len()));
        out.insert("entity_types".into(), Value::Object(types));
        out.insert("unique_sources".into(), Value::from(unique.len()));
        Ok(out)
    }

    // === CLI integration ===

    /// Lineage for CLI display: the first `depth` lineage entries (Python slicing, so a
    /// negative depth drops entries from the end) plus sources and aggregated metadata.
    pub fn lineage(
        &self,
        entity_id: &str,
        depth: i64,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let base = self.get_lineage(entity_id)?;
        let mut entries: Vec<Value> = base
            .get("lineage_chain")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if (entries.len() as i64) > depth {
            entries.truncate(slice_end(entries.len(), depth));
        }
        let sources = self.get_all_sources(entity_id)?;
        let mut out = Map::new();
        out.insert("entity_id".into(), Value::String(entity_id.to_string()));
        out.insert("depth".into(), Value::from(depth));
        out.insert("chain_length".into(), Value::from(entries.len()));
        out.insert(
            "source_documents".into(),
            base.get("source_documents")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
        );
        out.insert("lineage".into(), Value::Array(entries.clone()));
        out.insert("entries".into(), Value::Array(entries));
        out.insert(
            "sources".into(),
            Value::Array(sources.into_iter().map(Value::Object).collect()),
        );
        out.insert(
            "metadata".into(),
            base.get("metadata")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new())),
        );
        Ok(out)
    }

    /// Audit log of all entries (optionally only those recorded at or after `since`), sorted
    /// by timestamp: `json` gives records, `csv` a CSV text, any other format a fixed-width
    /// table.
    pub fn audit_log(&self, since: Option<&str>, format: &str) -> Result<AuditLog, SemanticaError> {
        let mut entries = self.storage.retrieve_all(None)?;
        if let Some(since) = since.filter(|s| !s.is_empty()) {
            entries.retain(|e| e.timestamp.as_str() >= since);
        }
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(match format {
            "json" => AuditLog::Records(entries.iter().map(ProvenanceEntry::to_dict).collect()),
            "csv" => {
                let mut lines =
                    vec!["entity_id,entity_type,activity_id,agent_id,timestamp".to_string()];
                for e in &entries {
                    lines.push(format!(
                        "{},{},{},{},{}",
                        e.entity_id, e.entity_type, e.activity_id, e.agent_id, e.timestamp
                    ));
                }
                AuditLog::Text(lines.join("\n"))
            }
            _ => {
                let mut lines = vec![
                    format!(
                        "{:<20} {:<15} {:<15} {:<25}",
                        "ENTITY_ID", "TYPE", "ACTIVITY", "TIMESTAMP"
                    ),
                    "-".repeat(75),
                ];
                for e in &entries {
                    lines.push(format!(
                        "{:<20} {:<15} {:<15} {:<25}",
                        e.entity_id, e.entity_type, e.activity_id, e.timestamp
                    ));
                }
                AuditLog::Text(lines.join("\n"))
            }
        })
    }

    /// The W3C PROV-O graph of all entries, IRIs minted under `base_uri` (default
    /// [`DEFAULT_BASE_URI`]): entities, agents (typed Person/SoftwareAgent/Organization,
    /// qualified association with `hadRole`, delegation), activities (timing, qualified
    /// generation, association, `wasInformedBy`), derivations and usages (plain and
    /// qualified), qualified invalidations and bundle membership.
    pub fn prov_graph(&self, base_uri: Option<&str>) -> Result<ProvGraph, SemanticaError> {
        let base = base_uri
            .filter(|b| !b.is_empty())
            .unwrap_or(DEFAULT_BASE_URI);
        let mut g = ProvGraph::new(base);
        let rdf_type = oxigraph::model::NamedNode::new_unchecked(RDF_TYPE);
        let prov = ProvGraph::prov;
        for e in self.storage.retrieve_all(None)? {
            let ent = g.uri(&e.entity_id);
            g.add(ent.clone(), rdf_type.clone(), prov("Entity"));
            if !e.timestamp.is_empty() {
                g.add(
                    ent.clone(),
                    prov("generatedAtTime"),
                    datetime_literal(&e.timestamp),
                );
            }

            let mut agent_uri = None;
            if !e.agent_id.is_empty() && e.agent_id != "unknown" {
                let ag = g.uri(&e.agent_id);
                g.add(ag.clone(), rdf_type.clone(), prov("Agent"));
                let subclass = match e.agent_type.as_str() {
                    "person" => Some("Person"),
                    "software_agent" => Some("SoftwareAgent"),
                    "organization" => Some("Organization"),
                    _ => None,
                };
                if let Some(class) = subclass {
                    g.add(ag.clone(), rdf_type.clone(), prov(class));
                }
                g.add(ent.clone(), prov("wasAttributedTo"), ag.clone());
                let association = g.bnode();
                g.add(
                    ent.clone(),
                    prov("qualifiedAssociation"),
                    association.clone(),
                );
                g.add(association.clone(), rdf_type.clone(), prov("Association"));
                g.add(association.clone(), prov("agent"), ag.clone());
                let role = e
                    .role
                    .as_deref()
                    .filter(|r| !r.is_empty())
                    .unwrap_or("generator");
                let role_uri = g.uri(&format!("role_{role}"));
                g.add(association, prov("hadRole"), role_uri);
                if let Some(delegate) = e.acted_on_behalf_of.as_deref().filter(|d| !d.is_empty()) {
                    let delegate_uri = g.uri(delegate);
                    g.add(delegate_uri.clone(), rdf_type.clone(), prov("Agent"));
                    g.add(ag.clone(), prov("actedOnBehalfOf"), delegate_uri);
                }
                agent_uri = Some(ag);
            }

            let mut activity_uri = None;
            if !e.activity_id.is_empty() && e.activity_id != "unknown" {
                let act = g.uri(&e.activity_id);
                g.add(act.clone(), rdf_type.clone(), prov("Activity"));
                g.add(ent.clone(), prov("wasGeneratedBy"), act.clone());
                if let Some(started) = e
                    .activity_started_at_time
                    .as_deref()
                    .filter(|s| !s.is_empty())
                {
                    g.add(
                        act.clone(),
                        prov("startedAtTime"),
                        datetime_literal(started),
                    );
                }
                if let Some(ended) = e
                    .activity_ended_at_time
                    .as_deref()
                    .filter(|s| !s.is_empty())
                {
                    g.add(act.clone(), prov("endedAtTime"), datetime_literal(ended));
                }
                let generation = g.bnode();
                g.add(ent.clone(), prov("qualifiedGeneration"), generation.clone());
                g.add(generation.clone(), rdf_type.clone(), prov("Generation"));
                g.add(generation.clone(), prov("activity"), act.clone());
                if !e.timestamp.is_empty() {
                    g.add(generation, prov("atTime"), datetime_literal(&e.timestamp));
                }
                if let Some(ag) = &agent_uri {
                    g.add(act.clone(), prov("wasAssociatedWith"), ag.clone());
                }
                for informing in &e.informed_by_activities {
                    let informing_uri = g.uri(informing);
                    g.add(act.clone(), prov("wasInformedBy"), informing_uri);
                }
                activity_uri = Some(act);
            }

            let emit_derivation = |g: &mut ProvGraph, source_id: &str| {
                let source_uri = g.uri(source_id);
                g.add(ent.clone(), prov("wasDerivedFrom"), source_uri.clone());
                let derivation = g.bnode();
                g.add(ent.clone(), prov("qualifiedDerivation"), derivation.clone());
                g.add(derivation.clone(), rdf_type.clone(), prov("Derivation"));
                g.add(derivation.clone(), prov("entity"), source_uri);
                if let Some(act) = &activity_uri {
                    g.add(derivation, prov("hadActivity"), act.clone());
                }
            };
            if let Some(parent) = e.parent_entity_id.as_deref().filter(|p| !p.is_empty()) {
                emit_derivation(&mut g, parent);
            }
            for used in &e.used_entities {
                if e.parent_entity_id.as_deref() != Some(used.as_str()) {
                    emit_derivation(&mut g, used);
                }
                if let Some(act) = &activity_uri {
                    let used_uri = g.uri(used);
                    g.add(act.clone(), prov("used"), used_uri.clone());
                    let usage = g.bnode();
                    g.add(act.clone(), prov("qualifiedUsage"), usage.clone());
                    g.add(usage.clone(), rdf_type.clone(), prov("Usage"));
                    g.add(usage, prov("entity"), used_uri);
                }
            }

            if e.invalidated {
                let invalidation = g.bnode();
                g.add(
                    ent.clone(),
                    prov("qualifiedInvalidation"),
                    invalidation.clone(),
                );
                g.add(invalidation.clone(), rdf_type.clone(), prov("Invalidation"));
                if let Some(at) = e.invalidated_at_time.as_deref().filter(|s| !s.is_empty()) {
                    g.add(
                        invalidation.clone(),
                        prov("invalidatedAtTime"),
                        datetime_literal(at),
                    );
                }
                if let Some(by) = e.invalidated_by.as_deref().filter(|s| !s.is_empty()) {
                    let by_uri = g.uri(by);
                    g.add(by_uri.clone(), rdf_type.clone(), prov("Agent"));
                    g.add(invalidation, prov("agent"), by_uri);
                }
            }

            if let Some(bundle) = e.bundle_id.as_deref().filter(|b| !b.is_empty()) {
                let bundle_uri = g.uri(&format!("bundle_{bundle}"));
                g.add(bundle_uri.clone(), rdf_type.clone(), prov("Bundle"));
                g.add(bundle_uri, prov("hadMember"), ent.clone());
            }
        }
        Ok(g)
    }

    /// Export provenance as W3C PROV-O RDF: `format` is an rdflib serializer name (`turtle`,
    /// `ntriples`/`nt`, `jsonld`/`json-ld`, `xml`, `hext`, ...). See [`Self::prov_graph`].
    pub fn export_prov(
        &self,
        format: &str,
        base_uri: Option<&str>,
    ) -> Result<String, SemanticaError> {
        let graph = self.prov_graph(base_uri)?;
        let rdf_format = if format == "jsonld" {
            "json-ld"
        } else {
            format
        };
        rdf::serialize(&graph, rdf_format)
    }

    /// Referential integrity check: every parent/used/previous-version/derived-from/supersedes
    /// reference must name a tracked entity and every `informed_by` activity a tracked
    /// activity. Returns `{valid, total_entries, missing_references, invalidated_count, strict,
    /// errors}`.
    pub fn check(&self, strict: bool) -> Result<Map<String, Value>, SemanticaError> {
        let entries = self.storage.retrieve_all(None)?;
        let all_ids: std::collections::HashSet<&str> =
            entries.iter().map(|e| e.entity_id.as_str()).collect();
        let all_activities: std::collections::HashSet<&str> = entries
            .iter()
            .map(|e| e.activity_id.as_str())
            .filter(|a| !a.is_empty())
            .collect();
        let mut missing: Vec<Value> = Vec::new();
        for e in &entries {
            if let Some(parent) = e.parent_entity_id.as_deref().filter(|p| !p.is_empty())
                && !all_ids.contains(parent)
            {
                missing.push(Value::String(format!("{} -> {}", e.entity_id, parent)));
            }
            for used in &e.used_entities {
                if !all_ids.contains(used.as_str()) {
                    missing.push(Value::String(format!("{} -> {}", e.entity_id, used)));
                }
            }
            for reference in [&e.previous_version_id, &e.derived_from_id, &e.supersedes] {
                if let Some(reference) = reference.as_deref().filter(|r| !r.is_empty())
                    && !all_ids.contains(reference)
                {
                    missing.push(Value::String(format!("{} -> {}", e.entity_id, reference)));
                }
            }
            for activity in &e.informed_by_activities {
                if !all_activities.contains(activity.as_str()) {
                    missing.push(Value::String(format!(
                        "{} (activity) -> {}",
                        e.entity_id, activity
                    )));
                }
            }
        }
        let errors = missing.len();
        let invalidated_count = entries.iter().filter(|e| e.invalidated).count();
        let mut out = Map::new();
        out.insert("valid".into(), Value::Bool(errors == 0));
        out.insert("total_entries".into(), Value::from(entries.len()));
        out.insert("missing_references".into(), Value::Array(missing));
        out.insert("invalidated_count".into(), Value::from(invalidated_count));
        out.insert("strict".into(), Value::Bool(strict));
        out.insert("errors".into(), Value::from(errors));
        Ok(out)
    }

    /// Verify the hash chain: in `sequence_id` order, every entry must match its own checksum,
    /// chain from its predecessor's checksum and continue its sequence without a gap.
    /// Returns `{valid, total_entries, broken_links}`; a `checksum_mismatch` link carries
    /// `entity_id`/`sequence_id`/`reason`, a `chain_break` also the expected/actual previous
    /// checksum and the expected sequence id.
    pub fn verify_chain(&self) -> Result<Map<String, Value>, SemanticaError> {
        let mut entries: Vec<ProvenanceEntry> = self
            .storage
            .retrieve_all(None)?
            .into_iter()
            .filter(|e| e.sequence_id.is_some())
            .collect();
        entries.sort_by_key(|e| e.sequence_id);
        let opt_str = |v: &Option<String>| v.clone().map(Value::String).unwrap_or(Value::Null);
        let mut broken: Vec<Value> = Vec::new();
        let mut expected_previous: Option<String> = None;
        let mut expected_sequence: Option<i64> = None;
        for entry in &entries {
            let sequence_id = entry.sequence_id.expect("filtered above");
            if !verify_checksum(entry, None) {
                let mut link = Map::new();
                link.insert("entity_id".into(), Value::String(entry.entity_id.clone()));
                link.insert("sequence_id".into(), Value::from(sequence_id));
                link.insert("reason".into(), Value::String("checksum_mismatch".into()));
                broken.push(Value::Object(link));
            } else {
                let sequence_gap = expected_sequence.is_some_and(|s| sequence_id != s + 1);
                let checksum_break = entry.previous_checksum != expected_previous;
                if sequence_gap || checksum_break {
                    let mut link = Map::new();
                    link.insert("entity_id".into(), Value::String(entry.entity_id.clone()));
                    link.insert("sequence_id".into(), Value::from(sequence_id));
                    link.insert("reason".into(), Value::String("chain_break".into()));
                    link.insert(
                        "expected_previous_checksum".into(),
                        opt_str(&expected_previous),
                    );
                    link.insert(
                        "actual_previous_checksum".into(),
                        opt_str(&entry.previous_checksum),
                    );
                    link.insert(
                        "expected_sequence_id".into(),
                        expected_sequence
                            .map(|s| Value::from(s + 1))
                            .unwrap_or(Value::Null),
                    );
                    broken.push(Value::Object(link));
                }
            }
            expected_previous = entry.checksum.clone();
            expected_sequence = Some(sequence_id);
        }
        let mut out = Map::new();
        out.insert("valid".into(), Value::Bool(broken.is_empty()));
        out.insert("total_entries".into(), Value::from(entries.len()));
        out.insert("broken_links".into(), Value::Array(broken));
        Ok(out)
    }
}
