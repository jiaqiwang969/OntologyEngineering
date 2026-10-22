//! Validation utilities (`semantica/utils/validators.py`).
//!
//! Validators return `(is_valid, error_message)`. The ones that can raise in Python for inputs
//! of the documented types (a bad regex pattern, an unhashable item in a `unique` list, a
//! malformed schema, comparisons between non-numbers) return
//! `Result<(bool, Option<String>), SemanticaError>` carrying the same exception kind.

use std::collections::HashSet;
use std::path::Path;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Number, Value};

use crate::exceptions::SemanticaError;
use crate::pycompat::fmt::{PyNum, hash_key, path_name, py_eq, py_str, str_list_repr, type_name};
use crate::types::{Entity, EntityDict, Relationship, RelationshipDict};

/// `(is_valid, error_message)`.
pub type ValidationResult = (bool, Option<String>);

fn ok() -> ValidationResult {
    (true, None)
}

fn fail(message: impl Into<String>) -> ValidationResult {
    (false, Some(message.into()))
}

/// A Python type used in `isinstance` checks (`field_types`, `key_types`, `item_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PyType {
    /// `str`.
    Str,
    /// `int` (a `bool` is an `int`, as in Python).
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
    /// `object` (everything is an instance).
    Object,
}

impl PyType {
    /// `type.__name__`.
    pub fn name(self) -> &'static str {
        match self {
            PyType::Str => "str",
            PyType::Int => "int",
            PyType::Float => "float",
            PyType::Bool => "bool",
            PyType::List => "list",
            PyType::Dict => "dict",
            PyType::NoneType => "NoneType",
            PyType::Object => "object",
        }
    }

    /// `isinstance(value, type)`.
    pub fn is_instance(self, value: &Value) -> bool {
        match self {
            PyType::Str => value.is_string(),
            PyType::Int => {
                matches!(value, Value::Bool(_)) || matches!(value, Value::Number(n) if !n.is_f64())
            }
            PyType::Float => matches!(value, Value::Number(n) if n.is_f64()),
            PyType::Bool => value.is_boolean(),
            PyType::List => value.is_array(),
            PyType::Dict => value.is_object(),
            PyType::NoneType => value.is_null(),
            PyType::Object => true,
        }
    }
}

/// The `**constraints` of `validate_data` and the constraint dicts of the
/// `validate_*_constraints` functions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Constraints {
    /// `required_fields`.
    pub required_fields: Option<Vec<String>>,
    /// `field_types`.
    pub field_types: Option<IndexMap<String, PyType>>,
    /// `min_length` (strings and lists).
    pub min_length: Option<Number>,
    /// `max_length` (strings and lists).
    pub max_length: Option<Number>,
    /// `min_value` (numbers).
    pub min_value: Option<Number>,
    /// `max_value` (numbers).
    pub max_value: Option<Number>,
    /// `pattern`: a regular expression the string must match at its start (`re.match`).
    pub pattern: Option<String>,
    /// `allowed_values`.
    pub allowed_values: Option<Vec<Value>>,
    /// `item_type` (lists).
    pub item_type: Option<PyType>,
    /// `unique` (lists; default false).
    pub unique: bool,
}

/// `value < bound` (or `>`), with the `TypeError` Python raises for non-numbers.
fn compare(value: &Value, bound: &Number, op: &str) -> Result<bool, SemanticaError> {
    let bound_value = Value::Number(bound.clone());
    match (PyNum::of(value), PyNum::of(&bound_value)) {
        (Some(a), Some(b)) => {
            let ordering = a.compare(b);
            Ok(match op {
                "<" => ordering == Some(std::cmp::Ordering::Less),
                _ => ordering == Some(std::cmp::Ordering::Greater),
            })
        }
        _ => Err(SemanticaError::Type(format!(
            "'{op}' not supported between instances of '{}' and '{}'",
            type_name(value),
            type_name(&bound_value)
        ))),
    }
}

fn number_str(n: &Number) -> String {
    py_str(&Value::Number(n.clone()))
}

fn len_value(len: usize) -> Value {
    Value::from(len as u64)
}

