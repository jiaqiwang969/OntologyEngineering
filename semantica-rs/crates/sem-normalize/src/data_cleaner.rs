//! Data Cleaning Module (`semantica/normalize/data_cleaner.py`).
//!
//! Duplicate detection and resolution, schema validation and missing-value handling over
//! datasets of JSON-like records (`List[Dict[str, Any]]` -> `Vec<Map<String, Value>>`).

use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Arc;

use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::{PyNum, py_eq, py_repr, py_str, type_name};
use sem_utils::{SemanticaError, get_logger, get_progress_tracker};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::kwargs;
use crate::pystr;

/// A dataset record (`Dict[str, Any]`).
pub type Record = Map<String, Value>;

/// Duplicate record group (`@dataclass DuplicateGroup`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// `records`: the duplicate records (the first is the canonical one).
    pub records: Vec<Record>,
    /// `similarity_score`: average similarity of the records to the first.
    pub similarity_score: f64,
    /// `canonical_record`.
    #[serde(default)]
    pub canonical_record: Option<Record>,
}

impl DuplicateGroup {
    /// `DuplicateGroup(records, similarity_score, canonical_record=None)`.
    pub fn new(
        records: Vec<Record>,
        similarity_score: f64,
        canonical_record: Option<Record>,
    ) -> Self {
        DuplicateGroup {
            records,
            similarity_score,
            canonical_record,
        }
    }
}

fn records_eq(a: &[Record], b: &[Record]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| py_eq(&Value::Object(x.clone()), &Value::Object(y.clone())))
}

impl PartialEq for DuplicateGroup {
    /// The dataclass `__eq__` (fields compared with Python's `==`).
    fn eq(&self, other: &Self) -> bool {
        records_eq(&self.records, &other.records)
            && self.similarity_score == other.similarity_score
            && match (&self.canonical_record, &other.canonical_record) {
                (None, None) => true,
                (Some(a), Some(b)) => py_eq(&Value::Object(a.clone()), &Value::Object(b.clone())),
                _ => false,
            }
    }
}

/// Data validation result (`@dataclass ValidationResult`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// `valid`: no errors.
    pub valid: bool,
    /// `errors`.
    #[serde(default)]
    pub errors: Vec<Map<String, Value>>,
    /// `warnings`.
    #[serde(default)]
    pub warnings: Vec<Map<String, Value>>,
}

