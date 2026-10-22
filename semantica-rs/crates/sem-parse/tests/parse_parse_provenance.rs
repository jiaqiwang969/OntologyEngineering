//! Tests for `parse_provenance.rs` (ParserWithProvenance).
//!
//! The original constructor always fails (`semantica.parse.parser` does not exist); that
//! outcome is checked against the oracle (fixtures/core, section "provenance"). `parse` can
//! never run in the original, so its tracking behaviour is checked by construction from the
//! original source, with a fake wrapped parser and an in-memory provenance store.

mod common;

use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, TimeDelta};
use serde_json::{Value, json};

use sem_parse::methods::ParseOutput;
use sem_parse::parse_provenance::{ParserWithProvenance, ProvenanceParser};
use sem_parse::{DocumentParser, Options};
use sem_provenance::{InMemoryStorage, ProvenanceManager};

/// A wrapped parser that records its calls (the test double for `Parser`).
#[derive(Default)]
struct FakeParser {
    calls: Arc<Mutex<Vec<(String, Options)>>>,
}

impl ProvenanceParser for FakeParser {
    fn parse(&self, file_path: &str, kwargs: &Options) -> Result<ParseOutput, sem_utils::SemanticaError> {
        self.calls.lock().unwrap().push((file_path.to_string(), kwargs.clone()));
        Ok(ParseOutput::Value(json!({"parsed": file_path})))
    }
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn test_constructor_fails_like_original() {
    let oracle = common::load_oracle("core");
    let exp = &oracle["provenance"];
    let err = ParserWithProvenance::new(None, None, None, Options::new()).unwrap_err();
    assert_eq!(common::err_json(&err), exp["construct_default"]);
    let err = ParserWithProvenance::new(Some(true), Some("agent-x"), Some(false), opts(json!({"x": 1}))).unwrap_err();
    assert_eq!(common::err_json(&err), exp["construct_tracking"]);
}

#[test]
fn test_parse_tracks_parsed_data_entity() {
    let fake = FakeParser::default();
    let calls = fake.calls.clone();
    let manager = ProvenanceManager::with_storage(InMemoryStorage::new());
    let wrapper = ParserWithProvenance::with_manager(Box::new(fake), manager, Some("agent-x"), Some(false));
    assert!(wrapper.provenance);
    assert_eq!(wrapper.agent_id(), "agent-x");
    assert!(!wrapper.is_automated());

    let start = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap().and_hms_opt(3, 4, 5).unwrap();
    let _clock = sem_provenance::clock::fixed_clock(start, TimeDelta::seconds(1));
    let out = wrapper.parse("docs/report.txt", &opts(json!({"format": "txt", "x": 1}))).unwrap();
    assert_eq!(out.to_value(), json!({"parsed": "docs/report.txt"}));
    assert_eq!(calls.lock().unwrap().clone(), vec![("docs/report.txt".to_string(), opts(json!({"format": "txt", "x": 1})))]);

    let entries = wrapper.prov_manager().unwrap().storage().retrieve_all(None).unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert!(entry.entity_id.starts_with("parse_") && entry.entity_id.len() == 14, "{}", entry.entity_id);
    assert!(entry.entity_id[6..].chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(entry.entity_type, "parsed_data");
    assert_eq!(entry.agent_id, "agent-x");
    assert_eq!(entry.agent_type, "software_agent");
    assert!(!entry.is_automated);
    assert_eq!(entry.source_document, "docs/report.txt");
    assert_eq!(entry.activity_started_at_time.as_deref(), Some("2026-01-02T03:04:05"));
    assert_eq!(entry.activity_ended_at_time.as_deref(), Some("2026-01-02T03:04:06"));
    assert_eq!(entry.metadata.get("file_path"), Some(&json!("docs/report.txt")));
    assert_eq!(entry.metadata.get("format"), Some(&json!("txt")));

    // Without a `format` keyword the metadata says "unknown".
    wrapper.parse("b.txt", &Options::new()).unwrap();
    let entries = wrapper.prov_manager().unwrap().storage().retrieve_all(Some("parsed_data")).unwrap();
    let second = entries.iter().find(|e| e.source_document == "b.txt").unwrap();
    assert_eq!(second.metadata.get("format"), Some(&json!("unknown")));
}

#[test]
fn test_parse_without_provenance_only_delegates() {
    common::quiet_progress();
    let fake = FakeParser::default();
    let calls = fake.calls.clone();
    let wrapper = ParserWithProvenance::with_parser(Box::new(fake), None, None, None).unwrap();
    assert!(!wrapper.provenance);
    assert!(wrapper.prov_manager().is_none());
    assert_eq!(wrapper.agent_id(), "ParserWithProvenance");
    assert!(wrapper.is_automated());
    wrapper.parse("x.md", &Options::new()).unwrap();
    assert_eq!(calls.lock().unwrap().len(), 1);
    // `__getattr__`: the wrapped parser is reachable through Deref.
    let out = ProvenanceParser::parse(&*wrapper, "y.md", &Options::new()).unwrap();
    assert_eq!(out.to_value(), json!({"parsed": "y.md"}));

    // With provenance on, a default (in-memory) manager is created.
    let tracking = ParserWithProvenance::with_parser(Box::new(FakeParser::default()), Some(true), None, None).unwrap();
    assert!(tracking.prov_manager().is_some());

    // A real parser can be wrapped.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");
    std::fs::write(&path, "hello").unwrap();
    let doc = ParserWithProvenance::with_parser(Box::new(DocumentParser::default()), Some(false), None, None).unwrap();
    let out = doc.parse(&path.display().to_string(), &Options::new()).unwrap();
    assert_eq!(out.get("text").unwrap(), Some(&json!("hello")));
}