/// `validate_data(data, schema=None, **constraints)`.
pub fn validate_data(
    data: &Value,
    schema: Option<&Map<String, Value>>,
    constraints: &Constraints,
) -> Result<ValidationResult, SemanticaError> {
    if let Some(schema) = schema.filter(|s| !s.is_empty()) {
        let (valid, error) = validate_schema(data, schema)?;
        if !valid {
            return Ok((false, error));
        }
    }
    if let Some(field_types) = &constraints.field_types {
        let (valid, error) = validate_types_of_any(data, field_types)?;
        if !valid {
            return Ok((false, error));
        }
    }
    if let (Value::Object(map), Some(required)) = (data, &constraints.required_fields) {
        let (valid, error) = validate_required_fields(map, required);
        if !valid {
            return Ok((false, error));
        }
    }
    if let Value::String(s) = data {
        let (valid, error) = validate_string_constraints(s, constraints)?;
        if !valid {
            return Ok((false, error));
        }
    }
    if matches!(data, Value::Number(_) | Value::Bool(_)) {
        let (valid, error) = validate_numeric_constraints(data, constraints)?;
        if !valid {
            return Ok((false, error));
        }
    }
    if let Value::Array(items) = data {
        let (valid, error) = validate_list_constraints(items, constraints)?;
        if !valid {
            return Ok((false, error));
        }
    }
    Ok(ok())
}

/// Keyword options of [`validate_config`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOptions {
    /// `allowed_keys`: any other key is invalid.
    pub allowed_keys: Option<Vec<String>>,
    /// `key_types`: expected type per key.
    pub key_types: Option<IndexMap<String, PyType>>,
    /// `nested_validation` (default true): validate nested dicts with the same options.
    pub nested_validation: Option<bool>,
}

/// `validate_config(config, required_keys=None, **options)`.
///
/// Invalid keys are listed in the configuration's key order (Python lists them in set order,
/// which varies between runs).
pub fn validate_config(
    config: &Value,
    required_keys: Option<&[&str]>,
    options: &ConfigOptions,
) -> ValidationResult {
    let Value::Object(config) = config else {
        return fail("Configuration must be a dictionary");
    };
    if let Some(required) = required_keys.filter(|r| !r.is_empty()) {
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|k| !config.contains_key(*k))
            .collect();
        if !missing.is_empty() {
            return fail(format!(
                "Missing required configuration keys: {}",
                missing.join(", ")
            ));
        }
    }
    if let Some(allowed) = &options.allowed_keys {
        let allowed: HashSet<&str> = allowed.iter().map(String::as_str).collect();
        let invalid: Vec<&str> = config
            .keys()
            .map(String::as_str)
            .filter(|k| !allowed.contains(k))
            .collect();
        if !invalid.is_empty() {
            return fail(format!(
                "Invalid configuration keys: {}",
                invalid.join(", ")
            ));
        }
    }
    if let Some(key_types) = &options.key_types {
        for (key, expected) in key_types {
            if let Some(value) = config.get(key)
                && !expected.is_instance(value)
            {
                return fail(format!(
                    "Configuration key '{key}' must be of type {}, got {}",
                    expected.name(),
                    type_name(value)
                ));
            }
        }
    }
    if options.nested_validation.unwrap_or(true) {
        for (key, value) in config {
            if value.is_object() {
                let (valid, error) = validate_config(value, None, options);
                if !valid {
                    return fail(format!(
                        "Error in nested config '{key}': {}",
                        error.unwrap_or_default()
                    ));
                }
            }
        }
    }
    ok()
}

/// `key in container` for a schema value of any type.
fn py_contains(container: &Value, key: &str) -> Result<bool, SemanticaError> {
    match container {
        Value::Object(m) => Ok(m.contains_key(key)),
        Value::String(s) => Ok(s.contains(key)),
        Value::Array(items) => Ok(items.iter().any(|v| py_eq(v, &Value::String(key.into())))),
        other => Err(SemanticaError::Type(format!(
            "argument of type '{}' is not iterable",
            type_name(other)
        ))),
    }
}

/// `container[key]` for a schema value of any type.
fn py_getitem<'a>(container: &'a Value, key: &str) -> Result<&'a Value, SemanticaError> {
    match container {
        Value::Object(m) => m
            .get(key)
            .ok_or_else(|| SemanticaError::Key(key.to_string())),
        Value::String(_) => Err(SemanticaError::Type(
            "string indices must be integers, not 'str'".to_string(),
        )),
        Value::Array(_) => Err(SemanticaError::Type(
            "list indices must be integers or slices, not str".to_string(),
        )),
        other => Err(SemanticaError::Type(format!(
            "'{}' object is not subscriptable",
            type_name(other)
        ))),
    }
}

