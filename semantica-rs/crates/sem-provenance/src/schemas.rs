//! W3C PROV-O compliant provenance schemas (port of `semantica/provenance/schemas.py`).
//!
//! The dataclasses become structs with public fields in the original field order. `to_dict`
//! returns the original key order; `from_dict` mirrors `cls(**data)` including its `TypeError`s
//! for unexpected and missing keyword arguments. `Debug` prints the dataclass `repr()`.
//!
//! W3C PROV-O mapping:
//! - `ProvenanceEntry.entity_id` → `prov:Entity`
//! - `ProvenanceEntry.activity_id` → `prov:Activity`
//! - `ProvenanceEntry.agent_id` / `agent_type` → `prov:Agent` (Person|SoftwareAgent|Organization)
//! - `ProvenanceEntry.role` → `prov:hadRole` (via `prov:qualifiedAssociation`)
//! - `ProvenanceEntry.parent_entity_id` → `prov:wasDerivedFrom` (legacy combined field)
//! - `ProvenanceEntry.derived_from_id` → `prov:wasDerivedFrom` (true cross-source derivation)
//! - `ProvenanceEntry.previous_version_id` → prior version of the same fact
//! - `ProvenanceEntry.used_entities` → `prov:used`
//! - `ProvenanceEntry.timestamp` → `prov:generatedAtTime`
//! - `ProvenanceEntry.invalidated*` → `prov:Invalidation` (tombstone, not a hard delete)

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::{float_repr, map_repr, py_repr, str_repr, type_name};
use sem_utils::pycompat::time::PyDateTime;

use crate::clock;
use crate::isoformat::datetime_fromisoformat;

// ---------------------------------------------------------------------------------------------
// keyword-argument helpers shared with the manager
// ---------------------------------------------------------------------------------------------

/// `cls(**data)` argument binding: the first unexpected keyword (in dict order) wins, then the
/// missing required arguments, with CPython 3.11's messages.
pub(crate) fn bind_kwargs(
    class: &str,
    data: &Map<String, Value>,
    fields: &[&str],
    required: &[&str],
) -> Result<(), SemanticaError> {
    if let Some(key) = data.keys().find(|key| !fields.contains(&key.as_str())) {
        return Err(SemanticaError::Type(format!(
            "{class}.__init__() got an unexpected keyword argument '{key}'"
        )));
    }
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|name| !data.contains_key(*name))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let quoted: Vec<String> = missing.iter().map(|name| format!("'{name}'")).collect();
    let names = match quoted.len() {
        1 => quoted[0].clone(),
        2 => format!("{} and {}", quoted[0], quoted[1]),
        n => format!("{}, and {}", quoted[..n - 1].join(", "), quoted[n - 1]),
    };
    let plural = if missing.len() == 1 {
        "argument"
    } else {
        "arguments"
    };
    Err(SemanticaError::Type(format!(
        "{class}.__init__() missing {} required positional {plural}: {names}",
        missing.len()
    )))
}

fn type_mismatch(class: &str, field: &str, expected: &str, value: &Value) -> SemanticaError {
    SemanticaError::Type(format!(
        "{class}.{field} must be {expected}, not {}",
        type_name(value)
    ))
}

pub(crate) fn value_str(class: &str, field: &str, value: &Value) -> Result<String, SemanticaError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_mismatch(class, field, "str", other)),
    }
}

pub(crate) fn value_opt_str(
    class: &str,
    field: &str,
    value: &Value,
) -> Result<Option<String>, SemanticaError> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        other => Err(type_mismatch(class, field, "str or None", other)),
    }
}

pub(crate) fn value_f64(class: &str, field: &str, value: &Value) -> Result<f64, SemanticaError> {
    match value {
        Value::Number(n) => Ok(n.as_f64().unwrap_or(f64::NAN)),
        other => Err(type_mismatch(class, field, "float", other)),
    }
}

pub(crate) fn value_opt_f64(
    class: &str,
    field: &str,
    value: &Value,
) -> Result<Option<f64>, SemanticaError> {
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(Some(n.as_f64().unwrap_or(f64::NAN))),
        other => Err(type_mismatch(class, field, "float or None", other)),
    }
}

pub(crate) fn value_opt_i64(
    class: &str,
    field: &str,
    value: &Value,
) -> Result<Option<i64>, SemanticaError> {
    match value {
        Value::Null => Ok(None),
        Value::Number(n) if n.as_i64().is_some() => Ok(n.as_i64()),
        other => Err(type_mismatch(class, field, "int or None", other)),
    }
}

pub(crate) fn value_bool(class: &str, field: &str, value: &Value) -> Result<bool, SemanticaError> {
    match value {
        Value::Bool(b) => Ok(*b),
        other => Err(type_mismatch(class, field, "bool", other)),
    }
}

