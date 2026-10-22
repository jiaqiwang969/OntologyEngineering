//! Characterisation of `data_cleaner` (`DataCleaner`, `DuplicateDetector`, `DataValidator`,
//! `MissingValueHandler`, `DuplicateGroup`, `ValidationResult`) against the oracle.

mod common;

use sem_normalize::data_cleaner::{ExpectedTypes, Record};
use sem_normalize::{
    DataCleaner, DataValidator, DuplicateDetector, DuplicateGroup, MissingValueHandler,
    ValidationResult,
};
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

use common::{check, err_json, kw, obj, oracle, records};

fn cases(key: &str) -> &'static Vec<Value> {
    oracle(key).as_array().expect("a list of cases")
}

/// The DATASET of the oracle script.
fn dataset() -> Vec<Record> {
    records(json!([
        {"id": 1, "name": "John Doe", "age": 30, "email": "john@example.com", "score": 1.5},
        {"id": 2, "name": "Jane Smith", "age": 25, "email": "jane@example.com", "score": 2},
        {"id": 3, "name": "John Doe", "age": 30, "email": "john@example.com", "score": 1.5},
        {"id": 4, "name": "Bob", "age": null, "email": "bob@example.com", "score": true},
        {"id": 5, "name": "N/A", "age": 40, "email": "", "score": null},
        {"id": 6, "name": "john doe", "age": 30.0, "email": "JOHN@example.com", "score": 1}
    ]))
}

fn json_of<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
}

fn groups_outcome(groups: Vec<DuplicateGroup>) -> Value {
    json!({"ok": json_of(&groups)})
}

fn validation_outcome(result: Result<ValidationResult, SemanticaError>) -> Value {
    match result {
        Ok(v) => json!({"ok": json_of(&v)}),
        Err(e) => err_json(&e),
    }
}

fn key_fields(value: &Value) -> Option<Vec<String>> {
    value.as_array().map(|items| {
        items
            .iter()
            .map(|i| i.as_str().unwrap().to_string())
            .collect()
    })
}

fn schema(value: &Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[test]
fn test_clean_data_matches_oracle() {
    let cleaner = DataCleaner::default();
    let data = dataset();
    for case in cases("clean_data") {
        let flags: Vec<bool> = case["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_bool().unwrap())
            .collect();
        let result =
            cleaner.clean_data(&data, flags[0], flags[1], flags[2], &obj(&case["options"]));
        check(common::outcome(result.map(|r| json_of(&r))), case);
    }
    assert_eq!(
        common::outcome(
            cleaner
                .clean_data(&[], true, true, true, &Map::new())
                .map(|r| json_of(&r))
        ),
        *oracle("clean_data_empty")
    );
}

#[test]
fn test_data_cleaner_delegates_match_oracle() {
    let cleaner = DataCleaner::default();
    let data = dataset();
    for case in cases("cleaner_detect_duplicates") {
        let fields = key_fields(&case["key_fields"]);
        let result =
            cleaner.detect_duplicates(&data, case["threshold"].as_f64(), fields.as_deref());
        check(groups_outcome(result), case);
    }
    for case in cases("cleaner_validate_data") {
        let data = records(case["dataset"].clone());
        let result = cleaner.validate_data(&data, schema(&case["schema"]).as_ref());
        check(validation_outcome(result), case);
    }
    for case in cases("cleaner_handle_missing_values") {
        let result =
            cleaner.handle_missing_values(&data, case["strategy"].as_str(), &obj(&case["options"]));
        check(json!({"ok": json_of(&result)}), case);
    }
}

#[test]
fn test_duplicate_detector_matches_oracle() {
    let detector = DuplicateDetector::new(kw(
        json!({"similarity_threshold": 0.6, "key_fields": ["name"]}),
    ));
    assert_eq!(detector.similarity_threshold, 0.6);
    assert_eq!(detector.key_fields, vec!["name"]);
    let data = dataset();
    for case in cases("detector_detect_duplicates") {
        let fields = key_fields(&case["key_fields"]);
        let result =
            detector.detect_duplicates(&data, case["threshold"].as_f64(), fields.as_deref());
        check(groups_outcome(result), case);
    }
    let plain = DuplicateDetector::default();
    for case in cases("calculate_similarity") {
        let fields = key_fields(&case["key_fields"]);
        let result =
            plain.calculate_similarity(&obj(&case["r1"]), &obj(&case["r2"]), fields.as_deref());
        check(json!({"ok": result}), case);
    }
    for case in cases("resolve_duplicates") {
        let groups: Vec<DuplicateGroup> = serde_json::from_value(case["groups"].clone()).unwrap();
        let result = plain.resolve_duplicates(&groups, case["strategy"].as_str());
        check(common::outcome(result.map(|r| json_of(&r))), case);
    }
    let empty = DuplicateGroup::new(Vec::new(), 0.0, None);
    assert_eq!(
        common::outcome(
            plain
                .resolve_duplicates(&[empty], Some("other"))
                .map(|r| json_of(&r))
        ),
        *oracle("resolve_duplicates_empty_records")
    );
}

#[test]
fn test_data_validator_matches_oracle() {
    let validator = DataValidator::default();
    for case in cases("validate_dataset") {
        let data = records(case["dataset"].clone());
        let result = validator.validate_dataset(&data, schema(&case["schema"]).as_ref());
        check(validation_outcome(result), case);
    }
    for case in cases("validate_record") {
        let result =
            validator.validate_record(&obj(&case["record"]), schema(&case["schema"]).as_ref());
        check(validation_outcome(result), case);
    }
    for case in cases("check_data_types") {
        let types = ExpectedTypes::from_value(&case["types"]).unwrap();
        check(
            json!({"ok": validator.check_data_types(&case["data"], &types)}),
            case,
        );
    }
}

#[test]
fn test_missing_value_handler_matches_oracle() {
    let handler = MissingValueHandler::default();
    for case in cases("identify_missing_values") {
        let data = records(case["dataset"].clone());
        check(json!({"ok": handler.identify_missing_values(&data)}), case);
    }
    let missing = records(json!([
        {"a": 1, "b": "x", "c": null},
        {"a": null, "b": "N/A", "c": 2.5},
        {"a": 3, "b": "", "c": true},
        {"a": 10, "b": "null", "d": "NULL"}
    ]));
    for case in cases("handle_missing_values") {
        let result = handler.handle_missing_values(
            &missing,
            case["strategy"].as_str(),
            &obj(&case["options"]),
        );
        check(json!({"ok": json_of(&result)}), case);
    }
    for case in cases("impute_values") {
        let data = records(case["dataset"].clone());
        let result = handler.impute_values(&data, case["method"].as_str());
        check(json!({"ok": json_of(&result)}), case);
    }
    let custom = MissingValueHandler::new(kw(json!({"missing_values": [0, "-"]})));
    let expected = oracle("custom_missing_values");
    let identify = custom.identify_missing_values(&records(json!([
        {"a": 0, "b": false}, {"a": "-", "b": null}, {"a": 1, "b": 2}
    ])));
    assert_eq!(json!({"ok": identify}), expected["identify"]);
    let removed = custom.handle_missing_values(
        &records(json!([{"a": 0}, {"a": false}, {"a": 1.0}, {"a": null}])),
        Some("remove"),
        &Map::new(),
    );
    assert_eq!(json!({"ok": json_of(&removed)}), expected["remove"]);
}