fn validate_schema_value(data: &Value, schema: &Value) -> Result<ValidationResult, SemanticaError> {
    if py_contains(schema, "type")? {
        let expected = py_getitem(schema, "type")?;
        let matches = match expected.as_str() {
            Some("string") => Some(PyType::Str.is_instance(data)),
            Some("integer") => Some(PyType::Int.is_instance(data)),
            Some("number") => {
                Some(PyType::Int.is_instance(data) || PyType::Float.is_instance(data))
            }
            Some("boolean") => Some(PyType::Bool.is_instance(data)),
            Some("array") => Some(PyType::List.is_instance(data)),
            Some("object") => Some(PyType::Dict.is_instance(data)),
            _ => None,
        };
        if matches == Some(false) {
            let label = expected.as_str().unwrap_or_default();
            return Ok(fail(format!("Expected {label}, got {}", type_name(data))));
        }
    }
    if let Value::Object(map) = data {
        if py_contains(schema, "properties")? {
            let properties = py_getitem(schema, "properties")?;
            let Value::Object(properties) = properties else {
                return Err(SemanticaError::Attribute(format!(
                    "'{}' object has no attribute 'items'",
                    type_name(properties)
                )));
            };
            for (prop_name, prop_schema) in properties {
                if let Some(prop_value) = map.get(prop_name) {
                    let (valid, error) = validate_schema_value(prop_value, prop_schema)?;
                    if !valid {
                        return Ok(fail(format!(
                            "Property '{prop_name}': {}",
                            error.unwrap_or_default()
                        )));
                    }
                }
            }
        }
        if py_contains(schema, "required")? {
            let required = py_getitem(schema, "required")?;
            let items: Vec<Value> = match required {
                Value::Array(items) => items.clone(),
                Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
                Value::Object(m) => m.keys().cloned().map(Value::String).collect(),
                other => {
                    return Err(SemanticaError::Type(format!(
                        "'{}' object is not iterable",
                        type_name(other)
                    )));
                }
            };
            let mut missing = Vec::new();
            for item in &items {
                hash_key(item)?;
                let present = matches!(item, Value::String(s) if map.contains_key(s));
                if !present {
                    missing.push(item);
                }
            }
            if !missing.is_empty() {
                let mut names = Vec::new();
                for (i, item) in missing.iter().enumerate() {
                    match item {
                        Value::String(s) => names.push(s.clone()),
                        other => {
                            return Err(SemanticaError::Type(format!(
                                "sequence item {i}: expected str instance, {} found",
                                type_name(other)
                            )));
                        }
                    }
                }
                return Ok(fail(format!(
                    "Missing required properties: {}",
                    names.join(", ")
                )));
            }
        }
    }
    Ok(ok())
}

/// `validate_schema(data, schema)`: a JSON-schema-like check of `type`, `properties` and
/// `required`. A malformed schema fails with the exception Python raises for it.
pub fn validate_schema(
    data: &Value,
    schema: &Map<String, Value>,
) -> Result<ValidationResult, SemanticaError> {
    validate_schema_value(data, &Value::Object(schema.clone()))
}

/// `validate_types(data, field_types)`.
pub fn validate_types(
    data: &Map<String, Value>,
    field_types: &IndexMap<String, PyType>,
) -> ValidationResult {
    for (field, expected) in field_types {
        if let Some(value) = data.get(field)
            && !expected.is_instance(value)
        {
            return fail(format!(
                "Field '{field}' must be of type {}, got {}",
                expected.name(),
                type_name(value)
            ));
        }
    }
    ok()
}

/// `validate_types` applied to data that may not be a dict (as `validate_data` does).
fn validate_types_of_any(
    data: &Value,
    field_types: &IndexMap<String, PyType>,
) -> Result<ValidationResult, SemanticaError> {
    match data {
        Value::Object(map) => Ok(validate_types(map, field_types)),
        other => {
            for field in field_types.keys() {
                if py_contains(other, field)? {
                    py_getitem(other, field)?;
                }
            }
            Ok(ok())
        }
    }
}

/// `validate_required_fields(data, required_fields)`.
pub fn validate_required_fields<S: AsRef<str>>(
    data: &Map<String, Value>,
    required_fields: &[S],
) -> ValidationResult {
    let missing: Vec<&str> = required_fields
        .iter()
        .map(AsRef::as_ref)
        .filter(|f| !data.contains_key(*f))
        .collect();
    if missing.is_empty() {
        ok()
    } else {
        fail(format!("Missing required fields: {}", missing.join(", ")))
    }
}

