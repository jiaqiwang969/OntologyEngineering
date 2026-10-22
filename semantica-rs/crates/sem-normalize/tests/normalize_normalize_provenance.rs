//! Characterisation of `normalize_provenance.NormalizerWithProvenance` against the oracle.
//!
//! The original constructor always fails (it imports the missing
//! `semantica.normalize.normalizer`). The oracle also runs it with a stand-in `Normalizer`
//! module and a fixed `datetime.utcnow`; the Rust tests inject the same normalizer
//! (`with_normalizer` / `with_manager`) and the same clock (`sem_provenance::clock`).

mod common;

use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, TimeDelta};
use sem_normalize::TextNormalizer;
use sem_normalize::normalize_provenance::{Normalize, NormalizerWithProvenance};
use sem_provenance::clock::fixed_clock;
use sem_provenance::{InMemoryStorage, ProvenanceEntry, ProvenanceManager};
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

use common::{err_json, obj, oracle};

/// The oracle's stand-in `Normalizer(**config)`: `normalize(data, **kwargs)` returns
/// `{"normalized": data, "kwargs": kwargs}`.
struct StandIn {
    calls: Arc<Mutex<usize>>,
}

impl Normalize for StandIn {
    fn normalize(
        &self,
        data: &Value,
        kwargs: &Map<String, Value>,
    ) -> Result<Value, SemanticaError> {
        *self.calls.lock().unwrap() += 1;
        Ok(json!({"normalized": data, "kwargs": kwargs}))
    }
}

/// A normalizer that fails (Python: an exception escaping `self._normalizer.normalize`).
struct Failing;

impl Normalize for Failing {
    fn normalize(
        &self,
        _data: &Value,
        _kwargs: &Map<String, Value>,
    ) -> Result<Value, SemanticaError> {
        Err(SemanticaError::Value("cannot normalize".to_string()))
    }
}

fn clock() -> sem_provenance::clock::ClockGuard {
    let start = NaiveDate::from_ymd_opt(2024, 1, 2)
        .unwrap()
        .and_hms_opt(0, 30, 0)
        .unwrap();
    fixed_clock(start, TimeDelta::milliseconds(1500))
}

/// The recorded fields of a tracked entry, as the oracle script lists them.
fn entry_json(entry: &ProvenanceEntry) -> Value {
    let suffix = &entry.entity_id[10..];
    json!({
        "entity_type": entry.entity_type,
        "agent_id": entry.agent_id,
        "agent_type": entry.agent_type,
        "is_automated": entry.is_automated,
        "source_document": entry.source_document,
        "metadata": entry.metadata,
        "activity_started_at_time": entry.activity_started_at_time,
        "activity_ended_at_time": entry.activity_ended_at_time,
        "entity_id_prefix": &entry.entity_id[..10],
        "entity_id_len": entry.entity_id.chars().count(),
        "entity_id_hex": suffix.chars().all(|c| "0123456789abcdef".contains(c)),
    })
}

/// Build the wrapper of one oracle scenario: `init` holds `provenance`, `agent_id`,
/// `is_automated` and the rest, the `**config` the original passes to `Normalizer(**config)`
/// (returned so it can be compared with the stand-in's recorded config).
fn wrapper_for(
    init: &Map<String, Value>,
    calls: Arc<Mutex<usize>>,
) -> (NormalizerWithProvenance, Map<String, Value>) {
    let mut config = init.clone();
    let provenance = config
        .remove("provenance")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let agent_id = config.remove("agent_id");
    let is_automated = config.remove("is_automated").and_then(|v| v.as_bool());
    let wrapper = NormalizerWithProvenance::with_normalizer(
        Box::new(StandIn { calls }),
        provenance,
        agent_id.as_ref().and_then(Value::as_str),
        is_automated,
    )
    .unwrap();
    (wrapper, config)
}

#[test]
fn test_constructor_fails_like_the_original() {
    for case in oracle("normalize_provenance_new").as_array().unwrap() {
        let kwargs = obj(&case["kwargs"]);
        let provenance = kwargs
            .get("provenance")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let agent_id = kwargs.get("agent_id").and_then(Value::as_str);
        let error =
            NormalizerWithProvenance::new(provenance, agent_id, None, kwargs.clone()).unwrap_err();
        assert_eq!(err_json(&error), case["result"], "case {case}");
    }
}

