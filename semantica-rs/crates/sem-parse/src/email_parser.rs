//! E-mail content parsing (`semantica/parse/email_parser.py`).
//!
//! Parses e-mail messages (RFC 2822 / MIME) into [`EmailData`]: decoded headers
//! ([`EmailHeaders`]), the plain-text and HTML bodies and attachments ([`EmailBody`]), and
//! analyses threads ([`EmailThreadAnalyzer`]). Messages are read with the port of Python's
//! `email` package and its `compat32` policy (see the private `mime` module), so header
//! values, folded lines, encoded words, transfer encodings and charsets behave as in the
//! original, including its failure modes (for instance raw 8-bit bytes in a decoded header
//! make the original fail with `LookupError: unknown encoding: unknown-8bit`).
//!
//! API CONTRACT used by other modules:
//! - `EmailSource` (`Union[str, bytes, Path]`) with `From<&str>`, `From<String>`,
//!   `From<&[u8]>`, `From<Vec<u8>>`, `From<&Path>`, `From<PathBuf>`.
//! - `EmailParser::new(config: Option<Options>, kwargs: Options) -> EmailParser`
//! - `EmailParser::parse_email(&self, email_content: impl Into<EmailSource>, options: &Options) -> PResult<EmailData>`
//! - `EmailData`: `Debug + Clone + PartialEq + Serialize`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::pycompat::fmt::truthy;
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{
    DecodeErrors, Options, PResult, merge_config, py_exists, py_name, py_path_str, py_read_bytes,
    sub_config,
};
use crate::mime::{
    self, HeaderValue, decode_header, decode_header_str, parsedate_to_datetime,
    py_split_whitespace, py_strip,
};

/// A header value as `Message.get` returns it (a string, or an `email.header.Header` for
/// values carrying raw 8-bit bytes).
pub use crate::mime::HeaderValue as EmailHeaderValue;
/// `email.message.Message` (compat32): the parsed message model
/// [`EmailParser::extract_headers`], [`EmailParser::extract_body`] and
/// [`MIMEParser::parse_mime_message`] work with.
pub use crate::mime::Message as EmailMessage;
/// The rest of the `email` package port (compat32) behind this module: `Header` objects,
/// payloads, RFC 2231 parameter values, `decode_header`, `parsedate_to_datetime`, the two
/// parser entry points and the surrogate-escape helpers for text read from bytes.
pub use crate::mime::{
    DecodedPart, EmailDateTime, Header, ParamValue, Payload, decode_header as email_decode_header,
    encode_surrogateescape, has_surrogates, message_from_bytes, message_from_string,
    parsedate_to_datetime as email_parsedate_to_datetime,
};

/// `Union[str, bytes, Path]` e-mail input: a `str` naming an existing file is read from
/// that file (as in the original), any other `str` is the message text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailSource {
    /// A `str`: a file path if such a path exists, otherwise the message itself.
    Text(String),
    /// A `bytes` message.
    Bytes(Vec<u8>),
    /// A `pathlib.Path` to an `.eml` file.
    Path(PathBuf),
}

impl From<&str> for EmailSource {
    fn from(v: &str) -> Self {
        EmailSource::Text(v.to_string())
    }
}
impl From<String> for EmailSource {
    fn from(v: String) -> Self {
        EmailSource::Text(v)
    }
}
impl From<&[u8]> for EmailSource {
    fn from(v: &[u8]) -> Self {
        EmailSource::Bytes(v.to_vec())
    }
}
impl From<Vec<u8>> for EmailSource {
    fn from(v: Vec<u8>) -> Self {
        EmailSource::Bytes(v)
    }
}
impl From<&Path> for EmailSource {
    fn from(v: &Path) -> Self {
        EmailSource::Path(v.to_path_buf())
    }
}
impl From<PathBuf> for EmailSource {
    fn from(v: PathBuf) -> Self {
        EmailSource::Path(v)
    }
}
impl From<&PathBuf> for EmailSource {
    fn from(v: &PathBuf) -> Self {
        EmailSource::Path(v.clone())
    }
}

