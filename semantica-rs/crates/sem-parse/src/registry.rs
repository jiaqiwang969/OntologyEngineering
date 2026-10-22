//! Method registry for custom parsing methods (`semantica/parse/registry.py`).
//!
//! Registers custom parsing methods per task (`"document"`, `"web"`, `"structured"`,
//! `"email"`, `"code"`, `"media"`, `"mcp"`) so the convenience functions in
//! [`crate::methods`] can dispatch to them by name.
//!
//! As in the original, the registry is class-level state: every [`MethodRegistry`] value
//! (including the global [`method_registry`]) reads and writes the same table. The table
//! starts with the methods `semantica.parse.methods` registers when it is imported
//! (`"docling"` and `"default"` for documents, `"default"` for the other tasks).
//!
//! A registered method is a [`ParseMethod`]: a callable receiving the positional arguments
//! the convenience function passes (as [`MethodArg`]s) and its keyword arguments.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use once_cell::sync::Lazy;

use crate::common::{Options, PResult, Source};
use crate::methods::{self, ParseOutput};

/// A positional argument passed to a registered method (Python passes `str`, `Path`,
/// `bytes` or `None` values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodArg {
    /// `None`.
    None,
    /// A `str`.
    Str(String),
    /// A `pathlib.Path`.
    Path(PathBuf),
    /// A `bytes` object.
    Bytes(Vec<u8>),
}

impl MethodArg {
    /// The argument as a [`Source`] (`None`/bytes are not paths or text).
    pub fn as_source(&self) -> Option<Source> {
        match self {
            MethodArg::Str(s) => Some(Source::Text(s.clone())),
            MethodArg::Path(p) => Some(Source::Path(p.clone())),
            _ => None,
        }
    }

    /// The argument as an optional string (`None` for `None`).
    pub fn as_opt_str(&self) -> Option<String> {
        match self {
            MethodArg::Str(s) => Some(s.clone()),
            MethodArg::Path(p) => Some(crate::common::py_path_str(p)),
            _ => None,
        }
    }
}

impl From<&Source> for MethodArg {
    fn from(value: &Source) -> Self {
        match value {
            Source::Text(s) => MethodArg::Str(s.clone()),
            Source::Path(p) => MethodArg::Path(p.clone()),
        }
    }
}

impl From<Option<&str>> for MethodArg {
    fn from(value: Option<&str>) -> Self {
        match value {
            Some(s) => MethodArg::Str(s.to_string()),
            None => MethodArg::None,
        }
    }
}

impl From<&str> for MethodArg {
    fn from(value: &str) -> Self {
        MethodArg::Str(value.to_string())
    }
}

/// A registered parsing method: `method(*args, **kwargs)`.
pub type ParseMethod =
    Arc<dyn Fn(&[MethodArg], &Options) -> PResult<ParseOutput> + Send + Sync + 'static>;

/// Wrap a closure as a [`ParseMethod`].
pub fn parse_method<F>(f: F) -> ParseMethod
where
    F: Fn(&[MethodArg], &Options) -> PResult<ParseOutput> + Send + Sync + 'static,
{
    Arc::new(f)
}

type Table = IndexMap<String, IndexMap<String, ParseMethod>>;

/// The class-level `_methods` table, initialised with the default task keys and the methods
/// `methods.py` registers at import time.
static METHODS: Lazy<Mutex<Table>> = Lazy::new(|| {
    let mut table: Table = IndexMap::new();
    for task in [
        "document",
        "web",
        "structured",
        "email",
        "code",
        "media",
        "mcp",
    ] {
        table.insert(task.to_string(), IndexMap::new());
    }
    for (task, name, method) in methods::import_time_registrations() {
        table
            .entry(task.to_string())
            .or_default()
            .insert(name.to_string(), method);
    }
    Mutex::new(table)
});

fn table() -> std::sync::MutexGuard<'static, Table> {
    METHODS.lock().unwrap_or_else(|e| e.into_inner())
}

/// `MethodRegistry`: registry for custom parsing methods. All instances share one table
/// (the original's methods are classmethods over a class attribute).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct MethodRegistry;

impl fmt::Debug for MethodRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.list_all(None).iter()).finish()
    }
}

impl MethodRegistry {
    /// `MethodRegistry()`.
    pub fn new() -> Self {
        MethodRegistry
    }

    /// `register(task, name, method_func)`: register (or replace) a method; unknown tasks
    /// are created.
    pub fn register(&self, task: &str, name: &str, method_func: ParseMethod) {
        table()
            .entry(task.to_string())
            .or_default()
            .insert(name.to_string(), method_func);
    }

    /// `get(task, name)`: the method, or `None`.
    pub fn get(&self, task: &str, name: &str) -> Option<ParseMethod> {
        table().get(task).and_then(|m| m.get(name)).cloned()
    }

    /// `list_all(task=None)`: task -> method names. With a (non-empty) task, only that task
    /// is listed (an unknown task lists no names).
    pub fn list_all(&self, task: Option<&str>) -> IndexMap<String, Vec<String>> {
        let table = table();
        match task.filter(|t| !t.is_empty()) {
            Some(task) => {
                let names = table
                    .get(task)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                IndexMap::from([(task.to_string(), names)])
            }
            None => table
                .iter()
                .map(|(t, m)| (t.clone(), m.keys().cloned().collect()))
                .collect(),
        }
    }

    /// `unregister(task, name)`: remove a method if present.
    pub fn unregister(&self, task: &str, name: &str) {
        if let Some(methods) = table().get_mut(task) {
            methods.shift_remove(name);
        }
    }

    /// `clear(task=None)`: remove every method of one task, or of all tasks (the task keys
    /// themselves remain).
    pub fn clear(&self, task: Option<&str>) {
        let mut table = table();
        match task.filter(|t| !t.is_empty()) {
            Some(task) => {
                if let Some(methods) = table.get_mut(task) {
                    methods.clear();
                }
            }
            None => {
                for methods in table.values_mut() {
                    methods.clear();
                }
            }
        }
    }
}

/// `method_registry`: the global registry instance.
#[allow(non_upper_case_globals)]
pub static method_registry: MethodRegistry = MethodRegistry;
