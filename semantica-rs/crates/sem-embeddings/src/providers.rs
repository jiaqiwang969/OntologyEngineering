//! Embedding providers: the one trait every embedding backend implements.
//!
//! The original calls three third-party libraries: `sentence_transformers.SentenceTransformer`
//! (`encode(texts, normalize_embeddings=..., **options)`), `fastembed.TextEmbedding`
//! (`embed(texts)`) and the `openai` client (`embeddings.create(model=..., input=text)`).
//! Here each is an implementation of [`EmbeddingProvider`]:
//!
//! - [`LocalSentenceTransformer`] and [`LocalTextEmbedding`] run BERT models with candle
//!   ([`crate::bert`]) from a local directory found by a [`ModelLocator`] — the port never
//!   downloads, so a model that is not on disk fails to load (and callers fall back exactly as
//!   the original does when a model cannot be loaded);
//! - [`OpenAIClient`] speaks OpenAI's documented `POST /embeddings` HTTP API with `reqwest`.
//!
//! Tests substitute deterministic fakes where the original tests use mocks.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use sem_utils::SemanticaError;
use sem_utils::exceptions::OsError;
use sem_utils::pycompat::fmt::py_str;
use serde_json::{Map, Value, json};

use crate::bert::{BertSentenceEncoder, Layout, PoolingModes};

/// The arguments of one embedding call besides the texts.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmbedRequest<'a> {
    /// sentence-transformers' `normalize_embeddings` (`None` for providers without the flag).
    pub normalize_embeddings: Option<bool>,
    /// The model to use for this call (OpenAI's `model=`), when it overrides the provider's.
    pub model: Option<&'a str>,
    /// Extra keyword arguments forwarded to the library call (`**options`).
    pub options: Option<&'a Map<String, Value>>,
}

/// An embedding backend: a sentence-transformers model, a fastembed model, a remote API, or
/// a test fake.
pub trait EmbeddingProvider: Send + Sync {
    /// Embed `texts`, one vector per text, in order.
    fn embed(
        &self,
        texts: &[String],
        request: &EmbedRequest<'_>,
    ) -> Result<Vec<Vec<f32>>, SemanticaError>;

    /// The embedding dimension when the backend knows it without embedding anything
    /// (`get_sentence_embedding_dimension()`).
    fn dimension(&self) -> Option<usize> {
        None
    }

    /// A short description (model name) for logs.
    fn name(&self) -> String;
}

impl fmt::Debug for dyn EmbeddingProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmbeddingProvider({})", self.name())
    }
}

// ----- locating local models ------------------------------------------------------------

/// Where local model weights are looked up.
///
/// A model name that is an existing directory is used as is (sentence-transformers accepts a
/// path). Otherwise each root is searched for `<root>/<org>/<name>` and for the Hugging Face
/// cache layout `<root>/models--<org>--<name>/snapshots/<revision>/` (the revision in
/// `refs/main` first). A bare sentence-transformers name (`all-MiniLM-L6-v2`) is also tried
/// as `sentence-transformers/<name>`, where the Hub redirects it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelLocator {
    /// Roots searched for sentence-transformers models.
    pub sentence_transformers_roots: Vec<PathBuf>,
    /// Roots searched for fastembed models.
    pub fastembed_roots: Vec<PathBuf>,
}

impl ModelLocator {
    /// No search roots: only explicit model directories load.
    pub fn none() -> ModelLocator {
        ModelLocator::default()
    }