/// `EmailHeaders` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EmailHeaders {
    /// Decoded `Subject`.
    pub subject: Option<String>,
    /// Decoded `From`.
    pub from_address: Option<String>,
    /// `To`, split on commas.
    pub to_addresses: Vec<String>,
    /// `Cc`, split on commas.
    pub cc_addresses: Vec<String>,
    /// `Bcc`, split on commas.
    pub bcc_addresses: Vec<String>,
    /// `Date` as ISO 8601 when it parses, else the raw value.
    pub date: Option<String>,
    /// `Message-ID` (raw).
    pub message_id: Option<String>,
    /// `In-Reply-To` (raw).
    pub in_reply_to: Option<String>,
    /// `References`, split on whitespace.
    pub references: Vec<String>,
    /// Every other header (decoded), by its name as written.
    pub custom_headers: IndexMap<String, String>,
}

/// One attachment of an [`EmailBody`] (the original's
/// `{"filename", "content_type", "size", "data"}` dict).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EmailAttachment {
    /// Decoded file name.
    pub filename: String,
    /// The part's content type.
    pub content_type: String,
    /// Size of the decoded payload in bytes.
    pub size: usize,
    /// The decoded payload (`None` for a multipart part). Serialised as an array of byte
    /// values (Python `bytes` have no JSON form).
    pub data: Option<Vec<u8>>,
}

/// `EmailBody` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EmailBody {
    /// The (last) `text/plain` body.
    pub text: Option<String>,
    /// The (last) `text/html` body.
    pub html: Option<String>,
    /// Extracted attachments.
    pub attachments: Vec<EmailAttachment>,
}

/// `EmailData` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EmailData {
    /// Parsed headers.
    pub headers: EmailHeaders,
    /// Parsed body.
    pub body: EmailBody,
    /// `{"message_id": ..., "date": ...}`.
    pub metadata: Map<String, Value>,
}

impl EmailData {
    /// `EmailData(headers, body, metadata={})`.
    pub fn new(
        headers: EmailHeaders,
        body: EmailBody,
        metadata: Option<Map<String, Value>>,
    ) -> Self {
        EmailData {
            headers,
            body,
            metadata: metadata.unwrap_or_default(),
        }
    }
}

/// The header names `extract_headers` does not copy into `custom_headers` (compared
/// case-sensitively, as in the original).
const STANDARD_HEADERS: [&str; 9] = [
    "Subject",
    "From",
    "To",
    "Cc",
    "Bcc",
    "Date",
    "Message-ID",
    "In-Reply-To",
    "References",
];

/// `EmailParser`: e-mail content parsing handler.
#[derive(Debug, Clone)]
pub struct EmailParser {
    /// `self.logger` (`semantica.email_parser`).
    pub logger: Logger,
    /// `self.config` (`config` merged with the keyword arguments).
    pub config: Options,
    /// `self.mime_parser`.
    pub mime_parser: MIMEParser,
    /// `self.thread_analyzer`.
    pub thread_analyzer: EmailThreadAnalyzer,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for EmailParser {
    fn default() -> Self {
        EmailParser::new(None, Options::new())
    }
}

/// `decode_header_value` inside `extract_headers`: decode RFC 2047 words and join the
/// parts (bytes decoded with their charset, `utf-8` when none, ignoring errors), stripped.
fn decode_header_value(value: Option<&HeaderValue>) -> PResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parts = decode_header(value)?;
    Ok(Some(join_decoded(parts)?))
}

fn join_decoded(parts: Vec<(DecodedPart, Option<String>)>) -> PResult<String> {
    let mut decoded = String::new();
    for (part, encoding) in parts {
        match part {
            DecodedPart::Bytes(bytes) => decoded.push_str(&mime::bytes_decode(
                &bytes,
                encoding.as_deref().unwrap_or("utf-8"),
                DecodeErrors::Ignore,
            )?),
            DecodedPart::Str(s) => decoded.push_str(&s),
        }
    }
    Ok(py_strip(&decoded).to_string())
}

/// `[addr.strip() for addr in str(header).split(",")]`.
fn split_addresses(value: &HeaderValue) -> Vec<String> {
    value
        .to_string()
        .split(',')
        .map(|a| py_strip(a).to_string())
        .collect()
}

impl EmailParser {
    /// `EmailParser(config=None, **kwargs)`.
    pub fn new(config: Option<Options>, kwargs: Options) -> Self {
        let config = merge_config(config, kwargs);
        EmailParser {
            logger: get_logger("email_parser"),
            mime_parser: MIMEParser::new(sub_config(&config, "mime")),
            thread_analyzer: EmailThreadAnalyzer::new(sub_config(&config, "thread")),
            progress_tracker: get_progress_tracker(),
            config,
        }
    }

