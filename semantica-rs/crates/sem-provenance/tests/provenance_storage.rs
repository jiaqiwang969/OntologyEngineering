//! Characterisation tests for `storage.rs` against the oracle (`fixtures/oracle.json`, section
//! `storage`): chain heads, BFS orders, SQLite rows and schema migration.

mod common;

use common::{assert_json_eq, dicts, fixed_clock, ids, obj, oracle};
use sem_provenance::schemas::ProvenanceEntry;
use sem_provenance::storage::{
    InMemoryStorage, ProvenanceStorage, ProvenanceStorageExt, SQLiteStorage, StorageConnection,
};
use serde_json::{Map, Value, json};

fn section(key: &str) -> &'static Value {
    &oracle("storage")[key]
}

fn head_json(head: Option<(i64, Option<String>)>) -> Value {
    match head {
        None => Value::Null,
        Some((seq, checksum)) => json!([seq, checksum]),
    }
}

fn sql_rows(conn: &rusqlite::Connection, sql: &str) -> Value {
    let mut stmt = conn.prepare(sql).unwrap();
    let count = stmt.column_count();
    let rows = stmt
        .query_map([], |row| {
            let mut values = Vec::with_capacity(count);
            for i in 0..count {
                values.push(match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Real(f) => json!(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        json!(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => json!(b),
                });
            }
            Ok(Value::Array(values))
        })
        .unwrap();
    Value::Array(rows.map(Result::unwrap).collect())
}

fn entry(id: &str, kind: &str, act: &str) -> ProvenanceEntry {
    ProvenanceEntry::new(id, kind, act)
}

#[test]
fn in_memory_chain_head_matches_oracle() {
    let _clock = fixed_clock();
    let s = InMemoryStorage::new();
    assert_eq!(
        head_json(s.get_chain_head(None).unwrap()),
        *section("in_memory_chain_head_empty")
    );
    s.store(&ProvenanceEntry {
        sequence_id: Some(3),
        checksum: Some("c3".into()),
        ..entry("e1", "entity", "act")
    })
    .unwrap();
    s.store(&ProvenanceEntry {
        sequence_id: Some(1),
        checksum: Some("c1".into()),
        ..entry("e2", "entity", "act")
    })
    .unwrap();
    assert_eq!(
        head_json(s.get_chain_head(None).unwrap()),
        *section("in_memory_chain_head_lower_seq")
    );
    s.store(&entry("e3", "entity", "act")).unwrap();
    assert_eq!(
        head_json(s.get_chain_head(None).unwrap()),
        *section("in_memory_chain_head_unchained")
    );
}

fn dag_storage() -> InMemoryStorage {
    let s = InMemoryStorage::new();
    let dag: [(&str, Option<&str>, &[&str]); 6] = [
        ("root", None, &[]),
        ("a", Some("root"), &[]),
        ("b", Some("root"), &["a"]),
        ("c", Some("b"), &["a", "root", "missing"]),
        ("d", Some("c"), &["b"]),
        ("e", None, &["d", "c"]),
    ];
    for (id, parent, used) in dag {
        s.store(&ProvenanceEntry {
            parent_entity_id: parent.map(str::to_string),
            used_entities: used.iter().map(|u| u.to_string()).collect(),
            ..entry(id, "entity", "act")
        })
        .unwrap();
    }
    s.store(&ProvenanceEntry {
        previous_version_id: Some("a".into()),
        derived_from_id: Some("e".into()),
        ..entry("v2", "entity", "act")
    })
    .unwrap();
    s
}

#[test]
fn in_memory_lineage_and_descendant_orders_match_oracle() {
    let _clock = fixed_clock();
    let s = dag_storage();
    let lineage = section("in_memory_lineage");
    let trace = |id: &str, depth: Option<usize>| json!(ids(&s.trace_lineage(id, depth).unwrap()));
    assert_eq!(trace("e", None), lineage["e"]);
    assert_eq!(trace("e", Some(1)), lineage["e_depth_1"]);
    assert_eq!(trace("e", Some(2)), lineage["e_depth_2"]);
    assert_eq!(trace("e", Some(0)), lineage["e_depth_0"]);
    assert_eq!(trace("missing", None), lineage["missing"]);
    assert_eq!(trace("d", None), lineage["d"]);

    let descendants = section("in_memory_descendants");
    let down =
        |id: &str, depth: Option<usize>| json!(ids(&s.trace_descendants(id, depth).unwrap()));
    assert_eq!(down("root", None), descendants["root"]);
    assert_eq!(down("a", None), descendants["a"]);
    assert_eq!(down("a", Some(1)), descendants["a_depth_1"]);
    assert_eq!(down("e", None), descendants["e"]);
    assert_eq!(down("v2", None), descendants["leaf"]);

    assert_eq!(
        json!(ids(&s.retrieve_all(Some("entity")).unwrap())),
        *section("in_memory_retrieve_all_entity")
    );
    assert_eq!(
        json!(ids(&s.retrieve_all(Some("")).unwrap())),
        *section("in_memory_retrieve_all_empty_filter")
    );
    assert_eq!(json!(s.clear().unwrap()), *section("in_memory_clear"));
}

#[test]
fn sqlite_rows_orders_and_chain_head_match_oracle() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("storage.db");
    let path_str = path.to_str().unwrap().to_string();
    let _clock = fixed_clock();
    let st = SQLiteStorage::new(path_str.clone()).unwrap();
    st.store(&ProvenanceEntry {
        metadata: obj(json!({"k": "v", "n": 1.5, "u": "\u{e9}", "l": [1, null]})),
        used_entities: vec!["x".into(), "y".into()],
        informed_by_activities: vec!["i".into()],
        sequence_id: Some(1),
        checksum: Some("ca".into()),
        ..entry("a", "entity", "act")
    })
    .unwrap();
    st.store(&ProvenanceEntry {
        parent_entity_id: Some("a".into()),
        sequence_id: Some(2),
        checksum: Some("cb".into()),
        is_automated: false,
        invalidated: true,
        ..entry("b", "chunk", "act")
    })
    .unwrap();
    st.store(&ProvenanceEntry {
        sequence_id: Some(3),
        checksum: Some("ca2".into()),
        ..entry("a", "entity", "act2")
    })
    .unwrap();
    st.store(&ProvenanceEntry {
        used_entities: vec!["a".into()],
        derived_from_id: Some("b".into()),
        ..entry("c", "entity", "act")
    })
    .unwrap();

    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        assert_json_eq(
            &sql_rows(
                &conn,
                "SELECT entity_id, used_entities, metadata, informed_by_activities, is_automated, \
                 invalidated, confidence, version, sequence_id FROM provenance ORDER BY rowid",
            ),
            section("sqlite_raw_rows"),
        );
        let columns: Vec<Value> = sql_rows(&conn, "PRAGMA table_info(provenance)")
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row[1].clone())
            .collect();
        assert_eq!(Value::Array(columns), *section("sqlite_columns"));
        let mut indexes: Vec<String> = sql_rows(&conn, "PRAGMA index_list(provenance)")
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row[1].as_str().unwrap().to_string())
            .collect();
        indexes.sort();
        assert_eq!(json!(indexes), *section("sqlite_indexes"));
    }

    assert_json_eq(
        &dicts(&st.retrieve_all(None).unwrap()),
        section("sqlite_retrieve_all"),
    );
    assert_eq!(
        json!(ids(&st.retrieve_all(Some("chunk")).unwrap())),
        *section("sqlite_retrieve_all_chunk")
    );
    assert_eq!(
        head_json(st.get_chain_head(None).unwrap()),
        *section("sqlite_chain_head")
    );
    assert_eq!(
        json!(ids(&st.trace_lineage("c", None).unwrap())),
        *section("sqlite_lineage_c")
    );
    let mut down_a = ids(&st.trace_descendants("a", None).unwrap());
    down_a.sort();
    assert_eq!(json!(down_a), *section("sqlite_descendants_a"));
    let mut down_b = ids(&st.trace_descendants("b", None).unwrap());
    down_b.sort();
    assert_eq!(json!(down_b), *section("sqlite_descendants_b"));
    assert_eq!(json!(st.clear().unwrap()), *section("sqlite_clear"));
    assert_eq!(
        head_json(st.get_chain_head(None).unwrap()),
        *section("sqlite_chain_head_after_clear")
    );
    st.store(&ProvenanceEntry {
        sequence_id: Some(4),
        ..entry("n", "entity", "act")
    })
    .unwrap();
    assert_eq!(
        head_json(st.get_chain_head(None).unwrap()),
        *section("sqlite_chain_head_null_checksum")
    );
}

