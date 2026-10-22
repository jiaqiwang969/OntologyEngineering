//! Port of `tests/provenance/test_storage.py`: the `InMemoryStorage` and `SQLiteStorage`
//! backends (`TestInMemoryStorage`, `TestSQLiteStorage`).
//!
//! The original creates each database with `tempfile.NamedTemporaryFile(delete=False,
//! suffix=".db")` and unlinks it in `finally`; here the `.db` file is a `tempfile` named file
//! that is removed when the test ends.
#![allow(non_snake_case)]

use std::collections::BTreeSet;

use sem_provenance::schemas::ProvenanceEntry;
use sem_provenance::storage::{InMemoryStorage, ProvenanceStorage, SQLiteStorage};

fn entry(entity_id: &str, entity_type: &str, activity_id: &str) -> ProvenanceEntry {
    ProvenanceEntry::new(entity_id, entity_type, activity_id)
}

fn temp_db() -> tempfile::TempPath {
    tempfile::Builder::new()
        .suffix(".db")
        .tempfile()
        .expect("temporary database file")
        .into_temp_path()
}

fn db_str(path: &tempfile::TempPath) -> String {
    path.to_str().expect("UTF-8 temp path").to_string()
}

fn id_set(entries: &[ProvenanceEntry]) -> BTreeSet<String> {
    entries.iter().map(|e| e.entity_id.clone()).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// --- TestInMemoryStorage ---

#[test]
fn test_in_memory_storage__test_store_and_retrieve() {
    let storage = InMemoryStorage::new();
    let e = entry("entity_1", "entity", "extraction");
    storage.store(&e).unwrap();
    let retrieved = storage.retrieve("entity_1").unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().entity_id, "entity_1");
}

#[test]
fn test_in_memory_storage__test_retrieve_nonexistent() {
    let storage = InMemoryStorage::new();
    let retrieved = storage.retrieve("nonexistent").unwrap();
    assert!(retrieved.is_none());
}

#[test]
fn test_in_memory_storage__test_retrieve_all() {
    let storage = InMemoryStorage::new();
    storage
        .store(&entry("entity_1", "entity", "extraction"))
        .unwrap();
    storage
        .store(&entry("entity_2", "chunk", "chunking"))
        .unwrap();
    let all_entries = storage.retrieve_all(None).unwrap();
    assert_eq!(all_entries.len(), 2);
}

#[test]
fn test_in_memory_storage__test_retrieve_by_type() {
    let storage = InMemoryStorage::new();
    storage
        .store(&entry("entity_1", "entity", "extraction"))
        .unwrap();
    storage
        .store(&entry("chunk_1", "chunk", "chunking"))
        .unwrap();
    let entities = storage.retrieve_all(Some("entity")).unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].entity_type, "entity");
}

#[test]
fn test_in_memory_storage__test_trace_lineage() {
    let storage = InMemoryStorage::new();
    // Create parent-child chain
    let entry1 = entry("entity_1", "entity", "extraction");
    let entry2 = ProvenanceEntry {
        parent_entity_id: Some("entity_1".into()),
        ..entry("entity_2", "entity", "transformation")
    };
    let entry3 = ProvenanceEntry {
        parent_entity_id: Some("entity_2".into()),
        ..entry("entity_3", "entity", "transformation")
    };
    storage.store(&entry1).unwrap();
    storage.store(&entry2).unwrap();
    storage.store(&entry3).unwrap();

    let lineage = storage.trace_lineage("entity_3", None).unwrap();
    assert_eq!(lineage.len(), 3);
    let entity_ids: Vec<String> = lineage.iter().map(|e| e.entity_id.clone()).collect();
    assert!(entity_ids.contains(&"entity_1".to_string()));
    assert!(entity_ids.contains(&"entity_2".to_string()));
    assert!(entity_ids.contains(&"entity_3".to_string()));
}

