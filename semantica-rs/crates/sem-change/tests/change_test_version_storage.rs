//! Port of `tests/change_management/test_version_storage.py` (`TestInMemoryVersionStorage`,
//! `TestSQLiteVersionStorage`, `TestChecksumUtilities`).
#![allow(non_snake_case)]

mod common;

use std::path::PathBuf;

use common::{assert_raises, is_validation, obj};
use sem_change::{
    InMemoryVersionStorage, SQLiteVersionStorage, VersionStorage, compute_checksum, verify_checksum,
};
use serde_json::{Map, Value, json};

fn sample_snapshot() -> Map<String, Value> {
    obj(json!({
        "label": "v1.0",
        "timestamp": "2024-01-15T10:30:00Z",
        "author": "alice@company.com",
        "description": "Initial version",
        "checksum": "abc123",
        "entities": [{"id": "1", "name": "Entity1"}],
        "relationships": [{"source": "1", "target": "2", "type": "relates"}],
        "metadata": {"version": "1.0"}
    }))
}

// ---------------------------------------------------------------------------------------------
// TestInMemoryVersionStorage
// ---------------------------------------------------------------------------------------------

/// `TestInMemoryVersionStorage.setup_method`.
struct InMemoryFixture {
    storage: InMemoryVersionStorage,
    sample_snapshot: Map<String, Value>,
}

fn in_memory_setup() -> InMemoryFixture {
    InMemoryFixture {
        storage: InMemoryVersionStorage::new(),
        sample_snapshot: sample_snapshot(),
    }
}

#[test]
fn test_in_memory_version_storage__test_save_and_get_snapshot() {
    let f = in_memory_setup();
    f.storage.save(&f.sample_snapshot).unwrap();
    let retrieved = f.storage.get("v1.0").unwrap();

    let retrieved = retrieved.expect("retrieved is not None");
    assert_eq!(retrieved["label"], "v1.0");
    assert_eq!(retrieved["author"], "alice@company.com");
    assert_eq!(retrieved["entities"].as_array().unwrap().len(), 1);
}

#[test]
fn test_in_memory_version_storage__test_save_snapshot_without_label_raises_error() {
    let f = in_memory_setup();
    let mut invalid_snapshot = f.sample_snapshot.clone();
    invalid_snapshot.remove("label");

    assert_raises(
        f.storage.save(&invalid_snapshot),
        is_validation,
        "Snapshot must have a 'label' field",
    );
}

#[test]
fn test_in_memory_version_storage__test_save_duplicate_label_raises_error() {
    let f = in_memory_setup();
    f.storage.save(&f.sample_snapshot).unwrap();

    assert_raises(
        f.storage.save(&f.sample_snapshot),
        is_validation,
        "Version 'v1.0' already exists",
    );
}

