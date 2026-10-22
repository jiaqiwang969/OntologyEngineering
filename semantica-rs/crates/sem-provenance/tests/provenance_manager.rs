//! Characterisation tests for `manager.rs` against the oracle (`fixtures/oracle.json`): every
//! scenario runs the same calls as `generate_oracle.py` under the same deterministic clock and
//! compares the complete results (entries, checksums, lineage, history, audit logs, chain
//! verification), including how many times the clock was read.

mod common;

use common::{
    CountingClock, assert_exports_match, assert_json_eq, dicts, err_json, ids, obj, oracle,
};
use sem_provenance::schemas::{ActivityRecord, AgentRecord, SourceReference};
use sem_provenance::storage::{InMemoryStorage, SQLiteStorage};
use sem_provenance::{
    ProvenanceManager, ProvenanceManagerOptions, TrackChunkOptions, TrackEntityOptions,
    TrackKwargs, default_storage_path,
};
use serde_json::{Map, Value, json};

fn te() -> TrackEntityOptions {
    TrackEntityOptions::default()
}

fn meta(value: Value) -> Option<Map<String, Value>> {
    Some(obj(value))
}

/// `source_documents` comes from a Python `set`: compare it sorted.
fn normalize_lineage(value: Value) -> Value {
    let mut value = value;
    if let Some(map) = value.as_object_mut()
        && let Some(Value::Array(docs)) = map.get_mut("source_documents")
    {
        docs.sort_by_key(|d| d.to_string());
    }
    value
}

/// Order of entries that come from Python set iteration (SQLite descendants): sort by id.
fn sort_entries(value: Value, keys: &[&str]) -> Value {
    let mut value = value;
    if let Some(map) = value.as_object_mut() {
        for key in keys {
            if let Some(Value::Array(items)) = map.get_mut(*key) {
                items.sort_by_key(|item| item["entity_id"].to_string());
            }
        }
    }
    value
}

fn map_value(map: Map<String, Value>) -> Value {
    Value::Object(map)
}

fn maps_value(maps: Vec<Map<String, Value>>) -> Value {
    Value::Array(maps.into_iter().map(Value::Object).collect())
}

#[test]
fn scenario_basic_matches_oracle() {
    let expected = oracle("basic");
    let clock = CountingClock::new();
    let m = ProvenanceManager::new().unwrap();
    let e = m
        .track_entity("entity_1", "doc_1", meta(json!({"confidence": 0.9})), te())
        .unwrap();
    assert_json_eq(&map_value(e.to_dict()), &expected["entry"]);
    assert_json_eq(
        &dicts(&m.storage().retrieve_all(None).unwrap()),
        &expected["all"],
    );
    m.track_entity("entity_2", "doc_1", None, te());
    assert_eq!(json!(m.clear().unwrap()), expected["clear"]);
    assert_json_eq(
        &map_value(m.get_lineage("entity_1").unwrap()),
        &expected["lineage_after_clear"],
    );
    assert_eq!(m.storage().get_chain_head(None).unwrap(), None);
    assert!(expected["chain_head_after_clear"].is_null());
    let fresh = m.track_entity("entity_3", "doc_2", None, te()).unwrap();
    assert_eq!(
        json!([fresh.sequence_id, fresh.previous_checksum]),
        expected["fresh_after_clear"]
    );
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
}

#[test]
fn scenario_lineage_matches_oracle() {
    let expected = oracle("lineage");
    let clock = CountingClock::new();
    let m = ProvenanceManager::new().unwrap();
    m.track_entity("e1", "doc_1", None, te());
    m.track_chunk(
        "chunk_1",
        "doc_1",
        TrackChunkOptions::new().parent_chunk_id("e1"),
    );
    assert_json_eq(
        &normalize_lineage(map_value(m.get_lineage("chunk_1").unwrap())),
        &normalize_lineage(expected["chunk_1"].clone()),
    );
    assert_json_eq(
        &normalize_lineage(map_value(m.get_lineage("e1").unwrap())),
        &normalize_lineage(expected["e1"].clone()),
    );
    assert_json_eq(
        &map_value(m.get_lineage("nope").unwrap()),
        &expected["missing"],
    );
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
}

