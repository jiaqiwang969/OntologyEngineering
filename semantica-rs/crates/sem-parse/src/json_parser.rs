//! JSON document parsing (`semantica/parse/json_parser.py`).
//!
//! [`JSONParser`] parses a JSON file or JSON string into [`JSONData`], optionally flattening
//! nested structures and listing JSON paths. Decoding follows CPython's `json` module
//! exactly (accepted documents, value forms and `JSONDecodeError` messages, see
//! `pyjson.rs`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::{SemanticaError, ValidationError};

use crate::common::{
    DecodeErrors, Options, PResult, Source, opt_string, opt_truthy, py_exists, py_name,
    py_path_str, py_read_text,
};
use crate::pyjson;

/// `JSONData`: JSON data representation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct JSONData {
    /// `data`: the parsed (possibly flattened) value.
    pub data: Value,
    /// `type`: `object`, `array`, `string`, `number`, `boolean` or `null`.
    pub r#type: String,
    /// `metadata`: `{"source": ..., "type": ..., "paths": ...}`.
    pub metadata: Map<String, Value>,
}

/// `json.loads(text)` with Python's behaviour and `JSONDecodeError` messages.
pub(crate) fn py_json_loads(text: &str) -> PResult<Value> {
    pyjson::loads(text).map_err(Into::into)
}

