//! Data Normalization Methods Module (`semantica/normalize/methods.py`).
//!
//! Convenience functions that dispatch to a registered custom method (see
//! [`crate::registry`]) or build the matching normalizer from the method configuration of
//! [`normalize_config`](crate::config::normalize_config) merged with the keyword arguments.
//!
//! A custom method receives the function's positional arguments as JSON values and the
//! keyword arguments; its result is converted to the function's return type. When it fails —
//! or returns a value of another type — a warning is logged and the default implementation
//! runs, as Python does for a failing custom method.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{PyNum, type_name};
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Value, json};

use crate::config::normalize_config;
use crate::data_cleaner::{DataCleaner, DuplicateGroup, Record};
use crate::date_normalizer::{DateInput, DateNormalizer};
use crate::encoding_handler::{EncodingHandler, TextOrBytes};
use crate::entity_normalizer::EntityNormalizer;
use crate::kwargs;
use crate::language_detector::LanguageDetector;
use crate::number_normalizer::{NumberInput, NumberNormalizer, NumberValue};
use crate::registry::{NormalizeMethod, method_registry};
use crate::text_cleaner::{CleanOptions, TextCleaner};
use crate::text_normalizer::{NormalizeTextOptions, TextNormalizer};

static LOGGER: Lazy<Logger> = Lazy::new(|| get_logger("normalize_methods"));

/// The result of [`detect_language`]: a code, or `(code, confidence)` for
/// `method="confidence"`.
#[derive(Debug, Clone, PartialEq)]
pub enum LanguageResult {
    /// `detect(text)`.
    Code(String),
    /// `detect_with_confidence(text)`.
    WithConfidence(String, f64),
}

/// The result of [`handle_encoding`]: `(encoding, confidence)` for `"detect"`, text for
/// `"convert"`, data without BOM for `"remove_bom"`.
#[derive(Debug, Clone, PartialEq)]
pub enum EncodingResult {
    /// `(encoding, confidence)`.
    Detection(Option<String>, f64),
    /// Converted text.
    Text(String),
    /// Data with the BOM removed (same type as the input).
    Data(TextOrBytes),
}

fn text_arg(text: Option<&str>) -> Value {
    text.map(|t| json!(t)).unwrap_or(Value::Null)
}

fn date_arg(input: &DateInput) -> Value {
    match input {
        DateInput::Str(s) => json!(s),
        DateInput::DateTime(dt) => json!(dt.to_string()),
        DateInput::Other(v) => v.clone(),
    }
}

fn number_arg(input: &NumberInput) -> Value {
    match input {
        NumberInput::Str(s) => json!(s),
        NumberInput::Int(i) => NumberValue::Int(*i).to_value(),
        NumberInput::Float(f) => NumberValue::Float(*f).to_value(),
        NumberInput::Other(v) => v.clone(),
    }
}

fn data_arg(data: &TextOrBytes) -> Value {
    match data {
        TextOrBytes::Text(s) => json!(s),
        TextOrBytes::Bytes(b) => Value::Array(b.iter().map(|x| json!(x)).collect()),
    }
}

fn dataset_arg(dataset: &[Record]) -> Value {
    Value::Array(dataset.iter().map(|r| Value::Object(r.clone())).collect())
}

fn wrong_type(expected: &str, value: &Value) -> SemanticaError {
    SemanticaError::Type(format!(
        "custom method returned {}, expected {expected}",
        type_name(value)
    ))
}

/// Run a registered custom method; `None` when there is none or it failed (logged).
fn run_custom<T>(
    task: &str,
    method: &str,
    args: Vec<Value>,
    kwargs: &Map<String, Value>,
    convert: impl Fn(Value) -> Result<T, SemanticaError>,
) -> Option<T> {
    let custom: NormalizeMethod = method_registry.get(task, method)?;
    match custom(&args, kwargs).and_then(convert) {
        Ok(value) => Some(value),
        Err(e) => {
            LOGGER.warning(format!(
                "Custom method {method} failed: {e}, falling back to default"
            ));
            None
        }
    }
}

fn as_string(value: Value) -> Result<String, SemanticaError> {
    match value {
        Value::String(s) => Ok(s),
        other => Err(wrong_type("str", &other)),
    }
}

