//! Text Embedder Module (port of `semantica/embeddings/text_embedder.py`).
//!
//! Text embedding generation with sentence-transformers or FastEmbed models, falling back to
//! deterministic hash-based embeddings when no model can be loaded.
//!
//! The original module imports `SentenceTransformer` and `fastembed.TextEmbedding` at import
//! time and records whether that worked in `SENTENCE_TRANSFORMERS_AVAILABLE` /
//! `FASTEMBED_AVAILABLE`; its tests patch those four module globals. Here they are a
//! [`ModelBackends`] value: the process default (BERT models from local files, see
//! [`crate::providers`]), replaceable with [`set_default_backends`], or patched for the current
//! thread with [`patch_backends`] (restored when the guard drops). A [`TextEmbedder`] reads the
//! backends each time it (re)initialises its model, exactly when the original reads the module
//! globals; [`TextEmbedderOptions::backends`] pins them for one embedder instead.
//!
//! ```no_run
//! use sem_embeddings::text_embedder::{TextEmbedder, TextEmbedderOptions};
//! use serde_json::Map;
//!
//! let embedder = TextEmbedder::new(TextEmbedderOptions {
//!     model_name: "all-MiniLM-L6-v2".into(),
//!     ..TextEmbedderOptions::default()
//! });
//! let embedding = embedder.embed_text("Hello world", &Map::new())?;
//! let batch = embedder.embed_batch(&["text1".to_string(), "text2".to_string()], &Map::new())?;
//! # let _ = (embedding, batch);
//! # Ok::<(), sem_utils::SemanticaError>(())
//! ```

use std::cell::RefCell;
use std::fmt;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::{slice_chars, strip, truthy, type_name};
use sem_utils::{ProcessingError, SemanticaError, get_logger, get_progress_tracker};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::mt19937::RandomState;
use crate::ndarray::{NdArray, row_norms, vector_norm};
use crate::providers::{
    EmbedRequest, EmbeddingProvider, LocalSentenceTransformer, LocalTextEmbedding, ModelLocator,
};
use crate::pyconv::{is_blank, py_index, py_lower};

/// `SentenceTransformer(model_name, device=device)`.
pub type SentenceTransformerLoader = Arc<
    dyn Fn(&str, &str) -> Result<Arc<dyn EmbeddingProvider>, SemanticaError> + Send + Sync,
>;
/// `TextEmbedding(model_name=model_name)`.
pub type TextEmbeddingLoader =
    Arc<dyn Fn(&str) -> Result<Arc<dyn EmbeddingProvider>, SemanticaError> + Send + Sync>;

/// The model libraries `text_embedder.py` imports: availability flags and constructors.
#[derive(Clone)]
pub struct ModelBackends {
    /// `SENTENCE_TRANSFORMERS_AVAILABLE`.
    pub sentence_transformers_available: bool,
    /// `FASTEMBED_AVAILABLE`.
    pub fastembed_available: bool,
    /// `SentenceTransformer`.
    pub sentence_transformer: SentenceTransformerLoader,
    /// `TextEmbedding`.
    pub text_embedding: TextEmbeddingLoader,
}

impl fmt::Debug for ModelBackends {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelBackends")
            .field(
                "sentence_transformers_available",
                &self.sentence_transformers_available,
            )
            .field("fastembed_available", &self.fastembed_available)
            .finish_non_exhaustive()
    }
}

impl ModelBackends {
    /// Both libraries available, loading BERT models from the directories of `locator`.
    pub fn local(locator: ModelLocator) -> ModelBackends {
        let st_locator = locator.clone();
        ModelBackends {
            sentence_transformers_available: true,
            fastembed_available: true,
            sentence_transformer: Arc::new(move |name: &str, device: &str| {
                let model = LocalSentenceTransformer::load(name, device, &st_locator)?;
                Ok(Arc::new(model) as Arc<dyn EmbeddingProvider>)
            }),
            text_embedding: Arc::new(move |name: &str| {
                let model = LocalTextEmbedding::load(name, &locator)?;
                Ok(Arc::new(model) as Arc<dyn EmbeddingProvider>)
            }),
        }
    }

