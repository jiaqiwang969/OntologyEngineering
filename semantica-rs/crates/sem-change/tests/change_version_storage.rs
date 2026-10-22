//! Characterisation tests for `version_storage.rs`
//! (`semantica/change_management/version_storage.py`): checksums, both storage backends, the
//! SQLite schema and on-disk format (including reading a database written by the original).
//! Expected values come from the oracle (`tests/fixtures/oracle.json`).

mod common;

use common::{mask, obj, oracle, outcome};
use sem_change::version_storage::create_graph_snapshot_record;
use sem_change::{
    InMemoryVersionStorage, SQLiteVersionStorage, VersionStorage, compute_checksum, verify_checksum,
};
use serde_json::{Map, Value, json};

#[test]
fn test_compute_checksum_matches_oracle() {
    for case in oracle()["checksums"].as_array().unwrap() {
        let data = case["data"].as_object().unwrap();
        assert_eq!(
            compute_checksum(data),
            case["checksum"].as_str().unwrap(),
            "{}",
            case["data"]
        );
    }
}

#[test]
fn test_verify_checksum_matches_oracle() {
    for case in oracle()["verify_checksum"].as_array().unwrap() {
        let snapshot = case["snapshot"].as_object().unwrap();
        assert_eq!(
            json!(verify_checksum(snapshot)),
            case["valid"],
            "{}",
            case["snapshot"]
        );
    }
}

#[test]
fn test_create_graph_snapshot_record_matches_oracle() {
    let expected = &oracle()["graph_snapshot_record"];
    let custom = create_graph_snapshot_record(
        "v-1",
        "http://ex.org/g/1",
        None,
        None,
        Some(obj(json!({"pipeline": "p"}))),
    );
    let default = create_graph_snapshot_record("v-2", "http://ex.org/g/2", None, None, None);
    for (record, expected) in [
        (&custom, &expected["custom"]),
        (&default, &expected["default"]),
    ] {
        let keys: Vec<&String> = record.keys().collect();
        assert_eq!(json!(keys), expected["keys"]);
        assert_eq!(
            mask(&Value::Object(record.clone()), &["timestamp", "checksum"]),
            expected["masked"]
        );
        assert_eq!(json!(verify_checksum(record)), expected["valid"]);
    }
    let falsy =
        create_graph_snapshot_record("v-3", "g", Some("me"), Some("desc"), Some(Map::new()));
    assert_eq!(
        mask(&Value::Object(falsy), &["timestamp", "checksum"]),
        expected["falsy_metadata"]
    );
}