impl ValidationResult {
    /// `ValidationResult(valid, errors=[], warnings=[])`.
    pub fn new(valid: bool) -> Self {
        ValidationResult {
            valid,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

impl PartialEq for ValidationResult {
    /// The dataclass `__eq__`.
    fn eq(&self, other: &Self) -> bool {
        self.valid == other.valid
            && records_eq(&self.errors, &other.errors)
            && records_eq(&self.warnings, &other.warnings)
    }
}

/// A Python type the validator checks with `isinstance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyType {
    /// `str`.
    Str,
    /// `int` (`bool` values are instances too).
    Int,
    /// `float`.
    Float,
    /// `bool`.
    Bool,
    /// `list`.
    List,
    /// `dict`.
    Dict,
    /// `type(None)`.
    NoneType,
}

impl PyType {
    /// `isinstance(value, self)`.
    pub fn is_instance(self, value: &Value) -> bool {
        match self {
            PyType::Str => value.is_string(),
            PyType::Int => value.is_boolean() || (value.is_number() && !value.is_f64()),
            PyType::Float => value.is_f64(),
            PyType::Bool => value.is_boolean(),
            PyType::List => value.is_array(),
            PyType::Dict => value.is_object(),
            PyType::NoneType => value.is_null(),
        }
    }

    /// The type for a name in `check_data_types`' map (`str`, `int`, `float`, `bool`,
    /// `list`, `dict`); unknown names mean `str`.
    pub fn from_name(name: &str) -> PyType {
        match name {
            "int" => PyType::Int,
            "float" => PyType::Float,
            "bool" => PyType::Bool,
            "list" => PyType::List,
            "dict" => PyType::Dict,
            _ => PyType::Str,
        }
    }
}

/// One expected type: a type object or a type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeSpec {
    /// A Python type object (`str`, `int`, ...).
    Type(PyType),
    /// A type name (`"str"`, `"int"`, ...).
    Name(String),
}

impl From<PyType> for TypeSpec {
    fn from(value: PyType) -> Self {
        TypeSpec::Type(value)
    }
}

impl From<&str> for TypeSpec {
    fn from(value: &str) -> Self {
        TypeSpec::Name(value.to_string())
    }
}

/// `expected_types` of `check_data_types`: one type/name or a list of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedTypes(pub Vec<TypeSpec>);

impl From<PyType> for ExpectedTypes {
    fn from(value: PyType) -> Self {
        ExpectedTypes(vec![TypeSpec::Type(value)])
    }
}

impl From<&str> for ExpectedTypes {
    fn from(value: &str) -> Self {
        ExpectedTypes(vec![TypeSpec::Name(value.to_string())])
    }
}

impl From<Vec<PyType>> for ExpectedTypes {
    fn from(value: Vec<PyType>) -> Self {
        ExpectedTypes(value.into_iter().map(TypeSpec::Type).collect())
    }
}

impl From<Vec<&str>> for ExpectedTypes {
    fn from(value: Vec<&str>) -> Self {
        ExpectedTypes(value.into_iter().map(TypeSpec::from).collect())
    }
}

impl From<Vec<TypeSpec>> for ExpectedTypes {
    fn from(value: Vec<TypeSpec>) -> Self {
        ExpectedTypes(value)
    }
}

impl ExpectedTypes {
    /// A schema's `type` entry: a name, a list of names, or a dict (iterated over its keys);
    /// other values raise the `TypeError` Python's iteration / `isinstance` would.
    pub fn from_value(value: &Value) -> Result<ExpectedTypes, SemanticaError> {
        match value {
            Value::String(s) => Ok(ExpectedTypes::from(s.as_str())),
            Value::Array(items) => items
                .iter()
                .map(|item| match item {
                    Value::String(s) => Ok(TypeSpec::Name(s.clone())),
                    _ => Err(SemanticaError::Type(
                        "isinstance() arg 2 must be a type, a tuple of types, or a union"
                            .to_string(),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(ExpectedTypes),
            Value::Object(map) => Ok(ExpectedTypes(
                map.keys().map(|k| TypeSpec::Name(k.clone())).collect(),
            )),
            other => Err(SemanticaError::Type(format!(
                "'{}' object is not iterable",
                type_name(other)
            ))),
        }
    }
}

/// Data cleaning coordinator (`DataCleaner(config=None, **kwargs)`).
pub struct DataCleaner {
    /// `config` (with the keyword arguments merged in).
    pub config: Map<String, Value>,
    /// `duplicate_detector`.
    pub duplicate_detector: DuplicateDetector,
    /// `data_validator`.
    pub data_validator: DataValidator,
    /// `missing_value_handler`.
    pub missing_value_handler: MissingValueHandler,
    /// `progress_tracker` (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for DataCleaner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataCleaner")
            .field("config", &self.config)
            .finish()
    }
}

impl Default for DataCleaner {
    fn default() -> Self {
        DataCleaner::new(Map::new())
    }
}

/// `key_fields` from keyword arguments. Non-string entries can never match a record key, so
/// they become keys no JSON record can contain.
fn key_fields_from_value(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Null => None,
        Value::Array(items) => Some(
            items
                .iter()
                .map(|item| match item {
                    Value::String(s) => s.clone(),
                    other => format!("\u{0}<non-str key {}>", py_repr(other)),
                })
                .collect(),
        ),
        Value::String(s) => Some(s.chars().map(|c| c.to_string()).collect()),
        _ => Some(Vec::new()),
    }
}

fn threshold_from_value(value: Option<&Value>) -> Result<Option<f64>, SemanticaError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match PyNum::of(v) {
            Some(n) => Ok(Some(n.to_f64())),
            None => Err(SemanticaError::Type(format!(
                "'>=' not supported between instances of 'float' and '{}'",
                type_name(v)
            ))),
        },
    }
}

impl DataCleaner {
    /// `DataCleaner(config)`: the engines are built from the same config.
    pub fn new(config: Map<String, Value>) -> DataCleaner {
        let logger = get_logger("data_cleaner");
        let duplicate_detector = DuplicateDetector::new(config.clone());
        let data_validator = DataValidator::new(config.clone());
        let missing_value_handler = MissingValueHandler::new(config.clone());
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug("Data cleaner initialized");
        DataCleaner {
            config,
            duplicate_detector,
            data_validator,
            missing_value_handler,
            progress_tracker,
            logger,
        }
    }

