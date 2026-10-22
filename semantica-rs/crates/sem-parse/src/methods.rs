//! Parsing convenience functions with method dispatch (`semantica/parse/methods.py`).
//!
//! Each function first looks up a registered method for its task and name
//! ([`crate::registry`]); a registered method that fails is logged and the function falls
//! back to its default parser.
//!
//! The original registers the convenience functions themselves as the `"default"` methods,
//! so calling one with `method="default"` re-dispatches to itself until Python raises
//! `RecursionError`, after which the deepest frames fall back to the default parser and
//! that result (or that parser's exception) propagates back up. The port reaches the same
//! outcome without the recursion: when the registered method is the function's own
//! built-in registration, it runs the default path directly. Cross-task defaults keep
//! their original effect: `parse_pdf`/`parse_docx` go through `parse_document`,
//! `parse_json`/`parse_csv`/`parse_xml` through `parse_structured_data`, and `parse_image`
//! through `parse_media` (which always fails, see [`crate::media_parser`]) before falling
//! back to their own parser.

use std::path::PathBuf;
use std::sync::Arc;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};

use crate::code_parser::CodeParser;
use crate::common::{Options, PResult, Source, bind_kwarg, type_error};
use crate::config::parse_config;
use crate::csv_parser::{CSVData, CSVParser};
use crate::docling_parser::DoclingParser;
use crate::document_parser::DocumentParser;
use crate::docx_parser::DOCXParser;
use crate::email_parser::{EmailData, EmailParser, EmailSource};
use crate::html_parser::HTMLData;
use crate::image_parser::ImageParser;
use crate::json_parser::{JSONData, JSONParser};
use crate::media_parser::MediaParser;
use crate::pdf_parser::PDFParser;
use crate::registry::{MethodArg, ParseMethod, method_registry, parse_method};
use crate::structured_data_parser::StructuredDataParser;
use crate::web_parser::WebParser;
use crate::xml_parser::{XMLData, XMLParser};

/// What a parsing function or registered method returns: the original returns dicts,
/// dataclass instances or lists depending on the parser (`Any`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ParseOutput {
    /// A `dict` result (PDF/DOCX/text documents, structured data, code, images, media,
    /// Docling).
    Dict(Map<String, Value>),
    /// An `HTMLData` instance.
    Html(HTMLData),
    /// An `XMLData` instance.
    Xml(XMLData),
    /// A `JSONData` instance.
    Json(JSONData),
    /// A `CSVData` instance.
    Csv(CSVData),
    /// An `EmailData` instance.
    Email(EmailData),
    /// A list of results (`DocumentParser.parse` on a list).
    List(Vec<ParseOutput>),
    /// Any other JSON-like value a custom method returns.
    Value(Value),
}

impl ParseOutput {
    /// `type(result).__name__`.
    pub fn type_name(&self) -> &'static str {
        match self {
            ParseOutput::Dict(_) => "dict",
            ParseOutput::Html(_) => "HTMLData",
            ParseOutput::Xml(_) => "XMLData",
            ParseOutput::Json(_) => "JSONData",
            ParseOutput::Csv(_) => "CSVData",
            ParseOutput::Email(_) => "EmailData",
            ParseOutput::List(_) => "list",
            ParseOutput::Value(v) => sem_utils::pycompat::fmt::type_name(v),
        }
    }

    /// The result as a dict, if it is one.
    pub fn as_dict(&self) -> Option<&Map<String, Value>> {
        match self {
            ParseOutput::Dict(m) => Some(m),
            ParseOutput::Value(Value::Object(m)) => Some(m),
            _ => None,
        }
    }

    /// The result as a dict, consuming it.
    pub fn into_dict(self) -> Option<Map<String, Value>> {
        match self {
            ParseOutput::Dict(m) => Some(m),
            ParseOutput::Value(Value::Object(m)) => Some(m),
            _ => None,
        }
    }

    /// `result.get(key)`: only dicts have `get`; other results raise `AttributeError` as in
    /// Python (`'HTMLData' object has no attribute 'get'`).
    pub fn get(&self, key: &str) -> PResult<Option<&Value>> {
        match self.as_dict() {
            Some(m) => Ok(m.get(key)),
            None => Err(crate::common::attribute_error(self.type_name(), "get")),
        }
    }

    /// The JSON form of the result (dataclasses become their field dictionaries).
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

