//! Text Cleaning Module (`semantica/normalize/text_cleaner.py`).
//!
//! HTML tag removal (regex, or `BeautifulSoup`-style text extraction when
//! `preserve_structure` is set), whitespace and Unicode normalization, special character
//! removal, security sanitization and batch cleaning.

use std::sync::Arc;

use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::{SemanticaError, get_logger, get_progress_tracker};
use serde_json::{Map, Value};

use crate::kwargs;
use crate::pystr::{self, PY_SPACE, PY_WORD};

/// `BEAUTIFULSOUP_AVAILABLE`: HTML text extraction is always available (html5ever).
pub const BEAUTIFULSOUP_AVAILABLE: bool = true;

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").expect("valid regex"));
static ENTITY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"&#?[{PY_WORD}]+;")).expect("valid regex"));
static TAB_NEWLINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\t\n\r]+").expect("valid regex"));
static SPACES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r" +").expect("valid regex"));
static SPECIAL_KEEP_SPACE_NEWLINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"[^a-zA-Z0-9{PY_SPACE}]")).expect("valid regex"));
static SPECIAL_KEEP_SPACE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^a-zA-Z0-9 ]").expect("valid regex"));
static SPECIAL_KEEP_NONE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^a-zA-Z0-9]").expect("valid regex"));
static SCRIPT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?is)<script[^>]*>.*?</script(?:[{PY_SPACE}][^>]*)?>"
    ))
    .expect("valid regex")
});
static IFRAME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?is)<iframe[^>]*>.*?</iframe(?:[{PY_SPACE}][^>]*)?>"
    ))
    .expect("valid regex")
});
static JAVASCRIPT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)javascript:").expect("valid regex"));
static DATA_URL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)data:[^;]*;base64,").expect("valid regex"));

/// The options of `TextCleaner.clean` (keyword arguments with their defaults).
#[derive(Debug, Clone, PartialEq)]
pub struct CleanOptions {
    /// `remove_html` (default `True`).
    pub remove_html: bool,
    /// `normalize_whitespace` (default `True`).
    pub normalize_whitespace: bool,
    /// `normalize_unicode` (default `True`).
    pub normalize_unicode: bool,
    /// `remove_special_chars` (default `False`).
    pub remove_special_chars: bool,
    /// `unicode_form` (default `"NFC"`).
    pub unicode_form: String,
    /// `allow_spaces` (default `True`).
    pub allow_spaces: bool,
    /// Further keyword arguments (`**options`, unused).
    pub extra: Map<String, Value>,
}

impl Default for CleanOptions {
    fn default() -> Self {
        CleanOptions {
            remove_html: true,
            normalize_whitespace: true,
            normalize_unicode: true,
            remove_special_chars: false,
            unicode_form: "NFC".to_string(),
            allow_spaces: true,
            extra: Map::new(),
        }
    }
}

impl CleanOptions {
    /// Bind keyword arguments the way `clean(text, **kwargs)` does: known names set the
    /// matching option (Python truthiness for flags), the rest land in `extra`.
    pub fn from_kwargs(kwargs: &Map<String, Value>) -> CleanOptions {
        let mut options = CleanOptions::default();
        for (key, value) in kwargs {
            match key.as_str() {
                "remove_html" => options.remove_html = kwargs::truthy(value),
                "normalize_whitespace" => options.normalize_whitespace = kwargs::truthy(value),
                "normalize_unicode" => options.normalize_unicode = kwargs::truthy(value),
                "remove_special_chars" => options.remove_special_chars = kwargs::truthy(value),
                "unicode_form" => options.unicode_form = kwargs::str_arg(value),
                "allow_spaces" => options.allow_spaces = kwargs::truthy(value),
                _ => {
                    options.extra.insert(key.clone(), value.clone());
                }
            }
        }
        options
    }
}

/// The text-cleaning interface `TextNormalizer` delegates to (the original duck-types its
/// `text_cleaner` attribute, and its tests replace it with a mock).
pub trait TextCleaning: Send + Sync {
    /// `clean(text, **options)`.
    fn clean(
        &self,
        text: Option<&str>,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError>;
}

/// Text cleaning coordinator (`TextCleaner(**config)`).
pub struct TextCleaner {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    /// The progress tracker (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for TextCleaner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextCleaner")
            .field("config", &self.config)
            .finish()
    }
}

impl Default for TextCleaner {
    fn default() -> Self {
        TextCleaner::new(Map::new())
    }
}

impl TextCleaner {
    /// `TextCleaner(**config)`.
    pub fn new(config: Map<String, Value>) -> TextCleaner {
        let logger = get_logger("text_cleaner");
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug("Text cleaner initialized");
        TextCleaner {
            config,
            progress_tracker,
            logger,
        }
    }

