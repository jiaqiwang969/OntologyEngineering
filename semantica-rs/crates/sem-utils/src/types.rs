//! Type definitions (`semantica/utils/types.py`): type aliases, string enums, entity and
//! relationship records, processing results, the processor/validator/exporter/importer
//! protocols, generic result containers, type guards and dict conversions.

use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::exceptions::SemanticaError;
use crate::pycompat::fmt::{PyNum, type_name};

/// `JSONType = Union[Dict[str, Any], List[Any], str, int, float, bool, None]`.
pub type JSONType = Value;
/// `EntityDict = Dict[str, Any]`.
pub type EntityDict = Map<String, Value>;
/// `RelationshipDict = Dict[str, Any]`.
pub type RelationshipDict = Map<String, Value>;
/// `ConfigDict = Dict[str, Any]`.
pub type ConfigDict = Map<String, Value>;
/// `DataDict = Dict[str, Any]`.
pub type DataDict = Map<String, Value>;

macro_rules! str_enum {
    ($(#[$meta:meta])* pub enum $name:ident { $($variant:ident($member:literal) = $value:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum $name {
            $(
                #[doc = concat!("`", $member, " = \"", $value, "\"`")]
                #[serde(rename = $value)]
                $variant,
            )+
        }

        impl $name {
            /// Every member, in definition order (`list(Enum)`).
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// `member.value`.
            pub fn value(self) -> &'static str {
                match self {
                    $($name::$variant => $value,)+
                }
            }

            /// `member.name` (the Python member name, e.g. `WORKS_FOR`).
            pub fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $member,)+
                }
            }

            /// `Enum(value)`: the member with this value.
            pub fn from_value(value: &str) -> Option<$name> {
                match value {
                    $($value => Some($name::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            /// `str(member)`: `ClassName.MEMBER`, as Python 3.11 prints `str` enums.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}.{}", stringify!($name), self.name())
            }
        }

        impl PartialEq<str> for $name {
            /// `member == "value"` (string enums compare equal to their value).
            fn eq(&self, other: &str) -> bool {
                self.value() == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.value() == *other
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.value()
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> String {
                value.value().to_string()
            }
        }

        impl From<$name> for Value {
            fn from(value: $name) -> Value {
                Value::String(value.value().to_string())
            }
        }
    };
}

str_enum! {
    /// `EntityType(str, Enum)`.
    pub enum EntityType {
        Person("PERSON") = "PERSON",
        Organization("ORGANIZATION") = "ORGANIZATION",
        Location("LOCATION") = "LOCATION",
        Event("EVENT") = "EVENT",
        Product("PRODUCT") = "PRODUCT",
        Concept("CONCEPT") = "CONCEPT",
        Date("DATE") = "DATE",
        Time("TIME") = "TIME",
        Money("MONEY") = "MONEY",
        Percent("PERCENT") = "PERCENT",
        Quantity("QUANTITY") = "QUANTITY",
        Ordinal("ORDINAL") = "ORDINAL",
        Cardinal("CARDINAL") = "CARDINAL",
        Unknown("UNKNOWN") = "UNKNOWN",
    }
}

str_enum! {
    /// `DataType(str, Enum)`.
    pub enum DataType {
        Text("TEXT") = "text",
        Image("IMAGE") = "image",
        Audio("AUDIO") = "audio",
        Video("VIDEO") = "video",
        Document("DOCUMENT") = "document",
        Structured("STRUCTURED") = "structured",
        Unstructured("UNSTRUCTURED") = "unstructured",
    }
}

str_enum! {
    /// `ProcessingStatus(str, Enum)`.
    pub enum ProcessingStatus {
        Pending("PENDING") = "pending",
        Processing("PROCESSING") = "processing",
        Completed("COMPLETED") = "completed",
        Failed("FAILED") = "failed",
        Cancelled("CANCELLED") = "cancelled",
        Paused("PAUSED") = "paused",
        Retrying("RETRYING") = "retrying",
    }
}

str_enum! {
    /// `QualityLevel(str, Enum)`.
    pub enum QualityLevel {
        High("HIGH") = "high",
        Medium("MEDIUM") = "medium",
        Low("LOW") = "low",
        Poor("POOR") = "poor",
    }
}

str_enum! {
    /// `RelationshipType(str, Enum)`.
    pub enum RelationshipType {
        WorksFor("WORKS_FOR") = "WORKS_FOR",
        LocatedIn("LOCATED_IN") = "LOCATED_IN",
        PartOf("PART_OF") = "PART_OF",
        RelatedTo("RELATED_TO") = "RELATED_TO",
        Causes("CAUSES") = "CAUSES",
        Affects("AFFECTS") = "AFFECTS",
        Before("BEFORE") = "BEFORE",
        After("AFTER") = "AFTER",
        During("DURING") = "DURING",
        SameAs("SAME_AS") = "SAME_AS",
        DifferentFrom("DIFFERENT_FROM") = "DIFFERENT_FROM",
        SimilarTo("SIMILAR_TO") = "SIMILAR_TO",
        Unknown("UNKNOWN") = "UNKNOWN",
    }
}

/// `Entity` dataclass. Equality and hashing use the `id` only, as in the original.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Entity id.
    pub id: String,
    /// Surface text.
    pub text: String,
    /// Entity type (an [`EntityType`] value or any other type name).
    #[serde(rename = "type")]
    pub r#type: String,
    /// Confidence (default 1.0).
    pub confidence: f64,
    /// Start offset.
    pub start: Option<i64>,
    /// End offset.
    pub end: Option<i64>,
    /// Free-form metadata.
    pub metadata: Map<String, Value>,
    /// Related relationship dicts.
    pub relations: Vec<RelationshipDict>,
}

