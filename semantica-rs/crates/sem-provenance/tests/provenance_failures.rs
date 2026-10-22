//! Storage-failure behaviour of the manager: graceful degradation, transaction rollback,
//! per-item savepoints and error logging. The scenarios and expected outcomes are those of the
//! original `tests/provenance/test_manager.py` failure tests (all passing in the original
//! baseline), which patch `storage.store`, `storage._store_with_conn`,
//! `storage._retrieve_with_conn` or `storage.transaction`; here a wrapper backend changes the
//! same method and delegates the rest (the in-memory wrapper commits through itself with
//! `InMemoryStorage::transaction_via`, as Python's `self.store` dispatch does).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sem_provenance::schemas::{ProvenanceEntry, SourceReference};
use sem_provenance::storage::{
    InMemoryStorage, ProvenanceStorage, SQLiteStorage, StorageConnection, TransactionBody,
};
use sem_provenance::{ProvenanceManager, TrackChunkOptions, TrackEntityOptions, TrackKwargs};
use sem_utils::SemanticaError;
use sem_utils::logging::{Handler, get_logger};
use serde_json::{Map, Value, json};

fn te() -> TrackEntityOptions {
    TrackEntityOptions::default()
}

fn obj(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("expected an object, got {other}"),
    }
}

fn runtime(message: &str) -> SemanticaError {
    SemanticaError::Runtime(message.to_string())
}

/// Messages logged by `semantica.provenance_manager` that mention `needle`.
struct LogCapture {
    handler: Handler,
}

impl LogCapture {
    fn new() -> Self {
        let handler = Handler::capture();
        get_logger("provenance_manager").add_handler(handler.clone());
        LogCapture { handler }
    }

    fn matching(&self, needle: &str) -> Vec<String> {
        self.handler
            .records()
            .iter()
            .map(|record| record.message.clone())
            .filter(|message| message.contains(needle))
            .collect()
    }
}

impl Drop for LogCapture {
    fn drop(&mut self) {
        get_logger("provenance_manager").remove_handler(&self.handler);
    }
}

/// Which storage method a wrapper makes fail, and when.
#[derive(Default)]
struct Faults {
    store: bool,
    retrieve_with_conn: bool,
    store_with_conn_always: bool,
    /// Fail the n-th `store_with_conn` call (1-based; 0 = never).
    fail_on_call: AtomicUsize,
    store_with_conn_calls: AtomicUsize,
    store_calls: AtomicUsize,
    commit: bool,
}

struct Wrapped<S> {
    inner: S,
    faults: Faults,
}

macro_rules! wrapped_storage {
    ($inner:ty, $transaction:expr, $savepoint:expr) => {
        impl ProvenanceStorage for Wrapped<$inner> {
            fn store(&self, entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
                self.faults.store_calls.fetch_add(1, Ordering::SeqCst);
                if self.faults.store {
                    return Err(runtime("storage error"));
                }
                self.inner.store(entry)
            }
            fn retrieve(&self, id: &str) -> Result<Option<ProvenanceEntry>, SemanticaError> {
                self.inner.retrieve(id)
            }
            fn retrieve_all(
                &self,
                kind: Option<&str>,
            ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
                self.inner.retrieve_all(kind)
            }
            fn trace_lineage(
                &self,
                id: &str,
                depth: Option<usize>,
            ) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
                self.inner.trace_lineage(id, depth)
            }
            fn clear(&self) -> Result<usize, SemanticaError> {
                self.inner.clear()
            }
            fn transaction(&self, body: &mut TransactionBody<'_>) -> Result<(), SemanticaError> {
                let run: fn(&Self, &mut TransactionBody<'_>) -> Result<(), SemanticaError> =
                    $transaction;
                if self.faults.commit {
                    return run(self, &mut |conn| {
                        body(conn)?;
                        Err(SemanticaError::Other {
                            module: "sqlite3".into(),
                            type_name: "OperationalError".into(),
                            message: "Commit failed".into(),
                        })
                    });
                }
                run(self, body)
            }
            fn savepoint(
                &self,
                conn: Option<&StorageConnection<'_>>,
                body: &mut TransactionBody<'_>,
            ) -> Result<(), SemanticaError> {
                let run: fn(
                    &Self,
                    Option<&StorageConnection<'_>>,
                    &mut TransactionBody<'_>,
                ) -> Result<(), SemanticaError> = $savepoint;
                run(self, conn, body)
            }
            fn store_with_conn(
                &self,
                conn: Option<&StorageConnection<'_>>,
                entry: &ProvenanceEntry,
            ) -> Result<(), SemanticaError> {
                let call = self
                    .faults
                    .store_with_conn_calls
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                if self.faults.store_with_conn_always {
                    return Err(runtime("batch store error"));
                }
                if call == self.faults.fail_on_call.load(Ordering::SeqCst) {
                    return Err(runtime(&format!("Simulated error on call {call}")));
                }
                self.inner.store_with_conn(conn, entry)
            }
            fn retrieve_with_conn(
                &self,
                conn: Option<&StorageConnection<'_>>,
                id: &str,
            ) -> Result<Option<ProvenanceEntry>, SemanticaError> {
                if self.faults.retrieve_with_conn {
                    return Err(runtime("retrieve error"));
                }
                self.inner.retrieve_with_conn(conn, id)
            }
            fn get_chain_head(
                &self,
                conn: Option<&StorageConnection<'_>>,
            ) -> Result<Option<(i64, Option<String>)>, SemanticaError> {
                self.inner.get_chain_head(conn)
            }
        }
    };
}