pub(crate) fn value_str_list(
    class: &str,
    field: &str,
    value: &Value,
) -> Result<Vec<String>, SemanticaError> {
    match value {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::String(s) => Ok(s.clone()),
                other => Err(type_mismatch(class, field, "a list of str", other)),
            })
            .collect(),
        other => Err(type_mismatch(class, field, "list", other)),
    }
}

pub(crate) fn value_map(
    class: &str,
    field: &str,
    value: &Value,
) -> Result<Map<String, Value>, SemanticaError> {
    match value {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_mismatch(class, field, "dict", other)),
    }
}

fn opt_str_value(value: &Option<String>) -> Value {
    value.clone().map(Value::String).unwrap_or(Value::Null)
}

fn opt_i64_value(value: Option<i64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

fn f64_value(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn opt_f64_value(value: Option<f64>) -> Value {
    value.map(f64_value).unwrap_or(Value::Null)
}

fn str_list_value(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

// ---------------------------------------------------------------------------------------------
// repr helpers
// ---------------------------------------------------------------------------------------------

fn repr_opt_str(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(str_repr)
        .unwrap_or_else(|| "None".to_string())
}

fn repr_bool(value: bool) -> String {
    if value { "True" } else { "False" }.to_string()
}

fn repr_opt_i64(value: Option<i64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "None".to_string())
}

fn write_dataclass(
    f: &mut fmt::Formatter<'_>,
    class: &str,
    fields: &[(&str, String)],
) -> fmt::Result {
    let parts: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    write!(f, "{class}({})", parts.join(", "))
}

/// `repr()` of a `datetime.datetime`.
pub(crate) fn datetime_repr(value: &PyDateTime) -> String {
    use chrono::{Datelike, Timelike};
    let wall = value.wall();
    let mut parts: Vec<i64> = vec![
        i64::from(wall.year()),
        i64::from(wall.month()),
        i64::from(wall.day()),
        i64::from(wall.hour()),
        i64::from(wall.minute()),
        i64::from(wall.second()),
        i64::from(wall.nanosecond() / 1000),
    ];
    if parts[6] == 0 {
        parts.pop();
        if parts[5] == 0 {
            parts.pop();
        }
    }
    let numbers: Vec<String> = parts.iter().map(|v| v.to_string()).collect();
    let mut out = format!("datetime.datetime({}", numbers.join(", "));
    if let Some(offset) = value.offset() {
        let seconds = offset.local_minus_utc();
        if seconds == 0 {
            out.push_str(", tzinfo=datetime.timezone.utc");
        } else {
            let (days, secs) = (
                i64::from(seconds).div_euclid(86_400),
                i64::from(seconds).rem_euclid(86_400),
            );
            let delta = if days != 0 {
                format!("days={days}, seconds={secs}")
            } else {
                format!("seconds={secs}")
            };
            out.push_str(&format!(
                ", tzinfo=datetime.timezone(datetime.timedelta({delta}))"
            ));
        }
    }
    out.push(')');
    out
}

// ---------------------------------------------------------------------------------------------
// ProvenanceEntry
// ---------------------------------------------------------------------------------------------

/// W3C PROV-O compliant provenance entry (`semantica.provenance.ProvenanceEntry`).
///
/// Consolidates the provenance kept by `kg.ProvenanceTracker`, `split.ProvenanceInfo` and
/// `conflicts.SourceReference`. Build one with [`ProvenanceEntry::new`] (the three required
/// fields, every other field at its default) or with struct-update syntax over
/// [`ProvenanceEntry::default`]. `timestamp` defaults to `datetime.utcnow().isoformat()`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    /// `prov:Entity` identifier (the storage primary key).
    pub entity_id: String,
    /// Kind of entity (`entity`, `chunk`, `relationship`, `property`, ...).
    pub entity_type: String,
    /// `prov:Activity` that generated the entity.
    pub activity_id: String,
    /// `prov:Agent` responsible (default `"semantica"`).
    pub agent_id: String,
    /// `"person"`, `"software_agent"` (default) or `"organization"`.
    pub agent_type: String,
    /// Whether the agent acted without direct human review (default `true`).
    pub is_automated: bool,
    /// `prov:hadRole` (e.g. `generator`, `approver`, `reviewer`).
    pub role: Option<String>,
    /// Source document identifier (DOI, file path, URL); default `""`.
    pub source_document: String,
    /// Location within the source (page, figure, character range).
    pub source_location: Option<String>,
    /// Direct quote from the source.
    pub source_quote: Option<String>,
    /// `prov:generatedAtTime` (ISO string).
    pub timestamp: String,
    /// When the entity was first tracked.
    pub first_seen: Option<String>,
    /// When the entity was last updated.
    pub last_updated: Option<String>,
    /// Confidence score (default `1.0`).
    pub confidence: f64,
    /// SHA-256 checksum (see [`crate::integrity::compute_checksum`]).
    pub checksum: Option<String>,
    /// Position in the global insertion order (hash chain).
    pub sequence_id: Option<i64>,
    /// Checksum of the preceding entry in insertion order (hash chain).
    pub previous_checksum: Option<String>,
    /// Legacy combined `prov:wasDerivedFrom` link.
    pub parent_entity_id: Option<String>,
    /// `prov:used` entities.
    pub used_entities: Vec<String>,
    /// The archived prior version of the same fact.
    pub previous_version_id: Option<String>,
    /// A different source entity this fact was derived from.
    pub derived_from_id: Option<String>,
    /// `prov:startedAtTime` of the generating activity.
    pub activity_started_at_time: Option<String>,
    /// `prov:endedAtTime` of the generating activity.
    pub activity_ended_at_time: Option<String>,
    /// `prov:actedOnBehalfOf` delegate of the agent.
    pub acted_on_behalf_of: Option<String>,
    /// `prov:wasInformedBy` activities.
    pub informed_by_activities: Vec<String>,
    /// Start of the fact's asserted validity window.
    pub valid_from: Option<String>,
    /// End of the fact's asserted validity window.
    pub valid_until: Option<String>,
    /// Revision kind (e.g. `correction`).
    pub revision_type: Option<String>,
    /// Identifier of the claim this one supersedes.
    pub supersedes: Option<String>,
    /// `prov:Bundle` membership.
    pub bundle_id: Option<String>,
    /// Tombstone flag set by `ProvenanceManager::invalidate`.
    pub invalidated: bool,
    /// `prov:invalidatedAtTime`.
    pub invalidated_at_time: Option<String>,
    /// Agent responsible for the invalidation.
    pub invalidated_by: Option<String>,
    /// Human-readable reason for the invalidation.
    pub invalidation_reason: Option<String>,
    /// Chunk start character index.
    pub start_index: Option<i64>,
    /// Chunk end character index.
    pub end_index: Option<i64>,
    /// Source credibility score.
    pub credibility: Option<f64>,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
    /// Provenance schema version (default `"1.0"`).
    pub version: String,
}

/// Field names of `ProvenanceEntry` in declaration (and `to_dict`) order.
pub(crate) const PROVENANCE_ENTRY_FIELDS: [&str; 39] = [
    "entity_id",
    "entity_type",
    "activity_id",
    "agent_id",
    "agent_type",
    "is_automated",
    "role",
    "source_document",
    "source_location",
    "source_quote",
    "timestamp",
    "first_seen",
    "last_updated",
    "confidence",
    "checksum",
    "sequence_id",
    "previous_checksum",
    "parent_entity_id",
    "used_entities",
    "previous_version_id",
    "derived_from_id",
    "activity_started_at_time",
    "activity_ended_at_time",
    "acted_on_behalf_of",
    "informed_by_activities",
    "valid_from",
    "valid_until",
    "revision_type",
    "supersedes",
    "bundle_id",
    "invalidated",
    "invalidated_at_time",
    "invalidated_by",
    "invalidation_reason",
    "start_index",
    "end_index",
    "credibility",
    "metadata",
    "version",
];

impl Default for ProvenanceEntry {
    /// Every field at its dataclass default; the three required identifiers are empty and
    /// `timestamp` reads the clock (`datetime.utcnow().isoformat()`).
    fn default() -> Self {
        ProvenanceEntry::with_timestamp("", "", "", clock::utcnow_isoformat())
    }
}

impl ProvenanceEntry {
    /// `ProvenanceEntry(entity_id, entity_type, activity_id)`; every other field takes its
    /// default (`timestamp` = `datetime.utcnow().isoformat()`).
    pub fn new(
        entity_id: impl Into<String>,
        entity_type: impl Into<String>,
        activity_id: impl Into<String>,
    ) -> Self {
        ProvenanceEntry::with_timestamp(
            entity_id,
            entity_type,
            activity_id,
            clock::utcnow_isoformat(),
        )
    }

    /// Like [`ProvenanceEntry::new`] with an explicit `timestamp` (the clock is not read).
    pub fn with_timestamp(
        entity_id: impl Into<String>,
        entity_type: impl Into<String>,
        activity_id: impl Into<String>,
        timestamp: impl Into<String>,
    ) -> Self {
        ProvenanceEntry {
            entity_id: entity_id.into(),
            entity_type: entity_type.into(),
            activity_id: activity_id.into(),
            agent_id: "semantica".to_string(),
            agent_type: "software_agent".to_string(),
            is_automated: true,
            role: None,
            source_document: String::new(),
            source_location: None,
            source_quote: None,
            timestamp: timestamp.into(),
            first_seen: None,
            last_updated: None,
            confidence: 1.0,
            checksum: None,
            sequence_id: None,
            previous_checksum: None,
            parent_entity_id: None,
            used_entities: Vec::new(),
            previous_version_id: None,
            derived_from_id: None,
            activity_started_at_time: None,
            activity_ended_at_time: None,
            acted_on_behalf_of: None,
            informed_by_activities: Vec::new(),
            valid_from: None,
            valid_until: None,
            revision_type: None,
            supersedes: None,
            bundle_id: None,
            invalidated: false,
            invalidated_at_time: None,
            invalidated_by: None,
            invalidation_reason: None,
            start_index: None,
            end_index: None,
            credibility: None,
            metadata: Map::new(),
            version: "1.0".to_string(),
        }
    }

    /// `to_dict()`: every field, in declaration order.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("entity_id".into(), Value::String(self.entity_id.clone()));
        d.insert(
            "entity_type".into(),
            Value::String(self.entity_type.clone()),
        );
        d.insert(
            "activity_id".into(),
            Value::String(self.activity_id.clone()),
        );
        d.insert("agent_id".into(), Value::String(self.agent_id.clone()));
        d.insert("agent_type".into(), Value::String(self.agent_type.clone()));
        d.insert("is_automated".into(), Value::Bool(self.is_automated));
        d.insert("role".into(), opt_str_value(&self.role));
        d.insert(
            "source_document".into(),
            Value::String(self.source_document.clone()),
        );
        d.insert(
            "source_location".into(),
            opt_str_value(&self.source_location),
        );
        d.insert("source_quote".into(), opt_str_value(&self.source_quote));
        d.insert("timestamp".into(), Value::String(self.timestamp.clone()));
        d.insert("first_seen".into(), opt_str_value(&self.first_seen));
        d.insert("last_updated".into(), opt_str_value(&self.last_updated));
        d.insert("confidence".into(), f64_value(self.confidence));
        d.insert("checksum".into(), opt_str_value(&self.checksum));
        d.insert("sequence_id".into(), opt_i64_value(self.sequence_id));
        d.insert(
            "previous_checksum".into(),
            opt_str_value(&self.previous_checksum),
        );
        d.insert(
            "parent_entity_id".into(),
            opt_str_value(&self.parent_entity_id),
        );
        d.insert("used_entities".into(), str_list_value(&self.used_entities));
        d.insert(
            "previous_version_id".into(),
            opt_str_value(&self.previous_version_id),
        );
        d.insert(
            "derived_from_id".into(),
            opt_str_value(&self.derived_from_id),
        );
        d.insert(
            "activity_started_at_time".into(),
            opt_str_value(&self.activity_started_at_time),
        );
        d.insert(
            "activity_ended_at_time".into(),
            opt_str_value(&self.activity_ended_at_time),
        );
        d.insert(
            "acted_on_behalf_of".into(),
            opt_str_value(&self.acted_on_behalf_of),
        );
        d.insert(
            "informed_by_activities".into(),
            str_list_value(&self.informed_by_activities),
        );
        d.insert("valid_from".into(), opt_str_value(&self.valid_from));
        d.insert("valid_until".into(), opt_str_value(&self.valid_until));
        d.insert("revision_type".into(), opt_str_value(&self.revision_type));
        d.insert("supersedes".into(), opt_str_value(&self.supersedes));
        d.insert("bundle_id".into(), opt_str_value(&self.bundle_id));
        d.insert("invalidated".into(), Value::Bool(self.invalidated));
        d.insert(
            "invalidated_at_time".into(),
            opt_str_value(&self.invalidated_at_time),
        );
        d.insert("invalidated_by".into(), opt_str_value(&self.invalidated_by));
        d.insert(
            "invalidation_reason".into(),
            opt_str_value(&self.invalidation_reason),
        );
        d.insert("start_index".into(), opt_i64_value(self.start_index));
        d.insert("end_index".into(), opt_i64_value(self.end_index));
        d.insert("credibility".into(), opt_f64_value(self.credibility));
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d.insert("version".into(), Value::String(self.version.clone()));
        d
    }

    /// `ProvenanceEntry.from_dict(data)` (= `ProvenanceEntry(**data)`).
    ///
    /// Raises Python's `TypeError` for an unexpected key or missing required keys. Values must
    /// have the field's type (`null` only for optional fields); Python would store any object.
    pub fn from_dict(data: &Map<String, Value>) -> Result<Self, SemanticaError> {
        const CLASS: &str = "ProvenanceEntry";
        bind_kwargs(
            CLASS,
            data,
            &PROVENANCE_ENTRY_FIELDS,
            &["entity_id", "entity_type", "activity_id"],
        )?;
        let req = |key: &str| value_str(CLASS, key, &data[key]);
        let timestamp = match data.get("timestamp") {
            Some(value) => value_str(CLASS, "timestamp", value)?,
            None => String::new(),
        };
        let mut entry = ProvenanceEntry::with_timestamp(
            req("entity_id")?,
            req("entity_type")?,
            req("activity_id")?,
            timestamp,
        );
        for (key, value) in data {
            let k = key.as_str();
            match k {
                "entity_id" | "entity_type" | "activity_id" | "timestamp" => {}
                "agent_id" => entry.agent_id = value_str(CLASS, k, value)?,
                "agent_type" => entry.agent_type = value_str(CLASS, k, value)?,
                "is_automated" => entry.is_automated = value_bool(CLASS, k, value)?,
                "role" => entry.role = value_opt_str(CLASS, k, value)?,
                "source_document" => entry.source_document = value_str(CLASS, k, value)?,
                "source_location" => entry.source_location = value_opt_str(CLASS, k, value)?,
                "source_quote" => entry.source_quote = value_opt_str(CLASS, k, value)?,
                "first_seen" => entry.first_seen = value_opt_str(CLASS, k, value)?,
                "last_updated" => entry.last_updated = value_opt_str(CLASS, k, value)?,
                "confidence" => entry.confidence = value_f64(CLASS, k, value)?,
                "checksum" => entry.checksum = value_opt_str(CLASS, k, value)?,
                "sequence_id" => entry.sequence_id = value_opt_i64(CLASS, k, value)?,
                "previous_checksum" => entry.previous_checksum = value_opt_str(CLASS, k, value)?,
                "parent_entity_id" => entry.parent_entity_id = value_opt_str(CLASS, k, value)?,
                "used_entities" => entry.used_entities = value_str_list(CLASS, k, value)?,
                "previous_version_id" => {
                    entry.previous_version_id = value_opt_str(CLASS, k, value)?
                }
                "derived_from_id" => entry.derived_from_id = value_opt_str(CLASS, k, value)?,
                "activity_started_at_time" => {
                    entry.activity_started_at_time = value_opt_str(CLASS, k, value)?
                }
                "activity_ended_at_time" => {
                    entry.activity_ended_at_time = value_opt_str(CLASS, k, value)?
                }
                "acted_on_behalf_of" => entry.acted_on_behalf_of = value_opt_str(CLASS, k, value)?,
                "informed_by_activities" => {
                    entry.informed_by_activities = value_str_list(CLASS, k, value)?
                }
                "valid_from" => entry.valid_from = value_opt_str(CLASS, k, value)?,
                "valid_until" => entry.valid_until = value_opt_str(CLASS, k, value)?,
                "revision_type" => entry.revision_type = value_opt_str(CLASS, k, value)?,
                "supersedes" => entry.supersedes = value_opt_str(CLASS, k, value)?,
                "bundle_id" => entry.bundle_id = value_opt_str(CLASS, k, value)?,
                "invalidated" => entry.invalidated = value_bool(CLASS, k, value)?,
                "invalidated_at_time" => {
                    entry.invalidated_at_time = value_opt_str(CLASS, k, value)?
                }
                "invalidated_by" => entry.invalidated_by = value_opt_str(CLASS, k, value)?,
                "invalidation_reason" => {
                    entry.invalidation_reason = value_opt_str(CLASS, k, value)?
                }
                "start_index" => entry.start_index = value_opt_i64(CLASS, k, value)?,
                "end_index" => entry.end_index = value_opt_i64(CLASS, k, value)?,
                "credibility" => entry.credibility = value_opt_f64(CLASS, k, value)?,
                "metadata" => entry.metadata = value_map(CLASS, k, value)?,
                "version" => entry.version = value_str(CLASS, k, value)?,
                _ => unreachable!("bind_kwargs rejects unknown keys"),
            }
        }
        if !data.contains_key("timestamp") {
            // The dataclass default factory runs after the explicit arguments are bound.
            entry.timestamp = clock::utcnow_isoformat();
        }
        Ok(entry)
    }
}

