//! Encoding Handling Module (`semantica/normalize/encoding_handler.py`).
//!
//! Encoding detection (chardet-compatible, see [`crate::chardet`]), conversion to UTF-8 with
//! fallback encodings, BOM removal, file conversion and encoding validation.

use std::path::Path;
use std::sync::Arc;

use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::{
    ProcessingError, SemanticaError, ValidationError, get_logger, get_progress_tracker,
};
use serde_json::{Map, Value};

use crate::{codecs, kwargs};

/// Python `str | bytes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextOrBytes {
    /// `str`.
    Text(String),
    /// `bytes`.
    Bytes(Vec<u8>),
}

impl From<&str> for TextOrBytes {
    fn from(value: &str) -> Self {
        TextOrBytes::Text(value.to_string())
    }
}

impl From<String> for TextOrBytes {
    fn from(value: String) -> Self {
        TextOrBytes::Text(value)
    }
}

impl From<&[u8]> for TextOrBytes {
    fn from(value: &[u8]) -> Self {
        TextOrBytes::Bytes(value.to_vec())
    }
}

impl<const N: usize> From<&[u8; N]> for TextOrBytes {
    fn from(value: &[u8; N]) -> Self {
        TextOrBytes::Bytes(value.to_vec())
    }
}

impl From<Vec<u8>> for TextOrBytes {
    fn from(value: Vec<u8>) -> Self {
        TextOrBytes::Bytes(value)
    }
}

impl From<&Vec<u8>> for TextOrBytes {
    fn from(value: &Vec<u8>) -> Self {
        TextOrBytes::Bytes(value.clone())
    }
}

impl TextOrBytes {
    /// The bytes (`str` values as UTF-8).
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            TextOrBytes::Text(s) => s.as_bytes(),
            TextOrBytes::Bytes(b) => b,
        }
    }
}

/// The error Python raises for `bytes.decode(None)`.
fn none_encoding_error() -> SemanticaError {
    SemanticaError::Type("decode() argument 'encoding' must be str, not None".to_string())
}

/// Encoding detection and conversion coordinator (`EncodingHandler(**config)`).
pub struct EncodingHandler {
    /// `config`.
    pub config: Map<String, Value>,
    /// `default_encoding` (default `"utf-8"`).
    pub default_encoding: String,
    /// `fallback_encodings` (default `["latin-1", "cp1252", "iso-8859-1"]`).
    pub fallback_encodings: Vec<String>,
    /// `progress_tracker` (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for EncodingHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncodingHandler")
            .field("default_encoding", &self.default_encoding)
            .field("fallback_encodings", &self.fallback_encodings)
            .finish()
    }
}

impl Default for EncodingHandler {
    fn default() -> Self {
        EncodingHandler::new(Map::new())
    }
}

impl EncodingHandler {
    /// `EncodingHandler(**config)` (`default_encoding`, `fallback_encodings`).
    pub fn new(config: Map<String, Value>) -> EncodingHandler {
        let logger = get_logger("encoding_handler");
        let default_encoding = kwargs::get_str(&config, "default_encoding", "utf-8");
        let fallback_encodings = kwargs::get_str_list(
            &config,
            "fallback_encodings",
            &["latin-1", "cp1252", "iso-8859-1"],
        );
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug(format!(
            "Encoding handler initialized (default={default_encoding})"
        ));
        EncodingHandler {
            config,
            default_encoding,
            fallback_encodings,
            progress_tracker,
            logger,
        }
    }

    /// `detect(data, **options)`: `(encoding, confidence)` from chardet (strings are encoded
    /// as UTF-8 first); `(default_encoding, 0.0)` for empty input. The encoding is `None` for
    /// binary data.
    pub fn detect(&self, data: impl Into<TextOrBytes>) -> (Option<String>, f64) {
        let data = data.into();
        let bytes = data.as_bytes();
        if bytes.is_empty() {
            return (Some(self.default_encoding.clone()), 0.0);
        }
        let result = crate::chardet::detect(bytes);
        (result.encoding, result.confidence)
    }

    /// `detect_file(file_path, sample_size=10000, **options)`: detect from the first
    /// `sample_size` bytes. A missing file raises `ValidationError`, an unreadable one
    /// `ProcessingError`.
    pub fn detect_file(
        &self,
        file_path: impl AsRef<Path>,
        sample_size: Option<usize>,
    ) -> Result<(Option<String>, f64), SemanticaError> {
        let path = file_path.as_ref();
        if !path.exists() {
            return Err(ValidationError::new(format!("File not found: {}", path.display())).into());
        }
        let sample_size = sample_size.unwrap_or(10_000);
        match std::fs::read(path) {
            Ok(mut data) => {
                data.truncate(sample_size);
                Ok(self.detect(data))
            }
            Err(e) => {
                let e = SemanticaError::from_io(&e, Some(path));
                self.logger
                    .error(format!("Failed to detect file encoding: {e}"));
                Err(ProcessingError::new(format!("Failed to detect file encoding: {e}")).into())
            }
        }
    }

