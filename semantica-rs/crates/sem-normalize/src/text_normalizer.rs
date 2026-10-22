//! Text Normalization Module (`semantica/normalize/text_normalizer.py`).
//!
//! `TextNormalizer` coordinates Unicode normalization, whitespace handling, special
//! character processing and case normalization; `UnicodeNormalizer`,
//! `WhitespaceNormalizer` and `SpecialCharacterProcessor` are its engines.
//!
//! The coordinator holds its engines behind small traits ([`UnicodeNormalizing`],
//! [`WhitespaceNormalizing`], [`SpecialCharacterProcessing`] and
//! [`TextCleaning`](crate::text_cleaner::TextCleaning)): the original reaches them through
//! duck-typed attributes, and its tests swap them for mocks.

use std::sync::Arc;

use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::{py_str, truthy, type_name};
use sem_utils::{SemanticaError, get_logger, get_progress_tracker};
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

use crate::encoding_handler::TextOrBytes;
use crate::kwargs;
use crate::pystr::{self, PY_SPACE};
use crate::text_cleaner::{TextCleaner, TextCleaning};

static SPACES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r" +").expect("valid regex"));
static BLANK_LINES_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"\n[{PY_SPACE}]*\n")).expect("valid regex"));
static WHITESPACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"[{PY_SPACE}]+")).expect("valid regex"));

/// `unicodedata.normalize(form, text)`: `ValueError("invalid normalization form")` for an
/// unknown form.
pub(crate) fn unicodedata_normalize(form: &str, text: &str) -> Result<String, SemanticaError> {
    Ok(match form {
        "NFC" => text.nfc().collect(),
        "NFD" => text.nfd().collect(),
        "NFKC" => text.nfkc().collect(),
        "NFKD" => text.nfkd().collect(),
        _ => {
            return Err(SemanticaError::Value(
                "invalid normalization form".to_string(),
            ));
        }
    })
}

/// The smart punctuation replaced by `normalize_punctuation` / `process_special_chars`.
const PUNCTUATION: [(&str, &str); 7] = [
    ("\u{2018}", "'"),
    ("\u{2019}", "'"),
    ("\u{201C}", "\""),
    ("\u{201D}", "\""),
    ("\u{2013}", "-"),
    ("\u{2014}", "--"),
    ("\u{2026}", "..."),
];

fn replace_punctuation(text: &str) -> String {
    let mut text = text.to_string();
    for (old, new) in PUNCTUATION {
        text = text.replace(old, new);
    }
    text
}

/// The options of `TextNormalizer.normalize_text` (keyword arguments with their defaults).
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizeTextOptions {
    /// `unicode_form` (default `"NFC"`).
    pub unicode_form: String,
    /// `case`: `"preserve"` (default), `"lower"`, `"upper"` or `"title"`.
    pub case: String,
    /// `normalize_diacritics` (default `False`).
    pub normalize_diacritics: bool,
    /// `line_break_type` (default `"unix"`).
    pub line_break_type: String,
    /// Further keyword arguments (`**options`), passed on to the whitespace normalizer and the
    /// special character processor (which reads `remove_diacritics`).
    pub extra: Map<String, Value>,
}

impl Default for NormalizeTextOptions {
    fn default() -> Self {
        NormalizeTextOptions {
            unicode_form: "NFC".to_string(),
            case: "preserve".to_string(),
            normalize_diacritics: false,
            line_break_type: "unix".to_string(),
            extra: Map::new(),
        }
    }
}

impl NormalizeTextOptions {
    /// Bind keyword arguments the way `normalize_text(text, **kwargs)` does.
    pub fn from_kwargs(kwargs: &Map<String, Value>) -> NormalizeTextOptions {
        let mut options = NormalizeTextOptions::default();
        for (key, value) in kwargs {
            match key.as_str() {
                "unicode_form" => options.unicode_form = kwargs::str_arg(value),
                "case" => {
                    // `case == "lower"` etc. never holds for a non-string.
                    options.case = value.as_str().unwrap_or("").to_string();
                }
                "normalize_diacritics" => options.normalize_diacritics = kwargs::truthy(value),
                "line_break_type" => {
                    options.line_break_type = value.as_str().unwrap_or("").to_string();
                }
                _ => {
                    options.extra.insert(key.clone(), value.clone());
                }
            }
        }
        options
    }

