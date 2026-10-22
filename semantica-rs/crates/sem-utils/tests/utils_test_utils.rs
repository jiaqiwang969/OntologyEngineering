//! Port of `tests/utils/test_utils.py` (`TestHelpers`, `TestValidators`).
#![allow(non_snake_case)]

use indexmap::IndexMap;
use sem_utils::helpers;
use sem_utils::validators::{self, Constraints, PyType};
use serde_json::{Map, Value, json};

fn obj(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("object literal")
}

#[test]
fn test_helpers__test_clean_text() {
    assert_eq!(
        helpers::clean_text("  Hello   World  ", None),
        "Hello World"
    );
    assert_eq!(helpers::clean_text("Line 1\nLine 2", None), "Line 1 Line 2");
}

#[test]
fn test_helpers__test_format_data_json() {
    let data = json!({"key": "value"});
    let formatted = helpers::format_data(&data, Some("json")).unwrap();
    assert!(formatted.contains("\"key\": \"value\""));
}

#[test]
fn test_helpers__test_format_data_invalid() {
    // assertRaises(ValueError)
    let err = helpers::format_data(&json!({}), Some("unknown")).unwrap_err();
    assert!(err.is_value_error(), "{err:?}");
}

#[test]
fn test_helpers__test_ensure_directory() {
    // The original patches `Path.mkdir` and asserts one call; Rust cannot patch the
    // filesystem, so the call is observed through its effect in a temporary directory.
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("test_dir");
    assert!(!target.exists());
    let created = helpers::ensure_directory(&target).unwrap();
    assert_eq!(created, target);
    assert!(target.is_dir());
}

#[test]
fn test_helpers__test_merge_dicts() {
    let dict1 = obj(json!({"a": 1, "b": {"c": 2}}));
    let dict2 = obj(json!({"b": {"d": 3}, "e": 4}));
    let merged = helpers::merge_dicts(&[&dict1, &dict2], Some(true));
    assert_eq!(
        Value::Object(merged),
        json!({"a": 1, "b": {"c": 2, "d": 3}, "e": 4})
    );
}

#[test]
fn test_helpers__test_flatten_dict() {
    let data = obj(json!({"a": {"b": 1, "c": 2}}));
    let result = helpers::flatten_dict(&data, None, None).unwrap();
    assert_eq!(Value::Object(result), json!({"a.b": 1, "a.c": 2}));
}

#[test]
fn test_helpers__test_flatten_dict_key_collision() {
    let data = obj(json!({"a.b": 1, "a": {"b": 2}}));
    // assertRaises(ValueError)
    let err = helpers::flatten_dict(&data, None, None).unwrap_err();
    assert!(err.is_value_error(), "{err:?}");
}

#[test]
fn test_helpers__test_safe_import_returns_module_and_flag() {
    let (module, available) = helpers::safe_import("json", None, None, None);
    assert!(available);
    // assertIs(module, json): the handle is the `json` module.
    assert_eq!(module.expect("module").name(), "json");
}

#[test]
fn test_validators__test_validate_data_required_fields() {
    let data = json!({"name": "Alice"});
    let constraints = Constraints {
        required_fields: Some(vec!["name".into(), "age".into()]),
        ..Default::default()
    };
    let (is_valid, error) = validators::validate_data(&data, None, &constraints).unwrap();
    assert!(!is_valid);
    assert!(error.unwrap().contains("age"));
}

#[test]
fn test_validators__test_validate_data_types() {
    let data = json!({"name": "Alice", "age": "30"});
    let constraints = Constraints {
        field_types: Some(IndexMap::from([
            ("name".to_string(), PyType::Str),
            ("age".to_string(), PyType::Int),
        ])),
        ..Default::default()
    };
    let (is_valid, error) = validators::validate_data(&data, None, &constraints).unwrap();
    assert!(!is_valid);
    assert!(error.unwrap().contains("age"));
}

#[test]
fn test_validators__test_validate_entity() {
    let entity = obj(json!({"id": "e1", "text": "Alice", "type": "Person"}));
    let (is_valid, _error) = validators::validate_entity(&entity);
    assert!(is_valid);
}

#[test]
fn test_validators__test_validate_entity_invalid() {
    let entity = obj(json!({"text": "Alice"})); // Missing id and type
    let (is_valid, _error) = validators::validate_entity(&entity);
    assert!(!is_valid);
}

#[test]
fn test_validators__test_validate_url() {
    assert!(validators::validate_url("https://example.com").0);
    assert!(!validators::validate_url("invalid-url").0);
}

#[test]
fn test_validators__test_validate_email() {
    assert!(validators::validate_email("test@example.com").0);
    assert!(!validators::validate_email("invalid-email").0);
}
