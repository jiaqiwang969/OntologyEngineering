//! Logging utilities (`semantica/utils/logging.py`) and the small port of Python's `logging`
//! module they run on.
//!
//! Loggers form Python's dotted hierarchy: `get_logger("ingest")` returns the logger named
//! `semantica.ingest`, whose records propagate to `semantica` (configured by
//! [`setup_logging`]) and then to the root logger. As in Python, a record that reaches no
//! handler at all is written to stderr when it is `WARNING` or above (the "last resort"
//! handler), and the root logger's default level is `WARNING`, so `info`/`debug` output is
//! silent until a level is configured.
//!
//! ```
//! use sem_utils::logging::{Handler, LogLevel};
//! use sem_utils::get_logger;
//!
//! let logger = get_logger("doc_example");
//! let capture = Handler::capture();
//! logger.add_handler(capture.clone());
//! logger.set_level(LogLevel::INFO);
//! logger.info("Processing started");
//! assert_eq!(capture.messages(), vec!["Processing started".to_string()]);
//! ```

use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::panic::Location;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Value};

use crate::constants::DEFAULT_CONFIG;
use crate::exceptions::SemanticaError;
use crate::pycompat::fmt::{
    PyNum, format_fixed, format_value_fixed, path_name, percent_format, py_repr, py_str, truthy,
    type_name,
};
use crate::pycompat::time::{PyDateTime, strftime};

/// A logging level (Python's integer levels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogLevel(pub i32);

impl LogLevel {
    /// `logging.NOTSET`.
    pub const NOTSET: LogLevel = LogLevel(0);
    /// `logging.DEBUG`.
    pub const DEBUG: LogLevel = LogLevel(10);
    /// `logging.INFO`.
    pub const INFO: LogLevel = LogLevel(20);
    /// `logging.WARNING`.
    pub const WARNING: LogLevel = LogLevel(30);
    /// `logging.ERROR`.
    pub const ERROR: LogLevel = LogLevel(40);
    /// `logging.CRITICAL`.
    pub const CRITICAL: LogLevel = LogLevel(50);

    /// `logging.getLevelName(level)`.
    pub fn name(self) -> String {
        match self.0 {
            0 => "NOTSET".to_string(),
            10 => "DEBUG".to_string(),
            20 => "INFO".to_string(),
            30 => "WARNING".to_string(),
            40 => "ERROR".to_string(),
            50 => "CRITICAL".to_string(),
            n => format!("Level {n}"),
        }
    }

    /// `getattr(logging, name.upper(), logging.INFO)`: the level a configuration string names.
    pub fn from_name(name: &str) -> LogLevel {
        match name.to_uppercase().as_str() {
            "CRITICAL" | "FATAL" => LogLevel::CRITICAL,
            "ERROR" => LogLevel::ERROR,
            "WARNING" | "WARN" => LogLevel::WARNING,
            "INFO" => LogLevel::INFO,
            "DEBUG" => LogLevel::DEBUG,
            "NOTSET" => LogLevel::NOTSET,
            _ => LogLevel::INFO,
        }
    }

    /// The level `Logger.log(level, ...)` accepts: an integer (or bool). Anything else is the
    /// `TypeError` Python raises.
    pub fn from_log_argument(value: &Value) -> Result<LogLevel, SemanticaError> {
        match value {
            Value::Bool(b) => Ok(LogLevel(i32::from(*b))),
            Value::Number(n) if !n.is_f64() => n
                .as_i64()
                .and_then(|i| i32::try_from(i).ok())
                .map(LogLevel)
                .ok_or_else(|| SemanticaError::Overflow("level out of range".to_string())),
            _ => Err(SemanticaError::Type("level must be an integer".to_string())),
        }
    }

    /// The level `setup_logging` reads from configuration: a name (unknown names give `INFO`)
    /// or an integer. Other values are rejected as `Logger.setLevel` rejects them.
    pub fn from_config(value: &Value) -> Result<LogLevel, SemanticaError> {
        match value {
            Value::String(name) => Ok(LogLevel::from_name(name)),
            Value::Bool(_) | Value::Number(_) => LogLevel::from_log_argument(value).map_err(|_| {
                SemanticaError::Type(format!(
                    "Level not an integer or a valid string: {}",
                    py_repr(value)
                ))
            }),
            other => Err(SemanticaError::Type(format!(
                "Level not an integer or a valid string: {}",
                py_repr(other)
            ))),
        }
    }
}

impl From<i32> for LogLevel {
    fn from(value: i32) -> Self {
        LogLevel(value)
    }
}

