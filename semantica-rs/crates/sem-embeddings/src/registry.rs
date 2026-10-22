//! Method Registry Module for Embeddings (port of `semantica/embeddings/registry.py`).
//!
//! A registry of custom embedding methods by task (`"generation"`, `"text"`, `"image"`,
//! `"audio"`, `"multimodal"`, `"optimization"`, `"pooling"`, `"provider"`, `"similarity"`),
//! enabling extensions without changing the module.
//!
//! As in Python the storage is class-level: every [`MethodRegistry`] value (and the global
//! [`method_registry`]) shares one process-wide table. Importing the original package also
//! imports `methods.py`, which registers the convenience functions themselves; the table
//! therefore starts with those registrations ([`Method::Builtin`]).
//!
//! ```no_run
//! use sem_embeddings::registry::{Method, method_registry};
//! use sem_embeddings::NdArray;
//!
//! method_registry.register(
//!     "pooling",
//!     "sum",
//!     Method::pooling(|embeddings: &NdArray, _kwargs| Ok(embeddings.clone())),
//! );
//! let available = method_registry.list_all(Some("pooling"));
//! # let _ = available;
//! ```

use std::any::Any;
use std::fmt;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sem_utils::SemanticaError;
use serde_json::{Map, Value};

use crate::methods::EmbeddingInput;
use crate::ndarray::NdArray;

/// A custom `"generation"` method: `method(data, data_type=..., **kwargs)`.
pub type GenerationFn = Arc<
    dyn Fn(&EmbeddingInput, Option<&str>, &Map<String, Value>) -> Result<NdArray, SemanticaError>
        + Send
        + Sync,
>;
/// A custom `"text"` method: `method(text, **kwargs)`.
pub type TextFn =
    Arc<dyn Fn(&EmbeddingInput, &Map<String, Value>) -> Result<NdArray, SemanticaError> + Send + Sync>;
/// A custom `"similarity"` method: `method(embedding1, embedding2, **kwargs)`.
pub type SimilarityFn = Arc<
    dyn Fn(&NdArray, &NdArray, &Map<String, Value>) -> Result<f64, SemanticaError> + Send + Sync,
>;
/// A custom `"pooling"` method: `method(embeddings, **kwargs)`.
pub type PoolingFn =
    Arc<dyn Fn(&NdArray, &Map<String, Value>) -> Result<NdArray, SemanticaError> + Send + Sync>;

/// The convenience functions of `methods.py`, which register themselves at import.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinMethod {
    /// `methods.generate_embeddings`.
    GenerateEmbeddings,
    /// `methods.embed_text`.
    EmbedText,
    /// `methods.calculate_similarity`.
    CalculateSimilarity,
    /// `methods.pool_embeddings`.
    PoolEmbeddings,
}

impl BuiltinMethod {
    /// The function's name in `methods.py`.
    pub fn name(self) -> &'static str {
        match self {
            BuiltinMethod::GenerateEmbeddings => "generate_embeddings",
            BuiltinMethod::EmbedText => "embed_text",
            BuiltinMethod::CalculateSimilarity => "calculate_similarity",
            BuiltinMethod::PoolEmbeddings => "pool_embeddings",
        }
    }
}

/// A registered method (the Python callable). Python calls whatever is registered with the
/// arguments of the task that looks it up; a method of another shape fails like a call with
/// the wrong signature (a `TypeError`).
#[derive(Clone)]
pub enum Method {
    /// A `"generation"`-shaped function.
    Generation(GenerationFn),
    /// A `"text"`-shaped function.
    Text(TextFn),
    /// A `"similarity"`-shaped function.
    Similarity(SimilarityFn),
    /// A `"pooling"`-shaped function.
    Pooling(PoolingFn),
    /// One of the `methods.py` functions.
    Builtin(BuiltinMethod),
    /// Any other object (for tasks this module never calls, such as `"image"`).
    Other(Arc<dyn Any + Send + Sync>),
}