/// The oracle's `storage_scenarios(storage)`.
fn storage_scenarios(storage: &dyn VersionStorage) -> Map<String, Value> {
    let mut out = Map::new();
    let snapshots = [
        json!({"label": "legacy", "timestamp": "2024-01-01T00:00:00", "author": "a@b.co", "description": "d",
               "checksum": "c", "entities": [{"id": 1}, {"id": 2}], "relationships": [{"source": 1, "target": 2}]}),
        json!({"label": "nodes", "timestamp": "2024-01-02T00:00:00", "nodes": [{"id": 1}], "edges": [],
               "graph_uri": "urn:g"}),
        json!({"label": "mixed", "timestamp": "2024-01-03T00:00:00", "entities": null, "nodes": [1, 2, 3],
               "relationships": [], "edges": [1], "version_id": "vid-3"}),
        json!({"label": "strings", "timestamp": "2024-01-04T00:00:00", "entities": "abcd",
               "relationships": {"x": 1, "y": 2}}),
        json!({"label": "bare", "timestamp": "2024-01-05T00:00:00"}),
        json!({"label": "nulls", "timestamp": "2024-01-06T00:00:00", "entities": 0, "nodes": [9],
               "relationships": "", "edges": [1, 2], "version_id": null}),
    ];
    for s in &snapshots {
        storage.save(s.as_object().unwrap()).unwrap();
    }
    out.insert("list_all".into(), json!(storage.list_all().unwrap()));
    out.insert("get_strings".into(), json!(storage.get("strings").unwrap()));
    out.insert("get_missing".into(), json!(storage.get("missing").unwrap()));
    out.insert(
        "exists".into(),
        json!([
            storage.exists("legacy").unwrap(),
            storage.exists("nope").unwrap()
        ]),
    );
    let save = |v: Value| outcome(storage.save(v.as_object().unwrap()));
    out.insert(
        "errors".into(),
        json!({
            "no_label": save(json!({"timestamp": "x"})),
            "empty_label": save(json!({"label": ""})),
            "zero_label": save(json!({"label": 0})),
            "duplicate": save(json!({"label": "legacy"})),
            "list_label": save(json!({"label": ["l"]})),
        }),
    );
    out.insert(
        "delete".into(),
        json!([
            storage.delete("bare").unwrap(),
            storage.delete("bare").unwrap()
        ]),
    );
    let labels: Vec<Value> = storage
        .list_all()
        .unwrap()
        .iter()
        .map(|m| m["label"].clone())
        .collect();
    out.insert("after_delete".into(), Value::Array(labels));
    storage.save_tag("prod", "legacy").unwrap();
    storage.save_tag("dev", "nodes").unwrap();
    storage.save_tag("prod", "mixed").unwrap();
    out.insert(
        "tags".into(),
        json!({
            "list": storage.list_tags().unwrap(),
            "get": [storage.get_tag("prod").unwrap(), storage.get_tag("nope").unwrap()],
        }),
    );
    let mutation = |v: Value| storage.save_mutation(v.as_object().unwrap()).unwrap();
    mutation(
        json!({"timestamp": "t1", "operation": "ADD_NODE", "entity_id": "n1", "payload": {"a": 1}}),
    );
    mutation(
        json!({"timestamp": "t2", "operation": "UPDATE_NODE", "entity_id": "n1", "payload": {"a": 2},
                    "version_label": "pre"}),
    );
    mutation(json!({"timestamp": "t3", "operation": "ADD_NODE", "entity_id": "n2"}));
    out.insert(
        "history_before".into(),
        json!(storage.get_entity_history("n1").unwrap()),
    );
    storage.assign_version_to_unlabeled_mutations("v1").unwrap();
    mutation(
        json!({"timestamp": "t4", "operation": "REMOVE_NODE", "entity_id": "n1", "payload": {}}),
    );
    out.insert(
        "history_after".into(),
        json!(storage.get_entity_history("n1").unwrap()),
    );
    out.insert(
        "history_n2".into(),
        json!(storage.get_entity_history("n2").unwrap()),
    );
    out.insert(
        "history_none".into(),
        json!(storage.get_entity_history("zzz").unwrap()),
    );
    out
}

fn assert_scenarios(actual: &Map<String, Value>, expected: &Value) {
    for (key, value) in actual {
        assert_eq!(value, &expected[key], "{key}");
    }
}

#[test]
fn test_in_memory_storage_matches_oracle() {
    let storage = InMemoryVersionStorage::new();
    let actual = storage_scenarios(&storage);
    assert_scenarios(&actual, &oracle()["in_memory_storage"]);
}