impl From<&str> for LogLevel {
    fn from(value: &str) -> Self {
        LogLevel::from_name(value)
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

static START_TIME: Lazy<f64> = Lazy::new(now_seconds);

fn thread_ident() -> u64 {
    let text = format!("{:?}", std::thread::current().id());
    text.trim_start_matches("ThreadId(")
        .trim_end_matches(')')
        .parse()
        .unwrap_or(0)
}

fn thread_name() -> String {
    match std::thread::current().name() {
        Some("main") => "MainThread".to_string(),
        Some(name) => name.to_string(),
        None => format!("Thread-{}", thread_ident()),
    }
}

/// A log record (`logging.LogRecord`).
#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    /// Logger name.
    pub name: String,
    /// `levelno`.
    pub level: LogLevel,
    /// The message (`getMessage()`).
    pub message: String,
    /// `created` (seconds since the epoch).
    pub created: f64,
    /// Source file of the logging call.
    pub pathname: String,
    /// Line of the logging call.
    pub lineno: u32,
    /// `threadName`.
    pub thread_name: String,
    /// `thread`.
    pub thread: u64,
    /// `process`.
    pub process: u32,
    /// Attributes passed with `extra=`.
    pub extra: Map<String, Value>,
}

const RECORD_ATTRIBUTES: &[&str] = &[
    "name",
    "msg",
    "args",
    "levelname",
    "levelno",
    "pathname",
    "filename",
    "module",
    "exc_info",
    "exc_text",
    "stack_info",
    "lineno",
    "funcName",
    "created",
    "msecs",
    "relativeCreated",
    "thread",
    "threadName",
    "processName",
    "process",
    "message",
    "asctime",
];

impl LogRecord {
    /// `record.levelname`.
    pub fn level_name(&self) -> String {
        self.level.name()
    }

    /// `record.msecs`.
    pub fn msecs(&self) -> f64 {
        ((self.created - self.created.trunc()) * 1000.0).trunc()
    }

    /// `getattr(record, key)` for the attributes `%`-style formats can name.
    pub fn attribute(&self, key: &str) -> Option<Value> {
        let filename = path_name(&self.pathname.replace('\\', "/"));
        Some(match key {
            "name" => Value::String(self.name.clone()),
            "msg" | "message" => Value::String(self.message.clone()),
            "args" => Value::Array(Vec::new()),
            "levelname" => Value::String(self.level_name()),
            "levelno" => Value::from(self.level.0),
            "pathname" => Value::String(self.pathname.clone()),
            "filename" => Value::String(filename),
            "module" => Value::String(
                filename
                    .rsplit_once('.')
                    .map(|(stem, _)| stem.to_string())
                    .unwrap_or(filename),
            ),
            "exc_info" | "exc_text" | "stack_info" => Value::Null,
            "lineno" => Value::from(self.lineno),
            "funcName" => Value::String("(unknown function)".to_string()),
            "created" => Value::from(self.created),
            "msecs" => Value::from(self.msecs()),
            "relativeCreated" => Value::from((self.created - *START_TIME) * 1000.0),
            "thread" => Value::from(self.thread),
            "threadName" => Value::String(self.thread_name.clone()),
            "processName" => Value::String("MainProcess".to_string()),
            "process" => Value::from(self.process),
            other => return self.extra.get(other).cloned(),
        })
    }
}

static FORMAT_VALIDATION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)%\(\w+\)[#0+ -]*(\*|\d+)?(\.(\*|\d+))?[diouxefgcrsa%]")
        .expect("validation pattern")
});

/// `logging.Formatter` with `%`-style formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatter {
    fmt: String,
    datefmt: Option<String>,
}

impl Default for Formatter {
    fn default() -> Self {
        Formatter {
            fmt: "%(message)s".to_string(),
            datefmt: None,
        }
    }
}

impl Formatter {
    /// `logging.Formatter(fmt, datefmt)`; an empty `fmt` means `%(message)s`. A format with no
    /// `%(name)s`-style field is the `ValueError` Python raises.
    pub fn new(fmt: Option<&str>, datefmt: Option<&str>) -> Result<Formatter, SemanticaError> {
        let fmt = match fmt {
            Some(f) if !f.is_empty() => f.to_string(),
            _ => "%(message)s".to_string(),
        };
        if !FORMAT_VALIDATION.is_match(&fmt) {
            return Err(SemanticaError::Value(format!(
                "Invalid format '{fmt}' for '%' style"
            )));
        }
        Ok(Formatter {
            fmt,
            datefmt: datefmt.map(str::to_string),
        })
    }

    /// The format string.
    pub fn format_string(&self) -> &str {
        &self.fmt
    }

    /// `formatTime(record, datefmt)`: local time, `%Y-%m-%d %H:%M:%S,mmm` by default.
    pub fn format_time(&self, record: &LogRecord) -> String {
        let secs = record.created.floor() as i64;
        let nanos = ((record.created - record.created.floor()) * 1e9) as u32;
        let local = Local
            .timestamp_opt(secs, nanos)
            .earliest()
            .map(|dt| PyDateTime::Naive(dt.naive_local()))
            .unwrap_or_else(|| PyDateTime::now(false));
        match &self.datefmt {
            Some(datefmt) => strftime(&local, datefmt),
            None => format!(
                "{},{:03}",
                strftime(&local, "%Y-%m-%d %H:%M:%S"),
                record.msecs() as i64
            ),
        }
    }

    /// `format(record)`; a field the record lacks is the `ValueError` Python reports.
    pub fn format(&self, record: &LogRecord) -> Result<String, SemanticaError> {
        let asctime = self
            .fmt
            .contains("%(asctime)")
            .then(|| self.format_time(record));
        let lookup = |key: &str| -> Option<Value> {
            if key == "asctime" {
                return asctime.clone().map(Value::String);
            }
            record.attribute(key)
        };
        percent_format(&self.fmt, &lookup).map_err(|err| match err {
            SemanticaError::Key(key) => SemanticaError::Value(format!(
                "Formatting field not found in record: {}",
                crate::pycompat::fmt::str_repr(&key)
            )),
            other => other,
        })
    }
}