    /// `parse_email(email_content, **options)`: parse a message given as text, bytes or a
    /// file path. Option `extract_attachments` (default true).
    ///
    /// Errors: a `str` too long to be a path raises `OSError` (Python's `Path.exists()`), a
    /// missing `Path` raises `ValidationError("Email file not found: ...")`, failures while
    /// extracting headers or body are wrapped in `ProcessingError("Failed to parse email:
    /// ...")`.
    pub fn parse_email(
        &self,
        email_content: impl Into<EmailSource>,
        options: &Options,
    ) -> PResult<EmailData> {
        let content: EmailSource = email_content.into();
        let file_path: Option<PathBuf> = match &content {
            EmailSource::Path(p) => Some(p.clone()),
            EmailSource::Text(s) => {
                if py_exists(Path::new(s))? {
                    Some(PathBuf::from(s))
                } else {
                    None
                }
            }
            EmailSource::Bytes(_) => None,
        };
        let file_text = file_path.as_deref().map(py_path_str);
        let label = file_path
            .as_deref()
            .map(py_name)
            .unwrap_or_else(|| "content".to_string());
        let tracking_id = self.progress_tracker.start_tracking(
            file_text.as_deref(),
            Some("parse"),
            Some("EmailParser"),
            &format!("Email: {label}"),
            None,
        );
        let result = (|| -> PResult<EmailData> {
            let message = match (&file_path, &content) {
                (Some(path), _) => {
                    if !py_exists(path)? {
                        return Err(ValidationError::new(format!(
                            "Email file not found: {}",
                            py_path_str(path)
                        ))
                        .into());
                    }
                    let bytes = py_read_bytes(path)?;
                    mime::message_from_bytes(&bytes)?
                }
                (None, EmailSource::Bytes(bytes)) => mime::message_from_bytes(bytes)?,
                (None, EmailSource::Text(text)) => mime::message_from_string(text)?,
                (None, EmailSource::Path(_)) => unreachable!("paths always have a file path"),
            };
            let inner = (|| -> PResult<EmailData> {
                let headers = self.extract_headers(&message)?;
                let extract_attachments = options
                    .get("extract_attachments")
                    .map(truthy)
                    .unwrap_or(true);
                let body = self.extract_body(&message, Some(extract_attachments))?;
                let subject = headers
                    .subject
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("No subject");
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!("Parsed email: {subject}"),
                    None,
                );
                let mut metadata = Map::new();
                metadata.insert(
                    "message_id".to_string(),
                    headers
                        .message_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
                metadata.insert(
                    "date".to_string(),
                    headers
                        .date
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
                Ok(EmailData {
                    headers,
                    body,
                    metadata,
                })
            })();
            inner.map_err(|e| {
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    &e.to_string(),
                    None,
                );
                self.logger.error(format!("Failed to parse email: {e}"));
                SemanticaError::from(ProcessingError::new(format!("Failed to parse email: {e}")))
            })
        })();
        result.map_err(|e| {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
            e
        })
    }

    /// `extract_headers(email_message)`: decoded standard headers plus every other header
    /// in `custom_headers`.
    pub fn extract_headers(&self, email_message: &EmailMessage) -> PResult<EmailHeaders> {
        let mut headers = EmailHeaders {
            subject: decode_header_value(email_message.get("Subject").as_ref())?,
            from_address: decode_header_value(email_message.get("From").as_ref())?,
            ..EmailHeaders::default()
        };
        if let Some(to) = email_message.get("To").filter(HeaderValue::truthy) {
            headers.to_addresses = split_addresses(&to);
        }
        if let Some(cc) = email_message.get("Cc").filter(HeaderValue::truthy) {
            headers.cc_addresses = split_addresses(&cc);
        }
        if let Some(bcc) = email_message.get("Bcc").filter(HeaderValue::truthy) {
            headers.bcc_addresses = split_addresses(&bcc);
        }
        if let Some(date_value) = email_message.get("Date").filter(HeaderValue::truthy) {
            headers.date = Some(match &date_value {
                HeaderValue::Str(s) => match parsedate_to_datetime(s) {
                    Ok(dt) => dt.isoformat(),
                    Err(_) => s.clone(),
                },
                // `parsedate_to_datetime(Header)` fails (`Header` has no `split`).
                HeaderValue::Header(h) => h.to_string(),
            });
        }
        headers.message_id = email_message.get("Message-ID").map(|v| v.to_string());
        headers.in_reply_to = email_message.get("In-Reply-To").map(|v| v.to_string());
        if let Some(references) = email_message.get("References").filter(HeaderValue::truthy) {
            headers.references = py_split_whitespace(&references.to_string())
                .into_iter()
                .map(|r| py_strip(r).to_string())
                .collect();
        }
        for (name, value) in email_message.items() {
            if !STANDARD_HEADERS.contains(&name.as_str()) {
                let decoded = decode_header_value(Some(&value))?.unwrap_or_default();
                headers.custom_headers.insert(name, decoded);
            }
        }
        Ok(headers)
    }

