//! Shared helpers for the sem-change integration tests.
#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard};

use regex::Regex;
use sem_utils::SemanticaError;
use serde_json::{Map, Value};

/// A JSON object literal as a map (`dict`).
pub fn obj(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("expected an object, got {other}"),
    }
}

/// `tempfile.NamedTemporaryFile(suffix=".db", delete=False)`: an empty file that is removed
/// when the returned path is dropped (the original removes it in a `finally` block).
pub fn temp_db() -> tempfile::TempPath {
    tempfile::Builder::new()
        .suffix(".db")
        .tempfile()
        .expect("create temporary database file")
        .into_temp_path()
}

/// The path as the `&str` the managers take.
pub fn path_str(path: &std::path::Path) -> &str {
    path.to_str().expect("UTF-8 temporary path")
}

/// `pytest.raises(<kind>, match=pattern)`: the error satisfies `kind` and `re.search(pattern,
/// str(err))` matches.
pub fn assert_raises<T: std::fmt::Debug>(
    result: Result<T, SemanticaError>,
    kind: fn(&SemanticaError) -> bool,
    pattern: &str,
) -> SemanticaError {
    let err = result.expect_err("expected an exception");
    assert!(kind(&err), "unexpected exception kind: {err:?}");
    let re = Regex::new(pattern).expect("valid pattern");
    assert!(
        re.is_match(&err.to_string()),
        "{pattern:?} not found in {err}"
    );
    err
}

/// `isinstance(e, ValidationError)`.
pub fn is_validation(err: &SemanticaError) -> bool {
    err.is_validation_error()
}

/// `isinstance(e, ProcessingError)`.
pub fn is_processing(err: &SemanticaError) -> bool {
    matches!(err, SemanticaError::Processing(_))
}

/// `isinstance(e, Exception)`.
pub fn is_any(_: &SemanticaError) -> bool {
    true
}

/// Timing-sensitive tests of one test binary run one at a time (pytest runs them serially;
/// libtest would otherwise run them in parallel threads of one process).
pub fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Silence the global progress tracker's console output in tests that construct ontology
/// managers (they enable it, as the original does).
pub fn quiet_progress() {
    sem_utils::get_progress_tracker().set_displays(Vec::new());
}

/// `tests/fixtures/oracle.json`: expected values computed by running the original (see
/// `tests/fixtures/generate_oracle.py`).
pub fn oracle() -> &'static Value {
    static ORACLE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    ORACLE.get_or_init(|| {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/oracle.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("oracle fixture"))
            .expect("valid oracle JSON")
    })
}

/// The oracle's `run(f)` record: `{"ok": value}` or `{"error": class, "message": str(e)}`.
pub fn outcome<T: serde::Serialize>(result: Result<T, SemanticaError>) -> Value {
    match result {
        Ok(value) => serde_json::json!({"ok": serde_json::to_value(value).expect("serializable")}),
        Err(e) => serde_json::json!({"error": e.type_name(), "message": e.to_string()}),
    }
}

/// The oracle's `mask(value, keys)`: time-dependent fields replaced by `"<masked>"`.
pub fn mask(value: &Value, keys: &[&str]) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = if keys.contains(&k.as_str()) {
                        Value::String("<masked>".into())
                    } else {
                        mask(v, keys)
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| mask(v, keys)).collect()),
        other => other.clone(),
    }
}

/// Lists Python builds from sets have no reproducible order: compare them as multisets.
pub fn assert_same_items(actual: &Value, expected: &Value, context: &str) {
    let (a, e) = (
        actual
            .as_array()
            .unwrap_or_else(|| panic!("{context}: not a list: {actual}")),
        expected
            .as_array()
            .unwrap_or_else(|| panic!("{context}: not a list: {expected}")),
    );
    assert_eq!(a.len(), e.len(), "{context}: {actual} vs {expected}");
    let mut remaining: Vec<&Value> = e.iter().collect();
    for item in a {
        let pos = remaining
            .iter()
            .position(|x| *x == item)
            .unwrap_or_else(|| panic!("{context}: unexpected {item} (expected {expected})"));
        remaining.remove(pos);
    }
}

/// Compare a result map with the oracle's, treating the listed keys as unordered lists.
pub fn assert_map_with_sets(actual: &Value, expected: &Value, set_keys: &[&str], context: &str) {
    let (a, e) = (actual.as_object().unwrap(), expected.as_object().unwrap());
    let a_keys: Vec<&String> = a.keys().collect();
    let e_keys: Vec<&String> = e.keys().collect();
    assert_eq!(a_keys, e_keys, "{context}: keys");
    for (key, value) in e {
        if set_keys.contains(&key.as_str()) {
            assert_same_items(&a[key], value, &format!("{context}.{key}"));
        } else {
            assert_eq!(&a[key], value, "{context}.{key}");
        }
    }
}