impl fmt::Debug for ProvenanceEntry {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dict = self.to_dict();
        let fields: Vec<(&str, String)> = dict
            .iter()
            .map(|(name, value)| (name.as_str(), py_repr(value)))
            .collect();
        write_dataclass(f, "ProvenanceEntry", &fields)
    }
}

// ---------------------------------------------------------------------------------------------
// SourceReference
// ---------------------------------------------------------------------------------------------

/// Source reference for provenance tracking (`conflicts.SourceReference` compatible).
#[derive(Clone, PartialEq)]
pub struct SourceReference {
    /// Document identifier (DOI, file path, URL).
    pub document: String,
    /// Page number within the document.
    pub page: Option<i64>,
    /// Section identifier within the document.
    pub section: Option<String>,
    /// Line number within the document.
    pub line: Option<i64>,
    /// When this source was accessed/created.
    pub timestamp: Option<PyDateTime>,
    /// Confidence score (default `1.0`).
    pub confidence: f64,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
}

const SOURCE_REFERENCE_FIELDS: [&str; 7] = [
    "document",
    "page",
    "section",
    "line",
    "timestamp",
    "confidence",
    "metadata",
];

impl SourceReference {
    /// `SourceReference(document)` with every other field at its default.
    pub fn new(document: impl Into<String>) -> Self {
        SourceReference {
            document: document.into(),
            page: None,
            section: None,
            line: None,
            timestamp: None,
            confidence: 1.0,
            metadata: Map::new(),
        }
    }

