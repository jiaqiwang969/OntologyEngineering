//! Port of `tests/normalize/test_data_cleaner.py` (`TestDataCleaner`, `TestDuplicateDetector`,
//! `TestDataValidator`, `TestMissingValueHandler`).
#![allow(non_snake_case)]

mod common;

use std::collections::HashSet;

use sem_normalize::data_cleaner::{PyType, Record};
use sem_normalize::{
    DataCleaner, DataValidator, DuplicateDetector, DuplicateGroup, MissingValueHandler,
};
use serde_json::{Map, Value, json};

use common::{rec, records};

fn data_cleaner_dataset() -> Vec<Record> {
    records(json!([
        {"id": 1, "name": "John Doe", "age": 30, "email": "john@example.com"},
        {"id": 2, "name": "Jane Smith", "age": 25, "email": "jane@example.com"},
        {"id": 3, "name": "John Doe", "age": 30, "email": "john@example.com"},
        {"id": 4, "name": "Bob", "age": null, "email": "bob@example.com"}
    ]))
}

fn ids(records: &[Record]) -> Vec<Value> {
    records.iter().map(|r| r["id"].clone()).collect()
}

fn kwargs(value: Value) -> Map<String, Value> {
    value.as_object().expect("object").clone()
}

#[test]
fn test_data_cleaner__test_clean_data_comprehensive() {
    let cleaner = DataCleaner::default();
    let cleaned = cleaner
        .clean_data(
            &data_cleaner_dataset(),
            true,
            false,
            true,
            &kwargs(json!({
                "duplicate_criteria": {"key_fields": ["name", "age", "email"]},
                "missing_strategy": "remove"
            })),
        )
        .unwrap();
    // id 3 removed (duplicate of 1), id 4 removed (missing age).
    assert_eq!(cleaned.len(), 2);
    let ids = ids(&cleaned);
    assert!(ids.contains(&json!(1)));
    assert!(ids.contains(&json!(2)));
    assert!(!ids.contains(&json!(3)));
    assert!(!ids.contains(&json!(4)));
}

#[test]
fn test_data_cleaner__test_clean_data_fill_missing() {
    let cleaner = DataCleaner::default();
    let cleaned = cleaner
        .clean_data(
            &data_cleaner_dataset(),
            true,
            false,
            true,
            &kwargs(json!({
                "duplicate_criteria": {"key_fields": ["name", "age", "email"]},
                "missing_strategy": "fill",
                "fill_value": 0
            })),
        )
        .unwrap();
    // id 3 removed (duplicate), id 4 kept with age filled with 0.
    assert_eq!(cleaned.len(), 3);
    let ids = ids(&cleaned);
    assert!(ids.contains(&json!(1)));
    assert!(ids.contains(&json!(2)));
    assert!(ids.contains(&json!(4)));
    let bob = cleaned.iter().find(|r| r["id"] == json!(4)).unwrap();
    assert_eq!(bob["age"], json!(0));
}

fn duplicate_dataset() -> Vec<Record> {
    records(json!([
        {"id": 1, "name": "John Doe", "city": "New York"},
        {"id": 2, "name": "Jane Smith", "city": "Los Angeles"},
        {"id": 3, "name": "John Doe", "city": "New York"},
        {"id": 4, "name": "Jon Doe", "city": "New York"},
        {"id": 5, "name": "Alice", "city": "Chicago"}
    ]))
}

fn detector() -> DuplicateDetector {
    DuplicateDetector::new(kwargs(json!({"similarity_threshold": 0.8})))
}

fn name_city() -> Vec<String> {
    vec!["name".to_string(), "city".to_string()]
}

fn id_set(group: &DuplicateGroup) -> HashSet<i64> {
    group
        .records
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect()
}

#[test]
fn test_duplicate_detector__test_detect_exact_duplicates() {
    let duplicates =
        detector().detect_duplicates(&duplicate_dataset(), Some(1.0), Some(&name_city()));
    assert_eq!(duplicates.len(), 1);
    let group = &duplicates[0];
    assert_eq!(group.records.len(), 2);
    assert_eq!(id_set(group), HashSet::from([1, 3]));
    assert_eq!(group.similarity_score, 1.0);
}

#[test]
fn test_duplicate_detector__test_detect_fuzzy_duplicates() {
    let duplicates =
        detector().detect_duplicates(&duplicate_dataset(), Some(0.8), Some(&name_city()));
    assert!(!duplicates.is_empty());
    let group = duplicates
        .iter()
        .find(|g| g.records.iter().any(|r| r["id"] == json!(1)))
        .expect("a group containing id 1");
    let ids = id_set(group);
    assert!(ids.contains(&1));
    assert!(ids.contains(&3));
    assert!(ids.contains(&4));
}

#[test]
fn test_duplicate_detector__test_calculate_similarity() {
    let detector = detector();
    let r1 = rec(json!({"a": "hello", "b": 10}));
    let r2 = rec(json!({"a": "hello", "b": 10}));
    assert_eq!(detector.calculate_similarity(&r1, &r2, None), 1.0);
    let r3 = rec(json!({"a": "hallo", "b": 10}));
    // "hello" vs "hallo": 3/5 = 0.6; b: 1.0; mean 0.8.
    assert!((detector.calculate_similarity(&r1, &r3, None) - 0.8).abs() < 1e-7);
}

fn resolve_group() -> DuplicateGroup {
    DuplicateGroup::new(
        records(json!([
            {"id": 1, "val": "A", "extra": null},
            {"id": 2, "val": "A", "extra": "data"}
        ])),
        1.0,
        Some(rec(json!({"id": 1, "val": "A", "extra": null}))),
    )
}

