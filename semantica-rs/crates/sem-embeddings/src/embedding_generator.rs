//! Embedding Generation Module (port of `semantica/embeddings/embedding_generator.py`).
//!
//! A unified interface for generating text embeddings with the configured models, batch
//! processing and similarity comparison.
//!
//! ```no_run
//! use sem_embeddings::{EmbeddingGenerator, EmbeddingInput};
//! use serde_json::Map;
//!
//! let generator = EmbeddingGenerator::new(None)?;
//! let emb = generator.generate_embeddings(&"Hello world".into(), Some("text"), &Map::new())?;
//! let embs = generator.process_batch(&["text1".to_string(), "text2".to_string()], None, &Map::new());
//! # let _ = (emb, embs, EmbeddingInput::from("x"));
//! # Ok::<(), sem_utils::SemanticaError>(())
//! ```

use std::fmt;
use std::sync::Arc;

use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::{py_str, truthy, type_name};
use sem_utils::{ProcessingError, SemanticaError, get_logger, get_progress_tracker};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::ndarray::{DType, Float, NdArray, shape_repr, vector_norm, with_float};
use crate::text_embedder::{ModelInfo, TextEmbedder};

/// The content of an object with a `content` attribute (such as a `FileObject`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectContent {
    /// `bytes` content (decoded as UTF-8; undecodable content becomes `""`).
    Bytes(Vec<u8>),
    /// Any other content, as its `str()`.
    Text(String),
}

/// One element of a batch passed to `generate_embeddings`.
#[derive(Clone, Debug, PartialEq)]
pub enum DataItem {
    /// A `str`.
    Text(String),
    /// An object with a `content` attribute (a `FileObject` or similar). `display` is the
    /// object's `str()`, used when the content is empty or missing.
    Object {
        /// `item.content`.
        content: Option<ObjectContent>,
        /// `str(item)`.
        display: String,
    },
    /// Any other value; a dict with a `"content"` key contributes `str(item["content"])`,
    /// anything else its `str()`.
    Value(Value),
}

impl DataItem {
    /// The text the original extracts from the item.
    pub fn to_text(&self) -> String {
        match self {
            DataItem::Text(s) => s.clone(),
            DataItem::Object { content, display } => match content {
                Some(ObjectContent::Bytes(b)) if !b.is_empty() => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
                Some(ObjectContent::Text(t)) if !t.is_empty() => t.clone(),
                _ => display.clone(),
            },
            DataItem::Value(Value::String(s)) => s.clone(),
            DataItem::Value(Value::Object(map)) if map.contains_key("content") => {
                py_str(&map["content"])
            }
            DataItem::Value(other) => py_str(other),
        }
    }
}

impl From<&str> for DataItem {
    fn from(s: &str) -> Self {
        DataItem::Text(s.to_string())
    }
}

impl From<String> for DataItem {
    fn from(s: String) -> Self {
        DataItem::Text(s)
    }
}

impl From<Value> for DataItem {
    fn from(v: Value) -> Self {
        match v {
            Value::String(s) => DataItem::Text(s),
            other => DataItem::Value(other),
        }
    }
}

/// What `generate_embeddings` / `embed_text` accept: one text or a batch.
#[derive(Clone, Debug, PartialEq)]
pub enum EmbeddingInput {
    /// A `str`: the result is a 1-D embedding.
    Text(String),
    /// A list: the result is a 2-D array, one row per item.
    Batch(Vec<DataItem>),
}

impl EmbeddingInput {
    /// A batch of texts.
    pub fn texts<I, S>(texts: I) -> EmbeddingInput
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        EmbeddingInput::Batch(texts.into_iter().map(|s| DataItem::Text(s.into())).collect())
    }
}

impl From<&str> for EmbeddingInput {
    fn from(s: &str) -> Self {
        EmbeddingInput::Text(s.to_string())
    }
}

impl From<String> for EmbeddingInput {
    fn from(s: String) -> Self {
        EmbeddingInput::Text(s)
    }
}

impl From<Vec<String>> for EmbeddingInput {
    fn from(texts: Vec<String>) -> Self {
        EmbeddingInput::texts(texts)
    }
}

