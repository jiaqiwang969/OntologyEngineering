//! Helper utilities (`semantica/utils/helpers.py`): data formatting, text cleaning, entity
//! normalization, hashing, file helpers, timestamps, dict manipulation, retries, optional
//! imports, path-distance bands and the graph-payload normalizer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::exceptions::{OsError, ProcessingError, SemanticaError, ValidationError};
use crate::logging::get_logger;
use crate::pycompat::fmt::{SPACE_CLASS, slice_chars, strip, strip_chars, truthy, type_name};
use crate::pycompat::json::{DumpsOptions, dumps};
use crate::pycompat::time::{PyDateTime, strptime};
use crate::pycompat::yaml::dump_default as yaml_dump;

/// `format_data(data, format_type="json")`: `json` (indented, non-ASCII kept) or `yaml`
/// (PyYAML block style); any other format is a `ValueError`.
pub fn format_data(data: &Value, format_type: Option<&str>) -> Result<String, SemanticaError> {
    match format_type.unwrap_or("json") {
        "json" => Ok(dumps(
            data,
            &DumpsOptions {
                indent: Some(2),
                ensure_ascii: false,
                ..Default::default()
            },
        )),
        "yaml" => Ok(yaml_dump(data)),
        other => Err(SemanticaError::Value(format!(
            "Unsupported format type: {other}"
        ))),
    }
}

static WHITESPACE_RUN: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!("[{SPACE_CLASS}]+")).expect("whitespace run"));
static BLANK_LINES: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!("\n[{SPACE_CLASS}]*\n")).expect("blank lines"));
static ZERO_WIDTH: Lazy<Regex> =
    Lazy::new(|| Regex::new("[\u{200b}-\u{200d}\u{feff}]").expect("zero-width"));

/// `clean_text(text, preserve_whitespace=False)`: strip, collapse whitespace runs to one
/// space (unless `preserve_whitespace`) and remove zero-width characters.
pub fn clean_text(text: &str, preserve_whitespace: Option<bool>) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut text = strip(text).to_string();
    if !preserve_whitespace.unwrap_or(false) {
        text = WHITESPACE_RUN.replace_all(&text, " ").into_owned();
        text = BLANK_LINES.replace_all(&text, "\n").into_owned();
    }
    ZERO_WIDTH.replace_all(&text, "").into_owned()
}

/// `a or b` over optional values (Python truthiness).
fn py_or(a: Option<&Value>, b: Option<&Value>) -> Value {
    match a {
        Some(v) if truthy(v) => v.clone(),
        _ => b.cloned().unwrap_or(Value::Null),
    }
}

/// `normalize_entities(entities)`: map alternative field names (`entity_id`, `label`/`name`,
/// `entity_type`, `start_offset`, `end_offset`) onto `id`, `text`, `type`, `start`, `end`;
/// keep `metadata` and `relations` when present; `confidence` defaults to 1.0.
pub fn normalize_entities(entities: &[Map<String, Value>]) -> Vec<Map<String, Value>> {
    entities
        .iter()
        .map(|entity| {
            let get = |k: &str| entity.get(k);
            let mut out = Map::new();
            out.insert("id".into(), py_or(get("id"), get("entity_id")));
            let text = match get("text") {
                Some(v) if truthy(v) => v.clone(),
                _ => py_or(get("label"), get("name")),
            };
            out.insert("text".into(), text);
            out.insert("type".into(), py_or(get("type"), get("entity_type")));
            out.insert(
                "confidence".into(),
                get("confidence").cloned().unwrap_or(Value::from(1.0)),
            );
            out.insert("start".into(), py_or(get("start"), get("start_offset")));
            out.insert("end".into(), py_or(get("end"), get("end_offset")));
            if let Some(metadata) = get("metadata") {
                out.insert("metadata".into(), metadata.clone());
            }
            if let Some(relations) = get("relations") {
                out.insert("relations".into(), relations.clone());
            }
            out
        })
        .collect()
}

/// Input of [`hash_data`]: a string, bytes or a dict.
#[derive(Debug, Clone, Copy)]
pub enum HashInput<'a> {
    /// A string (hashed as UTF-8).
    Str(&'a str),
    /// Raw bytes.
    Bytes(&'a [u8]),
    /// A dict (hashed as `json.dumps(data, sort_keys=True)`).
    Dict(&'a Map<String, Value>),
}

impl<'a> From<&'a str> for HashInput<'a> {
    fn from(value: &'a str) -> Self {
        HashInput::Str(value)
    }
}

impl<'a> From<&'a String> for HashInput<'a> {
    fn from(value: &'a String) -> Self {
        HashInput::Str(value)
    }
}

impl<'a> From<&'a [u8]> for HashInput<'a> {
    fn from(value: &'a [u8]) -> Self {
        HashInput::Bytes(value)
    }
}

impl<'a> From<&'a Vec<u8>> for HashInput<'a> {
    fn from(value: &'a Vec<u8>) -> Self {
        HashInput::Bytes(value)
    }
}