enum Sink {
    Stdout,
    Stderr,
    Writer(Box<dyn Write + Send>),
    File {
        path: PathBuf,
        file: Option<File>,
    },
    Rotating {
        path: PathBuf,
        file: Option<File>,
        max_bytes: u64,
        backup_count: u32,
    },
    Capture(Vec<(LogRecord, String)>),
}

struct HandlerInner {
    level: RwLock<LogLevel>,
    formatter: RwLock<Formatter>,
    sink: Mutex<Sink>,
}

/// A log handler (`StreamHandler`, `FileHandler`, `RotatingFileHandler`, or an in-memory
/// capture for tests). Cloning gives another handle to the same handler.
#[derive(Clone)]
pub struct Handler(Arc<HandlerInner>);

impl fmt::Debug for Handler {
    /// Python's handler `repr`, e.g. `<StreamHandler <stdout> (INFO)>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = self.level().name();
        let path = self.path().map(|p| p.display().to_string());
        match (self.kind(), path) {
            ("stdout", _) => write!(f, "<StreamHandler <stdout> ({level})>"),
            ("stderr", _) => write!(f, "<StreamHandler <stderr> ({level})>"),
            ("file", Some(path)) => write!(f, "<FileHandler {path} ({level})>"),
            ("rotating_file", Some(path)) => write!(f, "<RotatingFileHandler {path} ({level})>"),
            ("capture", _) => write!(f, "<CaptureHandler ({level})>"),
            _ => write!(f, "<StreamHandler ({level})>"),
        }
    }
}

fn open_append(path: &Path) -> Result<File, SemanticaError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| SemanticaError::from_io(&e, Some(path)))
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

impl Handler {
    fn with_sink(sink: Sink) -> Handler {
        Handler(Arc::new(HandlerInner {
            level: RwLock::new(LogLevel::NOTSET),
            formatter: RwLock::new(Formatter::default()),
            sink: Mutex::new(sink),
        }))
    }

    /// `StreamHandler(sys.stdout)`.
    pub fn stdout() -> Handler {
        Handler::with_sink(Sink::Stdout)
    }

    /// `StreamHandler()` (stderr).
    pub fn stderr() -> Handler {
        Handler::with_sink(Sink::Stderr)
    }

    /// `StreamHandler(stream)` over any writer.
    pub fn writer(writer: Box<dyn Write + Send>) -> Handler {
        Handler::with_sink(Sink::Writer(writer))
    }

    /// `FileHandler(path, encoding="utf-8")` (append mode, opened immediately).
    pub fn file(path: impl AsRef<Path>) -> Result<Handler, SemanticaError> {
        let path = absolute(path.as_ref());
        let file = open_append(&path)?;
        Ok(Handler::with_sink(Sink::File {
            path,
            file: Some(file),
        }))
    }

    /// `RotatingFileHandler(path, maxBytes=max_bytes, backupCount=backup_count)`.
    pub fn rotating_file(
        path: impl AsRef<Path>,
        max_bytes: u64,
        backup_count: u32,
    ) -> Result<Handler, SemanticaError> {
        let path = absolute(path.as_ref());
        let file = open_append(&path)?;
        Ok(Handler::with_sink(Sink::Rotating {
            path,
            file: Some(file),
            max_bytes,
            backup_count,
        }))
    }

    /// An in-memory handler that keeps every record it handles (for tests, like pytest's
    /// `caplog`).
    pub fn capture() -> Handler {
        Handler::with_sink(Sink::Capture(Vec::new()))
    }