fn run_retrack(m: &ProvenanceManager) {
    m.track_entity(
        "X",
        "doc_1",
        meta(json!({"status": "draft", "n": 1})),
        te().agent_id("alice"),
    );
    m.track_entity(
        "X",
        "doc_2",
        meta(json!({"status": "final"})),
        te().agent_id("bob").revision_type("correction"),
    );
    m.track_entity("P", "doc_0", None, te());
    m.track_entity(
        "X",
        "doc_3",
        None,
        te().agent_id("carol")
            .supersedes("X_old")
            .parent_entity_id("P"),
    );
    m.track_entity("Y", "X", None, te());
    m.track_entity(
        "Z",
        "doc_1",
        meta(json!({"derived_from": "Y", "z": true})),
        te().used_entities(["P"]),
    );
    m.track_entity(
        "W",
        "doc_1",
        meta(json!({"derived_from": ""})),
        te().parent_entity_id(""),
    );
    m.track_entity("V", "doc_1", meta(json!({"derived_from": 12345})), te());
}

fn check_retrack(m: &ProvenanceManager, expected: &Value, sqlite: bool) {
    let descendants_order = |v: Value| {
        if sqlite {
            sort_entries(v, &["descendant_chain", "entries"])
        } else {
            v
        }
    };
    let sorted_ids = |mut v: Vec<String>| {
        if sqlite {
            v.sort();
        }
        json!(v)
    };
    let sorted_expected = |v: &Value| {
        let mut items: Vec<String> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        if sqlite {
            items.sort();
        }
        json!(items)
    };

    assert_json_eq(
        &dicts(&m.storage().retrieve_all(None).unwrap()),
        &expected["all"],
    );
    assert_json_eq(
        &normalize_lineage(map_value(m.get_lineage("X").unwrap())),
        &normalize_lineage(expected["lineage_X"].clone()),
    );
    assert_json_eq(
        &normalize_lineage(map_value(m.get_lineage("Z").unwrap())),
        &normalize_lineage(expected["lineage_Z"].clone()),
    );
    assert_json_eq(
        &maps_value(m.revision_history("X").unwrap()),
        &expected["revision_history_X"],
    );
    assert_json_eq(
        &maps_value(m.revision_history("missing").unwrap()),
        &expected["revision_history_missing"],
    );
    assert_json_eq(
        &map_value(m.verify_chain().unwrap()),
        &expected["verify_chain"],
    );
    assert_json_eq(&map_value(m.check(false).unwrap()), &expected["check"]);
    assert_json_eq(
        &map_value(m.check(true).unwrap()),
        &expected["check_strict"],
    );
    assert_json_eq(
        &descendants_order(map_value(m.get_descendants("P").unwrap())),
        &descendants_order(expected["descendants_P"].clone()),
    );
    assert_json_eq(
        &map_value(m.get_descendants("missing").unwrap()),
        &expected["descendants_missing"],
    );
    assert_eq!(
        sorted_ids(ids(&m.trace_descendants("X", None).unwrap())),
        sorted_expected(&expected["trace_descendants_X"])
    );
    assert_eq!(
        sorted_ids(ids(&m.trace_descendants("P", Some(1)).unwrap())),
        sorted_expected(&expected["trace_descendants_P_1"])
    );
    assert_eq!(
        json!(ids(&m.trace_lineage("Z", Some(1)).unwrap())),
        expected["trace_lineage_Z_1"]
    );
    assert_eq!(
        json!(ids(&m.trace_lineage("Z", Some(2)).unwrap())),
        expected["trace_lineage_Z_2"]
    );
    assert_json_eq(
        &m.audit_log(None, "table").unwrap().to_value(),
        &expected["audit_table"],
    );
    assert_json_eq(
        &m.audit_log(None, "csv").unwrap().to_value(),
        &expected["audit_csv"],
    );
    assert_json_eq(
        &m.audit_log(None, "json").unwrap().to_value(),
        &expected["audit_json"],
    );
    assert_json_eq(
        &m.audit_log(Some("2026-01-01T00:00:10"), "csv")
            .unwrap()
            .to_value(),
        &expected["audit_since"],
    );
    assert_json_eq(
        &m.audit_log(None, "xml").unwrap().to_value(),
        &expected["audit_other_format"],
    );
    assert_json_eq(
        &normalize_lineage(map_value(m.lineage("Z", 2).unwrap())),
        &normalize_lineage(expected["lineage_cli_Z_2"].clone()),
    );
    assert_json_eq(
        &normalize_lineage(map_value(m.lineage("Z", -1).unwrap())),
        &normalize_lineage(expected["lineage_cli_Z_neg"].clone()),
    );
    assert_json_eq(
        &map_value(m.lineage("missing", 3).unwrap()),
        &expected["lineage_cli_missing"],
    );
    assert_json_eq(
        &map_value(m.get_statistics().unwrap()),
        &expected["statistics"],
    );
    assert_json_eq(
        &maps_value(m.get_all_sources("Z").unwrap()),
        &expected["sources_Z"],
    );
    assert_json_eq(
        &maps_value(
            m.query_recorded_between("2026-01-01T00:00:05", "2026-01-01T00:00:15")
                .unwrap(),
        ),
        &expected["recorded_between"],
    );
    assert_json_eq(
        &maps_value(
            m.query_recorded_between("1990-01-01T00:00:00", "1990-01-02T00:00:00")
                .unwrap(),
        ),
        &expected["recorded_between_none"],
    );
    assert_json_eq(
        &m.get_provenance("Y").unwrap().map(Value::Object).unwrap(),
        &expected["provenance_Y"],
    );
    assert_eq!(m.get_provenance("missing").unwrap(), None);
    assert!(expected["provenance_missing"].is_null());
}