    /// Options with only `case` set.
    pub fn with_case(case: &str) -> NormalizeTextOptions {
        NormalizeTextOptions {
            case: case.to_string(),
            ..NormalizeTextOptions::default()
        }
    }
}

/// The Unicode engine interface `TextNormalizer` delegates to.
pub trait UnicodeNormalizing: Send + Sync {
    /// `normalize_unicode(text, form=form)`.
    fn normalize_unicode(&self, text: Option<&str>, form: &str) -> Result<String, SemanticaError>;
}

/// The whitespace engine interface `TextNormalizer` delegates to.
pub trait WhitespaceNormalizing: Send + Sync {
    /// `normalize_whitespace(text, line_break_type=line_break_type, **options)`.
    fn normalize_whitespace(
        &self,
        text: Option<&str>,
        line_break_type: &str,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError>;
}

/// The special character engine interface `TextNormalizer` delegates to.
pub trait SpecialCharacterProcessing: Send + Sync {
    /// `process_special_chars(text, normalize_diacritics=normalize_diacritics, **options)`.
    fn process_special_chars(
        &self,
        text: &str,
        normalize_diacritics: bool,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError>;
}

/// Text normalization and cleaning coordinator (`TextNormalizer(config=None, **kwargs)`).
pub struct TextNormalizer {
    /// `config` (with the keyword arguments merged in).
    pub config: Map<String, Value>,
    /// `text_cleaner` (a [`TextCleaner`] unless replaced).
    pub text_cleaner: Box<dyn TextCleaning>,
    /// `unicode_normalizer` (a [`UnicodeNormalizer`] unless replaced).
    pub unicode_normalizer: Box<dyn UnicodeNormalizing>,
    /// `whitespace_normalizer` (a [`WhitespaceNormalizer`] unless replaced).
    pub whitespace_normalizer: Box<dyn WhitespaceNormalizing>,
    /// `special_char_processor` (a [`SpecialCharacterProcessor`] unless replaced).
    pub special_char_processor: Box<dyn SpecialCharacterProcessing>,
    /// `progress_tracker` (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for TextNormalizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextNormalizer")
            .field("config", &self.config)
            .finish()
    }
}

impl Default for TextNormalizer {
    fn default() -> Self {
        TextNormalizer::new(Map::new())
    }
}

impl TextNormalizer {
    /// `TextNormalizer(config)`: the engines are built from the same config.
    pub fn new(config: Map<String, Value>) -> TextNormalizer {
        let logger = get_logger("text_normalizer");
        let text_cleaner = Box::new(TextCleaner::new(config.clone()));
        let unicode_normalizer = Box::new(UnicodeNormalizer::new(config.clone()));
        let whitespace_normalizer = Box::new(WhitespaceNormalizer::new(config.clone()));
        let special_char_processor = Box::new(SpecialCharacterProcessor::new(config.clone()));
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug("Text normalizer initialized");
        TextNormalizer {
            config,
            text_cleaner,
            unicode_normalizer,
            whitespace_normalizer,
            special_char_processor,
            progress_tracker,
            logger,
        }
    }