impl From<Map<String, Value>> for ParseOutput {
    fn from(value: Map<String, Value>) -> Self {
        ParseOutput::Dict(value)
    }
}

static LOGGER: Lazy<Logger> = Lazy::new(|| get_logger("parse_methods"));

fn logger() -> &'static Logger {
    &LOGGER
}

/// Bind `name` from the positional argument at `index` or from the keywords (Python's
/// call binding), removing it from the keywords.
fn bind(args: &[MethodArg], index: usize, kwargs: &mut Options, name: &str) -> PResult<MethodArg> {
    let positional = args.get(index).cloned();
    let keyword = kwargs.shift_remove(name);
    match (positional, keyword) {
        (Some(_), Some(_)) => Err(type_error(format!(
            "got multiple values for argument '{name}'"
        ))),
        (Some(p), None) => Ok(p),
        (None, Some(Value::String(s))) => Ok(MethodArg::Str(s)),
        (None, Some(Value::Null)) | (None, None) => Ok(MethodArg::None),
        (None, Some(other)) => Ok(MethodArg::Str(sem_utils::pycompat::fmt::py_str(&other))),
    }
}

fn required_source(value: MethodArg, function: &str, name: &str) -> PResult<Source> {
    match value {
        MethodArg::Str(s) => Ok(Source::Text(s)),
        MethodArg::Path(p) => Ok(Source::Path(p)),
        MethodArg::Bytes(_) => Err(type_error(format!(
            "{function}() argument '{name}' must be str or Path, not bytes"
        ))),
        MethodArg::None => Err(type_error(format!(
            "{function}() missing 1 required positional argument: '{name}'"
        ))),
    }
}

fn method_name(kwargs: &mut Options) -> String {
    match kwargs.shift_remove("method") {
        Some(Value::String(s)) => s,
        _ => "default".to_string(),
    }
}

macro_rules! builtin {
    ($name:ident, $body:expr) => {
        static $name: Lazy<ParseMethod> = Lazy::new(|| parse_method($body));
    };
}

builtin!(BUILTIN_PARSE_DOCUMENT, |args, kwargs| {
    let mut kwargs = kwargs.clone();
    let file_path = required_source(
        bind(args, 0, &mut kwargs, "file_path")?,
        "parse_document",
        "file_path",
    )?;
    let file_type = bind(args, 1, &mut kwargs, "file_type")?.as_opt_str();
    let method = match args.get(2) {
        Some(m) => m.as_opt_str().unwrap_or_else(|| "default".to_string()),
        None => method_name(&mut kwargs),
    };
    parse_document(file_path, file_type.as_deref(), Some(&method), &kwargs)
});

builtin!(BUILTIN_PARSE_DOCUMENT_DOCLING, |args, kwargs| {
    let mut kwargs = kwargs.clone();
    let file_path = required_source(
        bind(args, 0, &mut kwargs, "file_path")?,
        "parse_document_docling",
        "file_path",
    )?;
    let file_type = bind(args, 1, &mut kwargs, "file_type")?.as_opt_str();
    parse_document_docling(file_path, file_type.as_deref(), &kwargs)
});