#[test]
fn test_sqlite_storage_matches_oracle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scenarios.db");
    let storage = SQLiteVersionStorage::new(&path).unwrap();
    assert_eq!(storage.storage_path, path);
    let actual = storage_scenarios(&storage);
    let expected = &oracle()["sqlite_storage"];
    assert_scenarios(&actual, expected);

    storage
        .save(&obj(json!({"label": "typed", "timestamp": true, "author": 5, "description": 1.5, "checksum": "c"})))
        .unwrap();
    let typed: Vec<Map<String, Value>> = storage
        .list_all()
        .unwrap()
        .into_iter()
        .filter(|m| m["label"] == "typed")
        .collect();
    assert_eq!(json!(typed), expected["typed_list"]);

    let save = |v: Value| outcome(storage.save(v.as_object().unwrap()));
    let big =
        serde_json::from_str::<Value>(r#"{"label": "b4", "timestamp": 18446744073709551615}"#)
            .unwrap();
    let errors = json!({
        "dict_timestamp": save(json!({"label": "b1", "timestamp": {"x": 1}})),
        "list_author": save(json!({"label": "b2", "author": [1]})),
        "null_author": save(json!({"label": "b3", "author": null})),
        "dict_mutation": outcome(storage.save_mutation(&obj(json!({"timestamp": {"a": 1}})))),
    });
    for key in [
        "dict_timestamp",
        "list_author",
        "null_author",
        "dict_mutation",
    ] {
        assert_eq!(errors[key], expected["binding_errors"][key], "{key}");
    }
    // Python's 2**70 and the port's u64::MAX both exceed SQLite INTEGER.
    assert_eq!(save(big), expected["binding_errors"]["big_int"]);

    storage
        .save_mutation(&obj(
            json!({"entity_id": 5, "payload": null, "operation": 7, "timestamp": 1.5}),
        ))
        .unwrap();
    assert_eq!(
        json!(storage.get_entity_history("5").unwrap()),
        expected["numeric_mutation"]
    );

    // Schema and on-disk rows are the original's, byte for byte.
    let conn = rusqlite::Connection::open(&path).unwrap();
    let rows = |sql: &str| -> Value {
        let mut stmt = conn.prepare(sql).unwrap();
        let count = stmt.column_count();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                let mut row = Vec::new();
                for i in 0..count {
                    row.push(match r.get::<_, rusqlite::types::Value>(i)? {
                        rusqlite::types::Value::Null => Value::Null,
                        rusqlite::types::Value::Integer(n) => json!(n),
                        rusqlite::types::Value::Real(f) => json!(f),
                        rusqlite::types::Value::Text(s) => json!(s),
                        rusqlite::types::Value::Blob(_) => json!("<blob>"),
                    });
                }
                Ok(Value::Array(row))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        Value::Array(rows)
    };
    assert_eq!(
        rows("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY name"),
        oracle()["sqlite_schema"]
    );
    assert_eq!(
        rows(
            "SELECT label, timestamp, author, description, checksum, snapshot_data FROM versions ORDER BY label"
        ),
        oracle()["sqlite_rows"]
    );
    assert_eq!(
        rows(
            "SELECT id, timestamp, operation, entity_id, payload, version_label FROM mutation_log ORDER BY id"
        ),
        oracle()["sqlite_mutation_rows"]
    );
}

#[test]
fn test_sqlite_storage_reads_database_written_by_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("compat.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let dump: Vec<&str> = oracle()["compat_db_dump"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        conn.execute_batch(&dump.join("\n")).unwrap();
    }
    let storage = SQLiteVersionStorage::new(&path).unwrap();
    let expected = &oracle()["compat_expected"];
    assert_eq!(json!(storage.get("py-v1").unwrap()), expected["get"]);
    assert_eq!(json!(storage.list_all().unwrap()), expected["list_all"]);
    assert_eq!(json!(storage.list_tags().unwrap()), expected["tags"]);
    assert_eq!(
        json!(storage.get_entity_history("e\u{e9}").unwrap()),
        expected["history"]
    );
    assert_eq!(
        storage.get_tag("release").unwrap().as_deref(),
        Some("py-v1")
    );
    assert!(verify_checksum(&storage.get("py-v1").unwrap().unwrap()));
}

