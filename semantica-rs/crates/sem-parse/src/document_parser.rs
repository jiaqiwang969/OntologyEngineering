//! Document format parsing (`semantica/parse/document_parser.py`).
//!
//! [`DocumentParser`] routes PDF, DOCX, HTML and plain-text files to their parsers, extracts
//! text and metadata, and parses batches.
//!
//! As in the original, an HTML document parses to an `HTMLData` object while the other
//! formats give dicts, so results are [`ParseOutput`]s; `extract_metadata` on an HTML file
//! fails with the original's `AttributeError` (`HTMLData` has no `get`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{
    DecodeErrors, Options, PResult, Source, bind_kwarg, merge_config, opt_string, opt_truthy,
    py_exists, py_name, py_path_str, py_read_text, py_suffix, sub_config, type_error, with_opt,
};
use crate::docx_parser::DOCXParser;
use crate::html_parser::HTMLParser;
use crate::methods::ParseOutput;
use crate::pdf_parser::PDFParser;

/// One item of a list passed to [`DocumentParser::parse`]: a path (`str`/`Path`), an
/// object with a `path` attribute (e.g. an ingested `FileObject`), or anything else (which
/// the original rejects with `ValueError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentItem {
    /// A `str` or `Path`.
    Path(Source),
    /// An object with a `.path` attribute.
    WithPath(Source),
    /// An unsupported value, identified by its Python type name (e.g. `int`).
    Unsupported(String),
}

impl From<&str> for DocumentItem {
    fn from(v: &str) -> Self {
        DocumentItem::Path(Source::from(v))
    }
}

impl From<String> for DocumentItem {
    fn from(v: String) -> Self {
        DocumentItem::Path(Source::from(v))
    }
}

impl From<&Path> for DocumentItem {
    fn from(v: &Path) -> Self {
        DocumentItem::Path(Source::from(v))
    }
}

impl From<PathBuf> for DocumentItem {
    fn from(v: PathBuf) -> Self {
        DocumentItem::Path(Source::from(v))
    }
}

/// The `source` argument of [`DocumentParser::parse`]: one path or a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentSource {
    /// A single `str`/`Path`.
    One(Source),
    /// A list of items.
    Many(Vec<DocumentItem>),
}

impl<T: Into<Source>> From<T> for DocumentSource {
    fn from(v: T) -> Self {
        DocumentSource::One(v.into())
    }
}

impl From<Vec<DocumentItem>> for DocumentSource {
    fn from(v: Vec<DocumentItem>) -> Self {
        DocumentSource::Many(v)
    }
}

/// A successfully parsed entry of [`DocumentBatchResult::successful`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchSuccess {
    /// `str(file_path)`.
    pub file_path: String,
    /// The parse result.
    pub result: ParseOutput,
}

/// A failed entry of [`DocumentBatchResult::failed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchFailure {
    /// `str(file_path)`.
    pub file_path: String,
    /// `str(e)`.
    pub error: String,
    /// `type(e).__name__`.
    pub error_type: String,
}

/// The dict `parse_batch` returns: `successful`, `failed`, `total`, `success_count`,
/// `failure_count`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentBatchResult {
    /// `results["successful"]`.
    pub successful: Vec<BatchSuccess>,
    /// `results["failed"]`.
    pub failed: Vec<BatchFailure>,
    /// `results["total"]`.
    pub total: usize,
    /// `results["success_count"]`.
    pub success_count: usize,
    /// `results["failure_count"]`.
    pub failure_count: usize,
}