impl From<Vec<&str>> for EmbeddingInput {
    fn from(texts: Vec<&str>) -> Self {
        EmbeddingInput::texts(texts)
    }
}

impl From<Vec<DataItem>> for EmbeddingInput {
    fn from(items: Vec<DataItem>) -> Self {
        EmbeddingInput::Batch(items)
    }
}

/// `get_methods_info()`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MethodsInfo {
    /// The text embedder's `get_model_info()`.
    pub text: ModelInfo,
}

/// One failed item of `process_batch`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FailedItem {
    /// `str(item)`.
    pub item: String,
    /// `str(error)`.
    pub error: String,
}

/// The result of `process_batch`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchResults {
    /// The embeddings of the successful items, in order.
    pub embeddings: Vec<NdArray>,
    /// The successful items.
    pub successful: Vec<String>,
    /// The failed items with their errors.
    pub failed: Vec<FailedItem>,
    /// Number of items.
    pub total: usize,
    /// `len(successful)`.
    pub success_count: usize,
    /// `len(failed)`.
    pub failure_count: usize,
}

/// Main embedding generation handler.
pub struct EmbeddingGenerator {
    logger: Logger,
    config: Map<String, Value>,
    text_embedder: TextEmbedder,
    supported_models: Vec<String>,
    progress_tracker: Arc<ProgressTracker>,
}

impl fmt::Debug for EmbeddingGenerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddingGenerator")
            .field("config", &self.config)
            .field("text_embedder", &self.text_embedder)
            .finish()
    }
}

impl EmbeddingGenerator {
    /// `EmbeddingGenerator(config=None)`: `config["text"]` holds the `TextEmbedder` keyword
    /// arguments. Python's `**kwargs` are merged into `config` (see
    /// [`EmbeddingGenerator::from_kwargs`]).
    pub fn new(config: Option<Map<String, Value>>) -> Result<EmbeddingGenerator, SemanticaError> {
        let config = config.unwrap_or_default();
        let text_config = match config.get("text") {
            None => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            Some(other) => {
                return Err(SemanticaError::Type(format!(
                    "semantica.embeddings.text_embedder.TextEmbedder() argument after ** must be \
                     a mapping, not {}",
                    type_name(other)
                )));
            }
        };
        let text_embedder = TextEmbedder::from_kwargs(&text_config)?;
        Ok(Self::assemble(config, text_embedder))
    }

    /// `EmbeddingGenerator(**kwargs)`: a `config` keyword is the configuration dict, every other
    /// keyword is merged into it.
    pub fn from_kwargs(kwargs: &Map<String, Value>) -> Result<EmbeddingGenerator, SemanticaError> {
        let mut config = match kwargs.get("config") {
            None => Map::new(),
            Some(v) if !truthy(v) => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            Some(other) => {
                return Err(SemanticaError::Attribute(format!(
                    "'{}' object has no attribute 'update'",
                    type_name(other)
                )));
            }
        };
        for (k, v) in kwargs {
            if k != "config" {
                config.insert(k.clone(), v.clone());
            }
        }
        Self::new(Some(config))
    }

    /// A generator around an existing text embedder (Rust-side injection point).
    pub fn with_text_embedder(text_embedder: TextEmbedder) -> EmbeddingGenerator {
        Self::assemble(Map::new(), text_embedder)
    }