    /// `clean_data(dataset, remove_duplicates=True, validate=True, handle_missing=True,
    /// **options)`: missing values (`missing_strategy`, default `"remove"`, with the other
    /// options), validation against `schema` (errors are only logged), then duplicates
    /// (`duplicate_criteria` -> `detect_duplicates` keyword arguments) are removed keeping the
    /// first occurrence.
    pub fn clean_data(
        &self,
        dataset: &[Record],
        remove_duplicates: bool,
        validate: bool,
        handle_missing: bool,
        options: &Map<String, Value>,
    ) -> Result<Vec<Record>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            None,
            None,
            "Semantica: Cleaning data",
            None,
        );
        match self.clean_data_inner(
            dataset,
            remove_duplicates,
            validate,
            handle_missing,
            options,
        ) {
            Ok(cleaned) => {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("completed"), "", None);
                Ok(cleaned)
            }
            Err(e) => {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("failed"), "", None);
                Err(e)
            }
        }
    }

    fn clean_data_inner(
        &self,
        dataset: &[Record],
        remove_duplicates: bool,
        validate: bool,
        handle_missing: bool,
        options: &Map<String, Value>,
    ) -> Result<Vec<Record>, SemanticaError> {
        let mut cleaned: Vec<Record> = dataset.to_vec();

        if handle_missing {
            if options.contains_key("strategy") {
                return Err(SemanticaError::Type(
                    "semantica.normalize.data_cleaner.MissingValueHandler.handle_missing_values() got multiple values for keyword argument 'strategy'"
                        .to_string(),
                ));
            }
            let strategy = options
                .get("missing_strategy")
                .cloned()
                .unwrap_or_else(|| json!("remove"));
            cleaned = self
                .missing_value_handler
                .handle_missing_values_value(&cleaned, &strategy, options);
        }

        if validate {
            let schema = options.get("schema").filter(|s| !s.is_null());
            let validation = self
                .data_validator
                .validate_dataset_value(&cleaned, schema)?;
            if !validation.valid {
                self.logger.warning(format!(
                    "Validation found {} errors",
                    validation.errors.len()
                ));
            }
        }

        if remove_duplicates {
            let criteria = match options.get("duplicate_criteria") {
                None => Map::new(),
                Some(Value::Object(map)) => map.clone(),
                Some(other) => {
                    return Err(SemanticaError::Type(format!(
                        "semantica.normalize.data_cleaner.DataCleaner.detect_duplicates() argument after ** must be a mapping, not {}",
                        type_name(other)
                    )));
                }
            };
            let threshold = threshold_from_value(criteria.get("threshold"))?;
            let key_fields = criteria.get("key_fields").and_then(key_fields_from_value);
            let duplicates = self.detect_duplicates(&cleaned, threshold, key_fields.as_deref());
            let mut duplicate_indices = HashSet::new();
            for group in &duplicates {
                for record in group.records.iter().skip(1) {
                    let record_value = Value::Object(record.clone());
                    if let Some(idx) = cleaned
                        .iter()
                        .position(|r| py_eq(&Value::Object(r.clone()), &record_value))
                    {
                        duplicate_indices.insert(idx);
                    }
                }
            }
            cleaned = cleaned
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !duplicate_indices.contains(i))
                .map(|(_, r)| r)
                .collect();
        }
        Ok(cleaned)
    }

    /// `detect_duplicates(dataset, threshold=None, key_fields=None, **criteria)`.
    pub fn detect_duplicates(
        &self,
        dataset: &[Record],
        threshold: Option<f64>,
        key_fields: Option<&[String]>,
    ) -> Vec<DuplicateGroup> {
        self.duplicate_detector
            .detect_duplicates(dataset, threshold, key_fields)
    }

    /// `validate_data(dataset, schema=None)`.
    pub fn validate_data(
        &self,
        dataset: &[Record],
        schema: Option<&Map<String, Value>>,
    ) -> Result<ValidationResult, SemanticaError> {
        self.data_validator.validate_dataset(dataset, schema)
    }

    /// `handle_missing_values(dataset, strategy="remove", **options)`.
    pub fn handle_missing_values(
        &self,
        dataset: &[Record],
        strategy: Option<&str>,
        options: &Map<String, Value>,
    ) -> Vec<Record> {
        self.missing_value_handler
            .handle_missing_values(dataset, strategy, options)
    }

    /// The logger (`semantica.data_cleaner`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// Duplicate detection engine (`DuplicateDetector(**config)`).
