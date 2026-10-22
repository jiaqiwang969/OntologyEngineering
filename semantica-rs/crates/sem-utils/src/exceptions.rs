//! Exception hierarchy (`semantica/utils/exceptions.py`).
//!
//! Python's classes map to one error enum, [`SemanticaError`], with one variant per original
//! class (`SemanticaError`, `ValidationError`, `TemporalValidationError`, `ProcessingError`,
//! `ConfigurationError`, `QualityError`). Each class also exists as its own struct
//! ([`ValidationError`], [`ProcessingError`], ...) so code can build one and convert it with
//! `?`/`.into()`:
//!
//! ```
//! use sem_utils::{ProcessingError, SemanticaError, ValidationError};
//!
//! fn check(name: &str) -> Result<(), SemanticaError> {
//!     if name.is_empty() {
//!         return Err(ValidationError::new("Invalid data format").into());
//!     }
//!     Err(ProcessingError::new(format!("Failed to process {name}")).into())
//! }
//! let err = check("").unwrap_err();
//! assert!(err.is_validation_error());
//! assert_eq!(err.error_code(), Some("SEM001"));
//! ```
//!
//! The enum also carries the Python built-in exceptions Semantica raises (`ValueError`,
//! `TypeError`, `KeyError`, `OSError` subclasses, ...) so ported functions can keep their
//! original failure kinds; [`SemanticaError::is_semantica_error`] answers
//! `isinstance(e, SemanticaError)`.
//!
//! `Display` is Python's `str(e)` and `Debug` is Python's `repr(e)`.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::pycompat::fmt::{map_repr, py_str, slice_chars, str_repr, truthy};
use crate::pycompat::warnings::{USER_WARNING, WarningCategory};

/// The state every Semantica exception carries (`SemanticaError.__init__`).
#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// `e.message`.
    pub message: String,
    /// `e.context` (the class-specific `*_context` argument, `{}` when omitted).
    pub context: Map<String, Value>,
    /// `e.details`: the keyword details, including the `error_code` the subclasses pass up.
    pub details: Map<String, Value>,
    /// `e.error_code`.
    pub error_code: String,
}

impl ErrorInfo {
    /// `SemanticaError(message, context, **details)`: the error code is
    /// `details["error_code"]`, defaulting to `SEM000`.
    pub fn new(
        message: impl Into<String>,
        context: Option<Map<String, Value>>,
        details: Map<String, Value>,
    ) -> Self {
        let error_code = match details.get("error_code") {
            Some(Value::String(code)) => code.clone(),
            Some(other) => py_str(other),
            None => "SEM000".to_string(),
        };
        ErrorInfo {
            message: message.into(),
            context: context.unwrap_or_default(),
            details,
            error_code,
        }
    }

    fn subclass(
        message: impl Into<String>,
        context: Option<Map<String, Value>>,
        details: Map<String, Value>,
        details_code: &str,
        error_code: &str,
    ) -> Self {
        let mut all = Map::new();
        all.insert(
            "error_code".to_string(),
            Value::String(details_code.to_string()),
        );
        for (key, value) in details {
            if key != "error_code" {
                all.insert(key, value);
            }
        }
        ErrorInfo {
            message: message.into(),
            context: context.unwrap_or_default(),
            details: all,
            error_code: error_code.to_string(),
        }
    }

    /// `to_dict()` for an exception of class `class_name`.
    pub fn to_dict(&self, class_name: &str) -> Map<String, Value> {
        let mut out = Map::new();
        out.insert(
            "error_type".to_string(),
            Value::String(class_name.to_string()),
        );
        out.insert("message".to_string(), Value::String(self.message.clone()));
        out.insert(
            "error_code".to_string(),
            Value::String(self.error_code.clone()),
        );
        out.insert("context".to_string(), Value::Object(self.context.clone()));
        out.insert("details".to_string(), Value::Object(self.details.clone()));
        out
    }

    /// `repr(e)` for an exception of class `class_name`.
    pub fn repr(&self, class_name: &str) -> String {
        format!(
            "{}(message={}, context={}, error_code={}, details={})",
            class_name,
            str_repr(&self.message),
            map_repr(&self.context),
            self.error_code,
            map_repr(&self.details)
        )
    }
}