builtin!(BUILTIN_PARSE_WEB_CONTENT, |args, kwargs| {
    let mut kwargs = kwargs.clone();
    let content = required_source(
        bind(args, 0, &mut kwargs, "content")?,
        "parse_web_content",
        "content",
    )?;
    let content_type = bind(args, 1, &mut kwargs, "content_type")?.as_opt_str();
    let base_url = bind(args, 2, &mut kwargs, "base_url")?.as_opt_str();
    let method = match args.get(3) {
        Some(m) => m.as_opt_str().unwrap_or_else(|| "default".to_string()),
        None => method_name(&mut kwargs),
    };
    // A positional `content_type` of `None` is passed through as `None` (not "html").
    let content_type = match (args.get(1), content_type) {
        (Some(MethodArg::None), _) => Some(String::new()),
        (_, ct) => ct,
    };
    parse_web_content_inner(
        content,
        content_type,
        base_url.as_deref(),
        Some(&method),
        &kwargs,
    )
});

builtin!(BUILTIN_PARSE_STRUCTURED_DATA, |args, kwargs| {
    let mut kwargs = kwargs.clone();
    let data = required_source(
        bind(args, 0, &mut kwargs, "data")?,
        "parse_structured_data",
        "data",
    )?;
    let data_format = bind(args, 1, &mut kwargs, "data_format")?.as_opt_str();
    let method = match args.get(2) {
        Some(m) => m.as_opt_str().unwrap_or_else(|| "default".to_string()),
        None => method_name(&mut kwargs),
    };
    parse_structured_data(data, data_format.as_deref(), Some(&method), &kwargs)
});

builtin!(BUILTIN_PARSE_EMAIL, |args, kwargs| {
    let mut kwargs = kwargs.clone();
    let content = match bind(args, 0, &mut kwargs, "email_content")? {
        MethodArg::Str(s) => EmailSource::Text(s),
        MethodArg::Path(p) => EmailSource::Path(p),
        MethodArg::Bytes(b) => EmailSource::Bytes(b),
        MethodArg::None => {
            return Err(type_error(
                "parse_email() missing 1 required positional argument: 'email_content'",
            ));
        }
    };
    let method = match args.get(1) {
        Some(m) => m.as_opt_str().unwrap_or_else(|| "default".to_string()),
        None => method_name(&mut kwargs),
    };
    parse_email(content, Some(&method), &kwargs)
});

builtin!(BUILTIN_PARSE_CODE, |args, kwargs| {
    let mut kwargs = kwargs.clone();
    let file_path = required_source(
        bind(args, 0, &mut kwargs, "file_path")?,
        "parse_code",
        "file_path",
    )?;
    let language = bind(args, 1, &mut kwargs, "language")?.as_opt_str();
    let method = match args.get(2) {
        Some(m) => m.as_opt_str().unwrap_or_else(|| "default".to_string()),
        None => method_name(&mut kwargs),
    };
    parse_code(file_path, language.as_deref(), Some(&method), &kwargs)
});

builtin!(BUILTIN_PARSE_MEDIA, |args, kwargs| {
    let mut kwargs = kwargs.clone();
    let file_path = required_source(
        bind(args, 0, &mut kwargs, "file_path")?,
        "parse_media",
        "file_path",
    )?;
    let media_type = bind(args, 1, &mut kwargs, "media_type")?.as_opt_str();
    let method = match args.get(2) {
        Some(m) => m.as_opt_str().unwrap_or_else(|| "default".to_string()),
        None => method_name(&mut kwargs),
    };
    parse_media(file_path, media_type.as_deref(), Some(&method), &kwargs)
});

/// The registrations `methods.py` performs when it is imported, in order: `"docling"` for
/// documents (the `docling_parser` module always imports), then the `"default"` methods.
pub(crate) fn import_time_registrations() -> Vec<(&'static str, &'static str, ParseMethod)> {
    vec![
        (
            "document",
            "docling",
            BUILTIN_PARSE_DOCUMENT_DOCLING.clone(),
        ),
        ("document", "default", BUILTIN_PARSE_DOCUMENT.clone()),
        ("web", "default", BUILTIN_PARSE_WEB_CONTENT.clone()),
        (
            "structured",
            "default",
            BUILTIN_PARSE_STRUCTURED_DATA.clone(),
        ),
        ("email", "default", BUILTIN_PARSE_EMAIL.clone()),
        ("code", "default", BUILTIN_PARSE_CODE.clone()),
        ("media", "default", BUILTIN_PARSE_MEDIA.clone()),
    ]
}

