//! Language Detection Module (`semantica/normalize/language_detector.py`).
//!
//! The original wraps `langdetect` (`detect`, `detect_langs`). The port puts the detection
//! library behind [`LanguageBackend`]; the default backend, [`WhatlangBackend`], uses
//! `whatlang` and reports langdetect's ISO 639-1 codes. A detector without a backend behaves
//! like the original when langdetect is not installed (always the default language).

use std::sync::Arc;

use sem_utils::get_logger;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use serde_json::{Map, Value};

use crate::kwargs;
use crate::pystr;

/// `LANGDETECT_AVAILABLE`: a detection backend is available (whatlang).
pub const LANGDETECT_AVAILABLE: bool = true;

/// `langdetect.lang_detect_exception.LangDetectException`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct LangDetectException(pub String);

/// One `langdetect.language.Language` (`lang`, `prob`).
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedLanguage {
    /// Language code (`en`, `fr`, `zh-cn`, ...).
    pub lang: String,
    /// Probability.
    pub prob: f64,
}

/// The language detection library (`langdetect.detect` / `langdetect.detect_langs`).
pub trait LanguageBackend: Send + Sync {
    /// `detect(text)`: the most likely language code.
    fn detect(&self, text: &str) -> Result<String, LangDetectException>;
    /// `detect_langs(text)`: candidate languages, most likely first.
    fn detect_langs(&self, text: &str) -> Result<Vec<DetectedLanguage>, LangDetectException>;
}

/// The default backend: `whatlang`, with langdetect's language codes.
#[derive(Debug, Clone, Copy, Default)]
pub struct WhatlangBackend;

/// langdetect's ISO 639-1 code for a `whatlang` language.
fn langdetect_code(lang: whatlang::Lang) -> String {
    let code = lang.code();
    let mapped = match code {
        "afr" => "af",
        "aka" => "ak",
        "amh" => "am",
        "ara" => "ar",
        "aze" => "az",
        "bel" => "be",
        "ben" => "bn",
        "bul" => "bg",
        "cat" => "ca",
        "ces" => "cs",
        "cmn" => "zh-cn",
        "dan" => "da",
        "deu" => "de",
        "ell" => "el",
        "eng" => "en",
        "epo" => "eo",
        "est" => "et",
        "fin" => "fi",
        "fra" => "fr",
        "guj" => "gu",
        "heb" => "he",
        "hin" => "hi",
        "hrv" => "hr",
        "hun" => "hu",
        "hye" => "hy",
        "ind" => "id",
        "ita" => "it",
        "jav" => "jv",
        "jpn" => "ja",
        "kan" => "kn",
        "kat" => "ka",
        "khm" => "km",
        "kor" => "ko",
        "lat" => "la",
        "lav" => "lv",
        "lit" => "lt",
        "mal" => "ml",
        "mar" => "mr",
        "mkd" => "mk",
        "mya" => "my",
        "nep" => "ne",
        "nld" => "nl",
        "nob" => "no",
        "ori" => "or",
        "pan" => "pa",
        "pes" => "fa",
        "pol" => "pl",
        "por" => "pt",
        "ron" => "ro",
        "rus" => "ru",
        "sin" => "si",
        "slk" => "sk",
        "slv" => "sl",
        "sna" => "sn",
        "spa" => "es",
        "srp" => "sr",
        "swe" => "sv",
        "tam" => "ta",
        "tel" => "te",
        "tgl" => "tl",
        "tha" => "th",
        "tuk" => "tk",
        "tur" => "tr",
        "ukr" => "uk",
        "urd" => "ur",
        "uzb" => "uz",
        "vie" => "vi",
        "yid" => "yi",
        "zul" => "zu",
        other => other,
    };
    mapped.to_string()
}

impl LanguageBackend for WhatlangBackend {
    fn detect(&self, text: &str) -> Result<String, LangDetectException> {
        whatlang::detect(text)
            .map(|info| langdetect_code(info.lang()))
            .ok_or_else(|| LangDetectException("No features in text.".to_string()))
    }

    fn detect_langs(&self, text: &str) -> Result<Vec<DetectedLanguage>, LangDetectException> {
        let info = whatlang::detect(text)
            .ok_or_else(|| LangDetectException("No features in text.".to_string()))?;
        Ok(vec![DetectedLanguage {
            lang: langdetect_code(info.lang()),
            prob: info.confidence(),
        }])
    }
}

/// Language detection coordinator (`LanguageDetector(**config)`).
pub struct LanguageDetector {
    /// `config`.
    pub config: Map<String, Value>,
    /// `default_language` (default `"en"`).
    pub default_language: String,
    /// `min_confidence` (default 0.5).
    pub min_confidence: f64,
    /// The detection library (`None`: langdetect not installed).
    pub backend: Option<Arc<dyn LanguageBackend>>,
    /// `progress_tracker` (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for LanguageDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageDetector")
            .field("default_language", &self.default_language)
            .field("min_confidence", &self.min_confidence)
            .field("backend", &self.backend.is_some())
            .finish()
    }
}

impl Default for LanguageDetector {
    fn default() -> Self {
        LanguageDetector::new(Map::new())
    }
}

impl LanguageDetector {
    /// `LanguageDetector(**config)` (`default_language`, `min_confidence`) with the whatlang
    /// backend.
    pub fn new(config: Map<String, Value>) -> LanguageDetector {
        LanguageDetector::with_backend(config, Some(Arc::new(WhatlangBackend)))
    }