    /// The same roots for both libraries.
    pub fn with_roots<I, P>(roots: I) -> ModelLocator
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let roots: Vec<PathBuf> = roots.into_iter().map(Into::into).collect();
        ModelLocator {
            sentence_transformers_roots: roots.clone(),
            fastembed_roots: roots,
        }
    }

    /// The roots the original libraries' own environment variables name:
    /// `SENTENCE_TRANSFORMERS_HOME`, `HF_HUB_CACHE`, `HF_HOME/hub` for sentence-transformers and
    /// `FASTEMBED_CACHE_PATH`, `HF_HUB_CACHE`, `HF_HOME/hub` for fastembed. Unlike the libraries
    /// there is no implicit default under the home directory or the temporary directory.
    pub fn from_env() -> ModelLocator {
        ModelLocator::from_vars(|key| {
            std::env::var_os(key).map(|v| v.to_string_lossy().into_owned())
        })
    }

    /// [`ModelLocator::from_env`] over a variable lookup function.
    pub fn from_vars(lookup: impl Fn(&str) -> Option<String>) -> ModelLocator {
        let var = |key: &str| lookup(key).filter(|v| !v.is_empty()).map(PathBuf::from);
        let hub = var("HF_HUB_CACHE");
        let hf_home_hub = var("HF_HOME").map(|p| p.join("hub"));
        let mut st = Vec::new();
        let mut fe = Vec::new();
        st.extend(var("SENTENCE_TRANSFORMERS_HOME"));
        fe.extend(var("FASTEMBED_CACHE_PATH"));
        for root in [hub, hf_home_hub].into_iter().flatten() {
            st.push(root.clone());
            fe.push(root);
        }
        ModelLocator {
            sentence_transformers_roots: st,
            fastembed_roots: fe,
        }
    }

    /// The directory of a sentence-transformers model (a path, or a repo id under the roots).
    pub fn find_sentence_transformer(&self, model_name_or_path: &str) -> Option<PathBuf> {
        let path = Path::new(model_name_or_path);
        if path.is_dir() {
            return Some(path.to_path_buf());
        }
        let mut names = vec![model_name_or_path.to_string()];
        if !model_name_or_path.contains('/') {
            names.push(format!("sentence-transformers/{model_name_or_path}"));
        }
        names
            .iter()
            .find_map(|name| find_in_roots(&self.sentence_transformers_roots, name))
    }

    /// The directory holding the weights of a fastembed model (by its canonical name).
    pub fn find_fastembed(&self, model_name: &str) -> Option<PathBuf> {
        find_in_roots(&self.fastembed_roots, model_name)
    }
}

fn has_model_files(dir: &Path) -> bool {
    dir.join("config.json").is_file() || dir.join("modules.json").is_file()
}

fn find_in_roots(roots: &[PathBuf], repo_id: &str) -> Option<PathBuf> {
    if repo_id.is_empty() || repo_id.contains("..") {
        return None;
    }
    for root in roots {
        let direct = root.join(repo_id);
        if direct.is_dir() && has_model_files(&direct) {
            return Some(direct);
        }
        let cache = root.join(format!("models--{}", repo_id.replace('/', "--")));
        let snapshots = cache.join("snapshots");
        if !snapshots.is_dir() {
            continue;
        }
        if let Ok(revision) = std::fs::read_to_string(cache.join("refs").join("main")) {
            let snapshot = snapshots.join(revision.trim());
            if snapshot.is_dir() && has_model_files(&snapshot) {
                return Some(snapshot);
            }
        }
        if let Ok(entries) = std::fs::read_dir(&snapshots) {
            let mut dirs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir() && has_model_files(p))
                .collect();
            dirs.sort();
            if let Some(first) = dirs.into_iter().next() {
                return Some(first);
            }
        }
    }
    None
}

// ----- local models -----------------------------------------------------------------------

/// `encode`'s `batch_size` option (default 32).
fn batch_size(request: &EmbedRequest<'_>) -> usize {
    request
        .options
        .and_then(|o| o.get("batch_size"))
        .and_then(Value::as_u64)
        .map_or(32, |b| b.max(1) as usize)
}

/// `SentenceTransformer(model_name_or_path, device=device)` for BERT models on disk.
pub struct LocalSentenceTransformer {
    model_name: String,
    encoder: BertSentenceEncoder,
}

impl fmt::Debug for LocalSentenceTransformer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalSentenceTransformer")
            .field("model_name", &self.model_name)
            .field("dir", &self.encoder.dir())
            .finish()
    }
}

impl LocalSentenceTransformer {
    /// Load the model named (or located at) `model_name_or_path`. A model that is not on disk
    /// is an `OSError`, as sentence-transformers raises when it cannot fetch a model.
    pub fn load(
        model_name_or_path: &str,
        device: &str,
        locator: &ModelLocator,
    ) -> Result<LocalSentenceTransformer, SemanticaError> {
        let dir = locator
            .find_sentence_transformer(model_name_or_path)
            .ok_or_else(|| {
                SemanticaError::Os(OsError::new(
                    "OSError",
                    format!(
                        "We couldn't find '{model_name_or_path}' in the local model directories \
                         and the Rust port does not download models."
                    ),
                ))
            })?;
        let encoder = BertSentenceEncoder::load(&dir, device, Layout::SentenceTransformers)?;
        Ok(LocalSentenceTransformer {
            model_name: model_name_or_path.to_string(),
            encoder,
        })
    }

    /// The model directory.
    pub fn dir(&self) -> &Path {
        self.encoder.dir()
    }

    /// The maximum sequence length (tokens).
    pub fn max_seq_length(&self) -> usize {
        self.encoder.max_seq_length()
    }
}