/// The outcome of looking up a registered method.
enum Dispatch {
    /// No method registered under that name.
    None,
    /// The function's own built-in registration (Python recurses and falls back).
    SelfCall,
    /// Another method.
    Method(ParseMethod),
}

fn lookup(task: &str, method: &str, own: &Lazy<ParseMethod>) -> Dispatch {
    match method_registry.get(task, method) {
        None => Dispatch::None,
        Some(m) if Arc::ptr_eq(&m, own) => Dispatch::SelfCall,
        Some(m) => Dispatch::Method(m),
    }
}

/// Run a registered method; on failure log the original's warning and return `None`.
fn try_custom(
    dispatch: Dispatch,
    method: &str,
    call: impl FnOnce(&ParseMethod) -> PResult<ParseOutput>,
) -> Option<PResult<ParseOutput>> {
    match dispatch {
        Dispatch::None => None,
        Dispatch::SelfCall => {
            logger().warning(format!(
                "Custom method {method} failed: maximum recursion depth exceeded while calling a Python object, falling back to default"
            ));
            None
        }
        Dispatch::Method(m) => match call(&m) {
            Ok(v) => Some(Ok(v)),
            Err(e) => {
                logger().warning(format!(
                    "Custom method {method} failed: {e}, falling back to default"
                ));
                None
            }
        },
    }
}

/// `Parser(**config)` for the parsers whose `__init__` is `(self, config=None, **kwargs)`:
/// a `"config"` keyword binds to the `config` parameter.
fn split_config(config: Options) -> PResult<(Option<Options>, Options)> {
    let (cfg, rest) = bind_kwarg(&config, "config");
    match cfg {
        None | Some(Value::Null) => Ok((None, rest)),
        Some(Value::Object(m)) => Ok((if m.is_empty() { None } else { Some(m) }, rest)),
        Some(other) => Err(crate::common::attribute_error(
            sem_utils::pycompat::fmt::type_name(&other),
            "update",
        )),
    }
}

fn log_and_raise<T>(context: &str, result: PResult<T>) -> PResult<T> {
    result.map_err(|e| {
        logger().error(format!("{context}: {e}"));
        e
    })
}

/// `parse_document(file_path, file_type=None, method="default", **kwargs)`: parse a
/// document of any supported format with [`DocumentParser`].
pub fn parse_document(
    file_path: impl Into<Source>,
    file_type: Option<&str>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    let dispatch = lookup("document", method, &BUILTIN_PARSE_DOCUMENT);
    if let Some(result) = try_custom(dispatch, method, |m| {
        m(
            &[MethodArg::from(&file_path), MethodArg::from(file_type)],
            kwargs,
        )
    }) {
        return result;
    }
    log_and_raise(
        "Failed to parse document",
        (|| {
            let config = parse_config.method_config_updated("document", kwargs);
            let (config, rest) = split_config(config)?;
            let parser = DocumentParser::new(config, rest);
            parser.parse_document(document_path(&file_path), file_type, kwargs)
        })(),
    )
}

fn document_path(source: &Source) -> PathBuf {
    match source {
        Source::Text(s) => PathBuf::from(s),
        Source::Path(p) => p.clone(),
    }
}

/// `parse_document_docling(file_path, file_type=None, **kwargs)`: parse a document with
/// [`DoclingParser`].
pub fn parse_document_docling(
    file_path: impl Into<Source>,
    _file_type: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    log_and_raise(
        "Failed to parse document with Docling",
        (|| {
            let config = parse_config.method_config_updated("document", kwargs);
            let mut parser = DoclingParser::new(config)?;
            parser
                .parse(document_path(&file_path), kwargs)
                .map(ParseOutput::Dict)
        })(),
    )
}