/// `validate_string_constraints(value, constraints)`: `min_length`, `max_length`, `pattern`
/// and `allowed_values`. The pattern uses Rust `regex` syntax, anchored at the start like
/// `re.match`; an invalid pattern is an `re.error`.
pub fn validate_string_constraints(
    value: &str,
    constraints: &Constraints,
) -> Result<ValidationResult, SemanticaError> {
    let len = value.chars().count();
    if let Some(min) = &constraints.min_length
        && compare(&len_value(len), min, "<")?
    {
        return Ok(fail(format!(
            "String length {len} is less than minimum {}",
            number_str(min)
        )));
    }
    if let Some(max) = &constraints.max_length
        && compare(&len_value(len), max, ">")?
    {
        return Ok(fail(format!(
            "String length {len} exceeds maximum {}",
            number_str(max)
        )));
    }
    if let Some(pattern) = &constraints.pattern {
        let re = Regex::new(&format!("^(?:{pattern})")).map_err(|e| SemanticaError::Other {
            module: "re".to_string(),
            type_name: "error".to_string(),
            message: e.to_string(),
        })?;
        if !re.is_match(value) {
            return Ok(fail(format!(
                "String does not match required pattern: {pattern}"
            )));
        }
    }
    if let Some(allowed) = &constraints.allowed_values {
        let candidate = Value::String(value.to_string());
        if !allowed.iter().any(|a| py_eq(a, &candidate)) {
            return Ok(fail(format!(
                "Value '{value}' is not in allowed values: {}",
                py_str(&Value::Array(allowed.clone()))
            )));
        }
    }
    Ok(ok())
}

/// `validate_numeric_constraints(value, constraints)`: `min_value`, `max_value` and
/// `allowed_values`. `value` is a number or bool; comparing anything else with a bound is the
/// `TypeError` Python raises.
pub fn validate_numeric_constraints(
    value: &Value,
    constraints: &Constraints,
) -> Result<ValidationResult, SemanticaError> {
    if let Some(min) = &constraints.min_value
        && compare(value, min, "<")?
    {
        return Ok(fail(format!(
            "Value {} is less than minimum {}",
            py_str(value),
            number_str(min)
        )));
    }
    if let Some(max) = &constraints.max_value
        && compare(value, max, ">")?
    {
        return Ok(fail(format!(
            "Value {} exceeds maximum {}",
            py_str(value),
            number_str(max)
        )));
    }
    if let Some(allowed) = &constraints.allowed_values
        && !allowed.iter().any(|a| py_eq(a, value))
    {
        return Ok(fail(format!(
            "Value {} is not in allowed values: {}",
            py_str(value),
            py_str(&Value::Array(allowed.clone()))
        )));
    }
    Ok(ok())
}

/// `validate_list_constraints(value, constraints)`: `min_length`, `max_length`, `item_type`
/// and `unique` (an unhashable item makes `unique` a `TypeError`, as in Python).
pub fn validate_list_constraints(
    value: &[Value],
    constraints: &Constraints,
) -> Result<ValidationResult, SemanticaError> {
    let len = value.len();
    if let Some(min) = &constraints.min_length
        && compare(&len_value(len), min, "<")?
    {
        return Ok(fail(format!(
            "List length {len} is less than minimum {}",
            number_str(min)
        )));
    }
    if let Some(max) = &constraints.max_length
        && compare(&len_value(len), max, ">")?
    {
        return Ok(fail(format!(
            "List length {len} exceeds maximum {}",
            number_str(max)
        )));
    }
    if let Some(item_type) = constraints.item_type {
        for (i, item) in value.iter().enumerate() {
            if !item_type.is_instance(item) {
                return Ok(fail(format!(
                    "List item at index {i} must be of type {}, got {}",
                    item_type.name(),
                    type_name(item)
                )));
            }
        }
    }
    if constraints.unique {
        let mut seen = HashSet::new();
        for item in value {
            seen.insert(hash_key(item)?);
        }
        if seen.len() != value.len() {
            return Ok(fail("List items must be unique"));
        }
    }
    Ok(ok())
}

/// An `Entity` or an entity dict (`Union[Entity, EntityDict]`).
#[derive(Debug, Clone, Copy)]
pub enum EntityRef<'a> {
    /// An [`Entity`].
    Entity(&'a Entity),
    /// An entity dict.
    Dict(&'a EntityDict),
}

impl<'a> From<&'a Entity> for EntityRef<'a> {
    fn from(value: &'a Entity) -> Self {
        EntityRef::Entity(value)
    }
}

impl<'a> From<&'a EntityDict> for EntityRef<'a> {
    fn from(value: &'a EntityDict) -> Self {
        EntityRef::Dict(value)
    }
}