#[test]
fn test_normalize_tracks_provenance() {
    for case in oracle("normalize_provenance_runs").as_array().unwrap() {
        let _clock = clock();
        let calls = Arc::new(Mutex::new(0));
        let (wrapper, config) = wrapper_for(&obj(&case["init"]), calls.clone());
        assert_eq!(json!(config), case["normalizer_config"], "case {case}");
        let result = wrapper
            .normalize(
                &case["data"],
                case["source"].as_str(),
                &obj(&case["kwargs"]),
            )
            .unwrap();
        assert_eq!(result, case["result"], "case {case}");
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(json!(wrapper.provenance), case["provenance"]);
        assert_eq!(json!(wrapper.agent_id()), case["agent_id"]);
        assert_eq!(json!(wrapper.is_automated()), case["is_automated"]);
        assert_eq!(json!(wrapper.prov_manager().is_some()), case["has_manager"]);
        let entries: Vec<Value> = match wrapper.prov_manager() {
            Some(manager) => manager
                .storage()
                .retrieve_all(None)
                .unwrap()
                .iter()
                .map(entry_json)
                .collect(),
            None => Vec::new(),
        };
        assert_eq!(json!(entries), case["entries"], "case {case}");
    }
}

#[test]
fn test_normalize_without_provenance() {
    let case = &oracle("normalize_provenance_runs")[0];
    assert_eq!(case["init"], json!({}));
    let calls = Arc::new(Mutex::new(0));
    let (wrapper, _) = wrapper_for(&Map::new(), calls.clone());
    assert!(!wrapper.provenance);
    assert!(wrapper.prov_manager().is_none());
    let result = wrapper
        .normalize(&case["data"], None, &obj(&case["kwargs"]))
        .unwrap();
    assert_eq!(result, case["result"]);
    // The wrapped normalizer stays reachable (the original's __getattr__ delegation).
    let direct = wrapper
        .normalizer()
        .normalize(&case["data"], &obj(&case["kwargs"]))
        .unwrap();
    assert_eq!(direct, case["result"]);
    assert_eq!(*calls.lock().unwrap(), 2);
}

#[test]
fn test_with_manager_records_into_the_given_manager() {
    // The second oracle scenario (provenance=True, defaults), with an explicit in-memory manager.
    let case = &oracle("normalize_provenance_runs")[1];
    let manager = ProvenanceManager::with_storage(InMemoryStorage::new());
    let calls = Arc::new(Mutex::new(0));
    let wrapper = NormalizerWithProvenance::with_manager(
        Box::new(StandIn { calls }),
        manager.clone(),
        None,
        None,
    );
    let _clock = clock();
    let result = wrapper
        .normalize(&case["data"], None, &obj(&case["kwargs"]))
        .unwrap();
    assert_eq!(result, case["result"]);
    // Clones of a manager share its storage.
    let entries: Vec<Value> = manager
        .storage()
        .retrieve_all(None)
        .unwrap()
        .iter()
        .map(entry_json)
        .collect();
    assert_eq!(json!(entries), case["entries"]);
    let stats = manager.get_statistics().unwrap();
    assert!(!stats.is_empty());
}

#[test]
fn test_failing_normalizer_tracks_nothing() {
    // The oracle's stand-in raises ValueError("cannot normalize"); nothing is tracked.
    let expected = oracle("normalize_provenance_failing");
    let manager = ProvenanceManager::with_storage(InMemoryStorage::new());
    let wrapper =
        NormalizerWithProvenance::with_manager(Box::new(Failing), manager.clone(), None, None);
    let error = wrapper
        .normalize(&json!("x"), Some("doc"), &Map::new())
        .unwrap_err();
    assert_eq!(err_json(&error), expected["result"]);
    assert_eq!(
        json!(manager.storage().retrieve_all(None).unwrap().len()),
        expected["entries_after"]
    );
}

#[test]
fn test_text_normalizer_can_be_wrapped() {
    // TextNormalizer.normalize(source, **options) as the wrapped normalizer.
    let case = oracle("normalize")
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["source"] == json!("TEST  Value") && c["kwargs"] == json!({"case": "lower"}))
        .expect("an oracle case");
    let wrapper = NormalizerWithProvenance::with_manager(
        Box::new(TextNormalizer::default()),
        ProvenanceManager::with_storage(InMemoryStorage::new()),
        Some("text-agent"),
        Some(false),
    );
    let result = wrapper
        .normalize(&case["source"], None, &obj(&case["kwargs"]))
        .unwrap();
    assert_eq!(json!({"ok": result}), case["result"]);
    let entries = wrapper
        .prov_manager()
        .unwrap()
        .storage()
        .retrieve_all(None)
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].agent_id, "text-agent");
    assert!(!entries[0].is_automated);
}
