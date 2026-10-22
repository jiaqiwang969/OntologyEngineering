//! Port of `tests/normalize/test_number_normalizer.py` (`TestNumberNormalizer`,
//! `TestUnitConverter`, `TestCurrencyNormalizer`, `TestScientificNotationHandler`).
#![allow(non_snake_case)]

use sem_normalize::{
    CurrencyNormalizer, NumberNormalizer, ScientificNotationHandler, UnitConverter,
};
use serde_json::json;

#[test]
fn test_number_normalizer__test_normalize_number_string() {
    let normalizer = NumberNormalizer::default();
    assert_eq!(normalizer.normalize_number("1,234.56").unwrap(), 1234.56);
}

#[test]
fn test_number_normalizer__test_normalize_quantity() {
    let normalizer = NumberNormalizer::default();
    let result = normalizer.normalize_quantity("5 kg").unwrap();
    assert_eq!(result["value"], json!(5.0));
    assert_eq!(result["unit"], json!("kilogram"));

    let result = normalizer.normalize_quantity("100 meters").unwrap();
    assert_eq!(result["value"], json!(100.0));
    assert_eq!(result["unit"], json!("meter"));
}

#[test]
fn test_unit_converter__test_convert() {
    let converter = UnitConverter::default();
    // 1 km = 1000 m
    assert_eq!(converter.convert_units(1.0, "km", "m").unwrap(), 1000.0);
    // 1 kg = 1000 g
    assert_eq!(converter.convert_units(1.0, "kg", "g").unwrap(), 1000.0);
}

#[test]
fn test_unit_converter__test_normalize_unit() {
    let converter = UnitConverter::default();
    assert_eq!(converter.normalize_unit("km"), "kilometer");
    assert_eq!(converter.normalize_unit("kgs"), "kilogram");
}

#[test]
fn test_currency_normalizer__test_parse_currency() {
    let normalizer = CurrencyNormalizer::default();
    let result = normalizer.normalize_currency("$1,234.56", None);
    assert_eq!(result["amount"], json!(1234.56));
    assert_eq!(result["currency"], json!("USD"));

    let result = normalizer.normalize_currency("100 EUR", None);
    assert_eq!(result["amount"], json!(100.0));
    assert_eq!(result["currency"], json!("EUR"));
}

#[test]
fn test_currency_normalizer__test_symbol_currencies_are_validated_as_supported_codes() {
    let normalizer = CurrencyNormalizer::default();
    for (symbol, expected_code) in &normalizer.currency_symbols {
        let result = normalizer.normalize_currency(&format!("{symbol}100"), None);
        assert_eq!(result["currency"], json!(expected_code));
        assert!(normalizer.validate_currency_code(result["currency"].as_str().unwrap()));
    }
}

#[test]
fn test_currency_normalizer__test_currency_codes_match_boundaries_without_matching_words() {
    let normalizer = CurrencyNormalizer::default();
    for value in ["RUB100", "100 RUB", "rub 100"] {
        let result = normalizer.normalize_currency(value, None);
        assert_eq!(result["amount"], json!(100.0), "{value}");
        assert_eq!(result["currency"], json!("RUB"), "{value}");
    }
    for value in ["ruby 100", "wilson 100"] {
        let result = normalizer.normalize_currency(value, None);
        assert_eq!(result["amount"], json!(100.0), "{value}");
        assert_eq!(result["currency"], json!("USD"), "{value}");
    }
}

#[test]
fn test_scientific_notation_handler__test_parse_scientific() {
    let handler = ScientificNotationHandler::default();
    assert_eq!(
        handler.parse_scientific_notation("1.23e4").unwrap(),
        12300.0
    );
    assert_eq!(
        handler.parse_scientific_notation("1.23E-2").unwrap(),
        0.0123
    );
}