/// `parse_web_content(content, content_type="html", base_url=None, method="default",
/// **kwargs)`: parse HTML or XML web content with [`WebParser`].
pub fn parse_web_content(
    content: impl Into<Source>,
    content_type: Option<&str>,
    base_url: Option<&str>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    parse_web_content_inner(
        content.into(),
        Some(content_type.unwrap_or("html").to_string()),
        base_url,
        method,
        kwargs,
    )
}

/// `content_type`: `Some("")` stands for an explicit `None`.
fn parse_web_content_inner(
    content: Source,
    content_type: Option<String>,
    base_url: Option<&str>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let content_type = content_type.unwrap_or_else(|| "html".to_string());
    let method = method.unwrap_or("default");
    let dispatch = lookup("web", method, &BUILTIN_PARSE_WEB_CONTENT);
    if let Some(result) = try_custom(dispatch, method, |m| {
        let ct = if content_type.is_empty() {
            MethodArg::None
        } else {
            MethodArg::Str(content_type.clone())
        };
        m(
            &[MethodArg::from(&content), ct, MethodArg::from(base_url)],
            kwargs,
        )
    }) {
        return result;
    }
    log_and_raise(
        "Failed to parse web content",
        (|| {
            let config = parse_config.method_config_updated("web", kwargs);
            let (config, rest) = split_config(config)?;
            let parser = WebParser::new(config, rest);
            let ct = if content_type.is_empty() {
                None
            } else {
                Some(content_type.as_str())
            };
            // `parse_web_content(content, content_type=content_type, ...)` with `None`
            // reaches the "Unsupported content type: None" branch.
            match ct {
                Some(ct) => parser.parse_web_content(content.clone(), Some(ct), base_url, kwargs),
                None => {
                    Err(sem_utils::ValidationError::new("Unsupported content type: None").into())
                }
            }
        })(),
    )
}

/// `parse_structured_data(data, data_format=None, method="default", **kwargs)`: parse
/// JSON, CSV, XML or YAML with [`StructuredDataParser`].
pub fn parse_structured_data(
    data: impl Into<Source>,
    data_format: Option<&str>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let data = data.into();
    let method = method.unwrap_or("default");
    let dispatch = lookup("structured", method, &BUILTIN_PARSE_STRUCTURED_DATA);
    if let Some(result) = try_custom(dispatch, method, |m| {
        m(
            &[MethodArg::from(&data), MethodArg::from(data_format)],
            kwargs,
        )
    }) {
        return result;
    }
    log_and_raise(
        "Failed to parse structured data",
        (|| {
            let config = parse_config.method_config_updated("structured", kwargs);
            let (config, rest) = split_config(config)?;
            let parser = StructuredDataParser::new(config, rest);
            parser
                .parse_data(data.clone(), data_format, kwargs)
                .map(ParseOutput::Dict)
        })(),
    )
}

/// `parse_email(email_content, method="default", **kwargs)`: parse an e-mail with
/// [`EmailParser`].
pub fn parse_email(
    email_content: impl Into<EmailSource>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let content: EmailSource = email_content.into();
    let method = method.unwrap_or("default");
    let dispatch = lookup("email", method, &BUILTIN_PARSE_EMAIL);
    if let Some(result) = try_custom(dispatch, method, |m| {
        let arg = match &content {
            EmailSource::Text(s) => MethodArg::Str(s.clone()),
            EmailSource::Bytes(b) => MethodArg::Bytes(b.clone()),
            EmailSource::Path(p) => MethodArg::Path(p.clone()),
        };
        m(&[arg], kwargs)
    }) {
        return result;
    }
    log_and_raise(
        "Failed to parse email",
        (|| {
            let config = parse_config.method_config_updated("email", kwargs);
            let (config, rest) = split_config(config)?;
            let parser = EmailParser::new(config, rest);
            parser
                .parse_email(content.clone(), kwargs)
                .map(ParseOutput::Email)
        })(),
    )
}

