//! Shared helpers for the sem-parse integration tests.
//!
//! Expected values come from running the original Python in the oracle environment: each
//! area keeps its fixtures and generator under `tests/fixtures/<area>/`
//! (`generate_oracle.py` writes `oracle.json`; the command is in each script's docstring).
#![allow(dead_code)]

use std::path::PathBuf;

use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

/// `tests/fixtures`.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// A fixture file under `tests/fixtures`.
pub fn fixture(rel: &str) -> PathBuf {
    fixtures_dir().join(rel)
}

/// `tests/fixtures/<area>/oracle.json`.
pub fn load_oracle(area: &str) -> Value {
    let path = fixtures_dir().join(area).join("oracle.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("oracle fixture parses")
}

/// A JSON object as a map.
pub fn obj(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().expect("JSON object")
}

/// The `{"error": type, "message": str}` form oracle fixtures use for exceptions.
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

/// Silence the global progress tracker's console output for a test binary.
pub fn quiet_progress() {
    sem_utils::get_progress_tracker().set_displays(Vec::new());
}

/// `tests/fixtures/<area>/files` (input files written by the area's oracle generator).
pub fn files_dir(area: &str) -> PathBuf {
    fixtures_dir().join(area).join("files")
}

/// Replace `from` by `to` in every string of a JSON value (to compare outputs that embed
/// absolute fixture paths, which the generators write as `$FILES`).
pub fn replace_in_strings(value: Value, from: &str, to: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(from, to)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|v| replace_in_strings(v, from, to))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, replace_in_strings(v, from, to)))
                .collect(),
        ),
        other => other,
    }
}

/// The oracle form of a result: `{"ok": <json>}` or `{"error": type, "message": str}`, with
/// the area's files directory written as `$FILES`.
pub fn oracle_form<T: serde::Serialize>(area: &str, result: Result<T, SemanticaError>) -> Value {
    let files = files_dir(area).display().to_string();
    let value = match result {
        Ok(v) => json!({"ok": serde_json::to_value(v).expect("serializable result")}),
        Err(e) => err_json(&e),
    };
    replace_in_strings(value, &files, "$FILES")
}

/// Assert that `actual` (an [`oracle_form`]) equals the oracle's case `section.key`.
pub fn assert_case(oracle: &Value, section: &str, key: &str, actual: Value) {
    let expected = &oracle[section][key];
    assert!(!expected.is_null(), "oracle has no case {section}.{key}");
    assert_eq!(&actual, expected, "case {section}.{key}");
}