    fn assemble(config: Map<String, Value>, text_embedder: TextEmbedder) -> EmbeddingGenerator {
        let logger = get_logger("embedding_generator");
        let progress_tracker = get_progress_tracker();
        // Ensure progress tracker is enabled
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.info("Embedding generator initialized");
        EmbeddingGenerator {
            logger,
            config,
            text_embedder,
            supported_models: ["sentence-transformers", "openai", "bge", "fastembed"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            progress_tracker,
        }
    }

    /// `set_text_model(method, model_name, **config)`.
    pub fn set_text_model(&mut self, method: &str, model_name: &str, config: &Map<String, Value>) {
        self.text_embedder.set_model(method, model_name, config);
        self.logger
            .info(format!("Switched text model to: {method}/{model_name}"));
    }

    /// `get_text_method()`.
    pub fn get_text_method(&self) -> String {
        self.text_embedder.get_method()
    }

    /// `get_methods_info()`.
    pub fn get_methods_info(&self) -> MethodsInfo {
        MethodsInfo {
            text: self.text_embedder.get_model_info(),
        }
    }

    /// `generate_embeddings(data, data_type=None, **options)`: a text gives a 1-D embedding, a
    /// list a 2-D array. `data_type` defaults to `"text"`, the only supported type.
    pub fn generate_embeddings(
        &self,
        data: &EmbeddingInput,
        data_type: Option<&str>,
        options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        let data_type = match data_type {
            Some(t) => t,
            None => {
                self.logger.debug("Using data type: text");
                "text"
            }
        };
        // Pre-process list if it contains FileObjects or dicts
        let batch: Option<Vec<String>> = match data {
            EmbeddingInput::Text(_) => None,
            EmbeddingInput::Batch(items) => Some(items.iter().map(DataItem::to_text).collect()),
        };
        if data_type == "text" {
            match (data, batch) {
                (EmbeddingInput::Text(text), _) => self.text_embedder.embed_text(text, options),
                (_, Some(texts)) => self.text_embedder.embed_batch(&texts, options),
                (EmbeddingInput::Batch(_), None) => unreachable!("batch is always preprocessed"),
            }
        } else {
            Err(ProcessingError::new(format!(
                "Unsupported data type: {data_type}. Supported types: text"
            ))
            .into())
        }
    }

    /// `compare_embeddings(embedding1, embedding2, method="cosine")`: `"euclidean"` or cosine
    /// (any other method).
    pub fn compare_embeddings(
        &self,
        embedding1: &NdArray,
        embedding2: &NdArray,
        options: &Map<String, Value>,
    ) -> Result<f64, SemanticaError> {
        match options.get("method") {
            Some(Value::String(m)) if m == "euclidean" => {
                euclidean_similarity(embedding1, embedding2)
            }
            _ => cosine_similarity(embedding1, embedding2),
        }
    }

    /// `process_batch(data_items, pipeline_id=None, **options)`: embed each item, collecting
    /// failures instead of raising.
    pub fn process_batch(
        &self,
        data_items: &[String],
        pipeline_id: Option<&str>,
        options: &Map<String, Value>,
    ) -> BatchResults {
        let total = data_items.len();
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("embeddings"),
            Some("EmbeddingGenerator"),
            &format!("Batch of {total} items"),
            pipeline_id,
        );
        let mut results = BatchResults {
            embeddings: Vec::new(),
            successful: Vec::new(),
            failed: Vec::new(),
            total,
            success_count: 0,
            failure_count: 0,
        };
        for (idx, item) in data_items.iter().enumerate() {
            let idx = idx + 1;
            self.progress_tracker.update_progress(
                &tracking_id,
                idx as i64,
                total as i64,
                &format!("Processing item {idx}/{total}"),
            );
            match self.generate_embeddings(&EmbeddingInput::Text(item.clone()), None, options) {
                Ok(embedding) => {
                    results.embeddings.push(embedding);
                    results.successful.push(item.clone());
                }
                Err(e) => results.failed.push(FailedItem {
                    item: item.clone(),
                    error: e.to_string(),
                }),
            }
        }
        results.success_count = results.successful.len();
        results.failure_count = results.failed.len();
        self.progress_tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Processed {}/{total} items successfully",
                results.success_count
            ),
            None,
        );
        results
    }

    /// `self.config`.
    pub fn config(&self) -> &Map<String, Value> {
        &self.config
    }

    /// `self.text_embedder`.
    pub fn text_embedder(&self) -> &TextEmbedder {
        &self.text_embedder
    }

    /// `self.text_embedder` (mutable).
    pub fn text_embedder_mut(&mut self) -> &mut TextEmbedder {
        &mut self.text_embedder
    }

    /// `self.supported_models`.
    pub fn supported_models(&self) -> &[String] {
        &self.supported_models
    }

    /// The `embedding_generator` logger.
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

// ----- similarity ---------------------------------------------------------------------------