impl Method {
    /// Wrap a generation function.
    pub fn generation<F>(f: F) -> Method
    where
        F: Fn(&EmbeddingInput, Option<&str>, &Map<String, Value>) -> Result<NdArray, SemanticaError>
            + Send
            + Sync
            + 'static,
    {
        Method::Generation(Arc::new(f))
    }

    /// Wrap a text-embedding function.
    pub fn text<F>(f: F) -> Method
    where
        F: Fn(&EmbeddingInput, &Map<String, Value>) -> Result<NdArray, SemanticaError>
            + Send
            + Sync
            + 'static,
    {
        Method::Text(Arc::new(f))
    }

    /// Wrap a similarity function.
    pub fn similarity<F>(f: F) -> Method
    where
        F: Fn(&NdArray, &NdArray, &Map<String, Value>) -> Result<f64, SemanticaError>
            + Send
            + Sync
            + 'static,
    {
        Method::Similarity(Arc::new(f))
    }

    /// Wrap a pooling function.
    pub fn pooling<F>(f: F) -> Method
    where
        F: Fn(&NdArray, &Map<String, Value>) -> Result<NdArray, SemanticaError>
            + Send
            + Sync
            + 'static,
    {
        Method::Pooling(Arc::new(f))
    }

    /// Wrap any other object.
    pub fn other<T: Any + Send + Sync>(value: T) -> Method {
        Method::Other(Arc::new(value))
    }

    /// Whether this is the given `methods.py` function.
    pub fn is_builtin(&self, builtin: BuiltinMethod) -> bool {
        matches!(self, Method::Builtin(b) if *b == builtin)
    }

    fn kind(&self) -> &'static str {
        match self {
            Method::Generation(_) => "generation",
            Method::Text(_) => "text",
            Method::Similarity(_) => "similarity",
            Method::Pooling(_) => "pooling",
            Method::Builtin(b) => b.name(),
            Method::Other(_) => "object",
        }
    }

    fn wrong_signature(&self, task: &str) -> SemanticaError {
        SemanticaError::Type(format!(
            "registered {} method cannot be called as a {task} method",
            self.kind()
        ))
    }

    /// Call as a `"generation"` method: `method(data, data_type=data_type, **kwargs)`.
    /// A registered `methods.py` function is called with its own default `method`.
    pub fn call_generation(
        &self,
        data: &EmbeddingInput,
        data_type: Option<&str>,
        kwargs: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        match self {
            Method::Generation(f) => f(data, data_type, kwargs),
            Method::Builtin(BuiltinMethod::GenerateEmbeddings) => {
                crate::methods::generate_embeddings(data, data_type, None, kwargs)
            }
            other => Err(other.wrong_signature("generation")),
        }
    }

    /// Call as a `"text"` method: `method(text, **kwargs)`.
    pub fn call_text(
        &self,
        text: &EmbeddingInput,
        kwargs: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        match self {
            Method::Text(f) => f(text, kwargs),
            Method::Builtin(BuiltinMethod::EmbedText) => {
                crate::methods::embed_text(text, None, kwargs)
            }
            other => Err(other.wrong_signature("text")),
        }
    }

    /// Call as a `"similarity"` method: `method(embedding1, embedding2, **kwargs)`.
    pub fn call_similarity(
        &self,
        embedding1: &NdArray,
        embedding2: &NdArray,
        kwargs: &Map<String, Value>,
    ) -> Result<f64, SemanticaError> {
        match self {
            Method::Similarity(f) => f(embedding1, embedding2, kwargs),
            Method::Builtin(BuiltinMethod::CalculateSimilarity) => {
                crate::methods::calculate_similarity(embedding1, embedding2, None, kwargs)
            }
            other => Err(other.wrong_signature("similarity")),
        }
    }

    /// Call as a `"pooling"` method: `method(embeddings, **kwargs)`.
    pub fn call_pooling(
        &self,
        embeddings: &NdArray,
        kwargs: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        match self {
            Method::Pooling(f) => f(embeddings, kwargs),
            Method::Builtin(BuiltinMethod::PoolEmbeddings) => {
                crate::methods::pool_embeddings(embeddings, None, kwargs)
            }
            other => Err(other.wrong_signature("pooling")),
        }
    }
}