    /// [`ModelBackends::local`] with the directories named by the environment
    /// ([`ModelLocator::from_env`]).
    pub fn from_env() -> ModelBackends {
        ModelBackends::local(ModelLocator::from_env())
    }
}

impl Default for ModelBackends {
    fn default() -> Self {
        ModelBackends::from_env()
    }
}

static DEFAULT_BACKENDS: Lazy<RwLock<Option<ModelBackends>>> = Lazy::new(|| RwLock::new(None));

thread_local! {
    static PATCHED: RefCell<Vec<ModelBackends>> = const { RefCell::new(Vec::new()) };
}

/// The backends in effect for this thread: the innermost [`patch_backends`], else the
/// process default.
pub fn current_backends() -> ModelBackends {
    if let Some(patched) = PATCHED.with(|p| p.borrow().last().cloned()) {
        return patched;
    }
    let mut slot = DEFAULT_BACKENDS.write().unwrap_or_else(|e| e.into_inner());
    slot.get_or_insert_with(ModelBackends::from_env).clone()
}

/// Replace the process-wide default backends (`None` restores the environment-based
/// default).
pub fn set_default_backends(backends: Option<ModelBackends>) {
    *DEFAULT_BACKENDS.write().unwrap_or_else(|e| e.into_inner()) = backends;
}

/// Restores the previous backends of this thread when dropped.
#[must_use = "the patch is undone when the guard is dropped"]
pub struct BackendsGuard {
    depth: usize,
}

impl Drop for BackendsGuard {
    fn drop(&mut self) {
        PATCHED.with(|p| p.borrow_mut().truncate(self.depth));
    }
}

/// Use `backends` on this thread until the guard drops (the counterpart of
/// `patch('semantica.embeddings.text_embedder.SentenceTransformer')` and friends).
pub fn patch_backends(backends: ModelBackends) -> BackendsGuard {
    PATCHED.with(|p| {
        let mut stack = p.borrow_mut();
        let depth = stack.len();
        stack.push(backends);
        BackendsGuard { depth }
    })
}

/// Keyword arguments of `TextEmbedder(...)`.
#[derive(Clone, Debug)]
pub struct TextEmbedderOptions {
    /// `model_name` (default `"BAAI/bge-small-en-v1.5"`).
    pub model_name: String,
    /// `device` (default `"cpu"`).
    pub device: String,
    /// `normalize` (default `true`).
    pub normalize: bool,
    /// `method`: `"fastembed"` (default) or `"sentence_transformers"`; any case.
    pub method: String,
    /// `**config` (e.g. `dimension`, `fastembed_model_name`).
    pub config: Map<String, Value>,
    /// Backends for this embedder only (`None`: [`current_backends`] at each initialisation).
    pub backends: Option<ModelBackends>,
}

impl Default for TextEmbedderOptions {
    fn default() -> Self {
        TextEmbedderOptions {
            model_name: "BAAI/bge-small-en-v1.5".to_string(),
            device: "cpu".to_string(),
            normalize: true,
            method: "fastembed".to_string(),
            config: Map::new(),
            backends: None,
        }
    }
}

impl TextEmbedderOptions {
    /// Bind `TextEmbedder(**kwargs)`: `model_name`, `device`, `normalize` and `method` are the
    /// named parameters, every other key goes to `config`.
    ///
    /// `method` must be a string (Python calls `.lower()` on it); a non-string `model_name` or
    /// `device` is converted with `str()`, `normalize` with `bool()`.
    pub fn from_kwargs(kwargs: &Map<String, Value>) -> Result<TextEmbedderOptions, SemanticaError> {
        let mut options = TextEmbedderOptions::default();
        for (key, value) in kwargs {
            match key.as_str() {
                "model_name" => options.model_name = value_str(value),
                "device" => options.device = value_str(value),
                "normalize" => options.normalize = truthy(value),
                "method" => match value {
                    Value::String(s) => options.method = s.clone(),
                    other => {
                        return Err(SemanticaError::Attribute(format!(
                            "'{}' object has no attribute 'lower'",
                            type_name(other)
                        )));
                    }
                },
                _ => {
                    options.config.insert(key.clone(), value.clone());
                }
            }
        }
        Ok(options)
    }
}

