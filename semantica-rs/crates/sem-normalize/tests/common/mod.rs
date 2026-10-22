//! Shared helpers of the sem-normalize tests (the Rust counterpart of the test package's
//! `tests/normalize/__init__.py`).
//!
//! `fixtures/oracle.json` holds expected values produced by running the original Python
//! (`fixtures/generate_oracle.py`, executed in the oracle environment).
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use sem_normalize::data_cleaner::Record;
use sem_utils::SemanticaError;
use sem_utils::progress_tracker::{ProgressDisplay, ProgressItem, ProgressTracker};
use serde_json::{Map, Value};

static ORACLE: Lazy<Value> = Lazy::new(|| {
    serde_json::from_str(include_str!("../fixtures/oracle.json")).expect("oracle fixture parses")
});

/// One section of the oracle fixture.
pub fn oracle(key: &str) -> &'static Value {
    let value = &ORACLE[key];
    assert!(!value.is_null(), "oracle fixture has no section {key}");
    value
}

/// A JSON object as a map.
pub fn obj(value: &Value) -> Map<String, Value> {
    value.as_object().expect("a JSON object").clone()
}

/// Keyword arguments from a JSON object literal.
pub fn kw(value: Value) -> Map<String, Value> {
    obj(&value)
}

/// A record from a JSON object literal.
pub fn rec(value: Value) -> Record {
    obj(&value)
}

/// Records from a JSON array of objects.
pub fn records(value: Value) -> Vec<Record> {
    value
        .as_array()
        .expect("a JSON array")
        .iter()
        .map(obj)
        .collect()
}

/// `{"error": type name, "message": str(e)}`, as the oracle script records exceptions.
pub fn err_json(err: &SemanticaError) -> Value {
    serde_json::json!({"error": err.type_name(), "message": err.to_string()})
}

/// `{"ok": value}` or the error, as the oracle script records calls.
pub fn outcome<T: Into<Value>>(result: Result<T, SemanticaError>) -> Value {
    match result {
        Ok(v) => serde_json::json!({"ok": v.into()}),
        Err(e) => err_json(&e),
    }
}

/// The text argument of a case (`None` for JSON null).
pub fn text_opt(value: &Value) -> Option<&str> {
    value.as_str()
}

/// `{"bytes": [...]}` as bytes, a string as text (the generator's encoding of `bytes | str`).
pub fn data_arg(value: &Value) -> sem_normalize::encoding_handler::TextOrBytes {
    use sem_normalize::encoding_handler::TextOrBytes;
    match value {
        Value::String(s) => TextOrBytes::Text(s.clone()),
        Value::Object(map) => TextOrBytes::Bytes(
            map["bytes"]
                .as_array()
                .expect("byte list")
                .iter()
                .map(|b| b.as_u64().expect("byte") as u8)
                .collect(),
        ),
        other => panic!("not a str/bytes argument: {other}"),
    }
}

/// The JSON form the generator gives a `str | bytes` result.
pub fn data_json(value: &sem_normalize::encoding_handler::TextOrBytes) -> Value {
    use sem_normalize::encoding_handler::TextOrBytes;
    match value {
        TextOrBytes::Text(s) => Value::String(s.clone()),
        TextOrBytes::Bytes(b) => serde_json::json!({"bytes": b}),
    }
}

/// The JSON form the generator gives a float (non-finite values tagged).
pub fn float_json(x: f64) -> Value {
    if x.is_nan() {
        serde_json::json!({"float": "nan"})
    } else if x.is_infinite() {
        serde_json::json!({"float": if x > 0.0 { "inf" } else { "-inf" }})
    } else {
        serde_json::json!(x)
    }
}

/// Compare an outcome with a case's recorded `result`, naming the case on failure.
pub fn check(actual: Value, case: &Value) {
    assert_eq!(actual, case["result"], "case {case}");
}

/// A progress display that records every update it receives.
#[derive(Default)]
pub struct CaptureDisplay {
    pub updates: Mutex<Vec<ProgressItem>>,
}

impl ProgressDisplay for CaptureDisplay {
    fn update(&self, item: &ProgressItem) {
        self.updates.lock().unwrap().push(item.clone());
    }

    fn show_summary(&self, _items: &[ProgressItem]) {}

    fn clear(&self) {}
}

/// A fresh, enabled progress tracker whose only display records updates (the Rust stand-in
/// for the original tests' mocked tracker).
pub fn capture_tracker() -> (Arc<ProgressTracker>, Arc<CaptureDisplay>) {
    let tracker = Arc::new(ProgressTracker::new(true, false, 0.0));
    let display = Arc::new(CaptureDisplay::default());
    tracker.set_displays(vec![display.clone()]);
    (tracker, display)
}