#[test]
fn test_in_memory_storage__test_clear() {
    let storage = InMemoryStorage::new();
    storage
        .store(&entry("entity_1", "entity", "extraction"))
        .unwrap();
    let count = storage.clear().unwrap();
    assert_eq!(count, 1);
    assert_eq!(storage.retrieve_all(None).unwrap().len(), 0);
}

#[test]
fn test_in_memory_storage__test_clear_resets_chain_state() {
    // Issue #825 fix: clear() must reset chain state so get_chain_head() returns None and the
    // first write after clear starts a fresh chain.
    let storage = InMemoryStorage::new();
    let entry1 = ProvenanceEntry {
        sequence_id: Some(1),
        checksum: Some("checksum_1".into()),
        ..entry("e1", "entity", "act")
    };
    storage.store(&entry1).unwrap();
    assert_eq!(
        storage.get_chain_head(None).unwrap(),
        Some((1, Some("checksum_1".to_string())))
    );

    storage.clear().unwrap();

    // Chain head must be None after clear
    assert_eq!(storage.get_chain_head(None).unwrap(), None);

    // First write after clear starts a fresh chain
    let entry2 = ProvenanceEntry {
        sequence_id: Some(1),
        checksum: Some("checksum_fresh".into()),
        ..entry("e2", "entity", "act")
    };
    storage.store(&entry2).unwrap();
    assert_eq!(
        storage.get_chain_head(None).unwrap(),
        Some((1, Some("checksum_fresh".to_string())))
    );
}

#[test]
fn test_in_memory_storage__test_get_chain_head() {
    // Issue #825, Part A item 2 — chain head reporting.
    let storage = InMemoryStorage::new();
    assert_eq!(storage.get_chain_head(None).unwrap(), None);

    let entry1 = ProvenanceEntry {
        sequence_id: Some(1),
        checksum: Some("checksum_1".into()),
        ..entry("e1", "entity", "act")
    };
    storage.store(&entry1).unwrap();
    assert_eq!(
        storage.get_chain_head(None).unwrap(),
        Some((1, Some("checksum_1".to_string())))
    );

    let entry2 = ProvenanceEntry {
        sequence_id: Some(2),
        checksum: Some("checksum_2".into()),
        ..entry("e2", "entity", "act")
    };
    storage.store(&entry2).unwrap();
    assert_eq!(
        storage.get_chain_head(None).unwrap(),
        Some((2, Some("checksum_2".to_string())))
    );
}

fn store_descendant_fixture(storage: &dyn ProvenanceStorage) {
    storage.store(&entry("a", "entity", "act")).unwrap();
    storage
        .store(&ProvenanceEntry {
            parent_entity_id: Some("a".into()),
            ..entry("b", "entity", "act")
        })
        .unwrap();
    storage
        .store(&ProvenanceEntry {
            parent_entity_id: Some("b".into()),
            ..entry("c", "entity", "act")
        })
        .unwrap();
    storage
        .store(&ProvenanceEntry {
            used_entities: vec!["a".into()],
            ..entry("d", "entity", "act")
        })
        .unwrap();
}

#[test]
fn test_in_memory_storage__test_trace_descendants() {
    // Issue #825, Part A item 5 — reverse (downstream) lineage traversal.
    let storage = InMemoryStorage::new();
    store_descendant_fixture(&storage);
    let descendants = storage.trace_descendants("a", None).unwrap();
    assert_eq!(id_set(&descendants), set(&["b", "c", "d"]));
}

#[test]
fn test_in_memory_storage__test_trace_descendants_respects_max_depth() {
    let storage = InMemoryStorage::new();
    storage.store(&entry("a", "entity", "act")).unwrap();
    storage
        .store(&ProvenanceEntry {
            parent_entity_id: Some("a".into()),
            ..entry("b", "entity", "act")
        })
        .unwrap();
    storage
        .store(&ProvenanceEntry {
            parent_entity_id: Some("b".into()),
            ..entry("c", "entity", "act")
        })
        .unwrap();
    let descendants = storage.trace_descendants("a", Some(1)).unwrap();
    assert_eq!(id_set(&descendants), set(&["b"]));
}