/// numpy's message for `np.dot` operands whose inner dimensions differ.
fn not_aligned(a: &[usize], b: &[usize], dim_a: usize, dim_b: usize) -> SemanticaError {
    let compact = |s: &[usize]| shape_repr(s).replace(", ", ",");
    SemanticaError::Value(format!(
        "shapes {} and {} not aligned: {} (dim {dim_a}) != {} (dim {dim_b})",
        compact(a),
        compact(b),
        a[dim_a],
        b[dim_b]
    ))
}

fn dot_impl<T: Float>(a: &[T], sa: &[usize], b: &[T], sb: &[usize]) -> Result<NdArray, SemanticaError> {
    match (sa.len(), sb.len()) {
        (0, _) => Ok(T::wrap(sb.to_vec(), b.iter().map(|&x| a[0] * x).collect())),
        (_, 0) => Ok(T::wrap(sa.to_vec(), a.iter().map(|&x| x * b[0]).collect())),
        (1, 1) => {
            if sa[0] != sb[0] {
                return Err(not_aligned(sa, sb, 0, 0));
            }
            Ok(T::wrap(Vec::new(), vec![T::dot(a, b)]))
        }
        (2, 1) => {
            let (r, c) = (sa[0], sa[1]);
            if c != sb[0] {
                return Err(not_aligned(sa, sb, 1, 0));
            }
            Ok(T::wrap(
                vec![r],
                (0..r).map(|i| T::dot(&a[i * c..(i + 1) * c], b)).collect(),
            ))
        }
        (1, 2) => {
            let (r, c) = (sb[0], sb[1]);
            if sa[0] != r {
                return Err(not_aligned(sa, sb, 0, 0));
            }
            let out = (0..c)
                .map(|j| {
                    let column: Vec<T> = (0..r).map(|i| b[i * c + j]).collect();
                    T::dot(a, &column)
                })
                .collect();
            Ok(T::wrap(vec![c], out))
        }
        _ => {
            let (r, k) = (sa[0], sa[1]);
            let (k2, c) = (sb[0], sb[1]);
            if k != k2 {
                return Err(not_aligned(sa, sb, 1, 0));
            }
            let mut out = Vec::with_capacity(r * c);
            for i in 0..r {
                for j in 0..c {
                    let column: Vec<T> = (0..k).map(|t| b[t * c + j]).collect();
                    out.push(T::dot(&a[i * k..(i + 1) * k], &column));
                }
            }
            Ok(T::wrap(vec![r, c], out))
        }
    }
}

/// `np.dot(a, b)` for arrays of up to two dimensions (computed in the common dtype).
pub(crate) fn np_dot(a: &NdArray, b: &NdArray) -> Result<NdArray, SemanticaError> {
    let dtype = a.dtype().promote(b.dtype());
    let (a, b) = (a.astype(dtype), b.astype(dtype));
    let sb = b.shape().to_vec();
    with_float!(&a, |va: T| {
        let vb = T::view(&b).expect("same dtype");
        dot_impl(va, a.shape(), vb, &sb)
    })
}

/// `float(x)`: a single-element array's value.
fn to_python_float(x: &NdArray) -> Result<f64, SemanticaError> {
    x.item().ok_or_else(|| {
        SemanticaError::Type("only length-1 arrays can be converted to Python scalars".to_string())
    })
}

/// `_cosine_similarity`: `float(dot / (norm1 * norm2))`, `0.0` when a norm is zero.
pub(crate) fn cosine_similarity(emb1: &NdArray, emb2: &NdArray) -> Result<f64, SemanticaError> {
    let dot_product = np_dot(emb1, emb2)?;
    let norm1 = emb1.norm();
    let norm2 = emb2.norm();
    if norm1 == 0.0 || norm2 == 0.0 {
        return Ok(0.0);
    }
    let dtype = emb1.dtype().promote(emb2.dtype());
    let quotient = match dtype {
        DType::Float32 => {
            let denominator = norm1 as f32 * norm2 as f32;
            let values: Vec<f32> = dot_product
                .to_vec_f32()
                .into_iter()
                .map(|d| d / denominator)
                .collect();
            NdArray::from_shape_f32(dot_product.shape().to_vec(), values)?
        }
        DType::Float64 => {
            let denominator = norm1 * norm2;
            let values: Vec<f64> = dot_product
                .to_vec_f64()
                .into_iter()
                .map(|d| d / denominator)
                .collect();
            NdArray::from_shape_f64(dot_product.shape().to_vec(), values)?
        }
    };
    to_python_float(&quotient)
}