impl<'a> From<&'a Map<String, Value>> for HashInput<'a> {
    fn from(value: &'a Map<String, Value>) -> Self {
        HashInput::Dict(value)
    }
}

/// `hash_data(data)`: SHA-256 hex digest; dicts are hashed as `json.dumps(data,
/// sort_keys=True)` so the digest matches the original's.
pub fn hash_data<'a>(data: impl Into<HashInput<'a>>) -> String {
    let bytes: Vec<u8> = match data.into() {
        HashInput::Str(s) => s.as_bytes().to_vec(),
        HashInput::Bytes(b) => b.to_vec(),
        HashInput::Dict(map) => dumps(
            &Value::Object(map.clone()),
            &DumpsOptions {
                sort_keys: true,
                ..Default::default()
            },
        )
        .into_bytes(),
    };
    hex::encode(Sha256::digest(&bytes))
}

static UNSAFE_FILENAME_CHARS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"[<>:"/\\|?*]"#).expect("unsafe chars"));

/// `os.path.splitext` (POSIX) over code points.
fn splitext(p: &[char]) -> (Vec<char>, Vec<char>) {
    let sep_index = p
        .iter()
        .rposition(|c| *c == '/')
        .map(|i| i as isize)
        .unwrap_or(-1);
    let dot_index = p
        .iter()
        .rposition(|c| *c == '.')
        .map(|i| i as isize)
        .unwrap_or(-1);
    if dot_index > sep_index {
        let mut filename_index = sep_index + 1;
        while filename_index < dot_index {
            if p[filename_index as usize] != '.' {
                let d = dot_index as usize;
                return (p[..d].to_vec(), p[d..].to_vec());
            }
            filename_index += 1;
        }
    }
    (p.to_vec(), Vec::new())
}

/// `safe_filename(filename, max_length=255)`: remove `<>:"/\|?*`, replace spaces with `_`,
/// strip leading/trailing dots and spaces and truncate (keeping the extension); an empty
/// result becomes `unnamed`.
pub fn safe_filename(filename: &str, max_length: Option<usize>) -> String {
    let max_length = max_length.unwrap_or(255) as i64;
    let cleaned = UNSAFE_FILENAME_CHARS
        .replace_all(filename, "")
        .replace(' ', "_");
    let mut filename = strip_chars(&cleaned, ". ").to_string();
    if filename.chars().count() as i64 > max_length {
        let chars: Vec<char> = filename.chars().collect();
        let (name, ext) = splitext(&chars);
        let max_name_length = max_length - ext.len() as i64;
        let name: String = name.iter().collect();
        let ext: String = ext.iter().collect();
        filename = format!("{}{ext}", slice_chars(&name, None, Some(max_name_length)));
    }
    if filename.is_empty() {
        "unnamed".to_string()
    } else {
        filename
    }
}

/// `str(Path(path))` on POSIX: collapse repeated separators, drop `.` components and a
/// trailing separator.
pub fn pure_path_str(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let root = if path.starts_with("//") && !path.starts_with("///") {
        "//"
    } else if path.starts_with('/') {
        "/"
    } else {
        ""
    };
    let parts: Vec<&str> = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    let joined = parts.join("/");
    if root.is_empty() && joined.is_empty() {
        ".".to_string()
    } else {
        format!("{root}{joined}")
    }
}

fn path_text(path: &Path) -> String {
    pure_path_str(&path.to_string_lossy())
}

fn io_error(err: &std::io::Error, path: &Path) -> SemanticaError {
    SemanticaError::Os(OsError::from_io(err, Some(&path_text(path))))
}

/// `ensure_directory(path)`: create the directory (and parents) if missing and return it.
pub fn ensure_directory(path: impl AsRef<Path>) -> Result<PathBuf, SemanticaError> {
    let path = path.as_ref();
    let dir = if path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        path.to_path_buf()
    };
    std::fs::create_dir_all(&dir).map_err(|e| io_error(&e, &dir))?;
    Ok(dir)
}

/// `read_json_file(filepath)`: parse a JSON file. A missing file is a `FileNotFoundError`
/// (`File not found: <path>`), invalid JSON a `JSONDecodeError`.
pub fn read_json_file(filepath: impl AsRef<Path>) -> Result<Value, SemanticaError> {
    let path = filepath.as_ref();
    if !path.exists() {
        return Err(SemanticaError::Os(OsError::new(
            "FileNotFoundError",
            format!("File not found: {}", path_text(path)),
        )));
    }
    let bytes = std::fs::read(path).map_err(|e| io_error(&e, path))?;
    let text = String::from_utf8(bytes).map_err(|e| {
        let pos = e.utf8_error().valid_up_to();
        let byte = e.as_bytes()[pos];
        SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "UnicodeDecodeError".to_string(),
            message: format!(
                "'utf-8' codec can't decode byte {byte:#04x} in position {pos}: invalid start byte"
            ),
        }
    })?;
    serde_json::from_str(&text).map_err(|e| SemanticaError::JsonDecode(e.to_string()))
}

