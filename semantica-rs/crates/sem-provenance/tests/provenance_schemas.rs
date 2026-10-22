//! Characterisation tests for `schemas.rs` and `isoformat.rs` against the oracle
//! (`fixtures/oracle.json`, sections `schemas` and `isoformat`).

mod common;

use chrono::NaiveDate;
use common::{assert_json_eq, err_json, fixed_clock, obj, oracle};
use sem_provenance::isoformat::datetime_fromisoformat;
use sem_provenance::schemas::{
    ActivityRecord, AgentRecord, Invalidation, PropertySource, ProvenanceEntry, SourceReference,
};
use sem_utils::SemanticaError;
use sem_utils::pycompat::time::PyDateTime;
use serde_json::{Map, Value, json};

fn section(key: &str) -> &'static Value {
    &oracle("schemas")[key]
}

fn to_value<T>(result: Result<T, SemanticaError>, f: impl Fn(T) -> Map<String, Value>) -> Value {
    match result {
        Ok(v) => json!({"ok": Value::Object(f(v))}),
        Err(e) => err_json(&e),
    }
}

#[test]
fn provenance_entry_defaults_and_repr_match_oracle() {
    let _clock = fixed_clock();
    let entry = ProvenanceEntry::new("e1", "entity", "act");
    assert_json_eq(&Value::Object(entry.to_dict()), section("default_entry"));
    assert_eq!(
        format!("{entry:?}"),
        section("default_entry_repr").as_str().unwrap()
    );
    let defaulted = ProvenanceEntry {
        entity_id: "e1".into(),
        entity_type: "entity".into(),
        activity_id: "act".into(),
        ..ProvenanceEntry::with_timestamp("", "", "", "2026-01-01T00:00:00")
    };
    assert_eq!(defaulted, entry);
}

#[test]
fn provenance_entry_full_to_dict_matches_oracle() {
    let full = ProvenanceEntry {
        agent_id: "alice".into(),
        agent_type: "person".into(),
        is_automated: false,
        role: Some("approver".into()),
        source_document: "doc".into(),
        source_location: Some("p. 4".into()),
        source_quote: Some("quote".into()),
        first_seen: Some("f".into()),
        last_updated: Some("l".into()),
        confidence: 0.92,
        checksum: Some("c".into()),
        sequence_id: Some(5),
        previous_checksum: Some("abc123".into()),
        parent_entity_id: Some("p".into()),
        used_entities: vec!["u1".into(), "u2".into()],
        previous_version_id: Some("e1:v:1".into()),
        derived_from_id: Some("source_entity".into()),
        activity_started_at_time: Some("2026-01-01T00:00:00".into()),
        activity_ended_at_time: Some("2026-01-01T00:00:05".into()),
        acted_on_behalf_of: Some("org1".into()),
        informed_by_activities: vec!["ingest_activity".into()],
        valid_from: Some("2026-01-01".into()),
        valid_until: Some("2026-06-01".into()),
        revision_type: Some("correction".into()),
        supersedes: Some("old_claim".into()),
        bundle_id: Some("ingestion_run_1".into()),
        invalidated: true,
        invalidated_at_time: Some("2026-01-02T00:00:00".into()),
        invalidated_by: Some("reviewer_jane".into()),
        invalidation_reason: Some("retracted".into()),
        start_index: Some(3),
        end_index: Some(9),
        credibility: Some(0.5),
        metadata: obj(json!({"k": [1, 2.5, null, true]})),
        version: "2.0".into(),
        ..ProvenanceEntry::with_timestamp("e1", "entity", "act", "2026-01-01T00:00:00")
    };
    assert_json_eq(&Value::Object(full.to_dict()), section("full_entry"));
    let restored = ProvenanceEntry::from_dict(&full.to_dict()).unwrap();
    assert_eq!(restored, full);
}