wrapped_storage!(
    InMemoryStorage,
    |this, body| this.inner.transaction_via(this, body),
    |this, _conn, body| this.inner.savepoint_via(this, body)
);
wrapped_storage!(
    SQLiteStorage,
    |this, body| this.inner.transaction(body),
    |this, conn, body| this.inner.savepoint(conn, body)
);

fn memory(faults: Faults) -> Arc<Wrapped<InMemoryStorage>> {
    Arc::new(Wrapped {
        inner: InMemoryStorage::new(),
        faults,
    })
}

fn sqlite(dir: &tempfile::TempDir, name: &str, faults: Faults) -> Arc<Wrapped<SQLiteStorage>> {
    Arc::new(Wrapped {
        inner: SQLiteStorage::new(dir.path().join(name).to_str().unwrap()).unwrap(),
        faults,
    })
}

fn track_all_kinds(m: &ProvenanceManager) -> [bool; 4] {
    let source = SourceReference {
        page: Some(1),
        confidence: 0.9,
        ..SourceReference::new("doc_1")
    };
    [
        m.track_entity("e_test", "doc_1", None, te()).is_none(),
        m.track_relationship("r_test", "doc_1", None, te())
            .is_none(),
        m.track_chunk(
            "c_test",
            "doc_1",
            TrackChunkOptions::new()
                .source_path("/path/to/doc.pdf")
                .span(0, 100),
        )
        .is_none(),
        m.track_property_source(
            "e_test",
            "prop_test",
            "val",
            &source,
            TrackKwargs::default(),
        )
        .is_none(),
    ]
}

#[test]
fn storage_store_errors_are_swallowed() {
    // test_track_*_storage_error_swallowed: nothing persisted -> None.
    let m = ProvenanceManager::with_shared_storage(memory(Faults {
        store: true,
        ..Faults::default()
    }));
    assert_eq!(track_all_kinds(&m), [true; 4]);

    // The *_sqlite variants: relationship, chunk and property-source entries are saved with
    // `store()`; (SQLite `track_entity` writes through `_store_with_conn`, which they do not
    // patch).
    let tmp = tempfile::tempdir().unwrap();
    let failing = sqlite(
        &tmp,
        "store_error.db",
        Faults {
            store: true,
            ..Faults::default()
        },
    );
    let m = ProvenanceManager::with_shared_storage(failing);
    let results = track_all_kinds(&m);
    assert_eq!(&results[1..], &[true, true, true]);
}