/// A `Relationship` or a relationship dict.
#[derive(Debug, Clone, Copy)]
pub enum RelationshipRef<'a> {
    /// A [`Relationship`].
    Relationship(&'a Relationship),
    /// A relationship dict.
    Dict(&'a RelationshipDict),
}

impl<'a> From<&'a Relationship> for RelationshipRef<'a> {
    fn from(value: &'a Relationship) -> Self {
        RelationshipRef::Relationship(value)
    }
}

impl<'a> From<&'a RelationshipDict> for RelationshipRef<'a> {
    fn from(value: &'a RelationshipDict) -> Self {
        RelationshipRef::Dict(value)
    }
}

fn check_confidence(record: &Map<String, Value>) -> ValidationResult {
    if let Some(confidence) = record.get("confidence") {
        let Some(n) = PyNum::of(confidence) else {
            return fail("Confidence must be a number");
        };
        let x = n.to_f64();
        if !(0.0..=1.0).contains(&x) {
            return fail("Confidence must be between 0.0 and 1.0");
        }
    }
    ok()
}

/// `validate_entity(entity)`: `id`, `text` and `type` are required; a dict's `confidence`,
/// when present, must be a number in [0, 1]. (An `Entity` object is checked for its required
/// fields only, as in the original.)
pub fn validate_entity<'a>(entity: impl Into<EntityRef<'a>>) -> ValidationResult {
    match entity.into() {
        EntityRef::Entity(_) => ok(),
        EntityRef::Dict(dict) => {
            let (valid, error) = validate_required_fields(dict, &["id", "text", "type"]);
            if !valid {
                return (false, error);
            }
            check_confidence(dict)
        }
    }
}

/// `validate_relationship(relationship)`: `id`, `source_id`, `target_id` and `type` are
/// required; a dict's `confidence`, when present, must be a number in [0, 1].
pub fn validate_relationship<'a>(relationship: impl Into<RelationshipRef<'a>>) -> ValidationResult {
    match relationship.into() {
        RelationshipRef::Relationship(_) => ok(),
        RelationshipRef::Dict(dict) => {
            let (valid, error) =
                validate_required_fields(dict, &["id", "source_id", "target_id", "type"]);
            if !valid {
                return (false, error);
            }
            check_confidence(dict)
        }
    }
}

/// `PurePath(path).suffix`.
fn suffix(path: &str) -> String {
    let name = path_name(path);
    let chars: Vec<char> = name.chars().collect();
    match chars.iter().rposition(|c| *c == '.') {
        Some(i) if i > 0 && i < chars.len() - 1 => chars[i..].iter().collect(),
        _ => String::new(),
    }
}

/// `validate_file_path(file_path, must_exist=False, allowed_extensions=None)`.
pub fn validate_file_path(
    file_path: impl AsRef<Path>,
    must_exist: Option<bool>,
    allowed_extensions: Option<&[&str]>,
) -> ValidationResult {
    let path = file_path.as_ref();
    if must_exist.unwrap_or(false) && !path.exists() {
        return fail(format!("File does not exist: {}", path.display()));
    }
    if let Some(allowed) = allowed_extensions.filter(|a| !a.is_empty()) {
        let extension = suffix(&path.to_string_lossy()).to_lowercase();
        if !allowed.iter().any(|ext| ext.to_lowercase() == extension) {
            return fail(format!(
                "File extension '{extension}' not in allowed extensions: {}",
                str_list_repr(allowed)
            ));
        }
    }
    ok()
}

static URL_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)^https?://",
        r"(?:(?:[A-Z0-9](?:[A-Z0-9-]{0,61}[A-Z0-9])?\.)+[A-Z]{2,6}\.?|",
        r"localhost|",
        r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})",
        r"(?::\d+)?",
        r"(?:/?|[/?][^\s\x{1c}-\x{1f}]+)\n?\z"
    ))
    .expect("URL pattern")
});

static EMAIL_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}\n?\z").expect("email pattern")
});

/// `validate_url(url)`: `http`/`https` with a domain, `localhost` or IPv4 address.
pub fn validate_url(url: &str) -> ValidationResult {
    if URL_PATTERN.is_match(url) {
        ok()
    } else {
        fail(format!("Invalid URL format: {url}"))
    }
}

/// `validate_email(email)`.
pub fn validate_email(email: &str) -> ValidationResult {
    if EMAIL_PATTERN.is_match(email) {
        ok()
    } else {
        fail(format!("Invalid email format: {email}"))
    }
}