/// `write_json_file(data, filepath, indent=2, ensure_ascii=False)`: write JSON (creating
/// parent directories) exactly as `json.dump` formats it.
pub fn write_json_file(
    data: &Value,
    filepath: impl AsRef<Path>,
    indent: Option<usize>,
    ensure_ascii: Option<bool>,
) -> Result<(), SemanticaError> {
    let path = filepath.as_ref();
    let parent = path.parent().unwrap_or(Path::new(""));
    ensure_directory(parent)?;
    let text = dumps(
        data,
        &DumpsOptions {
            indent: Some(indent.unwrap_or(2)),
            ensure_ascii: ensure_ascii.unwrap_or(false),
            ..Default::default()
        },
    );
    std::fs::write(path, text).map_err(|e| io_error(&e, path))
}

/// `get_file_size(filepath)`: size in bytes (`FileNotFoundError` when missing).
pub fn get_file_size(filepath: impl AsRef<Path>) -> Result<u64, SemanticaError> {
    let path = filepath.as_ref();
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| io_error(&e, path))
}

/// The `timestamp` argument of [`format_timestamp`]: a datetime or seconds since the epoch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Timestamp {
    /// A `datetime`.
    DateTime(PyDateTime),
    /// An `int`/`float` POSIX timestamp.
    Epoch(f64),
}

impl From<PyDateTime> for Timestamp {
    fn from(value: PyDateTime) -> Self {
        Timestamp::DateTime(value)
    }
}

impl From<f64> for Timestamp {
    fn from(value: f64) -> Self {
        Timestamp::Epoch(value)
    }
}

impl From<i64> for Timestamp {
    fn from(value: i64) -> Self {
        Timestamp::Epoch(value as f64)
    }
}

/// `format_timestamp(timestamp=None, format_str="%Y-%m-%d %H:%M:%S", timezone_aware=True)`.
///
/// `None` formats the current time; epoch seconds are converted in UTC when
/// `timezone_aware`, else in local time; datetimes are formatted as given.
pub fn format_timestamp(
    timestamp: Option<Timestamp>,
    format_str: Option<&str>,
    timezone_aware: Option<bool>,
) -> Result<String, SemanticaError> {
    let format_str = format_str.unwrap_or("%Y-%m-%d %H:%M:%S");
    let aware = timezone_aware.unwrap_or(true);
    let dt = match timestamp {
        None => PyDateTime::now(aware),
        Some(Timestamp::Epoch(ts)) => PyDateTime::from_timestamp(ts, aware)?,
        Some(Timestamp::DateTime(dt)) => dt,
    };
    Ok(dt.strftime(format_str))
}

const TIMESTAMP_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%dT%H:%M:%S%z",
    "%Y-%m-%d",
    "%d/%m/%Y",
    "%m/%d/%Y",
];

/// `parse_timestamp(timestamp_str, format_str=None)`: `strptime` with the given format, or
/// the first of the common formats that parses (`ValueError` when none does).
pub fn parse_timestamp(
    timestamp_str: &str,
    format_str: Option<&str>,
) -> Result<PyDateTime, SemanticaError> {
    if let Some(format) = format_str.filter(|f| !f.is_empty()) {
        return strptime(timestamp_str, format);
    }
    for format in TIMESTAMP_FORMATS {
        if let Ok(dt) = strptime(timestamp_str, format) {
            return Ok(dt);
        }
    }
    Err(SemanticaError::Value(format!(
        "Could not parse timestamp: {timestamp_str}"
    )))
}

/// `merge_dicts(*dicts, deep=True)`: later dicts win; with `deep`, nested dicts are merged.
pub fn merge_dicts(dicts: &[&Map<String, Value>], deep: Option<bool>) -> Map<String, Value> {
    let deep = deep.unwrap_or(true);
    let mut result = Map::new();
    for dict in dicts {
        for (key, value) in dict.iter() {
            let merged = match (result.get(key), value) {
                (Some(Value::Object(existing)), Value::Object(incoming)) if deep => {
                    Value::Object(merge_dicts(&[existing, incoming], Some(true)))
                }
                _ => value.clone(),
            };
            result.insert(key.clone(), merged);
        }
    }
    result
}

/// `chunk_list(items, chunk_size)` (a zero size is the `ValueError` `range` raises).
pub fn chunk_list<T: Clone>(items: &[T], chunk_size: usize) -> Result<Vec<Vec<T>>, SemanticaError> {
    if chunk_size == 0 {
        return Err(SemanticaError::Value(
            "range() arg 3 must not be zero".to_string(),
        ));
    }
    Ok(items.chunks(chunk_size).map(<[T]>::to_vec).collect())
}