    /// `to_dict()`; `timestamp` becomes `datetime.isoformat()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("document".into(), Value::String(self.document.clone()));
        d.insert("page".into(), opt_i64_value(self.page));
        d.insert("section".into(), opt_str_value(&self.section));
        d.insert("line".into(), opt_i64_value(self.line));
        d.insert(
            "timestamp".into(),
            self.timestamp
                .map(|ts| Value::String(ts.isoformat('T')))
                .unwrap_or(Value::Null),
        );
        d.insert("confidence".into(), f64_value(self.confidence));
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d
    }

    /// `SourceReference.from_dict(data)`: a string `timestamp` goes through
    /// `datetime.fromisoformat` (its `ValueError` propagates).
    pub fn from_dict(data: &Map<String, Value>) -> Result<Self, SemanticaError> {
        const CLASS: &str = "SourceReference";
        // The string timestamp is converted before `cls(**data)` binds the arguments.
        let parsed = match data.get("timestamp") {
            Some(Value::String(text)) => Some(datetime_fromisoformat(text)?),
            _ => None,
        };
        bind_kwargs(CLASS, data, &SOURCE_REFERENCE_FIELDS, &["document"])?;
        let mut source = SourceReference::new(value_str(CLASS, "document", &data["document"])?);
        source.timestamp = match (parsed, data.get("timestamp")) {
            (Some(ts), _) => Some(ts),
            (None, None | Some(Value::Null)) => None,
            (None, Some(other)) => {
                return Err(type_mismatch(CLASS, "timestamp", "datetime", other));
            }
        };
        for (key, value) in data {
            let k = key.as_str();
            match k {
                "document" | "timestamp" => {}
                "page" => source.page = value_opt_i64(CLASS, k, value)?,
                "section" => source.section = value_opt_str(CLASS, k, value)?,
                "line" => source.line = value_opt_i64(CLASS, k, value)?,
                "confidence" => source.confidence = value_f64(CLASS, k, value)?,
                "metadata" => source.metadata = value_map(CLASS, k, value)?,
                _ => unreachable!("bind_kwargs rejects unknown keys"),
            }
        }
        Ok(source)
    }
}