/// `parse_code(file_path, language=None, method="default", **kwargs)`: parse source code
/// with [`CodeParser`].
pub fn parse_code(
    file_path: impl Into<Source>,
    language: Option<&str>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    let dispatch = lookup("code", method, &BUILTIN_PARSE_CODE);
    if let Some(result) = try_custom(dispatch, method, |m| {
        m(
            &[MethodArg::from(&file_path), MethodArg::from(language)],
            kwargs,
        )
    }) {
        return result;
    }
    log_and_raise(
        "Failed to parse code",
        (|| {
            let config = parse_config.method_config_updated("code", kwargs);
            let (config, rest) = split_config(config)?;
            let parser = CodeParser::new(config, rest);
            parser
                .parse_code(document_path(&file_path), language, kwargs)
                .map(ParseOutput::Dict)
        })(),
    )
}

/// `parse_media(file_path, media_type=None, method="default", **kwargs)`: parse a media
/// file with [`MediaParser`] (which, as in the original, always fails: see
/// [`MediaParser::parse_media`]).
pub fn parse_media(
    file_path: impl Into<Source>,
    media_type: Option<&str>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    let dispatch = lookup("media", method, &BUILTIN_PARSE_MEDIA);
    if let Some(result) = try_custom(dispatch, method, |m| {
        m(
            &[MethodArg::from(&file_path), MethodArg::from(media_type)],
            kwargs,
        )
    }) {
        return result;
    }
    log_and_raise(
        "Failed to parse media",
        (|| {
            let config = parse_config.method_config_updated("media", kwargs);
            let (config, rest) = split_config(config)?;
            let parser = MediaParser::new(config, rest);
            parser
                .parse_media(document_path(&file_path), media_type, kwargs)
                .map(ParseOutput::Dict)
        })(),
    )
}

/// Run the `"document"`/`"structured"`/`"media"` method registered under `method` with
/// `(file_path, *extra)` (the call `parse_pdf` and friends make).
fn custom_then<F>(
    task: &str,
    method: &str,
    args: Vec<MethodArg>,
    kwargs: &Options,
    fallback: F,
) -> PResult<ParseOutput>
where
    F: FnOnce() -> PResult<ParseOutput>,
{
    if let Some(m) = method_registry.get(task, method) {
        match m(&args, kwargs) {
            Ok(v) => return Ok(v),
            Err(e) => logger().warning(format!(
                "Custom method {method} failed: {e}, falling back to default"
            )),
        }
    }
    fallback()
}

/// `parse_pdf(file_path, method="default", **kwargs)`: the `"document"` method registered
/// under `method` (by default `parse_document`), else [`PDFParser`].
pub fn parse_pdf(
    file_path: impl Into<Source>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    custom_then(
        "document",
        method,
        vec![MethodArg::from(&file_path)],
        kwargs,
        || {
            log_and_raise(
                "Failed to parse PDF",
                (|| {
                    let config = parse_config.method_config_updated("document", kwargs);
                    let parser = PDFParser::new(config);
                    let (pipeline_id, rest) = bind_kwarg(kwargs, "pipeline_id");
                    let pipeline_id = pipeline_id.and_then(|v| v.as_str().map(str::to_string));
                    parser
                        .parse(document_path(&file_path), pipeline_id.as_deref(), &rest)
                        .map(ParseOutput::Dict)
                })(),
            )
        },
    )
}

/// `parse_docx(file_path, method="default", **kwargs)`: the `"document"` method registered
/// under `method` (by default `parse_document`), else [`DOCXParser`].
pub fn parse_docx(
    file_path: impl Into<Source>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    custom_then(
        "document",
        method,
        vec![MethodArg::from(&file_path)],
        kwargs,
        || {
            log_and_raise(
                "Failed to parse DOCX",
                (|| {
                    let config = parse_config.method_config_updated("document", kwargs);
                    let parser = DOCXParser::new(config);
                    let (pipeline_id, rest) = bind_kwarg(kwargs, "pipeline_id");
                    let pipeline_id = pipeline_id.and_then(|v| v.as_str().map(str::to_string));
                    parser
                        .parse(document_path(&file_path), pipeline_id.as_deref(), &rest)
                        .map(ParseOutput::Dict)
                })(),
            )
        },
    )
}