#[test]
fn scenario_retrack_matches_oracle() {
    let expected = oracle("retrack");
    let clock = CountingClock::new();
    let m = ProvenanceManager::new().unwrap();
    run_retrack(&m);
    let calls = clock.calls();
    check_retrack(&m, expected, false);
    assert_eq!(json!(calls), expected["clock_calls"]);
}

#[test]
fn scenario_retrack_sqlite_matches_oracle() {
    let expected = oracle("retrack_sqlite");
    let tmp = tempfile::tempdir().unwrap();
    let clock = CountingClock::new();
    let m = ProvenanceManager::with_storage_path(tmp.path().join("retrack.db").to_str().unwrap())
        .unwrap();
    run_retrack(&m);
    let calls = clock.calls();
    check_retrack(&m, expected, true);
    assert_eq!(json!(calls), expected["clock_calls"]);
}

fn check_invalidate(m: ProvenanceManager, expected: &Value, clock: &CountingClock) {
    m.track_entity("e1", "doc1", None, te());
    m.track_entity("e2", "doc1", None, te().parent_entity_id("e1"));
    let inv = m
        .invalidate(
            "e1",
            "reviewer_jane",
            Some("retracted"),
            meta(json!({"ticket": 7})),
        )
        .unwrap();
    let inv2 = m.invalidate("e1", "r2", None, None).unwrap();
    assert_json_eq(&map_value(inv.to_dict()), &expected["inv"]);
    assert_json_eq(&map_value(inv2.to_dict()), &expected["inv2"]);
    let missing = m.invalidate("never_tracked", "x", None, None).unwrap_err();
    assert_eq!(err_json(&missing), expected["missing"]);
    assert_json_eq(
        &dicts(&m.storage().retrieve_all(None).unwrap()),
        &expected["all"],
    );
    assert_json_eq(
        &map_value(m.verify_chain().unwrap()),
        &expected["verify_chain"],
    );
    assert_json_eq(&map_value(m.check(false).unwrap()), &expected["check"]);
    assert_exports_match(&m, None, &expected["export"], false);
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
}

#[test]
fn scenario_invalidate_matches_oracle() {
    let clock = CountingClock::new();
    check_invalidate(
        ProvenanceManager::new().unwrap(),
        oracle("invalidate"),
        &clock,
    );
}