impl EmbeddingProvider for LocalSentenceTransformer {
    /// `model.encode(texts, normalize_embeddings=..., batch_size=...)`.
    fn embed(
        &self,
        texts: &[String],
        request: &EmbedRequest<'_>,
    ) -> Result<Vec<Vec<f32>>, SemanticaError> {
        self.encoder.encode(
            texts,
            batch_size(request),
            request.normalize_embeddings.unwrap_or(false),
        )
    }

    fn dimension(&self) -> Option<usize> {
        Some(self.encoder.dimension())
    }

    fn name(&self) -> String {
        self.model_name.clone()
    }
}

/// How fastembed post-processes a model's output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FastEmbedKind {
    /// `OnnxTextEmbedding`: CLS token, L2-normalised.
    ClsNormalized,
    /// `PooledNormalizedEmbedding`: mean pooling, L2-normalised.
    MeanNormalized,
    /// `PooledEmbedding`: mean pooling.
    Mean,
    /// A model class the port has no BERT equivalent for (CLIP, SigLIP, Jina v3, Gemma,
    /// Granite, Qwen3).
    Unsupported,
}

/// fastembed 0.9's `TextEmbedding.list_supported_models()`: name, dimension, post-processing.
pub const FASTEMBED_MODELS: [(&str, usize, FastEmbedKind); 41] = [
    ("BAAI/bge-base-en", 768, FastEmbedKind::ClsNormalized),
    ("BAAI/bge-base-en-v1.5", 768, FastEmbedKind::ClsNormalized),
    ("BAAI/bge-large-en-v1.5", 1024, FastEmbedKind::ClsNormalized),
    ("BAAI/bge-small-en", 384, FastEmbedKind::ClsNormalized),
    ("BAAI/bge-small-en-v1.5", 384, FastEmbedKind::ClsNormalized),
    ("BAAI/bge-small-zh-v1.5", 512, FastEmbedKind::ClsNormalized),
    ("mixedbread-ai/mxbai-embed-large-v1", 1024, FastEmbedKind::ClsNormalized),
    ("snowflake/snowflake-arctic-embed-xs", 384, FastEmbedKind::ClsNormalized),
    ("snowflake/snowflake-arctic-embed-s", 384, FastEmbedKind::ClsNormalized),
    ("snowflake/snowflake-arctic-embed-m", 768, FastEmbedKind::ClsNormalized),
    ("snowflake/snowflake-arctic-embed-m-long", 768, FastEmbedKind::ClsNormalized),
    ("snowflake/snowflake-arctic-embed-l", 1024, FastEmbedKind::ClsNormalized),
    ("jinaai/jina-clip-v1", 768, FastEmbedKind::ClsNormalized),
    ("Qdrant/constella-zero", 1024, FastEmbedKind::ClsNormalized),
    ("Qdrant/stella-en-400M-v5-doc-onnx", 1024, FastEmbedKind::ClsNormalized),
    ("minishlab/potion-base-8M", 256, FastEmbedKind::ClsNormalized),
    ("minishlab/potion-retrieval-32M", 512, FastEmbedKind::ClsNormalized),
    ("minishlab/potion-multilingual-128M", 256, FastEmbedKind::ClsNormalized),
    ("Qdrant/clip-ViT-B-32-text", 512, FastEmbedKind::Unsupported),
    ("google/siglip2-base-patch16-224", 768, FastEmbedKind::Unsupported),
    ("sentence-transformers/all-MiniLM-L6-v2", 384, FastEmbedKind::MeanNormalized),
    ("jinaai/jina-embeddings-v2-base-en", 768, FastEmbedKind::MeanNormalized),
    ("jinaai/jina-embeddings-v2-small-en", 512, FastEmbedKind::MeanNormalized),
    ("jinaai/jina-embeddings-v2-base-de", 768, FastEmbedKind::MeanNormalized),
    ("jinaai/jina-embeddings-v2-base-code", 768, FastEmbedKind::MeanNormalized),
    ("jinaai/jina-embeddings-v2-base-zh", 768, FastEmbedKind::MeanNormalized),
    ("jinaai/jina-embeddings-v2-base-es", 768, FastEmbedKind::MeanNormalized),
    ("thenlper/gte-base", 768, FastEmbedKind::MeanNormalized),
    ("thenlper/gte-large", 1024, FastEmbedKind::MeanNormalized),
    ("Qdrant/constella-nano", 1024, FastEmbedKind::MeanNormalized),
    ("nomic-ai/nomic-embed-text-v1.5", 768, FastEmbedKind::Mean),
    ("nomic-ai/nomic-embed-text-v1.5-Q", 768, FastEmbedKind::Mean),
    ("nomic-ai/nomic-embed-text-v1", 768, FastEmbedKind::Mean),
    ("sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2", 384, FastEmbedKind::Mean),
    ("sentence-transformers/paraphrase-multilingual-mpnet-base-v2", 768, FastEmbedKind::Mean),
    ("intfloat/multilingual-e5-large", 1024, FastEmbedKind::Mean),
    ("jinaai/jina-embeddings-v3", 1024, FastEmbedKind::Unsupported),
    ("google/embeddinggemma-300m", 768, FastEmbedKind::Unsupported),
    ("ibm-granite/granite-embedding-small-english-r2", 384, FastEmbedKind::Unsupported),
    ("Qwen/Qwen3-Embedding-0.6B", 1024, FastEmbedKind::Unsupported),
    ("Qwen/Qwen3-Embedding-0.6B-Q", 1024, FastEmbedKind::Unsupported),
];

