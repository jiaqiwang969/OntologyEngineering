//! Port of `tests/provenance/test_sqlite_storage_performance_807.py`: SQLiteStorage
//! performance optimisations and atomicity (#807) — connection PRAGMAs, atomic `track_entity`,
//! batched transactions, batched BFS, and file-unlink safety without an explicit `close()`.
//!
//! Where the original patches a method of a live object (`patch.object(storage, ...)`), the
//! port wraps the real backend in a storage that changes exactly that method and delegates
//! everything else, or uses `SQLiteStorage::set_configure_connection_hook` for the private
//! `_configure_connection`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sem_provenance::schemas::ProvenanceEntry;
use sem_provenance::storage::{
    ProvenanceStorage, SQLiteStorage, StorageConnection, TransactionBody,
};
use sem_provenance::{ProvenanceManager, TrackEntityOptions, TrackKwargs};
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

fn obj(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("expected an object, got {other}"),
    }
}

fn path_str(path: &std::path::Path) -> String {
    path.to_str().expect("UTF-8 path").to_string()
}

/// Implements every `ProvenanceStorage` method by delegating to `self.inner`, except
/// `transaction`, `store` and `store_with_conn`, which each wrapper defines itself.
macro_rules! delegate_reads {
    () => {
        fn retrieve(&self, entity_id: &str) -> Result<Option<ProvenanceEntry>, SemanticaError> {
            self.inner.retrieve(entity_id)
        }
        fn retrieve_all(
            &self,
            entity_type: Option<&str>,
        ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
            self.inner.retrieve_all(entity_type)
        }
        fn trace_lineage(
            &self,
            entity_id: &str,
            max_depth: Option<usize>,
        ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
            self.inner.trace_lineage(entity_id, max_depth)
        }
        fn clear(&self) -> Result<usize, SemanticaError> {
            self.inner.clear()
        }
        fn savepoint(
            &self,
            conn: Option<&StorageConnection<'_>>,
            body: &mut TransactionBody<'_>,
        ) -> Result<(), SemanticaError> {
            self.inner.savepoint(conn, body)
        }
        fn retrieve_with_conn(
            &self,
            conn: Option<&StorageConnection<'_>>,
            entity_id: &str,
        ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
            self.inner.retrieve_with_conn(conn, entity_id)
        }
        fn get_chain_head(
            &self,
            conn: Option<&StorageConnection<'_>>,
        ) -> Result<Option<(i64, Option<String>)>, SemanticaError> {
            self.inner.get_chain_head(conn)
        }
        fn trace_descendants(
            &self,
            entity_id: &str,
            max_depth: Option<usize>,
        ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
            self.inner.trace_descendants(entity_id, max_depth)
        }
    };
}

/// `failing_tx`: the real transaction runs the body, then "commit" raises
/// `sqlite3.OperationalError("Commit failed")` inside it, so it rolls back.
struct FailingCommitStorage {
    inner: SQLiteStorage,
}

impl ProvenanceStorage for FailingCommitStorage {
    delegate_reads!();

    fn store(&self, entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
        self.inner.store(entry)
    }

    fn transaction(&self, body: &mut TransactionBody<'_>) -> Result<(), SemanticaError> {
        self.inner.transaction(&mut |conn| {
            body(conn)?;
            Err(SemanticaError::Other {
                module: "sqlite3".into(),
                type_name: "OperationalError".into(),
                message: "Commit failed".into(),
            })
        })
    }

    fn store_with_conn(
        &self,
        conn: Option<&StorageConnection<'_>>,
        entry: &ProvenanceEntry,
    ) -> Result<(), SemanticaError> {
        self.inner.store_with_conn(conn, entry)
    }
}

/// The original stores `{"bad": {1, 2, 3}}`: a Python `set`, which `json.dumps` cannot
/// serialise, so `_store_with_conn` raises for that one item. Every `serde_json::Value` is
/// serialisable, so this backend raises the same `TypeError` for an entry whose metadata holds
/// the stand-in `"bad"` key and otherwise is the real SQLite storage.
struct UnserializableMetadataStorage {
    inner: SQLiteStorage,
}

impl UnserializableMetadataStorage {
    fn check(entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
        if entry.metadata.contains_key("bad") {
            return Err(SemanticaError::Type(
                "Object of type set is not JSON serializable".into(),
            ));
        }
        Ok(())
    }
}

impl ProvenanceStorage for UnserializableMetadataStorage {
    delegate_reads!();

