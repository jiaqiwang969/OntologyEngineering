//! Method registry for ingestion (`semantica/ingest/registry.py`): custom ingestion methods
//! registered per task ("file", "web", "feed", "stream", "repo", "email", "db", "api",
//! "public_api", "mcp", "parquet", "arrow", "xml", "ingest", ...).
//!
//! As in the original, the registry is process-global class state and the convenience
//! functions of [`crate::methods`] are registered as the defaults when it is first used.

use std::sync::{Arc, RwLock};

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sem_utils::SemanticaError;

use crate::methods::{Builtin, IngestOutput, Source};
use crate::support::kw::Kwargs;

/// A user-supplied ingestion function: `method_func(source, **kwargs)`.
pub type IngestFn = Arc<dyn Fn(&Source, &Kwargs) -> Result<IngestOutput, SemanticaError> + Send + Sync>;

/// A registered method: one of the built-in convenience functions (compared by identity, as
/// `custom_method != ingest_file` does) or a custom function.
#[derive(Clone)]
pub enum RegisteredMethod {
    /// A convenience function of [`crate::methods`].
    Builtin(Builtin),
    /// A custom function.
    Custom(IngestFn),
}

impl std::fmt::Debug for RegisteredMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisteredMethod::Builtin(b) => write!(f, "<function {}>", b.name()),
            RegisteredMethod::Custom(_) => write!(f, "<function custom>"),
        }
    }
}

impl PartialEq for RegisteredMethod {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RegisteredMethod::Builtin(a), RegisteredMethod::Builtin(b)) => a == b,
            (RegisteredMethod::Custom(a), RegisteredMethod::Custom(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl RegisteredMethod {
    /// A custom method from a closure.
    pub fn custom(
        f: impl Fn(&Source, &Kwargs) -> Result<IngestOutput, SemanticaError> + Send + Sync + 'static,
    ) -> RegisteredMethod {
        RegisteredMethod::Custom(Arc::new(f))
    }

    /// `method_func(source, **kwargs)`.
    pub fn call(&self, source: &Source, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
        match self {
            RegisteredMethod::Builtin(b) => b.call(source, kwargs),
            RegisteredMethod::Custom(f) => f(source, kwargs),
        }
    }
}

type Methods = IndexMap<String, IndexMap<String, RegisteredMethod>>;

const TASKS: [&str; 14] = [
    "file",
    "web",
    "feed",
    "stream",
    "repo",
    "email",
    "db",
    "api",
    "public_api",
    "mcp",
    "parquet",
    "arrow",
    "xml",
    "ingest",
];

static METHODS: Lazy<RwLock<Methods>> = Lazy::new(|| {
    let mut methods: Methods = TASKS
        .iter()
        .map(|t| (t.to_string(), IndexMap::new()))
        .collect();
    for (task, name, builtin) in crate::methods::default_registrations() {
        methods
            .entry(task.to_string())
            .or_default()
            .insert(name.to_string(), RegisteredMethod::Builtin(builtin));
    }
    RwLock::new(methods)
});

fn with_read<T>(f: impl FnOnce(&Methods) -> T) -> T {
    match METHODS.read() {
        Ok(m) => f(&m),
        Err(e) => f(&e.into_inner()),
    }
}

fn with_write<T>(f: impl FnOnce(&mut Methods) -> T) -> T {
    match METHODS.write() {
        Ok(mut m) => f(&mut m),
        Err(e) => f(&mut e.into_inner()),
    }
}

/// `MethodRegistry`: registry for custom ingestion methods (class-level state).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MethodRegistry;

impl MethodRegistry {
    /// `register(task, name, method_func)`.
    pub fn register(task: &str, name: &str, method_func: RegisteredMethod) {
        with_write(|m| {
            m.entry(task.to_string())
                .or_default()
                .insert(name.to_string(), method_func);
        });
    }

    /// `get(task, name)`.
    pub fn get(task: &str, name: &str) -> Option<RegisteredMethod> {
        with_read(|m| m.get(task).and_then(|t| t.get(name)).cloned())
    }

    /// `list_all(task=None)`: task -> method names (an unknown task lists no methods).
    pub fn list_all(task: Option<&str>) -> IndexMap<String, Vec<String>> {
        with_read(|m| match task.filter(|t| !t.is_empty()) {
            Some(t) => {
                let mut out = IndexMap::new();
                out.insert(
                    t.to_string(),
                    m.get(t).map(|names| names.keys().cloned().collect()).unwrap_or_default(),
                );
                out
            }
            None => m
                .iter()
                .map(|(t, names)| (t.clone(), names.keys().cloned().collect()))
                .collect(),
        })
    }

    /// `unregister(task, name)`.
    pub fn unregister(task: &str, name: &str) {
        with_write(|m| {
            if let Some(t) = m.get_mut(task) {
                t.shift_remove(name);
            }
        });
    }

    /// `clear(task=None)`: empties one task, or every task (the task keys stay).
    pub fn clear(task: Option<&str>) {
        with_write(|m| match task.filter(|t| !t.is_empty()) {
            Some(t) => {
                if let Some(names) = m.get_mut(t) {
                    names.clear();
                }
            }
            None => {
                for names in m.values_mut() {
                    names.clear();
                }
            }
        });
    }
}

static METHOD_REGISTRY: MethodRegistry = MethodRegistry;

/// `method_registry`: the global registry instance.
pub fn method_registry() -> &'static MethodRegistry {
    &METHOD_REGISTRY
}