#[test]
fn scenario_invalidate_sqlite_matches_oracle() {
    let tmp = tempfile::tempdir().unwrap();
    let clock = CountingClock::new();
    let m =
        ProvenanceManager::with_storage_path(tmp.path().join("inv.db").to_str().unwrap()).unwrap();
    check_invalidate(m, oracle("invalidate_sqlite"), &clock);
}

pub fn run_kwargs(m: &ProvenanceManager) -> Map<String, Value> {
    let mut out = Map::new();
    let put = |out: &mut Map<String, Value>,
               key: &str,
               entry: Option<sem_provenance::ProvenanceEntry>| {
        out.insert(key.into(), map_value(entry.expect("tracked").to_dict()));
    };
    put(
        &mut out,
        "agent_record",
        m.track_entity(
            "e1",
            "doc1",
            None,
            te().agent(AgentRecord {
                agent_type: "person".into(),
                is_automated: false,
                ..AgentRecord::new("reviewer_bob")
            })
            .role("approver")
            .agent_id("ignored"),
        ),
    );
    put(
        &mut out,
        "activity_record",
        m.track_entity(
            "e2",
            "doc1",
            None,
            te().activity(ActivityRecord {
                started_at_time: Some("2026-01-01T00:00:00".into()),
                ended_at_time: Some("2026-01-01T00:00:03".into()),
                ..ActivityRecord::new("run42")
            })
            .activity_id("ignored"),
        ),
    );
    put(
        &mut out,
        "scalars",
        m.track_entity(
            "e3",
            "doc1",
            None,
            te().agent_id("alice")
                .agent_type("person")
                .is_automated(false)
                .role("r")
                .activity_id("parse")
                .activity_started_at_time("t0")
                .activity_ended_at_time("t1")
                .acted_on_behalf_of("org1")
                .informed_by(["ingest"])
                .valid_from("2026-01-01")
                .valid_until("2026-06-01")
                .revision_type("correction")
                .supersedes("old")
                .bundle_id("run_1")
                .source_location("p.4")
                .source_quote("q")
                .confidence(0.75)
                .entity_type("claim"),
        ),
    );
    put(
        &mut out,
        "relationship",
        m.track_relationship(
            "r1",
            "doc1",
            meta(json!({"type": "founded"})),
            te().agent_id("bot1")
                .confidence(0.5)
                .source_location("loc")
                .informed_by(["x"])
                .source_quote("ignored")
                .parent_entity_id("ignored")
                .entity_type("ignored")
                .used_entities(["ignored"]),
        ),
    );
    put(
        &mut out,
        "relationship_plain",
        m.track_relationship("r2", "doc2", None, te()),
    );
    put(
        &mut out,
        "chunk",
        m.track_chunk(
            "c1",
            "doc1",
            TrackChunkOptions::new()
                .source_path("/p.pdf")
                .span(5, 10)
                .metadata(obj(
                    json!({"agent_id": "chunker_v2", "note": "hello", "activity_id": "split"}),
                )),
        ),
    );
    put(
        &mut out,
        "chunk_records",
        m.track_chunk(
            "c2",
            "doc1",
            TrackChunkOptions {
                parent_chunk_id: Some("c1".into()),
                agent: Some(AgentRecord::new("ag")),
                activity: Some(ActivityRecord {
                    started_at_time: Some("s".into()),
                    ..ActivityRecord::new("act2")
                }),
                metadata: obj(json!({"role": "x", "is_automated": false, "activity_started_at_time": "ignored"})),
                ..TrackChunkOptions::default()
            },
        ),
    );
    let src = SourceReference {
        page: Some(4),
        confidence: 0.92,
        metadata: obj(json!({"credibility": 0.8, "note": "n"})),
        ..SourceReference::new("DOI:10.1038/x")
    };
    put(
        &mut out,
        "property",
        m.track_property_source(
            "e1",
            "biomass",
            "463%",
            &src,
            TrackKwargs::from_metadata(obj(
                json!({"agent_id": "prop_tracker", "extra": 1, "value_unit": "pct"}),
            )),
        ),
    );
    let src2 = SourceReference {
        page: Some(0),
        section: Some("Table 1".into()),
        metadata: obj(json!({"value": "shadow"})),
        ..SourceReference::new("doc2")
    };
    put(
        &mut out,
        "property_section",
        m.track_property_source(
            "e2",
            "p",
            json!({"v": [1, 2]}),
            &src2,
            TrackKwargs::default(),
        ),
    );
    let src3 = SourceReference::new("doc3");
    put(
        &mut out,
        "property_plain",
        m.track_property_source(
            "e3",
            "q",
            Value::Null,
            &src3,
            TrackKwargs::from_metadata(obj(json!({"entity_id_note": "x"}))),
        ),
    );
    out
}