fn value_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => sem_utils::pycompat::fmt::py_str(other),
    }
}

/// `get_model_info()`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelInfo {
    /// Active method: `"fastembed"`, `"sentence_transformers"` or `"fallback"`.
    pub method: String,
    /// The configured model name.
    pub model_name: String,
    /// Whether the active method's model is loaded.
    pub model_loaded: bool,
    /// The embedding dimension.
    pub dimension: usize,
    /// Whether embeddings are normalised.
    pub normalize: bool,
    /// The device (only for sentence-transformers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// Text embedding generator for semantic text representation.
///
/// Uses a sentence-transformers or FastEmbed model when one loads, otherwise a deterministic
/// SHA-256-seeded embedding (not semantic; for tests and missing dependencies).
pub struct TextEmbedder {
    logger: Logger,
    config: Map<String, Value>,
    model_name: String,
    device: String,
    normalize: bool,
    method: String,
    model: Option<Arc<dyn EmbeddingProvider>>,
    fastembed_model: Option<Arc<dyn EmbeddingProvider>>,
    embedding_dimension: Option<usize>,
    dimension_error: Option<SemanticaError>,
    progress_tracker: Arc<ProgressTracker>,
    backends: Option<ModelBackends>,
}

impl fmt::Debug for TextEmbedder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextEmbedder")
            .field("method", &self.method)
            .field("model_name", &self.model_name)
            .field("device", &self.device)
            .field("normalize", &self.normalize)
            .field("model", &self.model.as_ref().map(|m| m.name()))
            .field("fastembed_model", &self.fastembed_model.as_ref().map(|m| m.name()))
            .field("embedding_dimension", &self.embedding_dimension)
            .finish()
    }
}

impl Default for TextEmbedder {
    fn default() -> Self {
        TextEmbedder::new(TextEmbedderOptions::default())
    }
}

impl TextEmbedder {
    /// `TextEmbedder(model_name=..., device=..., normalize=..., method=..., **config)`:
    /// loads the model, falling back (with a warning) to hash-based embeddings.
    pub fn new(options: TextEmbedderOptions) -> TextEmbedder {
        let mut embedder = TextEmbedder {
            logger: get_logger("text_embedder"),
            config: options.config,
            model_name: options.model_name,
            device: options.device,
            normalize: options.normalize,
            method: py_lower(&options.method),
            model: None,
            fastembed_model: None,
            embedding_dimension: None,
            dimension_error: None,
            progress_tracker: get_progress_tracker(),
            backends: options.backends,
        };
        embedder.initialize_model();
        embedder
    }

    /// `TextEmbedder(**kwargs)` (see [`TextEmbedderOptions::from_kwargs`]).
    pub fn from_kwargs(kwargs: &Map<String, Value>) -> Result<TextEmbedder, SemanticaError> {
        Ok(TextEmbedder::new(TextEmbedderOptions::from_kwargs(kwargs)?))
    }

    fn backends(&self) -> ModelBackends {
        self.backends.clone().unwrap_or_else(current_backends)
    }