#[test]
fn sqlite_migrates_old_schema_like_oracle() {
    let tmp = tempfile::tempdir().unwrap();
    let old = tmp.path().join("old.db");
    {
        let conn = rusqlite::Connection::open(&old).unwrap();
        conn.execute_batch(
            "CREATE TABLE provenance (
            entity_id TEXT PRIMARY KEY, entity_type TEXT NOT NULL, activity_id TEXT NOT NULL,
            agent_id TEXT DEFAULT 'semantica', source_document TEXT, source_location TEXT,
            source_quote TEXT, timestamp TEXT NOT NULL, first_seen TEXT, last_updated TEXT,
            confidence REAL DEFAULT 1.0, checksum TEXT, parent_entity_id TEXT, used_entities TEXT,
            start_index INTEGER, end_index INTEGER, credibility REAL, metadata TEXT,
            version TEXT DEFAULT '1.0')",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO provenance VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                "old_entity",
                "entity",
                "legacy_activity",
                "semantica",
                None::<String>,
                None::<String>,
                None::<String>,
                "2025-01-01T00:00:00",
                "2025-01-01T00:00:00",
                "2025-01-01T00:00:00",
                1,
                "oldchecksum",
                None::<String>,
                None::<String>,
                4,
                8,
                0.25,
                None::<String>,
                "1.0"
            ],
        )
        .unwrap();
    }
    let migrated = SQLiteStorage::new(old.to_str().unwrap()).unwrap();
    let entry = migrated.retrieve("old_entity").unwrap().unwrap();
    assert_json_eq(&Value::Object(entry.to_dict()), section("migrated_entry"));
    let conn = rusqlite::Connection::open(&old).unwrap();
    let columns: Vec<Value> = sql_rows(&conn, "PRAGMA table_info(provenance)")
        .as_array()
        .unwrap()
        .iter()
        .map(|row| Value::Array(row.as_array().unwrap()[1..5].to_vec()))
        .collect();
    assert_json_eq(&Value::Array(columns), section("migrated_columns"));
}