#[test]
fn scenario_kwargs_matches_oracle() {
    let expected = oracle("kwargs");
    let clock = CountingClock::new();
    let m = ProvenanceManager::new().unwrap();
    let out = run_kwargs(&m);
    for (key, value) in &out {
        assert_json_eq(value, &expected[key.as_str()]);
    }
    assert_json_eq(
        &map_value(m.get_statistics().unwrap()),
        &expected["statistics"],
    );
    assert_exports_match(&m, None, &expected["export"], false);
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
}

fn check_batch(m: ProvenanceManager, expected: &Value, clock: &CountingClock) {
    let entities: Vec<Map<String, Value>> = vec![
        obj(json!({"id": "b1"})),
        obj(json!({"entity_id": "b2", "metadata": {"k": 1, "extra_note": "override"}})),
        obj(json!({"id": ""})),
        obj(json!({"name": "no id"})),
        obj(json!({"id": "b3", "entity_id": "ignored", "confidence": 0.1})),
    ];
    let count = m.track_entities_batch(
        &entities,
        "doc_batch",
        te(),
        obj(json!({
            "agent_id": "batch_v2", "entity_type": "credit_feature",
            "activity_id": "bureau_parsing", "extra_note": "free", "informed_by": ["ingest"],
            "bundle_id": "run_1", "confidence": 0.5
        })),
    );
    assert_eq!(json!(count), expected["entities_count"]);
    let chunks: Vec<Map<String, Value>> = vec![
        obj(json!({"id": "c1", "start_index": 0, "end_index": 10})),
        obj(
            json!({"chunk_id": "c2", "start_index": 10, "end_index": 20, "parent_chunk_id": "c1",
                   "metadata": {"m": 2, "lang": "de"}}),
        ),
        obj(json!({"id": "c3", "metadata": {"source_path": "boom"}})),
        obj(json!({"id": "c4", "metadata": null})),
        obj(json!({"id": "c5"})),
    ];
    let count = m.track_chunks_batch(
        &chunks,
        "doc1",
        Some("/p"),
        TrackKwargs::from_metadata(obj(json!({"lang": "en", "agent_id": "chunker"}))),
    );
    assert_eq!(json!(count), expected["chunks_count"]);
    let failing = vec![
        obj(json!({"id": "z1"})),
        obj(json!({"id": "z2", "metadata": null})),
    ];
    assert_eq!(
        json!(m.track_entities_batch(&failing, "d", te(), Map::new())),
        expected["block_failure_count"]
    );
    assert_eq!(
        json!(m.track_entities_batch(&[], "d", te(), Map::new())),
        expected["empty_count"]
    );
    assert_json_eq(
        &dicts(&m.storage().retrieve_all(None).unwrap()),
        &expected["all"],
    );
    assert_json_eq(
        &map_value(m.verify_chain().unwrap()),
        &expected["verify_chain"],
    );
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
}

#[test]
fn scenario_batch_matches_oracle() {
    let clock = CountingClock::new();
    check_batch(ProvenanceManager::new().unwrap(), oracle("batch"), &clock);
}

#[test]
fn scenario_batch_sqlite_matches_oracle() {
    let tmp = tempfile::tempdir().unwrap();
    let clock = CountingClock::new();
    let m = ProvenanceManager::with_storage_path(tmp.path().join("batch.db").to_str().unwrap())
        .unwrap();
    check_batch(m, oracle("batch_sqlite"), &clock);
}