impl fmt::Debug for SourceReference {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_dataclass(
            f,
            "SourceReference",
            &[
                ("document", str_repr(&self.document)),
                ("page", repr_opt_i64(self.page)),
                ("section", repr_opt_str(&self.section)),
                ("line", repr_opt_i64(self.line)),
                (
                    "timestamp",
                    self.timestamp
                        .as_ref()
                        .map(datetime_repr)
                        .unwrap_or_else(|| "None".to_string()),
                ),
                ("confidence", float_repr(self.confidence)),
                ("metadata", map_repr(&self.metadata)),
            ],
        )
    }
}

// ---------------------------------------------------------------------------------------------
// PropertySource
// ---------------------------------------------------------------------------------------------

/// Property source information for conflict tracking (`conflicts.PropertySource` compatible).
#[derive(Clone, PartialEq)]
pub struct PropertySource {
    /// Name of the property.
    pub property_name: String,
    /// Value of the property.
    pub value: Value,
    /// Source references.
    pub sources: Vec<SourceReference>,
    /// Entity the property belongs to.
    pub entity_id: Option<String>,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
}

const PROPERTY_SOURCE_FIELDS: [&str; 5] =
    ["property_name", "value", "sources", "entity_id", "metadata"];

impl PropertySource {
    /// `PropertySource(property_name, value)` with the other fields at their defaults.
    pub fn new(property_name: impl Into<String>, value: impl Into<Value>) -> Self {
        PropertySource {
            property_name: property_name.into(),
            value: value.into(),
            sources: Vec::new(),
            entity_id: None,
            metadata: Map::new(),
        }
    }

