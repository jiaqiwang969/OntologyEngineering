//! Ingestion methods (`semantica/ingest/methods.py`): the convenience functions that build an
//! ingestor from the `ingest_config` method configuration plus keyword arguments, the unified
//! [`ingest`] dispatcher with source-type auto-detection, and the default method
//! registrations.
//!
//! Python passes the same `**kwargs` both to the ingestor constructor and to the ingest call;
//! the port does the same with a JSON keyword map ([`Kwargs`]). Keyword clashes the original
//! turns into `TypeError`s (for example `recursive=` with a directory) are reproduced.

use std::path::PathBuf;
use std::sync::Arc;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Value};

use crate::api_ingestor::APIData;
use crate::config::{ingest_config, processing_error, validation_error};
use crate::feed_ingestor::{FeedData, FeedIngestor};
use crate::file_ingestor::{CloudStorageIngestor, FileIngestor, FileObject};
use crate::mcp_ingestor::{MCPData, MCPIngestor};
use crate::public_api_ingestor::{PublicAPIDetection, PublicAPIExample, PublicAPIExamples, PublicAPIIngestor, PublicApiRequest};
use crate::registry::{RegisteredMethod, method_registry};
use crate::repo_ingestor::RepoIngestor;
use crate::ssrf::parse_bool;
use crate::stream_ingestor::{StreamIngestor, StreamOptions, StreamProcessor};
use crate::support::kw::{self, Kwargs};
use crate::support::pypath;
use crate::web_ingestor::{WebContent, WebIngestor};

static LOGGER: Lazy<Logger> = Lazy::new(|| get_logger("ingest_methods"));

/// SCP-like SSH remotes (`user@host:path`) — kept in sync with the repository ingestor.
static SCP_LIKE_REPO_URL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[^@\s]+@[^:\s]+:.+$").expect("valid regex"));

/// `_is_scp_like_repo_source(source)`.
pub fn _is_scp_like_repo_source(source: &str) -> bool {
    SCP_LIKE_REPO_URL_RE.is_match(sem_utils::pycompat::fmt::strip(source))
}

/// A source argument: a string, a `pathlib.Path`, a list of sources or a configuration dict.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// `str`.
    Str(String),
    /// `pathlib.Path`.
    Path(PathBuf),
    /// `list`.
    List(Vec<Source>),
    /// `dict` (stream, email, database and MCP configurations).
    Dict(Map<String, Value>),
}

impl From<&str> for Source {
    fn from(s: &str) -> Source {
        Source::Str(s.to_string())
    }
}

impl From<String> for Source {
    fn from(s: String) -> Source {
        Source::Str(s)
    }
}

impl From<PathBuf> for Source {
    fn from(p: PathBuf) -> Source {
        Source::Path(p)
    }
}

impl From<&std::path::Path> for Source {
    fn from(p: &std::path::Path) -> Source {
        Source::Path(p.to_path_buf())
    }
}

impl From<Map<String, Value>> for Source {
    fn from(m: Map<String, Value>) -> Source {
        Source::Dict(m)
    }
}

impl<T: Into<Source>> From<Vec<T>> for Source {
    fn from(v: Vec<T>) -> Source {
        Source::List(v.into_iter().map(Into::into).collect())
    }
}

