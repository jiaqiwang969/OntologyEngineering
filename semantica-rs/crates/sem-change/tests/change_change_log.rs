//! Characterisation tests for `change_log.rs` (`semantica/change_management/change_log.py`):
//! expected values come from the oracle (`tests/fixtures/oracle.json`).

mod common;

use common::{mask, obj, oracle, outcome};
use sem_change::ChangeLogEntry;
use sem_change::change_log::{
    ChangeCategory, ChangeLogAnalyzer, ImpactReport, MutationRecord, Severity,
    generate_change_report,
};
use serde_json::{Map, Value, json};

fn entry_outcome(timestamp: &str, author: &str, description: &str) -> Value {
    outcome(ChangeLogEntry::new(
        timestamp,
        author,
        description,
        None,
        None,
    ))
}

#[test]
fn test_change_log_entry_timestamp_validation_matches_oracle() {
    let cases = oracle()["changelog_timestamps"].as_array().unwrap();
    assert!(cases.len() > 100);
    for case in cases {
        let ts = case["timestamp"].as_str().unwrap();
        assert_eq!(entry_outcome(ts, "a@b.co", "d"), case["result"], "{ts:?}");
    }
}

#[test]
fn test_change_log_entry_email_validation_matches_oracle() {
    for case in oracle()["changelog_emails"].as_array().unwrap() {
        let author = case["author"].as_str().unwrap();
        assert_eq!(
            entry_outcome("2024-01-15T10:30:00Z", author, "d"),
            case["result"],
            "{author:?}"
        );
    }
}

#[test]
fn test_change_log_entry_description_validation_matches_oracle() {
    for case in oracle()["changelog_descriptions"].as_array().unwrap() {
        let description = case["description"].as_str().unwrap();
        assert_eq!(
            entry_outcome("2024-01-15T10:30:00Z", "a@b.co", description),
            case["result"],
            "{description:?}"
        );
    }
}

#[test]
fn test_change_log_entry_optional_fields_and_serialization() {
    let entry = ChangeLogEntry::new(
        "2024-01-15T10:30:00Z",
        "a@b.co",
        "d",
        Some("C-1".into()),
        Some(vec!["C-0".into(), "C-00".into()]),
    );
    assert_eq!(outcome(entry.clone()), oracle()["changelog_optional"]);
    // Dataclass fields round-trip through serde (asdict).
    let entry = entry.unwrap();
    let back: ChangeLogEntry =
        serde_json::from_value(serde_json::to_value(&entry).unwrap()).unwrap();
    assert_eq!(back, entry);
}

#[test]
fn test_change_log_entry_create_now_shape() {
    let expected = &oracle()["changelog_create_now"];
    let entry =
        ChangeLogEntry::create_now("charlie@company.com", "now", Some("X".into()), None).unwrap();
    assert_eq!(json!(entry.timestamp.contains('T')), expected["has_T"]);
    assert_eq!(json!(entry.change_id), expected["change_id"]);
    assert_eq!(json!(entry.related_changes), expected["related_changes"]);
}

#[test]
fn test_mutation_record_fields() {
    let mut payload = Map::new();
    payload.insert("a".into(), json!(1));
    let record = MutationRecord::new("2024-01-01T00:00:00", "ADD_NODE", "n1", payload, None);
    assert_eq!(
        serde_json::to_value(&record).unwrap(),
        oracle()["mutation_record"]
    );
}

#[test]
fn test_severity_and_change_category_members() {
    let expected = &oracle()["enums"];
    let severities = [
        Severity::Critical,
        Severity::High,
        Severity::Medium,
        Severity::Low,
        Severity::Info,
    ];
    let actual: Vec<Value> = severities
        .iter()
        .map(|m| json!([m.name(), m.value(), m.to_string()]))
        .collect();
    assert_eq!(Value::Array(actual), expected["severity"]);
    let categories = [
        ChangeCategory::Breaking,
        ChangeCategory::PotentiallyBreaking,
        ChangeCategory::NonBreaking,
        ChangeCategory::Unknown,
    ];
    let actual: Vec<Value> = categories
        .iter()
        .map(|m| json!([m.name(), m.value(), m.to_string()]))
        .collect();
    assert_eq!(Value::Array(actual), expected["category"]);
    // Enum values serialize as their `.value`.
    assert_eq!(
        serde_json::to_value(Severity::Critical).unwrap(),
        json!("critical")
    );
    assert_eq!(
        serde_json::to_value(ChangeCategory::PotentiallyBreaking).unwrap(),
        json!("potentially_breaking")
    );
}

#[test]
fn test_impact_report_to_dict() {
    let report = ImpactReport {
        timestamp: "T0".into(),
        summary: obj(json!({"total_changes": 1})),
        breaking_changes: vec![obj(json!({"a": 1}))],
        potentially_breaking: vec![obj(json!({"b": 2}))],
        safe_changes: vec![obj(json!({"c": 3}))],
        recommendations: vec!["r".into()],
    };
    assert_eq!(
        Value::Object(report.to_dict()),
        oracle()["impact_report_to_dict"]
    );
    let default = ImpactReport::new();
    assert_eq!(
        mask(&Value::Object(default.to_dict()), &["timestamp"]),
        oracle()["impact_report_default"]
    );
    assert!(default.timestamp.contains('T'));
}

#[test]
fn test_generate_change_report_matches_oracle() {
    let cases = oracle()["change_reports"].as_array().unwrap();
    assert!(cases.len() > 20);
    for case in cases {
        let diff = case["diff"].as_object().unwrap();
        let actual = outcome(generate_change_report(diff));
        let actual = match actual.get("ok") {
            Some(report) => json!({"ok": mask(report, &["timestamp"])}),
            None => actual,
        };
        assert_eq!(actual, case["result"], "diff {}", case["diff"]);
    }
}

#[test]
fn test_change_log_analyzer_analyze_and_constants() {
    let expected = &oracle()["analyzer_constants"];
    let mut validity = ChangeLogAnalyzer::VALIDITY_CONSTRAINTS.to_vec();
    validity.sort();
    let mut structural = ChangeLogAnalyzer::STRUCTURAL_FIELDS.to_vec();
    structural.sort();
    assert_eq!(json!(validity), expected["validity"]);
    assert_eq!(json!(structural), expected["structural"]);

    // analyze() returns the report generate_change_report() serializes.
    let case = &oracle()["change_reports"][1];
    let report = ChangeLogAnalyzer::new()
        .analyze(case["diff"].as_object().unwrap())
        .unwrap();
    assert_eq!(
        mask(&Value::Object(report.to_dict()), &["timestamp"]),
        case["result"]["ok"]
    );
    assert_eq!(report.breaking_changes.len(), 1);
    assert_eq!(report.breaking_changes[0]["severity"], "critical");
}
