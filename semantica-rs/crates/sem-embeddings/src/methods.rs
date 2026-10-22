//! Embedding Methods Module (port of `semantica/embeddings/methods.py`).
//!
//! Convenience functions for embedding generation, text embedding, similarity and pooling,
//! with registry dispatch: each function first looks for a method registered under its task
//! and name, calls it, and falls back to the built-in implementation when that fails.
//!
//! **Registry dispatch, as the original behaves.** `methods.py` registers the functions
//! themselves as the default methods (`generation`: `default`, `text`; `text`:
//! `sentence_transformers`, `fastembed`, `fallback`; `similarity`: `cosine`, `euclidean`;
//! `pooling`: `mean`, `max`, `cls`, `attention`, `hierarchical`). Looking up e.g.
//! `pooling/max` therefore finds `pool_embeddings` and calls it *without* `method`, i.e. with
//! its default `"mean"`; that call finds itself again and recurses until Python's recursion
//! limit, after which the innermost calls run the default branch. The net effect, reproduced
//! here directly (without the recursion and the warnings it logs), is:
//!
//! - a registered built-in method runs the function's *default* method (`generate_embeddings`:
//!   `"default"`, `embed_text`: `"sentence_transformers"`, `calculate_similarity`: `"cosine"`,
//!   `pool_embeddings`: `"mean"`), so `pool_embeddings(x, "max")` returns the mean and
//!   `calculate_similarity(a, b, "euclidean")` the cosine similarity;
//! - only when that default fails does the requested method's own branch run (and its error is
//!   the one raised).
//!
//! ```no_run
//! use sem_embeddings::{NdArray, calculate_similarity, embed_text};
//! use serde_json::Map;
//!
//! let a = embed_text(&"Hello world".into(), Some("sentence_transformers"), &Map::new())?;
//! let b = embed_text(&"Hi there".into(), Some("fastembed"), &Map::new())?;
//! let similarity = calculate_similarity(&a, &b, Some("cosine"), &Map::new())?;
//! # let _ = (similarity, NdArray::empty());
//! # Ok::<(), sem_utils::SemanticaError>(())
//! ```

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::type_name;
use sem_utils::{ProcessingError, SemanticaError, get_logger};
use serde_json::{Map, Value};

use crate::config::embeddings_config;
use crate::embedding_generator::EmbeddingGenerator;
pub use crate::embedding_generator::{DataItem, EmbeddingInput, ObjectContent};
use crate::ndarray::NdArray;
use crate::pooling_strategies::PoolingStrategyFactory;
use crate::registry::{BuiltinMethod, Method, method_registry};
use crate::text_embedder::{TextEmbedder, current_backends};

static LOGGER: Lazy<Logger> = Lazy::new(|| get_logger("embeddings_methods"));

/// The `embeddings_methods` logger.
pub fn logger() -> &'static Logger {
    &LOGGER
}

fn warn_custom_failed(method: &str, error: &SemanticaError) {
    LOGGER.warning(format!(
        "Custom method {method} failed: {error}, falling back to default"
    ));
}

/// Resolve a registered method the way Python's call chain resolves it (see the module docs):
/// `Some(result)` when the registered method produced the answer (or the final error of a
/// self-registration), `None` when the function's own branch for `method` must run.
fn dispatch_custom<T>(
    task: &str,
    method: &str,
    builtin: BuiltinMethod,
    default_method: &str,
    default_branch: impl Fn(&str) -> Result<T, SemanticaError>,
    call_default: impl Fn() -> Result<T, SemanticaError>,
    call_custom: impl Fn(&Method) -> Result<T, SemanticaError>,
) -> Option<Result<T, SemanticaError>> {
    let custom = method_registry.get(task, method)?;
    let outcome = if custom.is_builtin(builtin) {
        if method == default_method {
            // The function registered under its own default name: Python recurses until
            // RecursionError and the innermost frames run the default branch.
            return Some(default_branch(default_method));
        }
        call_default()
    } else {
        call_custom(&custom)
    };
    match outcome {
        Ok(value) => Some(Ok(value)),
        Err(e) => {
            warn_custom_failed(method, &e);
            None
        }
    }
}