/// `flatten_dict(d, parent_key="", sep=".")`; two paths producing the same key are a
/// `ValueError` (`Key collision while flattening dictionary: <key>`).
pub fn flatten_dict(
    d: &Map<String, Value>,
    parent_key: Option<&str>,
    sep: Option<&str>,
) -> Result<Map<String, Value>, SemanticaError> {
    let parent_key = parent_key.unwrap_or("");
    let sep = sep.unwrap_or(".");
    let mut result = Map::new();
    for (k, v) in d {
        let new_key = if parent_key.is_empty() {
            k.clone()
        } else {
            format!("{parent_key}{sep}{k}")
        };
        if let Value::Object(nested) = v {
            for (key, value) in flatten_dict(nested, Some(&new_key), Some(sep))? {
                if result.contains_key(&key) {
                    return Err(SemanticaError::Value(format!(
                        "Key collision while flattening dictionary: {key}"
                    )));
                }
                result.insert(key, value);
            }
        } else {
            if result.contains_key(&new_key) {
                return Err(SemanticaError::Value(format!(
                    "Key collision while flattening dictionary: {new_key}"
                )));
            }
            result.insert(new_key, v.clone());
        }
    }
    Ok(result)
}

fn split_path<'a>(key_path: &'a str, sep: &str) -> Result<Vec<&'a str>, SemanticaError> {
    if sep.is_empty() {
        return Err(SemanticaError::Value("empty separator".to_string()));
    }
    Ok(key_path.split(sep).collect())
}

/// `get_nested_value(d, key_path, default=None, sep=".")`: the value at a dotted path, or
/// `default` (`None` -> `null`) when any step is missing or not a dict.
pub fn get_nested_value(
    d: &Map<String, Value>,
    key_path: &str,
    default: Option<Value>,
    sep: Option<&str>,
) -> Result<Value, SemanticaError> {
    let keys = split_path(key_path, sep.unwrap_or("."))?;
    let default = default.unwrap_or(Value::Null);
    let mut current = d;
    let (last, parents) = keys.split_last().expect("split yields at least one part");
    for key in parents {
        match current.get(*key) {
            Some(Value::Object(next)) => current = next,
            _ => return Ok(default),
        }
    }
    Ok(current.get(*last).cloned().unwrap_or(default))
}

fn descend_error(target: &Value, key: &str) -> SemanticaError {
    match target {
        Value::String(s) if s.contains(key) => {
            SemanticaError::Type("string indices must be integers, not 'str'".to_string())
        }
        Value::String(_) => {
            SemanticaError::Type("'str' object does not support item assignment".to_string())
        }
        Value::Array(_) => {
            SemanticaError::Type("list indices must be integers or slices, not str".to_string())
        }
        other => SemanticaError::Type(format!(
            "argument of type '{}' is not iterable",
            type_name(other)
        )),
    }
}

fn assign_error(target: &Value) -> SemanticaError {
    match target {
        Value::Array(_) => {
            SemanticaError::Type("list indices must be integers or slices, not str".to_string())
        }
        other => SemanticaError::Type(format!(
            "'{}' object does not support item assignment",
            type_name(other)
        )),
    }
}

/// `set_nested_value(d, key_path, value, sep=".")`: set a value at a dotted path, creating
/// intermediate dicts. Walking through a non-dict value fails with the `TypeError` Python
/// raises (after the intermediate dicts created so far, as in Python).
pub fn set_nested_value(
    d: &mut Map<String, Value>,
    key_path: &str,
    value: Value,
    sep: Option<&str>,
) -> Result<(), SemanticaError> {
    let keys = split_path(key_path, sep.unwrap_or("."))?;
    let (last, parents) = keys.split_last().expect("split yields at least one part");
    let mut current = d;
    for (i, key) in parents.iter().enumerate() {
        if !current.contains_key(*key) {
            current.insert((*key).to_string(), Value::Object(Map::new()));
        }
        match current.get_mut(*key).expect("just ensured") {
            Value::Object(next) => current = next,
            other => {
                return Err(match parents.get(i + 1) {
                    Some(next_key) => descend_error(other, next_key),
                    None => assign_error(other),
                });
            }
        }
    }
    current.insert((*last).to_string(), value);
    Ok(())
}

/// A module handle returned by [`safe_import`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Module {
    name: Arc<str>,
}

impl Module {
    /// A module handle with this name (not registered; e.g. a `default` for [`safe_import`]).
    pub fn new(name: &str) -> Self {
        Module {
            name: Arc::from(name),
        }
    }