impl Source {
    /// The text of a `str` or `Path` source.
    pub fn as_text(&self) -> Option<String> {
        match self {
            Source::Str(s) => Some(s.clone()),
            Source::Path(p) => Some(p.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    /// Python `str(source)`.
    pub fn py_str(&self) -> String {
        match self {
            Source::Str(s) => s.clone(),
            Source::Path(p) => p.to_string_lossy().into_owned(),
            Source::List(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(Source::py_repr)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Source::Dict(m) => sem_utils::pycompat::fmt::py_str(&Value::Object(m.clone())),
        }
    }

    fn py_repr(&self) -> String {
        match self {
            Source::Str(s) => sem_utils::pycompat::fmt::str_repr(s),
            Source::Path(p) => format!(
                "PosixPath({})",
                sem_utils::pycompat::fmt::str_repr(&p.to_string_lossy())
            ),
            other => other.py_str(),
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Source::Str(_) => "str",
            Source::Path(_) => "PosixPath",
            Source::List(_) => "list",
            Source::Dict(_) => "dict",
        }
    }

    /// `Path(source)`: a `TypeError` for anything but `str` / `Path`.
    fn path_text(&self) -> Result<String, SemanticaError> {
        self.as_text().ok_or_else(|| {
            SemanticaError::Type(format!(
                "expected str, bytes or os.PathLike object, not {}",
                self.type_name()
            ))
        })
    }
}

/// What a convenience function returns (the original's `Union[...]` results).
#[derive(Debug, Clone)]
pub enum IngestOutput {
    /// A `FileObject`.
    File(FileObject),
    /// A `WebContent`.
    Web(WebContent),
    /// An `APIData`.
    Api(APIData),
    /// A `PublicAPIDetection`.
    Detection(PublicAPIDetection),
    /// A `PublicAPIExample`.
    Example(PublicAPIExample),
    /// A `FeedData`.
    Feed(FeedData),
    /// A stream processor.
    Stream(Arc<StreamProcessor>),
    /// An `MCPData`.
    Mcp(MCPData),
    /// A list of results.
    List(Vec<IngestOutput>),
    /// A dictionary of results (`ingest()` and dictionary-returning methods).
    Dict(IndexMap<String, IngestOutput>),
    /// Plain JSON data (strings, metadata dictionaries, ...).
    Value(Value),
}

impl IngestOutput {
    /// `result[key]` of a dictionary result.
    pub fn get(&self, key: &str) -> Option<&IngestOutput> {
        match self {
            IngestOutput::Dict(m) => m.get(key),
            _ => None,
        }
    }

    /// The list items of a list result.
    pub fn as_list(&self) -> Option<&[IngestOutput]> {
        match self {
            IngestOutput::List(items) => Some(items),
            _ => None,
        }
    }

    fn dict(key: &str, value: IngestOutput) -> IngestOutput {
        let mut m = IndexMap::new();
        m.insert(key.to_string(), value);
        IngestOutput::Dict(m)
    }
}

/// The convenience functions registered as default methods (compared by identity when a
/// registry lookup returns one of them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    /// [`ingest_file`].
    IngestFile,
    /// [`ingest_web`].
    IngestWeb,
    /// [`ingest_public_api`].
    IngestPublicApi,
    /// [`ingest_feed`].
    IngestFeed,
    /// [`ingest_stream`].
    IngestStream,
    /// [`ingest_repository`].
    IngestRepository,
    /// [`ingest_email`].
    IngestEmail,
    /// [`ingest_database`].
    IngestDatabase,
    /// [`ingest_parquet`].
    IngestParquet,
    /// [`ingest_arrow`].
    IngestArrow,
    /// [`ingest_xml`].
    IngestXml,
    /// [`ingest_mcp`].
    IngestMcp,
    /// [`ingest_ontology`].
    IngestOntology,
    /// [`ingest`].
    Ingest,
}

impl Builtin {
    /// The Python function name.
    pub fn name(&self) -> &'static str {
        match self {
            Builtin::IngestFile => "ingest_file",
            Builtin::IngestWeb => "ingest_web",
            Builtin::IngestPublicApi => "ingest_public_api",
            Builtin::IngestFeed => "ingest_feed",
            Builtin::IngestStream => "ingest_stream",
            Builtin::IngestRepository => "ingest_repository",
            Builtin::IngestEmail => "ingest_email",
            Builtin::IngestDatabase => "ingest_database",
            Builtin::IngestParquet => "ingest_parquet",
            Builtin::IngestArrow => "ingest_arrow",
            Builtin::IngestXml => "ingest_xml",
            Builtin::IngestMcp => "ingest_mcp",
            Builtin::IngestOntology => "ingest_ontology",
            Builtin::Ingest => "ingest",
        }
    }

    /// `func(source, **kwargs)` with the function's default method.
    pub fn call(&self, source: &Source, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
        match self {
            Builtin::IngestFile => ingest_file(source, "file", kwargs),
            Builtin::IngestWeb => ingest_web(source, "url", kwargs),
            Builtin::IngestPublicApi => ingest_public_api(source, "endpoint", kwargs),
            Builtin::IngestFeed => ingest_feed(source, "rss", kwargs),
            Builtin::IngestStream => ingest_stream(source, "kafka", kwargs),
            Builtin::IngestRepository => ingest_repository(source, "git", kwargs),
            Builtin::IngestEmail => ingest_email(source, "imap", kwargs),
            Builtin::IngestDatabase => ingest_database(source, None, kwargs),
            Builtin::IngestParquet => ingest_parquet(source, "file", kwargs),
            Builtin::IngestArrow => ingest_arrow(source, "file", kwargs),
            Builtin::IngestXml => ingest_xml(source, "file", kwargs),
            Builtin::IngestMcp => ingest_mcp(source, "resources", None, kwargs),
            Builtin::IngestOntology => ingest_ontology(source, "file", kwargs),
            Builtin::Ingest => ingest(source, None, None, kwargs),
        }
    }
}

/// The registrations `methods.py` performs at import time, in order.
pub fn default_registrations() -> Vec<(&'static str, &'static str, Builtin)> {
    use Builtin::*;
    vec![
        ("file", "default", IngestFile),
        ("file", "file", IngestFile),
        ("file", "directory", IngestFile),
        ("file", "cloud", IngestFile),
        ("web", "default", IngestWeb),
        ("web", "url", IngestWeb),
        ("web", "sitemap", IngestWeb),
        ("web", "crawl", IngestWeb),
        ("public_api", "default", IngestPublicApi),
        ("public_api", "endpoint", IngestPublicApi),
        ("public_api", "example", IngestPublicApi),
        ("public_api", "sample", IngestPublicApi),
        ("public_api", "detect", IngestPublicApi),
        ("public_api", "detection", IngestPublicApi),
        ("public_api", "batch", IngestPublicApi),
        ("public_api", "examples", IngestPublicApi),
        ("public_api", "list_examples", IngestPublicApi),
        ("api", "public", IngestPublicApi),
        ("api", "endpoint", IngestPublicApi),
        ("feed", "default", IngestFeed),
        ("feed", "rss", IngestFeed),
        ("feed", "atom", IngestFeed),
        ("feed", "discover", IngestFeed),
        ("stream", "default", IngestStream),
        ("stream", "kafka", IngestStream),
        ("stream", "rabbitmq", IngestStream),
        ("stream", "kinesis", IngestStream),
        ("stream", "pulsar", IngestStream),
        ("repo", "default", IngestRepository),
        ("repo", "git", IngestRepository),
        ("repo", "clone", IngestRepository),
        ("repo", "analyze", IngestRepository),
        ("email", "default", IngestEmail),
        ("email", "imap", IngestEmail),
        ("email", "pop3", IngestEmail),
        ("db", "default", IngestDatabase),
        ("db", "postgresql", IngestDatabase),
        ("db", "mysql", IngestDatabase),
        ("db", "sqlite", IngestDatabase),
        ("db", "oracle", IngestDatabase),
        ("db", "mssql", IngestDatabase),
        ("parquet", "default", IngestParquet),
        ("parquet", "file", IngestParquet),
        ("parquet", "directory", IngestParquet),
        ("parquet", "schema", IngestParquet),
        ("parquet", "metadata", IngestParquet),
        ("file", "parquet", IngestParquet),
        ("arrow", "default", IngestArrow),
        ("arrow", "file", IngestArrow),
        ("arrow", "schema", IngestArrow),
        ("arrow", "metadata", IngestArrow),
        ("file", "arrow", IngestArrow),
        ("xml", "default", IngestXml),
        ("xml", "file", IngestXml),
        ("xml", "directory", IngestXml),
        ("xml", "metadata", IngestXml),
        ("xml", "validate", IngestXml),
        ("xml", "validation", IngestXml),
        ("file", "xml", IngestXml),
        ("mcp", "default", IngestMcp),
        ("mcp", "resources", IngestMcp),
        ("mcp", "tools", IngestMcp),
        ("mcp", "all", IngestMcp),
        ("ontology", "default", IngestOntology),
        ("ontology", "file", IngestOntology),
        ("ontology", "directory", IngestOntology),
        ("ingest", "default", Ingest),
        ("ingest", "unified", Ingest),
    ]
}

/// `_missing_optional_dependency(feature, package)`.
pub fn _missing_optional_dependency(feature: &str, package: &str) -> SemanticaError {
    sem_utils::ConfigurationError::new(format!(
        "{feature} requires optional dependency '{package}'. Install it before using this ingestion backend."
    ))
    .into()
}

/// Runs a registered custom method for `(task, method)` unless it is `this` function; a
/// failure is logged and the default implementation runs instead.
fn try_custom(
    task: &str,
    method: &str,
    this: Builtin,
    source: &Source,
    kwargs: &Kwargs,
) -> Option<IngestOutput> {
    let custom = method_registry_get(task, method)?;
    if custom == RegisteredMethod::Builtin(this) {
        return None;
    }
    match custom.call(source, kwargs) {
        Ok(result) => Some(result),
        Err(e) => {
            LOGGER.warning(format!(
                "Custom method {method} failed: {e}, falling back to default"
            ));
            None
        }
    }
}

fn method_registry_get(task: &str, name: &str) -> Option<RegisteredMethod> {
    let _ = method_registry();
    crate::registry::MethodRegistry::get(task, name)
}

/// `except ConfigurationError: raise / except Exception as e: logger.error(...); raise`.
fn logged<T>(what: &str, result: Result<T, SemanticaError>) -> Result<T, SemanticaError> {
    if let Err(e) = &result
        && e.type_name() != "ConfigurationError"
    {
        LOGGER.error(format!("Failed to ingest {what}: {e}"));
    }
    result
}

fn without(kwargs: &Kwargs, keys: &[&str]) -> Kwargs {
    let mut out = kwargs.clone();
    for k in keys {
        out.remove(*k);
    }
    out
}

/// `ingest_file(source, method="file", **kwargs)`.
pub fn ingest_file(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("file", method, Builtin::IngestFile, source, kwargs) {
        return Ok(r);
    }
    logged("file", (|| {
        let config = ingest_config().merged_method_config("file", kwargs);
        let mut ingestor = FileIngestor::new(config.clone());
        let source_path = source.as_text().map(|s| pypath::normalize(&s));
        let is_file = source_path.as_deref().is_some_and(pypath::is_file);
        let is_dir = source_path.as_deref().is_some_and(pypath::is_dir);
        if method == "file" && is_file {
            let path = source_path.unwrap_or_default();
            return Ok(IngestOutput::File(ingestor.ingest_file(&path, kwargs)?));
        }
        if method == "directory" || is_dir {
            let recursive = match kwargs.get("recursive") {
                Some(v) => kw::truthy(v),
                None => kw::truthy(&ingest_config().get("recursive", Value::Bool(true))),
            };
            if kwargs.contains_key("recursive") {
                return Err(SemanticaError::Type(
                    "semantica.ingest.file_ingestor.FileIngestor.ingest_directory() got multiple values for keyword argument 'recursive'"
                        .to_string(),
                ));
            }
            let dir = match &source_path {
                Some(p) => p.clone(),
                None => source.path_text()?,
            };
            let objects = ingestor.ingest_directory(&dir, recursive, kwargs)?;
            return Ok(IngestOutput::List(objects.into_iter().map(IngestOutput::File).collect()));
        }
        if method == "cloud" {
            let provider = kw::get_str(kwargs, "provider").unwrap_or_else(|| "s3".to_string());
            let mut cloud_config = config.clone();
            cloud_config.remove("provider");
            let _cloud = CloudStorageIngestor::new(&provider, cloud_config)?;
            return Err(SemanticaError::Attribute(
                "'CloudStorageIngestor' object has no attribute 'ingest'".to_string(),
            ));
        }
        let path = source.path_text()?;
        Ok(IngestOutput::File(ingestor.ingest_file(&path, kwargs)?))
    })())
}

/// `ingest_parquet(source, method="file", **kwargs)`.
pub fn ingest_parquet(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("parquet", method, Builtin::IngestParquet, source, kwargs) {
        return Ok(r);
    }
    // PENDING-PORT: parquet_ingestor
    logged("Parquet", Err(SemanticaError::NotImplemented("parquet ingestion pending".to_string())))
}

/// `ingest_arrow(source, method="file", **kwargs)`.
pub fn ingest_arrow(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("arrow", method, Builtin::IngestArrow, source, kwargs) {
        return Ok(r);
    }
    // PENDING-PORT: arrow_ingestor
    logged("Arrow", Err(SemanticaError::NotImplemented("arrow ingestion pending".to_string())))
}

/// `ingest_xml(source, method="file", **kwargs)`.
pub fn ingest_xml(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("xml", method, Builtin::IngestXml, source, kwargs) {
        return Ok(r);
    }
    // PENDING-PORT: xml_ingestor
    logged("XML", Err(SemanticaError::NotImplemented("xml ingestion pending".to_string())))
}

fn web_ingestor(kwargs: &Kwargs) -> Result<WebIngestor, SemanticaError> {
    let mut config = ingest_config().merged_method_config("web", kwargs);
    if let Some(v) = config.get("allow_private_ips").cloned() {
        config.insert("allow_private_ips".to_string(), Value::Bool(parse_bool(&v, false)?));
    }
    WebIngestor::from_kwargs(&config)
}

fn timeout_of(kwargs: &Kwargs) -> Option<f64> {
    kwargs.get("timeout").and_then(Value::as_f64)
}

fn url_text(source: &Source) -> Result<String, SemanticaError> {
    source.as_text().ok_or_else(|| {
        SemanticaError::Type(format!(
            "expected string or bytes-like object, got '{}'",
            source.type_name()
        ))
    })
}

/// `ingest_web(source, method="url", **kwargs)`.
pub fn ingest_web(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("web", method, Builtin::IngestWeb, source, kwargs) {
        return Ok(r);
    }
    logged("web", (|| {
        let mut ingestor = web_ingestor(kwargs)?;
        match method {
            "sitemap" => {
                let max_urls = kw::get_i64(kwargs, "max_urls");
                let fail_fast = kw::get_truthy(kwargs, "fail_fast", false);
                let filters = without(kwargs, &["max_urls", "fail_fast"]);
                let pages = ingestor.crawl_sitemap(&url_text(source)?, max_urls, fail_fast, &filters)?;
                Ok(IngestOutput::List(pages.into_iter().map(IngestOutput::Web).collect()))
            }
            "crawl" => {
                let max_pages = kw::get_i64(kwargs, "max_pages").unwrap_or(100).max(0) as usize;
                let fail_fast = kw::get_truthy(kwargs, "fail_fast", false);
                let pages = ingestor.crawl_domain(&url_text(source)?, max_pages, fail_fast)?;
                Ok(IngestOutput::List(pages.into_iter().map(IngestOutput::Web).collect()))
            }
            _ => match source {
                Source::List(urls) if method == "url" => {
                    let mut out = Vec::new();
                    for url in urls {
                        out.push(IngestOutput::Web(
                            ingestor.ingest_url(&url_text(url)?, timeout_of(kwargs))?,
                        ));
                    }
                    Ok(IngestOutput::List(out))
                }
                _ => Ok(IngestOutput::Web(
                    ingestor.ingest_url(&url_text(source)?, timeout_of(kwargs))?,
                )),
            },
        }
    })())
}

/// `ingest_public_api(source, method="endpoint", **kwargs)` (`http_method` overrides the HTTP
/// method, since `method` selects the ingestion method).
pub fn ingest_public_api(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("public_api", method, Builtin::IngestPublicApi, source, kwargs) {
        return Ok(r);
    }
    logged("public API", (|| {
        let config = ingest_config().merged_method_config("public_api", kwargs);
        let mut ingestor = PublicAPIIngestor::from_kwargs(&config)?;
        let mut request_kwargs = without(
            kwargs,
            &[
                "backoff_factor",
                "delay",
                "fail_fast",
                "max_retries",
                "rate_limit_delay",
                "validate_no_auth",
            ],
        );
        if let Some(http_method) = request_kwargs.remove("http_method")
            && kw::truthy(&http_method)
        {
            request_kwargs.insert("method".to_string(), http_method);
        }
        if method == "examples" || method == "list_examples" {
            let tag = request_kwargs.remove("tag").and_then(|t| t.as_str().map(str::to_string));
            let examples = PublicAPIExamples::list_examples(tag.as_deref());
            return Ok(IngestOutput::dict(
                "examples",
                IngestOutput::List(examples.into_iter().map(IngestOutput::Example).collect()),
            ));
        }
        let request = PublicApiRequest::from_kwargs(&request_kwargs);
        if method == "detect" || method == "detection" {
            if let Source::List(endpoints) = source {
                let mut out = Vec::new();
                for endpoint in endpoints {
                    out.push(IngestOutput::Detection(
                        ingestor.detect_public_api(&url_text(endpoint)?, &request)?,
                    ));
                }
                return Ok(IngestOutput::List(out));
            }
            return Ok(IngestOutput::Detection(
                ingestor.detect_public_api(&url_text(source)?, &request)?,
            ));
        }
        if method == "example" || method == "sample" {
            if let Source::List(names) = source {
                let mut out = Vec::new();
                for name in names {
                    out.push(IngestOutput::Api(
                        ingestor.ingest_example(&url_text(name)?, &request_kwargs)?,
                    ));
                }
                return Ok(IngestOutput::List(out));
            }
            return Ok(IngestOutput::Api(
                ingestor.ingest_example(&url_text(source)?, &request_kwargs)?,
            ));
        }
        if method == "batch" || matches!(source, Source::List(_)) {
            let Source::List(endpoints) = source else {
                return Err(processing_error("Public API batch ingestion requires a list"));
            };
            let endpoints: Vec<String> = endpoints
                .iter()
                .map(url_text)
                .collect::<Result<_, _>>()?;
            let refs: Vec<&str> = endpoints.iter().map(String::as_str).collect();
            let results = ingestor.batch_public_apis(&refs, &request)?;
            return Ok(IngestOutput::List(results.into_iter().map(IngestOutput::Api).collect()));
        }
        Ok(IngestOutput::Api(
            ingestor.ingest_public_api(&url_text(source)?, &request)?,
        ))
    })())
}

/// `ingest_feed(source, method="rss", **kwargs)`.
pub fn ingest_feed(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("feed", method, Builtin::IngestFeed, source, kwargs) {
        return Ok(r);
    }
    logged("feed", (|| {
        let config = ingest_config().merged_method_config("feed", kwargs);
        let mut ingestor = FeedIngestor::new(config)?;
        let timeout = timeout_of(kwargs);
        let validate = kw::get_truthy(kwargs, "validate", true);
        let options = without(kwargs, &["timeout", "validate"]);
        if method == "discover" {
            if let Some(key) = kwargs.keys().next() {
                return Err(SemanticaError::Type(format!(
                    "FeedIngestor.discover_feeds() got an unexpected keyword argument '{key}'"
                )));
            }
            // discover_feeds() logs and returns [] for anything it cannot fetch or parse.
            let feeds = match source.as_text() {
                Some(url) => ingestor.discover_feeds(&url),
                None => Vec::new(),
            };
            return Ok(IngestOutput::Value(Value::from(feeds)));
        }
        if let Source::List(urls) = source {
            let mut out = Vec::new();
            for url in urls {
                out.push(IngestOutput::Feed(ingestor.ingest_feed(
                    &url_text(url)?,
                    timeout,
                    validate,
                    &options,
                )?));
            }
            return Ok(IngestOutput::List(out));
        }
        Ok(IngestOutput::Feed(ingestor.ingest_feed(
            &url_text(source)?,
            timeout,
            validate,
            &options,
        )?))
    })())
}

fn dict_str(source: &Map<String, Value>, key: &str, default: &str) -> String {
    match source.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => sem_utils::pycompat::fmt::py_str(other),
        None => default.to_string(),
    }
}

/// `ingest_stream(source, method="kafka", **kwargs)`.
pub fn ingest_stream(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("stream", method, Builtin::IngestStream, source, kwargs) {
        return Ok(r);
    }
    logged("stream", (|| {
        let config = ingest_config().merged_method_config("stream", kwargs);
        let mut ingestor = StreamIngestor::new(config);
        let empty = Map::new();
        let src = match source {
            Source::Dict(m) => m,
            _ => &empty,
        };
        if !matches!(source, Source::Dict(_)) && ["kafka", "rabbitmq", "kinesis", "pulsar"].contains(&method) {
            return Err(SemanticaError::Attribute(format!(
                "'{}' object has no attribute 'get'",
                source.type_name()
            )));
        }
        let options = StreamOptions::from_kwargs(kwargs);
        let processor = match method {
            "kafka" => {
                let servers: Vec<String> = src
                    .get("bootstrap_servers")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(sem_utils::pycompat::fmt::py_str).collect())
                    .unwrap_or_default();
                ingestor.ingest_kafka(&dict_str(src, "topic", ""), &servers, options)?
            }
            "rabbitmq" => {
                return Err(SemanticaError::Type(
                    "StreamIngestor.ingest_rabbitmq() takes 3 positional arguments but 4 were given"
                        .to_string(),
                ));
            }
            "kinesis" => ingestor.ingest_kinesis(
                &dict_str(src, "stream_name", ""),
                &dict_str(src, "region", "us-east-1"),
                options,
            )?,
            "pulsar" => ingestor.ingest_pulsar(
                &dict_str(src, "topic", ""),
                &dict_str(src, "service_url", "pulsar://localhost:6650"),
                options,
            )?,
            _ => return Err(processing_error(format!("Unknown stream method: {method}"))),
        };
        Ok(IngestOutput::Stream(processor))
    })())
}