    /// What this handler writes to: `stdout`, `stderr`, `stream`, `file`, `rotating_file` or
    /// `capture`.
    pub fn kind(&self) -> &'static str {
        match &*self.0.sink.lock().unwrap_or_else(|e| e.into_inner()) {
            Sink::Stdout => "stdout",
            Sink::Stderr => "stderr",
            Sink::Writer(_) => "stream",
            Sink::File { .. } => "file",
            Sink::Rotating { .. } => "rotating_file",
            Sink::Capture(_) => "capture",
        }
    }

    /// `baseFilename` of a file handler.
    pub fn path(&self) -> Option<PathBuf> {
        match &*self.0.sink.lock().unwrap_or_else(|e| e.into_inner()) {
            Sink::File { path, .. } | Sink::Rotating { path, .. } => Some(path.clone()),
            _ => None,
        }
    }

    /// `handler.level`.
    pub fn level(&self) -> LogLevel {
        *self.0.level.read().unwrap_or_else(|e| e.into_inner())
    }

    /// `handler.setLevel(level)`.
    pub fn set_level(&self, level: LogLevel) {
        *self.0.level.write().unwrap_or_else(|e| e.into_inner()) = level;
    }

    /// `handler.formatter`.
    pub fn formatter(&self) -> Formatter {
        self.0
            .formatter
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// `handler.setFormatter(formatter)`.
    pub fn set_formatter(&self, formatter: Formatter) {
        *self.0.formatter.write().unwrap_or_else(|e| e.into_inner()) = formatter;
    }

    /// Records a capture handler has kept.
    pub fn records(&self) -> Vec<LogRecord> {
        match &*self.0.sink.lock().unwrap_or_else(|e| e.into_inner()) {
            Sink::Capture(items) => items.iter().map(|(r, _)| r.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// Formatted lines a capture handler has kept.
    pub fn messages(&self) -> Vec<String> {
        match &*self.0.sink.lock().unwrap_or_else(|e| e.into_inner()) {
            Sink::Capture(items) => items.iter().map(|(_, s)| s.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// Forget the records a capture handler has kept.
    pub fn clear_records(&self) {
        if let Sink::Capture(items) = &mut *self.0.sink.lock().unwrap_or_else(|e| e.into_inner()) {
            items.clear();
        }
    }

    /// Whether two handles refer to the same handler.
    pub fn ptr_eq(&self, other: &Handler) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// `handler.handle(record)`: format and emit (a formatting or I/O failure is reported on
    /// stderr, as `Handler.handleError` does, and the record is dropped).
    pub fn handle(&self, record: &LogRecord) {
        let formatted = match self.formatter().format(record) {
            Ok(text) => text,
            Err(err) => return handle_error(record, &err),
        };
        let mut sink = self.0.sink.lock().unwrap_or_else(|e| e.into_inner());
        let result = match &mut *sink {
            Sink::Stdout => {
                println!("{formatted}");
                Ok(())
            }
            Sink::Stderr => {
                eprintln!("{formatted}");
                Ok(())
            }
            Sink::Writer(writer) => writer
                .write_all(format!("{formatted}\n").as_bytes())
                .and_then(|_| writer.flush())
                .map_err(|e| SemanticaError::from_io(&e, None)),
            Sink::File { path, file } => write_file(path, file, &formatted),
            Sink::Rotating {
                path,
                file,
                max_bytes,
                backup_count,
            } => {
                let msg_len = formatted.chars().count() as u64 + 1;
                let rollover = should_rollover(path, file, *max_bytes, msg_len);
                let rolled = match rollover {
                    Ok(true) => do_rollover(path, file, *backup_count),
                    Ok(false) => Ok(()),
                    Err(e) => Err(e),
                };
                rolled.and_then(|_| write_file(path, file, &formatted))
            }
            Sink::Capture(items) => {
                items.push((record.clone(), formatted));
                Ok(())
            }
        };
        drop(sink);
        if let Err(err) = result {
            handle_error(record, &err);
        }
    }

    /// `handler.flush()`.
    pub fn flush(&self) {
        let mut sink = self.0.sink.lock().unwrap_or_else(|e| e.into_inner());
        let _ = match &mut *sink {
            Sink::Stdout => std::io::stdout().flush(),
            Sink::Stderr => std::io::stderr().flush(),
            Sink::Writer(w) => w.flush(),
            Sink::File { file: Some(f), .. } | Sink::Rotating { file: Some(f), .. } => f.flush(),
            _ => Ok(()),
        };
    }
}

fn write_file(path: &Path, file: &mut Option<File>, text: &str) -> Result<(), SemanticaError> {
    if file.is_none() {
        *file = Some(open_append(path)?);
    }
    let handle = file.as_mut().expect("file just opened");
    handle
        .write_all(format!("{text}\n").as_bytes())
        .and_then(|_| handle.flush())
        .map_err(|e| SemanticaError::from_io(&e, Some(path)))
}

fn should_rollover(
    path: &Path,
    file: &mut Option<File>,
    max_bytes: u64,
    msg_len: u64,
) -> Result<bool, SemanticaError> {
    if path.exists() && !path.is_file() {
        return Ok(false);
    }
    if file.is_none() {
        *file = Some(open_append(path)?);
    }
    if max_bytes > 0 {
        let handle = file.as_mut().expect("file just opened");
        let pos = handle
            .seek(SeekFrom::End(0))
            .map_err(|e| SemanticaError::from_io(&e, Some(path)))?;
        if pos + msg_len >= max_bytes {
            return Ok(true);
        }
    }
    Ok(false)
}

fn do_rollover(
    path: &Path,
    file: &mut Option<File>,
    backup_count: u32,
) -> Result<(), SemanticaError> {
    *file = None;
    let numbered = |i: u32| PathBuf::from(format!("{}.{i}", path.display()));
    let io = |e: std::io::Error, p: &Path| SemanticaError::from_io(&e, Some(p));
    if backup_count > 0 {
        for i in (1..backup_count).rev() {
            let (source, dest) = (numbered(i), numbered(i + 1));
            if source.exists() {
                if dest.exists() {
                    std::fs::remove_file(&dest).map_err(|e| io(e, &dest))?;
                }
                std::fs::rename(&source, &dest).map_err(|e| io(e, &source))?;
            }
        }
        let dest = numbered(1);
        if dest.exists() {
            std::fs::remove_file(&dest).map_err(|e| io(e, &dest))?;
        }
        if path.exists() {
            std::fs::rename(path, &dest).map_err(|e| io(e, path))?;
        }
    }
    *file = Some(open_append(path)?);
    Ok(())
}

/// `Handler.handleError(record)`.
fn handle_error(record: &LogRecord, err: &SemanticaError) {
    eprint!(
        "--- Logging error ---\n{}\nLogged from file {}, line {}\nMessage: {}\nArguments: ()\n",
        err.traceback_text().trim_end(),
        path_name(&record.pathname),
        record.lineno,
        crate::pycompat::fmt::str_repr(&record.message)
    );
}

struct LoggerState {
    level: LogLevel,
    propagate: bool,
    handlers: Vec<Handler>,
}

struct LoggerInner {
    name: String,
    state: RwLock<LoggerState>,
}

/// A named logger (`logging.Logger`). Cloning gives another handle to the same logger.
#[derive(Clone)]
pub struct Logger(Arc<LoggerInner>);

impl fmt::Debug for Logger {
    /// Python's logger `repr`: `<Logger name (EFFECTIVE_LEVEL)>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let class = if Arc::ptr_eq(&self.0, &ROOT.0) {
            "RootLogger"
        } else {
            "Logger"
        };
        write!(
            f,
            "<{class} {} ({})>",
            self.name(),
            self.effective_level().name()
        )
    }
}

static ROOT: Lazy<Logger> = Lazy::new(|| Logger::new_inner("root", LogLevel::WARNING));
static LOGGERS: Lazy<Mutex<HashMap<String, Logger>>> = Lazy::new(|| Mutex::new(HashMap::new()));

impl Logger {
    fn new_inner(name: &str, level: LogLevel) -> Logger {
        Logger(Arc::new(LoggerInner {
            name: name.to_string(),
            state: RwLock::new(LoggerState {
                level,
                propagate: true,
                handlers: Vec::new(),
            }),
        }))
    }

    /// `logging.getLogger(name)` (`""` or `"root"` give the root logger).
    pub fn get(name: &str) -> Logger {
        if name.is_empty() || name == "root" {
            return ROOT.clone();
        }
        let mut loggers = LOGGERS.lock().unwrap_or_else(|e| e.into_inner());
        loggers
            .entry(name.to_string())
            .or_insert_with(|| Logger::new_inner(name, LogLevel::NOTSET))
            .clone()
    }

    /// `logging.getLogger()`.
    pub fn root() -> Logger {
        ROOT.clone()
    }

    /// `logger.name`.
    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// `logger.parent`: the nearest existing ancestor, or the root logger.
    pub fn parent(&self) -> Option<Logger> {
        if Arc::ptr_eq(&self.0, &ROOT.0) {
            return None;
        }
        let loggers = LOGGERS.lock().unwrap_or_else(|e| e.into_inner());
        let mut name = self.0.name.as_str();
        while let Some((prefix, _)) = name.rsplit_once('.') {
            if let Some(found) = loggers.get(prefix) {
                return Some(found.clone());
            }
            name = prefix;
        }
        Some(ROOT.clone())
    }

    fn state(&self) -> std::sync::RwLockReadGuard<'_, LoggerState> {
        self.0.state.read().unwrap_or_else(|e| e.into_inner())
    }

    fn state_mut(&self) -> std::sync::RwLockWriteGuard<'_, LoggerState> {
        self.0.state.write().unwrap_or_else(|e| e.into_inner())
    }

    /// `logger.level`.
    pub fn level(&self) -> LogLevel {
        self.state().level
    }

    /// `logger.setLevel(level)`.
    pub fn set_level(&self, level: LogLevel) {
        self.state_mut().level = level;
    }

    /// `logger.getEffectiveLevel()`.
    pub fn effective_level(&self) -> LogLevel {
        let mut current = Some(self.clone());
        while let Some(logger) = current {
            let level = logger.level();
            if level != LogLevel::NOTSET {
                return level;
            }
            current = logger.parent();
        }
        LogLevel::NOTSET
    }

    /// `logger.isEnabledFor(level)`.
    pub fn is_enabled_for(&self, level: LogLevel) -> bool {
        level >= self.effective_level()
    }

    /// `logger.propagate`.
    pub fn propagate(&self) -> bool {
        self.state().propagate
    }

    /// `logger.propagate = value`.
    pub fn set_propagate(&self, value: bool) {
        self.state_mut().propagate = value;
    }

    /// `logger.handlers` (handles to the same handlers).
    pub fn handlers(&self) -> Vec<Handler> {
        self.state().handlers.clone()
    }

    /// `logger.addHandler(handler)` (adding the same handler twice has no effect).
    pub fn add_handler(&self, handler: Handler) {
        let mut state = self.state_mut();
        if !state.handlers.iter().any(|h| h.ptr_eq(&handler)) {
            state.handlers.push(handler);
        }
    }

    /// `logger.removeHandler(handler)`.
    pub fn remove_handler(&self, handler: &Handler) {
        self.state_mut().handlers.retain(|h| !h.ptr_eq(handler));
    }

    /// `logger.handlers.clear()`.
    pub fn clear_handlers(&self) {
        self.state_mut().handlers.clear();
    }

    /// `logger.hasHandlers()`.
    pub fn has_handlers(&self) -> bool {
        let mut current = Some(self.clone());
        while let Some(logger) = current {
            if !logger.state().handlers.is_empty() {
                return true;
            }
            if !logger.propagate() {
                return false;
            }
            current = logger.parent();
        }
        false
    }

    /// `logger.debug(msg)`.
    #[track_caller]
    pub fn debug(&self, msg: impl AsRef<str>) {
        self.emit(
            LogLevel::DEBUG,
            msg.as_ref(),
            Map::new(),
            Location::caller(),
        );
    }

    /// `logger.info(msg)`.
    #[track_caller]
    pub fn info(&self, msg: impl AsRef<str>) {
        self.emit(LogLevel::INFO, msg.as_ref(), Map::new(), Location::caller());
    }

    /// `logger.warning(msg)`.
    #[track_caller]
    pub fn warning(&self, msg: impl AsRef<str>) {
        self.emit(
            LogLevel::WARNING,
            msg.as_ref(),
            Map::new(),
            Location::caller(),
        );
    }

    /// `logger.error(msg)`.
    #[track_caller]
    pub fn error(&self, msg: impl AsRef<str>) {
        self.emit(
            LogLevel::ERROR,
            msg.as_ref(),
            Map::new(),
            Location::caller(),
        );
    }

    /// `logger.exception(msg)`: an `ERROR` record (Rust has no active exception to append).
    #[track_caller]
    pub fn exception(&self, msg: impl AsRef<str>) {
        self.emit(
            LogLevel::ERROR,
            msg.as_ref(),
            Map::new(),
            Location::caller(),
        );
    }

    /// `logger.critical(msg)`.
    #[track_caller]
    pub fn critical(&self, msg: impl AsRef<str>) {
        self.emit(
            LogLevel::CRITICAL,
            msg.as_ref(),
            Map::new(),
            Location::caller(),
        );
    }

    /// `logger.log(level, msg)`.
    #[track_caller]
    pub fn log(&self, level: LogLevel, msg: impl AsRef<str>) {
        self.emit(level, msg.as_ref(), Map::new(), Location::caller());
    }

    /// `logger.log(level, msg, extra=extra)`. An `extra` key that would overwrite a record
    /// attribute is the `KeyError` Python raises.
    #[track_caller]
    pub fn log_with_extra(
        &self,
        level: LogLevel,
        msg: impl AsRef<str>,
        extra: Map<String, Value>,
    ) -> Result<(), SemanticaError> {
        if let Some(key) = extra
            .keys()
            .find(|k| RECORD_ATTRIBUTES.contains(&k.as_str()))
        {
            return Err(SemanticaError::Key(format!(
                "Attempt to overwrite {} in LogRecord",
                crate::pycompat::fmt::str_repr(key)
            )));
        }
        self.emit(level, msg.as_ref(), extra, Location::caller());
        Ok(())
    }

    fn emit(&self, level: LogLevel, msg: &str, extra: Map<String, Value>, at: &Location<'_>) {
        if !self.is_enabled_for(level) {
            return;
        }
        let record = LogRecord {
            name: self.0.name.clone(),
            level,
            message: msg.to_string(),
            created: now_seconds(),
            pathname: at.file().to_string(),
            lineno: at.line(),
            thread_name: thread_name(),
            thread: thread_ident(),
            process: std::process::id(),
            extra,
        };
        self.call_handlers(&record);
    }

    /// `logger.handle(record)` without the level check: run the record through this logger's
    /// handlers and its ancestors' while `propagate` holds.
    pub fn call_handlers(&self, record: &LogRecord) {
        let mut found = 0usize;
        let mut current = Some(self.clone());
        while let Some(logger) = current {
            let (handlers, propagate) = {
                let state = logger.state();
                (state.handlers.clone(), state.propagate)
            };
            for handler in handlers {
                found += 1;
                if record.level >= handler.level() {
                    handler.handle(record);
                }
            }
            current = if propagate { logger.parent() } else { None };
        }
        if found == 0 && record.level >= LogLevel::WARNING {
            // logging.lastResort
            eprintln!("{}", record.message);
        }
    }
}

/// `setup_logging(config=None, **kwargs)`: configure the `semantica` logger.
///
/// `config` defaults to `DEFAULT_CONFIG["logging"]`; `kwargs` override it. Keys: `level`
/// (name or integer, default `INFO`), `format`, `file` (default `semantica.log`; empty or
/// null disables the file handler), `console` (default true: a stdout handler), `rotation`
/// (default true: a rotating file handler), `max_bytes` (10 MiB) and `backup_count` (5).
/// Existing handlers are removed and propagation to the root logger is turned off.
pub fn setup_logging(
    config: Option<&Map<String, Value>>,
    kwargs: &Map<String, Value>,
) -> Result<Logger, SemanticaError> {
    let mut merged = match config {
        Some(c) => c.clone(),
        None => DEFAULT_CONFIG
            .get("logging")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
    };
    for (key, value) in kwargs {
        merged.insert(key.clone(), value.clone());
    }
    let level = match merged.get("level") {
        Some(value) => LogLevel::from_config(value)?,
        None => LogLevel::INFO,
    };
    let format = merged.get("format").cloned().unwrap_or_else(|| {
        Value::String("%(asctime)s - %(name)s - %(levelname)s - %(message)s".into())
    });
    let log_file = merged
        .get("file")
        .cloned()
        .unwrap_or_else(|| Value::String("semantica.log".into()));
    let console_enabled = merged.get("console").map(truthy).unwrap_or(true);
    let rotation_enabled = merged.get("rotation").map(truthy).unwrap_or(true);
    let int_option = |key: &str, default: i64| -> Result<i64, SemanticaError> {
        match merged.get(key) {
            None => Ok(default),
            Some(value) => match PyNum::of(value) {
                Some(PyNum::Int(i)) => Ok(i as i64),
                _ => Err(SemanticaError::Type(format!(
                    "'{key}' must be an integer, not {}",
                    type_name(value)
                ))),
            },
        }
    };
    let max_bytes = int_option("max_bytes", 10_485_760)?;
    let backup_count = int_option("backup_count", 5)?;

    let root = Logger::get("semantica");
    root.set_level(level);
    root.clear_handlers();

    let formatter = match &format {
        Value::Null => Formatter::new(None, None)?,
        Value::String(fmt) => Formatter::new(Some(fmt), None)?,
        other => {
            return Err(SemanticaError::Type(format!(
                "expected string or bytes-like object, got '{}'",
                type_name(other)
            )));
        }
    };

    if console_enabled {
        let console = Handler::stdout();
        console.set_level(level);
        console.set_formatter(formatter.clone());
        root.add_handler(console);
    }

    if truthy(&log_file) {
        let path = match &log_file {
            Value::String(path) => PathBuf::from(path),
            other => {
                return Err(SemanticaError::Type(format!(
                    "expected str, bytes or os.PathLike object, not {}",
                    type_name(other)
                )));
            }
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .map_err(|e| SemanticaError::from_io(&e, Some(parent)))?;
        }
        let file_handler = if rotation_enabled {
            Handler::rotating_file(&path, max_bytes.max(0) as u64, backup_count.max(0) as u32)?
        } else {
            Handler::file(&path)?
        };
        file_handler.set_level(level);
        file_handler.set_formatter(formatter);
        root.add_handler(file_handler);
    }

    root.set_propagate(false);
    Ok(root)
}

/// `get_logger(name)`: the logger named `semantica.{name}`.
pub fn get_logger(name: &str) -> Logger {
    Logger::get(&format!("semantica.{name}"))
}

/// Keyword options of `get_logger`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetLoggerOptions {
    /// `propagate`: set the logger's propagation flag.
    pub propagate: Option<bool>,
    /// `format`: add a stdout handler with this format (each call adds one, as in Python).
    pub format: Option<String>,
}

/// `get_logger(name, level=None, **options)` with its optional arguments.
pub fn get_logger_with(
    name: &str,
    level: Option<LogLevel>,
    options: &GetLoggerOptions,
) -> Result<Logger, SemanticaError> {
    let logger = get_logger(name);
    if let Some(level) = level {
        logger.set_level(level);
    }
    if let Some(propagate) = options.propagate {
        logger.set_propagate(propagate);
    }
    if let Some(format) = &options.format {
        let handler = Handler::stdout();
        handler.set_formatter(Formatter::new(Some(format), None)?);
        logger.add_handler(handler);
    }
    Ok(logger)
}

/// `log_performance(func_name, execution_time, **metrics)` on the `semantica.performance`
/// logger: `Function: f | Execution Time: 0.1235s | Memory: 1.00MB | ...`.
///
/// `metrics` may hold `memory_usage` (bytes), `input_size`, `output_size`, `cache_hit` and
/// `level` (an integer, default `INFO`); any other metrics travel as the record's `metrics`
/// extra. Fails as Python does for a non-integer `level` or a non-numeric `memory_usage`.
pub fn log_performance(
    func_name: &str,
    execution_time: f64,
    metrics: &Map<String, Value>,
) -> Result<(), SemanticaError> {
    let logger = get_logger("performance");
    let mut metrics = metrics.clone();
    let level = match metrics.shift_remove("level") {
        Some(value) => LogLevel::from_log_argument(&value)?,
        None => LogLevel::INFO,
    };
    let mut parts = vec![
        format!("Function: {func_name}"),
        format!("Execution Time: {}s", format_fixed(execution_time, 4)),
    ];
    if let Some(memory) = metrics.get("memory_usage") {
        let bytes = match memory {
            Value::String(_) | Value::Null | Value::Array(_) | Value::Object(_) => {
                return Err(SemanticaError::Type(format!(
                    "unsupported operand type(s) for /: '{}' and 'int'",
                    type_name(memory)
                )));
            }
            other => PyNum::of(other).map(PyNum::to_f64).unwrap_or(f64::NAN),
        };
        parts.push(format!(
            "Memory: {}MB",
            format_fixed(bytes / (1024.0 * 1024.0), 2)
        ));
    }
    if let Some(size) = metrics.get("input_size") {
        parts.push(format!("Input Size: {}", py_str(size)));
    }
    if let Some(size) = metrics.get("output_size") {
        parts.push(format!("Output Size: {}", py_str(size)));
    }
    if let Some(hit) = metrics.get("cache_hit") {
        parts.push(format!(
            "Cache: {}",
            if truthy(hit) { "HIT" } else { "MISS" }
        ));
    }
    let message = parts.join(" | ");
    let extra_metrics: Map<String, Value> = metrics
        .into_iter()
        .filter(|(k, _)| {
            ![
                "memory_usage",
                "input_size",
                "output_size",
                "cache_hit",
                "level",
            ]
            .contains(&k.as_str())
        })
        .collect();
    if extra_metrics.is_empty() {
        logger.log(level, &message);
        Ok(())
    } else {
        let mut extra = Map::new();
        extra.insert("metrics".to_string(), Value::Object(extra_metrics));
        logger.log_with_extra(level, &message, extra)
    }
}

/// `log_error(error, context=None, **details)` on the `semantica.error` logger:
/// `ErrorType: message`, with the context, details, traceback text and error code as record
/// extras. Details `level` (integer, default `ERROR`), `include_traceback` (default true) and
/// `escalate` (default false: when true, a `CRITICAL` `ESCALATED: ...` record follows) are
/// consumed; the rest travel as `details`.
pub fn log_error(
    error: &SemanticaError,
    context: Option<&Map<String, Value>>,
    details: &Map<String, Value>,
) -> Result<(), SemanticaError> {
    let logger = get_logger("error");
    let mut details = details.clone();
    let level = match details.shift_remove("level") {
        Some(value) => LogLevel::from_log_argument(&value)?,
        None => LogLevel::ERROR,
    };
    let include_traceback = details
        .shift_remove("include_traceback")
        .map(|v| truthy(&v))
        .unwrap_or(true);
    let escalate = details
        .shift_remove("escalate")
        .map(|v| truthy(&v))
        .unwrap_or(false);

    let error_type = error.type_name();
    let error_message = error.to_string();
    let message = format!("{error_type}: {error_message}");
    let mut log_data = Map::new();
    log_data.insert("error_type".to_string(), Value::String(error_type));
    log_data.insert("error_message".to_string(), Value::String(error_message));
    log_data.insert(
        "context".to_string(),
        Value::Object(context.cloned().unwrap_or_default()),
    );
    log_data.insert("details".to_string(), Value::Object(details));
    if include_traceback {
        log_data.insert(
            "traceback".to_string(),
            Value::String(error.traceback_text()),
        );
    }
    if let Some(code) = error.error_code() {
        log_data.insert("error_code".to_string(), Value::String(code.to_string()));
    }
    logger.log_with_extra(level, &message, log_data.clone())?;
    if escalate {
        logger.log_with_extra(
            LogLevel::CRITICAL,
            format!("ESCALATED: {message}"),
            log_data,
        )?;
    }
    Ok(())
}

/// Keyword options of [`log_data_quality`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DataQualityOptions {
    /// `level` (default `INFO`).
    pub level: LogLevel,
    /// `threshold` (default 0.7).
    pub threshold: f64,
    /// `alert_on_threshold` (default true): warn when the score is below the threshold.
    pub alert_on_threshold: bool,
    /// `generate_report` (default false): log a `Quality Report` record with a report extra.
    pub generate_report: bool,
}

