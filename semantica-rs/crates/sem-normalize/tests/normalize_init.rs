//! The `semantica.normalize` export surface (`__init__.py`'s `__all__`): every name is
//! reachable from the crate root and gives the oracle's answer (`init_exports`).

mod common;

use chrono::NaiveDate;
use sem_normalize::methods::{EncodingResult, LanguageResult};
use sem_normalize::number_normalizer::NumberValue;
use serde_json::{Map, Value, json};

use common::{obj, oracle, records, text_opt};

fn expected(key: &str) -> &'static Value {
    let value = &oracle("init_exports")[key];
    assert!(!value.is_null(), "no oracle value for {key}");
    value
}

fn ok(value: impl Into<Value>) -> Value {
    json!({"ok": value.into()})
}

fn number(n: NumberValue) -> Value {
    match n {
        NumberValue::Int(i) => json!(i64::try_from(i).unwrap()),
        NumberValue::Float(f) => json!(f),
    }
}

fn first(key: &str) -> &'static Value {
    &oracle(key)[0]
}

#[test]
fn test_text_exports() {
    let case = first("normalize_text");
    let normalizer = sem_normalize::TextNormalizer::default();
    let options =
        sem_normalize::text_normalizer::NormalizeTextOptions::from_kwargs(&obj(&case["kwargs"]));
    assert_eq!(
        ok(normalizer
            .normalize_text(text_opt(&case["text"]), &options)
            .unwrap()),
        case["result"]
    );
    let case = first("normalize_unicode");
    let unicode = sem_normalize::UnicodeNormalizer::default();
    assert_eq!(
        ok(unicode.normalize_unicode(text_opt(&case["text"]), case["form"].as_str())),
        case["result"]
    );
    let case = first("normalize_whitespace");
    let whitespace = sem_normalize::WhitespaceNormalizer::default();
    assert_eq!(
        ok(whitespace
            .normalize_whitespace(case["text"].as_str(), case["line_break_type"].as_str())),
        case["result"]
    );
    let case = first("normalize_punctuation");
    let special = sem_normalize::SpecialCharacterProcessor::default();
    assert_eq!(
        ok(special.normalize_punctuation(case["text"].as_str().unwrap())),
        case["result"]
    );
    let case = first("clean");
    let options = sem_normalize::text_cleaner::CleanOptions::from_kwargs(&obj(&case["kwargs"]));
    let cleaner = sem_normalize::TextCleaner::default();
    assert_eq!(
        ok(cleaner.clean(text_opt(&case["text"]), &options)),
        case["result"]
    );
}

#[test]
fn test_entity_exports() {
    let aliases = json!({"alias_map": {"j. doe": "John Doe"}});
    let normalizer = sem_normalize::EntityNormalizer::new(obj(&aliases));
    assert_eq!(
        ok(normalizer.normalize_entity("  john   doe  ", Some("Person"), true)),
        *expected("entity_normalizer")
    );
    let resolver = sem_normalize::AliasResolver::new(obj(&aliases));
    assert_eq!(
        ok(resolver.resolve_aliases("J. Doe")),
        *expected("alias_resolver")
    );
    let case = first("classify_entity_type");
    let disambiguator = sem_normalize::EntityDisambiguator::default();
    assert_eq!(
        ok(disambiguator.classify_entity_type(case["name"].as_str().unwrap())),
        case["result"]
    );
    let case = first("normalize_name_format");
    let handler = sem_normalize::NameVariantHandler::default();
    assert_eq!(
        ok(handler
            .normalize_name_format(case["name"].as_str().unwrap(), case["format_type"].as_str())),
        case["result"]
    );
}

#[test]
fn test_date_exports() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zoneinfo");
    if sem_normalize::date_normalizer::tzpath() != vec![dir.clone()] {
        sem_normalize::date_normalizer::reset_tzpath(Some(&[dir]));
    }
    let _now = sem_normalize::date_normalizer::patch_now(
        NaiveDate::from_ymd_opt(2024, 1, 2)
            .unwrap()
            .and_hms_opt(0, 30, 0)
            .unwrap(),
    );
    let case = first("normalize_date");
    let normalizer = sem_normalize::DateNormalizer::default();
    assert_eq!(
        ok(normalizer
            .normalize_date(
                case["input"].as_str().unwrap(),
                case["format"].as_str(),
                case["timezone"].as_str()
            )
            .unwrap()),
        case["result"]
    );
    let noon =
        sem_normalize::date_normalizer::DateTime::from_ymd_hms(2023, 1, 1, 12, 0, 0).unwrap();
    let utc = sem_normalize::TimeZoneNormalizer::default()
        .convert_to_utc(&noon, None)
        .unwrap();
    assert_eq!(
        ok(json!({"datetime": utc.isoformat().unwrap(), "tzname": utc.tzname()})),
        *expected("convert_to_utc")
    );
    let yesterday = sem_normalize::RelativeDateProcessor::default()
        .process_relative_expression("yesterday", None)
        .unwrap();
    assert_eq!(
        ok(json!({"datetime": yesterday.isoformat().unwrap(), "tzname": yesterday.tzname()})),
        *expected("relative_yesterday")
    );
    let case = first("extract_date_components");
    let parser = sem_normalize::TemporalExpressionParser::default();
    assert_eq!(
        ok(parser.extract_date_components(case["text"].as_str().unwrap())),
        case["result"]
    );
    assert_eq!(
        ok(sem_normalize::normalize_date("2023-01-01", None, None, None, &Map::new()).unwrap()),
        *expected("normalize_date")
    );
    assert_eq!(
        ok(sem_normalize::normalize_time("10:30 PM", None, &Map::new()).unwrap()),
        *expected("normalize_time")
    );
}

