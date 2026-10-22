//! Characterisation tests for `integrity.rs` against the oracle (`fixtures/oracle.json`,
//! section `integrity`).

mod common;

use common::{assert_json_eq, obj, oracle};
use sem_provenance::integrity::{
    compute_checksum, compute_data_checksum, compute_dict_checksum, verify_checksum,
    verify_data_checksum, verify_dict_checksum,
};
use sem_provenance::schemas::ProvenanceEntry;
use serde_json::{Value, json};

fn entries() -> Vec<(&'static str, ProvenanceEntry)> {
    let base = |id: &str, kind: &str, act: &str, ts: &str| {
        ProvenanceEntry::with_timestamp(id, kind, act, ts)
    };
    vec![
        ("plain", base("x", "entity", "act", "2026-01-01T00:00:00")),
        (
            "rich",
            ProvenanceEntry {
                agent_id: "alice".into(),
                agent_type: "person".into(),
                source_document: "DOI:10.1/x".into(),
                confidence: 0.92,
                parent_entity_id: Some("p".into()),
                previous_version_id: Some("y:v:1".into()),
                derived_from_id: Some("d".into()),
                used_entities: vec!["u1".into(), "u2".into()],
                previous_checksum: Some("prev".into()),
                invalidated: true,
                invalidated_at_time: Some("t".into()),
                invalidated_by: Some("jane".into()),
                invalidation_reason: Some("why".into()),
                ..base("y", "claim", "extract", "2026-01-01T00:00:01.250000")
            },
        ),
        (
            "float_confidence",
            ProvenanceEntry {
                confidence: 0.1 + 0.2,
                ..base("z", "entity", "act", "2026")
            },
        ),
        (
            "zero_confidence",
            ProvenanceEntry {
                confidence: 0.0,
                agent_id: String::new(),
                agent_type: String::new(),
                ..base("z", "entity", "act", "2026")
            },
        ),
        (
            "unicode",
            ProvenanceEntry {
                source_document: "\u{1f389}".into(),
                confidence: 1e-7,
                ..base("\u{e9}", "\u{5b9e}\u{4f53}", "act", "2026")
            },
        ),
        (
            "big_confidence",
            ProvenanceEntry {
                confidence: 1e16,
                ..base("b", "e", "a", "2026")
            },
        ),
    ]
}

#[test]
fn entry_checksums_match_oracle() {
    let expected = &oracle("integrity")["entries"];
    for (name, mut entry) in entries() {
        let case = &expected[name];
        let checksum = compute_checksum(&entry);
        assert_eq!(checksum, case["checksum"].as_str().unwrap(), "{name}");
        entry.checksum = Some(checksum);
        assert_json_eq(&Value::Object(entry.to_dict()), &case["dict"]);
        assert_eq!(
            compute_checksum(&entry.to_dict()),
            case["dict_checksum"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(json!(verify_checksum(&entry, None)), case["verify"]);
        assert_eq!(
            json!(verify_checksum(&entry.to_dict(), None)),
            case["verify_dict"]
        );
        assert_eq!(
            json!(verify_checksum(&entry, Some("nope"))),
            case["verify_other"]
        );
    }
}

#[test]
fn dict_path_checksums_match_oracle() {
    for case in oracle("integrity")["dict_cases"].as_array().unwrap() {
        let input = obj(case["input"].clone());
        assert_eq!(
            compute_checksum(&input),
            case["checksum"].as_str().unwrap(),
            "{input:?}"
        );
    }
    let verify = &oracle("integrity")["verify_cases"];
    assert_eq!(
        json!(verify_checksum(&obj(json!({"entity_type": "t"})), None)),
        verify["dict_without_checksum"]
    );
    assert_eq!(
        json!(verify_checksum(
            &obj(json!({"entity_type": "t", "checksum": null})),
            None
        )),
        verify["dict_none_checksum"]
    );
    assert_eq!(
        json!(verify_checksum(
            &ProvenanceEntry::with_timestamp("q", "t", "a", "x"),
            None
        )),
        verify["entry_without_checksum"]
    );
}

#[test]
fn data_and_dict_checksums_match_oracle() {
    let integrity = oracle("integrity");
    for (data, checksum) in integrity["data_checksums"].as_object().unwrap() {
        assert_eq!(compute_data_checksum(data), checksum.as_str().unwrap());
    }
    assert_eq!(
        json!([
            verify_data_checksum("some data", &compute_data_checksum("some data")),
            verify_data_checksum("some data", "x")
        ]),
        integrity["verify_data"]
    );
    for case in integrity["dict_checksums"].as_array().unwrap() {
        let input = obj(case["input"].clone());
        let checksum = compute_dict_checksum(&input);
        assert_eq!(
            checksum,
            case["checksum"].as_str().unwrap(),
            "{}",
            case["repr"]
        );
        assert_eq!(
            json!(verify_dict_checksum(&input, &checksum)),
            case["verify"]
        );
        assert!(!verify_dict_checksum(&input, "0"));
    }
}