/// `generate_embeddings(data, data_type=None, method="default", **kwargs)`.
///
/// `"default"` builds `EmbeddingGenerator(**kwargs)` and calls its `generate_embeddings`;
/// `"text"` calls [`embed_text`]; other names must be registered generation methods.
pub fn generate_embeddings(
    data: &EmbeddingInput,
    data_type: Option<&str>,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<NdArray, SemanticaError> {
    let method = method.unwrap_or("default");
    let branch = |m: &str| generation_branch(data, data_type, m, kwargs);
    if let Some(result) = dispatch_custom(
        "generation",
        method,
        BuiltinMethod::GenerateEmbeddings,
        "default",
        branch,
        || generate_embeddings(data, data_type, None, kwargs),
        |custom| custom.call_generation(data, data_type, kwargs),
    ) {
        return result;
    }
    branch(method)
}

fn generation_branch(
    data: &EmbeddingInput,
    data_type: Option<&str>,
    method: &str,
    kwargs: &Map<String, Value>,
) -> Result<NdArray, SemanticaError> {
    let result = match method {
        "default" => EmbeddingGenerator::from_kwargs(kwargs)
            .and_then(|generator| generator.generate_embeddings(data, data_type, kwargs)),
        "text" => embed_text(data, None, kwargs),
        other => Err(ProcessingError::new(format!("Unknown generation method: {other}")).into()),
    };
    if let Err(e) = &result {
        LOGGER.error(format!("Failed to generate embeddings: {e}"));
    }
    result
}

/// `embed_text(text, method="sentence_transformers", **kwargs)`.
///
/// Builds `TextEmbedder(**config)` from the global `"text"` method configuration updated
/// with `kwargs` (when that configuration exists it is updated in place, as the original
/// mutates the dict it gets back), with `method` set for `"fastembed"` (default model
/// `BAAI/bge-small-en-v1.5`) and `"sentence_transformers"`.
pub fn embed_text(
    text: &EmbeddingInput,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<NdArray, SemanticaError> {
    let method = method.unwrap_or("sentence_transformers");
    let branch = |m: &str| embed_text_branch(text, m, kwargs);
    if let Some(result) = dispatch_custom(
        "text",
        method,
        BuiltinMethod::EmbedText,
        "sentence_transformers",
        branch,
        || embed_text(text, None, kwargs),
        |custom| custom.call_text(text, kwargs),
    ) {
        return result;
    }
    branch(method)
}

fn batch_texts(items: &[DataItem]) -> Result<Vec<String>, SemanticaError> {
    items
        .iter()
        .map(|item| match item {
            DataItem::Text(s) | DataItem::Value(Value::String(s)) => Ok(s.clone()),
            DataItem::Object { .. } => Err(SemanticaError::Attribute(
                "'FileObject' object has no attribute 'encode'".to_string(),
            )),
            DataItem::Value(other) => Err(SemanticaError::Attribute(format!(
                "'{}' object has no attribute 'encode'",
                type_name(other)
            ))),
        })
        .collect()
}

fn embed_text_branch(
    text: &EmbeddingInput,
    method: &str,
    kwargs: &Map<String, Value>,
) -> Result<NdArray, SemanticaError> {
    let result = (|| {
        let config = embeddings_config.with_method_config("text", |config| {
            for (k, v) in kwargs {
                config.insert(k.clone(), v.clone());
            }
            if method == "fastembed" {
                config.insert("method".to_string(), Value::from("fastembed"));
                if !config.contains_key("model_name") {
                    config.insert(
                        "model_name".to_string(),
                        Value::from("BAAI/bge-small-en-v1.5"),
                    );
                }
            } else if method == "sentence_transformers" {
                config.insert("method".to_string(), Value::from("sentence_transformers"));
            }
            config.clone()
        });
        let embedder = TextEmbedder::from_kwargs(&config)?;
        match text {
            EmbeddingInput::Batch(items) => {
                if items.is_empty() {
                    return embedder.embed_batch(&[], kwargs);
                }
                embedder.embed_batch(&batch_texts(items)?, kwargs)
            }
            EmbeddingInput::Text(t) => embedder.embed_text(t, kwargs),
        }
    })();
    if let Err(e) = &result {
        LOGGER.error(format!("Failed to embed text: {e}"));
    }
    result
}

/// `calculate_similarity(embedding1, embedding2, method="cosine", **kwargs)`: builds
/// `EmbeddingGenerator(**kwargs)` and calls `compare_embeddings`.
pub fn calculate_similarity(
    embedding1: &NdArray,
    embedding2: &NdArray,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<f64, SemanticaError> {
    let method = method.unwrap_or("cosine");
    let branch = |m: &str| similarity_branch(embedding1, embedding2, m, kwargs);
    if let Some(result) = dispatch_custom(
        "similarity",
        method,
        BuiltinMethod::CalculateSimilarity,
        "cosine",
        branch,
        || calculate_similarity(embedding1, embedding2, None, kwargs),
        |custom| custom.call_similarity(embedding1, embedding2, kwargs),
    ) {
        return result;
    }
    branch(method)
}

fn similarity_branch(
    embedding1: &NdArray,
    embedding2: &NdArray,
    method: &str,
    kwargs: &Map<String, Value>,
) -> Result<f64, SemanticaError> {
    let result = EmbeddingGenerator::from_kwargs(kwargs).and_then(|generator| {
        let mut options = kwargs.clone();
        options.insert("method".to_string(), Value::from(method));
        generator.compare_embeddings(embedding1, embedding2, &options)
    });
    if let Err(e) = &result {
        LOGGER.error(format!("Failed to calculate similarity: {e}"));
    }
    result
}

/// `pool_embeddings(embeddings, method="mean", **kwargs)`: `PoolingStrategyFactory.create(
/// method, **kwargs).pool(embeddings, **kwargs)`.
pub fn pool_embeddings(
    embeddings: &NdArray,
    method: Option<&str>,
    kwargs: &Map<String, Value>,
) -> Result<NdArray, SemanticaError> {
    let method = method.unwrap_or("mean");
    let branch = |m: &str| pooling_branch(embeddings, m, kwargs);
    if let Some(result) = dispatch_custom(
        "pooling",
        method,
        BuiltinMethod::PoolEmbeddings,
        "mean",
        branch,
        || pool_embeddings(embeddings, None, kwargs),
        |custom| custom.call_pooling(embeddings, kwargs),
    ) {
        return result;
    }
    branch(method)
}

fn pooling_branch(
    embeddings: &NdArray,
    method: &str,
    kwargs: &Map<String, Value>,
) -> Result<NdArray, SemanticaError> {
    let result = PoolingStrategyFactory::create(method, kwargs)
        .and_then(|strategy| strategy.pool(embeddings, kwargs));
    if let Err(e) = &result {
        LOGGER.error(format!("Failed to pool embeddings: {e}"));
    }
    result
}

/// `get_embedding_method(task, name)`: the registered method, or `None`.
pub fn get_embedding_method(task: &str, name: &str) -> Option<Method> {
    method_registry.get(task, name)
}

/// `list_available_methods(task=None)`: task type -> registered method names.
pub fn list_available_methods(task: Option<&str>) -> IndexMap<String, Vec<String>> {
    method_registry.list_all(task)
}

/// `check_available_providers()`: which embedding libraries are available
/// (`sentence_transformers`, `fastembed` from the backends in effect, `openai` always).
pub fn check_available_providers() -> IndexMap<String, bool> {
    let backends = current_backends();
    IndexMap::from([
        (
            "sentence_transformers".to_string(),
            backends.sentence_transformers_available,
        ),
        ("fastembed".to_string(), backends.fastembed_available),
        ("openai".to_string(), true),
    ])
}