    /// `extract_body(email_message, extract_attachments=True)`: the `text/plain` and
    /// `text/html` bodies (decoded with their charset, `utf-8` by default, ignoring errors)
    /// and the attachments of a message.
    pub fn extract_body(
        &self,
        email_message: &EmailMessage,
        extract_attachments: Option<bool>,
    ) -> PResult<EmailBody> {
        let extract_attachments = extract_attachments.unwrap_or(true);
        let mut body = EmailBody::default();
        if email_message.is_multipart() {
            for part in email_message.walk() {
                let content_type = part.get_content_type();
                let disposition = part
                    .get("Content-Disposition")
                    .unwrap_or_else(|| HeaderValue::Str(String::new()));
                if disposition.contains("attachment")? && extract_attachments {
                    if let Some(attachment) = self.extract_attachment(part) {
                        body.attachments.push(attachment);
                    }
                } else if content_type == "text/plain" {
                    if let Some(payload) = part.get_payload_decoded().filter(|p| !p.is_empty()) {
                        let charset = part
                            .get_content_charset()?
                            .filter(|c| !c.is_empty())
                            .unwrap_or_else(|| "utf-8".to_string());
                        body.text = Some(mime::bytes_decode(
                            &payload,
                            &charset,
                            DecodeErrors::Ignore,
                        )?);
                    }
                } else if content_type == "text/html"
                    && let Some(payload) = part.get_payload_decoded().filter(|p| !p.is_empty())
                {
                    let charset = part
                        .get_content_charset()?
                        .filter(|c| !c.is_empty())
                        .unwrap_or_else(|| "utf-8".to_string());
                    body.html = Some(mime::bytes_decode(
                        &payload,
                        &charset,
                        DecodeErrors::Ignore,
                    )?);
                }
            }
        } else {
            let content_type = email_message.get_content_type();
            if let Some(payload) = email_message
                .get_payload_decoded()
                .filter(|p| !p.is_empty())
            {
                let charset = email_message
                    .get_content_charset()?
                    .filter(|c| !c.is_empty())
                    .unwrap_or_else(|| "utf-8".to_string());
                let decoded = mime::bytes_decode(&payload, &charset, DecodeErrors::Ignore)?;
                if content_type == "text/html" {
                    body.html = Some(decoded);
                } else {
                    body.text = Some(decoded);
                }
            }
        }
        Ok(body)
    }

    /// `_extract_attachment(part)`: the attachment record, or `None` when the part has no
    /// file name or extraction fails (logged as a warning).
    fn extract_attachment(&self, part: &EmailMessage) -> Option<EmailAttachment> {
        let result = (|| -> PResult<Option<EmailAttachment>> {
            let Some(filename) = part.get_filename()? else {
                return Ok(None);
            };
            if filename.is_empty() {
                return Ok(None);
            }
            let mut decoded_filename = String::new();
            for (piece, encoding) in decode_header_str(&filename)? {
                match piece {
                    DecodedPart::Bytes(bytes) => decoded_filename.push_str(&mime::bytes_decode(
                        &bytes,
                        encoding.as_deref().unwrap_or("utf-8"),
                        DecodeErrors::Ignore,
                    )?),
                    DecodedPart::Str(s) => decoded_filename.push_str(&s),
                }
            }
            let payload = part.get_payload_decoded();
            Ok(Some(EmailAttachment {
                filename: decoded_filename,
                content_type: part.get_content_type(),
                size: payload.as_ref().map(Vec::len).unwrap_or(0),
                data: payload,
            }))
        })();
        match result {
            Ok(attachment) => attachment,
            Err(e) => {
                self.logger
                    .warning(format!("Failed to extract attachment: {e}"));
                None
            }
        }
    }

