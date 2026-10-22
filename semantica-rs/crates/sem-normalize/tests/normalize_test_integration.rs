//! Port of `tests/normalize/test_integration.py` (`TestNormalizeIntegration`).
#![allow(non_snake_case)]

mod common;

use std::sync::Arc;

use sem_normalize::method_registry;
use sem_normalize::methods;
use sem_normalize::number_normalizer::NumberValue;
use serde_json::{Map, Value, json};

use common::{kw, records};

#[test]
fn test_normalize_integration__test_normalize_text_integration() {
    let text = "Hello   World";
    // Default
    let normalized = methods::normalize_text(text, None, &Map::new()).unwrap();
    assert_eq!(normalized, "Hello World");
    // With kwargs
    let normalized_lower =
        methods::normalize_text(text, None, &kw(json!({"case": "lower"}))).unwrap();
    assert_eq!(normalized_lower, "hello world");
}

#[test]
fn test_normalize_integration__test_normalize_date_integration() {
    let date_str = "2023-01-01";
    // Default ISO
    let normalized = methods::normalize_date(date_str, None, None, None, &Map::new()).unwrap();
    assert_eq!(normalized, "2023-01-01T00:00:00+00:00");
    // Relative: the result is a string.
    let relative: String =
        methods::normalize_date("yesterday", None, None, Some("relative"), &Map::new()).unwrap();
    assert!(!relative.is_empty());
}

#[test]
fn test_normalize_integration__test_normalize_number_integration() {
    // Default
    let num = methods::normalize_number("1,234.56", None, &Map::new()).unwrap();
    assert_eq!(num, NumberValue::Float(1234.56));
    // Quantity
    let qty = methods::normalize_quantity("1 km", None, &Map::new()).unwrap();
    assert_eq!(qty["value"], json!(1.0));
    assert_eq!(qty["unit"], json!("kilometer"));
}

#[test]
fn test_normalize_integration__test_normalize_entity_integration() {
    let normalized =
        methods::normalize_entity("  john   doe  ", Some("Person"), None, &Map::new()).unwrap();
    assert_eq!(normalized, "John Doe");
}

#[test]
fn test_normalize_integration__test_clean_data_integration() {
    let dataset = records(json!([
        {"id": 1, "val": "A"},
        {"id": 1, "val": "A"},
        {"id": 2, "val": "B"}
    ]));
    let cleaned = methods::clean_data(
        &dataset,
        true,
        true,
        true,
        None,
        &kw(json!({"duplicate_criteria": {"key_fields": ["id", "val"]}})),
    )
    .unwrap();
    assert_eq!(cleaned.len(), 2);
}

#[test]
fn test_normalize_integration__test_config_override() {
    // Keyword arguments override the method configuration.
    let res = methods::normalize_text("HELLO", None, &kw(json!({"case": "lower"}))).unwrap();
    assert_eq!(res, "hello");
}

#[test]
fn test_normalize_integration__test_registry_custom_method() {
    let custom_text_normalizer = Arc::new(
        |args: &[Value],
         _kwargs: &Map<String, Value>|
         -> Result<Value, sem_utils::SemanticaError> {
            let text = args[0].as_str().unwrap_or_default();
            Ok(json!(format!("CUSTOM: {text}")))
        },
    );
    method_registry.register("text", "my_custom", custom_text_normalizer);

    let res = methods::normalize_text("hello", Some("my_custom"), &Map::new()).unwrap();
    assert_eq!(res, "CUSTOM: hello");

    // Clean up (method_registry._methods["text"].pop("my_custom", None)).
    method_registry.unregister("text", "my_custom");
}
