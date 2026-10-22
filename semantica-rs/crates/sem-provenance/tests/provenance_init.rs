//! The `semantica.provenance` export surface (`__init__.py`): every name in `__all__` is
//! reachable from the crate root, plus the module constants, checked against the oracle
//! (`fixtures/oracle.json`, section `module`).

mod common;

use common::{fixed_clock, oracle};
use serde_json::{Value, json};

#[test]
fn all_matches_oracle() {
    assert_eq!(json!(sem_provenance::ALL), oracle("module")["all"]);
    assert_eq!(sem_provenance::VERSION, oracle("module")["version"]);
    assert_eq!(sem_provenance::AUTHOR, "Semantica Team");
    assert_eq!(
        sem_provenance::DESCRIPTION,
        "Audit-Grade Provenance Tracking for Semantica"
    );
    assert_eq!(
        sem_provenance::DEFAULT_BASE_URI,
        oracle("module")["default_base_uri"]
    );
}

#[test]
fn manager_constants_match_oracle() {
    let mut kwargs: Vec<&str> = sem_provenance::ProvenanceManager::TRACK_ENTITY_KWARGS.to_vec();
    kwargs.sort();
    assert_eq!(json!(kwargs), oracle("module")["track_entity_kwargs"]);
    let columns: Vec<Value> = sem_provenance::SQLiteStorage::MIGRATION_COLUMNS
        .iter()
        .map(|(name, definition)| json!([name, definition]))
        .collect();
    assert_eq!(Value::Array(columns), oracle("module")["migration_columns"]);
}

#[test]
fn exported_names_work_from_the_crate_root() {
    use sem_provenance::{
        ActivityRecord, AgentRecord, InMemoryStorage, Invalidation, ProvenanceEntry,
        ProvenanceManager, ProvenanceStorage, SQLiteStorage, SourceReference, compute_checksum,
        default_storage_path, verify_checksum,
    };

    let _clock = fixed_clock();
    let mut entry = ProvenanceEntry::new("e1", "entity", "act");
    entry.checksum = Some(compute_checksum(&entry));
    assert!(verify_checksum(&entry, None));
    assert_eq!(SourceReference::new("doc").document, "doc");
    assert_eq!(AgentRecord::new("a").agent_type, "software_agent");
    assert_eq!(ActivityRecord::new("x").activity_type, "process");
    assert_eq!(Invalidation::new("e", "t", "by").entity_id, "e");

    let storage: Box<dyn ProvenanceStorage> = Box::new(InMemoryStorage::new());
    storage.store(&entry).unwrap();
    assert_eq!(storage.retrieve_all(None).unwrap().len(), 1);

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("init.db");
    let sqlite = SQLiteStorage::new(path.to_str().unwrap()).unwrap();
    sqlite.store(&entry).unwrap();
    assert!(sqlite.retrieve("e1").unwrap().is_some());

    {
        let _guard = default_storage_path(Some(path.to_str().unwrap()));
        let manager = ProvenanceManager::new().unwrap();
        assert!(manager.storage().is::<SQLiteStorage>());
        assert!(manager.get_provenance("e1").unwrap().is_some());
    }
    assert!(
        ProvenanceManager::new()
            .unwrap()
            .storage()
            .is::<InMemoryStorage>()
    );
}