    /// `analyze_thread(emails)`: delegate to [`EmailThreadAnalyzer::analyze_thread`].
    pub fn analyze_thread(&self, emails: &[EmailData]) -> Map<String, Value> {
        self.thread_analyzer.analyze_thread(emails)
    }
}

/// `MIMEParser`: MIME message parsing engine.
#[derive(Debug, Clone)]
pub struct MIMEParser {
    /// `self.logger` (`semantica.mime_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
}

impl Default for MIMEParser {
    fn default() -> Self {
        MIMEParser::new(Options::new())
    }
}

impl MIMEParser {
    /// `MIMEParser(**config)`.
    pub fn new(config: Options) -> Self {
        MIMEParser {
            logger: get_logger("mime_parser"),
            config,
        }
    }

    /// `parse_mime_message(message_content)`: `message_from_bytes` for bytes,
    /// `message_from_string` for text. A path is not accepted (Python raises `TypeError`
    /// building a `StringIO` from it).
    pub fn parse_mime_message(
        &self,
        message_content: impl Into<EmailSource>,
    ) -> PResult<EmailMessage> {
        match message_content.into() {
            EmailSource::Bytes(bytes) => mime::message_from_bytes(&bytes),
            EmailSource::Text(text) => mime::message_from_string(&text),
            EmailSource::Path(_) => Err(SemanticaError::Type(
                "initial_value must be str or None, not PosixPath".to_string(),
            )),
        }
    }
}

/// `EmailThreadAnalyzer`: e-mail thread analysis engine.
#[derive(Debug, Clone)]
pub struct EmailThreadAnalyzer {
    /// `self.logger` (`semantica.email_thread_analyzer`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
}

impl Default for EmailThreadAnalyzer {
    fn default() -> Self {
        EmailThreadAnalyzer::new(Options::new())
    }
}

impl EmailThreadAnalyzer {
    /// `EmailThreadAnalyzer(**config)`.
    pub fn new(config: Options) -> Self {
        EmailThreadAnalyzer {
            logger: get_logger("email_thread_analyzer"),
            config,
        }
    }

    /// `analyze_thread(emails)`: `{"thread_count", "participants", "messages_by_date",
    /// "reply_chains"}`. The original collects participants in a `set` (iteration order
    /// varies between Python runs); the port lists them in first-seen order.
    pub fn analyze_thread(&self, emails: &[EmailData]) -> Map<String, Value> {
        let mut participants: Vec<String> = Vec::new();
        let mut add = |p: &str| {
            if !participants.iter().any(|x| x == p) {
                participants.push(p.to_string());
            }
        };
        for email in emails {
            if let Some(from) = email
                .headers
                .from_address
                .as_deref()
                .filter(|f| !f.is_empty())
            {
                add(from);
            }
            for to in &email.headers.to_addresses {
                add(to);
            }
            for cc in &email.headers.cc_addresses {
                add(cc);
            }
        }
        let mut messages_by_date: Map<String, Value> = Map::new();
        for email in emails {
            if let Some(date) = email.headers.date.as_deref().filter(|d| !d.is_empty()) {
                let date_key: String = date.chars().take(10).collect();
                let entry = messages_by_date
                    .entry(date_key)
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(items) = entry {
                    items.push(
                        email
                            .headers
                            .message_id
                            .clone()
                            .map(Value::String)
                            .unwrap_or(Value::Null),
                    );
                }
            }
        }
        let mut reply_chains = Vec::new();
        for email in emails {
            if let Some(in_reply_to) = email
                .headers
                .in_reply_to
                .as_deref()
                .filter(|r| !r.is_empty())
            {
                let mut chain = Map::new();
                chain.insert(
                    "message_id".to_string(),
                    email
                        .headers
                        .message_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
                chain.insert(
                    "in_reply_to".to_string(),
                    Value::String(in_reply_to.to_string()),
                );
                reply_chains.push(Value::Object(chain));
            }
        }
        let mut analysis = Map::new();
        analysis.insert("thread_count".to_string(), Value::from(emails.len()));
        analysis.insert(
            "participants".to_string(),
            Value::Array(participants.into_iter().map(Value::String).collect()),
        );
        analysis.insert(
            "messages_by_date".to_string(),
            Value::Object(messages_by_date),
        );
        analysis.insert("reply_chains".to_string(), Value::Array(reply_chains));
        analysis
    }
}