fn as_map(value: Value) -> Result<Map<String, Value>, SemanticaError> {
    match value {
        Value::Object(map) => Ok(map),
        other => Err(wrong_type("dict", &other)),
    }
}

fn log_error<T>(what: &str, result: Result<T, SemanticaError>) -> Result<T, SemanticaError> {
    result.map_err(|e| {
        LOGGER.error(format!("Failed to {what}: {e}"));
        e
    })
}

/// `normalize_text(text, method="default", **kwargs)`.
pub fn normalize_text<'a>(
    text: impl Into<Option<&'a str>>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<String, SemanticaError> {
    let text = text.into();
    let method = method.unwrap_or("default");
    if let Some(result) = run_custom("text", method, vec![text_arg(text)], kwargs, as_string) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("text", kwargs);
    let normalizer = TextNormalizer::new(config);
    log_error(
        "normalize text",
        normalizer.normalize_text(text, &NormalizeTextOptions::from_kwargs(kwargs)),
    )
}

/// `clean_text(text, method="default", **kwargs)`.
pub fn clean_text<'a>(
    text: impl Into<Option<&'a str>>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<String, SemanticaError> {
    let text = text.into();
    let method = method.unwrap_or("default");
    if let Some(result) = run_custom("clean", method, vec![text_arg(text)], kwargs, as_string) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("clean", kwargs);
    let cleaner = TextCleaner::new(config);
    Ok(cleaner.clean(text, &CleanOptions::from_kwargs(kwargs)))
}

/// `normalize_entity(entity_name, entity_type=None, method="default", **kwargs)`.
pub fn normalize_entity<'a>(
    entity_name: impl Into<Option<&'a str>>,
    entity_type: Option<&str>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<String, SemanticaError> {
    let entity_name = entity_name.into();
    let method = method.unwrap_or("default");
    let args = vec![text_arg(entity_name), text_arg(entity_type)];
    if let Some(result) = run_custom("entity", method, args, kwargs, as_string) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("entity", kwargs);
    let normalizer = EntityNormalizer::new(config);
    let resolve = kwargs
        .get("resolve_aliases")
        .map(kwargs::truthy)
        .unwrap_or(true);
    Ok(normalizer.normalize_entity(entity_name, entity_type, resolve))
}

/// `resolve_aliases(entity_name, entity_type=None, method="default", **kwargs)`.
pub fn resolve_aliases(
    entity_name: &str,
    entity_type: Option<&str>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<Option<String>, SemanticaError> {
    let method = method.unwrap_or("default");
    let args = vec![json!(entity_name), text_arg(entity_type)];
    let convert = |v: Value| -> Result<Option<String>, SemanticaError> {
        match v {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s)),
            other => Err(wrong_type("str", &other)),
        }
    };
    if let Some(result) = run_custom("entity", method, args, kwargs, convert) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("entity", kwargs);
    let normalizer = EntityNormalizer::new(config);
    let mut context = kwargs.clone();
    context.insert("entity_type".to_string(), text_arg(entity_type));
    Ok(normalizer.resolve_aliases(entity_name, &context))
}

/// `disambiguate_entity(entity_name, method="default", **context)`.
pub fn disambiguate_entity(
    entity_name: &str,
    method: Option<&str>,
    context: &Map<String, Value>,
) -> Result<Map<String, Value>, SemanticaError> {
    let method = method.unwrap_or("default");
    if let Some(result) = run_custom("entity", method, vec![json!(entity_name)], context, as_map) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("entity", context);
    let normalizer = EntityNormalizer::new(config);
    Ok(normalizer.disambiguate_entity(entity_name, context))
}