#[test]
fn test_in_memory_storage__test_trace_descendants_empty_for_leaf() {
    let storage = InMemoryStorage::new();
    storage.store(&entry("leaf", "entity", "act")).unwrap();
    assert!(storage.trace_descendants("leaf", None).unwrap().is_empty());
}

// --- TestSQLiteStorage ---

#[test]
fn test_sqlite_storage__test_migrates_pre_existing_old_schema_database() {
    // SQLiteStorage must migrate a provenance.db created before issue #825's Part A/B columns
    // existed in place instead of failing on every insert/select.
    let db_path = temp_db();
    {
        // Simulate a database created by the pre-#825 code: only the original 19 columns.
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE provenance (
                entity_id TEXT PRIMARY KEY,
                entity_type TEXT NOT NULL,
                activity_id TEXT NOT NULL,
                agent_id TEXT DEFAULT 'semantica',
                source_document TEXT,
                source_location TEXT,
                source_quote TEXT,
                timestamp TEXT NOT NULL,
                first_seen TEXT,
                last_updated TEXT,
                confidence REAL DEFAULT 1.0,
                checksum TEXT,
                parent_entity_id TEXT,
                used_entities TEXT,
                start_index INTEGER,
                end_index INTEGER,
                credibility REAL,
                metadata TEXT,
                version TEXT DEFAULT '1.0'
            )
        ",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO provenance VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                "old_entity",
                "entity",
                "legacy_activity",
                "semantica",
                "doc1",
                None::<String>,
                None::<String>,
                "2025-01-01T00:00:00",
                "2025-01-01T00:00:00",
                "2025-01-01T00:00:00",
                1.0,
                "oldchecksum",
                None::<String>,
                "[]",
                None::<i64>,
                None::<i64>,
                None::<f64>,
                "{}",
                "1.0",
            ],
        )
        .unwrap();
    }

    // Opening with the current SQLiteStorage must not crash, and must migrate in place.
    let storage = SQLiteStorage::new(db_str(&db_path)).unwrap();

    let old_entry = storage.retrieve("old_entity").unwrap();
    assert!(old_entry.is_some());
    let old_entry = old_entry.unwrap();
    assert_eq!(old_entry.entity_id, "old_entity");
    assert_eq!(old_entry.agent_type, "software_agent"); // new column default
    assert_eq!(old_entry.sequence_id, None); // never assigned pre-migration

    // New writes against the migrated table must work too.
    let new_entry = ProvenanceEntry {
        agent_id: "alice".into(),
        sequence_id: Some(1),
        checksum: Some("chk1".into()),
        ..entry("new_entity", "entity", "act")
    };
    storage.store(&new_entry).unwrap();
    let retrieved = storage.retrieve("new_entity").unwrap().unwrap();
    assert_eq!(retrieved.agent_id, "alice");
    assert_eq!(retrieved.sequence_id, Some(1));
}

#[test]
fn test_sqlite_storage__test_all_fields_round_trip_through_sqlite() {
    // Every ProvenanceEntry field, including issue #825 Part A and Part B additions, must
    // survive a SQLite store/retrieve round trip.
    let db_path = temp_db();
    let storage = SQLiteStorage::new(db_str(&db_path)).unwrap();
    let e = ProvenanceEntry {
        agent_id: "alice".into(),
        agent_type: "person".into(),
        is_automated: false,
        role: Some("approver".into()),
        sequence_id: Some(1),
        previous_checksum: Some("prevchk".into()),
        parent_entity_id: Some("parent_1".into()),
        used_entities: vec!["u1".into(), "u2".into()],
        previous_version_id: Some("entity_1:v:1".into()),
        derived_from_id: Some("source_1".into()),
        invalidated: true,
        invalidated_at_time: Some("2026-01-01T00:00:00".into()),
        invalidated_by: Some("reviewer_jane".into()),
        invalidation_reason: Some("retracted".into()),
        activity_started_at_time: Some("2026-01-01T00:00:00".into()),
        activity_ended_at_time: Some("2026-01-01T00:00:05".into()),
        acted_on_behalf_of: Some("org1".into()),
        informed_by_activities: vec!["act_a".into(), "act_b".into()],
        valid_from: Some("2026-01-01".into()),
        valid_until: Some("2026-06-01".into()),
        revision_type: Some("correction".into()),
        supersedes: Some("old_claim".into()),
        bundle_id: Some("ingestion_run_1".into()),
        ..entry("entity_1", "entity", "extraction")
    };
    storage.store(&e).unwrap();
    let retrieved = storage.retrieve("entity_1").unwrap();
    assert_eq!(retrieved, Some(e));
}

