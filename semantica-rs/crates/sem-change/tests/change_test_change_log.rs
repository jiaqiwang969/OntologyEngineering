//! Port of `tests/change_management/test_change_log.py` (`TestChangeLogEntry`).
#![allow(non_snake_case)]

mod common;

use chrono::NaiveDateTime;
use common::{assert_raises, is_validation};
use sem_change::ChangeLogEntry;

/// `datetime.fromisoformat(timestamp)` for the naive `isoformat()` text `create_now` writes.
fn parse_naive(timestamp: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S"))
        .expect("isoformat timestamp")
}

#[test]
fn test_change_log_entry__test_valid_change_log_entry() {
    let entry = ChangeLogEntry::new(
        "2024-01-15T10:30:00Z",
        "alice@company.com",
        "Added Customer entity",
        None,
        None,
    )
    .unwrap();

    assert_eq!(entry.timestamp, "2024-01-15T10:30:00Z");
    assert_eq!(entry.author, "alice@company.com");
    assert_eq!(entry.description, "Added Customer entity");
    assert_eq!(entry.change_id, None);
    assert_eq!(entry.related_changes, Vec::<String>::new());
}

#[test]
fn test_change_log_entry__test_change_log_entry_with_optional_fields() {
    let entry = ChangeLogEntry::new(
        "2024-01-15T10:30:00Z",
        "bob@company.com",
        "Modified Product entity",
        Some("CHG-001".into()),
        Some(vec!["CHG-000".into()]),
    )
    .unwrap();

    assert_eq!(entry.change_id.as_deref(), Some("CHG-001"));
    assert_eq!(entry.related_changes, vec!["CHG-000"]);
}

#[test]
fn test_change_log_entry__test_invalid_timestamp_format() {
    assert_raises(
        ChangeLogEntry::new(
            "2024-01-15 10:30:00", // Wrong format
            "alice@company.com",
            "Test change",
            None,
            None,
        ),
        is_validation,
        "Invalid timestamp format",
    );
}

#[test]
fn test_change_log_entry__test_invalid_email_format() {
    assert_raises(
        ChangeLogEntry::new(
            "2024-01-15T10:30:00Z",
            "invalid-email", // Invalid email
            "Test change",
            None,
            None,
        ),
        is_validation,
        "Invalid email format",
    );
}

#[test]
fn test_change_log_entry__test_empty_description() {
    assert_raises(
        ChangeLogEntry::new(
            "2024-01-15T10:30:00Z",
            "alice@company.com",
            "   ", // Empty/whitespace only
            None,
            None,
        ),
        is_validation,
        "Description cannot be empty",
    );
}

#[test]
fn test_change_log_entry__test_description_too_long() {
    let long_description = "x".repeat(501);
    assert_raises(
        ChangeLogEntry::new(
            "2024-01-15T10:30:00Z",
            "alice@company.com",
            long_description,
            None,
            None,
        ),
        is_validation,
        "Description too long",
    );
}

#[test]
fn test_change_log_entry__test_description_exactly_500_chars() {
    let description_500 = "x".repeat(500);
    let entry = ChangeLogEntry::new(
        "2024-01-15T10:30:00Z",
        "alice@company.com",
        description_500,
        None,
        None,
    )
    .unwrap();
    assert_eq!(entry.description.chars().count(), 500);
}

#[test]
fn test_change_log_entry__test_create_now_class_method() {
    let entry = ChangeLogEntry::create_now(
        "charlie@company.com",
        "Test change with current timestamp",
        None,
        None,
    )
    .unwrap();

    // Verify timestamp is recent (within last minute)
    let entry_time = parse_naive(&entry.timestamp);
    let now = chrono::Local::now().naive_local();
    let time_diff = (now - entry_time).num_microseconds().unwrap().abs() as f64 / 1e6;
    assert!(time_diff < 60.0); // Within 1 minute

    assert_eq!(entry.author, "charlie@company.com");
    assert_eq!(entry.description, "Test change with current timestamp");
}

#[test]
fn test_change_log_entry__test_create_now_with_optional_fields() {
    let entry = ChangeLogEntry::create_now(
        "dave@company.com",
        "Test change",
        Some("CHG-002".into()),
        Some(vec!["CHG-001".into(), "CHG-000".into()]),
    )
    .unwrap();

    assert_eq!(entry.change_id.as_deref(), Some("CHG-002"));
    assert_eq!(entry.related_changes, vec!["CHG-001", "CHG-000"]);
}

#[test]
fn test_change_log_entry__test_various_valid_email_formats() {
    let valid_emails = [
        "user@domain.com",
        "user.name@domain.co.uk",
        "user+tag@domain.org",
        "user123@domain123.net",
        "user_name@sub.domain.com",
    ];

    for email in valid_emails {
        let entry =
            ChangeLogEntry::new("2024-01-15T10:30:00Z", email, "Test change", None, None).unwrap();
        assert_eq!(entry.author, email);
    }
}

#[test]
fn test_change_log_entry__test_various_invalid_email_formats() {
    let invalid_emails = [
        "plainaddress",
        "@missingdomain.com",
        "missing@.com",
        "missing@domain",
        "spaces @domain.com",
        "double@@domain.com",
    ];

    for email in invalid_emails {
        assert_raises(
            ChangeLogEntry::new("2024-01-15T10:30:00Z", email, "Test change", None, None),
            is_validation,
            "Invalid email format",
        );
    }
}

#[test]
fn test_change_log_entry__test_various_valid_timestamp_formats() {
    let valid_timestamps = [
        "2024-01-15T10:30:00Z",
        "2024-01-15T10:30:00+00:00",
        "2024-01-15T10:30:00.123Z",
        "2024-01-15T10:30:00.123456Z",
        "2024-01-15T10:30:00+05:30",
        "2024-01-15T10:30:00-08:00",
    ];

    for timestamp in valid_timestamps {
        let entry =
            ChangeLogEntry::new(timestamp, "test@company.com", "Test change", None, None).unwrap();
        assert_eq!(entry.timestamp, timestamp);
    }
}