    /// `clean(text, remove_html=True, normalize_whitespace=True, normalize_unicode=True,
    /// remove_special_chars=False, unicode_form="NFC", allow_spaces=True, **options)`.
    pub fn clean<'a>(&self, text: impl Into<Option<&'a str>>, options: &CleanOptions) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let mut cleaned = text.to_string();
        if options.remove_html {
            cleaned = self.remove_html(cleaned.as_str(), false);
        }
        if options.normalize_unicode {
            cleaned = self.normalize_unicode(cleaned.as_str(), Some(&options.unicode_form));
        }
        if options.normalize_whitespace {
            cleaned = self.normalize_whitespace(cleaned.as_str());
        }
        if options.remove_special_chars {
            cleaned = self.remove_special_chars(cleaned.as_str(), options.allow_spaces, false);
        }
        pystr::strip(&cleaned).to_string()
    }

    /// `remove_html(text, preserve_structure=False)`: regex tag removal plus decoding of six
    /// common entities (other entities are dropped); with `preserve_structure` the text nodes
    /// of the parsed document, stripped and joined with newlines.
    pub fn remove_html<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        preserve_structure: bool,
    ) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        if preserve_structure && BEAUTIFULSOUP_AVAILABLE {
            return crate::htmltext::get_text_stripped(text, "\n");
        }
        let text = TAG_RE.replace_all(text, "");
        let text = text
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'");
        ENTITY_RE.replace_all(&text, "").into_owned()
    }

    /// `normalize_whitespace(text)`: tabs/newlines to spaces, runs of spaces collapsed,
    /// stripped.
    pub fn normalize_whitespace<'a>(&self, text: impl Into<Option<&'a str>>) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let text = TAB_NEWLINE_RE.replace_all(text, " ");
        let text = SPACES_RE.replace_all(&text, " ");
        pystr::strip(&text).to_string()
    }

    /// `normalize_unicode(text, form="NFC")`; an invalid form logs a warning and returns the
    /// text unchanged.
    pub fn normalize_unicode<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        form: Option<&str>,
    ) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        match crate::text_normalizer::unicodedata_normalize(form.unwrap_or("NFC"), text) {
            Ok(normalized) => normalized,
            Err(e) => {
                self.logger
                    .warning(format!("Failed to normalize unicode: {e}"));
                text.to_string()
            }
        }
    }

    /// `remove_special_chars(text, allow_spaces=True, allow_newlines=False)`: keep ASCII
    /// letters and digits (and spaces / any whitespace).
    pub fn remove_special_chars<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        allow_spaces: bool,
        allow_newlines: bool,
    ) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let pattern: &Regex = if allow_spaces && allow_newlines {
            &SPECIAL_KEEP_SPACE_NEWLINE
        } else if allow_spaces {
            &SPECIAL_KEEP_SPACE
        } else {
            &SPECIAL_KEEP_NONE
        };
        pattern.replace_all(text, "").into_owned()
    }

    /// `sanitize(text, remove_data_urls=False, **options)`: drop `<script>`/`<iframe>`
    /// elements and `javascript:` (and optionally `data:...;base64,`) prefixes.
    pub fn sanitize<'a>(&self, text: impl Into<Option<&'a str>>, remove_data_urls: bool) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let text = SCRIPT_RE.replace_all(text, "");
        let text = IFRAME_RE.replace_all(&text, "");
        let text = JAVASCRIPT_RE.replace_all(&text, "");
        if remove_data_urls {
            DATA_URL_RE.replace_all(&text, "").into_owned()
        } else {
            text.into_owned()
        }
    }

    /// `trim(text, remove_empty_lines=False, **options)`.
    pub fn trim<'a>(&self, text: impl Into<Option<&'a str>>, remove_empty_lines: bool) -> String {
        let Some(text) = text.into().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let text = pystr::strip(text);
        if remove_empty_lines {
            text.split('\n')
                .filter(|line| !pystr::strip(line).is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            text.to_string()
        }
    }

    /// `clean_batch(texts, **options)`.
    pub fn clean_batch<S: AsRef<str>>(&self, texts: &[S], options: &CleanOptions) -> Vec<String> {
        texts
            .iter()
            .map(|text| self.clean(text.as_ref(), options))
            .collect()
    }
}

impl TextCleaning for TextCleaner {
    fn clean(
        &self,
        text: Option<&str>,
        options: &Map<String, Value>,
    ) -> Result<String, SemanticaError> {
        Ok(TextCleaner::clean(
            self,
            text,
            &CleanOptions::from_kwargs(options),
        ))
    }
}