    /// A detector with the given backend; `None` reproduces the original without langdetect
    /// (a warning is logged and every detection returns the default language).
    pub fn with_backend(
        config: Map<String, Value>,
        backend: Option<Arc<dyn LanguageBackend>>,
    ) -> LanguageDetector {
        let logger = get_logger("language_detector");
        let default_language = kwargs::get_str(&config, "default_language", "en");
        let min_confidence = kwargs::get_f64(&config, "min_confidence", 0.5);
        if backend.is_none() {
            logger.warning("langdetect library not available, language detection will be limited");
        }
        let progress_tracker = sem_utils::get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug(format!(
            "Language detector initialized (default={default_language})"
        ));
        LanguageDetector {
            config,
            default_language,
            min_confidence,
            backend,
            progress_tracker,
            logger,
        }
    }

    fn too_short(text: Option<&str>) -> bool {
        match text {
            None => true,
            Some(t) => t.is_empty() || pystr::char_len(pystr::strip(t)) < 10,
        }
    }

    /// `detect(text, **options)`: the language code, or the default language for texts
    /// shorter than 10 characters (stripped) or when detection fails.
    pub fn detect<'a>(&self, text: impl Into<Option<&'a str>>) -> String {
        let text = text.into();
        if Self::too_short(text) {
            return self.default_language.clone();
        }
        let Some(backend) = &self.backend else {
            return self.default_language.clone();
        };
        match backend.detect(text.unwrap_or_default()) {
            Ok(language) => language,
            Err(_) => {
                self.logger.warning(format!(
                    "Failed to detect language, using default: {}",
                    self.default_language
                ));
                self.default_language.clone()
            }
        }
    }

    /// `detect_with_confidence(text, **options)`: `(language, probability)` of the top
    /// candidate; below `min_confidence` the default language is reported with that
    /// probability; `(default, 0.0)` when nothing can be detected.
    pub fn detect_with_confidence<'a>(&self, text: impl Into<Option<&'a str>>) -> (String, f64) {
        let text = text.into();
        if Self::too_short(text) {
            return (self.default_language.clone(), 0.0);
        }
        let Some(backend) = &self.backend else {
            return (self.default_language.clone(), 0.0);
        };
        match backend.detect_langs(text.unwrap_or_default()) {
            Ok(languages) => match languages.first() {
                Some(top) if top.prob >= self.min_confidence => (top.lang.clone(), top.prob),
                Some(top) => (self.default_language.clone(), top.prob),
                None => (self.default_language.clone(), 0.0),
            },
            Err(_) => {
                self.logger.warning(format!(
                    "Failed to detect language, using default: {}",
                    self.default_language
                ));
                (self.default_language.clone(), 0.0)
            }
        }
    }

    /// `detect_multiple(text, top_n=3, **options)`: up to `top_n` `(language, probability)`
    /// candidates.
    pub fn detect_multiple<'a>(
        &self,
        text: impl Into<Option<&'a str>>,
        top_n: Option<usize>,
    ) -> Vec<(String, f64)> {
        let text = text.into();
        let top_n = top_n.unwrap_or(3);
        if Self::too_short(text) {
            return vec![(self.default_language.clone(), 0.0)];
        }
        let Some(backend) = &self.backend else {
            return vec![(self.default_language.clone(), 0.0)];
        };
        match backend.detect_langs(text.unwrap_or_default()) {
            Ok(languages) if !languages.is_empty() => languages
                .into_iter()
                .take(top_n)
                .map(|l| (l.lang, l.prob))
                .collect(),
            Ok(_) => vec![(self.default_language.clone(), 0.0)],
            Err(_) => {
                self.logger.warning(format!(
                    "Failed to detect languages, using default: {}",
                    self.default_language
                ));
                vec![(self.default_language.clone(), 0.0)]
            }
        }
    }

    /// `detect_batch(texts, **options)`.
    pub fn detect_batch<S: AsRef<str>>(&self, texts: &[S]) -> Vec<String> {
        texts.iter().map(|t| self.detect(t.as_ref())).collect()
    }

    /// `detect_batch_with_confidence(texts, **options)`.
    pub fn detect_batch_with_confidence<S: AsRef<str>>(&self, texts: &[S]) -> Vec<(String, f64)> {
        texts
            .iter()
            .map(|t| self.detect_with_confidence(t.as_ref()))
            .collect()
    }

    /// `is_language(text, target_language, min_confidence=None, **options)`.
    pub fn is_language(
        &self,
        text: &str,
        target_language: &str,
        min_confidence: Option<f64>,
    ) -> bool {
        let (detected, confidence) = self.detect_with_confidence(text);
        let threshold = min_confidence.unwrap_or(self.min_confidence);
        detected == target_language && confidence >= threshold
    }

    /// `get_language_name(language_code)`: the English name, or the uppercased code.
    pub fn get_language_name(&self, language_code: &str) -> String {
        let name = match language_code {
            "en" => "English",
            "fr" => "French",
            "de" => "German",
            "es" => "Spanish",
            "it" => "Italian",
            "pt" => "Portuguese",
            "ru" => "Russian",
            "zh" => "Chinese",
            "ja" => "Japanese",
            "ko" => "Korean",
            "ar" => "Arabic",
            "hi" => "Hindi",
            "nl" => "Dutch",
            "sv" => "Swedish",
            "da" => "Danish",
            "no" => "Norwegian",
            "fi" => "Finnish",
            "pl" => "Polish",
            "tr" => "Turkish",
            "cs" => "Czech",
            "hu" => "Hungarian",
            "ro" => "Romanian",
            "th" => "Thai",
            "vi" => "Vietnamese",
            other => return pystr::upper(other),
        };
        name.to_string()
    }

    /// The logger (`semantica.language_detector`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}