#[test]
fn test_in_memory_version_storage__test_get_nonexistent_snapshot_returns_none() {
    let f = in_memory_setup();
    let result = f.storage.get("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_in_memory_version_storage__test_list_all_empty_storage() {
    let f = in_memory_setup();
    let result = f.storage.list_all().unwrap();
    assert_eq!(result, Vec::<Map<String, Value>>::new());
}

#[test]
fn test_in_memory_version_storage__test_list_all_with_snapshots() {
    let f = in_memory_setup();
    f.storage.save(&f.sample_snapshot).unwrap();

    let mut snapshot2 = f.sample_snapshot.clone();
    snapshot2.insert("label".into(), json!("v2.0"));
    f.storage.save(&snapshot2).unwrap();

    let result = f.storage.list_all().unwrap();
    assert_eq!(result.len(), 2);

    let labels: Vec<&Value> = result.iter().map(|item| &item["label"]).collect();
    assert!(labels.contains(&&json!("v1.0")));
    assert!(labels.contains(&&json!("v2.0")));

    // Check metadata structure
    for item in &result {
        assert!(item.contains_key("entity_count"));
        assert!(item.contains_key("relationship_count"));
        assert_eq!(item["entity_count"], 1);
        assert_eq!(item["relationship_count"], 1);
    }
}

#[test]
fn test_in_memory_version_storage__test_exists_method() {
    let f = in_memory_setup();
    assert!(!f.storage.exists("v1.0").unwrap());

    f.storage.save(&f.sample_snapshot).unwrap();
    assert!(f.storage.exists("v1.0").unwrap());
    assert!(!f.storage.exists("v2.0").unwrap());
}

#[test]
fn test_in_memory_version_storage__test_delete_method() {
    let f = in_memory_setup();
    // Delete non-existent returns False
    assert!(!f.storage.delete("nonexistent").unwrap());

    // Save and delete existing returns True
    f.storage.save(&f.sample_snapshot).unwrap();
    assert!(f.storage.delete("v1.0").unwrap());
    assert!(!f.storage.exists("v1.0").unwrap());
}

#[test]
fn test_in_memory_version_storage__test_data_isolation() {
    let f = in_memory_setup();
    f.storage.save(&f.sample_snapshot).unwrap();
    let mut retrieved = f.storage.get("v1.0").unwrap().unwrap();

    // Modify retrieved data
    retrieved
        .get_mut("entities")
        .and_then(Value::as_array_mut)
        .unwrap()
        .push(json!({"id": "2", "name": "Entity2"}));

    // Original should be unchanged
    let retrieved_again = f.storage.get("v1.0").unwrap().unwrap();
    assert_eq!(retrieved_again["entities"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------------------------
// TestSQLiteVersionStorage
// ---------------------------------------------------------------------------------------------

/// `TestSQLiteVersionStorage.setup_method`; dropping it is `teardown_method` (the temporary
/// directory and database are removed).
struct SqliteFixture {
    temp_dir: tempfile::TempDir,
    db_path: PathBuf,
    storage: SQLiteVersionStorage,
    sample_snapshot: Map<String, Value>,
}

fn sqlite_setup() -> SqliteFixture {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_versions.db");
    let storage = SQLiteVersionStorage::new(&db_path).unwrap();
    SqliteFixture {
        temp_dir,
        db_path,
        storage,
        sample_snapshot: sample_snapshot(),
    }
}

#[test]
fn test_sqlite_version_storage__test_database_initialization() {
    let f = sqlite_setup();
    assert!(f.db_path.exists());

    // Verify table exists
    let conn = rusqlite::Connection::open(&f.db_path).unwrap();
    let result: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='versions'",
            [],
            |row| row.get(0),
        )
        .ok();
    drop(conn);

    assert!(result.is_some());
}

#[test]
fn test_sqlite_version_storage__test_save_and_get_snapshot() {
    let f = sqlite_setup();
    f.storage.save(&f.sample_snapshot).unwrap();
    let retrieved = f.storage.get("v1.0").unwrap();

    let retrieved = retrieved.expect("retrieved is not None");
    assert_eq!(retrieved["label"], "v1.0");
    assert_eq!(retrieved["author"], "alice@company.com");
    assert_eq!(retrieved["entities"].as_array().unwrap().len(), 1);
    assert_eq!(retrieved["entities"][0]["name"], "Entity1");
}

#[test]
fn test_sqlite_version_storage__test_save_duplicate_label_raises_error() {
    let f = sqlite_setup();
    f.storage.save(&f.sample_snapshot).unwrap();

    assert_raises(
        f.storage.save(&f.sample_snapshot),
        is_validation,
        "Version 'v1.0' already exists",
    );
}

#[test]
fn test_sqlite_version_storage__test_persistence_across_instances() {
    let f = sqlite_setup();
    // Save with first instance
    f.storage.save(&f.sample_snapshot).unwrap();

    // Create new instance and retrieve
    let new_storage = SQLiteVersionStorage::new(&f.db_path).unwrap();
    let retrieved = new_storage.get("v1.0").unwrap();

    let retrieved = retrieved.expect("retrieved is not None");
    assert_eq!(retrieved["label"], "v1.0");
}

#[test]
fn test_sqlite_version_storage__test_list_all_with_ordering() {
    let f = sqlite_setup();
    // Save multiple snapshots
    let mut snapshot1 = f.sample_snapshot.clone();
    snapshot1.insert("timestamp".into(), json!("2024-01-15T10:30:00Z"));

    let mut snapshot2 = f.sample_snapshot.clone();
    snapshot2.insert("label".into(), json!("v2.0"));
    snapshot2.insert("timestamp".into(), json!("2024-01-15T11:30:00Z"));

    f.storage.save(&snapshot1).unwrap();
    f.storage.save(&snapshot2).unwrap();

    let result = f.storage.list_all().unwrap();
    assert_eq!(result.len(), 2);

    // Should be ordered by timestamp DESC (newest first)
    assert_eq!(result[0]["label"], "v2.0");
    assert_eq!(result[1]["label"], "v1.0");
}

#[test]
fn test_sqlite_version_storage__test_exists_method() {
    let f = sqlite_setup();
    assert!(!f.storage.exists("v1.0").unwrap());

    f.storage.save(&f.sample_snapshot).unwrap();
    assert!(f.storage.exists("v1.0").unwrap());
}

#[test]
fn test_sqlite_version_storage__test_delete_method() {
    let f = sqlite_setup();
    // Delete non-existent returns False
    assert!(!f.storage.delete("nonexistent").unwrap());

    // Save and delete existing returns True
    f.storage.save(&f.sample_snapshot).unwrap();
    assert!(f.storage.delete("v1.0").unwrap());
    assert!(!f.storage.exists("v1.0").unwrap());
}

#[test]
fn test_sqlite_version_storage__test_directory_creation() {
    let f = sqlite_setup();
    let nested_path = f
        .temp_dir
        .path()
        .join("nested")
        .join("path")
        .join("versions.db");
    let _storage = SQLiteVersionStorage::new(&nested_path).unwrap();

    assert!(nested_path.exists());

    // Clean up
    std::fs::remove_file(&nested_path).unwrap();
    std::fs::remove_dir(nested_path.parent().unwrap()).unwrap();
    std::fs::remove_dir(nested_path.parent().unwrap().parent().unwrap()).unwrap();
}

// ---------------------------------------------------------------------------------------------
// TestChecksumUtilities
// ---------------------------------------------------------------------------------------------

#[test]
fn test_checksum_utilities__test_compute_checksum_deterministic() {
    let data = obj(json!({
        "entities": [{"id": "1", "name": "Entity1"}],
        "relationships": [{"source": "1", "target": "2"}],
        "metadata": {"version": "1.0"}
    }));

    let checksum1 = compute_checksum(&data);
    let checksum2 = compute_checksum(&data);

    assert_eq!(checksum1, checksum2);
    assert_eq!(checksum1.len(), 64); // SHA-256 hex length
}

#[test]
fn test_checksum_utilities__test_compute_checksum_different_data() {
    let data1 = obj(json!({"entities": [{"id": "1", "name": "Entity1"}]}));
    let data2 = obj(json!({"entities": [{"id": "1", "name": "Entity2"}]}));

    let checksum1 = compute_checksum(&data1);
    let checksum2 = compute_checksum(&data2);

    assert_ne!(checksum1, checksum2);
}

#[test]
fn test_checksum_utilities__test_compute_checksum_order_independence() {
    let data1 = obj(json!({"b": 2, "a": 1}));
    let data2 = obj(json!({"a": 1, "b": 2}));

    let checksum1 = compute_checksum(&data1);
    let checksum2 = compute_checksum(&data2);

    assert_eq!(checksum1, checksum2);
}

#[test]
fn test_checksum_utilities__test_verify_checksum_valid() {
    let data = obj(json!({"entities": [{"id": "1"}], "metadata": {}}));
    let checksum = compute_checksum(&data);

    let mut snapshot = data.clone();
    snapshot.insert("checksum".into(), Value::String(checksum));

    assert!(verify_checksum(&snapshot));
}

#[test]
fn test_checksum_utilities__test_verify_checksum_invalid() {
    let snapshot = obj(json!({
        "entities": [{"id": "1"}],
        "metadata": {},
        "checksum": "invalid_checksum"
    }));

    assert!(!verify_checksum(&snapshot));
}

#[test]
fn test_checksum_utilities__test_verify_checksum_missing() {
    let snapshot = obj(json!({
        "entities": [{"id": "1"}],
        "metadata": {}
    }));

    assert!(!verify_checksum(&snapshot));
}

#[test]
fn test_checksum_utilities__test_verify_checksum_with_modified_data() {
    let data = obj(json!({"entities": [{"id": "1"}], "metadata": {}}));
    let checksum = compute_checksum(&data);

    // Modify data after computing checksum
    let mut snapshot = data.clone();
    snapshot
        .get_mut("entities")
        .and_then(Value::as_array_mut)
        .unwrap()
        .push(json!({"id": "2"}));
    snapshot.insert("checksum".into(), Value::String(checksum));

    assert!(!verify_checksum(&snapshot));
}