    /// `to_dict()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert(
            "property_name".into(),
            Value::String(self.property_name.clone()),
        );
        d.insert("value".into(), self.value.clone());
        d.insert(
            "sources".into(),
            Value::Array(
                self.sources
                    .iter()
                    .map(|s| Value::Object(s.to_dict()))
                    .collect(),
            ),
        );
        d.insert("entity_id".into(), opt_str_value(&self.entity_id));
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d
    }

    /// `PropertySource.from_dict(data)`: dict sources become `SourceReference`s.
    pub fn from_dict(data: &Map<String, Value>) -> Result<Self, SemanticaError> {
        const CLASS: &str = "PropertySource";
        // Dict sources are converted before `cls(**data)` binds the arguments.
        let sources: Option<Vec<Result<SourceReference, &Value>>> = match data.get("sources") {
            None => None,
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .map(|item| match item {
                        Value::Object(map) => SourceReference::from_dict(map).map(Ok),
                        other => Ok(Err(other)),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Some(Value::String(_)) | Some(Value::Object(_)) => Some(Vec::new()),
            Some(other) => {
                return Err(SemanticaError::Type(format!(
                    "'{}' object is not iterable",
                    type_name(other)
                )));
            }
        };
        bind_kwargs(
            CLASS,
            data,
            &PROPERTY_SOURCE_FIELDS,
            &["property_name", "value"],
        )?;
        let sources = match (sources, data.get("sources")) {
            (Some(_), Some(other @ (Value::String(_) | Value::Object(_)))) => {
                return Err(type_mismatch(CLASS, "sources", "list", other));
            }
            (Some(items), _) => Some(
                items
                    .into_iter()
                    .map(|item| {
                        item.map_err(|other| {
                            type_mismatch(CLASS, "sources", "a list of SourceReference", other)
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            (None, _) => None,
        };
        let mut prop = PropertySource::new(
            value_str(CLASS, "property_name", &data["property_name"])?,
            data["value"].clone(),
        );
        if let Some(sources) = sources {
            prop.sources = sources;
        }
        if let Some(value) = data.get("entity_id") {
            prop.entity_id = value_opt_str(CLASS, "entity_id", value)?;
        }
        if let Some(value) = data.get("metadata") {
            prop.metadata = value_map(CLASS, "metadata", value)?;
        }
        Ok(prop)
    }
}

impl fmt::Debug for PropertySource {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sources: Vec<String> = self.sources.iter().map(|s| format!("{s:?}")).collect();
        write_dataclass(
            f,
            "PropertySource",
            &[
                ("property_name", str_repr(&self.property_name)),
                ("value", py_repr(&self.value)),
                ("sources", format!("[{}]", sources.join(", "))),
                ("entity_id", repr_opt_str(&self.entity_id)),
                ("metadata", map_repr(&self.metadata)),
            ],
        )
    }
}

// ---------------------------------------------------------------------------------------------
// AgentRecord / ActivityRecord / Invalidation
// ---------------------------------------------------------------------------------------------

/// Minimum-viable typed W3C PROV-O agent (`prov:Agent`).
///
/// Pass one to `ProvenanceManager::track_entity` (via `TrackEntityOptions::agent`) to set
/// `agent_id`/`agent_type`/`is_automated` together.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRecord {
    /// Agent identifier.
    pub id: String,
    /// `"person"`, `"software_agent"` (default) or `"organization"`.
    pub agent_type: String,
    /// Whether the agent acted without direct human review (default `true`).
    pub is_automated: bool,
    /// Human-readable name.
    pub name: Option<String>,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
}

impl AgentRecord {
    /// `AgentRecord(id)` with the other fields at their defaults.
    pub fn new(id: impl Into<String>) -> Self {
        AgentRecord {
            id: id.into(),
            agent_type: "software_agent".to_string(),
            is_automated: true,
            name: None,
            metadata: Map::new(),
        }
    }

    /// `to_dict()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("id".into(), Value::String(self.id.clone()));
        d.insert("agent_type".into(), Value::String(self.agent_type.clone()));
        d.insert("is_automated".into(), Value::Bool(self.is_automated));
        d.insert("name".into(), opt_str_value(&self.name));
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d
    }

    /// `AgentRecord.from_dict(data)` (= `AgentRecord(**data)`).
    pub fn from_dict(data: &Map<String, Value>) -> Result<Self, SemanticaError> {
        const CLASS: &str = "AgentRecord";
        bind_kwargs(
            CLASS,
            data,
            &["id", "agent_type", "is_automated", "name", "metadata"],
            &["id"],
        )?;
        let mut agent = AgentRecord::new(value_str(CLASS, "id", &data["id"])?);
        for (key, value) in data {
            let k = key.as_str();
            match k {
                "id" => {}
                "agent_type" => agent.agent_type = value_str(CLASS, k, value)?,
                "is_automated" => agent.is_automated = value_bool(CLASS, k, value)?,
                "name" => agent.name = value_opt_str(CLASS, k, value)?,
                "metadata" => agent.metadata = value_map(CLASS, k, value)?,
                _ => unreachable!("bind_kwargs rejects unknown keys"),
            }
        }
        Ok(agent)
    }
}

impl fmt::Debug for AgentRecord {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_dataclass(
            f,
            "AgentRecord",
            &[
                ("id", str_repr(&self.id)),
                ("agent_type", str_repr(&self.agent_type)),
                ("is_automated", repr_bool(self.is_automated)),
                ("name", repr_opt_str(&self.name)),
                ("metadata", map_repr(&self.metadata)),
            ],
        )
    }
}

/// Minimum-viable typed W3C PROV-O activity (`prov:Activity`).
///
/// Pass one to `ProvenanceManager::track_entity` (via `TrackEntityOptions::activity`) to set
/// `activity_id`/`activity_started_at_time`/`activity_ended_at_time` together.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityRecord {
    /// Activity identifier.
    pub id: String,
    /// Free-form classification (default `"process"`).
    pub activity_type: String,
    /// `prov:startedAtTime` (ISO string).
    pub started_at_time: Option<String>,
    /// `prov:endedAtTime` (ISO string).
    pub ended_at_time: Option<String>,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
}

impl ActivityRecord {
    /// `ActivityRecord(id)` with the other fields at their defaults.
    pub fn new(id: impl Into<String>) -> Self {
        ActivityRecord {
            id: id.into(),
            activity_type: "process".to_string(),
            started_at_time: None,
            ended_at_time: None,
            metadata: Map::new(),
        }
    }

    /// `to_dict()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("id".into(), Value::String(self.id.clone()));
        d.insert(
            "activity_type".into(),
            Value::String(self.activity_type.clone()),
        );
        d.insert(
            "started_at_time".into(),
            opt_str_value(&self.started_at_time),
        );
        d.insert("ended_at_time".into(), opt_str_value(&self.ended_at_time));
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d
    }

    /// `ActivityRecord.from_dict(data)` (= `ActivityRecord(**data)`).
    pub fn from_dict(data: &Map<String, Value>) -> Result<Self, SemanticaError> {
        const CLASS: &str = "ActivityRecord";
        bind_kwargs(
            CLASS,
            data,
            &[
                "id",
                "activity_type",
                "started_at_time",
                "ended_at_time",
                "metadata",
            ],
            &["id"],
        )?;
        let mut activity = ActivityRecord::new(value_str(CLASS, "id", &data["id"])?);
        for (key, value) in data {
            let k = key.as_str();
            match k {
                "id" => {}
                "activity_type" => activity.activity_type = value_str(CLASS, k, value)?,
                "started_at_time" => activity.started_at_time = value_opt_str(CLASS, k, value)?,
                "ended_at_time" => activity.ended_at_time = value_opt_str(CLASS, k, value)?,
                "metadata" => activity.metadata = value_map(CLASS, k, value)?,
                _ => unreachable!("bind_kwargs rejects unknown keys"),
            }
        }
        Ok(activity)
    }
}