    /// `normalize(source, **options)`: a list normalizes each document (dicts with a
    /// `content` key get a normalized copy; anything else is normalized as `str(doc)`, or kept
    /// when that fails); any other value is normalized as `str(source)`.
    pub fn normalize(
        &self,
        source: &Value,
        options: &NormalizeTextOptions,
    ) -> Result<Value, SemanticaError> {
        let Value::Array(docs) = source else {
            return Ok(Value::String(
                self.normalize_text(py_str(source).as_str(), options)?,
            ));
        };
        let mut normalized_docs = Vec::with_capacity(docs.len());
        for doc in docs {
            match doc {
                Value::Object(map) if map.contains_key("content") => {
                    let mut new_doc = map.clone();
                    let content = self.normalize_text_value(&map["content"], options)?;
                    new_doc.insert("content".to_string(), Value::String(content));
                    normalized_docs.push(Value::Object(new_doc));
                }
                other => match self.normalize_text(py_str(other).as_str(), options) {
                    Ok(text) => normalized_docs.push(Value::String(text)),
                    Err(_) => normalized_docs.push(other.clone()),
                },
            }
        }
        Ok(Value::Array(normalized_docs))
    }

    /// `normalize_text` on an arbitrary document `content` value.
    fn normalize_text_value(
        &self,
        content: &Value,
        options: &NormalizeTextOptions,
    ) -> Result<String, SemanticaError> {
        match content {
            Value::String(s) => self.normalize_text(s.as_str(), options),
            v if !truthy(v) => self.normalize_text(None, options),
            other => {
                // Python: the Unicode step returns the non-string unchanged, then the
                // whitespace step calls `.replace` on it.
                let tracking_id = self.progress_tracker.start_tracking(
                    None,
                    None,
                    None,
                    "Semantica: Normalizing text",
                    None,
                );
                let error = SemanticaError::Attribute(format!(
                    "'{}' object has no attribute 'replace'",
                    type_name(other)
                ));
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    &error.to_string(),
                    None,
                );
                Err(error)
            }
        }
    }

    /// `normalize_text(text, unicode_form="NFC", case="preserve", normalize_diacritics=False,
    /// line_break_type="unix", **options)`: Unicode, whitespace, punctuation (and
    /// diacritics), then case; the result is stripped. Engine errors mark the progress item
    /// failed and propagate.
    pub fn normalize_text<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        options: &NormalizeTextOptions,
    ) -> Result<String, SemanticaError> {
        let text = text.into();
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            None,
            None,
            "Semantica: Normalizing text",
            None,
        );
        match self.normalize_text_inner(text, options) {
            Ok(result) => {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("completed"), "", None);
                Ok(result)
            }
            Err(e) => {
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    &e.to_string(),
                    None,
                );
                Err(e)
            }
        }
    }

    fn normalize_text_inner(
        &self,
        text: Option<&str>,
        options: &NormalizeTextOptions,
    ) -> Result<String, SemanticaError> {
        let Some(text) = text.filter(|t| !t.is_empty()) else {
            return Ok(String::new());
        };
        let normalized = self
            .unicode_normalizer
            .normalize_unicode(Some(text), &options.unicode_form)?;
        let normalized = self.whitespace_normalizer.normalize_whitespace(
            Some(&normalized),
            &options.line_break_type,
            &options.extra,
        )?;
        let mut normalized = self.special_char_processor.process_special_chars(
            &normalized,
            options.normalize_diacritics,
            &options.extra,
        )?;
        match options.case.as_str() {
            "lower" => normalized = pystr::lower(&normalized),
            "upper" => normalized = pystr::upper(&normalized),
            "title" => normalized = pystr::title(&normalized),
            _ => {}
        }
        Ok(pystr::strip(&normalized).to_string())
    }

    /// `clean_text(text, **options)`: delegates to the text cleaner.
    pub fn clean_text<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError> {
        self.text_cleaner.clean(text.into(), options)
    }

    /// `standardize_format(text, format_type="standard")`: `"compact"` collapses whitespace
    /// runs; the result is stripped.
    pub fn standardize_format(&self, text: &str, format_type: Option<&str>) -> String {
        if format_type == Some("compact") {
            let compact = WHITESPACE_RE.replace_all(text, " ");
            return pystr::strip(&compact).to_string();
        }
        pystr::strip(text).to_string()
    }

    /// `process_batch(texts, **options)`.
    pub fn process_batch<S: AsRef<str>>(
        &self,
        texts: &[S],
        options: &NormalizeTextOptions,
    ) -> Result<Vec<String>, SemanticaError> {
        texts
            .iter()
            .map(|text| self.normalize_text(text.as_ref(), options))
            .collect()
    }

    /// The logger (`semantica.text_normalizer`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// Unicode normalization engine (`UnicodeNormalizer(**config)`).