/// numpy broadcasting of two shapes (up to two dimensions).
fn broadcast_shape(a: &[usize], b: &[usize]) -> Result<Vec<usize>, SemanticaError> {
    let n = a.len().max(b.len());
    let mut out = vec![0; n];
    for i in 0..n {
        let da = if i + a.len() >= n { a[i + a.len() - n] } else { 1 };
        let db = if i + b.len() >= n { b[i + b.len() - n] } else { 1 };
        out[i] = if da == db || db == 1 {
            da
        } else if da == 1 {
            db
        } else {
            return Err(SemanticaError::Value(format!(
                "operands could not be broadcast together with shapes {} {} ",
                shape_repr(a).replace(", ", ","),
                shape_repr(b).replace(", ", ",")
            )));
        };
    }
    Ok(out)
}

/// The flat index into an array of `shape` for a position in the broadcast `out` shape.
fn broadcast_index(shape: &[usize], out: &[usize], position: &[usize]) -> usize {
    let offset = out.len() - shape.len();
    let mut index = 0;
    for (k, &dim) in shape.iter().enumerate() {
        let p = if dim == 1 { 0 } else { position[offset + k] };
        index = index * dim + p;
    }
    index
}

fn subtract_impl<T: Float>(a: &[T], sa: &[usize], b: &[T], sb: &[usize], out: &[usize]) -> Vec<T> {
    let size: usize = out.iter().product();
    let mut result = Vec::with_capacity(size);
    let mut position = vec![0usize; out.len()];
    for _ in 0..size {
        let ia = broadcast_index(sa, out, &position);
        let ib = broadcast_index(sb, out, &position);
        result.push(a[ia] - b[ib]);
        for k in (0..out.len()).rev() {
            position[k] += 1;
            if position[k] < out[k] {
                break;
            }
            position[k] = 0;
        }
    }
    result
}

/// `np.linalg.norm(emb1 - emb2)` in the common dtype, as `f64`.
fn difference_norm(emb1: &NdArray, emb2: &NdArray) -> Result<f64, SemanticaError> {
    let out = broadcast_shape(emb1.shape(), emb2.shape())?;
    let dtype = emb1.dtype().promote(emb2.dtype());
    let (a, b) = (emb1.astype(dtype), emb2.astype(dtype));
    let sb = b.shape().to_vec();
    Ok(with_float!(&a, |va: T| {
        let vb = T::view(&b).expect("same dtype");
        let diff = subtract_impl(va, a.shape(), vb, &sb, &out);
        vector_norm(&diff).to_f64()
    }))
}

/// `_euclidean_similarity`: `1 - |e1 - e2| / (|e1| + |e2|)` clipped to `[0, 1]` (`0.0` when
/// both norms are zero).
pub(crate) fn euclidean_similarity(emb1: &NdArray, emb2: &NdArray) -> Result<f64, SemanticaError> {
    let distance = difference_norm(emb1, emb2)?;
    let (norm1, norm2) = (emb1.norm(), emb2.norm());
    let dtype = emb1.dtype().promote(emb2.dtype());
    let similarity = match dtype {
        DType::Float32 => {
            let max_distance = norm1 as f32 + norm2 as f32;
            if max_distance > 0.0 {
                (1.0f32 - distance as f32 / max_distance) as f64
            } else {
                0.0
            }
        }
        DType::Float64 => {
            let max_distance = norm1 + norm2;
            if max_distance > 0.0 {
                1.0 - distance / max_distance
            } else {
                0.0
            }
        }
    };
    // max(0.0, min(1.0, similarity)) with Python's builtin semantics (NaN -> 1.0)
    let upper = if similarity < 1.0 { similarity } else { 1.0 };
    Ok(if upper > 0.0 { upper } else { 0.0 })
}