impl Default for DataQualityOptions {
    fn default() -> Self {
        DataQualityOptions {
            level: LogLevel::INFO,
            threshold: 0.7,
            alert_on_threshold: true,
            generate_report: false,
        }
    }
}

/// `log_data_quality(quality_metrics, **options)` on the `semantica.quality` logger:
/// `Quality Score: 0.850 | Completeness: 0.900 | ...`.
///
/// The score is `quality_metrics["score"]` (or `overall_score`); a missing or non-numeric
/// score or metric fails with the `TypeError`/`ValueError` Python's formatting raises.
pub fn log_data_quality(
    quality_metrics: &Map<String, Value>,
    options: &DataQualityOptions,
) -> Result<(), SemanticaError> {
    let logger = get_logger("quality");
    let overall = match quality_metrics.get("score") {
        Some(score) => score.clone(),
        None => quality_metrics
            .get("overall_score")
            .cloned()
            .unwrap_or(Value::Null),
    };
    let mut parts = vec![format!(
        "Quality Score: {}",
        format_value_fixed(&overall, 3)?
    )];
    for metric in [
        "completeness",
        "accuracy",
        "consistency",
        "validity",
        "timeliness",
    ] {
        if let Some(value) = quality_metrics.get(metric) {
            let mut name = metric.to_string();
            name[..1].make_ascii_uppercase();
            parts.push(format!("{name}: {}", format_value_fixed(value, 3)?));
        }
    }
    let message = parts.join(" | ");
    let mut extra = Map::new();
    extra.insert(
        "quality_metrics".to_string(),
        Value::Object(quality_metrics.clone()),
    );
    logger.log_with_extra(options.level, &message, extra)?;

    let threshold = Value::from(options.threshold);
    let score = PyNum::of(&overall);
    if options.alert_on_threshold
        && let Some(score) = score
        && score.to_f64() < options.threshold
    {
        let mut extra = Map::new();
        extra.insert(
            "quality_metrics".to_string(),
            Value::Object(quality_metrics.clone()),
        );
        extra.insert("threshold".to_string(), threshold.clone());
        logger.log_with_extra(
            LogLevel::WARNING,
            format!(
                "Quality score {} below threshold {}",
                format_value_fixed(&overall, 3)?,
                py_str(&threshold)
            ),
            extra,
        )?;
    }

    if options.generate_report {
        let passed = truthy(&overall)
            && score
                .map(|s| s.to_f64() >= options.threshold)
                .unwrap_or(false);
        let mut report = Map::new();
        report.insert(
            "timestamp".to_string(),
            Value::String(PyDateTime::now(false).isoformat('T')),
        );
        report.insert(
            "quality_metrics".to_string(),
            Value::Object(quality_metrics.clone()),
        );
        report.insert("threshold".to_string(), threshold);
        report.insert(
            "status".to_string(),
            Value::String(if passed { "PASS" } else { "FAIL" }.to_string()),
        );
        let mut extra = Map::new();
        extra.insert("report".to_string(), Value::Object(report));
        logger.log_with_extra(LogLevel::INFO, "Quality Report", extra)?;
    }
    Ok(())
}

/// The `@log_execution_time` decorator: run `func`, then log its execution time through
/// [`log_performance`] with `success` (and `error` on failure) metrics, and return its result.
pub fn log_execution_time<T, E: fmt::Display>(
    func_name: &str,
    func: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let _logger = get_logger("performance");
    let start = Instant::now();
    let result = func();
    let elapsed = start.elapsed().as_secs_f64();
    let mut metrics = Map::new();
    match &result {
        Ok(_) => {
            metrics.insert("success".to_string(), Value::Bool(true));
        }
        Err(err) => {
            metrics.insert("success".to_string(), Value::Bool(false));
            metrics.insert("error".to_string(), Value::String(err.to_string()));
        }
    }
    // Only a `level` metric can make log_performance fail, and none is passed here.
    let _ = log_performance(func_name, elapsed, &metrics);
    result
}