#[derive(Debug, Clone)]
pub struct DuplicateDetector {
    /// `config`.
    pub config: Map<String, Value>,
    /// `similarity_threshold` (default 0.8).
    pub similarity_threshold: f64,
    /// `key_fields` (default: all common fields).
    pub key_fields: Vec<String>,
    logger: Logger,
}

impl Default for DuplicateDetector {
    fn default() -> Self {
        DuplicateDetector::new(Map::new())
    }
}

impl DuplicateDetector {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `DuplicateDetector(**config)` (`similarity_threshold`, `key_fields`).
    pub fn new(config: Map<String, Value>) -> DuplicateDetector {
        let logger = get_logger("duplicate_detector");
        let similarity_threshold = kwargs::get_f64(&config, "similarity_threshold", 0.8);
        let key_fields = config
            .get("key_fields")
            .and_then(key_fields_from_value)
            .unwrap_or_default();
        logger.debug(format!(
            "Duplicate detector initialized (threshold={})",
            sem_utils::pycompat::fmt::float_repr(similarity_threshold)
        ));
        DuplicateDetector {
            config,
            similarity_threshold,
            key_fields,
            logger,
        }
    }

    /// `detect_duplicates(dataset, threshold=None, key_fields=None, **criteria)`: greedy
    /// pairwise grouping; each record joins the first earlier record it matches at
    /// `similarity >= threshold`.
    pub fn detect_duplicates(
        &self,
        dataset: &[Record],
        threshold: Option<f64>,
        key_fields: Option<&[String]>,
    ) -> Vec<DuplicateGroup> {
        let threshold = threshold.unwrap_or(self.similarity_threshold);
        let key_fields: &[String] = key_fields.unwrap_or(&self.key_fields);
        let mut groups = Vec::new();
        let mut processed = HashSet::new();
        for (i, record1) in dataset.iter().enumerate() {
            if processed.contains(&i) {
                continue;
            }
            let mut group = vec![record1.clone()];
            for (j, record2) in dataset.iter().enumerate().skip(i + 1) {
                if processed.contains(&j) {
                    continue;
                }
                if self.calculate_similarity(record1, record2, Some(key_fields)) >= threshold {
                    group.push(record2.clone());
                    processed.insert(j);
                }
            }
            if group.len() > 1 {
                let total: f64 = group[1..]
                    .iter()
                    .map(|r| self.calculate_similarity(&group[0], r, Some(key_fields)))
                    .fold(0.0, |acc, s| acc + s);
                let avg = total / (group.len() - 1) as f64;
                let canonical = group[0].clone();
                groups.push(DuplicateGroup::new(group, avg, Some(canonical)));
                processed.insert(i);
            }
        }
        groups
    }