#[test]
fn test_number_exports() {
    let normalizer = sem_normalize::NumberNormalizer::default();
    assert_eq!(
        ok(number(normalizer.normalize_number("1,234.56").unwrap())),
        *expected("normalize_number")
    );
    assert_eq!(
        ok(sem_normalize::UnitConverter::default().normalize_unit("km")),
        *expected("normalize_unit")
    );
    assert_eq!(
        ok(sem_normalize::CurrencyNormalizer::default().validate_currency_code("usd")),
        *expected("validate_currency_code")
    );
    assert_eq!(
        ok(sem_normalize::ScientificNotationHandler::default().convert_to_scientific(1500.0, None)),
        *expected("convert_to_scientific")
    );
    let empty = Map::new();
    assert_eq!(
        ok(number(
            sem_normalize::normalize_number("2.5k", None, &empty).unwrap()
        )),
        *expected("normalize_number_fn")
    );
    assert_eq!(
        ok(sem_normalize::normalize_quantity("1 km", None, &empty).unwrap()),
        *expected("normalize_quantity")
    );
}

#[test]
fn test_data_cleaning_exports() {
    let data = records(json!([{"a": 1}, {"a": 1}, {"a": null}]));
    let empty = Map::new();
    let cleaned = sem_normalize::DataCleaner::default()
        .clean_data(&data, true, true, true, &empty)
        .unwrap();
    assert_eq!(ok(json!(cleaned)), *expected("clean_data"));
    let groups: Vec<sem_normalize::DuplicateGroup> =
        sem_normalize::DuplicateDetector::default().detect_duplicates(&data, None, None);
    assert_eq!(ok(groups.len()), *expected("detect_duplicates_count"));
    let result: sem_normalize::ValidationResult = sem_normalize::DataValidator::default()
        .validate_dataset(&data, None)
        .unwrap();
    assert_eq!(ok(result.valid), *expected("validate_dataset_valid"));
    let filled = sem_normalize::MissingValueHandler::default().handle_missing_values(
        &data,
        Some("fill"),
        &empty,
    );
    assert_eq!(ok(json!(filled)), *expected("fill"));
    let pair = records(json!([{"a": 1}, {"a": 1}]));
    assert_eq!(
        ok(
            sem_normalize::clean_data(&pair, true, true, true, None, &empty)
                .unwrap()
                .len()
        ),
        *expected("clean_data_fn")
    );
    assert_eq!(
        ok(
            sem_normalize::detect_duplicates(&pair, None, None, None, &empty)
                .unwrap()
                .len()
        ),
        *expected("detect_duplicates_fn")
    );
}

#[test]
fn test_language_and_encoding_exports() {
    let detector = sem_normalize::LanguageDetector::default();
    assert_eq!(ok(detector.detect("Hi")), *expected("detect_language_hi"));
    assert_eq!(
        ok(detector.get_language_name("fr")),
        *expected("language_name_fr")
    );
    let (encoding, confidence) = sem_normalize::EncodingHandler::default().detect(b"Hello");
    assert_eq!(ok(json!([encoding, confidence])), *expected("detect_hello"));
    let empty = Map::new();
    let LanguageResult::Code(code) = sem_normalize::detect_language("Hi", None, &empty).unwrap()
    else {
        panic!("a language code");
    };
    assert_eq!(ok(code), *expected("detect_language_fn"));
    let EncodingResult::Detection(encoding, confidence) =
        sem_normalize::handle_encoding(b"Hello", None, None, &empty).unwrap()
    else {
        panic!("a detection");
    };
    assert_eq!(
        ok(json!([encoding, confidence])),
        *expected("handle_encoding_fn")
    );
}

#[test]
fn test_function_exports() {
    let empty = Map::new();
    assert_eq!(
        ok(sem_normalize::normalize_text("Hello   World", None, &empty).unwrap()),
        *expected("normalize_text")
    );
    assert_eq!(
        ok(sem_normalize::clean_text("<b>x</b>", None, &empty).unwrap()),
        *expected("clean_text")
    );
    assert_eq!(
        ok(
            sem_normalize::normalize_entity("  john   doe  ", Some("Person"), None, &empty)
                .unwrap()
        ),
        *expected("normalize_entity")
    );
    assert_eq!(
        ok(sem_normalize::resolve_aliases("x", None, None, &empty).unwrap()),
        *expected("resolve_aliases")
    );
    assert_eq!(
        ok(sem_normalize::disambiguate_entity("Apple", None, &empty).unwrap()),
        *expected("disambiguate_entity")
    );
    assert_eq!(
        json!(sem_normalize::get_normalize_method("text", "nope").is_none()),
        *expected("get_normalize_method_is_none")
    );
    assert_eq!(
        ok(json!(sem_normalize::list_available_methods(Some("date")))),
        *expected("list_available_methods_date")
    );
}

#[test]
fn test_registry_and_config_exports() {
    let registry: &sem_normalize::MethodRegistry = &sem_normalize::method_registry;
    assert_eq!(
        ok(json!(registry.list_all(Some("language")))),
        *expected("registry_language")
    );
    let config = sem_normalize::NormalizeConfig::new(None);
    config.set("k", "v");
    assert_eq!(
        ok(config.get("k", Value::Null)),
        *expected("config_set_get")
    );
    assert_eq!(
        ok(sem_normalize::normalize_config.get_method_config("never-configured")),
        *expected("global_method_config")
    );
}
