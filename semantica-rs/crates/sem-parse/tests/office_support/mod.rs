//! Helpers shared by the office parser tests (`parse_docx_parser`, `parse_pptx_parser`,
//! `parse_excel_parser`): oracle cases from `tests/fixtures/office/oracle.json` (written by
//! `tests/fixtures/office/generate_oracle.py` in the oracle environment) and comparison of
//! port results with them.
#![allow(dead_code)]

use std::path::PathBuf;

use sem_parse::Options;
use sem_utils::SemanticaError;
use serde_json::{Value, json};

/// `tests/fixtures/office`.
pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("office")
}

/// A fixture file.
pub fn file(name: &str) -> PathBuf {
    dir().join(name)
}

/// The oracle cases of one section (`docx`, `pptx`, `excel`).
pub fn cases(section: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(dir().join("oracle.json")).expect("office oracle");
    let all: Value = serde_json::from_str(&text).expect("oracle parses");
    all[section].as_array().expect("section").clone()
}

/// The cases whose name starts with `prefix`.
pub fn cases_named(section: &str, prefix: &str) -> Vec<Value> {
    let found: Vec<Value> = cases(section)
        .into_iter()
        .filter(|c| c["name"].as_str().unwrap_or("").starts_with(prefix))
        .collect();
    assert!(!found.is_empty(), "no oracle cases for {section}/{prefix}");
    found
}

/// The case's `options` as keyword arguments.
pub fn options(case: &Value) -> Options {
    case["options"].as_object().cloned().unwrap_or_default()
}

fn scrub(value: Value) -> Value {
    let d = dir().display().to_string();
    match value {
        Value::String(s) => Value::String(s.replace(&d, "{DIR}")),
        Value::Array(items) => Value::Array(items.into_iter().map(scrub).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k.replace(&d, "{DIR}"), scrub(v)))
                .collect(),
        ),
        other => other,
    }
}

/// The generator's `mask_now`: default core properties carry the current time.
fn mask_now(mut value: Value) -> Value {
    if let Some(meta) = value.get_mut("metadata").and_then(Value::as_object_mut) {
        let by = meta.get("last_modified_by").and_then(Value::as_str);
        if matches!(by, Some("python-docx") | Some("python-pptx")) {
            meta.insert("modified".to_string(), json!("{NOW}"));
        }
    }
    value
}

/// The oracle form of a port result: `{"ok": value}` or `{"error": type, "message": str}`.
pub fn outcome(result: Result<Value, SemanticaError>) -> Value {
    match result {
        Ok(v) => json!({"ok": mask_now(scrub(v))}),
        Err(e) => json!({"error": e.type_name(), "message": scrub(Value::String(e.to_string()))}),
    }
}

/// Compare a port result with an oracle case.
pub fn check(case: &Value, result: Result<Value, SemanticaError>) {
    let actual = outcome(result);
    let mut expected = serde_json::Map::new();
    for key in ["ok", "error", "message"] {
        if let Some(v) = case.get(key) {
            expected.insert(key.to_string(), v.clone());
        }
    }
    let expected = Value::Object(expected);
    if actual != expected {
        panic!(
            "case {} differs\n--- expected\n{}\n--- actual\n{}",
            case["name"],
            serde_json::to_string_pretty(&expected).unwrap_or_default(),
            serde_json::to_string_pretty(&actual).unwrap_or_default()
        );
    }
}

/// The JSON form of a serialisable result.
pub fn to_json<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).expect("serialisable")
}