#[test]
fn test_duplicate_detector__test_resolve_duplicates_keep_first() {
    let resolved = detector()
        .resolve_duplicates(&[resolve_group()], Some("keep_first"))
        .unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0]["id"], json!(1));
}

#[test]
fn test_duplicate_detector__test_resolve_duplicates_merge() {
    let resolved = detector()
        .resolve_duplicates(&[resolve_group()], Some("merge"))
        .unwrap();
    assert_eq!(resolved.len(), 1);
    // 'extra' comes from the second record since the first was None.
    assert_eq!(resolved[0]["extra"], json!("data"));
    assert_eq!(resolved[0]["val"], json!("A"));
}

fn schema() -> Map<String, Value> {
    kwargs(json!({
        "fields": {
            "name": {"type": "str", "required": true},
            "age": {"type": "int", "required": false},
            "tags": {"type": "list", "required": false}
        }
    }))
}

#[test]
fn test_data_validator__test_validate_valid_record() {
    let validator = DataValidator::default();
    let record = rec(json!({"name": "Test", "age": 20, "tags": ["a", "b"]}));
    let result = validator.validate_record(&record, Some(&schema())).unwrap();
    assert!(result.valid);
    assert_eq!(result.errors.len(), 0);
}

#[test]
fn test_data_validator__test_validate_missing_required() {
    let validator = DataValidator::default();
    let record = rec(json!({"age": 20}));
    let result = validator.validate_record(&record, Some(&schema())).unwrap();
    assert!(!result.valid);
    assert!(result.errors.iter().any(|e| e["field"] == json!("name")));
}

#[test]
fn test_data_validator__test_validate_wrong_type() {
    let validator = DataValidator::default();
    let record = rec(json!({"name": "Test", "age": "twenty"}));
    let result = validator.validate_record(&record, Some(&schema())).unwrap();
    assert!(!result.valid);
    assert!(result.errors.iter().any(|e| e["field"] == json!("age")));
}

#[test]
fn test_data_validator__test_check_data_types() {
    let validator = DataValidator::default();
    assert!(validator.check_data_types(&json!("test"), &PyType::Str.into()));
    assert!(validator.check_data_types(&json!(123), &PyType::Int.into()));
    assert!(validator.check_data_types(&json!(123), &vec![PyType::Str, PyType::Int].into()));
    assert!(validator.check_data_types(&json!("123"), &vec!["str", "int"].into()));
    assert!(!validator.check_data_types(&json!(123), &PyType::Str.into()));
}

fn missing_dataset() -> Vec<Record> {
    records(json!([
        {"a": 1, "b": 2},
        {"a": null, "b": 2},
        {"a": 3, "b": null},
        {"a": 10, "b": 20}
    ]))
}

#[test]
fn test_missing_value_handler__test_identify_missing_values() {
    let handler = MissingValueHandler::default();
    let info = handler.identify_missing_values(&missing_dataset());
    assert_eq!(info["total_records"], json!(4));
    assert_eq!(info["missing_counts"]["a"], json!(1));
    assert_eq!(info["missing_counts"]["b"], json!(1));
}

#[test]
fn test_missing_value_handler__test_handle_missing_remove() {
    let handler = MissingValueHandler::default();
    let cleaned = handler.handle_missing_values(&missing_dataset(), Some("remove"), &Map::new());
    assert_eq!(cleaned.len(), 2);
    for r in &cleaned {
        assert!(!r["a"].is_null());
        assert!(!r["b"].is_null());
    }
}

#[test]
fn test_missing_value_handler__test_handle_missing_fill() {
    let handler = MissingValueHandler::default();
    let cleaned = handler.handle_missing_values(
        &missing_dataset(),
        Some("fill"),
        &kwargs(json!({"fill_value": 0})),
    );
    assert_eq!(cleaned.len(), 4);
    assert_eq!(cleaned[1]["a"], json!(0));
    assert_eq!(cleaned[2]["b"], json!(0));
}

#[test]
fn test_missing_value_handler__test_handle_missing_impute_mean() {
    let handler = MissingValueHandler::default();
    let cleaned = handler.handle_missing_values(
        &missing_dataset(),
        Some("impute"),
        &kwargs(json!({"method": "mean"})),
    );
    assert_eq!(cleaned.len(), 4);
    // a: 1, 3, 10 -> 14/3; b: 2, 2, 20 -> 8.0
    let a = cleaned[1]["a"].as_f64().unwrap();
    assert!((a - 4.6666666).abs() < 0.5e-5, "{a}");
    assert_eq!(cleaned[2]["b"], json!(8.0));
}

#[test]
fn test_missing_value_handler__test_handle_missing_impute_median() {
    let handler = MissingValueHandler::default();
    let dataset = records(json!([{"a": 1}, {"a": 3}, {"a": 10}, {"a": null}]));
    let cleaned = handler.handle_missing_values(
        &dataset,
        Some("impute"),
        &kwargs(json!({"method": "median"})),
    );
    assert_eq!(cleaned[3]["a"], json!(3));
}

#[test]
fn test_missing_value_handler__test_handle_missing_impute_zero() {
    let handler = MissingValueHandler::default();
    let dataset = records(json!([{"a": 1}, {"a": null}]));
    let cleaned =
        handler.handle_missing_values(&dataset, Some("impute"), &kwargs(json!({"method": "zero"})));
    assert_eq!(cleaned[1]["a"], json!(0));
}