#[test]
fn test_sqlite_storage_errors_are_python_exceptions() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the database file should be: sqlite3.OperationalError.
    let err = SQLiteVersionStorage::new(dir.path()).unwrap_err();
    assert_eq!(err.type_name(), "OperationalError");
    assert_eq!(err.module_name(), "sqlite3");
    assert_eq!(err.to_string(), "unable to open database file");
    // A file in place of the parent directory: FileExistsError from mkdir(parents=True).
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let err = SQLiteVersionStorage::new(blocker.join("db.sqlite")).unwrap_err();
    assert_eq!(err.type_name(), "FileExistsError");
    // A database file that is not SQLite: sqlite3.DatabaseError from _init_database.
    let bogus = dir.path().join("bogus.db");
    std::fs::write(&bogus, vec![b'x'; 4096]).unwrap();
    let err = SQLiteVersionStorage::new(&bogus).unwrap_err();
    assert_eq!(err.type_name(), "DatabaseError");
    assert_eq!(err.to_string(), "file is not a database");
    // Corrupt JSON in a row: json.JSONDecodeError (raised outside the sqlite3 handler).
    let path = dir.path().join("json.db");
    let storage = SQLiteVersionStorage::new(&path).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO versions VALUES ('bad', 't', 'a', 'd', 'c', '{not json', 'now')",
            [],
        )
        .unwrap();
    assert_eq!(
        storage.get("bad").unwrap_err().type_name(),
        "JSONDecodeError"
    );
    // The table removed behind the storage's back: ProcessingError wrapping the sqlite3 error.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute("DROP TABLE versions", [])
        .unwrap();
    let err = storage.get("bad").unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
    assert_eq!(
        err.to_string(),
        "Failed to retrieve version from database: no such table: versions"
    );
    assert_eq!(
        storage.list_all().unwrap_err().to_string(),
        "Failed to list versions from database: no such table: versions"
    );
    assert_eq!(
        storage.exists("x").unwrap_err().to_string(),
        "Failed to check version existence: no such table: versions"
    );
    assert_eq!(
        storage.delete("x").unwrap_err().to_string(),
        "Failed to delete version from database: no such table: versions"
    );
    assert_eq!(
        storage
            .save(&obj(json!({"label": "x"})))
            .unwrap_err()
            .to_string(),
        "Failed to save version to database: no such table: versions"
    );
}

#[test]
fn test_in_memory_storage_isolates_mutations_and_tags() {
    let storage = InMemoryVersionStorage::default();
    let mut mutation = obj(json!({"entity_id": "n", "payload": {"k": [1]}}));
    storage.save_mutation(&mutation).unwrap();
    mutation.insert("entity_id".into(), json!("changed"));
    let history = storage.get_entity_history("n").unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["payload"], json!({"k": [1]}));
    assert_eq!(storage.get_tag("none").unwrap(), None);
    storage.save_tag("t", "v").unwrap();
    assert_eq!(
        storage.list_tags().unwrap().get("t").map(String::as_str),
        Some("v")
    );
}

/// The abstract `VersionStorage` interface, exercised through a trait object for both backends.
#[test]
fn test_version_storage_trait_contract() {
    let dir = tempfile::tempdir().unwrap();
    let backends: Vec<Box<dyn VersionStorage>> = vec![
        Box::new(InMemoryVersionStorage::new()),
        Box::new(SQLiteVersionStorage::new(dir.path().join("contract.db")).unwrap()),
    ];
    for storage in &backends {
        let snapshot =
            obj(json!({"label": "v1", "timestamp": "2024-01-01T00:00:00", "nodes": [{"id": "a"}]}));
        storage.save(&snapshot).unwrap();
        assert_eq!(storage.get("v1").unwrap(), Some(snapshot.clone()));
        assert_eq!(storage.list_all().unwrap()[0]["entity_count"], 1);
        assert!(storage.exists("v1").unwrap());
        storage.save_tag("stable", "v1").unwrap();
        assert_eq!(storage.get_tag("stable").unwrap().as_deref(), Some("v1"));
        assert_eq!(storage.list_tags().unwrap().len(), 1);
        storage
            .save_mutation(&obj(
                json!({"timestamp": "t", "operation": "ADD_NODE", "entity_id": "a", "payload": {}}),
            ))
            .unwrap();
        storage.assign_version_to_unlabeled_mutations("v1").unwrap();
        assert_eq!(
            storage.get_entity_history("a").unwrap()[0]["version_label"],
            "v1"
        );
        assert!(storage.delete("v1").unwrap());
        assert!(!storage.exists("v1").unwrap());
    }
}
