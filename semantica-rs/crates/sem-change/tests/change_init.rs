//! The package export surface (`semantica/change_management/__init__.py` -> `lib.rs`): every
//! name in `__all__` is importable from the crate root and usable.

mod common;

use common::{obj, quiet_progress};
use serde_json::json;

#[test]
fn test_change_management_exports() {
    quiet_progress();
    // Change metadata
    let entry =
        sem_change::ChangeLogEntry::new("2024-01-15T10:30:00Z", "a@b.co", "d", None, None).unwrap();
    assert_eq!(entry.author, "a@b.co");

    // Storage backends
    let memory: Box<dyn sem_change::VersionStorage> =
        Box::new(sem_change::InMemoryVersionStorage::new());
    let dir = tempfile::tempdir().unwrap();
    let sqlite: Box<dyn sem_change::VersionStorage> =
        Box::new(sem_change::SQLiteVersionStorage::new(dir.path().join("x.db")).unwrap());
    for storage in [memory, sqlite] {
        storage.save(&obj(json!({"label": "v"}))).unwrap();
        assert!(storage.exists("v").unwrap());
    }

    // Integrity utilities
    let mut data = obj(json!({"b": 2, "a": 1}));
    let checksum = sem_change::compute_checksum(&data);
    assert_eq!(
        checksum,
        "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777"
    );
    data.insert("checksum".into(), json!(checksum));
    assert!(sem_change::verify_checksum(&data));

    // Version managers
    let temporal = sem_change::TemporalVersionManager::new(None, None).unwrap();
    let ontology = sem_change::OntologyVersionManager::new(None, None).unwrap();
    let managers: [&dyn sem_change::BaseVersionManager; 2] = [&temporal, &ontology];
    for manager in managers {
        assert!(manager.list_versions().unwrap().is_empty());
    }

    // Ontology version management
    let mut vm = sem_change::VersionManager::new(None, None);
    let version: sem_change::OntologyVersion = vm
        .create_version("1.0", &mut serde_json::Map::new(), &Default::default())
        .unwrap();
    assert_eq!(version.ontology_iri, "https://semantica.dev/ontology/v1.0");

    // Module attributes
    assert_eq!(sem_change::VERSION, "1.0.0");
    assert_eq!(sem_change::AUTHOR, "Semantica Team");
    assert_eq!(
        sem_change::DESCRIPTION,
        "Enhanced Change Management for Semantica"
    );
}