impl fmt::Debug for ActivityRecord {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_dataclass(
            f,
            "ActivityRecord",
            &[
                ("id", str_repr(&self.id)),
                ("activity_type", str_repr(&self.activity_type)),
                ("started_at_time", repr_opt_str(&self.started_at_time)),
                ("ended_at_time", repr_opt_str(&self.ended_at_time)),
                ("metadata", map_repr(&self.metadata)),
            ],
        )
    }
}

/// W3C PROV-O invalidation record (`prov:Invalidation`): the tombstone for a retracted or
/// corrected entry.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Invalidation {
    /// The invalidated entity.
    pub entity_id: String,
    /// `prov:invalidatedAtTime`.
    pub invalidated_at_time: String,
    /// Agent responsible for the invalidation.
    pub invalidated_by: String,
    /// Human-readable reason.
    pub reason: Option<String>,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
}

impl Invalidation {
    /// `Invalidation(entity_id, invalidated_at_time, invalidated_by)`.
    pub fn new(
        entity_id: impl Into<String>,
        invalidated_at_time: impl Into<String>,
        invalidated_by: impl Into<String>,
    ) -> Self {
        Invalidation {
            entity_id: entity_id.into(),
            invalidated_at_time: invalidated_at_time.into(),
            invalidated_by: invalidated_by.into(),
            reason: None,
            metadata: Map::new(),
        }
    }