/// `normalize_date(date_input, format="ISO8601", timezone="UTC", method="default",
/// **kwargs)`.
pub fn normalize_date(
    date_input: impl Into<DateInput>,
    format: Option<&str>,
    timezone: Option<&str>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<String, SemanticaError> {
    let date_input = date_input.into();
    let method = method.unwrap_or("default");
    let format = format.unwrap_or("ISO8601");
    let timezone = timezone.unwrap_or("UTC");
    let args = vec![date_arg(&date_input), json!(format), json!(timezone)];
    if let Some(result) = run_custom("date", method, args, kwargs, as_string) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("date", kwargs);
    let normalizer = DateNormalizer::new(config);
    log_error(
        "normalize date",
        normalizer.normalize_date(date_input, Some(format), Some(timezone)),
    )
}

/// `normalize_time(time_input, method="default", **kwargs)`.
pub fn normalize_time(
    time_input: impl Into<DateInput>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<String, SemanticaError> {
    let time_input = time_input.into();
    let method = method.unwrap_or("default");
    if let Some(result) = run_custom(
        "date",
        method,
        vec![date_arg(&time_input)],
        kwargs,
        as_string,
    ) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("date", kwargs);
    let normalizer = DateNormalizer::new(config);
    Ok(normalizer.normalize_time(time_input))
}

/// `normalize_number(number_input, method="default", **kwargs)`.
pub fn normalize_number(
    number_input: impl Into<NumberInput>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<NumberValue, SemanticaError> {
    let number_input = number_input.into();
    let method = method.unwrap_or("default");
    let convert = |v: Value| -> Result<NumberValue, SemanticaError> {
        match PyNum::of(&v) {
            Some(PyNum::Int(i)) => Ok(NumberValue::Int(i)),
            Some(PyNum::Float(f)) => Ok(NumberValue::Float(f)),
            None => Err(wrong_type("int | float", &v)),
        }
    };
    if let Some(result) = run_custom(
        "number",
        method,
        vec![number_arg(&number_input)],
        kwargs,
        convert,
    ) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("number", kwargs);
    let normalizer = NumberNormalizer::new(config);
    log_error(
        "normalize number",
        normalizer.normalize_number(number_input),
    )
}

/// `normalize_quantity(quantity_input, method="default", **kwargs)`.
pub fn normalize_quantity(
    quantity_input: &str,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<Map<String, Value>, SemanticaError> {
    let method = method.unwrap_or("default");
    if let Some(result) = run_custom(
        "number",
        method,
        vec![json!(quantity_input)],
        kwargs,
        as_map,
    ) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("number", kwargs);
    let normalizer = NumberNormalizer::new(config);
    log_error(
        "normalize quantity",
        normalizer.normalize_quantity(quantity_input),
    )
}

fn as_records(value: Value) -> Result<Vec<Record>, SemanticaError> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                Value::Object(map) => Ok(map),
                other => Err(wrong_type("dict", &other)),
            })
            .collect(),
        other => Err(wrong_type("list", &other)),
    }
}