    /// `convert_to_utf8(data, source_encoding=None, **options)`: strings are returned as they
    /// are; bytes are decoded (with replacement characters) using `source_encoding` or the
    /// detected encoding, then the fallback encodings, then `default_encoding`. Binary data
    /// (no detected encoding) raises Python's `TypeError`.
    pub fn convert_to_utf8(
        &self,
        data: impl Into<TextOrBytes>,
        source_encoding: Option<&str>,
    ) -> Result<String, SemanticaError> {
        let bytes = match data.into() {
            TextOrBytes::Text(s) => return Ok(s),
            TextOrBytes::Bytes(b) => b,
        };
        let source_encoding = match source_encoding {
            Some(e) => Some(e.to_string()),
            None => self.detect(bytes.as_slice()).0,
        };
        let Some(source_encoding) = source_encoding else {
            return Err(none_encoding_error());
        };
        if let Ok(text) = codecs::decode(&bytes, &source_encoding, "replace") {
            return Ok(text);
        }
        for encoding in &self.fallback_encodings {
            if let Ok(text) = codecs::decode(&bytes, encoding, "replace") {
                return Ok(text);
            }
        }
        self.logger
            .warning("Failed to decode with detected encoding, using errors='replace'");
        codecs::decode(&bytes, &self.default_encoding, "replace")
    }

    /// `convert_file_to_utf8(file_path, output_path=None, **options)`: read the file with the
    /// detected encoding (text mode: replacement characters, universal newlines), optionally
    /// write it as UTF-8 to `output_path` (creating parent directories), return the text.
    pub fn convert_file_to_utf8(
        &self,
        file_path: impl AsRef<Path>,
        output_path: Option<&Path>,
    ) -> Result<String, SemanticaError> {
        let path = file_path.as_ref();
        if !path.exists() {
            return Err(ValidationError::new(format!("File not found: {}", path.display())).into());
        }
        let (source_encoding, _) = self.detect_file(path, None)?;
        let read = |encoding: &str| -> Result<String, SemanticaError> {
            let bytes = std::fs::read(path).map_err(|e| SemanticaError::from_io(&e, Some(path)))?;
            let text = codecs::decode(&bytes, encoding, "replace")?;
            Ok(codecs::universal_newlines(&text))
        };
        // `open(..., encoding=None)` uses the locale encoding (UTF-8 here).
        let content = match read(source_encoding.as_deref().unwrap_or("utf-8")) {
            Ok(content) => content,
            Err(first_error) => {
                let mut content = None;
                for encoding in &self.fallback_encodings {
                    if let Ok(text) = read(encoding) {
                        content = Some(text);
                        break;
                    }
                }
                match content {
                    Some(content) => content,
                    None => {
                        self.logger.error(format!(
                            "Failed to read file with any encoding: {first_error}"
                        ));
                        return Err(ProcessingError::new(format!(
                            "Failed to read file: {first_error}"
                        ))
                        .into());
                    }
                }
            }
        };
        if let Some(output_path) = output_path {
            if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)
                    .map_err(|e| SemanticaError::from_io(&e, Some(parent)))?;
            }
            std::fs::write(output_path, content.as_bytes())
                .map_err(|e| SemanticaError::from_io(&e, Some(output_path)))?;
        }
        Ok(content)
    }

    /// `remove_bom(data)`: strip a UTF-8 or UTF-16 BOM from bytes, or U+FEFF from a string.
    pub fn remove_bom(&self, data: impl Into<TextOrBytes>) -> TextOrBytes {
        match data.into() {
            TextOrBytes::Bytes(bytes) => {
                let stripped = if bytes.starts_with(b"\xef\xbb\xbf") {
                    &bytes[3..]
                } else if bytes.starts_with(b"\xff\xfe") || bytes.starts_with(b"\xfe\xff") {
                    &bytes[2..]
                } else {
                    &bytes[..]
                };
                TextOrBytes::Bytes(stripped.to_vec())
            }
            TextOrBytes::Text(text) => match text.strip_prefix('\u{feff}') {
                Some(rest) => TextOrBytes::Text(rest.to_string()),
                None => TextOrBytes::Text(text),
            },
        }
    }

    /// `handle_encoding_error(data, error_strategy="replace", **options)`: decode with the
    /// detected encoding, then the fallbacks, then `default_encoding` (whose error, if any,
    /// propagates), using the given error handler.
    pub fn handle_encoding_error(
        &self,
        data: &[u8],
        error_strategy: Option<&str>,
    ) -> Result<String, SemanticaError> {
        let errors = error_strategy.unwrap_or("replace");
        let (encoding, _) = self.detect(data);
        let first = match encoding {
            Some(encoding) => codecs::decode(data, &encoding, errors),
            None => Err(none_encoding_error()),
        };
        if let Ok(text) = first {
            return Ok(text);
        }
        for encoding in &self.fallback_encodings {
            if let Ok(text) = codecs::decode(data, encoding, errors) {
                return Ok(text);
            }
        }
        codecs::decode(data, &self.default_encoding, errors)
    }

    /// `validate_encoding(data, encoding, **options)`: whether strings encode / bytes decode
    /// strictly with `encoding`.
    pub fn validate_encoding(&self, data: impl Into<TextOrBytes>, encoding: &str) -> bool {
        match data.into() {
            TextOrBytes::Text(text) => codecs::encode(&text, encoding, "strict").is_ok(),
            TextOrBytes::Bytes(bytes) => codecs::decode(&bytes, encoding, "strict").is_ok(),
        }
    }

    /// The logger (`semantica.encoding_handler`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}