/// `k=v, k=v` with Python's `str()` of each value, optionally skipping `error_code`.
fn join_items(map: &Map<String, Value>, skip_error_code: bool) -> String {
    map.iter()
        .filter(|(k, _)| !(skip_error_code && k.as_str() == "error_code"))
        .map(|(k, v)| format!("{k}={}", py_str(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for ErrorInfo {
    /// `SemanticaError.__str__`: `message | Context: k=v | Details: k=v`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = vec![self.message.clone()];
        if !self.context.is_empty() {
            parts.push(format!("Context: {}", join_items(&self.context, false)));
        }
        if !self.details.is_empty() {
            let details = join_items(&self.details, true);
            if !details.is_empty() {
                parts.push(format!("Details: {details}"));
            }
        }
        f.write_str(&parts.join(" | "))
    }
}

impl fmt::Debug for ErrorInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.repr("SemanticaError"))
    }
}

/// `ValidationError(message, validation_context=None, **details)`: code `SEM001`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationError(Box<ErrorInfo>);

/// `TemporalValidationError(message, temporal_context=None, **details)`: a
/// `ValidationError` whose `error_code` is `SEM001T` (its `details` still record `SEM001`).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalValidationError(Box<ErrorInfo>);

/// `ProcessingError(message, processing_context=None, **details)`: code `SEM002`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessingError(Box<ErrorInfo>);

/// `ConfigurationError(message, config_context=None, **details)`: code `SEM003`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationError(Box<ErrorInfo>);

/// `QualityError(message, quality_context=None, **details)`: code `SEM004`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityError(Box<ErrorInfo>);

macro_rules! semantica_error_class {
    ($name:ident, $variant:ident, $details_code:literal, $error_code:literal) => {
        impl $name {
            /// Construct with a message only.
            pub fn new(message: impl Into<String>) -> Self {
                Self::with_details(message, None, Map::new())
            }

            /// Construct with a message and the class's context dictionary.
            pub fn with_context(message: impl Into<String>, context: Map<String, Value>) -> Self {
                Self::with_details(message, Some(context), Map::new())
            }

            /// Construct with a message, an optional context and keyword details.
            pub fn with_details(
                message: impl Into<String>,
                context: Option<Map<String, Value>>,
                details: Map<String, Value>,
            ) -> Self {
                $name(Box::new(ErrorInfo::subclass(
                    message,
                    context,
                    details,
                    $details_code,
                    $error_code,
                )))
            }

            /// Add one keyword detail (builder style).
            pub fn detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
                self.0.details.insert(key.into(), value.into());
                self
            }

            /// The shared exception state.
            pub fn info(&self) -> &ErrorInfo {
                &self.0
            }

            /// The shared exception state, mutably.
            pub fn info_mut(&mut self) -> &mut ErrorInfo {
                &mut self.0
            }

            /// `e.message`.
            pub fn message(&self) -> &str {
                &self.0.message
            }

            /// `e.context`.
            pub fn context(&self) -> &Map<String, Value> {
                &self.0.context
            }

            /// `e.details`.
            pub fn details(&self) -> &Map<String, Value> {
                &self.0.details
            }

            /// `e.error_code`.
            pub fn error_code(&self) -> &str {
                &self.0.error_code
            }

            /// `e.to_dict()`.
            pub fn to_dict(&self) -> Map<String, Value> {
                self.0.to_dict(stringify!($name))
            }

            fn detail_value(&self, key: &str) -> Option<&Value> {
                self.0.details.get(key)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0.repr(stringify!($name)))
            }
        }

        impl std::error::Error for $name {}

        impl From<$name> for SemanticaError {
            fn from(value: $name) -> Self {
                SemanticaError::$variant(value)
            }
        }
    };
}

semantica_error_class!(ValidationError, Validation, "SEM001", "SEM001");
semantica_error_class!(
    TemporalValidationError,
    TemporalValidation,
    "SEM001",
    "SEM001T"
);
semantica_error_class!(ProcessingError, Processing, "SEM002", "SEM002");
semantica_error_class!(ConfigurationError, Configuration, "SEM003", "SEM003");
semantica_error_class!(QualityError, Quality, "SEM004", "SEM004");

impl ValidationError {
    /// `e.validation_context`.
    pub fn validation_context(&self) -> &Map<String, Value> {
        &self.0.context
    }
    /// `e.field` (from the `field` detail).
    pub fn field(&self) -> Option<&Value> {
        self.detail_value("field")
    }
    /// `e.value` (from the `value` detail).
    pub fn value(&self) -> Option<&Value> {
        self.detail_value("value")
    }
    /// `e.constraint` (from the `constraint` detail).
    pub fn constraint(&self) -> Option<&Value> {
        self.detail_value("constraint")
    }
}

