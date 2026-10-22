//! Helpers for `**kwargs`-style configuration dictionaries (`serde_json::Map`), with Python
//! truthiness and `dict.get(key, default)` semantics.

use serde_json::{Map, Value};

/// A `**kwargs` / config dictionary.
pub type Kwargs = Map<String, Value>;

/// `config = config or {}; config.update(kwargs)`.
pub fn merged(config: Option<&Kwargs>, kwargs: &Kwargs) -> Kwargs {
    let mut out = config.cloned().unwrap_or_default();
    for (k, v) in kwargs {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// Python truthiness of a JSON value.
pub fn truthy(value: &Value) -> bool {
    sem_utils::pycompat::fmt::truthy(value)
}

/// `bool(m.get(key, default))`.
pub fn get_truthy(m: &Kwargs, key: &str, default: bool) -> bool {
    m.get(key).map(truthy).unwrap_or(default)
}

/// `m.get(key, default)` as a float (numbers and numeric strings; else `default`).
pub fn get_f64(m: &Kwargs, key: &str, default: f64) -> f64 {
    match m.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => default,
    }
}

/// `m.get(key)` as an integer.
pub fn get_i64(m: &Kwargs, key: &str) -> Option<i64> {
    match m.get(key) {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::Bool(b)) => Some(i64::from(*b)),
        _ => None,
    }
}

/// `m.get(key)` as a string.
pub fn get_str(m: &Kwargs, key: &str) -> Option<String> {
    match m.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// `m.get(key)` as a list of strings (non-string items are converted with `str()`).
pub fn get_str_list(m: &Kwargs, key: &str) -> Option<Vec<String>> {
    match m.get(key) {
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => sem_utils::pycompat::fmt::py_str(other),
                })
                .collect(),
        ),
        Some(Value::String(s)) => Some(vec![s.clone()]),
        _ => None,
    }
}

/// A JSON object from `(key, value)` pairs.
pub fn obj<I, K>(pairs: I) -> Kwargs
where
    I: IntoIterator<Item = (K, Value)>,
    K: Into<String>,
{
    pairs.into_iter().map(|(k, v)| (k.into(), v)).collect()
}

/// `Value::String` or `null`.
pub fn opt_str(s: Option<&str>) -> Value {
    match s {
        Some(s) => Value::String(s.to_string()),
        None => Value::Null,
    }
}