#[derive(Debug, Clone)]
pub struct UnicodeNormalizer {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for UnicodeNormalizer {
    fn default() -> Self {
        UnicodeNormalizer::new(Map::new())
    }
}

impl UnicodeNormalizer {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `UnicodeNormalizer(**config)`.
    pub fn new(config: Map<String, Value>) -> UnicodeNormalizer {
        let logger = get_logger("unicode_normalizer");
        logger.debug("Unicode normalizer initialized");
        UnicodeNormalizer { config, logger }
    }

    /// `normalize_unicode(text, form="NFC")`: an invalid form logs a warning and returns the
    /// text unchanged; empty or `None` input gives `""`.
    pub fn normalize_unicode<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        form: Option<&str>,
    ) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        match unicodedata_normalize(form.unwrap_or("NFC"), text) {
            Ok(normalized) => normalized,
            Err(e) => {
                self.logger
                    .warning(format!("Unicode normalization failed: {e}"));
                text.to_string()
            }
        }
    }

    /// `handle_encoding(text, source_encoding, target_encoding="utf-8")`: bytes are decoded
    /// with `source_encoding`, round-tripped through `target_encoding`, or decoded as UTF-8
    /// with replacement characters when that fails; strings are returned unchanged.
    pub fn handle_encoding(
        &self,
        text: impl Into<TextOrBytes>,
        source_encoding: &str,
        target_encoding: Option<&str>,
    ) -> String {
        match text.into() {
            TextOrBytes::Text(s) => s,
            TextOrBytes::Bytes(bytes) => {
                let target = target_encoding.unwrap_or("utf-8");
                let attempt = crate::codecs::decode(&bytes, source_encoding, "strict")
                    .and_then(|s| crate::codecs::encode(&s, target, "strict"))
                    .and_then(|b| crate::codecs::decode(&b, target, "strict"));
                match attempt {
                    Ok(s) => s,
                    Err(_) => crate::codecs::decode(&bytes, "utf-8", "replace")
                        .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned()),
                }
            }
        }
    }

    /// `process_special_chars(text)`: smart quotes, dashes and the ellipsis to ASCII.
    pub fn process_special_chars(&self, text: &str) -> String {
        replace_punctuation(text)
    }
}

impl UnicodeNormalizing for UnicodeNormalizer {
    fn normalize_unicode(&self, text: Option<&str>, form: &str) -> Result<String, SemanticaError> {
        Ok(UnicodeNormalizer::normalize_unicode(self, text, Some(form)))
    }
}

/// Whitespace normalization engine (`WhitespaceNormalizer(**config)`).
#[derive(Debug, Clone)]
pub struct WhitespaceNormalizer {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for WhitespaceNormalizer {
    fn default() -> Self {
        WhitespaceNormalizer::new(Map::new())
    }
}

impl WhitespaceNormalizer {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `WhitespaceNormalizer(**config)`.
    pub fn new(config: Map<String, Value>) -> WhitespaceNormalizer {
        let logger = get_logger("whitespace_normalizer");
        logger.debug("Whitespace normalizer initialized");
        WhitespaceNormalizer { config, logger }
    }