/// `ingest_repository(source, method="git", **kwargs)`.
pub fn ingest_repository(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("repo", method, Builtin::IngestRepository, source, kwargs) {
        return Ok(r);
    }
    logged("repository", (|| {
        let config = ingest_config().merged_method_config("repo", kwargs);
        let mut ingestor = RepoIngestor::new(config);
        let is_remote = matches!(source, Source::Str(s)
            if s.starts_with("http://") || s.starts_with("https://") || _is_scp_like_repo_source(s));
        if method == "analyze" && !is_remote {
            return Err(SemanticaError::Attribute(
                "'RepoIngestor' object has no attribute 'analyze_repository'".to_string(),
            ));
        }
        // `_validate_repo_url` rejects anything that is not a `str`.
        let url = match source {
            Source::Str(s) => s.clone(),
            _ => return Err(validation_error("Repository URL must be a non-empty string")),
        };
        let data = ingestor.ingest_repository(&url, kwargs)?;
        Ok(IngestOutput::Dict(
            data.into_iter().map(|(k, v)| (k, IngestOutput::Value(v))).collect(),
        ))
    })())
}

/// `ingest_email(source, method="imap", **kwargs)`.
pub fn ingest_email(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("email", method, Builtin::IngestEmail, source, kwargs) {
        return Ok(r);
    }
    // PENDING-PORT: email_ingestor
    logged("email", Err(SemanticaError::NotImplemented("email ingestion pending".to_string())))
}

