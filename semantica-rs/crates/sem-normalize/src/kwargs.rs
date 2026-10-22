//! Reading Python keyword arguments / config dicts held as `serde_json` maps.

use sem_utils::pycompat::fmt::{PyNum, py_str};
use serde_json::{Map, Value};

/// Python truthiness (`bool(value)`).
pub(crate) fn truthy(value: &Value) -> bool {
    sem_utils::pycompat::fmt::truthy(value)
}

/// A string argument: strings as they are, other values as their `str()`.
pub(crate) fn str_arg(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => py_str(other),
    }
}

/// `config.get(key, default)` for a string option (non-strings fall back to `default`).
pub(crate) fn get_str(map: &Map<String, Value>, key: &str, default: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => s.clone(),
        _ => default.to_string(),
    }
}

/// `config.get(key, default)` for a numeric option (`bool` counts as `int`).
pub(crate) fn get_f64(map: &Map<String, Value>, key: &str, default: f64) -> f64 {
    map.get(key)
        .and_then(PyNum::of)
        .map(PyNum::to_f64)
        .unwrap_or(default)
}

/// `config.get(key, default)` for a list of strings (non-string items are skipped).
pub(crate) fn get_str_list(map: &Map<String, Value>, key: &str, default: &[&str]) -> Vec<String> {
    match map.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(s)) => s.chars().map(|c| c.to_string()).collect(),
        _ => default.iter().map(|s| s.to_string()).collect(),
    }
}