fn sql_rows(path: &std::path::Path, sql: &str) -> Value {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let count = stmt.column_count();
    let rows: Vec<Value> = stmt
        .query_map([], |row| {
            let values: Vec<Value> = (0..count)
                .map(|i| match row.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Real(f) => json!(f),
                    rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    rusqlite::types::ValueRef::Blob(b) => json!(b),
                })
                .collect();
            Ok(Value::Array(values))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

#[test]
fn scenario_sqlite_matches_oracle() {
    let expected = oracle("sqlite");
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("manager.db");
    let db_str = db.to_str().unwrap().to_string();
    let clock = CountingClock::new();
    let m = ProvenanceManager::with_storage_path(db_str.clone()).unwrap();
    m.track_entity(
        "e1",
        "doc1",
        meta(json!({"k": "v", "n": 1.5, "u": "\u{e9}"})),
        te().used_entities(["a", "b"]),
    );
    m.track_entity("e2", "doc1", None, te().parent_entity_id("e1"));
    m.track_entity("e1", "doc1", None, te());
    m.track_chunk(
        "c1",
        "doc1",
        TrackChunkOptions::new().parent_chunk_id("e2").span(1, 2),
    );
    assert_json_eq(
        &dicts(&m.storage().retrieve_all(None).unwrap()),
        &expected["all"],
    );
    let head = m.storage().get_chain_head(None).unwrap().unwrap();
    assert_eq!(json!([head.0, head.1]), expected["chain_head"]);
    assert_json_eq(
        &normalize_lineage(map_value(m.get_lineage("c1").unwrap())),
        &normalize_lineage(expected["lineage_c1"].clone()),
    );
    let mut down = ids(&m.trace_descendants("e1", None).unwrap());
    down.sort();
    assert_eq!(json!(down), expected["descendants_e1_sorted"]);
    assert_json_eq(
        &map_value(m.verify_chain().unwrap()),
        &expected["verify_chain"],
    );
    assert_json_eq(
        &sql_rows(
            &db,
            "SELECT entity_id, used_entities, metadata FROM provenance ORDER BY rowid",
        ),
        &expected["raw"],
    );

    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("DELETE FROM provenance WHERE entity_id = 'e2'")
            .unwrap();
    }
    let m2 = ProvenanceManager::with_storage_path(db_str.clone()).unwrap();
    assert_json_eq(
        &map_value(m2.verify_chain().unwrap()),
        &expected["verify_after_delete"],
    );
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "UPDATE provenance SET sequence_id = 10 WHERE entity_id = 'c1';
             UPDATE provenance SET source_document = 'tampered' WHERE entity_id = 'e1';",
        )
        .unwrap();
    }
    let m3 = ProvenanceManager::with_storage_path(db_str).unwrap();
    assert_json_eq(
        &map_value(m3.verify_chain().unwrap()),
        &expected["verify_after_tamper"],
    );
    assert_eq!(json!(clock.calls()), expected["clock_calls"]);
}

fn kind(m: &ProvenanceManager) -> &'static str {
    if m.storage().is::<SQLiteStorage>() {
        "SQLiteStorage"
    } else if m.storage().is::<InMemoryStorage>() {
        "InMemoryStorage"
    } else {
        "custom"
    }
}

#[test]
fn constructor_config_resolution_matches_oracle() {
    let expected = oracle("config");
    let tmp = tempfile::tempdir().unwrap();
    let p1 = tmp.path().join("cfg.db").to_str().unwrap().to_string();
    let with = |config: Value| -> Value {
        match ProvenanceManager::with_config(config) {
            Ok(m) => json!(kind(&m)),
            Err(e) => err_json(&e),
        }
    };
    assert_eq!(
        with(json!({"provenance": {"storage_path": p1}})),
        expected["provenance_section"]
    );
    assert_eq!(with(json!({"storage_path": p1})), expected["top_level"]);
    assert_eq!(
        with(json!({"provenance": {"storage_path": null}, "storage_path": p1})),
        expected["section_none"]
    );
    assert_eq!(
        with(json!({"provenance": "x", "storage_path": p1})),
        expected["section_not_mapping"]
    );
    assert_eq!(with(json!({})), expected["empty"]);
    assert_eq!(with(json!(["storage_path"])), expected["not_mapping"]);
    assert_eq!(with(json!({"storage_path": 5})), expected["bad_type"]);
    let unopenable = tmp.path().join("missing_dir").join("x.db");
    let got = match ProvenanceManager::with_storage_path(unopenable.to_str().unwrap()) {
        Ok(m) => json!(kind(&m)),
        Err(e) => err_json(&e),
    };
    assert_eq!(got, expected["unopenable"]);

    // An explicit storage path wins over the config.
    let explicit = tmp.path().join("explicit.db").to_str().unwrap().to_string();
    let m = ProvenanceManager::with_options(ProvenanceManagerOptions {
        storage_path: Some(explicit.clone()),
        config: Some(json!({"provenance": {"storage_path": p1}})),
        ..ProvenanceManagerOptions::default()
    })
    .unwrap();
    assert_eq!(
        m.storage()
            .downcast_ref::<SQLiteStorage>()
            .unwrap()
            .db_path(),
        explicit
    );
}