/// The fastembed model description for `model_name` (matched case-insensitively, as fastembed
/// does).
pub fn fastembed_model(model_name: &str) -> Option<(&'static str, usize, FastEmbedKind)> {
    let lowered = model_name.to_lowercase();
    FASTEMBED_MODELS
        .iter()
        .copied()
        .find(|(name, _, _)| name.to_lowercase() == lowered)
}

/// `fastembed.TextEmbedding(model_name=...)` for the BERT models it supports, with the
/// original model's weights on disk.
pub struct LocalTextEmbedding {
    model_name: String,
    kind: FastEmbedKind,
    encoder: BertSentenceEncoder,
}

impl fmt::Debug for LocalTextEmbedding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalTextEmbedding")
            .field("model_name", &self.model_name)
            .field("kind", &self.kind)
            .field("dir", &self.encoder.dir())
            .finish()
    }
}

impl LocalTextEmbedding {
    /// Load a model fastembed supports. An unknown name is fastembed's `ValueError`; a known
    /// model whose weights are not on disk cannot be loaded (fastembed would download it).
    pub fn load(
        model_name: &str,
        locator: &ModelLocator,
    ) -> Result<LocalTextEmbedding, SemanticaError> {
        let Some((canonical, _dim, kind)) = fastembed_model(model_name) else {
            return Err(SemanticaError::Value(format!(
                "Model {model_name} is not supported in TextEmbedding. Please check the \
                 supported models using `TextEmbedding.list_supported_models()`"
            )));
        };
        let (pooling, normalize) = match kind {
            FastEmbedKind::ClsNormalized => (PoolingModes::CLS, true),
            FastEmbedKind::MeanNormalized => (PoolingModes::MEAN, true),
            FastEmbedKind::Mean => (PoolingModes::MEAN, false),
            FastEmbedKind::Unsupported => {
                return Err(SemanticaError::Value(format!(
                    "Model {canonical} is not a BERT model and cannot be run by the Rust port"
                )));
            }
        };
        let dir = locator.find_fastembed(canonical).ok_or_else(|| {
            SemanticaError::Value(format!("Could not load model {canonical} from any source."))
        })?;
        let encoder = BertSentenceEncoder::load(&dir, "cpu", Layout::Fixed { pooling, normalize })?;
        Ok(LocalTextEmbedding {
            model_name: canonical.to_string(),
            kind,
            encoder,
        })
    }

    /// The post-processing fastembed applies to this model.
    pub fn kind(&self) -> FastEmbedKind {
        self.kind
    }

    /// The model directory.
    pub fn dir(&self) -> &Path {
        self.encoder.dir()
    }
}

impl EmbeddingProvider for LocalTextEmbedding {
    /// `list(model.embed(texts))` (fastembed's default batch size is 256).
    fn embed(
        &self,
        texts: &[String],
        _request: &EmbedRequest<'_>,
    ) -> Result<Vec<Vec<f32>>, SemanticaError> {
        self.encoder.encode(texts, 256, false)
    }

    fn dimension(&self) -> Option<usize> {
        Some(self.encoder.dimension())
    }

    fn name(&self) -> String {
        self.model_name.clone()
    }
}

// ----- OpenAI ------------------------------------------------------------------------------

/// The default OpenAI API base URL.
pub const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// A client for OpenAI's embeddings endpoint (`client.embeddings.create`).
pub struct OpenAIClient {
    api_key: String,
    base_url: String,
    organization: Option<String>,
    timeout: Duration,
    http: OnceLock<Result<reqwest::blocking::Client, String>>,
}

