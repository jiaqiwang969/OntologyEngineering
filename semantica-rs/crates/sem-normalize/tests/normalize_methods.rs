//! Characterisation of the convenience functions of `methods` against the oracle.

mod common;

use chrono::{NaiveDate, NaiveDateTime};
use sem_normalize::date_normalizer::{DateInput, patch_now, reset_tzpath, tzpath};
use sem_normalize::methods::{EncodingResult, LanguageResult};
use sem_normalize::number_normalizer::{NumberInput, NumberValue};
use sem_normalize::{
    clean_data, clean_text, detect_duplicates, detect_language, disambiguate_entity,
    get_normalize_method, handle_encoding, list_available_methods, normalize_date,
    normalize_entity, normalize_number, normalize_quantity, normalize_text, normalize_time,
    resolve_aliases,
};
use sem_utils::SemanticaError;
use serde_json::{Value, json};

use common::{check, data_arg, data_json, err_json, float_json, obj, oracle, records, text_opt};

fn cases(key: &str) -> &'static Vec<Value> {
    oracle("methods")[key].as_array().expect("a list of cases")
}

fn dataset() -> Vec<sem_normalize::data_cleaner::Record> {
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

fn fixed_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2024, 1, 2)
        .unwrap()
        .and_hms_opt(0, 30, 0)
        .unwrap()
}

#[test]
fn test_text_functions_match_oracle() {
    for case in cases("normalize_text") {
        let result = normalize_text(text_opt(&case["text"]), None, &obj(&case["kwargs"]));
        check(common::outcome(result), case);
    }
    for case in cases("clean_text") {
        let result = clean_text(text_opt(&case["text"]), None, &obj(&case["kwargs"]));
        check(common::outcome(result), case);
    }
}

#[test]
fn test_entity_functions_match_oracle() {
    for case in cases("normalize_entity") {
        let result = normalize_entity(
            case["name"].as_str(),
            case["entity_type"].as_str(),
            None,
            &obj(&case["kwargs"]),
        );
        check(common::outcome(result), case);
    }
    for case in cases("resolve_aliases") {
        let result = resolve_aliases(
            case["name"].as_str().unwrap(),
            None,
            None,
            &obj(&case["kwargs"]),
        );
        check(common::outcome(result.map(|r| json!(r))), case);
    }
    for case in cases("disambiguate_entity") {
        let result =
            disambiguate_entity(case["name"].as_str().unwrap(), None, &obj(&case["context"]));
        check(common::outcome(result), case);
    }
}

#[test]
fn test_date_functions_match_oracle() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zoneinfo");
    if tzpath() != vec![dir.clone()] {
        reset_tzpath(Some(&[dir]));
    }
    let _now = patch_now(fixed_now());
    for case in cases("normalize_date") {
        let result = normalize_date(
            DateInput::from(&case["input"]),
            case["format"].as_str(),
            case["timezone"].as_str(),
            None,
            &serde_json::Map::new(),
        );
        check(common::outcome(result), case);
    }
    for case in cases("normalize_time") {
        let result = normalize_time(
            DateInput::from(&case["input"]),
            None,
            &serde_json::Map::new(),
        );
        check(common::outcome(result), case);
    }
}

fn number_outcome(result: Result<NumberValue, SemanticaError>) -> Value {
    match result {
        Ok(NumberValue::Int(i)) => json!({"ok": i64::try_from(i).unwrap()}),
        Ok(NumberValue::Float(f)) => json!({"ok": float_json(f)}),
        Err(e) => err_json(&e),
    }
}

#[test]
fn test_number_functions_match_oracle() {
    for case in cases("normalize_number") {
        let result = normalize_number(
            NumberInput::from(&case["input"]),
            None,
            &serde_json::Map::new(),
        );
        check(number_outcome(result), case);
    }
    for case in cases("normalize_quantity") {
        let result = normalize_quantity(
            case["input"].as_str().unwrap(),
            None,
            &serde_json::Map::new(),
        );
        check(common::outcome(result), case);
    }
}

#[test]
fn test_data_functions_match_oracle() {
    let data = dataset();
    for case in cases("clean_data") {
        let flags: Vec<bool> = case["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_bool().unwrap())
            .collect();
        let result = clean_data(
            &data,
            flags[0],
            flags[1],
            flags[2],
            None,
            &obj(&case["kwargs"]),
        );
        check(common::outcome(result.map(|r| json_of(&r))), case);
    }
    for case in cases("detect_duplicates") {
        let key_fields: Option<Vec<String>> = case["key_fields"]
            .as_array()
            .map(|k| k.iter().map(|f| f.as_str().unwrap().to_string()).collect());
        let result = detect_duplicates(
            &data,
            case["threshold"].as_f64(),
            key_fields.as_deref(),
            None,
            &serde_json::Map::new(),
        );
        check(common::outcome(result.map(|r| json_of(&r))), case);
    }
}

#[test]
fn test_language_and_encoding_functions_match_oracle() {
    for case in cases("detect_language") {
        let result = detect_language(
            case["text"].as_str().unwrap(),
            case["method"].as_str(),
            &serde_json::Map::new(),
        );
        let actual = match result {
            Ok(LanguageResult::Code(code)) => json!({"ok": code}),
            Ok(LanguageResult::WithConfidence(code, conf)) => json!({"ok": [code, conf]}),
            Err(e) => err_json(&e),
        };
        check(actual, case);
    }
    for case in cases("handle_encoding") {
        let result = handle_encoding(
            data_arg(&case["data"]),
            case["operation"].as_str(),
            None,
            &obj(&case["kwargs"]),
        );
        let actual = match result {
            Ok(EncodingResult::Detection(encoding, confidence)) => {
                json!({"ok": [encoding, confidence]})
            }
            Ok(EncodingResult::Text(text)) => json!({"ok": text}),
            Ok(EncodingResult::Data(data)) => json!({"ok": data_json(&data)}),
            Err(e) => err_json(&e),
        };
        check(actual, case);
    }
}

#[test]
fn test_registry_functions_match_oracle() {
    let expected = oracle("methods");
    assert_eq!(
        json!({"ok": get_normalize_method("text", "nope").map(|_| "method")}),
        expected["get_normalize_method"]
    );
    assert_eq!(
        json!({"ok": list_available_methods(None)}),
        expected["list_available_methods"]
    );
    assert_eq!(
        json!({"ok": list_available_methods(Some("text"))}),
        expected["list_available_methods_text"]
    );
}