/// `ingest_ontology(source, method="file", **kwargs)`.
pub fn ingest_ontology(source: &Source, method: &str, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("ontology", method, Builtin::IngestOntology, source, kwargs) {
        return Ok(r);
    }
    // PENDING-PORT: ontology_ingestor
    logged("ontology", Err(SemanticaError::NotImplemented("ontology ingestion pending".to_string())))
}

/// `ingest_database(source, method=None, **kwargs)`.
pub fn ingest_database(source: &Source, method: Option<&str>, kwargs: &Kwargs) -> Result<IngestOutput, SemanticaError> {
    if let Some(m) = method.filter(|m| !m.is_empty())
        && let Some(r) = try_custom("db", m, Builtin::IngestDatabase, source, kwargs)
    {
        return Ok(r);
    }
    // PENDING-PORT: db_ingestor
    logged("database", Err(SemanticaError::NotImplemented("database ingestion pending".to_string())))
}

/// `ingest_mcp(source, method="resources", server_name=None, **kwargs)`.
pub fn ingest_mcp(
    source: &Source,
    method: &str,
    server_name: Option<&str>,
    kwargs: &Kwargs,
) -> Result<IngestOutput, SemanticaError> {
    if let Some(r) = try_custom("mcp", method, Builtin::IngestMcp, source, kwargs) {
        return Ok(r);
    }
    logged("MCP", (|| {
        let config = ingest_config().merged_method_config("mcp", kwargs);
        let mut ingestor = MCPIngestor::new(config);
        let headers_of = |v: Option<&Value>| {
            v.and_then(Value::as_object).map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), sem_utils::pycompat::fmt::py_str(v)))
                    .collect::<IndexMap<String, String>>()
            })
        };
        let server_name: String = match source {
            Source::Str(s) => {
                if ["http://", "https://", "mcp://", "sse://"]
                    .iter()
                    .any(|p| s.starts_with(p))
                {
                    let name = match server_name.filter(|n| !n.is_empty()) {
                        Some(n) => n.to_string(),
                        None => format!("mcp_server_{}", ingestor.get_connected_servers().len()),
                    };
                    ingestor.connect(&name, Some(s), headers_of(kwargs.get("headers")), &Kwargs::new())?;
                    name
                } else {
                    if !ingestor.is_connected(s) {
                        return Err(processing_error(format!(
                            "MCP server '{s}' not connected. Provide MCP server URL: http://, https://, or mcp://"
                        )));
                    }
                    s.clone()
                }
            }
            Source::Dict(m) => {
                let name = match server_name.filter(|n| !n.is_empty()) {
                    Some(n) => n.to_string(),
                    None => match m.get("server_name") {
                        Some(Value::String(s)) => s.clone(),
                        Some(other) => sem_utils::pycompat::fmt::py_str(other),
                        None => format!("mcp_server_{}", ingestor.get_connected_servers().len()),
                    },
                };
                let url = match m.get("url") {
                    Some(u) if kw::truthy(u) => sem_utils::pycompat::fmt::py_str(u),
                    _ => {
                        return Err(processing_error(
                            "MCP server URL is required. Provide 'url' in configuration dict: http://, https://, or mcp://",
                        ));
                    }
                };
                let extra: Kwargs = m
                    .iter()
                    .filter(|(k, _)| !["url", "headers", "server_name"].contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                ingestor.connect(&name, Some(&url), headers_of(m.get("headers")), &extra)?;
                name
            }
            _ => {
                return Err(processing_error(
                    "Source must be MCP server URL (str), configuration dict with 'url' key, or server name (str) if already connected",
                ));
            }
        };
        match method {
            "all" => {
                let uris = kw::get_str_list(kwargs, "resource_uris");
                let data = ingestor.ingest_resources(&server_name, uris.as_deref(), None)?;
                Ok(IngestOutput::List(data.into_iter().map(IngestOutput::Mcp).collect()))
            }
            "resources" => {
                if kwargs.contains_key("resource_uris") {
                    return Err(SemanticaError::Type(
                        "semantica.ingest.mcp_ingestor.MCPIngestor.ingest_resources() got multiple values for keyword argument 'resource_uris'"
                            .to_string(),
                    ));
                }
                let data = ingestor.ingest_resources(&server_name, None, None)?;
                Ok(IngestOutput::List(data.into_iter().map(IngestOutput::Mcp).collect()))
            }
            "tools" => {
                if !kwargs.get("tool_name").is_some_and(kw::truthy) {
                    return Err(processing_error("tool_name required for tools method"));
                }
                // `ingest_tool_output(server_name, tool_name, tool_arguments, **kwargs)` with
                // `tool_name` also inside `kwargs`.
                Err(SemanticaError::Type(
                    "MCPIngestor.ingest_tool_output() got multiple values for argument 'tool_name'"
                        .to_string(),
                ))
            }
            _ => Err(processing_error(format!("Unknown MCP method: {method}"))),
        }
    })())
}