#[test]
fn save_entry_logs_on_every_failure_path() {
    let logs = LogCapture::new();
    let m = ProvenanceManager::with_shared_storage(memory(Faults {
        store: true,
        ..Faults::default()
    }));
    let mut entry = ProvenanceEntry {
        source_document: "doc_1".into(),
        ..ProvenanceEntry::new("log_test_id", "entity", "test")
    };
    // Swallowing branch
    assert_eq!(m.save_entry(&mut entry, None, false).unwrap(), None);
    assert_eq!(logs.matching("'log_test_id'").len(), 1);
    // Raising branch
    let err = m.save_entry(&mut entry, None, true).unwrap_err();
    assert_eq!(err.type_name(), "RuntimeError");
    assert_eq!(err.to_string(), "storage error");
    assert_eq!(logs.matching("'log_test_id'").len(), 2);
    assert_eq!(
        logs.matching("'log_test_id'")[0],
        "Failed to save provenance entry for entity 'log_test_id': storage error"
    );
}

#[test]
fn batches_log_per_item_and_block_failures() {
    let logs = LogCapture::new();
    let m = ProvenanceManager::with_shared_storage(memory(Faults {
        store_with_conn_always: true,
        ..Faults::default()
    }));
    let entities = vec![
        obj(json!({"id": "e_fail_1"})),
        obj(json!({"id": "e_fail_2"})),
    ];
    assert_eq!(
        m.track_entities_batch(&entities, "doc_1", te(), Map::new()),
        0
    );
    assert_eq!(logs.matching("'e_fail_").len(), 2);
    let chunks = vec![
        obj(json!({"id": "chk_fail_1", "start_index": 0, "end_index": 10})),
        obj(json!({"id": "chk_fail_2", "start_index": 0, "end_index": 10})),
    ];
    assert_eq!(
        m.track_chunks_batch(&chunks, "doc_1", None, TrackKwargs::default()),
        0
    );
    assert_eq!(logs.matching("'chk_fail_").len(), 2);

    // Block-level failure: the shared transaction fails to commit.
    let tmp = tempfile::tempdir().unwrap();
    let m = ProvenanceManager::with_shared_storage(sqlite(
        &tmp,
        "block.db",
        Faults {
            commit: true,
            ..Faults::default()
        },
    ));
    let chunks: Vec<_> = (0..5)
        .map(|i| obj(json!({"id": format!("blk_{i}"), "start_index": 0, "end_index": 10})))
        .collect();
    assert_eq!(
        m.track_chunks_batch(&chunks, "doc_1", None, TrackKwargs::default()),
        0
    );
    assert_eq!(
        logs.matching("Block-level storage transaction failed in track_chunks_batch"),
        vec!["Block-level storage transaction failed in track_chunks_batch: Commit failed"]
    );
    assert!(m.storage().retrieve_all(None).unwrap().is_empty());
}

#[test]
fn pre_build_failure_returns_none_without_storing() {
    let logs = LogCapture::new();
    let storage = memory(Faults {
        retrieve_with_conn: true,
        ..Faults::default()
    });
    let m = ProvenanceManager::with_shared_storage(storage.clone());
    assert!(m.track_entity("e_prebuild", "doc_1", None, te()).is_none());
    assert_eq!(storage.faults.store_calls.load(Ordering::SeqCst), 0);
    let logged = logs.matching("'e_prebuild'");
    assert_eq!(logged.len(), 1);
    assert_eq!(
        logged[0],
        "Failed to track entity 'e_prebuild' (transaction rolled back): retrieve error. \
         Returning pre-failure state (None (no prior entry existed))."
    );
}

