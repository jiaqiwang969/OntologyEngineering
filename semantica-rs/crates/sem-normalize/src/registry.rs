//! Method Registry Module for Normalize (`semantica/normalize/registry.py`).
//!
//! Custom normalization methods registered per task (`text`, `entity`, `date`, `number`,
//! `clean`, `language`, `encoding`). Python's registry is a class attribute shared by every
//! instance; here the registry state is process-wide and [`MethodRegistry`] is a handle to it.

use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sem_utils::SemanticaError;
use serde_json::{Map, Value};

/// A registered normalization method: called with the convenience function's positional
/// arguments and keyword arguments (Python's `method_func(*args, **kwargs)`).
pub type NormalizeMethod =
    Arc<dyn Fn(&[Value], &Map<String, Value>) -> Result<Value, SemanticaError> + Send + Sync>;

const DEFAULT_TASKS: [&str; 7] = [
    "text", "entity", "date", "number", "clean", "language", "encoding",
];

static METHODS: Lazy<Mutex<IndexMap<String, IndexMap<String, NormalizeMethod>>>> =
    Lazy::new(|| {
        Mutex::new(
            DEFAULT_TASKS
                .iter()
                .map(|t| (t.to_string(), IndexMap::new()))
                .collect(),
        )
    });

fn methods() -> MutexGuard<'static, IndexMap<String, IndexMap<String, NormalizeMethod>>> {
    METHODS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Registry for custom normalization methods (`MethodRegistry`, all classmethods).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MethodRegistry;

impl MethodRegistry {
    /// `register(task, name, method_func)`: adds the task when it is new.
    pub fn register(&self, task: &str, name: &str, method_func: NormalizeMethod) {
        methods()
            .entry(task.to_string())
            .or_default()
            .insert(name.to_string(), method_func);
    }

    /// `get(task, name)`.
    pub fn get(&self, task: &str, name: &str) -> Option<NormalizeMethod> {
        methods().get(task).and_then(|m| m.get(name)).cloned()
    }

    /// `list_all(task=None)`: task -> method names (a given task is listed even when
    /// unknown).
    pub fn list_all(&self, task: Option<&str>) -> IndexMap<String, Vec<String>> {
        let methods = methods();
        match task.filter(|t| !t.is_empty()) {
            Some(task) => {
                let names = methods
                    .get(task)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                IndexMap::from([(task.to_string(), names)])
            }
            None => methods
                .iter()
                .map(|(t, m)| (t.clone(), m.keys().cloned().collect()))
                .collect(),
        }
    }

    /// `unregister(task, name)`.
    pub fn unregister(&self, task: &str, name: &str) {
        if let Some(m) = methods().get_mut(task) {
            m.shift_remove(name);
        }
    }

    /// `clear(task=None)`: one task's methods, or every task's.
    pub fn clear(&self, task: Option<&str>) {
        let mut methods = methods();
        match task.filter(|t| !t.is_empty()) {
            Some(task) => {
                if let Some(m) = methods.get_mut(task) {
                    m.clear();
                }
            }
            None => {
                for m in methods.values_mut() {
                    m.clear();
                }
            }
        }
    }
}

/// `method_registry`: the global registry instance.
#[allow(non_upper_case_globals)]
pub static method_registry: MethodRegistry = MethodRegistry;