#[test]
fn test_sqlite_storage__test_store_and_retrieve() {
    let db_path = temp_db();
    let storage = SQLiteStorage::new(db_str(&db_path)).unwrap();
    storage
        .store(&entry("entity_1", "entity", "extraction"))
        .unwrap();
    let retrieved = storage.retrieve("entity_1").unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().entity_id, "entity_1");
}

#[test]
fn test_sqlite_storage__test_persistence() {
    // Data persists across connections.
    let db_path = temp_db();
    let storage1 = SQLiteStorage::new(db_str(&db_path)).unwrap();
    storage1
        .store(&entry("entity_1", "entity", "extraction"))
        .unwrap();

    // Retrieve with new connection
    let storage2 = SQLiteStorage::new(db_str(&db_path)).unwrap();
    let retrieved = storage2.retrieve("entity_1").unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().entity_id, "entity_1");
}

#[test]
fn test_sqlite_storage__test_trace_lineage() {
    let db_path = temp_db();
    let storage = SQLiteStorage::new(db_str(&db_path)).unwrap();
    // Create parent-child chain
    storage
        .store(&entry("entity_1", "entity", "extraction"))
        .unwrap();
    storage
        .store(&ProvenanceEntry {
            parent_entity_id: Some("entity_1".into()),
            ..entry("entity_2", "entity", "transformation")
        })
        .unwrap();
    let lineage = storage.trace_lineage("entity_2", None).unwrap();
    assert_eq!(lineage.len(), 2);
}

#[test]
fn test_sqlite_storage__test_get_chain_head() {
    // Issue #825, Part A item 2 — chain head reporting.
    let db_path = temp_db();
    let storage = SQLiteStorage::new(db_str(&db_path)).unwrap();
    assert_eq!(storage.get_chain_head(None).unwrap(), None);

    storage
        .store(&ProvenanceEntry {
            sequence_id: Some(1),
            checksum: Some("checksum_1".into()),
            ..entry("e1", "entity", "act")
        })
        .unwrap();
    assert_eq!(
        storage.get_chain_head(None).unwrap(),
        Some((1, Some("checksum_1".to_string())))
    );

    storage
        .store(&ProvenanceEntry {
            sequence_id: Some(2),
            checksum: Some("checksum_2".into()),
            ..entry("e2", "entity", "act")
        })
        .unwrap();
    assert_eq!(
        storage.get_chain_head(None).unwrap(),
        Some((2, Some("checksum_2".to_string())))
    );
}

#[test]
fn test_sqlite_storage__test_trace_descendants() {
    // Issue #825, Part A item 5 — reverse (downstream) lineage traversal.
    let db_path = temp_db();
    let storage = SQLiteStorage::new(db_str(&db_path)).unwrap();
    store_descendant_fixture(&storage);
    let descendants = storage.trace_descendants("a", None).unwrap();
    assert_eq!(id_set(&descendants), set(&["b", "c", "d"]));
}

#[test]
fn test_sqlite_storage__test_trace_descendants_empty_for_leaf() {
    let db_path = temp_db();
    let storage = SQLiteStorage::new(db_str(&db_path)).unwrap();
    storage.store(&entry("leaf", "entity", "act")).unwrap();
    assert!(storage.trace_descendants("leaf", None).unwrap().is_empty());
}