fn detect_source_type(sources: &Source) -> &'static str {
    match sources {
        Source::Str(_) | Source::Path(_) => {
            let source_str = sources.py_str();
            let lower = source_str.to_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                if [".xml", "/feed", "/rss", "/atom"].iter().any(|e| lower.contains(e)) {
                    "feed"
                } else {
                    "web"
                }
            } else if ["postgresql://", "mysql://", "sqlite://", "oracle://", "mssql://"]
                .iter()
                .any(|p| lower.starts_with(p))
            {
                "db"
            } else if _is_scp_like_repo_source(&source_str)
                || lower.starts_with("https://github.com")
                || lower.starts_with("https://gitlab.com")
            {
                "repo"
            } else if [".ttl", ".owl", ".rdf", ".jsonld", ".n3", ".nt"]
                .iter()
                .any(|e| lower.ends_with(e))
            {
                "ontology"
            } else if lower.ends_with(".parquet") || lower.ends_with(".pq") {
                "parquet"
            } else if [".arrow", ".feather", ".ipc"].iter().any(|e| lower.ends_with(e)) {
                "arrow"
            } else if lower.ends_with(".xml") {
                "xml"
            } else {
                "file"
            }
        }
        Source::List(items) if !items.is_empty() => {
            let lowered: Vec<String> = items.iter().map(|s| s.py_str().to_lowercase()).collect();
            if lowered.iter().all(|s| s.ends_with(".parquet") || s.ends_with(".pq")) {
                "parquet"
            } else if lowered
                .iter()
                .all(|s| [".arrow", ".feather", ".ipc"].iter().any(|e| s.ends_with(e)))
            {
                "arrow"
            } else if lowered.iter().all(|s| s.ends_with(".xml")) {
                "xml"
            } else {
                "file"
            }
        }
        _ => "file",
    }
}

