//! Characterisation of `number_normalizer` (`NumberNormalizer`, `UnitConverter`,
//! `CurrencyNormalizer`, `ScientificNotationHandler`) against the oracle.

mod common;

use sem_normalize::number_normalizer::{NumberInput, NumberValue};
use sem_normalize::{
    CurrencyNormalizer, NumberNormalizer, ScientificNotationHandler, UnitConverter,
};
use sem_utils::SemanticaError;
use serde_json::{Value, json};

use common::{check, err_json, float_json, oracle};

fn cases(key: &str) -> &'static Vec<Value> {
    oracle(key).as_array().expect("a list of cases")
}

fn number_json(n: NumberValue) -> Value {
    match n {
        NumberValue::Int(i) => json!(i64::try_from(i).expect("fits i64")),
        NumberValue::Float(f) => float_json(f),
    }
}

fn number_outcome(result: Result<NumberValue, SemanticaError>) -> Value {
    match result {
        Ok(n) => json!({"ok": number_json(n)}),
        Err(e) => err_json(&e),
    }
}

fn float_outcome(result: Result<f64, SemanticaError>) -> Value {
    match result {
        Ok(f) => json!({"ok": float_json(f)}),
        Err(e) => err_json(&e),
    }
}

fn float_arg(value: &Value) -> f64 {
    match value {
        Value::Object(tag) => match tag["float"].as_str().unwrap() {
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            _ => f64::NAN,
        },
        other => other.as_f64().unwrap(),
    }
}

#[test]
fn test_normalize_number_matches_oracle() {
    let normalizer = NumberNormalizer::default();
    for case in cases("normalize_number") {
        let result = normalizer.normalize_number(NumberInput::from(&case["input"]));
        check(number_outcome(result), case);
    }
}

#[test]
fn test_quantities_units_and_currency_match_oracle() {
    let normalizer = NumberNormalizer::default();
    for case in cases("normalize_quantity") {
        let result = normalizer.normalize_quantity(case["input"].as_str().unwrap());
        check(common::outcome(result), case);
    }
    for case in cases("convert_units") {
        let args = case["args"].as_array().unwrap();
        let result = normalizer.convert_units(
            args[0].as_f64().unwrap(),
            args[1].as_str().unwrap(),
            args[2].as_str().unwrap(),
        );
        check(float_outcome(result), case);
    }
    for case in cases("process_currency") {
        let result =
            normalizer.process_currency(case["input"].as_str().unwrap(), case["default"].as_str());
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_unit_converter_matches_oracle() {
    let converter = UnitConverter::default();
    for case in cases("validate_units") {
        let args = case["args"].as_array().unwrap();
        let result = converter.validate_units(args[0].as_str().unwrap(), args[1].as_str().unwrap());
        check(json!({"ok": result}), case);
    }
    for case in cases("get_conversion_factor") {
        let args = case["args"].as_array().unwrap();
        let result =
            converter.get_conversion_factor(args[0].as_str().unwrap(), args[1].as_str().unwrap());
        check(json!({"ok": float_json(result)}), case);
    }
    for case in cases("normalize_unit") {
        check(
            json!({"ok": converter.normalize_unit(case["unit"].as_str().unwrap())}),
            case,
        );
    }
    assert_eq!(converter.conversion_factors.len(), 45);
    assert_eq!(
        converter.unit_categories.keys().collect::<Vec<_>>(),
        vec!["length", "weight", "volume"]
    );
}

#[test]
fn test_currency_normalizer_matches_oracle() {
    let normalizer = CurrencyNormalizer::default();
    for case in cases("normalize_currency") {
        check(
            json!({"ok": normalizer.normalize_currency(case["input"].as_str().unwrap(), None)}),
            case,
        );
    }
    assert_eq!(
        json!({"ok": float_json(normalizer.convert_currency(12.5, "USD", "EUR"))}),
        *oracle("convert_currency")
    );
    for case in cases("validate_currency_code") {
        check(
            json!({"ok": normalizer.validate_currency_code(case["code"].as_str().unwrap())}),
            case,
        );
    }
}

#[test]
fn test_scientific_notation_handler_matches_oracle() {
    let handler = ScientificNotationHandler::default();
    for case in cases("parse_scientific_notation") {
        check(
            float_outcome(handler.parse_scientific_notation(case["input"].as_str().unwrap())),
            case,
        );
    }
    for case in cases("convert_to_scientific") {
        let result = handler.convert_to_scientific(
            float_arg(&case["number"]),
            case["precision"].as_u64().map(|p| p as usize),
        );
        check(json!({"ok": result}), case);
    }
    for case in cases("normalize_precision") {
        let result = handler
            .normalize_precision(float_arg(&case["number"]), case["digits"].as_i64().unwrap());
        check(json!({"ok": float_json(result)}), case);
    }
}
