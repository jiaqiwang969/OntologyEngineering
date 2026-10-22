//! Port of `tests/provenance/test_schemas.py`: W3C PROV-O schemas (`ProvenanceEntry`,
//! `SourceReference`, `PropertySource`, `AgentRecord`, `ActivityRecord`, `Invalidation`).
#![allow(non_snake_case)]

mod common;

use common::obj;
use sem_provenance::schemas::{
    ActivityRecord, AgentRecord, Invalidation, PropertySource, ProvenanceEntry, SourceReference,
};
use serde_json::json;

// --- TestProvenanceEntry ---

#[test]
fn test_provenance_entry__test_create_basic_entry() {
    let entry = ProvenanceEntry::new("entity_1", "entity", "extraction");
    assert_eq!(entry.entity_id, "entity_1");
    assert_eq!(entry.entity_type, "entity");
    assert_eq!(entry.activity_id, "extraction");
    assert_eq!(entry.agent_id, "semantica");
    assert_eq!(entry.confidence, 1.0);
}

#[test]
fn test_provenance_entry__test_entry_with_source_tracking() {
    let entry = ProvenanceEntry {
        source_document: "DOI:10.1371/journal.pone.0023601".into(),
        source_location: Some("Figure 2".into()),
        source_quote: Some("Total fish biomass increased by 463%".into()),
        confidence: 0.92,
        ..ProvenanceEntry::new("entity_1", "entity", "extraction")
    };
    assert_eq!(entry.source_document, "DOI:10.1371/journal.pone.0023601");
    assert_eq!(entry.source_location.as_deref(), Some("Figure 2"));
    assert_eq!(
        entry.source_quote.as_deref(),
        Some("Total fish biomass increased by 463%")
    );
    assert_eq!(entry.confidence, 0.92);
}

#[test]
fn test_provenance_entry__test_entry_with_lineage() {
    let entry = ProvenanceEntry {
        parent_entity_id: Some("entity_1".into()),
        used_entities: vec!["entity_1".into(), "axiom_1".into()],
        ..ProvenanceEntry::new("entity_2", "entity", "transformation")
    };
    assert_eq!(entry.parent_entity_id.as_deref(), Some("entity_1"));
    assert_eq!(entry.used_entities.len(), 2);
    assert!(entry.used_entities.contains(&"entity_1".to_string()));
}

#[test]
fn test_provenance_entry__test_entry_to_dict() {
    let entry = ProvenanceEntry::new("entity_1", "entity", "extraction");
    let data = entry.to_dict();
    assert_eq!(data["entity_id"], "entity_1");
    assert_eq!(data["entity_type"], "entity");
    assert!(data.contains_key("timestamp"));
}

#[test]
fn test_provenance_entry__test_entry_from_dict() {
    let data = obj(json!({
        "entity_id": "entity_1",
        "entity_type": "entity",
        "activity_id": "extraction",
        "agent_id": "semantica",
        "source_document": "doc_1",
        "confidence": 0.9
    }));
    let entry = ProvenanceEntry::from_dict(&data).unwrap();
    assert_eq!(entry.entity_id, "entity_1");
    assert_eq!(entry.confidence, 0.9);
}

// --- TestSourceReference ---

#[test]
fn test_source_reference__test_create_basic_source() {
    let source = SourceReference::new("DOI:10.1038/s41586-021-03371-z");
    assert_eq!(source.document, "DOI:10.1038/s41586-021-03371-z");
    assert_eq!(source.confidence, 1.0);
}

#[test]
fn test_source_reference__test_source_with_location() {
    let source = SourceReference {
        page: Some(4),
        section: Some("Table S4".into()),
        confidence: 0.92,
        ..SourceReference::new("DOI:10.1038/s41586-021-03371-z")
    };
    assert_eq!(source.page, Some(4));
    assert_eq!(source.section.as_deref(), Some("Table S4"));
    assert_eq!(source.confidence, 0.92);
}

#[test]
fn test_source_reference__test_source_to_dict() {
    let source = SourceReference {
        page: Some(1),
        ..SourceReference::new("doc_1")
    };
    let data = source.to_dict();
    assert_eq!(data["document"], "doc_1");
    assert_eq!(data["page"], 1);
}

// --- TestPropertySource ---

#[test]
fn test_property_source__test_create_property_source() {
    let source_ref = SourceReference::new("doc_1");
    let prop_source = PropertySource {
        sources: vec![source_ref],
        entity_id: Some("cabo_pulmo_mpa".into()),
        ..PropertySource::new("biomass_increase", "463%")
    };
    assert_eq!(prop_source.property_name, "biomass_increase");
    assert_eq!(prop_source.value, json!("463%"));
    assert_eq!(prop_source.sources.len(), 1);
    assert_eq!(prop_source.entity_id.as_deref(), Some("cabo_pulmo_mpa"));
}

