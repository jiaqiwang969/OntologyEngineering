//! Characterisation of `date_normalizer` (`DateNormalizer`, `TimeZoneNormalizer`,
//! `RelativeDateProcessor`, `TemporalExpressionParser`) against the oracle.
//!
//! As in the oracle run, `datetime.now()` is fixed at 2024-01-02 00:30 and time zones come
//! from `tests/fixtures/zoneinfo`.

mod common;

use std::path::PathBuf;

use chrono::{NaiveDate, NaiveDateTime};
use sem_normalize::date_normalizer::{
    DateInput, DateTime, HAS_DATEUTIL, NowGuard, TzInfo, patch_now, reset_tzpath, tzpath,
};
use sem_normalize::{
    DateNormalizer, RelativeDateProcessor, TemporalExpressionParser, TimeZoneNormalizer,
};
use sem_utils::SemanticaError;
use serde_json::{Value, json};

use common::{check, err_json, oracle};

// The dateutil parser port is always available.
const _: () = assert!(HAS_DATEUTIL);

fn cases(key: &str) -> &'static Vec<Value> {
    oracle(key).as_array().expect("a list of cases")
}

fn fixed_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2024, 1, 2)
        .unwrap()
        .and_hms_opt(0, 30, 0)
        .unwrap()
}

/// The oracle's environment: fixed clock and the fixture time zone database.
fn setup() -> NowGuard {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zoneinfo");
    if tzpath() != vec![dir.clone()] {
        reset_tzpath(Some(&[dir]));
    }
    patch_now(fixed_now())
}

/// `{"dt": iso, "offset": seconds | null}` as a datetime (offsets are
/// `datetime.timezone` offsets).
fn dt_input(spec: &Value) -> DateTime {
    let wall = NaiveDateTime::parse_from_str(spec["dt"].as_str().unwrap(), "%Y-%m-%dT%H:%M:%S%.f")
        .expect("ISO datetime");
    match spec["offset"].as_i64() {
        None => DateTime::naive(wall),
        Some(offset) => DateTime::aware(wall, TzInfo::Fixed(offset as i32)),
    }
}

fn date_arg(value: &Value) -> DateInput {
    match value {
        Value::Object(_) => DateInput::DateTime(dt_input(value)),
        other => DateInput::from(other),
    }
}

/// The generator's JSON form of a datetime result.
fn dt_json(dt: &DateTime) -> Value {
    match dt.isoformat() {
        Ok(iso) => json!({"datetime": iso, "tzname": dt.tzname()}),
        Err(e) => json!({"datetime": err_json(&e), "tzname": null}),
    }
}

fn dt_outcome(result: Result<DateTime, SemanticaError>) -> Value {
    match result {
        Ok(dt) => json!({"ok": dt_json(&dt)}),
        Err(e) => err_json(&e),
    }
}

#[test]
fn test_normalize_date_matches_oracle() {
    let _now = setup();
    let normalizer = DateNormalizer::default();
    for case in cases("normalize_date") {
        let result = normalizer.normalize_date(
            date_arg(&case["input"]),
            case["format"].as_str(),
            case["timezone"].as_str(),
        );
        check(common::outcome(result), case);
    }
}

#[test]
fn test_normalize_time_matches_oracle() {
    let _now = setup();
    let normalizer = DateNormalizer::default();
    for case in cases("normalize_time") {
        let result = normalizer.normalize_time(date_arg(&case["input"]));
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_relative_dates_match_oracle() {
    let _now = setup();
    let normalizer = DateNormalizer::default();
    for case in cases("process_relative_date") {
        let reference = (!case["reference"].is_null()).then(|| dt_input(&case["reference"]));
        let result = normalizer
            .process_relative_date(case["expression"].as_str().unwrap(), reference.as_ref());
        check(dt_outcome(result), case);
    }
    let processor = RelativeDateProcessor::default();
    assert_eq!(
        processor.relative_terms.keys().collect::<Vec<_>>(),
        vec!["today", "yesterday", "tomorrow", "now"]
    );
    for case in cases("calculate_date_offset") {
        let result = processor.calculate_date_offset(
            case["expression"].as_str().unwrap(),
            &dt_input(&case["reference"]),
        );
        check(dt_outcome(result), case);
    }
    for case in cases("handle_relative_terms") {
        let result = processor.handle_relative_terms(
            case["expression"].as_str().unwrap(),
            &dt_input(&case["reference"]),
        );
        check(dt_outcome(result), case);
    }
}

#[test]
fn test_temporal_expressions_match_oracle() {
    let _now = setup();
    let normalizer = DateNormalizer::default();
    for case in cases("parse_temporal_expression") {
        let result = normalizer.parse_temporal_expression(case["text"].as_str().unwrap());
        check(json!({"ok": result}), case);
    }
    let parser = TemporalExpressionParser::default();
    for case in cases("extract_date_components") {
        check(
            json!({"ok": parser.extract_date_components(case["text"].as_str().unwrap())}),
            case,
        );
    }
    for case in cases("extract_time_components") {
        check(
            json!({"ok": parser.extract_time_components(case["text"].as_str().unwrap())}),
            case,
        );
    }
    for case in cases("process_temporal_ranges") {
        check(
            json!({"ok": parser.process_temporal_ranges(case["text"].as_str().unwrap())}),
            case,
        );
    }
}

#[test]
fn test_time_zone_normalizer_matches_oracle() {
    let _now = setup();
    let normalizer = TimeZoneNormalizer::default();
    for case in cases("normalize_timezone") {
        let result =
            normalizer.normalize_timezone(&dt_input(&case["input"]), case["timezone"].as_str());
        check(json!({"ok": dt_json(&result)}), case);
    }
    for case in cases("convert_to_utc") {
        let result = normalizer.convert_to_utc(&dt_input(&case["input"]), case["source"].as_str());
        check(dt_outcome(result), case);
    }
    for case in cases("handle_dst_transitions") {
        let result = normalizer.handle_dst_transitions(
            &dt_input(&case["input"]),
            case["timezone"].as_str().unwrap(),
        );
        check(json!({"ok": dt_json(&result)}), case);
    }
}