impl TemporalValidationError {
    /// `e.validation_context` (the `temporal_context` argument).
    pub fn validation_context(&self) -> &Map<String, Value> {
        &self.0.context
    }
    /// `e.field`.
    pub fn field(&self) -> Option<&Value> {
        self.detail_value("field")
    }
    /// `e.value`.
    pub fn value(&self) -> Option<&Value> {
        self.detail_value("value")
    }
    /// `e.constraint`.
    pub fn constraint(&self) -> Option<&Value> {
        self.detail_value("constraint")
    }
}

impl ProcessingError {
    /// `e.processing_context`.
    pub fn processing_context(&self) -> &Map<String, Value> {
        &self.0.context
    }
    /// `e.stage`.
    pub fn stage(&self) -> Option<&Value> {
        self.detail_value("stage")
    }
    /// `e.input_data`.
    pub fn input_data(&self) -> Option<&Value> {
        self.detail_value("input_data")
    }
    /// `e.output_data`.
    pub fn output_data(&self) -> Option<&Value> {
        self.detail_value("output_data")
    }
}

impl ConfigurationError {
    /// `e.config_context`.
    pub fn config_context(&self) -> &Map<String, Value> {
        &self.0.context
    }
    /// `e.config_key`.
    pub fn config_key(&self) -> Option<&Value> {
        self.detail_value("config_key")
    }
    /// `e.config_value`.
    pub fn config_value(&self) -> Option<&Value> {
        self.detail_value("config_value")
    }
    /// `e.expected_type`.
    pub fn expected_type(&self) -> Option<&Value> {
        self.detail_value("expected_type")
    }
}

impl QualityError {
    /// `e.quality_context`.
    pub fn quality_context(&self) -> &Map<String, Value> {
        &self.0.context
    }
    /// `e.quality_score`.
    pub fn quality_score(&self) -> Option<&Value> {
        self.detail_value("quality_score")
    }
    /// `e.threshold`.
    pub fn threshold(&self) -> Option<&Value> {
        self.detail_value("threshold")
    }
    /// `e.metrics` (`{}` when the detail is absent).
    pub fn metrics(&self) -> Value {
        self.detail_value("metrics")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()))
    }
}

/// An `OSError` (or subclass such as `FileNotFoundError`), with Python's `str()` layout:
/// `[Errno 2] No such file or directory: 'path'`, or just the message for `OSError(message)`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsError {
    /// The Python class name (`FileNotFoundError`, `PermissionError`, `OSError`, ...).
    pub type_name: String,
    /// `e.errno`.
    pub errno: Option<i32>,
    /// `e.strerror`, or the single message argument when there is no errno.
    pub text: String,
    /// `e.filename`.
    pub filename: Option<String>,
}

impl OsError {
    /// `<type_name>(message)`.
    pub fn new(type_name: impl Into<String>, message: impl Into<String>) -> Self {
        OsError {
            type_name: type_name.into(),
            errno: None,
            text: message.into(),
            filename: None,
        }
    }

    /// The `OSError` Python raises for an I/O failure on `filename`.
    pub fn from_io(err: &std::io::Error, filename: Option<&str>) -> Self {
        use std::io::ErrorKind;
        let type_name = match err.kind() {
            ErrorKind::NotFound => "FileNotFoundError",
            ErrorKind::PermissionDenied => "PermissionError",
            ErrorKind::AlreadyExists => "FileExistsError",
            ErrorKind::IsADirectory => "IsADirectoryError",
            ErrorKind::NotADirectory => "NotADirectoryError",
            ErrorKind::Interrupted => "InterruptedError",
            ErrorKind::TimedOut => "TimeoutError",
            ErrorKind::ConnectionRefused => "ConnectionRefusedError",
            ErrorKind::ConnectionReset => "ConnectionResetError",
            ErrorKind::ConnectionAborted => "ConnectionAbortedError",
            ErrorKind::BrokenPipe => "BrokenPipeError",
            ErrorKind::WouldBlock => "BlockingIOError",
            _ => "OSError",
        };
        let text = err.to_string();
        match err.raw_os_error() {
            Some(code) => {
                let suffix = format!(" (os error {code})");
                let strerror = text.strip_suffix(&suffix).unwrap_or(&text).to_string();
                OsError {
                    type_name: type_name.to_string(),
                    errno: Some(code),
                    text: strerror,
                    filename: filename.map(str::to_string),
                }
            }
            None => OsError {
                type_name: type_name.to_string(),
                errno: None,
                text,
                filename: filename.map(str::to_string),
            },
        }
    }
}

impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.errno, &self.filename) {
            (Some(errno), Some(filename)) => {
                write!(f, "[Errno {errno}] {}: {}", self.text, str_repr(filename))
            }
            (Some(errno), None) => write!(f, "[Errno {errno}] {}", self.text),
            (None, _) => f.write_str(&self.text),
        }
    }
}

impl fmt::Debug for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.errno {
            Some(errno) => write!(f, "{}({errno}, {})", self.type_name, str_repr(&self.text)),
            None => write!(f, "{}({})", self.type_name, str_repr(&self.text)),
        }
    }
}

/// An error raised by Semantica code: the framework hierarchy of `exceptions.py` plus the
/// Python built-in exception kinds the original raises.
#[derive(Clone, PartialEq, thiserror::Error)]
pub enum SemanticaError {
    /// `SemanticaError` itself (code `SEM000` unless an `error_code` detail is given).
    #[error("{0}")]
    Semantica(Box<ErrorInfo>),
    /// `ValidationError`.
    #[error("{0}")]
    Validation(ValidationError),
    /// `TemporalValidationError` (a `ValidationError`).
    #[error("{0}")]
    TemporalValidation(TemporalValidationError),
    /// `ProcessingError`.
    #[error("{0}")]
    Processing(ProcessingError),
    /// `ConfigurationError`.
    #[error("{0}")]
    Configuration(ConfigurationError),
    /// `QualityError`.
    #[error("{0}")]
    Quality(QualityError),
    /// Built-in `ValueError`.
    #[error("{0}")]
    Value(String),
    /// Built-in `TypeError`.
    #[error("{0}")]
    Type(String),
    /// Built-in `KeyError` holding the missing key (its `str()` is the key's `repr()`).
    #[error("{}", str_repr(.0))]
    Key(String),
    /// Built-in `IndexError`.
    #[error("{0}")]
    Index(String),
    /// Built-in `AttributeError`.
    #[error("{0}")]
    Attribute(String),
    /// Built-in `RuntimeError`.
    #[error("{0}")]
    Runtime(String),
    /// Built-in `NotImplementedError`.
    #[error("{0}")]
    NotImplemented(String),
    /// Built-in `ImportError`.
    #[error("{0}")]
    Import(String),
    /// Built-in `OverflowError`.
    #[error("{0}")]
    Overflow(String),
    /// Built-in `OSError` or one of its subclasses.
    #[error("{0}")]
    Os(OsError),
    /// `json.JSONDecodeError` (a `ValueError`).
    #[error("{0}")]
    JsonDecode(String),
    /// Any other exception class, identified by module and class name.
    #[error("{message}")]
    Other {
        /// `type(e).__module__`.
        module: String,
        /// `type(e).__name__`.
        type_name: String,
        /// `str(e)`.
        message: String,
    },
}

const EXCEPTIONS_MODULE: &str = "semantica.utils.exceptions";

impl SemanticaError {
    /// `SemanticaError(message)` (the base class, code `SEM000`).
    pub fn new(message: impl Into<String>) -> Self {
        SemanticaError::Semantica(Box::new(ErrorInfo::new(message, None, Map::new())))
    }

    /// `SemanticaError(message, context, **details)`.
    pub fn with_details(
        message: impl Into<String>,
        context: Option<Map<String, Value>>,
        details: Map<String, Value>,
    ) -> Self {
        SemanticaError::Semantica(Box::new(ErrorInfo::new(message, context, details)))
    }

    /// The `OSError` Python raises for an I/O failure on `path`.
    pub fn from_io(err: &std::io::Error, path: Option<&std::path::Path>) -> Self {
        let filename = path.map(|p| p.display().to_string());
        SemanticaError::Os(OsError::from_io(err, filename.as_deref()))
    }

    /// `type(e).__name__`.
    pub fn type_name(&self) -> String {
        match self {
            SemanticaError::Semantica(_) => "SemanticaError".to_string(),
            SemanticaError::Validation(_) => "ValidationError".to_string(),
            SemanticaError::TemporalValidation(_) => "TemporalValidationError".to_string(),
            SemanticaError::Processing(_) => "ProcessingError".to_string(),
            SemanticaError::Configuration(_) => "ConfigurationError".to_string(),
            SemanticaError::Quality(_) => "QualityError".to_string(),
            SemanticaError::Value(_) => "ValueError".to_string(),
            SemanticaError::Type(_) => "TypeError".to_string(),
            SemanticaError::Key(_) => "KeyError".to_string(),
            SemanticaError::Index(_) => "IndexError".to_string(),
            SemanticaError::Attribute(_) => "AttributeError".to_string(),
            SemanticaError::Runtime(_) => "RuntimeError".to_string(),
            SemanticaError::NotImplemented(_) => "NotImplementedError".to_string(),
            SemanticaError::Import(_) => "ImportError".to_string(),
            SemanticaError::Overflow(_) => "OverflowError".to_string(),
            SemanticaError::Os(os) => os.type_name.clone(),
            SemanticaError::JsonDecode(_) => "JSONDecodeError".to_string(),
            SemanticaError::Other { type_name, .. } => type_name.clone(),
        }
    }