impl Entity {
    /// `Entity(id, text, type)` with the dataclass defaults for the other fields.
    pub fn new(id: impl Into<String>, text: impl Into<String>, r#type: impl Into<String>) -> Self {
        Entity {
            id: id.into(),
            text: text.into(),
            r#type: r#type.into(),
            confidence: 1.0,
            start: None,
            end: None,
            metadata: Map::new(),
            relations: Vec::new(),
        }
    }
}

impl PartialEq for Entity {
    /// `Entity.__eq__`: entities are equal when their ids are.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Entity {}

impl Hash for Entity {
    /// `Entity.__hash__`: `hash(self.id)`.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq<Map<String, Value>> for Entity {
    /// `entity == {"id": ...}`: compares with the dict's `id` (or `entity_id`).
    fn eq(&self, other: &Map<String, Value>) -> bool {
        let candidate = match other.get("id") {
            Some(v) if crate::pycompat::fmt::truthy(v) => Some(v),
            _ => other.get("entity_id"),
        };
        matches!(candidate, Some(Value::String(id)) if *id == self.id)
    }
}

/// `Relationship` dataclass (field-wise equality).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    /// Relationship id.
    pub id: String,
    /// Source entity id.
    pub source_id: String,
    /// Target entity id.
    pub target_id: String,
    /// Relationship type (a [`RelationshipType`] value or any other type name).
    #[serde(rename = "type")]
    pub r#type: String,
    /// Confidence (default 1.0).
    pub confidence: f64,
    /// Free-form metadata.
    pub metadata: Map<String, Value>,
    /// Free-form properties.
    pub properties: Map<String, Value>,
}

impl Relationship {
    /// `Relationship(id, source_id, target_id, type)` with the dataclass defaults.
    pub fn new(
        id: impl Into<String>,
        source_id: impl Into<String>,
        target_id: impl Into<String>,
        r#type: impl Into<String>,
    ) -> Self {
        Relationship {
            id: id.into(),
            source_id: source_id.into(),
            target_id: target_id.into(),
            r#type: r#type.into(),
            confidence: 1.0,
            metadata: Map::new(),
            properties: Map::new(),
        }
    }
}