fn check_write_failure_rollback<S>(storage: Arc<Wrapped<S>>, fail_on_call: usize)
where
    Wrapped<S>: ProvenanceStorage,
    S: Send + Sync + 'static,
{
    let m = ProvenanceManager::with_shared_storage(storage.clone());
    let v1 = m.track_entity("e1", "doc1", None, te()).unwrap();
    assert_eq!(v1.source_document, "doc1");

    storage
        .faults
        .fail_on_call
        .store(fail_on_call, Ordering::SeqCst);
    // A standalone call does not raise and returns the pre-failure state (v1).
    let res = m.track_entity("e1", "doc2", None, te()).unwrap();
    let in_storage = storage.retrieve("e1").unwrap().unwrap();
    assert_eq!(res.source_document, "doc1");
    assert_eq!(in_storage.source_document, "doc1");
    let ids: Vec<String> = storage
        .retrieve_all(None)
        .unwrap()
        .into_iter()
        .map(|e| e.entity_id)
        .collect();
    assert_eq!(ids, vec!["e1".to_string()]);

    // With a connection (batch mode) the error propagates.
    storage
        .faults
        .store_with_conn_calls
        .store(1, Ordering::SeqCst);
    let raised = Mutex::new(None);
    let outcome = m.get_or_create_transaction(None, &mut |conn| {
        let result = m.track_entity_with_conn("e1", "doc2", None, te(), conn);
        let failed = result.is_err();
        *raised.lock().unwrap() = Some(failed);
        result.map(|_| ())
    });
    assert!(outcome.is_err());
    assert_eq!(*raised.lock().unwrap(), Some(true));
}

#[test]
fn history_and_primary_write_failures_roll_back() {
    // test_track_entity_atomic_rollback_on_{history,primary}_write_failure, both backends.
    check_write_failure_rollback(memory(Faults::default()), 2);
    check_write_failure_rollback(memory(Faults::default()), 3);
    let tmp = tempfile::tempdir().unwrap();
    check_write_failure_rollback(sqlite(&tmp, "hist.db", Faults::default()), 2);
    check_write_failure_rollback(sqlite(&tmp, "prim.db", Faults::default()), 3);
}

fn check_safe_copy<S>(storage: Arc<Wrapped<S>>)
where
    Wrapped<S>: ProvenanceStorage,
    S: Send + Sync + 'static,
{
    let m = ProvenanceManager::with_shared_storage(storage.clone());
    let v1 = m
        .track_entity("e1", "doc1", Some(obj(json!({"key": "val"}))), te())
        .unwrap();
    assert_eq!(v1.source_document, "doc1");
    storage.faults.fail_on_call.store(2, Ordering::SeqCst);
    let mut res = m.track_entity("e1", "doc2", None, te()).unwrap();
    assert_eq!(res.source_document, "doc1");
    res.source_document = "mutated".into();
    res.metadata.insert("tampered".into(), json!(true));
    let in_storage = storage.retrieve("e1").unwrap().unwrap();
    assert_eq!(in_storage.source_document, "doc1");
    assert_eq!(
        Value::Object(in_storage.metadata.clone()),
        json!({"key": "val"})
    );
}

#[test]
fn rollback_returns_a_safe_copy() {
    check_safe_copy(memory(Faults::default()));
    let tmp = tempfile::tempdir().unwrap();
    check_safe_copy(sqlite(&tmp, "safe.db", Faults::default()));
}

fn check_batch_savepoint<S>(storage: Arc<Wrapped<S>>)
where
    Wrapped<S>: ProvenanceStorage,
    S: Send + Sync + 'static,
{
    let m = ProvenanceManager::with_shared_storage(storage.clone());
    let v1 = m.track_entity("e1", "doc1", None, te()).unwrap();
    assert_eq!(v1.source_document, "doc1");
    // Fail on call 3: the primary update for e1, after its history entry was stored on call 2.
    storage.faults.fail_on_call.store(3, Ordering::SeqCst);
    let count = m.track_entities_batch(
        &[obj(json!({"id": "e1"})), obj(json!({"id": "e2"}))],
        "doc_batch",
        te(),
        Map::new(),
    );
    assert_eq!(count, 1);
    let all = storage.retrieve_all(None).unwrap();
    let e2 = all.iter().find(|e| e.entity_id == "e2").unwrap();
    assert_eq!(e2.source_document, "doc_batch");
    let e1 = all.iter().find(|e| e.entity_id == "e1").unwrap();
    assert_eq!(e1.source_document, "doc1");
    assert!(
        !all.iter().any(|e| e.entity_id.contains(":v:")),
        "leaked history entry"
    );
}

#[test]
fn batch_savepoints_roll_back_failed_items() {
    check_batch_savepoint(memory(Faults::default()));
    let tmp = tempfile::tempdir().unwrap();
    check_batch_savepoint(sqlite(&tmp, "batch_sp.db", Faults::default()));
}