    /// `type(e).__module__`.
    pub fn module_name(&self) -> &str {
        match self {
            SemanticaError::JsonDecode(_) => "json.decoder",
            SemanticaError::Other { module, .. } => module,
            e if e.is_semantica_error() => EXCEPTIONS_MODULE,
            _ => "builtins",
        }
    }

    /// The class name `traceback` prints: module-qualified unless built in.
    pub fn qualified_type_name(&self) -> String {
        let module = self.module_name();
        if module == "builtins" || module == "__main__" || module.is_empty() {
            self.type_name()
        } else {
            format!("{module}.{}", self.type_name())
        }
    }

    /// `isinstance(e, SemanticaError)`.
    pub fn is_semantica_error(&self) -> bool {
        self.info().is_some()
    }

    /// `isinstance(e, ValidationError)` (true for `TemporalValidationError` too).
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            SemanticaError::Validation(_) | SemanticaError::TemporalValidation(_)
        )
    }

    /// `isinstance(e, ValueError)` (true for `JSONDecodeError` too).
    pub fn is_value_error(&self) -> bool {
        matches!(
            self,
            SemanticaError::Value(_) | SemanticaError::JsonDecode(_)
        )
    }

    /// The framework state, `None` for built-in exception kinds.
    pub fn info(&self) -> Option<&ErrorInfo> {
        match self {
            SemanticaError::Semantica(info) => Some(info),
            SemanticaError::Validation(e) => Some(&e.0),
            SemanticaError::TemporalValidation(e) => Some(&e.0),
            SemanticaError::Processing(e) => Some(&e.0),
            SemanticaError::Configuration(e) => Some(&e.0),
            SemanticaError::Quality(e) => Some(&e.0),
            _ => None,
        }
    }

    /// `e.message` for Semantica errors, `str(e)` otherwise.
    pub fn message(&self) -> String {
        match self.info() {
            Some(info) => info.message.clone(),
            None => self.to_string(),
        }
    }

    /// `e.error_code` (`None` for built-in exception kinds).
    pub fn error_code(&self) -> Option<&str> {
        self.info().map(|info| info.error_code.as_str())
    }

    /// `e.context` (`None` for built-in exception kinds).
    pub fn context(&self) -> Option<&Map<String, Value>> {
        self.info().map(|info| &info.context)
    }

    /// `e.details` (`None` for built-in exception kinds).
    pub fn details(&self) -> Option<&Map<String, Value>> {
        self.info().map(|info| &info.details)
    }

    /// `e.to_dict()`; built-in exception kinds have no `to_dict` and return `None`.
    pub fn to_dict(&self) -> Option<Map<String, Value>> {
        self.info().map(|info| info.to_dict(&self.type_name()))
    }

    /// What `traceback.format_exception` produces for this exception. Rust keeps no Python
    /// frames, so this is the frameless form (`module.Class: message`).
    pub fn traceback_text(&self) -> String {
        let text = self.to_string();
        if text.is_empty() {
            format!("{}\n", self.qualified_type_name())
        } else {
            format!("{}: {text}\n", self.qualified_type_name())
        }
    }
}

impl From<std::io::Error> for SemanticaError {
    /// The `OSError` subclass Python raises for this I/O failure (no file name attached).
    fn from(err: std::io::Error) -> Self {
        SemanticaError::from_io(&err, None)
    }
}

impl fmt::Debug for SemanticaError {
    /// Python's `repr(e)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticaError::Os(os) => fmt::Debug::fmt(os, f),
            SemanticaError::Key(key) => write!(f, "KeyError({})", str_repr(key)),
            SemanticaError::Other {
                type_name, message, ..
            } => write!(f, "{type_name}({})", str_repr(message)),
            other => match other.info() {
                Some(info) => f.write_str(&info.repr(&other.type_name())),
                None => write!(f, "{}({})", other.type_name(), str_repr(&other.to_string())),
            },
        }
    }
}