/// `ingest(sources, source_type=None, method=None, **kwargs)`: routes to the matching
/// convenience function and wraps its result under `files`, `content`, `data`, `feeds`,
/// `processor`, `emails`, `xml` or `ontology` (repository results are returned as they are).
pub fn ingest(
    sources: &Source,
    source_type: Option<&str>,
    method: Option<&str>,
    kwargs: &Kwargs,
) -> Result<IngestOutput, SemanticaError> {
    let source_type = match source_type.filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => detect_source_type(sources).to_string(),
    };
    let method = method.filter(|m| !m.is_empty());
    let or = |default: &'static str| method.unwrap_or(default);
    match source_type.as_str() {
        "file" => Ok(IngestOutput::dict("files", ingest_file(sources, or("file"), kwargs)?)),
        "web" => Ok(IngestOutput::dict("content", ingest_web(sources, or("url"), kwargs)?)),
        "public_api" | "api" => Ok(IngestOutput::dict(
            "data",
            ingest_public_api(sources, or("endpoint"), kwargs)?,
        )),
        "feed" => Ok(IngestOutput::dict("feeds", ingest_feed(sources, or("rss"), kwargs)?)),
        "stream" => match sources {
            Source::Dict(_) => Ok(IngestOutput::dict(
                "processor",
                ingest_stream(sources, or("kafka"), kwargs)?,
            )),
            _ => Err(processing_error("Stream ingestion requires configuration dictionary")),
        },
        "repo" => ingest_repository(sources, or("git"), kwargs),
        "email" => match sources {
            Source::Dict(_) => Ok(IngestOutput::dict("emails", ingest_email(sources, or("imap"), kwargs)?)),
            _ => Err(processing_error("Email ingestion requires configuration dictionary")),
        },
        "db" => Ok(IngestOutput::dict("data", ingest_database(sources, method, kwargs)?)),
        "parquet" => Ok(IngestOutput::dict("data", ingest_parquet(sources, or("file"), kwargs)?)),
        "arrow" => Ok(IngestOutput::dict("data", ingest_arrow(sources, or("file"), kwargs)?)),
        "xml" => Ok(IngestOutput::dict("xml", ingest_xml(sources, or("file"), kwargs)?)),
        "ontology" => Ok(IngestOutput::dict(
            "ontology",
            ingest_ontology(sources, or("file"), kwargs)?,
        )),
        "mcp" => Ok(IngestOutput::dict(
            "data",
            ingest_mcp(sources, or("resources"), None, kwargs)?,
        )),
        other => Err(processing_error(format!("Unknown source type: {other}"))),
    }
}

/// `get_ingest_method(task, name)`.
pub fn get_ingest_method(task: &str, name: &str) -> Option<RegisteredMethod> {
    method_registry_get(task, name)
}

/// `list_available_methods(task=None)`.
pub fn list_available_methods(task: Option<&str>) -> IndexMap<String, Vec<String>> {
    crate::registry::MethodRegistry::list_all(task)
}
