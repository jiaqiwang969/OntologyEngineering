//! Port of `tests/normalize/test_date_normalizer.py` (`TestDateNormalizer`,
//! `TestTimeZoneNormalizer`, `TestRelativeDateProcessor`).
#![allow(non_snake_case)]

use chrono::{Local, NaiveDate};
use sem_normalize::date_normalizer::{DateTime, patch_now};
use sem_normalize::{DateNormalizer, RelativeDateProcessor, TimeZoneNormalizer};

#[test]
fn test_date_normalizer__test_normalize_date_iso() {
    let normalizer = DateNormalizer::default();
    assert_eq!(
        normalizer
            .normalize_date("2023-01-01", Some("date"), None)
            .unwrap(),
        "2023-01-01"
    );
    assert_eq!(
        normalizer
            .normalize_date("2023-01-01T12:00:00", Some("ISO8601"), None)
            .unwrap(),
        "2023-01-01T12:00:00+00:00"
    );
}

#[test]
fn test_date_normalizer__test_normalize_date_relative() {
    let normalizer = DateNormalizer::default();
    // patch("semantica.normalize.date_normalizer.datetime", FixedDateTime) with
    // FixedDateTime.now() == datetime(2024, 1, 2, 0, 30).
    let _now = patch_now(
        NaiveDate::from_ymd_opt(2024, 1, 2)
            .unwrap()
            .and_hms_opt(0, 30, 0)
            .unwrap(),
    );
    assert_eq!(
        normalizer
            .normalize_date("today", Some("date"), None)
            .unwrap(),
        "2024-01-02"
    );
}

#[test]
fn test_date_normalizer__test_normalize_timezone() {
    let normalizer = DateNormalizer::default();
    // "2023-01-01T12:00:00+01:00" -> UTC is "2023-01-01T11:00:00+00:00"
    let normalized = normalizer
        .normalize_date("2023-01-01T12:00:00+01:00", None, Some("UTC"))
        .unwrap();
    assert_eq!(normalized, "2023-01-01T11:00:00+00:00");
}

#[test]
fn test_date_normalizer__test_parse_temporal_expression() {
    let normalizer = DateNormalizer::default();
    let result = normalizer.parse_temporal_expression("from 2023-01-01 to 2023-01-31");
    assert!(result.get("range").is_some_and(|r| !r.is_null()));
}

#[test]
fn test_time_zone_normalizer__test_normalize_timezone_obj() {
    let tz_normalizer = TimeZoneNormalizer::default();
    let dt = DateTime::from_ymd_hms(2023, 1, 1, 12, 0, 0).unwrap();
    let normalized = tz_normalizer.normalize_timezone(&dt, Some("UTC"));
    // The offset equals timezone.utc's.
    assert_eq!(normalized.utcoffset().unwrap(), Some(0));
}

#[test]
fn test_relative_date_processor__test_process_relative_expression() {
    let processor = RelativeDateProcessor::default();
    let dt = processor
        .process_relative_expression("3 days ago", None)
        .unwrap();
    // The result is a naive datetime; compare with the local naive now.
    assert!(dt.tzinfo.is_none());
    let diff = Local::now().naive_local() - dt.wall;
    assert!(
        chrono::Duration::days(2) + chrono::Duration::hours(23) < diff
            && diff < chrono::Duration::days(3) + chrono::Duration::hours(1),
        "{diff:?}"
    );
}