impl fmt::Debug for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::Builtin(b) => write!(f, "Method::Builtin({})", b.name()),
            other => write!(f, "Method::{}(..)", other.kind()),
        }
    }
}

/// The task types the registry starts with, in the original's order.
pub const DEFAULT_TASKS: [&str; 9] = [
    "generation",
    "text",
    "image",
    "audio",
    "multimodal",
    "optimization",
    "pooling",
    "provider",
    "similarity",
];

type Table = IndexMap<String, IndexMap<String, Method>>;

/// `MethodRegistry._methods`, with the registrations `methods.py` makes at import.
static METHODS: Lazy<RwLock<Table>> = Lazy::new(|| {
    let mut table: Table = DEFAULT_TASKS
        .iter()
        .map(|t| (t.to_string(), IndexMap::new()))
        .collect();
    let defaults: [(&str, &str, BuiltinMethod); 12] = [
        ("generation", "default", BuiltinMethod::GenerateEmbeddings),
        ("generation", "text", BuiltinMethod::GenerateEmbeddings),
        ("text", "sentence_transformers", BuiltinMethod::EmbedText),
        ("text", "fastembed", BuiltinMethod::EmbedText),
        ("text", "fallback", BuiltinMethod::EmbedText),
        ("similarity", "cosine", BuiltinMethod::CalculateSimilarity),
        ("similarity", "euclidean", BuiltinMethod::CalculateSimilarity),
        ("pooling", "mean", BuiltinMethod::PoolEmbeddings),
        ("pooling", "max", BuiltinMethod::PoolEmbeddings),
        ("pooling", "cls", BuiltinMethod::PoolEmbeddings),
        ("pooling", "attention", BuiltinMethod::PoolEmbeddings),
        ("pooling", "hierarchical", BuiltinMethod::PoolEmbeddings),
    ];
    for (task, name, builtin) in defaults {
        table
            .entry(task.to_string())
            .or_default()
            .insert(name.to_string(), Method::Builtin(builtin));
    }
    RwLock::new(table)
});

fn read() -> RwLockReadGuard<'static, Table> {
    METHODS.read().unwrap_or_else(|e| e.into_inner())
}

fn write() -> RwLockWriteGuard<'static, Table> {
    METHODS.write().unwrap_or_else(|e| e.into_inner())
}

/// Registry for custom embedding methods.
///
/// All values share the class-level table, so `MethodRegistry` is a unit handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MethodRegistry;

impl MethodRegistry {
    /// `MethodRegistry()`.
    pub const fn new() -> MethodRegistry {
        MethodRegistry
    }

    /// `register(task, name, method_func)`: register a custom embedding method (a new task
    /// type is created on demand).
    pub fn register(&self, task: &str, name: &str, method_func: Method) {
        write()
            .entry(task.to_string())
            .or_default()
            .insert(name.to_string(), method_func);
    }

    /// `get(task, name)`: the method, or `None`.
    pub fn get(&self, task: &str, name: &str) -> Option<Method> {
        read().get(task).and_then(|m| m.get(name)).cloned()
    }

    /// `list_all(task=None)`: task type -> registered method names (only `task` when given and
    /// non-empty; an unknown task lists no names).
    pub fn list_all(&self, task: Option<&str>) -> IndexMap<String, Vec<String>> {
        let table = read();
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

    /// `unregister(task, name)`.
    pub fn unregister(&self, task: &str, name: &str) {
        if let Some(methods) = write().get_mut(task) {
            methods.shift_remove(name);
        }
    }

    /// `clear(task=None)`: remove the methods of one task, or of every task (task types stay).
    pub fn clear(&self, task: Option<&str>) {
        let mut table = write();
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

/// The global registry (`method_registry`).
#[allow(non_upper_case_globals)]
pub static method_registry: MethodRegistry = MethodRegistry;