    /// `calculate_similarity(record1, record2, key_fields=None, **options)`: the mean over the
    /// key fields present (non-`None`) in both records of 1.0 for equal values, a character
    /// set similarity for two strings, else 0.0. Without key fields the common fields are
    /// used (in `record1`'s order).
    pub fn calculate_similarity(
        &self,
        record1: &Record,
        record2: &Record,
        key_fields: Option<&[String]>,
    ) -> f64 {
        let key_fields: Vec<String> = match key_fields {
            Some(fields) => fields.to_vec(),
            None => self.key_fields.clone(),
        };
        let key_fields: Vec<String> = if key_fields.is_empty() {
            record1
                .keys()
                .filter(|k| record2.contains_key(*k))
                .cloned()
                .collect()
        } else {
            key_fields
        };
        if key_fields.is_empty() {
            return 0.0;
        }
        let mut similarities = Vec::new();
        for field in &key_fields {
            let (Some(val1), Some(val2)) = (record1.get(field), record2.get(field)) else {
                continue;
            };
            if val1.is_null() || val2.is_null() {
                continue;
            }
            if py_eq(val1, val2) {
                similarities.push(1.0);
            } else if let (Value::String(s1), Value::String(s2)) = (val1, val2) {
                similarities.push(string_similarity(s1, s2));
            } else {
                similarities.push(0.0);
            }
        }
        if similarities.is_empty() {
            0.0
        } else {
            similarities.iter().fold(0.0, |acc, s| acc + s) / similarities.len() as f64
        }
    }

    /// `resolve_duplicates(duplicate_groups, strategy="keep_first", **options)`: one record per
    /// group (`keep_first`: the canonical record, `merge`: first non-null value per field,
    /// anything else: the first record).
    pub fn resolve_duplicates(
        &self,
        duplicate_groups: &[DuplicateGroup],
        strategy: Option<&str>,
    ) -> Result<Vec<Record>, SemanticaError> {
        let strategy = strategy.unwrap_or("keep_first");
        let mut resolved = Vec::with_capacity(duplicate_groups.len());
        for group in duplicate_groups {
            let first = || {
                group
                    .records
                    .first()
                    .cloned()
                    .ok_or_else(|| SemanticaError::Index("list index out of range".to_string()))
            };
            match strategy {
                "keep_first" => match &group.canonical_record {
                    Some(canonical) if !canonical.is_empty() => resolved.push(canonical.clone()),
                    _ => resolved.push(first()?),
                },
                "merge" => resolved.push(merge_records(&group.records)),
                _ => resolved.push(first()?),
            }
        }
        Ok(resolved)
    }
}

/// `_string_similarity`: 1.0 for equal strings, 0.95 when equal ignoring case, else the
/// Jaccard similarity of their lowercased character sets.
fn string_similarity(s1: &str, s2: &str) -> f64 {
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    if s1 == s2 {
        return 1.0;
    }
    let s1_lower = pystr::lower(s1);
    let s2_lower = pystr::lower(s2);
    if s1_lower == s2_lower {
        return 0.95;
    }
    let set1: HashSet<char> = s1_lower.chars().collect();
    let set2: HashSet<char> = s2_lower.chars().collect();
    let intersection = set1.intersection(&set2).count();
    let union = set1.union(&set2).count();
    if union > 0 {
        intersection as f64 / union as f64
    } else {
        0.0
    }
}

