//! Shared helpers for the sem-pipeline tests (the port of the `tests/pipeline` package):
//! the oracle fixture, error/value comparisons in Python terms, and fakes for the
//! collaborators the original tests mock.
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use sem_pipeline::{DeltaTripletStore, KwargValue, Kwargs, StepHandler, VersionManager};
use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::py_eq;
use serde_json::{Map, Value, json};

/// `tests/fixtures/oracle.json`: what the original produced (see `generate_oracle.py`).
pub fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        serde_json::from_str(include_str!("../fixtures/oracle.json")).expect("oracle fixture")
    })
}

/// The oracle value at a dotted path (`"engine.order.calls"`; numbers index arrays).
pub fn expected(path: &str) -> &'static Value {
    let mut value = oracle();
    for part in path.split('.') {
        value = match value {
            Value::Array(items) => &items[part.parse::<usize>().expect("array index")],
            other => other
                .get(part)
                .unwrap_or_else(|| panic!("oracle has no {path} (at {part})")),
        };
    }
    value
}

/// `{"type": type(e).__name__, "message": str(e)}`, as the oracle records errors.
pub fn err_json(e: &SemanticaError) -> Value {
    json!({"type": e.type_name(), "message": e.to_string()})
}

/// Python `==` on JSON-like values (`1 == 1.0`).
pub fn assert_py_eq(actual: &Value, expected: &Value) {
    assert!(
        py_eq(actual, expected),
        "\n  actual: {actual}\nexpected: {expected}"
    );
}

/// `ValueError(message)`.
pub fn value_error(message: &str) -> SemanticaError {
    SemanticaError::Value(message.to_string())
}

/// `RuntimeError(message)`.
pub fn runtime_error(message: &str) -> SemanticaError {
    SemanticaError::Runtime(message.to_string())
}

/// The JSON form of keyword arguments (handlers and objects as their `str()`).
pub fn kwargs_json(map: &Kwargs) -> Value {
    KwargValue::Dict(map.clone()).to_json_default_str()
}

/// A JSON object as keyword arguments.
pub fn kw(value: Value) -> Kwargs {
    sem_pipeline::kwargs_from_json(value)
}

/// The names of `items` in a set `repr` (`{'a', 'b'}`), sorted.
pub fn set_repr_items(text: &str) -> Vec<String> {
    let inner = text
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}');
    let mut items: Vec<String> = inner
        .split(", ")
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_matches('\'').to_string())
        .collect();
    items.sort();
    items
}

/// A handler that logs `[name, list(kwargs)]` and appends `name` to the list it receives
/// (`(data or []) + [name]`).
pub fn recording_handler(name: &str, log: &Arc<Mutex<Vec<Value>>>) -> StepHandler {
    let log = Arc::clone(log);
    let step = name.to_string();
    StepHandler::new(name, move |data, kwargs| {
        let keys: Vec<&String> = kwargs.keys().collect();
        log.lock().unwrap().push(json!([step, keys]));
        let mut items = match data {
            Value::Array(items) => items,
            _ => Vec::new(),
        };
        items.push(Value::from(step.clone()));
        Ok(Value::Array(items))
    })
}

/// A handler that fails with `ValueError(messages[i])` on call `i`, then returns `then`
/// (MagicMock's `side_effect` list).
pub fn side_effect_handler(
    name: &str,
    failures: &[SemanticaError],
    then: Value,
    calls: &Arc<AtomicUsize>,
) -> StepHandler {
    let failures: Vec<SemanticaError> = failures.to_vec();
    let calls = Arc::clone(calls);
    StepHandler::new(name, move |_, _| {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        match failures.get(n) {
            Some(error) => Err(error.clone()),
            None => Ok(then.clone()),
        }
    })
}

/// A version manager returning fixed snapshots and recording the requested ids.
pub struct FakeVersionManager {
    snapshots: Map<String, Value>,
    pub calls: Mutex<Vec<String>>,
}

impl FakeVersionManager {
    pub fn new(snapshots: Value) -> Arc<Self> {
        Arc::new(FakeVersionManager {
            snapshots: snapshots.as_object().cloned().unwrap_or_default(),
            calls: Mutex::new(Vec::new()),
        })
    }
}

impl VersionManager for FakeVersionManager {
    fn get_version(&self, version_id: &str) -> Result<Option<Map<String, Value>>, SemanticaError> {
        self.calls.lock().unwrap().push(version_id.to_string());
        Ok(self
            .snapshots
            .get(version_id)
            .and_then(Value::as_object)
            .cloned())
    }
}

/// A triplet store whose `compute_delta` returns `payload` (or, without one, a summary of
/// its arguments) and records `[old, new, sorted(option names)]` plus the options.
pub struct FakeTripletStore {
    payload: Option<Value>,
    pub calls: Mutex<Vec<Value>>,
    pub options: Mutex<Vec<Kwargs>>,
}

impl FakeTripletStore {
    pub fn new(payload: Option<Value>) -> Arc<Self> {
        Arc::new(FakeTripletStore {
            payload,
            calls: Mutex::new(Vec::new()),
            options: Mutex::new(Vec::new()),
        })
    }
}

impl DeltaTripletStore for FakeTripletStore {
    fn compute_delta(
        &self,
        old_graph_uri: &str,
        new_graph_uri: &str,
        options: &Kwargs,
    ) -> Result<Value, SemanticaError> {
        let mut names: Vec<&String> = options.keys().collect();
        names.sort();
        self.calls
            .lock()
            .unwrap()
            .push(json!([old_graph_uri, new_graph_uri, names]));
        self.options.lock().unwrap().push(options.clone());
        Ok(self.payload.clone().unwrap_or_else(|| {
            json!({"old_graph_uri": old_graph_uri, "new_graph_uri": new_graph_uri, "added_count": 1})
        }))
    }
}

/// `as Arc<dyn VersionManager>` for passing a fake as an option.
pub fn vm_value(vm: &Arc<FakeVersionManager>) -> KwargValue {
    KwargValue::from(Arc::clone(vm) as Arc<dyn VersionManager>)
}

/// `as Arc<dyn DeltaTripletStore>` for passing a fake as an option.
pub fn ts_value(ts: &Arc<FakeTripletStore>) -> KwargValue {
    KwargValue::from(Arc::clone(ts) as Arc<dyn DeltaTripletStore>)
}