    /// The module's dotted name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The `semantica.utils` package modules, which this crate provides.
const UTILS_MODULES: &[&str] = &[
    "semantica",
    "semantica.utils",
    "semantica.utils.constants",
    "semantica.utils.exceptions",
    "semantica.utils.helpers",
    "semantica.utils.logging",
    "semantica.utils.progress_tracker",
    "semantica.utils.skos",
    "semantica.utils.types",
    "semantica.utils.validators",
];

/// Python 3.11 standard-library modules importable on the reference platform (macOS).
const STDLIB_MODULES: &[&str] = &[
    "__future__",
    "_abc",
    "_aix_support",
    "_ast",
    "_asyncio",
    "_bisect",
    "_blake2",
    "_bootsubprocess",
    "_bz2",
    "_codecs",
    "_codecs_cn",
    "_codecs_hk",
    "_codecs_iso2022",
    "_codecs_jp",
    "_codecs_kr",
    "_codecs_tw",
    "_collections",
    "_collections_abc",
    "_compat_pickle",
    "_compression",
    "_contextvars",
    "_crypt",
    "_csv",
    "_ctypes",
    "_curses",
    "_curses_panel",
    "_datetime",
    "_dbm",
    "_decimal",
    "_elementtree",
    "_frozen_importlib",
    "_frozen_importlib_external",
    "_functools",
    "_hashlib",
    "_heapq",
    "_imp",
    "_io",
    "_json",
    "_locale",
    "_lsprof",
    "_lzma",
    "_markupbase",
    "_md5",
    "_multibytecodec",
    "_multiprocessing",
    "_opcode",
    "_operator",
    "_osx_support",
    "_pickle",
    "_posixshmem",
    "_posixsubprocess",
    "_py_abc",
    "_pydecimal",
    "_pyio",
    "_queue",
    "_random",
    "_scproxy",
    "_sha1",
    "_sha256",
    "_sha3",
    "_sha512",
    "_signal",
    "_sitebuiltins",
    "_socket",
    "_sqlite3",
    "_sre",
    "_ssl",
    "_stat",
    "_statistics",
    "_string",
    "_strptime",
    "_struct",
    "_symtable",
    "_thread",
    "_threading_local",
    "_tkinter",
    "_tokenize",
    "_tracemalloc",
    "_typing",
    "_uuid",
    "_warnings",
    "_weakref",
    "_weakrefset",
    "_zoneinfo",
    "abc",
    "aifc",
    "antigravity",
    "argparse",
    "array",
    "ast",
    "asynchat",
    "asyncio",
    "asyncore",
    "atexit",
    "audioop",
    "base64",
    "bdb",
    "binascii",
    "bisect",
    "builtins",
    "bz2",
    "cProfile",
    "calendar",
    "cgi",
    "cgitb",
    "chunk",
    "cmath",
    "cmd",
    "code",
    "codecs",
    "codeop",
    "collections",
    "colorsys",
    "compileall",
    "concurrent",
    "configparser",
    "contextlib",
    "contextvars",
    "copy",
    "copyreg",
    "crypt",
    "csv",
    "ctypes",
    "curses",
    "dataclasses",
    "datetime",
    "dbm",
    "decimal",
    "difflib",
    "dis",
    "distutils",
    "doctest",
    "email",
    "encodings",
    "ensurepip",
    "enum",
    "errno",
    "faulthandler",
    "fcntl",
    "filecmp",
    "fileinput",
    "fnmatch",
    "fractions",
    "ftplib",
    "functools",
    "gc",
    "genericpath",
    "getopt",
    "getpass",
    "gettext",
    "glob",
    "graphlib",
    "grp",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "idlelib",
    "imaplib",
    "imghdr",
    "imp",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "keyword",
    "lib2to3",
    "linecache",
    "locale",
    "logging",
    "lzma",
    "mailbox",
    "mailcap",
    "marshal",
    "math",
    "mimetypes",
    "mmap",
    "modulefinder",
    "multiprocessing",
    "netrc",
    "nis",
    "nntplib",
    "ntpath",
    "nturl2path",
    "numbers",
    "opcode",
    "operator",
    "optparse",
    "os",
    "pathlib",
    "pdb",
    "pickle",
    "pickletools",
    "pipes",
    "pkgutil",
    "platform",
    "plistlib",
    "poplib",
    "posix",
    "posixpath",
    "pprint",
    "profile",
    "pstats",
    "pty",
    "pwd",
    "py_compile",
    "pyclbr",
    "pydoc",
    "pydoc_data",
    "pyexpat",
    "queue",
    "quopri",
    "random",
    "re",
    "readline",
    "reprlib",
    "resource",
    "rlcompleter",
    "runpy",
    "sched",
    "secrets",
    "select",
    "selectors",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "site",
    "smtpd",
    "smtplib",
    "sndhdr",
    "socket",
    "socketserver",
    "sqlite3",
    "sre_compile",
    "sre_constants",
    "sre_parse",
    "ssl",
    "stat",
    "statistics",
    "string",
    "stringprep",
    "struct",
    "subprocess",
    "sunau",
    "symtable",
    "sys",
    "sysconfig",
    "syslog",
    "tabnanny",
    "tarfile",
    "telnetlib",
    "tempfile",
    "termios",
    "textwrap",
    "this",
    "threading",
    "time",
    "timeit",
    "tkinter",
    "token",
    "tokenize",
    "tomllib",
    "trace",
    "traceback",
    "tracemalloc",
    "tty",
    "turtle",
    "turtledemo",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uu",
    "uuid",
    "venv",
    "warnings",
    "wave",
    "weakref",
    "webbrowser",
    "wsgiref",
    "xdrlib",
    "xml",
    "xmlrpc",
    "zipapp",
    "zipfile",
    "zipimport",
    "zlib",
    "zoneinfo",
];

static MODULES: Lazy<Mutex<HashMap<String, Module>>> = Lazy::new(|| {
    let map = STDLIB_MODULES
        .iter()
        .chain(UTILS_MODULES)
        .map(|name| ((*name).to_string(), Module::new(name)))
        .collect();
    Mutex::new(map)
});

/// Make `name` importable through [`safe_import`] (a crate providing the Rust counterpart of
/// an optional Python dependency registers it here). Returns the module handle.
pub fn register_module(name: &str) -> Module {
    let mut modules = MODULES.lock().unwrap_or_else(|e| e.into_inner());
    modules
        .entry(name.to_string())
        .or_insert_with(|| Module::new(name))
        .clone()
}

fn resolve_module_name(name: &str, package: Option<&str>) -> Result<String, String> {
    let level = name.chars().take_while(|c| *c == '.').count();
    if level == 0 {
        return Ok(name.to_string());
    }
    let Some(package) = package.filter(|p| !p.is_empty()) else {
        return Err(format!(
            "the 'package' argument is required to perform a relative import for {}",
            crate::pycompat::fmt::str_repr(name)
        ));
    };
    let bits: Vec<&str> = package.rsplitn(level, '.').collect();
    if bits.len() < level {
        return Err("attempted relative import beyond top-level package".to_string());
    }
    let base = bits[bits.len() - 1];
    let rest = &name[level..];
    Ok(if rest.is_empty() {
        base.to_string()
    } else {
        format!("{base}.{rest}")
    })
}

/// `safe_import(module_name, package=None, default=None, error_message=None)`:
/// `(module, true)` when the module is available, else `(default, false)`.
///
/// Rust has no dynamic imports: the importable modules are the Python standard library's
/// (always available), the `semantica.utils` modules, and those registered with
/// [`register_module`]. When `error_message` is given, a failure is logged at debug level as
/// `"<error_message>: No module named '<name>'"`. A relative name without `package` (a
/// `TypeError` in Python) is reported as unavailable.
pub fn safe_import(
    module_name: &str,
    package: Option<&str>,
    default: Option<Module>,
    error_message: Option<&str>,
) -> (Option<Module>, bool) {
    let failure = match resolve_module_name(module_name, package) {
        Ok(absolute) => {
            let modules = MODULES.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(module) = modules.get(&absolute) {
                return (Some(module.clone()), true);
            }
            let parts: Vec<&str> = absolute.split('.').collect();
            let missing = (1..=parts.len())
                .map(|n| parts[..n].join("."))
                .find(|prefix| !modules.contains_key(prefix))
                .unwrap_or(absolute.clone());
            format!("No module named '{missing}'")
        }
        Err(message) => message,
    };
    if let Some(error_message) = error_message.filter(|m| !m.is_empty()) {
        get_logger("utils.helpers").debug(format!("{error_message}: {failure}"));
    }
    (default, false)
}

/// The decorator `retry_on_error(...)` returns: call [`RetryOnError::call`] to run a
/// fallible operation with retries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryOnError {
    /// `max_retries` (default 3): retries after the first attempt.
    pub max_retries: u32,
    /// `delay` (default 1.0 s) before the first retry.
    pub delay: f64,
    /// `backoff_factor` (default 2.0) applied to the delay after each retry.
    pub backoff_factor: f64,
}