#[test]
fn default_storage_path_matches_oracle() {
    let expected = oracle("default_path").as_array().unwrap().clone();
    let tmp = tempfile::tempdir().unwrap();
    let path = |name: &str| tmp.path().join(name).to_str().unwrap().to_string();
    let mut observed: Vec<Value> = Vec::new();
    let mut record = |label: &str| {
        let current = ProvenanceManager::current_default_storage_path();
        let base = current.map(|p| {
            std::path::Path::new(&p)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        });
        let m = ProvenanceManager::new().unwrap();
        observed.push(json!([label, base, kind(&m)]));
    };

    // Hold the class lock for the whole sequence so concurrently running tests that build a
    // ProvenanceManager() wait instead of seeing these temporary defaults.
    let lock_holder = default_storage_path(None);
    record("initial");
    {
        let _outer = default_storage_path(Some(&path("outer.db")));
        record("outer");
        {
            let _inner = ProvenanceManager::default_storage_path(Some(&path("inner.db")));
            record("inner");
            {
                let _none = default_storage_path(None);
                record("none");
            }
            record("back_inner");
        }
        record("back_outer");
    }
    record("after");
    {
        let _guard = default_storage_path(None);
        record("guard");
        ProvenanceManager::set_default_storage_path(Some(&path("x.db")));
        record("set");
        {
            let _nested = default_storage_path(None);
            record("nested_none");
        }
        record("after_nested");
        ProvenanceManager::set_default_storage_path(None);
        record("unset");
    }
    record("after_guard");
    // The original raises inside the context; the Rust counterpart is an early error return,
    // which drops the guard the same way.
    let exc = path("exc.db");
    let result: Result<(), sem_utils::SemanticaError> = (|| {
        let _ctx = default_storage_path(Some(&exc));
        record("in_exception_ctx");
        Err(sem_utils::SemanticaError::Runtime("boom".into()))?;
        Ok(())
    })();
    assert!(result.is_err());
    record("after_exception");
    drop(lock_holder);
    assert_eq!(Value::Array(observed), Value::Array(expected));
    // Read back under the class lock, as ProvenanceManager() does.
    assert!(
        ProvenanceManager::new()
            .unwrap()
            .storage()
            .is::<InMemoryStorage>()
    );
}

#[test]
fn default_storage_path_contexts_serialize_across_threads() {
    // The original's concurrency test: each thread's context sees its own path.
    let tmp = tempfile::tempdir().unwrap();
    let paths: Vec<String> = ["thread_a.db", "thread_b.db"]
        .iter()
        .map(|name| tmp.path().join(name).to_str().unwrap().to_string())
        .collect();
    let handles: Vec<_> = paths
        .iter()
        .cloned()
        .zip([50u64, 10])
        .map(|(path, delay)| {
            std::thread::spawn(move || {
                let _guard = default_storage_path(Some(&path));
                std::thread::sleep(std::time::Duration::from_millis(delay));
                let m = ProvenanceManager::new().unwrap();
                let storage = m.storage().downcast_ref::<SQLiteStorage>().unwrap();
                assert_eq!(storage.db_path(), path);
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(
        ProvenanceManager::new()
            .unwrap()
            .storage()
            .is::<InMemoryStorage>()
    );
}