#[test]
fn test_property_source__test_property_source_to_dict() {
    let source_ref = SourceReference::new("doc_1");
    let prop_source = PropertySource {
        sources: vec![source_ref],
        ..PropertySource::new("name", "test")
    };
    let data = prop_source.to_dict();
    assert_eq!(data["property_name"], "name");
    assert_eq!(data["sources"].as_array().unwrap().len(), 1);
}

// --- TestProvenanceEntryPart825Fields ---

#[test]
fn test_provenance_entry_part825_fields__test_defaults() {
    let entry = ProvenanceEntry::new("e1", "entity", "act");
    assert_eq!(entry.agent_type, "software_agent");
    assert!(entry.is_automated);
    assert_eq!(entry.role, None);
    assert_eq!(entry.previous_version_id, None);
    assert_eq!(entry.derived_from_id, None);
    assert_eq!(entry.sequence_id, None);
    assert_eq!(entry.previous_checksum, None);
    assert!(!entry.invalidated);
    assert_eq!(entry.invalidated_at_time, None);
    assert_eq!(entry.invalidated_by, None);
    assert_eq!(entry.invalidation_reason, None);
    // Part B additive fields
    assert_eq!(entry.activity_started_at_time, None);
    assert_eq!(entry.activity_ended_at_time, None);
    assert_eq!(entry.acted_on_behalf_of, None);
    assert!(entry.informed_by_activities.is_empty());
    assert_eq!(entry.valid_from, None);
    assert_eq!(entry.valid_until, None);
    assert_eq!(entry.revision_type, None);
    assert_eq!(entry.supersedes, None);
    assert_eq!(entry.bundle_id, None);
}

#[test]
fn test_provenance_entry_part825_fields__test_round_trip_to_dict_from_dict() {
    let entry = ProvenanceEntry {
        agent_id: "alice".into(),
        agent_type: "person".into(),
        is_automated: false,
        role: Some("approver".into()),
        previous_version_id: Some("e1:v:1".into()),
        derived_from_id: Some("source_entity".into()),
        sequence_id: Some(5),
        previous_checksum: Some("abc123".into()),
        invalidated: true,
        invalidated_at_time: Some("2026-01-01T00:00:00".into()),
        invalidated_by: Some("reviewer_jane".into()),
        invalidation_reason: Some("retracted".into()),
        activity_started_at_time: Some("2026-01-01T00:00:00".into()),
        activity_ended_at_time: Some("2026-01-01T00:00:05".into()),
        acted_on_behalf_of: Some("org1".into()),
        informed_by_activities: vec!["ingest_activity".into()],
        valid_from: Some("2026-01-01".into()),
        valid_until: Some("2026-06-01".into()),
        revision_type: Some("correction".into()),
        supersedes: Some("old_claim".into()),
        bundle_id: Some("ingestion_run_1".into()),
        ..ProvenanceEntry::new("e1", "entity", "act")
    };
    let data = entry.to_dict();
    let restored = ProvenanceEntry::from_dict(&data).unwrap();
    assert_eq!(restored, entry);
}

// --- TestActivityRecord ---

#[test]
fn test_activity_record__test_defaults() {
    let activity = ActivityRecord::new("act1");
    assert_eq!(activity.activity_type, "process");
    assert_eq!(activity.started_at_time, None);
    assert_eq!(activity.ended_at_time, None);
}

#[test]
fn test_activity_record__test_round_trip() {
    let activity = ActivityRecord {
        activity_type: "extraction".into(),
        started_at_time: Some("2026-01-01T00:00:00".into()),
        ended_at_time: Some("2026-01-01T00:00:03".into()),
        ..ActivityRecord::new("bureau_parsing_run_42")
    };
    let data = activity.to_dict();
    let restored = ActivityRecord::from_dict(&data).unwrap();
    assert_eq!(restored, activity);
}

// --- TestAgentRecord ---

#[test]
fn test_agent_record__test_defaults() {
    let agent = AgentRecord::new("bot1");
    assert_eq!(agent.agent_type, "software_agent");
    assert!(agent.is_automated);
    assert_eq!(agent.name, None);
}

#[test]
fn test_agent_record__test_round_trip() {
    let agent = AgentRecord {
        agent_type: "person".into(),
        is_automated: false,
        name: Some("Jane".into()),
        ..AgentRecord::new("reviewer_jane")
    };
    let data = agent.to_dict();
    let restored = AgentRecord::from_dict(&data).unwrap();
    assert_eq!(restored, agent);
}

// --- TestInvalidation ---

#[test]
fn test_invalidation__test_round_trip() {
    let inv = Invalidation {
        reason: Some("retracted".into()),
        ..Invalidation::new("e1", "2026-01-01T00:00:00", "reviewer_jane")
    };
    let data = inv.to_dict();
    let restored = Invalidation::from_dict(&data).unwrap();
    assert_eq!(restored, inv);
}