/// `ProcessingResult` dataclass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessingResult {
    /// Processing status.
    pub status: ProcessingStatus,
    /// Result payload.
    pub data: Option<Value>,
    /// Extracted entities.
    pub entities: Vec<Entity>,
    /// Extracted relationships.
    pub relationships: Vec<Relationship>,
    /// Free-form metadata.
    pub metadata: Map<String, Value>,
    /// Error message, if any.
    pub error: Option<String>,
    /// Execution time in seconds.
    pub execution_time: Option<f64>,
}

impl ProcessingResult {
    /// `ProcessingResult(status)` with the dataclass defaults.
    pub fn new(status: ProcessingStatus) -> Self {
        ProcessingResult {
            status,
            data: None,
            entities: Vec::new(),
            relationships: Vec::new(),
            metadata: Map::new(),
            error: None,
            execution_time: None,
        }
    }
}

/// `QualityMetrics` dataclass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Overall score.
    pub score: f64,
    /// Completeness score.
    pub completeness: Option<f64>,
    /// Accuracy score.
    pub accuracy: Option<f64>,
    /// Consistency score.
    pub consistency: Option<f64>,
    /// Validity score.
    pub validity: Option<f64>,
    /// Timeliness score.
    pub timeliness: Option<f64>,
    /// Free-form metadata.
    pub metadata: Map<String, Value>,
}

impl QualityMetrics {
    /// `QualityMetrics(score)` with the dataclass defaults.
    pub fn new(score: f64) -> Self {
        QualityMetrics {
            score,
            completeness: None,
            accuracy: None,
            consistency: None,
            validity: None,
            timeliness: None,
            metadata: Map::new(),
        }
    }
}

/// `Processor` protocol: `process(data, **options) -> ProcessingResult`.
pub trait Processor {
    /// Process data and return the result.
    fn process(&self, data: &Value, options: &Map<String, Value>) -> ProcessingResult;
}

/// `Validator` protocol: `validate(data, **options) -> bool`.
pub trait Validator {
    /// Validate data; true when valid.
    fn validate(&self, data: &Value, options: &Map<String, Value>) -> bool;
}

/// `Exporter` protocol: `export(data, destination, **options) -> bool`.
pub trait Exporter {
    /// Export data to a destination; true on success.
    fn export(&self, data: &Value, destination: &str, options: &Map<String, Value>) -> bool;
}

/// `Importer` protocol: `import_data(source, **options) -> Any`.
pub trait Importer {
    /// Import data from a source.
    fn import_data(&self, source: &str, options: &Map<String, Value>) -> Value;
}

/// `DataTransformer = Callable[[Any], Any]`.
pub type DataTransformer = Box<dyn Fn(&Value) -> Value + Send + Sync>;
/// `DataFilter = Callable[[Any], bool]`.
pub type DataFilter = Box<dyn Fn(&Value) -> bool + Send + Sync>;
/// `DataValidator = Callable[[Any], Tuple[bool, Optional[str]]]`.
pub type DataValidator = Box<dyn Fn(&Value) -> (bool, Option<String>) + Send + Sync>;

/// `Result(Generic[T])`: a value or an error message. (Distinct from `std::result::Result`.)
#[derive(Clone, PartialEq)]
pub struct Result<T> {
    /// `result.value`.
    pub value: Option<T>,
    /// `result.error`.
    pub error: Option<String>,
    /// `result.success` (`error is None`).
    pub success: bool,
}

impl<T> Result<T> {
    /// `Result(value=None, error=None)`.
    pub fn new(value: Option<T>, error: Option<String>) -> Self {
        let success = error.is_none();
        Result {
            value,
            error,
            success,
        }
    }

    /// `Result(value)`.
    pub fn ok(value: T) -> Self {
        Result::new(Some(value), None)
    }

    /// `Result(error=message)`.
    pub fn err(error: impl Into<String>) -> Self {
        Result::new(None, Some(error.into()))
    }