#[test]
fn provenance_entry_from_dict_matches_oracle() {
    let _clock = fixed_clock();
    let data = obj(json!({
        "entity_id": "entity_1", "entity_type": "entity", "activity_id": "extraction",
        "agent_id": "semantica", "source_document": "doc_1", "confidence": 0.9
    }));
    let entry = ProvenanceEntry::from_dict(&data).unwrap();
    assert_json_eq(
        &Value::Object(entry.to_dict()),
        section("from_dict_minimal"),
    );

    let cases = [
        (
            "unexpected",
            json!({"entity_id": "e", "entity_type": "t", "activity_id": "a", "bogus": 1, "other": 2}),
        ),
        ("missing_one", json!({"entity_id": "e", "entity_type": "t"})),
        ("missing_two", json!({"entity_id": "e"})),
        ("missing_three", json!({})),
        ("unexpected_before_missing", json!({"bogus": 1})),
    ];
    for (name, data) in cases {
        let got = to_value(ProvenanceEntry::from_dict(&obj(data)), |e| e.to_dict());
        assert_eq!(got, section("entry_from_dict_errors")[name], "{name}");
    }
}

#[test]
fn source_reference_matches_oracle() {
    let basic = SourceReference {
        page: Some(1),
        ..SourceReference::new("doc_1")
    };
    assert_json_eq(&Value::Object(basic.to_dict()), section("source_basic"));

    let timestamp = NaiveDate::from_ymd_opt(2026, 1, 2)
        .unwrap()
        .and_hms_micro_opt(3, 4, 5, 6)
        .unwrap();
    let full = SourceReference {
        page: Some(4),
        section: Some("Table S4".into()),
        line: Some(7),
        timestamp: Some(PyDateTime::Naive(timestamp)),
        confidence: 0.92,
        metadata: obj(json!({"m": 1})),
        ..SourceReference::new("d")
    };
    assert_json_eq(&Value::Object(full.to_dict()), section("source_full"));
    let round_trip = SourceReference::from_dict(&full.to_dict()).unwrap();
    assert_json_eq(
        &Value::Object(round_trip.to_dict()),
        section("source_round_trip"),
    );
    assert_eq!(round_trip, full);
    let reprs = section("reprs");
    assert_eq!(format!("{full:?}"), reprs["source_full"].as_str().unwrap());
    for (key, text) in [
        ("source_aware", "2026-01-01T00:00:00+05:30"),
        ("source_utc", "2026-01-01T10:20:00Z"),
        ("source_negative", "2026-01-01T10:20:30-08:00"),
    ] {
        let source =
            SourceReference::from_dict(&obj(json!({"document": "d", "timestamp": text}))).unwrap();
        assert_eq!(format!("{source:?}"), reprs[key].as_str().unwrap(), "{key}");
    }

    let errors = section("source_from_dict_errors");
    let cases = [
        ("missing", json!({"page": 1})),
        ("unexpected", json!({"document": "d", "pages": 1})),
        (
            "bad_timestamp",
            json!({"document": "d", "timestamp": "yesterday"}),
        ),
    ];
    for (name, data) in cases {
        let got = to_value(SourceReference::from_dict(&obj(data)), |s| s.to_dict());
        assert_eq!(got, errors[name], "{name}");
    }
}

#[test]
fn property_source_matches_oracle() {
    let prop = PropertySource {
        sources: vec![SourceReference::new("doc_1")],
        entity_id: Some("cabo_pulmo_mpa".into()),
        metadata: obj(json!({"x": 1})),
        ..PropertySource::new("biomass_increase", "463%")
    };
    assert_json_eq(&Value::Object(prop.to_dict()), section("property_source"));
    assert_eq!(
        format!("{prop:?}"),
        section("reprs")["property_source"].as_str().unwrap()
    );
    let restored = PropertySource::from_dict(&prop.to_dict()).unwrap();
    assert_json_eq(
        &Value::Object(restored.to_dict()),
        section("property_source_round_trip"),
    );
    assert_eq!(restored, prop);

    let errors = section("property_source_errors");
    for (name, data) in [
        ("missing", json!({"property_name": "p"})),
        (
            "unexpected",
            json!({"property_name": "p", "value": 1, "x": 2}),
        ),
    ] {
        let got = to_value(PropertySource::from_dict(&obj(data)), |p| p.to_dict());
        assert_eq!(got, errors[name], "{name}");
    }
}