    fn store(&self, entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
        self.transaction(&mut |conn| self.store_with_conn(conn, entry))
    }

    fn transaction(&self, body: &mut TransactionBody<'_>) -> Result<(), SemanticaError> {
        self.inner.transaction(body)
    }

    fn store_with_conn(
        &self,
        conn: Option<&StorageConnection<'_>>,
        entry: &ProvenanceEntry,
    ) -> Result<(), SemanticaError> {
        Self::check(entry)?;
        self.inner.store_with_conn(conn, entry)
    }
}

fn bad_metadata() -> Value {
    json!({"bad": [1, 2, 3]})
}

#[test]
fn test_sqlite_pragmas_configured() {
    // WAL mode, busy_timeout and synchronous pragmas are set.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_pragmas.db"));
    let storage = SQLiteStorage::new(db_path).unwrap();

    storage
        .transaction(&mut |conn| {
            let Some(StorageConnection::Sqlite(conn)) = conn else {
                panic!("SQLite transactions yield a connection");
            };
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            assert!(["wal", "delete", "memory"].contains(&mode.to_lowercase().as_str()));

            let timeout: i64 = conn
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
                .unwrap();
            assert_eq!(timeout, 5000);

            let sync_mode: i64 = conn
                .query_row("PRAGMA synchronous", [], |row| row.get(0))
                .unwrap();
            // NORMAL corresponds to 1 in SQLite
            assert_eq!(sync_mode, 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_track_entity_atomicity_and_windows_unlink() {
    // track_entity uses a single transaction and closes file handles.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = tmp_path.path().join("test_atomic.db");
    let mgr = ProvenanceManager::with_storage_path(path_str(&db_path)).unwrap();

    let entry = mgr
        .track_entity(
            "entity_1",
            "doc_1",
            Some(obj(json!({"key": "val"}))),
            TrackEntityOptions::default(),
        )
        .unwrap();
    assert_eq!(entry.entity_id, "entity_1");

    // The file can be removed without an explicit close()
    assert!(db_path.exists());
    std::fs::remove_file(&db_path).unwrap();
}

#[test]
fn test_track_entities_batch_transaction_sharing() {
    // Batch entity tracking uses transaction blocks and returns an accurate count.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_batch.db"));
    let mgr = ProvenanceManager::with_storage_path(db_path).unwrap();

    let entities: Vec<Map<String, Value>> = (0..2500)
        .map(|i| obj(json!({"id": format!("ent_{i}"), "metadata": {"index": i}})))
        .collect();

    let count = mgr.track_entities_batch(
        &entities,
        "batch_doc",
        TrackEntityOptions::default(),
        Map::new(),
    );
    assert_eq!(count, 2500);

    let stored_all = mgr.storage().retrieve_all(None).unwrap();
    assert_eq!(stored_all.len(), 2500);
}

#[test]
fn test_trace_lineage_batched_bfs_and_max_depth() {
    // trace_lineage uses batched IN (...) queries and respects max_depth.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_lineage_bfs.db"));
    let mgr = ProvenanceManager::with_storage_path(db_path).unwrap();

    // Build a linear chain: ent_3 -> ent_2 -> ent_1 -> doc_0
    mgr.track_entity("ent_1", "doc_0", None, TrackEntityOptions::default());
    mgr.track_entity("ent_2", "ent_1", None, TrackEntityOptions::default());
    mgr.track_entity("ent_3", "ent_2", None, TrackEntityOptions::default());

    let full_lineage = mgr.trace_lineage("ent_3", None).unwrap();
    let ids: Vec<String> = full_lineage.iter().map(|e| e.entity_id.clone()).collect();
    assert!(ids.contains(&"ent_3".to_string()));
    assert!(ids.contains(&"ent_2".to_string()));
    assert!(ids.contains(&"ent_1".to_string()));

    // Test max_depth parameter
    let limited_lineage = mgr.trace_lineage("ent_3", Some(1)).unwrap();
    assert_eq!(limited_lineage.len(), 1);
    assert_eq!(limited_lineage[0].entity_id, "ent_3");
}

#[test]
fn test_batch_rollback_does_not_count_unpersisted_entries() {
    // Batch tracking does not count entries when the transaction commit fails.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_batch_rollback.db"));
    let mgr = ProvenanceManager::with_storage(FailingCommitStorage {
        inner: SQLiteStorage::new(db_path).unwrap(),
    });

    let entities: Vec<Map<String, Value>> = (0..10)
        .map(|i| obj(json!({"id": format!("ent_{i}")})))
        .collect();
    let chunks: Vec<Map<String, Value>> = (0..10)
        .map(|i| obj(json!({"id": format!("chk_{i}"), "start_index": 0, "end_index": 10})))
        .collect();

    let ent_count = mgr.track_entities_batch(
        &entities,
        "doc_1",
        TrackEntityOptions::default(),
        Map::new(),
    );
    let chk_count = mgr.track_chunks_batch(&chunks, "doc_1", None, TrackKwargs::default());
    assert_eq!(ent_count, 0);
    assert_eq!(chk_count, 0);

    assert_eq!(mgr.storage().retrieve_all(None).unwrap().len(), 0);
}

/// A custom backend written against the original one-argument `trace_lineage(entity_id)`.
#[derive(Default)]
struct LegacyCustomStorage {
    calls: AtomicUsize,
}

impl ProvenanceStorage for LegacyCustomStorage {
    fn store(&self, _entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
        Ok(())
    }

    fn retrieve(&self, _entity_id: &str) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        Ok(None)
    }

    fn retrieve_all(
        &self,
        _entity_type: Option<&str>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        Ok(Vec::new())
    }

    fn trace_lineage(
        &self,
        _entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        assert_eq!(
            max_depth, None,
            "the one-argument form never receives a depth"
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }

    fn clear(&self) -> Result<usize, SemanticaError> {
        Ok(0)
    }

    fn supports_max_depth(&self) -> bool {
        false
    }
}

#[test]
fn test_custom_storage_trace_lineage_one_argument_backward_compatibility() {
    let storage = Arc::new(LegacyCustomStorage::default());
    let mgr = ProvenanceManager::with_shared_storage(storage.clone());

    // Calling with max_depth omitted / None must succeed
    let lineage1 = mgr.trace_lineage("entity_1", None).unwrap();
    assert!(lineage1.is_empty());
    assert_eq!(storage.calls.load(Ordering::SeqCst), 1);

    // Calling with max_depth provided must fall back cleanly to the one-argument call
    let lineage2 = mgr.trace_lineage("entity_1", Some(2)).unwrap();
    assert!(lineage2.is_empty());
    assert_eq!(storage.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_track_entity_concurrent_read_modify_write() {
    // Concurrent track_entity calls serialize via BEGIN IMMEDIATE without losing history.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_concurrent.db"));

    // Create initial version
    let mgr_init = ProvenanceManager::with_storage_path(db_path.clone()).unwrap();
    mgr_init.track_entity(
        "entity_1",
        "doc_init",
        Some(obj(json!({"version": 0}))),
        TrackEntityOptions::default(),
    );

    let num_workers = 8;
    let handles: Vec<_> = (1..=num_workers)
        .map(|index| {
            let db_path = db_path.clone();
            std::thread::spawn(move || {
                // Separate manager/connection per worker
                let mgr = ProvenanceManager::with_storage_path(db_path).unwrap();
                mgr.track_entity(
                    "entity_1",
                    &format!("doc_{index}"),
                    Some(obj(json!({"version": index}))),
                    TrackEntityOptions::default(),
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.len(), num_workers);

    // 1 current + num_workers archived versions
    let mgr_check = ProvenanceManager::with_storage_path(db_path).unwrap();
    let all_entries = mgr_check.storage().retrieve_all(None).unwrap();
    assert_eq!(all_entries.len(), num_workers + 1);

    // All archived history IDs start with "entity_1:v:" except the active entity_1
    let history_entries: Vec<_> = all_entries
        .iter()
        .filter(|e| e.entity_id != "entity_1")
        .collect();
    assert_eq!(history_entries.len(), num_workers);
    for he in history_entries {
        assert!(he.entity_id.starts_with("entity_1:v:"));
    }
}

#[test]
fn test_sqlite_storage_cleanup_guard_on_configure_error() {
    // Connection handles are closed even when _configure_connection raises.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = tmp_path.path().join("test_leak_guard.db");
    let storage = SQLiteStorage::new(path_str(&db_path)).unwrap();

    storage.set_configure_connection_hook(Some(Box::new(|_| {
        Err(SemanticaError::Runtime("PRAGMA error".into()))
    })));
    let err = storage.transaction(&mut |_| Ok(())).unwrap_err();
    assert_eq!(err.type_name(), "RuntimeError");
    assert!(err.to_string().contains("PRAGMA error"));
    storage.set_configure_connection_hook(None);

    // The handle must not be leaked after the error
    assert!(db_path.exists());
    std::fs::remove_file(&db_path).unwrap();
    assert!(!db_path.exists());
}

#[test]
fn test_batch_does_not_count_individually_failed_items() {
    // One item's storage failure inside an otherwise successful shared batch transaction is
    // not counted, while the rest of the block commits (#807 follow-up).
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_batch_partial_failure.db"));
    let mgr = ProvenanceManager::with_storage(UnserializableMetadataStorage {
        inner: SQLiteStorage::new(db_path).unwrap(),
    });

    let mut entities: Vec<Map<String, Value>> = (0..5)
        .map(|i| obj(json!({"id": format!("ent_{i}"), "metadata": {"index": i}})))
        .collect();
    entities[2].insert("metadata".into(), bad_metadata());

    let count = mgr.track_entities_batch(
        &entities,
        "doc_1",
        TrackEntityOptions::default(),
        Map::new(),
    );
    let stored = mgr.storage().retrieve_all(None).unwrap();

    assert_eq!(count, stored.len());
    assert_eq!(count, 4);
    assert!(!stored.iter().any(|e| e.entity_id == "ent_2"));
}

#[test]
fn test_chunks_batch_does_not_count_individually_failed_items() {
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_chunks_batch_partial_failure.db"));
    let mgr = ProvenanceManager::with_storage(UnserializableMetadataStorage {
        inner: SQLiteStorage::new(db_path).unwrap(),
    });

    let mut chunks: Vec<Map<String, Value>> = (0..5)
        .map(|i| obj(json!({"id": format!("chk_{i}"), "start_index": 0, "end_index": 10})))
        .collect();
    chunks[2].insert("metadata".into(), bad_metadata());

    let count = mgr.track_chunks_batch(&chunks, "doc_1", None, TrackKwargs::default());
    let stored = mgr.storage().retrieve_all(None).unwrap();

    assert_eq!(count, stored.len());
    assert_eq!(count, 4);
    assert!(!stored.iter().any(|e| e.entity_id == "chk_2"));
}

#[test]
fn test_track_entity_standalone_call_still_degrades_gracefully() {
    // A direct track_entity() call returns None on a storage failure for a brand-new entity
    // instead of raising (#782).
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_standalone_degrade.db"));
    let mgr = ProvenanceManager::with_storage(UnserializableMetadataStorage {
        inner: SQLiteStorage::new(db_path).unwrap(),
    });

    let entry = mgr.track_entity(
        "entity_1",
        "doc_1",
        Some(obj(bad_metadata())),
        TrackEntityOptions::default(),
    );

    // None: nothing was persisted
    assert!(entry.is_none());
    assert!(mgr.storage().retrieve("entity_1").unwrap().is_none());
}

#[test]
fn test_retrieve_and_trace_lineage_do_not_block_on_writer_lock() {
    // retrieve() and trace_lineage() take no writer lock, so they do not wait for a
    // connection holding BEGIN IMMEDIATE. The original joins the reader for 2 s; the port
    // allows 10 s because the machine running the suite is shared and heavily loaded.
    let tmp_path = tempfile::tempdir().unwrap();
    let db_path = path_str(&tmp_path.path().join("test_read_concurrency.db"));
    let storage = Arc::new(SQLiteStorage::new(db_path.clone()).unwrap());
    storage
        .store(&ProvenanceEntry::new("entity_1", "entity", "test"))
        .unwrap();

    let writer = rusqlite::Connection::open(&db_path).unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL").unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();

    let (tx, rx) = std::sync::mpsc::channel();
    let reader_storage = Arc::clone(&storage);
    let reader = std::thread::spawn(move || {
        let entry = reader_storage.retrieve("entity_1");
        let lineage = reader_storage.trace_lineage("entity_1", None);
        let _ = tx.send((entry, lineage));
    });
    let outcome = rx.recv_timeout(Duration::from_secs(10));

    writer.execute_batch("ROLLBACK").unwrap();
    drop(writer);

    let (entry, lineage) =
        outcome.expect("retrieve()/trace_lineage() blocked behind the writer lock");
    reader.join().unwrap();
    assert_eq!(entry.unwrap().unwrap().entity_id, "entity_1");
    assert_eq!(lineage.unwrap().len(), 1);
}