    /// `_initialize_model()`: load the configured model; on failure keep no model (the
    /// fallback method) and take the dimension from `config["dimension"]` (default 128).
    fn initialize_model(&mut self) {
        // Clear previous models to avoid conflicts when switching
        self.model = None;
        self.fastembed_model = None;
        self.embedding_dimension = None;
        self.dimension_error = None;
        let mut dimension_known = false;
        let backends = self.backends();

        if self.method == "fastembed" {
            if backends.fastembed_available {
                let fastembed_model_name = match self.config.get("fastembed_model_name") {
                    Some(v) => value_str(v),
                    None => self.model_name.clone(),
                };
                match (backends.text_embedding)(&fastembed_model_name) {
                    Ok(model) => {
                        // FastEmbed doesn't expose dimension directly, so we generate a test embedding
                        let test = model.embed(&["test".to_string()], &EmbedRequest::default());
                        let dimension = match test {
                            Ok(rows) if !rows.is_empty() => rows[0].len(),
                            _ => 384,
                        };
                        self.embedding_dimension = Some(dimension);
                        dimension_known = true;
                        self.fastembed_model = Some(model);
                        self.logger.info(format!(
                            "Loaded FastEmbed model: {fastembed_model_name} (dim: {dimension})"
                        ));
                    }
                    Err(e) => {
                        self.logger.warning(format!(
                            "Failed to load FastEmbed model '{}': {e}. Using fallback embedding method.",
                            self.model_name
                        ));
                        self.fastembed_model = None;
                    }
                }
            } else {
                self.logger.warning(
                    "fastembed not available. Install with: pip install fastembed. Using fallback embedding method.",
                );
            }
        } else if backends.sentence_transformers_available {
            // Default to sentence-transformers
            match (backends.sentence_transformer)(&self.model_name, &self.device) {
                Ok(model) => {
                    self.embedding_dimension = model.dimension();
                    dimension_known = self.embedding_dimension.is_some();
                    let dim = self
                        .embedding_dimension
                        .map_or("None".to_string(), |d| d.to_string());
                    self.model = Some(model);
                    self.logger.info(format!(
                        "Loaded sentence-transformers model: {} (device: {}, dim: {dim})",
                        self.model_name, self.device
                    ));
                }
                Err(e) => {
                    self.logger.warning(format!(
                        "Failed to load model '{}': {e}. Using fallback embedding method.",
                        self.model_name
                    ));
                    self.model = None;
                }
            }
        } else {
            self.logger.warning(
                "sentence-transformers not available. Install with: pip install sentence-transformers. Using fallback embedding method.",
            );
        }

        // If no model loaded, use fallback dimension
        if !dimension_known {
            match self.config.get("dimension") {
                None => self.embedding_dimension = Some(128),
                Some(Value::Null) => self.embedding_dimension = None,
                Some(value) => match py_index(value) {
                    Ok(d) if d >= 0 => self.embedding_dimension = Some(d as usize),
                    Ok(_) => {
                        self.dimension_error = Some(SemanticaError::Value(
                            "negative dimensions are not allowed".to_string(),
                        ))
                    }
                    Err(e) => self.dimension_error = Some(e),
                },
            }
        }
        self.logger.debug(format!(
            "Initialized text embedder with dimension: {}",
            self.get_embedding_dimension()
        ));
    }

    /// `set_model(method, model_name, **config)`: switch model dynamically; `device` and
    /// `normalize` in `config` also update those attributes.
    pub fn set_model(&mut self, method: &str, model_name: &str, config: &Map<String, Value>) {
        self.method = py_lower(method);
        self.model_name = model_name.to_string();
        for (k, v) in config {
            self.config.insert(k.clone(), v.clone());
        }
        if let Some(device) = config.get("device") {
            self.device = value_str(device);
        }
        if let Some(normalize) = config.get("normalize") {
            self.normalize = truthy(normalize);
        }
        self.initialize_model();
    }