#[test]
fn agent_activity_invalidation_records_match_oracle() {
    let agent = AgentRecord {
        agent_type: "person".into(),
        is_automated: false,
        name: Some("Jane".into()),
        ..AgentRecord::new("reviewer_jane")
    };
    assert_json_eq(&Value::Object(agent.to_dict()), section("agent"));
    assert_json_eq(
        &Value::Object(AgentRecord::new("bot1").to_dict()),
        section("agent_default"),
    );
    let activity = ActivityRecord {
        activity_type: "extraction".into(),
        started_at_time: Some("t0".into()),
        ended_at_time: Some("t1".into()),
        metadata: obj(json!({"a": 1})),
        ..ActivityRecord::new("run")
    };
    assert_json_eq(&Value::Object(activity.to_dict()), section("activity"));
    assert_json_eq(
        &Value::Object(ActivityRecord::new("act1").to_dict()),
        section("activity_default"),
    );
    let inv = Invalidation {
        reason: Some("retracted".into()),
        ..Invalidation::new("e1", "2026-01-01T00:00:00", "reviewer_jane")
    };
    assert_json_eq(&Value::Object(inv.to_dict()), section("invalidation"));

    let errors = section("record_errors");
    assert_eq!(
        to_value(AgentRecord::from_dict(&obj(json!({}))), |a| a.to_dict()),
        errors["agent_missing"]
    );
    assert_eq!(
        to_value(
            AgentRecord::from_dict(&obj(json!({"id": "a", "kind": "x"}))),
            |a| a.to_dict()
        ),
        errors["agent_unexpected"]
    );
    assert_eq!(
        to_value(
            ActivityRecord::from_dict(&obj(json!({"activity_type": "x"}))),
            |a| a.to_dict()
        ),
        errors["activity_missing"]
    );
    assert_eq!(
        to_value(
            Invalidation::from_dict(&obj(json!({"entity_id": "e"}))),
            |i| i.to_dict()
        ),
        errors["invalidation_missing"]
    );
    assert_eq!(
        to_value(Invalidation::from_dict(&obj(json!({}))), |i| i.to_dict()),
        errors["invalidation_missing_all"]
    );

    let reprs = section("reprs");
    assert_eq!(format!("{agent:?}"), reprs["agent"].as_str().unwrap());
    assert_eq!(format!("{activity:?}"), reprs["activity"].as_str().unwrap());
    assert_eq!(format!("{inv:?}"), reprs["invalidation"].as_str().unwrap());
}

#[test]
fn datetime_fromisoformat_matches_cpython() {
    for case in oracle("isoformat").as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let data = obj(json!({"document": "d", "timestamp": input}));
        let got = match SourceReference::from_dict(&data) {
            Ok(source) => json!({"input": input, "ok": source.to_dict()["timestamp"]}),
            Err(err) => {
                let mut v = err_json(&err);
                v["input"] = json!(input);
                v
            }
        };
        if input == "2026-01-01T12:30:45+01:30:15.5" {
            // Known deviation (PORT-NOTES.md): a UTC offset with a fractional second keeps only
            // whole seconds (`PyDateTime` holds a `chrono::FixedOffset`).
            assert_eq!(case["ok"], "2026-01-01T12:30:45+01:30:15.500000");
            assert_eq!(got["ok"], "2026-01-01T12:30:45+01:30:15");
            continue;
        }
        assert_eq!(got, *case, "{input}");
        // The function itself agrees with the dataclass path.
        assert_eq!(
            datetime_fromisoformat(input).is_ok(),
            case.get("ok").is_some()
        );
    }
}