impl Default for RetryOnError {
    fn default() -> Self {
        RetryOnError {
            max_retries: 3,
            delay: 1.0,
            backoff_factor: 2.0,
        }
    }
}

impl RetryOnError {
    /// Run `func`, retrying on any error (`exceptions=(Exception,)`); the last error is
    /// returned when every attempt fails.
    pub fn call<T, E>(&self, func: impl FnMut() -> Result<T, E>) -> Result<T, E> {
        self.call_if(|_| true, func)
    }

    /// Run `func`, retrying only errors for which `retry_on` holds (the `exceptions` tuple);
    /// other errors are returned at once.
    pub fn call_if<T, E>(
        &self,
        retry_on: impl Fn(&E) -> bool,
        mut func: impl FnMut() -> Result<T, E>,
    ) -> Result<T, E> {
        let mut current_delay = self.delay;
        let mut attempt = 0;
        loop {
            match func() {
                Ok(value) => return Ok(value),
                Err(err) if retry_on(&err) && attempt < self.max_retries => {
                    if current_delay.is_finite() && current_delay > 0.0 {
                        std::thread::sleep(Duration::from_secs_f64(current_delay));
                    }
                    current_delay *= self.backoff_factor;
                    attempt += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

/// `retry_on_error(max_retries=3, delay=1.0, backoff_factor=2.0)`: a retry policy (the
/// decorator); use [`RetryOnError::call`] / [`RetryOnError::call_if`] to apply it.
pub fn retry_on_error(
    max_retries: Option<u32>,
    delay: Option<f64>,
    backoff_factor: Option<f64>,
) -> RetryOnError {
    let defaults = RetryOnError::default();
    RetryOnError {
        max_retries: max_retries.unwrap_or(defaults.max_retries),
        delay: delay.unwrap_or(defaults.delay),
        backoff_factor: backoff_factor.unwrap_or(defaults.backoff_factor),
    }
}

/// `classify_path_distance(hop_count)`: `direct` (0-1 hops), `near` (2-3), `mid-range` (4-6)
/// or `distant` (7+).
pub fn classify_path_distance(hop_count: i64) -> &'static str {
    if hop_count <= 1 {
        "direct"
    } else if hop_count <= 3 {
        "near"
    } else if hop_count <= 6 {
        "mid-range"
    } else {
        "distant"
    }
}

const ENTITY_KEYS: &[&str] = &["entities", "nodes"];
const RELATIONSHIP_KEYS: &[&str] = &["relationships", "edges"];
const TRIPLET_KEYS: &[&str] = &["triplets"];
/// Keys that travel alongside the collections without being records.
const CONTEXT_KEYS: &[&str] = &["metadata", "statistics", "count"];

fn quoted(keys: &[&str], sep: &str) -> String {
    keys.iter()
        .map(|k| format!("'{k}'"))
        .collect::<Vec<_>>()
        .join(sep)
}

fn require_recognized_keys(
    payload: &Map<String, Value>,
    recognized: &[&str],
    what: &str,
) -> Result<(), SemanticaError> {
    if payload.is_empty() || recognized.iter().any(|k| payload.contains_key(*k)) {
        return Ok(());
    }
    let mut supplied: Vec<&str> = payload.keys().map(String::as_str).collect();
    supplied.sort();
    Err(ValidationError::new(format!(
        "{what} has no recognized key. Supplied: {}. Expected at least one of: {}.",
        quoted(&supplied, ", "),
        quoted(recognized, ", ")
    ))
    .into())
}

fn require_nothing_dropped(
    payload: &Map<String, Value>,
    recognized: &[&str],
    resolved: &NormalizedGraphPayload,
    what: &str,
) -> Result<(), SemanticaError> {
    if !resolved.entities.is_empty()
        || !resolved.relationships.is_empty()
        || !resolved.triplets.is_empty()
    {
        return Ok(());
    }
    let mut dropped: Vec<&str> = payload
        .iter()
        .filter(|(key, value)| {
            !recognized.contains(&key.as_str())
                && !CONTEXT_KEYS.contains(&key.as_str())
                && matches!(value, Value::Array(items) if !items.is_empty())
        })
        .map(|(key, _)| key.as_str())
        .collect();
    if dropped.is_empty() {
        return Ok(());
    }
    dropped.sort();
    Err(ValidationError::new(format!(
        "{what} resolved to nothing, but {} still holds records. Exporting it would drop them \
         silently. Supply the records under one of: {}.",
        quoted(&dropped, ", "),
        quoted(recognized, ", ")
    ))
    .into())
}

/// Validate one collection value and materialize it as a list of record dicts.
fn coerce_records(key: &str, value: &Value) -> Result<Vec<Map<String, Value>>, SemanticaError> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::String(_) => Err(ValidationError::new(format!(
            "Graph payload key '{key}' holds a str, not a collection of records. Iterating it \
             would yield characters, not records. Supply a list of records."
        ))
        .into()),
        Value::Object(_) => Err(ValidationError::new(format!(
            "Graph payload key '{key}' holds a mapping, not a collection of records. If it is a \
             single record, wrap it in a list; if it is keyed by ID, supply its values as a list."
        ))
        .into()),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, record)| match record {
                Value::Object(map) => Ok(map.clone()),
                other => Err(ValidationError::new(format!(
                    "Graph payload key '{key}' holds a {} at index {index}, not a record. \
                     Records must be mappings or objects with attributes.",
                    type_name(other)
                ))
                .into()),
            })
            .collect(),
        other => Err(ValidationError::new(format!(
            "Graph payload key '{key}' holds a {}, not a collection of records. Supply a list \
             of records.",
            type_name(other)
        ))
        .into()),
    }
}