    /// `to_dict()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("entity_id".into(), Value::String(self.entity_id.clone()));
        d.insert(
            "invalidated_at_time".into(),
            Value::String(self.invalidated_at_time.clone()),
        );
        d.insert(
            "invalidated_by".into(),
            Value::String(self.invalidated_by.clone()),
        );
        d.insert("reason".into(), opt_str_value(&self.reason));
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d
    }

    /// `Invalidation.from_dict(data)` (= `Invalidation(**data)`).
    pub fn from_dict(data: &Map<String, Value>) -> Result<Self, SemanticaError> {
        const CLASS: &str = "Invalidation";
        bind_kwargs(
            CLASS,
            data,
            &[
                "entity_id",
                "invalidated_at_time",
                "invalidated_by",
                "reason",
                "metadata",
            ],
            &["entity_id", "invalidated_at_time", "invalidated_by"],
        )?;
        let mut inv = Invalidation::new(
            value_str(CLASS, "entity_id", &data["entity_id"])?,
            value_str(CLASS, "invalidated_at_time", &data["invalidated_at_time"])?,
            value_str(CLASS, "invalidated_by", &data["invalidated_by"])?,
        );
        if let Some(value) = data.get("reason") {
            inv.reason = value_opt_str(CLASS, "reason", value)?;
        }
        if let Some(value) = data.get("metadata") {
            inv.metadata = value_map(CLASS, "metadata", value)?;
        }
        Ok(inv)
    }
}

impl fmt::Debug for Invalidation {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_dataclass(
            f,
            "Invalidation",
            &[
                ("entity_id", str_repr(&self.entity_id)),
                ("invalidated_at_time", str_repr(&self.invalidated_at_time)),
                ("invalidated_by", str_repr(&self.invalidated_by)),
                ("reason", repr_opt_str(&self.reason)),
                ("metadata", map_repr(&self.metadata)),
            ],
        )
    }
}