/// Marker for the `TemporalAmbiguityWarning(UserWarning)` category: raised (via
/// [`crate::pycompat::warnings::warn`]) when a temporal expression such as `03/04/2022` is
/// ambiguous without locale information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TemporalAmbiguityWarning;

/// The warning category behind [`TemporalAmbiguityWarning`].
pub static TEMPORAL_AMBIGUITY_WARNING: WarningCategory =
    WarningCategory::new("TemporalAmbiguityWarning", &USER_WARNING);

impl From<TemporalAmbiguityWarning> for WarningCategory {
    fn from(_: TemporalAmbiguityWarning) -> Self {
        TEMPORAL_AMBIGUITY_WARNING
    }
}

/// `handle_exception(exception, context=None, **options)`: structured error data.
///
/// Options: `recovery_action` (default `"log_and_continue"`), `log_error` (default true:
/// logs through [`crate::logging::log_error`], which receives every option as a detail) and
/// `include_traceback` (default true). The returned `context` is the caller's context updated
/// with the exception's own context. Fails only when logging does (e.g. a non-integer `level`).
pub fn handle_exception(
    exception: &SemanticaError,
    context: Option<&Map<String, Value>>,
    options: &Map<String, Value>,
) -> Result<Map<String, Value>, SemanticaError> {
    let mut context = context.cloned().unwrap_or_default();
    let recovery = options
        .get("recovery_action")
        .cloned()
        .unwrap_or_else(|| Value::String("log_and_continue".to_string()));
    let log = options.get("log_error").map(truthy).unwrap_or(true);

    let mut traceback = Value::Null;
    if options.get("include_traceback").map(truthy).unwrap_or(true) {
        traceback = Value::String(exception.traceback_text());
    }
    let mut error_code = None;
    let mut details = None;
    if let Some(info) = exception.info() {
        error_code = Some(Value::String(info.error_code.clone()));
        details = Some(Value::Object(info.details.clone()));
        for (key, value) in &info.context {
            context.insert(key.clone(), value.clone());
        }
    }

    let mut error_info = Map::new();
    error_info.insert("handled".to_string(), Value::Bool(true));
    error_info.insert(
        "error_type".to_string(),
        Value::String(exception.type_name()),
    );
    error_info.insert("message".to_string(), Value::String(exception.to_string()));
    error_info.insert("context".to_string(), Value::Object(context.clone()));
    error_info.insert("traceback".to_string(), traceback);
    error_info.insert("recovery".to_string(), recovery);
    if let (Some(code), Some(details)) = (error_code, details) {
        error_info.insert("error_code".to_string(), code);
        error_info.insert("details".to_string(), details);
    }

    if log {
        crate::logging::log_error(exception, Some(&context), options)?;
    }
    Ok(error_info)
}

/// Keyword options of [`format_exception`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatExceptionOptions {
    /// `include_traceback` (default false).
    pub include_traceback: bool,
    /// `include_context` (default true).
    pub include_context: bool,
    /// `max_length` (default none; `0` means no limit and negative values slice from the end,
    /// as in Python).
    pub max_length: Option<i64>,
}

impl Default for FormatExceptionOptions {
    fn default() -> Self {
        FormatExceptionOptions {
            include_traceback: false,
            include_context: true,
            max_length: None,
        }
    }
}

/// `format_exception(exception, **options)`: a display/log string with the error code,
/// context and details of Semantica errors.
pub fn format_exception(exception: &SemanticaError, options: &FormatExceptionOptions) -> String {
    let mut parts = vec![format!("{}: {}", exception.type_name(), exception)];
    if let Some(info) = exception.info() {
        if !info.error_code.is_empty() {
            parts.push(format!("Error Code: {}", info.error_code));
        }
        if options.include_context && !info.context.is_empty() {
            parts.push(format!("Context: {}", join_items(&info.context, false)));
        }
        if !info.details.is_empty() {
            let details = join_items(&info.details, true);
            if !details.is_empty() {
                parts.push(format!("Details: {details}"));
            }
        }
    }
    if options.include_traceback {
        parts.push(format!("\nTraceback:\n{}", exception.traceback_text()));
    }
    let formatted = parts.join("\n");
    match options.max_length {
        Some(max) if max != 0 && (formatted.chars().count() as i64) > max => {
            format!("{}...", slice_chars(&formatted, None, Some(max)))
        }
        _ => formatted,
    }
}