#[test]
fn transactions_stage_and_roll_back_like_the_original() {
    // InMemoryStorage: staged entries are visible through the connection only, nested
    // savepoints merge into their parent, a failing level is discarded.
    let s = InMemoryStorage::new();
    let result: Result<(), _> = s.with_transaction(|conn| {
        assert!(matches!(conn, Some(StorageConnection::InMemory)));
        s.store_with_conn(conn, &entry("staged", "entity", "act"))?;
        assert!(s.retrieve("staged")?.is_none());
        assert!(s.retrieve_with_conn(conn, "staged")?.is_some());
        let inner: Result<(), _> = s.with_savepoint(conn, |sp| {
            s.store_with_conn(sp, &entry("discarded", "entity", "act"))?;
            Err(sem_utils::SemanticaError::Runtime("boom".into()))
        });
        assert!(inner.is_err());
        s.with_savepoint(conn, |sp| {
            s.store_with_conn(sp, &entry("kept", "entity", "act"))
        })?;
        Ok(())
    });
    result.unwrap();
    let stored: Vec<String> = ids(&s.retrieve_all(None).unwrap());
    assert_eq!(stored, vec!["staged".to_string(), "kept".to_string()]);

    let failed: Result<(), _> = s.with_transaction(|conn| {
        s.store_with_conn(conn, &entry("never", "entity", "act"))?;
        Err(sem_utils::SemanticaError::Runtime("rollback".into()))
    });
    assert!(failed.is_err());
    assert!(s.retrieve("never").unwrap().is_none());

    // SQLiteStorage: a failing savepoint is rolled back, the rest of the transaction commits.
    let tmp = tempfile::tempdir().unwrap();
    let db = SQLiteStorage::new(tmp.path().join("tx.db").to_str().unwrap()).unwrap();
    db.with_transaction(|conn| {
        db.store_with_conn(conn, &entry("one", "entity", "act"))?;
        let failed: Result<(), _> = db.with_savepoint(conn, |sp| {
            db.store_with_conn(sp, &entry("two", "entity", "act"))?;
            Err(sem_utils::SemanticaError::Runtime("item".into()))
        });
        assert!(failed.is_err());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        ids(&db.retrieve_all(None).unwrap()),
        vec!["one".to_string()]
    );
    assert_eq!(db.db_path(), tmp.path().join("tx.db").to_str().unwrap());
    assert_eq!(db.logger().name(), "semantica.sqlite_storage");

    // The default ProvenanceStorage methods (a backend that implements only the abstract ones).
    struct Minimal(InMemoryStorage);
    impl ProvenanceStorage for Minimal {
        fn store(&self, e: &ProvenanceEntry) -> Result<(), sem_utils::SemanticaError> {
            self.0.store(e)
        }
        fn retrieve(&self, id: &str) -> Result<Option<ProvenanceEntry>, sem_utils::SemanticaError> {
            self.0.retrieve(id)
        }
        fn retrieve_all(
            &self,
            t: Option<&str>,
        ) -> Result<Vec<ProvenanceEntry>, sem_utils::SemanticaError> {
            self.0.retrieve_all(t)
        }
        fn trace_lineage(
            &self,
            id: &str,
            d: Option<usize>,
        ) -> Result<Vec<ProvenanceEntry>, sem_utils::SemanticaError> {
            self.0.trace_lineage(id, d)
        }
        fn clear(&self) -> Result<usize, sem_utils::SemanticaError> {
            self.0.clear()
        }
    }
    let minimal = Minimal(dag_storage());
    assert_eq!(minimal.get_chain_head(None).unwrap(), None);
    minimal
        .transaction(&mut |conn| {
            assert!(conn.is_none());
            minimal.store_with_conn(conn, &entry("direct", "entity", "act"))?;
            minimal.savepoint(conn, &mut |sp| {
                assert!(sp.is_none());
                Ok(())
            })
        })
        .unwrap();
    assert!(
        minimal
            .retrieve_with_conn(None, "direct")
            .unwrap()
            .is_some()
    );
    assert!(minimal.supports_max_depth());
    let down: Vec<String> = ids(&minimal.trace_descendants("a", None).unwrap());
    assert_eq!(json!(down), oracle("storage")["in_memory_descendants"]["a"]);
    let dyn_storage: &dyn ProvenanceStorage = &minimal;
    assert!(dyn_storage.is::<Minimal>());
    assert!(dyn_storage.downcast_ref::<InMemoryStorage>().is_none());
    let _unused: Map<String, Value> = Map::new();
}