    /// `bool(result)`.
    pub fn as_bool(&self) -> bool {
        self.success
    }
}

impl<T: fmt::Display> fmt::Debug for Result<T> {
    /// `repr(result)`: `Result(value=...)` or `Result(error=...)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.success {
            match &self.value {
                Some(v) => write!(f, "Result(value={v})"),
                None => f.write_str("Result(value=None)"),
            }
        } else {
            match &self.error {
                Some(e) => write!(f, "Result(error={e})"),
                None => f.write_str("Result(error=None)"),
            }
        }
    }
}

/// `BatchResult(Generic[T])`: a list of [`Result`]s with success counts.
#[derive(Clone, PartialEq)]
pub struct BatchResult<T> {
    /// `batch.results`.
    pub results: Vec<Result<T>>,
}

impl<T: fmt::Display> fmt::Debug for BatchResult<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchResult")
            .field("results", &self.results)
            .finish()
    }
}

impl<T> BatchResult<T> {
    /// `BatchResult(results)`.
    pub fn new(results: Vec<Result<T>>) -> Self {
        BatchResult { results }
    }

    /// `batch.successful`.
    pub fn successful(&self) -> Vec<&Result<T>> {
        self.results.iter().filter(|r| r.success).collect()
    }

    /// `batch.failed`.
    pub fn failed(&self) -> Vec<&Result<T>> {
        self.results.iter().filter(|r| !r.success).collect()
    }

    /// `batch.success_count`.
    pub fn success_count(&self) -> usize {
        self.results.iter().filter(|r| r.success).count()
    }

    /// `batch.failure_count`.
    pub fn failure_count(&self) -> usize {
        self.results.iter().filter(|r| !r.success).count()
    }

    /// `batch.total_count`.
    pub fn total_count(&self) -> usize {
        self.results.len()
    }

    /// `batch.success_rate` (0.0 for an empty batch).
    pub fn success_rate(&self) -> f64 {
        if self.total_count() == 0 {
            return 0.0;
        }
        self.success_count() as f64 / self.total_count() as f64
    }
}

/// `is_entity_dict(obj)`: a dict with `id`, `text` and `type`.
pub fn is_entity_dict(obj: &Value) -> bool {
    matches!(obj, Value::Object(m) if m.contains_key("id") && m.contains_key("text") && m.contains_key("type"))
}

/// `is_relationship_dict(obj)`: a dict with `id`, `source_id`, `target_id` and `type`.
pub fn is_relationship_dict(obj: &Value) -> bool {
    matches!(obj, Value::Object(m) if ["id", "source_id", "target_id", "type"].iter().all(|k| m.contains_key(*k)))
}

/// `is_processing_result(obj)`: a dict whose `status` is a [`ProcessingStatus`] value.
pub fn is_processing_result(obj: &Value) -> bool {
    match obj {
        Value::Object(m) => matches!(
            m.get("status"),
            Some(Value::String(s)) if ProcessingStatus::from_value(s).is_some()
        ),
        _ => false,
    }
}

/// `entity_to_dict(entity)`.
pub fn entity_to_dict(entity: &Entity) -> EntityDict {
    let mut out = Map::new();
    out.insert("id".into(), Value::String(entity.id.clone()));
    out.insert("text".into(), Value::String(entity.text.clone()));
    out.insert("type".into(), Value::String(entity.r#type.clone()));
    out.insert("confidence".into(), Value::from(entity.confidence));
    out.insert(
        "start".into(),
        entity.start.map(Value::from).unwrap_or(Value::Null),
    );
    out.insert(
        "end".into(),
        entity.end.map(Value::from).unwrap_or(Value::Null),
    );
    out.insert("metadata".into(), Value::Object(entity.metadata.clone()));
    out.insert(
        "relations".into(),
        Value::Array(
            entity
                .relations
                .iter()
                .cloned()
                .map(Value::Object)
                .collect(),
        ),
    );
    out
}

fn required<'a>(
    data: &'a Map<String, Value>,
    key: &str,
) -> std::result::Result<&'a Value, SemanticaError> {
    data.get(key)
        .ok_or_else(|| SemanticaError::Key(key.to_string()))
}