/// `JSONParser(**config)`: JSON document parser.
#[derive(Debug, Clone)]
pub struct JSONParser {
    /// `self.logger`.
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker` (enabled on construction, as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for JSONParser {
    fn default() -> Self {
        JSONParser::new(Options::new())
    }
}

impl JSONParser {
    /// `JSONParser(**config)`.
    pub fn new(config: Options) -> Self {
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        JSONParser {
            logger: get_logger("json_parser"),
            config,
            progress_tracker,
        }
    }

    /// `parse(file_path, **options)`: parse a JSON file (a `Path`, or a `str` naming an
    /// existing file) or a JSON string. Options: `encoding` (default `utf-8`), `flatten`,
    /// `extract_paths`.
    pub fn parse(&self, file_path: impl Into<Source>, options: &Options) -> PResult<JSONData> {
        let source_arg: Source = file_path.into();
        let file_path_obj = source_arg.file_path()?;
        let message = format!(
            "JSON: {}",
            file_path_obj
                .as_deref()
                .map(py_name)
                .unwrap_or_else(|| "content".to_string())
        );
        let file_text = file_path_obj.as_deref().map(py_path_str);
        let tracking_id = self.progress_tracker.start_tracking(
            file_text.as_deref(),
            Some("parse"),
            Some("JSONParser"),
            &message,
            None,
        );
        let result = (|| -> PResult<JSONData> {
            let encoding = opt_string(options, "encoding", "utf-8");
            let (data, source) = match &file_path_obj {
                Some(path) => {
                    if !py_exists(path)? {
                        return Err(ValidationError::new(format!(
                            "JSON file not found: {}",
                            py_path_str(path)
                        ))
                        .into());
                    }
                    let text = py_read_text(path, &encoding, DecodeErrors::Strict)?;
                    let data = pyjson::loads(&text).map_err(|e| -> SemanticaError {
                        if e.recursion {
                            e.into()
                        } else {
                            ValidationError::new(format!("Invalid JSON file: {e}")).into()
                        }
                    })?;
                    (data, py_path_str(path))
                }
                None => {
                    let text = source_arg.text().unwrap_or_default();
                    let data = pyjson::loads(text).map_err(|e| -> SemanticaError {
                        if e.recursion {
                            e.into()
                        } else {
                            ValidationError::new(format!("Invalid JSON string: {e}")).into()
                        }
                    })?;
                    (data, "string".to_string())
                }
            };
            let data_type = determine_type(&data);
            let data = if opt_truthy(options, "flatten", false) {
                Value::Object(flatten(&data, "", "."))
            } else {
                data
            };
            let paths = if opt_truthy(options, "extract_paths", false) {
                Value::Array(
                    extract_paths(&data, "")
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                )
            } else {
                Value::Null
            };
            let mut metadata = Map::new();
            metadata.insert("source".to_string(), Value::String(source));
            metadata.insert("type".to_string(), Value::String(data_type.to_string()));
            metadata.insert("paths".to_string(), paths);
            Ok(JSONData {
                data,
                r#type: data_type.to_string(),
                metadata,
            })
        })();
        match &result {
            Ok(data) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Parsed JSON ({})", data.r#type),
                None,
            ),
            Err(e) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("failed"),
                &e.to_string(),
                None,
            ),
        }
        result
    }

    /// `parse_to_dict(file_path, **options)`: an object document itself; for an array of
    /// objects its single element or `{"items": [...]}`; otherwise `{"data": ...}`.
    pub fn parse_to_dict(
        &self,
        file_path: impl Into<Source>,
        options: &Options,
    ) -> PResult<Map<String, Value>> {
        let json_data = self.parse(file_path, options)?;
        if json_data.r#type == "object" {
            return Ok(match json_data.data {
                Value::Object(m) => m,
                other => Map::from_iter([("data".to_string(), other)]),
            });
        }
        if json_data.r#type == "array" {
            match &json_data.data {
                Value::Array(items) if !items.is_empty() && items[0].is_object() => {
                    if items.len() == 1 {
                        return Ok(items[0].as_object().cloned().unwrap_or_default());
                    }
                    return Ok(Map::from_iter([(
                        "items".to_string(),
                        json_data.data.clone(),
                    )]));
                }
                // A flattened array is a dict, and `data[0]` raises `KeyError: 0`.
                Value::Object(m) if !m.is_empty() => return Err(key_error_int(0)),
                _ => {}
            }
        }
        Ok(Map::from_iter([("data".to_string(), json_data.data)]))
    }

    /// `parse_to_list(file_path, **options)`: an array document itself, otherwise
    /// `[data]`. (With `flatten=True` an array document is the flattened dict, which the
    /// original returns as is.)
    pub fn parse_to_list(&self, file_path: impl Into<Source>, options: &Options) -> PResult<Value> {
        let json_data = self.parse(file_path, options)?;
        if json_data.r#type == "array" {
            Ok(json_data.data)
        } else {
            Ok(Value::Array(vec![json_data.data]))
        }
    }

    /// `extract_values(file_path, key_path, **options)`: the values at a dot-separated key
    /// path; lists are traversed element-wise without consuming a key.
    pub fn extract_values(
        &self,
        file_path: impl Into<Source>,
        key_path: &str,
        options: &Options,
    ) -> PResult<Vec<Value>> {
        let json_data = self.parse(file_path, options)?;
        Ok(extract_by_path(&json_data.data, key_path))
    }
}

/// `KeyError(<int>)` (its `str()` is the bare number).
fn key_error_int(key: i64) -> SemanticaError {
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "KeyError".to_string(),
        message: key.to_string(),
    }
}

/// `_determine_type`: note that Python's `bool` is an `int`, so booleans are `"number"`.
fn determine_type(data: &Value) -> &'static str {
    match data {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) | Value::Bool(_) => "number",
        Value::Null => "null",
    }
}

/// `_flatten(data, parent_key="", separator=".")`.
fn flatten(data: &Value, parent_key: &str, separator: &str) -> Map<String, Value> {
    let mut items = Map::new();
    let join = |key: &str| {
        if parent_key.is_empty() {
            key.to_string()
        } else {
            format!("{parent_key}{separator}{key}")
        }
    };
    match data {
        Value::Object(map) => {
            for (key, value) in map {
                let new_key = join(key);
                if value.is_object() || value.is_array() {
                    for (k, v) in flatten(value, &new_key, separator) {
                        items.insert(k, v);
                    }
                } else {
                    items.insert(new_key, value.clone());
                }
            }
        }
        Value::Array(list) => {
            for (idx, value) in list.iter().enumerate() {
                let new_key = join(&idx.to_string());
                if value.is_object() || value.is_array() {
                    for (k, v) in flatten(value, &new_key, separator) {
                        items.insert(k, v);
                    }
                } else {
                    items.insert(new_key, value.clone());
                }
            }
        }
        other => {
            items.insert(parent_key.to_string(), other.clone());
        }
    }
    items
}

/// `_extract_paths(data, prefix="")`: `a`, `a.b`, `a[0]`, ...
fn extract_paths(data: &Value, prefix: &str) -> Vec<String> {
    let mut paths = Vec::new();
    match data {
        Value::Object(map) => {
            for (key, value) in map {
                let current = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                paths.push(current.clone());
                if value.is_object() || value.is_array() {
                    paths.extend(extract_paths(value, &current));
                }
            }
        }
        Value::Array(list) => {
            for (idx, value) in list.iter().enumerate() {
                let current = if prefix.is_empty() {
                    format!("[{idx}]")
                } else {
                    format!("{prefix}[{idx}]")
                };
                paths.push(current.clone());
                if value.is_object() || value.is_array() {
                    paths.extend(extract_paths(value, &current));
                }
            }
        }
        _ => {}
    }
    paths
}

/// `_extract_by_path(data, key_path)`.
fn extract_by_path(data: &Value, key_path: &str) -> Vec<Value> {
    fn recurse(obj: &Value, keys: &[&str], results: &mut Vec<Value>) {
        let Some((key, rest)) = keys.split_first() else {
            results.push(obj.clone());
            return;
        };
        match obj {
            Value::Object(map) => {
                if let Some(v) = map.get(*key) {
                    recurse(v, rest, results);
                }
            }
            Value::Array(items) => {
                for item in items {
                    recurse(item, keys, results);
                }
            }
            _ => {}
        }
    }
    let keys: Vec<&str> = key_path.split('.').collect();
    let mut results = Vec::new();
    recurse(data, &keys, &mut results);
    results
}