/// `clean_data(dataset, remove_duplicates=True, validate=True, handle_missing=True,
/// method="default", **kwargs)`.
pub fn clean_data(
    dataset: &[Record],
    remove_duplicates: bool,
    validate: bool,
    handle_missing: bool,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<Vec<Record>, SemanticaError> {
    let method = method.unwrap_or("default");
    let args = vec![
        dataset_arg(dataset),
        json!(remove_duplicates),
        json!(validate),
        json!(handle_missing),
    ];
    if let Some(result) = run_custom("clean", method, args, kwargs, as_records) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("clean", kwargs);
    let cleaner = DataCleaner::new(config);
    log_error(
        "clean data",
        cleaner.clean_data(dataset, remove_duplicates, validate, handle_missing, kwargs),
    )
}

/// `detect_duplicates(dataset, threshold=None, key_fields=None, method="default",
/// **kwargs)`.
pub fn detect_duplicates(
    dataset: &[Record],
    threshold: Option<f64>,
    key_fields: Option<&[String]>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<Vec<DuplicateGroup>, SemanticaError> {
    let method = method.unwrap_or("default");
    let args = vec![
        dataset_arg(dataset),
        threshold.map(|t| json!(t)).unwrap_or(Value::Null),
        key_fields.map(|k| json!(k)).unwrap_or(Value::Null),
    ];
    let convert = |v: Value| -> Result<Vec<DuplicateGroup>, SemanticaError> {
        serde_json::from_value(v).map_err(|e| SemanticaError::Type(e.to_string()))
    };
    if let Some(result) = run_custom("clean", method, args, kwargs, convert) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("clean", kwargs);
    let cleaner = DataCleaner::new(config);
    Ok(cleaner.detect_duplicates(dataset, threshold, key_fields))
}

/// `detect_language(text, method="default", **kwargs)`: a code, or `(code, confidence)` for
/// `method="confidence"`.
pub fn detect_language(
    text: &str,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<LanguageResult, SemanticaError> {
    let method = method.unwrap_or("default");
    let convert = |v: Value| -> Result<LanguageResult, SemanticaError> {
        match v {
            Value::String(s) => Ok(LanguageResult::Code(s)),
            Value::Array(items) if items.len() == 2 => match (&items[0], PyNum::of(&items[1])) {
                (Value::String(s), Some(n)) => {
                    Ok(LanguageResult::WithConfidence(s.clone(), n.to_f64()))
                }
                _ => Err(wrong_type("str", &Value::Array(items.clone()))),
            },
            other => Err(wrong_type("str", &other)),
        }
    };
    if let Some(result) = run_custom("language", method, vec![json!(text)], kwargs, convert) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("language", kwargs);
    let detector = LanguageDetector::new(config);
    Ok(if method == "confidence" {
        let (code, confidence) = detector.detect_with_confidence(text);
        LanguageResult::WithConfidence(code, confidence)
    } else {
        LanguageResult::Code(detector.detect(text))
    })
}

/// `handle_encoding(data, operation="detect", method="default", **kwargs)`: `"detect"`,
/// `"convert"` (to UTF-8, auto-detecting the source) or `"remove_bom"`. As in the original,
/// passing `source_encoding` with `"convert"` (or any keyword argument with `"remove_bom"`)
/// raises `TypeError`, and other operations raise `ValueError`.
pub fn handle_encoding(
    data: impl Into<TextOrBytes>,
    operation: Option<&str>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<EncodingResult, SemanticaError> {
    let data = data.into();
    let operation = operation.unwrap_or("detect");
    let method = method.unwrap_or("default");
    let convert = |v: Value| -> Result<EncodingResult, SemanticaError> {
        match v {
            Value::String(s) => Ok(EncodingResult::Text(s)),
            Value::Array(items) => {
                if items.len() == 2
                    && let (Some(n), true) = (
                        PyNum::of(&items[1]),
                        items[0].is_string() || items[0].is_null(),
                    )
                {
                    return Ok(EncodingResult::Detection(
                        items[0].as_str().map(str::to_string),
                        n.to_f64(),
                    ));
                }
                let bytes: Option<Vec<u8>> = items
                    .iter()
                    .map(|i| i.as_u64().and_then(|b| u8::try_from(b).ok()))
                    .collect();
                bytes
                    .map(|b| EncodingResult::Data(TextOrBytes::Bytes(b)))
                    .ok_or_else(|| wrong_type("bytes", &Value::Array(items.clone())))
            }
            other => Err(wrong_type("str", &other)),
        }
    };
    if let Some(result) = run_custom(
        "encoding",
        method,
        vec![data_arg(&data), json!(operation)],
        kwargs,
        convert,
    ) {
        return Ok(result);
    }
    let config = normalize_config.merge_method_config("encoding", kwargs);
    let handler = EncodingHandler::new(config);
    let result = match operation {
        "detect" => {
            let (encoding, confidence) = handler.detect(data);
            Ok(EncodingResult::Detection(encoding, confidence))
        }
        "convert" => {
            if kwargs.contains_key("source_encoding") {
                Err(SemanticaError::Type(
                    "semantica.normalize.encoding_handler.EncodingHandler.convert_to_utf8() got multiple values for keyword argument 'source_encoding'"
                        .to_string(),
                ))
            } else {
                handler
                    .convert_to_utf8(data, None)
                    .map(EncodingResult::Text)
            }
        }
        "remove_bom" => match kwargs.keys().next() {
            Some(key) => Err(SemanticaError::Type(format!(
                "EncodingHandler.remove_bom() got an unexpected keyword argument '{key}'"
            ))),
            None => Ok(EncodingResult::Data(handler.remove_bom(data))),
        },
        other => Err(SemanticaError::Value(format!("Unknown operation: {other}"))),
    };
    log_error("handle encoding", result)
}

/// `get_normalize_method(task, name)`.
pub fn get_normalize_method(task: &str, name: &str) -> Option<NormalizeMethod> {
    method_registry.get(task, name)
}

/// `list_available_methods(task=None)`.
pub fn list_available_methods(task: Option<&str>) -> IndexMap<String, Vec<String>> {
    method_registry.list_all(task)
}