/// `DocumentParser(config=None, **kwargs)`: document format parsing handler.
#[derive(Debug, Clone)]
pub struct DocumentParser {
    /// `self.logger`.
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.pdf_parser` (`PDFParser(**config.get("pdf", {}))`).
    pub pdf_parser: PDFParser,
    /// `self.docx_parser`.
    pub docx_parser: DOCXParser,
    /// `self.html_parser`.
    pub html_parser: HTMLParser,
    /// `self.supported_formats`: extension -> file type.
    pub supported_formats: IndexMap<String, String>,
    /// `self.progress_tracker` (enabled on construction, as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for DocumentParser {
    fn default() -> Self {
        DocumentParser::new(None, Options::new())
    }
}

impl DocumentParser {
    /// `DocumentParser(config=None, **kwargs)`.
    pub fn new(config: Option<Options>, kwargs: Options) -> Self {
        let config = merge_config(config, kwargs);
        let supported_formats = [
            (".pdf", "pdf"),
            (".docx", "docx"),
            (".doc", "docx"),
            (".html", "html"),
            (".htm", "html"),
            (".txt", "text"),
            (".text", "text"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        DocumentParser {
            logger: get_logger("document_parser"),
            pdf_parser: PDFParser::new(sub_config(&config, "pdf")),
            docx_parser: DOCXParser::new(sub_config(&config, "docx")),
            html_parser: HTMLParser::new(sub_config(&config, "html")),
            config,
            supported_formats,
            progress_tracker,
        }
    }

    /// `parse(source, **options)`: `parse_document` for one path; for a list, `parse_batch`
    /// over the items' paths and the successful results (a `ParseOutput::List`).
    pub fn parse(
        &self,
        source: impl Into<DocumentSource>,
        options: &Options,
    ) -> PResult<ParseOutput> {
        match source.into() {
            DocumentSource::One(path) => {
                let (file_type, rest) = bind_kwarg(options, "file_type");
                let file_type = file_type.and_then(|v| v.as_str().map(str::to_string));
                self.parse_document(source_path(&path), file_type.as_deref(), &rest)
            }
            DocumentSource::Many(items) => {
                let mut paths = Vec::new();
                for item in items {
                    match item {
                        DocumentItem::Path(p) | DocumentItem::WithPath(p) => paths.push(p),
                        DocumentItem::Unsupported(type_name) => {
                            return Err(SemanticaError::Value(format!(
                                "Unsupported item type in list: <class '{type_name}'>"
                            )));
                        }
                    }
                }
                let batch = self.parse_batch(&paths, options)?;
                Ok(ParseOutput::List(
                    batch.successful.into_iter().map(|s| s.result).collect(),
                ))
            }
        }
    }

    /// `parse_document(file_path, file_type=None, **options)`: route the file to the PDF,
    /// DOCX, HTML or text parser (by extension unless `file_type` is given). Parser failures
    /// become `ProcessingError("Failed to parse document: ...")`.
    pub fn parse_document(
        &self,
        file_path: impl AsRef<Path>,
        file_type: Option<&str>,
        options: &Options,
    ) -> PResult<ParseOutput> {
        let file_path = file_path.as_ref();
        let path_text = py_path_str(file_path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&path_text),
            Some("parse"),
            Some("DocumentParser"),
            &format!("Document: {}", py_name(file_path)),
            None,
        );
        let result = (|| -> PResult<ParseOutput> {
            if !py_exists(file_path)? {
                return Err(
                    ValidationError::new(format!("Document file not found: {path_text}")).into(),
                );
            }
            let file_type = match file_type {
                Some(t) => t.to_string(),
                None => self.detect_file_type(file_path),
            };
            self.progress_tracker.update_tracking(
                &tracking_id,
                None,
                &format!("Parsing {file_type} document"),
            );
            let routed = (|| -> PResult<ParseOutput> {
                match file_type.as_str() {
                    "pdf" => {
                        let (pipeline_id, rest) = bind_kwarg(options, "pipeline_id");
                        let pipeline_id = pipeline_id.and_then(|v| v.as_str().map(str::to_string));
                        self.pdf_parser
                            .parse(file_path, pipeline_id.as_deref(), &rest)
                            .map(ParseOutput::Dict)
                    }
                    "docx" => {
                        let (pipeline_id, rest) = bind_kwarg(options, "pipeline_id");
                        let pipeline_id = pipeline_id.and_then(|v| v.as_str().map(str::to_string));
                        self.docx_parser
                            .parse(file_path, pipeline_id.as_deref(), &rest)
                            .map(ParseOutput::Dict)
                    }
                    "html" => {
                        let (base_url, rest) = bind_kwarg(options, "base_url");
                        let base_url = base_url.and_then(|v| v.as_str().map(str::to_string));
                        self.html_parser
                            .parse(
                                Source::Path(file_path.to_path_buf()),
                                base_url.as_deref(),
                                &rest,
                            )
                            .map(ParseOutput::Html)
                    }
                    "text" => self.parse_text(file_path, options).map(ParseOutput::Dict),
                    other => Err(ValidationError::new(format!(
                        "Unsupported document format: {other}"
                    ))
                    .into()),
                }
            })();
            match routed {
                Ok(result) => {
                    self.progress_tracker.stop_tracking(
                        &tracking_id,
                        Some("completed"),
                        &format!("Parsed {} ({file_type})", py_name(file_path)),
                        None,
                    );
                    Ok(result)
                }
                Err(e) => {
                    self.progress_tracker.stop_tracking(
                        &tracking_id,
                        Some("failed"),
                        &e.to_string(),
                        None,
                    );
                    self.logger
                        .error(format!("Failed to parse document {path_text}: {e}"));
                    Err(ProcessingError::new(format!("Failed to parse document: {e}")).into())
                }
            }
        })();
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `extract_text(file_path, **options)`: the document's text (PDF: `pages` option; HTML:
    /// `clean` option; text: `encoding` option).
    pub fn extract_text(&self, file_path: impl AsRef<Path>, options: &Options) -> PResult<String> {
        let file_path = file_path.as_ref();
        match self.detect_file_type(file_path).as_str() {
            "pdf" => {
                // `self.pdf_parser.extract_text(file_path, **options)`: only `pages` binds.
                if let Some(key) = options.keys().find(|k| k.as_str() != "pages") {
                    return Err(type_error(format!(
                        "PDFParser.extract_text() got an unexpected keyword argument '{key}'"
                    )));
                }
                let pages = options
                    .get("pages")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(Value::as_i64).collect::<Vec<i64>>());
                self.pdf_parser.extract_text(file_path, pages.as_deref())
            }
            "docx" => self.docx_parser.extract_text(file_path),
            "html" => self.html_parser.extract_text(
                Source::Path(file_path.to_path_buf()),
                Some(opt_truthy(options, "clean", true)),
            ),
            "text" => {
                let result = self.parse_text(file_path, options)?;
                Ok(match result.get("text") {
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                })
            }
            other => {
                Err(ValidationError::new(format!("Unsupported document format: {other}")).into())
            }
        }
    }

    /// `extract_metadata(file_path)`: `parse_document(..., extract_tables=False,
    /// extract_images=False).get("metadata", {})`.
    pub fn extract_metadata(&self, file_path: impl AsRef<Path>) -> PResult<Value> {
        let file_path = file_path.as_ref();
        let file_type = self.detect_file_type(file_path);
        let options = with_opt(
            &with_opt(&Options::new(), "extract_tables", Value::Bool(false)),
            "extract_images",
            Value::Bool(false),
        );
        let result = self.parse_document(file_path, Some(&file_type), &options)?;
        Ok(result
            .get("metadata")?
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())))
    }

    /// `parse_batch(file_paths, **options)`: parse each file, collecting successes and
    /// failures (`continue_on_error`, default `True`; when false the first failure raises
    /// `ProcessingError("Batch processing failed at <path>: ...")`).
    pub fn parse_batch(
        &self,
        file_paths: &[Source],
        options: &Options,
    ) -> PResult<DocumentBatchResult> {
        let total_files = file_paths.len();
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("parse"),
            Some("DocumentParser"),
            &format!("Parsing {total_files} documents in batch"),
            None,
        );
        let continue_on_error = opt_truthy(options, "continue_on_error", true);
        let update_interval = (total_files / 20).max(1);
        let (file_type, rest) = bind_kwarg(options, "file_type");
        let file_type = file_type.and_then(|v| v.as_str().map(str::to_string));
        let mut results = DocumentBatchResult {
            successful: Vec::new(),
            failed: Vec::new(),
            total: total_files,
            success_count: 0,
            failure_count: 0,
        };
        for (idx, file_path) in file_paths.iter().enumerate() {
            let idx = idx + 1;
            match self.parse_document(source_path(file_path), file_type.as_deref(), &rest) {
                Ok(result) => results.successful.push(BatchSuccess {
                    file_path: file_path.as_py_str(),
                    result,
                }),
                Err(e) => {
                    results.failed.push(BatchFailure {
                        file_path: file_path.as_py_str(),
                        error: e.to_string(),
                        error_type: e.type_name(),
                    });
                    if !continue_on_error {
                        self.progress_tracker.stop_tracking(
                            &tracking_id,
                            Some("failed"),
                            &e.to_string(),
                            None,
                        );
                        let err: SemanticaError = ProcessingError::new(format!(
                            "Batch processing failed at {}: {e}",
                            file_path.as_py_str()
                        ))
                        .into();
                        self.progress_tracker.stop_tracking(
                            &tracking_id,
                            Some("failed"),
                            &err.to_string(),
                            None,
                        );
                        return Err(err);
                    }
                }
            }
            if idx % update_interval == 0 || idx == total_files {
                self.progress_tracker.update_progress(
                    &tracking_id,
                    idx as i64,
                    total_files as i64,
                    &format!("Parsing documents... {idx}/{total_files}"),
                );
            }
        }
        results.success_count = results.successful.len();
        results.failure_count = results.failed.len();
        self.progress_tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Parsed {}/{total_files} documents successfully",
                results.success_count
            ),
            None,
        );
        Ok(results)
    }

    /// `_detect_file_type(file_path)`.
    fn detect_file_type(&self, file_path: &Path) -> String {
        let suffix = py_suffix(file_path).to_lowercase();
        self.supported_formats
            .get(&suffix)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// `_parse_text(file_path, **options)`.
    fn parse_text(&self, file_path: &Path, options: &Options) -> PResult<Map<String, Value>> {
        let encoding = opt_string(options, "encoding", "utf-8");
        match py_read_text(file_path, &encoding, DecodeErrors::Ignore) {
            Ok(text) => {
                let mut metadata = Map::new();
                metadata.insert(
                    "file_path".to_string(),
                    Value::String(py_path_str(file_path)),
                );
                metadata.insert("encoding".to_string(), Value::String(encoding));
                metadata.insert("size".to_string(), Value::from(text.chars().count()));
                let mut out = Map::new();
                out.insert("text".to_string(), Value::String(text.clone()));
                out.insert("metadata".to_string(), Value::Object(metadata));
                out.insert("full_text".to_string(), Value::String(text));
                Ok(out)
            }
            Err(e) => {
                self.logger.error(format!(
                    "Failed to parse text file {}: {e}",
                    py_path_str(file_path)
                ));
                Err(ProcessingError::new(format!("Failed to parse text file: {e}")).into())
            }
        }
    }
}

fn source_path(source: &Source) -> PathBuf {
    match source {
        Source::Text(s) => PathBuf::from(s),
        Source::Path(p) => p.clone(),
    }
}
