//! Shared helpers for the sem-utils integration tests.
//!
//! `fixtures/oracle.json` holds expected values produced by running the original Python
//! (`fixtures/generate_oracle.py`, executed in the oracle environment).
#![allow(dead_code)]

use once_cell::sync::Lazy;
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

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
    value.as_object().cloned().expect("JSON object")
}

/// The `{"error": type, "message": str}` form the fixture uses for exceptions.
pub fn err_json(err: &SemanticaError) -> Value {
    json!({"error": err.type_name(), "message": err.to_string()})
}

/// The `{"ok": value}` / `{"error": ..., "message": ...}` form of a fallible result.
pub fn outcome<T: Into<Value>>(result: Result<T, SemanticaError>) -> Value {
    match result {
        Ok(v) => json!({"ok": v.into()}),
        Err(e) => err_json(&e),
    }
}

/// The `ok`/`error`/`message` keys of a fixture case.
pub fn expected_outcome(case: &Value) -> Value {
    let mut out = Map::new();
    for key in ["ok", "error", "message"] {
        if let Some(v) = case.get(key) {
            out.insert(key.to_string(), v.clone());
        }
    }
    Value::Object(out)
}