/// `parse_json(file_path, method="default", **kwargs)`: the `"structured"` method
/// registered under `method` (by default `parse_structured_data`, which returns the
/// `JSONData` fields as a dict), else [`JSONParser`].
pub fn parse_json(
    file_path: impl Into<Source>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    custom_then(
        "structured",
        method,
        vec![MethodArg::from(&file_path)],
        kwargs,
        || {
            log_and_raise(
                "Failed to parse JSON",
                (|| {
                    let config = parse_config.method_config_updated("structured", kwargs);
                    let parser = JSONParser::new(config);
                    parser
                        .parse(file_path.clone(), kwargs)
                        .map(ParseOutput::Json)
                })(),
            )
        },
    )
}

/// `parse_csv(file_path, delimiter=",", method="default", **kwargs)`: the `"structured"`
/// method registered under `method`, called as `method(file_path, delimiter, **kwargs)`
/// (so the default `parse_structured_data` receives the delimiter as its `data_format`
/// and fails), else [`CSVParser`]. `delimiter: None` is the original's default `","`.
pub fn parse_csv(
    file_path: impl Into<Source>,
    delimiter: Option<&str>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let delimiter = delimiter.unwrap_or(",");
    let method = method.unwrap_or("default");
    custom_then(
        "structured",
        method,
        vec![
            MethodArg::from(&file_path),
            MethodArg::Str(delimiter.to_string()),
        ],
        kwargs,
        || {
            log_and_raise(
                "Failed to parse CSV",
                (|| {
                    let config = parse_config.method_config_updated("structured", kwargs);
                    let parser = CSVParser::new(config);
                    parser
                        .parse(document_path(&file_path), Some(delimiter), kwargs)
                        .map(ParseOutput::Csv)
                })(),
            )
        },
    )
}

/// `parse_xml(file_path, method="default", **kwargs)`: the `"structured"` method
/// registered under `method` (by default `parse_structured_data`), else [`XMLParser`].
pub fn parse_xml(
    file_path: impl Into<Source>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    custom_then(
        "structured",
        method,
        vec![MethodArg::from(&file_path)],
        kwargs,
        || {
            log_and_raise(
                "Failed to parse XML",
                (|| {
                    let config = parse_config.method_config_updated("structured", kwargs);
                    let parser = XMLParser::new(config);
                    parser
                        .parse(file_path.clone(), kwargs)
                        .map(ParseOutput::Xml)
                })(),
            )
        },
    )
}

/// `parse_image(file_path, method="default", **kwargs)`: the `"media"` method registered
/// under `method` (by default `parse_media`, which fails), else [`ImageParser`].
pub fn parse_image(
    file_path: impl Into<Source>,
    method: Option<&str>,
    kwargs: &Options,
) -> PResult<ParseOutput> {
    let file_path = file_path.into();
    let method = method.unwrap_or("default");
    custom_then(
        "media",
        method,
        vec![MethodArg::from(&file_path)],
        kwargs,
        || {
            log_and_raise(
                "Failed to parse image",
                (|| {
                    let config = parse_config.method_config_updated("media", kwargs);
                    let parser = ImageParser::new(config);
                    parser
                        .parse(document_path(&file_path), kwargs)
                        .map(ParseOutput::Dict)
                })(),
            )
        },
    )
}

/// `get_parse_method(task, name)`: the registered method, or `None`.
pub fn get_parse_method(task: &str, name: &str) -> Option<ParseMethod> {
    method_registry.get(task, name)
}

/// `list_available_methods(task=None)`: task -> registered method names.
pub fn list_available_methods(task: Option<&str>) -> IndexMap<String, Vec<String>> {
    method_registry.list_all(task)
}