fn field_type_error(key: &str, expected: &str, value: &Value) -> SemanticaError {
    SemanticaError::Type(format!(
        "{key} must be {expected}, not {}",
        type_name(value)
    ))
}

fn string_field(
    data: &Map<String, Value>,
    key: &str,
) -> std::result::Result<String, SemanticaError> {
    match required(data, key)? {
        Value::String(s) => Ok(s.clone()),
        other => Err(field_type_error(key, "str", other)),
    }
}

fn float_field(data: &Map<String, Value>, key: &str) -> std::result::Result<f64, SemanticaError> {
    match data.get(key) {
        None => Ok(1.0),
        Some(v) => match (v, PyNum::of(v)) {
            (Value::Number(_), Some(n)) => Ok(n.to_f64()),
            (other, _) => Err(field_type_error(key, "a number", other)),
        },
    }
}

fn opt_int_field(
    data: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Option<i64>, SemanticaError> {
    match data.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) if n.as_i64().is_some() => Ok(n.as_i64()),
        Some(other) => Err(field_type_error(key, "an int or None", other)),
    }
}

fn map_field(
    data: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Map<String, Value>, SemanticaError> {
    match data.get(key) {
        None => Ok(Map::new()),
        Some(Value::Object(m)) => Ok(m.clone()),
        Some(other) => Err(field_type_error(key, "a dict", other)),
    }
}

/// `dict_to_entity(data)`. A missing `id`/`text`/`type` is a `KeyError` as in Python; values
/// of a type the typed record cannot hold are a `TypeError` (Python stores them unchecked).
pub fn dict_to_entity(data: &EntityDict) -> std::result::Result<Entity, SemanticaError> {
    let id = string_field(data, "id")?;
    let text = string_field(data, "text")?;
    let r#type = string_field(data, "type")?;
    let relations = match data.get("relations") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Object(m) => Ok(m.clone()),
                other => Err(field_type_error("relations", "a list of dicts", other)),
            })
            .collect::<std::result::Result<_, _>>()?,
        Some(other) => return Err(field_type_error("relations", "a list", other)),
    };
    Ok(Entity {
        id,
        text,
        r#type,
        confidence: float_field(data, "confidence")?,
        start: opt_int_field(data, "start")?,
        end: opt_int_field(data, "end")?,
        metadata: map_field(data, "metadata")?,
        relations,
    })
}

/// `relationship_to_dict(relationship)`.
pub fn relationship_to_dict(relationship: &Relationship) -> RelationshipDict {
    let mut out = Map::new();
    out.insert("id".into(), Value::String(relationship.id.clone()));
    out.insert(
        "source_id".into(),
        Value::String(relationship.source_id.clone()),
    );
    out.insert(
        "target_id".into(),
        Value::String(relationship.target_id.clone()),
    );
    out.insert("type".into(), Value::String(relationship.r#type.clone()));
    out.insert("confidence".into(), Value::from(relationship.confidence));
    out.insert(
        "metadata".into(),
        Value::Object(relationship.metadata.clone()),
    );
    out.insert(
        "properties".into(),
        Value::Object(relationship.properties.clone()),
    );
    out
}

/// `dict_to_relationship(data)` (same error rules as [`dict_to_entity`]).
pub fn dict_to_relationship(
    data: &RelationshipDict,
) -> std::result::Result<Relationship, SemanticaError> {
    Ok(Relationship {
        id: string_field(data, "id")?,
        source_id: string_field(data, "source_id")?,
        target_id: string_field(data, "target_id")?,
        r#type: string_field(data, "type")?,
        confidence: float_field(data, "confidence")?,
        metadata: map_field(data, "metadata")?,
        properties: map_field(data, "properties")?,
    })
}