/// `_merge_records`: the first non-null value of each field.
fn merge_records(records: &[Record]) -> Record {
    let mut merged = Record::new();
    for record in records {
        for (key, value) in record {
            let missing = merged.get(key).is_none_or(Value::is_null);
            if missing {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    merged
}

/// Data validation engine (`DataValidator(**config)`).
#[derive(Debug, Clone)]
pub struct DataValidator {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for DataValidator {
    fn default() -> Self {
        DataValidator::new(Map::new())
    }
}

impl DataValidator {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `DataValidator(**config)`.
    pub fn new(config: Map<String, Value>) -> DataValidator {
        let logger = get_logger("data_validator");
        logger.debug("Data validator initialized");
        DataValidator { config, logger }
    }

    /// `validate_dataset(dataset, schema=None)`: every record's errors and warnings, tagged
    /// with `record_index`. A schema Python cannot use raises the same `TypeError` /
    /// `AttributeError`.
    pub fn validate_dataset(
        &self,
        dataset: &[Record],
        schema: Option<&Map<String, Value>>,
    ) -> Result<ValidationResult, SemanticaError> {
        let schema = schema.map(|s| Value::Object(s.clone()));
        self.validate_dataset_value(dataset, schema.as_ref())
    }

    pub(crate) fn validate_dataset_value(
        &self,
        dataset: &[Record],
        schema: Option<&Value>,
    ) -> Result<ValidationResult, SemanticaError> {
        if dataset.is_empty() {
            return Ok(ValidationResult::new(true));
        }
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        for (i, record) in dataset.iter().enumerate() {
            let validation = self.validate_record_value(record, schema)?;
            for error in validation.errors {
                let mut tagged = Map::new();
                tagged.insert("record_index".to_string(), json!(i));
                tagged.extend(error);
                errors.push(tagged);
            }
            for warning in validation.warnings {
                let mut tagged = Map::new();
                tagged.insert("record_index".to_string(), json!(i));
                tagged.extend(warning);
                warnings.push(tagged);
            }
        }
        Ok(ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        })
    }

    /// `validate_record(record, schema=None)`: required fields and field types from
    /// `schema["fields"]`.
    pub fn validate_record(
        &self,
        record: &Record,
        schema: Option<&Map<String, Value>>,
    ) -> Result<ValidationResult, SemanticaError> {
        let schema = schema.map(|s| Value::Object(s.clone()));
        self.validate_record_value(record, schema.as_ref())
    }

    fn validate_record_value(
        &self,
        record: &Record,
        schema: Option<&Value>,
    ) -> Result<ValidationResult, SemanticaError> {
        let mut errors = Vec::new();
        if let Some(schema) = schema.filter(|s| kwargs::truthy(s)) {
            let Value::Object(schema) = schema else {
                return Err(SemanticaError::Attribute(format!(
                    "'{}' object has no attribute 'get'",
                    type_name(schema)
                )));
            };
            let empty = Map::new();
            let fields = match schema.get("fields") {
                Some(Value::Object(map)) => map,
                None => &empty,
                Some(other) => {
                    return Err(SemanticaError::Attribute(format!(
                        "'{}' object has no attribute 'items'",
                        type_name(other)
                    )));
                }
            };
            for (field, field_schema) in fields {
                let Value::Object(field_schema) = field_schema else {
                    return Err(SemanticaError::Attribute(format!(
                        "'{}' object has no attribute 'get'",
                        type_name(field_schema)
                    )));
                };
                let value = record.get(field).filter(|v| !v.is_null());
                let required = field_schema
                    .get("required")
                    .map(kwargs::truthy)
                    .unwrap_or(false);
                if required && value.is_none() {
                    let mut error = Map::new();
                    error.insert("field".to_string(), json!(field));
                    error.insert(
                        "message".to_string(),
                        json!(format!("Required field '{field}' is missing")),
                    );
                    errors.push(error);
                }
                if let (Some(expected), Some(value)) = (field_schema.get("type"), value)
                    && kwargs::truthy(expected)
                {
                    let expected_types = ExpectedTypes::from_value(expected)?;
                    if !self.check_data_types(value, &expected_types) {
                        let mut error = Map::new();
                        error.insert("field".to_string(), json!(field));
                        error.insert(
                            "message".to_string(),
                            json!(format!(
                                "Field '{field}' has incorrect type, expected {}",
                                py_str(expected)
                            )),
                        );
                        errors.push(error);
                    }
                }
            }
        }
        Ok(ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings: Vec::new(),
        })
    }

    /// `check_data_types(data, expected_types)`: whether `data` is an instance of any of the
    /// expected types (names map through `str`/`int`/`float`/`bool`/`list`/`dict`, unknown
    /// names mean `str`).
    pub fn check_data_types(&self, data: &Value, expected_types: &ExpectedTypes) -> bool {
        expected_types.0.iter().any(|spec| {
            let ty = match spec {
                TypeSpec::Type(t) => *t,
                TypeSpec::Name(name) => PyType::from_name(name),
            };
            ty.is_instance(data)
        })
    }
}

/// Missing value processing engine (`MissingValueHandler(**config)`).
#[derive(Debug, Clone)]
pub struct MissingValueHandler {
    /// `config`.
    pub config: Map<String, Value>,
    /// `missing_values` (default `[None, "", "N/A", "null", "NULL"]`).
    pub missing_values: Vec<Value>,
    logger: Logger,
}

impl Default for MissingValueHandler {
    fn default() -> Self {
        MissingValueHandler::new(Map::new())
    }
}

/// A numeric value of the dataset with its original JSON form.
#[derive(Clone)]
struct NumericValue {
    num: PyNum,
    original: Value,
}

fn python_sum(values: &[NumericValue]) -> PyNum {
    let mut int_acc: i128 = 0;
    let mut float_acc: Option<f64> = None;
    for v in values {
        match (float_acc, v.num) {
            (None, PyNum::Int(i)) => int_acc = int_acc.saturating_add(i),
            (None, PyNum::Float(f)) => float_acc = Some(int_acc as f64 + f),
            (Some(acc), PyNum::Int(i)) => float_acc = Some(acc + i as f64),
            (Some(acc), PyNum::Float(f)) => float_acc = Some(acc + f),
        }
    }
    match float_acc {
        Some(f) => PyNum::Float(f),
        None => PyNum::Int(int_acc),
    }
}

impl MissingValueHandler {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `MissingValueHandler(**config)` (`missing_values`).
    pub fn new(config: Map<String, Value>) -> MissingValueHandler {
        let logger = get_logger("missing_value_handler");
        let missing_values = match config.get("missing_values") {
            Some(Value::Array(items)) => items.clone(),
            _ => vec![
                Value::Null,
                json!(""),
                json!("N/A"),
                json!("null"),
                json!("NULL"),
            ],
        };
        logger.debug("Missing value handler initialized");
        MissingValueHandler {
            config,
            missing_values,
            logger,
        }
    }

    fn is_missing(&self, value: &Value) -> bool {
        value.is_null() || self.missing_values.iter().any(|m| py_eq(value, m))
    }

    /// `identify_missing_values(dataset)`: `{"total_records", "missing_counts",
    /// "missing_percentages"}` over every field seen (in first-seen order; Python uses a set).
    pub fn identify_missing_values(&self, dataset: &[Record]) -> Map<String, Value> {
        let mut result = Map::new();
        if dataset.is_empty() {
            result.insert("total_records".to_string(), json!(0));
            result.insert("missing_counts".to_string(), json!({}));
            return result;
        }
        let total = dataset.len();
        let mut fields: Vec<&String> = Vec::new();
        for record in dataset {
            for key in record.keys() {
                if !fields.contains(&key) {
                    fields.push(key);
                }
            }
        }
        let mut counts = Map::new();
        let mut percentages = Map::new();
        for field in fields {
            let missing = dataset
                .iter()
                .filter(|r| self.is_missing(r.get(field).unwrap_or(&Value::Null)))
                .count();
            counts.insert(field.clone(), json!(missing));
            percentages.insert(field.clone(), json!(missing as f64 / total as f64 * 100.0));
        }
        result.insert("total_records".to_string(), json!(total));
        result.insert("missing_counts".to_string(), Value::Object(counts));
        result.insert(
            "missing_percentages".to_string(),
            Value::Object(percentages),
        );
        result
    }

    /// `handle_missing_values(dataset, strategy="remove", **options)`: `remove` drops records
    /// with a missing value, `fill` replaces them with `options["fill_value"]` (default `""`),
    /// `impute` uses `options["method"]` (default `"mean"`); other strategies return the
    /// dataset unchanged.
    pub fn handle_missing_values(
        &self,
        dataset: &[Record],
        strategy: Option<&str>,
        options: &Map<String, Value>,
    ) -> Vec<Record> {
        let strategy = json!(strategy.unwrap_or("remove"));
        self.handle_missing_values_value(dataset, &strategy, options)
    }

    pub(crate) fn handle_missing_values_value(
        &self,
        dataset: &[Record],
        strategy: &Value,
        options: &Map<String, Value>,
    ) -> Vec<Record> {
        match strategy.as_str() {
            Some("remove") => self.remove_missing(dataset),
            Some("fill") => {
                let fill_value = options.get("fill_value").cloned().unwrap_or(json!(""));
                self.fill_missing(dataset, fill_value)
            }
            Some("impute") => {
                let method = options.get("method").cloned().unwrap_or(json!("mean"));
                self.impute_values_value(dataset, &method)
            }
            _ => dataset.to_vec(),
        }
    }

    fn remove_missing(&self, dataset: &[Record]) -> Vec<Record> {
        dataset
            .iter()
            .filter(|record| !record.values().any(|v| self.is_missing(v)))
            .cloned()
            .collect()
    }

    fn fill_missing(&self, dataset: &[Record], fill_value: Value) -> Vec<Record> {
        let fill_value = if fill_value.is_null() {
            json!("")
        } else {
            fill_value
        };
        dataset
            .iter()
            .map(|record| {
                record
                    .iter()
                    .map(|(k, v)| {
                        let v = if self.is_missing(v) {
                            fill_value.clone()
                        } else {
                            v.clone()
                        };
                        (k.clone(), v)
                    })
                    .collect()
            })
            .collect()
    }

    /// `impute_values(dataset, method="mean")`: missing values of numeric fields become the
    /// field's mean (a float), median (the middle element of the sorted values) or 0
    /// (`zero` and unknown methods); fields without numbers get `""`.
    pub fn impute_values(&self, dataset: &[Record], method: Option<&str>) -> Vec<Record> {
        self.impute_values_value(dataset, &json!(method.unwrap_or("mean")))
    }

    fn impute_values_value(&self, dataset: &[Record], method: &Value) -> Vec<Record> {
        if dataset.is_empty() {
            return Vec::new();
        }
        let mut numeric_fields: indexmap::IndexMap<String, Vec<NumericValue>> =
            indexmap::IndexMap::new();
        for record in dataset {
            for (key, value) in record {
                if let Some(num) = PyNum::of(value) {
                    numeric_fields
                        .entry(key.clone())
                        .or_default()
                        .push(NumericValue {
                            num,
                            original: value.clone(),
                        });
                }
            }
        }
        let mut imputation_values: Map<String, Value> = Map::new();
        for (field, values) in &numeric_fields {
            let imputed = match method.as_str() {
                Some("mean") => {
                    let mean = match python_sum(values) {
                        PyNum::Int(sum) => sum as f64 / values.len() as f64,
                        PyNum::Float(sum) => sum / values.len() as f64,
                    };
                    serde_json::Number::from_f64(mean)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }
                Some("median") => {
                    let mut sorted = values.clone();
                    sorted.sort_by(|a, b| a.num.compare(b.num).unwrap_or(Ordering::Equal));
                    sorted[sorted.len() / 2].original.clone()
                }
                _ => json!(0),
            };
            imputation_values.insert(field.clone(), imputed);
        }
        dataset
            .iter()
            .map(|record| {
                record
                    .iter()
                    .map(|(k, v)| {
                        let v = if self.is_missing(v) {
                            imputation_values.get(k).cloned().unwrap_or(json!(""))
                        } else {
                            v.clone()
                        };
                        (k.clone(), v)
                    })
                    .collect()
            })
            .collect()
    }
}