/// Records as an order-independent multiset of their canonical JSON form.
fn canonical_record_multiset(records: &[Map<String, Value>]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for record in records {
        let key = dumps(
            &Value::Object(record.clone()),
            &DumpsOptions {
                sort_keys: true,
                ..Default::default()
            },
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn resolve_collection(
    payload: &Map<String, Value>,
    keys: &[&str],
) -> Result<Vec<Map<String, Value>>, SemanticaError> {
    let mut present: Vec<(&str, Vec<Map<String, Value>>)> = Vec::new();
    for key in keys {
        if let Some(value) = payload.get(*key) {
            present.push((key, coerce_records(key, value)?));
        }
    }
    let populated: Vec<&(&str, Vec<Map<String, Value>>)> =
        present.iter().filter(|(_, v)| !v.is_empty()).collect();
    if populated.len() > 1 {
        let first = canonical_record_multiset(&populated[0].1);
        if populated[1..]
            .iter()
            .any(|(_, v)| canonical_record_multiset(v) != first)
        {
            let named: Vec<String> = populated.iter().map(|(k, _)| format!("'{k}'")).collect();
            return Err(ValidationError::new(format!(
                "Graph payload carries {} with different contents; cannot determine which to \
                 export. Supply one, or make them identical.",
                named.join(" and ")
            ))
            .into());
        }
    }
    for key in keys {
        if let Some((_, records)) = present.iter().find(|(k, v)| k == key && !v.is_empty()) {
            return Ok(records.clone());
        }
    }
    Ok(Vec::new())
}

/// `_require_mapping(data, expected_keys)`: the `ProcessingError` every exporter raises for a
/// payload that is not a mapping (`Cannot export object of type 'list': expected a dict with
/// 'entities'/'relationships'.`). Exposed for the exporters of later crates.
pub fn require_mapping(data: &Value, expected_keys: &[&str]) -> Result<(), SemanticaError> {
    if data.is_object() {
        return Ok(());
    }
    Err(ProcessingError::new(format!(
        "Cannot export object of type '{}': expected a dict with {}.",
        type_name(data),
        quoted(expected_keys, "/")
    ))
    .into())
}

/// The canonical graph payload returned by [`normalize_graph_payload`]
/// (`{"entities": [...], "relationships": [...], "triplets": [...]}`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NormalizedGraphPayload {
    /// Entity records (from `entities` or `nodes`).
    pub entities: Vec<Map<String, Value>>,
    /// Relationship records (from `relationships` or `edges`).
    pub relationships: Vec<Map<String, Value>>,
    /// Triplet records.
    pub triplets: Vec<Map<String, Value>>,
}

impl NormalizedGraphPayload {
    /// The payload as the dict the original returns.
    pub fn to_value(&self) -> Value {
        let to_list = |records: &[Map<String, Value>]| {
            Value::Array(records.iter().cloned().map(Value::Object).collect())
        };
        let mut out = Map::new();
        out.insert("entities".into(), to_list(&self.entities));
        out.insert("relationships".into(), to_list(&self.relationships));
        out.insert("triplets".into(), to_list(&self.triplets));
        Value::Object(out)
    }
}

/// `normalize_graph_payload(payload)`: reduce a graph payload using `entities`/`relationships`
/// or `nodes`/`edges` (or a mix) to the canonical vocabulary.
///
/// Records must be dicts (serialize typed records to JSON objects first). Fails with a
/// `ValidationError` when the payload is not a mapping, a collection key holds something other
/// than a list of records, two spellings of one collection are non-empty and differ (order
/// ignored), a non-empty payload has no recognized key, or nothing resolved while another key
/// still holds a non-empty list.
pub fn normalize_graph_payload(payload: &Value) -> Result<NormalizedGraphPayload, SemanticaError> {
    let Value::Object(payload) = payload else {
        return Err(ValidationError::new(format!(
            "Cannot normalize graph payload of type '{}': expected a mapping.",
            type_name(payload)
        ))
        .into());
    };
    let recognized: Vec<&str> = ENTITY_KEYS
        .iter()
        .chain(RELATIONSHIP_KEYS)
        .chain(TRIPLET_KEYS)
        .copied()
        .collect();
    require_recognized_keys(payload, &recognized, "Graph payload")?;
    let resolved = NormalizedGraphPayload {
        entities: resolve_collection(payload, ENTITY_KEYS)?,
        relationships: resolve_collection(payload, RELATIONSHIP_KEYS)?,
        triplets: resolve_collection(payload, TRIPLET_KEYS)?,
    };
    require_nothing_dropped(payload, &recognized, &resolved, "Graph payload")?;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitext_follows_posixpath() {
        let s = |t: &str| t.chars().collect::<Vec<_>>();
        assert_eq!(splitext(&s("a.b.c")), (s("a.b"), s(".c")));
        assert_eq!(splitext(&s(".bashrc")), (s(".bashrc"), s("")));
        assert_eq!(splitext(&s("..x")), (s("..x"), s("")));
    }

    #[test]
    fn pure_path_str_follows_pathlib() {
        assert_eq!(pure_path_str("./a//b/"), "a/b");
        assert_eq!(pure_path_str(""), ".");
        assert_eq!(pure_path_str("//x"), "//x");
        assert_eq!(pure_path_str("///x"), "/x");
    }
}