    /// `normalize_whitespace(text, line_break_type="unix", **options)`: tabs to spaces,
    /// line breaks normalized, runs of spaces collapsed, blank-line runs reduced to one blank
    /// line, stripped.
    pub fn normalize_whitespace<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        line_break_type: Option<&str>,
    ) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let text = text.replace('\t', " ");
        let text = self.handle_line_breaks(&text, line_break_type);
        let text = SPACES_RE.replace_all(&text, " ");
        let text = BLANK_LINES_RE.replace_all(&text, "\n\n");
        pystr::strip(&text).to_string()
    }

    /// `handle_line_breaks(text, line_break_type="unix")`: `"unix"` turns `\r\n` and `\r` into
    /// `\n`; `"windows"` applies the original's replacement chain (`\r` -> `\r\n`, then
    /// `\n` -> `\r\n`); other types leave the text unchanged.
    pub fn handle_line_breaks(&self, text: &str, line_break_type: Option<&str>) -> String {
        match line_break_type.unwrap_or("unix") {
            "unix" => text.replace("\r\n", "\n").replace('\r', "\n"),
            "windows" => {
                // The original first replaces "\r\n" with itself (a no-op), then runs two
                // passes: the second also expands the "\n" the first one inserted, so a
                // lone "\r" ends up as "\r\r\n".
                let text = text.replace('\r', "\r\n");
                text.replace('\n', "\r\n")
            }
            _ => text.to_string(),
        }
    }

    /// `process_indentation(text, indent_type="spaces")`: tabs to four spaces, or four spaces
    /// to tabs.
    pub fn process_indentation(&self, text: &str, indent_type: Option<&str>) -> String {
        match indent_type.unwrap_or("spaces") {
            "spaces" => text.replace('\t', "    "),
            "tabs" => text.replace("    ", "\t"),
            _ => text.to_string(),
        }
    }
}

impl WhitespaceNormalizing for WhitespaceNormalizer {
    fn normalize_whitespace(
        &self,
        text: Option<&str>,
        line_break_type: &str,
        _options: &Map<String, Value>,
    ) -> Result<String, SemanticaError> {
        Ok(WhitespaceNormalizer::normalize_whitespace(
            self,
            text,
            Some(line_break_type),
        ))
    }
}

/// Special character processing engine (`SpecialCharacterProcessor(**config)`).
#[derive(Debug, Clone)]
pub struct SpecialCharacterProcessor {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for SpecialCharacterProcessor {
    fn default() -> Self {
        SpecialCharacterProcessor::new(Map::new())
    }
}

impl SpecialCharacterProcessor {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `SpecialCharacterProcessor(**config)`.
    pub fn new(config: Map<String, Value>) -> SpecialCharacterProcessor {
        let logger = get_logger("special_char_processor");
        logger.debug("Special character processor initialized");
        SpecialCharacterProcessor { config, logger }
    }

    /// `process_special_chars(text, normalize_diacritics=False, **options)`: punctuation,
    /// then diacritics when requested (`options["remove_diacritics"]` chooses removal).
    pub fn process_special_chars(
        &self,
        text: &str,
        normalize_diacritics: bool,
        options: &Map<String, Value>,
    ) -> String {
        let text = self.normalize_punctuation(text);
        if normalize_diacritics {
            let remove = options
                .get("remove_diacritics")
                .map(truthy)
                .unwrap_or(false);
            return self.process_diacritics(&text, remove);
        }
        text
    }

    /// `normalize_punctuation(text)`: smart quotes, dashes and the ellipsis to ASCII.
    pub fn normalize_punctuation(&self, text: &str) -> String {
        replace_punctuation(text)
    }

    /// `process_diacritics(text, remove_diacritics=False, **options)`: remove nonspacing
    /// marks after NFD decomposition, or compose to NFC.
    pub fn process_diacritics(&self, text: &str, remove_diacritics: bool) -> String {
        if remove_diacritics {
            let nfd: String = text.nfd().collect();
            pystr::remove_nonspacing_marks(&nfd)
        } else {
            text.nfc().collect()
        }
    }
}

impl SpecialCharacterProcessing for SpecialCharacterProcessor {
    fn process_special_chars(
        &self,
        text: &str,
        normalize_diacritics: bool,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError> {
        Ok(SpecialCharacterProcessor::process_special_chars(
            self,
            text,
            normalize_diacritics,
            options,
        ))
    }
}