    /// `embed_text(text, **options)`: one embedding (1-D `float32`). Empty or whitespace-only
    /// text is a `ProcessingError`. `options` go to sentence-transformers' `encode`.
    pub fn embed_text(
        &self,
        text: &str,
        options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("embeddings"),
            Some("TextEmbedder"),
            &format!(
                "Generating text embedding: {}...",
                slice_chars(text, None, Some(50))
            ),
            None,
        );
        let result = (|| {
            if is_blank(text) {
                return Err(SemanticaError::from(ProcessingError::new(
                    "Text cannot be empty or whitespace-only",
                )));
            }
            if self.fastembed_model.is_some() {
                self.progress_tracker.update_tracking(
                    &tracking_id,
                    None,
                    "Using FastEmbed model...",
                );
                self.embed_with_fastembed(text)
            } else if self.model.is_some() {
                self.progress_tracker.update_tracking(
                    &tracking_id,
                    None,
                    "Using sentence-transformers model...",
                );
                self.embed_with_model(text, options)
            } else {
                self.progress_tracker.update_tracking(
                    &tracking_id,
                    None,
                    "Using fallback embedding method...",
                );
                self.embed_fallback(text)
            }
        })();
        match &result {
            Ok(embedding) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Generated embedding (dim: {})", embedding.len()),
                None,
            ),
            Err(e) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("failed"),
                &e.to_string(),
                None,
            ),
        }
        result
    }

    /// `_embed_with_model`: `model.encode([text], normalize_embeddings=normalize, **options)[0]`.
    fn embed_with_model(
        &self,
        text: &str,
        options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        let model = self.model.as_ref().expect("checked by caller");
        let rows = model.embed(
            &[text.to_string()],
            &EmbedRequest {
                normalize_embeddings: Some(self.normalize),
                model: None,
                options: Some(options),
            },
        )?;
        let first = rows.into_iter().next().ok_or_else(|| {
            SemanticaError::Index("index 0 is out of bounds for axis 0 with size 0".to_string())
        })?;
        Ok(NdArray::from_vec_f32(first))
    }

    /// `_embed_with_fastembed`: the first embedding of `model.embed([text])`, as `float32`,
    /// L2-normalised when `normalize` is set.
    fn embed_with_fastembed(&self, text: &str) -> Result<NdArray, SemanticaError> {
        let model = self.fastembed_model.as_ref().expect("checked by caller");
        let rows = model.embed(&[text.to_string()], &EmbedRequest::default())?;
        match rows.into_iter().next() {
            Some(embedding) => Ok(NdArray::from_vec_f32(self.maybe_normalize(embedding))),
            None => Err(ProcessingError::new("FastEmbed returned empty embedding").into()),
        }
    }

    fn maybe_normalize(&self, mut embedding: Vec<f32>) -> Vec<f32> {
        if self.normalize {
            let norm = vector_norm(&embedding);
            if norm > 0.0 {
                for x in &mut embedding {
                    *x /= norm;
                }
            }
        }
        embedding
    }

    /// `_embed_fallback`: a deterministic embedding from `RandomState(sha256(text) % 2**32)`:
    /// `rand(dim)` as `float32`, L2-normalised when `normalize` is set. Not semantic.
    fn embed_fallback(&self, text: &str) -> Result<NdArray, SemanticaError> {
        if let Some(e) = &self.dimension_error {
            return Err(e.clone());
        }
        let digest = Sha256::digest(text.as_bytes());
        let seed = u32::from_be_bytes([digest[28], digest[29], digest[30], digest[31]]);
        let mut rng = RandomState::new(seed);
        // Determine dimension - use stored dimension or default
        let dim = match self.embedding_dimension {
            Some(d) if d > 0 => d,
            _ => 128,
        };
        let embedding: Vec<f32> = rng.rand(dim).into_iter().map(|x| x as f32).collect();
        Ok(NdArray::from_vec_f32(self.maybe_normalize(embedding)))
    }

    /// `embed_batch(texts, **options)`: a 2-D array `(n_texts, dim)`; an empty list gives
    /// `np.array([])`.
    pub fn embed_batch(
        &self,
        texts: &[String],
        options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        if texts.is_empty() {
            self.logger
                .debug("Empty text list provided, returning empty array");
            return Ok(NdArray::empty());
        }
        self.logger
            .debug(format!("Generating embeddings for {} text(s)", texts.len()));
        if let Some(model) = &self.fastembed_model {
            // Use FastEmbed's efficient batch encoding
            let rows = model.embed(texts, &EmbedRequest::default())?;
            let array = NdArray::from_rows_f32(rows)?;
            if self.normalize && array.ndim() != 2 {
                return Err(SemanticaError::Other {
                    module: "numpy.exceptions".to_string(),
                    type_name: "AxisError".to_string(),
                    message: format!(
                        "axis 1 is out of bounds for array of dimension {}",
                        array.ndim()
                    ),
                });
            }
            if self.normalize {
                let (n, d) = (array.shape()[0], array.shape()[1]);
                let data = array.as_f32_slice().expect("float32").to_vec();
                let norms = row_norms(&data, n, d);
                let normalized: Vec<f32> = data
                    .chunks(d.max(1))
                    .zip(&norms)
                    .flat_map(|(row, &norm)| {
                        let norm = if norm == 0.0 { 1.0 } else { norm };
                        row.iter().map(move |x| x / norm).collect::<Vec<f32>>()
                    })
                    .collect();
                return NdArray::from_shape_f32(vec![n, d], normalized);
            }
            Ok(array)
        } else if let Some(model) = &self.model {
            // Use model's efficient batch encoding
            let rows = model.embed(
                texts,
                &EmbedRequest {
                    normalize_embeddings: Some(self.normalize),
                    model: None,
                    options: Some(options),
                },
            )?;
            NdArray::from_rows_f32(rows)
        } else {
            // Fallback: process each text individually
            self.logger.debug("Using fallback method for batch embedding");
            let rows = texts
                .iter()
                .map(|t| self.embed_fallback(t))
                .collect::<Result<Vec<NdArray>, SemanticaError>>()?;
            NdArray::stack(&rows)
        }
    }

    /// `embed_sentences(text, **options)`: split on runs of `.`, `!`, `?`, strip, drop empty
    /// pieces and embed each sentence (one 1-D embedding per sentence).
    pub fn embed_sentences(
        &self,
        text: &str,
        options: &Map<String, Value>,
    ) -> Result<Vec<NdArray>, SemanticaError> {
        if is_blank(text) {
            self.logger
                .debug("Empty text provided for sentence embedding");
            return Ok(Vec::new());
        }
        let sentences: Vec<String> = text
            .split(['.', '!', '?'])
            .map(strip)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if sentences.is_empty() {
            self.logger.debug("No sentences found in text");
            return Ok(Vec::new());
        }
        self.logger.debug(format!(
            "Extracting embeddings for {} sentence(s)",
            sentences.len()
        ));
        let embeddings = self.embed_batch(&sentences, options)?;
        (0..sentences.len()).map(|i| embeddings.row(i)).collect()
    }

    /// `get_embedding_dimension()`: the model's dimension, or the fallback dimension (128).
    pub fn get_embedding_dimension(&self) -> usize {
        match self.embedding_dimension {
            Some(d) if d > 0 => d,
            _ => 128,
        }
    }

    /// `get_method()`: `"fastembed"`, `"sentence_transformers"` or `"fallback"`.
    pub fn get_method(&self) -> String {
        if self.fastembed_model.is_some() {
            "fastembed".to_string()
        } else if self.model.is_some() {
            "sentence_transformers".to_string()
        } else {
            "fallback".to_string()
        }
    }

    /// `get_model_info()`.
    pub fn get_model_info(&self) -> ModelInfo {
        let method = self.get_method();
        let model_loaded = match method.as_str() {
            "fastembed" => self.fastembed_model.is_some(),
            "sentence_transformers" => self.model.is_some(),
            _ => false,
        };
        ModelInfo {
            device: (method == "sentence_transformers").then(|| self.device.clone()),
            method,
            model_name: self.model_name.clone(),
            model_loaded,
            dimension: self.get_embedding_dimension(),
            normalize: self.normalize,
        }
    }

    /// `self.model_name`.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// `self.device`.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// `self.normalize`.
    pub fn normalize(&self) -> bool {
        self.normalize
    }

    /// `self.normalize = value`.
    pub fn set_normalize(&mut self, value: bool) {
        self.normalize = value;
    }

    /// `self.method` (lower-cased requested method, not necessarily the active one).
    pub fn method(&self) -> &str {
        &self.method
    }

    /// `self.config`.
    pub fn config(&self) -> &Map<String, Value> {
        &self.config
    }

    /// `self.model`: the sentence-transformers model, if loaded.
    pub fn model(&self) -> Option<&Arc<dyn EmbeddingProvider>> {
        self.model.as_ref()
    }

    /// `self.fastembed_model`: the FastEmbed model, if loaded.
    pub fn fastembed_model(&self) -> Option<&Arc<dyn EmbeddingProvider>> {
        self.fastembed_model.as_ref()
    }

    /// `self.embedding_dimension` as stored (`None` when unknown).
    pub fn embedding_dimension(&self) -> Option<usize> {
        self.embedding_dimension
    }

    /// The `text_embedder` logger.
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}