impl fmt::Debug for OpenAIClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAIClient")
            .field("base_url", &self.base_url)
            .field("organization", &self.organization)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl OpenAIClient {
    /// `OpenAI(api_key=api_key)`: the base URL comes from `OPENAI_BASE_URL` (default
    /// [`OPENAI_DEFAULT_BASE_URL`]) and the organization from `OPENAI_ORG_ID`, as in the
    /// Python client. Nothing is sent until the first request.
    pub fn new(api_key: impl Into<String>) -> OpenAIClient {
        let env = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
        OpenAIClient {
            api_key: api_key.into(),
            base_url: env("OPENAI_BASE_URL").unwrap_or_else(|| OPENAI_DEFAULT_BASE_URL.to_string()),
            organization: env("OPENAI_ORG_ID"),
            timeout: Duration::from_secs(600),
            http: OnceLock::new(),
        }
    }

    /// A client for an explicit base URL (an OpenAI-compatible server).
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> OpenAIClient {
        OpenAIClient {
            base_url: base_url.into(),
            ..OpenAIClient::new(api_key)
        }
    }

    /// The base URL requests go to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The URL of the embeddings endpoint.
    pub fn endpoint(&self) -> String {
        format!("{}/embeddings", self.base_url.trim_end_matches('/'))
    }

    /// The JSON body of `embeddings.create(model=model, input=input)`: a single text is sent as
    /// a string, several as a list.
    pub fn request_body(model: &str, input: &[String]) -> Value {
        let input = match input {
            [one] => Value::String(one.clone()),
            many => Value::Array(many.iter().cloned().map(Value::String).collect()),
        };
        json!({"model": model, "input": input})
    }

    /// The embeddings of a response body (`response.data[i].embedding`, ordered by `index`).
    pub fn parse_response(body: &Value) -> Result<Vec<Vec<f32>>, SemanticaError> {
        let data = body
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| SemanticaError::Key("data".to_string()))?;
        let mut rows: Vec<(u64, Vec<f32>)> = Vec::with_capacity(data.len());
        for (position, item) in data.iter().enumerate() {
            let index = item
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(position as u64);
            let embedding = item
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| SemanticaError::Key("embedding".to_string()))?;
            let vector = embedding
                .iter()
                .map(|v| {
                    v.as_f64().map(|f| f as f32).ok_or_else(|| {
                        SemanticaError::Type(format!(
                            "embedding values must be numbers, got {}",
                            py_str(v)
                        ))
                    })
                })
                .collect::<Result<Vec<f32>, SemanticaError>>()?;
            rows.push((index, vector));
        }
        rows.sort_by_key(|(index, _)| *index);
        Ok(rows.into_iter().map(|(_, v)| v).collect())
    }

    fn http(&self) -> Result<&reqwest::blocking::Client, SemanticaError> {
        self.http
            .get_or_init(|| {
                reqwest::blocking::Client::builder()
                    .timeout(self.timeout)
                    .build()
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|e| SemanticaError::Runtime(e.clone()))
    }

    fn api_error(message: &str) -> SemanticaError {
        SemanticaError::Other {
            module: "openai".to_string(),
            type_name: "APIError".to_string(),
            message: message.to_string(),
        }
    }
}

impl EmbeddingProvider for OpenAIClient {
    /// `POST {base_url}/embeddings` with `{"model": ..., "input": ...}`. An HTTP error status is
    /// reported like the Python client (`Error code: 401 - {...}`).
    fn embed(
        &self,
        texts: &[String],
        request: &EmbedRequest<'_>,
    ) -> Result<Vec<Vec<f32>>, SemanticaError> {
        let model = request.model.unwrap_or("text-embedding-3-small");
        let body = OpenAIClient::request_body(model, texts);
        let mut builder = self
            .http()?
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&body);
        if let Some(org) = &self.organization {
            builder = builder.header("OpenAI-Organization", org);
        }
        let response = builder.send().map_err(|e| {
            if e.is_timeout() {
                Self::api_error("Request timed out.")
            } else {
                Self::api_error("Connection error.")
            }
        })?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|_| Self::api_error("Connection error."))?;
        let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
        if !status.is_success() {
            return Err(Self::api_error(&format!(
                "Error code: {} - {}",
                status.as_u16(),
                sem_utils::pycompat::fmt::py_repr(&parsed)
            )));
        }
        OpenAIClient::parse_response(&parsed)
    }

    fn name(&self) -> String {
        format!("openai@{}", self.base_url)
    }
}
