//! Shared helpers for the parse port: the Python semantics every parser module relies on
//! (`pathlib.Path` string forms and `exists()`, `open(..., encoding=..., errors=...)`
//! decoding with universal newlines, `**options` lookups with Python truthiness).
//!
//! Nothing here is part of the original public API; it is the crate's equivalent of the
//! Python standard-library behaviour the original modules get for free.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use sem_utils::SemanticaError;
use sem_utils::exceptions::OsError;
use sem_utils::helpers::pure_path_str;
use sem_utils::pycompat::fmt::{path_name, truthy};

/// `**options` / `**config` / `**kwargs`: free-form keyword arguments, passed through between
/// parsers exactly as the original passes its keyword dictionaries.
pub type Options = Map<String, Value>;

/// The result type used across the crate.
pub type PResult<T> = std::result::Result<T, SemanticaError>;

/// An argument the original types as `Union[str, Path]` and treats as a file path when it is
/// a `Path`, or a `str` naming an existing file, and as literal content otherwise
/// (`isinstance(x, Path) or (isinstance(x, str) and Path(x).exists())`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A `str` argument: a file path if such a path exists, otherwise the content itself.
    Text(String),
    /// A `pathlib.Path` argument: always a file path.
    Path(PathBuf),
}

impl Source {
    /// The file this argument names, if it is one: `Path` arguments always, `str` arguments
    /// when `Path(x).exists()` (which raises `OSError` for over-long names, as in Python).
    pub fn file_path(&self) -> PResult<Option<PathBuf>> {
        match self {
            Source::Path(p) => Ok(Some(p.clone())),
            Source::Text(s) => {
                if py_exists(Path::new(s))? {
                    Ok(Some(PathBuf::from(s)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// The text of a `str` argument (`None` for a `Path`).
    pub fn text(&self) -> Option<&str> {
        match self {
            Source::Text(s) => Some(s),
            Source::Path(_) => None,
        }
    }

    /// `str(x)`: the string itself, or the normalised path text.
    pub fn as_py_str(&self) -> String {
        match self {
            Source::Text(s) => s.clone(),
            Source::Path(p) => py_path_str(p),
        }
    }

    /// Python's type name of the argument (`str` or `PosixPath`).
    pub fn py_type_name(&self) -> &'static str {
        match self {
            Source::Text(_) => "str",
            Source::Path(_) => "PosixPath",
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_py_str())
    }
}

impl From<&str> for Source {
    fn from(value: &str) -> Self {
        Source::Text(value.to_string())
    }
}

impl From<String> for Source {
    fn from(value: String) -> Self {
        Source::Text(value)
    }
}

impl From<&String> for Source {
    fn from(value: &String) -> Self {
        Source::Text(value.clone())
    }
}

impl From<&Path> for Source {
    fn from(value: &Path) -> Self {
        Source::Path(value.to_path_buf())
    }
}

impl From<PathBuf> for Source {
    fn from(value: PathBuf) -> Self {
        Source::Path(value)
    }
}

impl From<&PathBuf> for Source {
    fn from(value: &PathBuf) -> Self {
        Source::Path(value.clone())
    }
}

impl From<&Source> for Source {
    fn from(value: &Source) -> Self {
        value.clone()
    }
}

// ----------------------------------------------------------------------------- pathlib

/// `str(Path(p))`.
pub fn py_path_str(path: &Path) -> String {
    pure_path_str(&path.to_string_lossy())
}

/// `Path(p).name`.
pub fn py_name(path: &Path) -> String {
    path_name(&py_path_str(path))
}

/// `Path(p).suffix`: the final component's last `.ext` (empty for `.bashrc` or `name.`).
pub fn py_suffix(path: &Path) -> String {
    let name = py_name(path);
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => name[i..].to_string(),
        _ => String::new(),
    }
}

/// `Path(p).stem`.
pub fn py_stem(path: &Path) -> String {
    let name = py_name(path);
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => name[..i].to_string(),
        _ => name,
    }
}

/// `Path(p).parent` (as a path).
pub fn py_parent(path: &Path) -> PathBuf {
    let text = py_path_str(path);
    match text.rfind('/') {
        Some(0) => PathBuf::from("/"),
        Some(i) => PathBuf::from(&text[..i]),
        None => PathBuf::from("."),
    }
}

#[cfg(target_os = "linux")]
const ELOOP: i32 = 40;
#[cfg(not(target_os = "linux"))]
const ELOOP: i32 = 62;
const ENOENT: i32 = 2;
const ENOTDIR: i32 = 20;
const EBADF: i32 = 9;

/// The path Python's `os.stat`/`open` would use for `Path(p)` (`""` is `"."`).
pub fn py_fs_path(path: &Path) -> PathBuf {
    PathBuf::from(py_path_str(path))
}

/// `Path(p).exists()` in Python 3.11: `False` for missing paths (`ENOENT`, `ENOTDIR`,
/// `EBADF`, `ELOOP`) and for names Python cannot encode (embedded NUL); any other `stat`
/// failure (e.g. `ENAMETOOLONG` for a long content string) raises `OSError`.
pub fn py_exists(path: &Path) -> PResult<bool> {
    let text = py_path_str(path);
    if text.contains('\0') {
        return Ok(false);
    }
    match std::fs::metadata(&text) {
        Ok(_) => Ok(true),
        Err(err) => match err.raw_os_error() {
            Some(code) if [ENOENT, ENOTDIR, EBADF, ELOOP].contains(&code) => Ok(false),
            Some(_) => Err(py_os_error(&err, path)),
            None => Ok(false),
        },
    }
}

/// The `OSError` Python raises for an I/O failure on `path` (`str(e)` names the path).
pub fn py_os_error(err: &std::io::Error, path: &Path) -> SemanticaError {
    SemanticaError::Os(OsError::from_io(err, Some(&py_path_str(path))))
}

/// Read a whole file as bytes (`open(path, "rb").read()`), with Python's `OSError`s.
pub fn py_read_bytes(path: &Path) -> PResult<Vec<u8>> {
    std::fs::read(py_fs_path(path)).map_err(|e| py_os_error(&e, path))
}

/// Decode error handling of `bytes.decode` / `open(..., errors=...)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeErrors {
    /// `errors="strict"`: raise `UnicodeDecodeError`.
    Strict,
    /// `errors="ignore"`: drop undecodable bytes.
    Ignore,
    /// `errors="replace"`: substitute U+FFFD.
    Replace,
}

/// `open(path, "r", encoding=encoding, errors=errors).read()`: decode the file and apply
/// universal-newline translation (`\r\n` and `\r` become `\n`).
pub fn py_read_text(path: &Path, encoding: &str, errors: DecodeErrors) -> PResult<String> {
    let bytes = py_read_bytes(path)?;
    let text = py_decode(&bytes, encoding, errors)?;
    Ok(universal_newlines(&text))
}

/// `open(path, "r", encoding=encoding, errors=errors, newline="").read()` (no newline
/// translation).
pub fn py_read_text_raw(path: &Path, encoding: &str, errors: DecodeErrors) -> PResult<String> {
    let bytes = py_read_bytes(path)?;
    py_decode(&bytes, encoding, errors)
}

/// Universal newlines: `\r\n` and lone `\r` become `\n`.
pub fn universal_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

/// CPython's codec-name normalisation (`encodings.normalize_encoding`): ASCII letters
/// lower-cased, every run of characters other than ASCII alphanumerics and `.` collapsed to
/// one `_`, leading and trailing runs dropped.
pub fn py_normalize_encoding(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut punct = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            if punct && !out.is_empty() {
                out.push('_');
            }
            punct = false;
            out.push(c.to_ascii_lowercase());
        } else {
            punct = true;
        }
    }
    out
}

/// Python's codec lookup (normalisation + aliases) for the codecs the port supports.
/// Returns the canonical codec name.
pub fn py_codec_name(encoding: &str) -> Option<&'static str> {
    let norm = py_normalize_encoding(encoding);
    let name = match norm.as_str() {
        "utf_8" | "utf8" | "u8" | "utf" | "utf8_ucs2" | "utf8_ucs4" | "cp65001" => "utf-8",
        "utf_8_sig" | "utf8_sig" => "utf-8-sig",
        "ascii" | "us_ascii" | "646" | "us" | "ansi_x3.4_1968" | "ansi_x3_4_1968" | "cp367"
        | "csascii" | "ibm367" | "iso646_us" | "iso_646.irv_1991" | "iso_ir_6" => "ascii",
        "latin_1" | "latin1" | "latin" | "l1" | "iso_8859_1" | "iso8859_1" | "8859" | "cp819"
        | "iso_ir_100" | "ibm819" | "csisolatin1" | "iso_8859_1_1987" => "latin-1",
        "utf_16" | "utf16" | "u16" => "utf-16",
        "utf_16_le" | "utf_16le" | "utf16le" => "utf-16-le",
        "utf_16_be" | "utf_16be" | "utf16be" => "utf-16-be",
        "utf_32" | "utf32" | "u32" => "utf-32",
        "utf_32_le" | "utf_32le" | "utf32le" => "utf-32-le",
        "utf_32_be" | "utf_32be" | "utf32be" => "utf-32-be",
        "cp1250" | "windows_1250" => "cp1250",
        "cp1251" | "windows_1251" => "cp1251",
        "cp1252" | "windows_1252" => "cp1252",
        "cp1253" | "windows_1253" => "cp1253",
        "cp1254" | "windows_1254" => "cp1254",
        "cp1255" | "windows_1255" => "cp1255",
        "cp1256" | "windows_1256" => "cp1256",
        "cp1257" | "windows_1257" => "cp1257",
        "cp1258" | "windows_1258" => "cp1258",
        "iso_8859_2" | "iso8859_2" | "latin2" | "l2" => "iso8859-2",
        "iso_8859_3" | "iso8859_3" | "latin3" | "l3" => "iso8859-3",
        "iso_8859_4" | "iso8859_4" | "latin4" | "l4" => "iso8859-4",
        "iso_8859_5" | "iso8859_5" | "cyrillic" => "iso8859-5",
        "iso_8859_6" | "iso8859_6" | "arabic" => "iso8859-6",
        "iso_8859_7" | "iso8859_7" | "greek" | "greek8" => "iso8859-7",
        "iso_8859_8" | "iso8859_8" | "hebrew" => "iso8859-8",
        "iso_8859_10" | "iso8859_10" | "latin6" | "l6" => "iso8859-10",
        "iso_8859_13" | "iso8859_13" | "latin7" | "l7" => "iso8859-13",
        "iso_8859_14" | "iso8859_14" | "latin8" | "l8" => "iso8859-14",
        "iso_8859_15" | "iso8859_15" | "latin9" | "l9" => "iso8859-15",
        "iso_8859_16" | "iso8859_16" | "latin10" | "l10" => "iso8859-16",
        "koi8_r" => "koi8-r",
        "koi8_u" => "koi8-u",
        "gbk" | "cp936" | "ms936" | "936" => "gbk",
        "gb2312" | "chinese" | "csiso58gb231280" | "euc_cn" | "euccn" | "eucgb2312_cn"
        | "gb2312_1980" | "gb2312_80" | "iso_ir_58" => "gb2312",
        "gb18030" | "gb18030_2000" => "gb18030",
        "big5" | "big5_tw" | "csbig5" => "big5",
        "shift_jis" | "csshiftjis" | "shiftjis" | "sjis" | "s_jis" => "shift_jis",
        "euc_jp" | "eucjp" | "ujis" | "u_jis" => "euc_jp",
        "euc_kr" | "euckr" | "korean" | "ksc5601" | "ks_c_5601" | "ks_c_5601_1987" | "ksx1001"
        | "ks_x_1001" => "euc_kr",
        "iso2022_jp" | "csiso2022jp" | "iso2022jp" | "iso_2022_jp" => "iso2022_jp",
        "mac_roman" | "macroman" | "macintosh" => "mac-roman",
        "cp866" | "866" | "ibm866" => "cp866",
        "iso_8859_9" | "iso8859_9" | "latin5" | "l5" | "iso_ir_148" | "csisolatin5" => "iso8859-9",
        "iso_8859_11" | "iso8859_11" | "thai" | "iso_8859_11_2001" => "iso8859-11",
        "tis_620" | "tis620" | "tis_620_0" | "tis_620_2529_0" | "tis_620_2529_1" | "iso_ir_166" => {
            "tis-620"
        }
        "cp874" => "cp874",
        "cp932" | "ms932" | "mskanji" | "ms_kanji" | "932" => "cp932",
        "big5hkscs" | "big5_hkscs" | "hkscs" => "big5hkscs",
        "mac_cyrillic" | "maccyrillic" => "mac-cyrillic",
        "cp949" | "uhc" | "ms949" | "949" => "cp949",
        _ => return None,
    };
    Some(name)
}

/// `LookupError: unknown encoding: x`.
pub fn unknown_encoding(encoding: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "LookupError".to_string(),
        message: format!("unknown encoding: {encoding}"),
    }
}

fn unicode_decode_error(
    codec: &str,
    bytes: &[u8],
    start: usize,
    end: usize,
    reason: &str,
) -> SemanticaError {
    let message = if end - start == 1 {
        format!(
            "'{codec}' codec can't decode byte 0x{:02x} in position {start}: {reason}",
            bytes[start]
        )
    } else {
        format!(
            "'{codec}' codec can't decode bytes in position {start}-{}: {reason}",
            end - 1
        )
    };
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "UnicodeDecodeError".to_string(),
        message,
    }
}

/// `bytes.decode(encoding, errors)` for the codecs [`py_codec_name`] knows. Unknown
/// encodings raise `LookupError`, undecodable input with `errors="strict"` raises
/// `UnicodeDecodeError` (CPython's message for UTF-8 and ASCII).
pub fn py_decode(bytes: &[u8], encoding: &str, errors: DecodeErrors) -> PResult<String> {
    let codec = py_codec_name(encoding).ok_or_else(|| unknown_encoding(encoding))?;
    match codec {
        "utf-8" => decode_utf8(bytes, errors),
        "utf-8-sig" => {
            let body = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
            decode_utf8(body, errors).map_err(|e| rename_codec(e, "utf-8", "utf-8-sig"))
        }
        "ascii" => {
            let mut out = String::with_capacity(bytes.len());
            for (i, &b) in bytes.iter().enumerate() {
                if b < 0x80 {
                    out.push(b as char);
                } else {
                    match errors {
                        DecodeErrors::Strict => {
                            return Err(unicode_decode_error(
                                "ascii",
                                bytes,
                                i,
                                i + 1,
                                "ordinal not in range(128)",
                            ));
                        }
                        DecodeErrors::Ignore => {}
                        DecodeErrors::Replace => out.push('\u{fffd}'),
                    }
                }
            }
            Ok(out)
        }
        "latin-1" => Ok(bytes.iter().map(|&b| b as char).collect()),
        other => decode_with_encoding_rs(bytes, other, errors),
    }
}

fn rename_codec(err: SemanticaError, from: &str, to: &str) -> SemanticaError {
    match err {
        SemanticaError::Other {
            module,
            type_name,
            message,
        } => SemanticaError::Other {
            module,
            type_name,
            message: message.replacen(&format!("'{from}'"), &format!("'{to}'"), 1),
        },
        other => other,
    }
}

/// Strict/ignore/replace UTF-8 decoding with CPython's error positions and reasons.
pub fn decode_utf8(bytes: &[u8], errors: DecodeErrors) -> PResult<String> {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match std::str::from_utf8(&bytes[i..]) {
            Ok(s) => {
                out.push_str(s);
                break;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                out.push_str(std::str::from_utf8(&bytes[i..i + valid]).expect("valid prefix"));
                let start = i + valid;
                let (bad_len, reason) = match e.error_len() {
                    Some(n) => {
                        // CPython reports "invalid start byte" for bytes that cannot start a
                        // sequence and "invalid continuation byte" otherwise.
                        let lead = bytes[start];
                        let reason = if (0x80..0xc2).contains(&lead) || lead > 0xf4 {
                            "invalid start byte"
                        } else {
                            "invalid continuation byte"
                        };
                        (n, reason)
                    }
                    None => (bytes.len() - start, "unexpected end of data"),
                };
                match errors {
                    DecodeErrors::Strict => {
                        let end = if reason == "invalid continuation byte" {
                            start + bad_len
                        } else if reason == "unexpected end of data" {
                            bytes.len()
                        } else {
                            start + 1
                        };
                        return Err(unicode_decode_error("utf-8", bytes, start, end, reason));
                    }
                    DecodeErrors::Ignore => {}
                    DecodeErrors::Replace => out.push('\u{fffd}'),
                }
                i = start + bad_len.max(1);
            }
        }
    }
    Ok(out)
}

fn decode_with_encoding_rs(bytes: &[u8], codec: &str, errors: DecodeErrors) -> PResult<String> {
    let label: &[u8] = match codec {
        "utf-16" => {
            // Python's utf-16 honours a BOM and defaults to little endian.
            if bytes.starts_with(b"\xfe\xff") {
                return decode_with_encoding_rs(&bytes[2..], "utf-16-be", errors);
            }
            let body = bytes.strip_prefix(b"\xff\xfe").unwrap_or(bytes);
            return decode_with_encoding_rs(body, "utf-16-le", errors);
        }
        "utf-16-le" => b"utf-16le",
        "utf-16-be" => b"utf-16be",
        "utf-32" | "utf-32-le" | "utf-32-be" => {
            let (body, big) = match codec {
                "utf-32-be" => (bytes, true),
                "utf-32-le" => (bytes, false),
                _ if bytes.starts_with(b"\x00\x00\xfe\xff") => (&bytes[4..], true),
                _ => (
                    bytes.strip_prefix(b"\xff\xfe\x00\x00").unwrap_or(bytes),
                    false,
                ),
            };
            let mut out = String::new();
            for chunk in body.chunks(4) {
                let ch = if chunk.len() == 4 {
                    let v = if big {
                        u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    } else {
                        u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    };
                    char::from_u32(v)
                } else {
                    None
                };
                match (ch, errors) {
                    (Some(c), _) => out.push(c),
                    (None, DecodeErrors::Strict) => {
                        return Err(SemanticaError::Other {
                            module: "builtins".to_string(),
                            type_name: "UnicodeDecodeError".to_string(),
                            message: format!("'{codec}' codec can't decode bytes"),
                        });
                    }
                    (None, DecodeErrors::Ignore) => {}
                    (None, DecodeErrors::Replace) => out.push('\u{fffd}'),
                }
            }
            return Ok(out);
        }
        "cp1250" => b"windows-1250",
        "cp1251" => b"windows-1251",
        "cp1252" => b"windows-1252",
        "cp1253" => b"windows-1253",
        "cp1254" => b"windows-1254",
        "cp1255" => b"windows-1255",
        "cp1256" => b"windows-1256",
        "cp1257" => b"windows-1257",
        "cp1258" => b"windows-1258",
        "iso8859-2" => b"iso-8859-2",
        "iso8859-3" => b"iso-8859-3",
        "iso8859-4" => b"iso-8859-4",
        "iso8859-5" => b"iso-8859-5",
        "iso8859-6" => b"iso-8859-6",
        "iso8859-7" => b"iso-8859-7",
        "iso8859-8" => b"iso-8859-8",
        "iso8859-10" => b"iso-8859-10",
        "iso8859-13" => b"iso-8859-13",
        "iso8859-14" => b"iso-8859-14",
        "iso8859-15" => b"iso-8859-15",
        "iso8859-16" => b"iso-8859-16",
        "koi8-r" => b"koi8-r",
        "koi8-u" => b"koi8-u",
        "gbk" | "gb2312" => b"gbk",
        "gb18030" => b"gb18030",
        "big5" => b"big5",
        "shift_jis" => b"shift_jis",
        "euc_jp" => b"euc-jp",
        "euc_kr" => b"euc-kr",
        "iso2022_jp" => b"iso-2022-jp",
        "mac-roman" => b"macintosh",
        "cp866" => b"ibm866",
        "iso8859-9" => b"iso-8859-9",
        "iso8859-11" | "tis-620" | "cp874" => b"windows-874",
        "cp932" => b"shift_jis",
        "big5hkscs" => b"big5",
        "mac-cyrillic" => b"x-mac-cyrillic",
        "cp949" => b"euc-kr",
        _ => return Err(unknown_encoding(codec)),
    };
    let encoding =
        encoding_rs::Encoding::for_label(label).ok_or_else(|| unknown_encoding(codec))?;
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let mut out = String::with_capacity(bytes.len() * 2);
    let max = decoder
        .max_utf8_buffer_length(bytes.len())
        .unwrap_or(bytes.len() * 4 + 16);
    out.reserve(max);
    let (result, _read, had_errors) = decoder.decode_to_string(bytes, &mut out, true);
    debug_assert!(matches!(result, encoding_rs::CoderResult::InputEmpty));
    if had_errors {
        match errors {
            DecodeErrors::Strict => {
                return Err(SemanticaError::Other {
                    module: "builtins".to_string(),
                    type_name: "UnicodeDecodeError".to_string(),
                    message: format!(
                        "'{codec}' codec can't decode bytes: illegal multibyte sequence"
                    ),
                });
            }
            DecodeErrors::Ignore => {
                out = out.replace('\u{fffd}', "");
            }
            DecodeErrors::Replace => {}
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------------------ options

/// `options.get(key)`.
pub fn opt<'a>(options: &'a Options, key: &str) -> Option<&'a Value> {
    options.get(key)
}

/// `bool(options.get(key, default))`.
pub fn opt_truthy(options: &Options, key: &str, default: bool) -> bool {
    match options.get(key) {
        Some(v) => truthy(v),
        None => default,
    }
}

/// `options.get(key, default)` for a string option (non-strings fall back to `str()`).
pub fn opt_string(options: &Options, key: &str, default: &str) -> String {
    match options.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => default.to_string(),
        Some(other) => sem_utils::pycompat::fmt::py_str(other),
    }
}

/// `options.get(key)` as an optional string (`None`/missing give `None`).
pub fn opt_opt_string(options: &Options, key: &str) -> Option<String> {
    match options.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Null) | None => None,
        Some(other) => Some(sem_utils::pycompat::fmt::py_str(other)),
    }
}

/// `config.get(key, {})` for a nested keyword dictionary (`**self.config.get("pdf", {})`).
/// Non-mapping values count as empty.
pub fn sub_config(config: &Options, key: &str) -> Options {
    match config.get(key) {
        Some(Value::Object(map)) => map.clone(),
        _ => Options::new(),
    }
}

/// `config = config or {}; config.update(kwargs)`.
pub fn merge_config(config: Option<Options>, kwargs: Options) -> Options {
    let mut merged = config.unwrap_or_default();
    for (k, v) in kwargs {
        merged.insert(k, v);
    }
    merged
}

/// `dict(options, **{key: value})`: a copy of `options` with one keyword set.
pub fn with_opt(options: &Options, key: &str, value: Value) -> Options {
    let mut out = options.clone();
    out.insert(key.to_string(), value);
    out
}

/// Bind a named parameter from keyword arguments, as Python does for
/// `def f(x, name=default, **options)` called with `f(x, **kwargs)`: returns the value bound
/// to `name` (if given) and the remaining keywords.
pub fn bind_kwarg(options: &Options, key: &str) -> (Option<Value>, Options) {
    let mut rest = options.clone();
    let value = rest.shift_remove(key);
    (value, rest)
}

/// `AttributeError: '<type>' object has no attribute '<attr>'`.
pub fn attribute_error(type_name: &str, attr: &str) -> SemanticaError {
    SemanticaError::Attribute(format!("'{type_name}' object has no attribute '{attr}'"))
}

/// `TypeError(message)`.
pub fn type_error(message: impl Into<String>) -> SemanticaError {
    SemanticaError::Type(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_parts_follow_pathlib() {
        assert_eq!(py_path_str(Path::new("")), ".");
        assert_eq!(py_path_str(Path::new("a//b/./c/")), "a/b/c");
        assert_eq!(py_name(Path::new("/x/y/file.tar.gz")), "file.tar.gz");
        assert_eq!(py_suffix(Path::new("/x/y/file.tar.gz")), ".gz");
        assert_eq!(py_suffix(Path::new(".bashrc")), "");
        assert_eq!(py_suffix(Path::new("a.")), "");
        assert_eq!(py_stem(Path::new("/x/report.PDF")), "report");
        assert_eq!(py_parent(Path::new("/x/y.txt")), PathBuf::from("/x"));
        assert_eq!(py_parent(Path::new("y.txt")), PathBuf::from("."));
    }

    #[test]
    fn exists_raises_for_long_names() {
        assert!(py_exists(Path::new("")).unwrap());
        assert!(!py_exists(Path::new("a\0b")).unwrap());
        let err = py_exists(Path::new(&"x".repeat(300))).unwrap_err();
        assert_eq!(err.type_name(), "OSError");
        assert!(
            err.to_string()
                .starts_with("[Errno 63] File name too long: 'xxx")
        );
    }

    #[test]
    fn decoding_matches_python() {
        assert_eq!(
            py_decode(b"caf\xc3\xa9", "UTF-8", DecodeErrors::Strict).unwrap(),
            "café"
        );
        assert_eq!(
            py_decode(b"a\xffb", "utf-8", DecodeErrors::Ignore).unwrap(),
            "ab"
        );
        let err = py_decode(b"a\xffb", "utf-8", DecodeErrors::Strict).unwrap_err();
        assert_eq!(
            err.to_string(),
            "'utf-8' codec can't decode byte 0xff in position 1: invalid start byte"
        );
        let err = py_decode(b"ab\xc3", "utf-8", DecodeErrors::Strict).unwrap_err();
        assert_eq!(
            err.to_string(),
            "'utf-8' codec can't decode byte 0xc3 in position 2: unexpected end of data"
        );
        assert_eq!(
            py_decode(b"\xe9", "latin-1", DecodeErrors::Strict).unwrap(),
            "é"
        );
        assert_eq!(
            py_decode(b"\x93x\x94", "cp1252", DecodeErrors::Strict).unwrap(),
            "\u{201c}x\u{201d}"
        );
        assert_eq!(
            py_decode(b"x", "nope", DecodeErrors::Strict)
                .unwrap_err()
                .to_string(),
            "unknown encoding: nope"
        );
        assert_eq!(universal_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(py_codec_name("UTF\"-8"), Some("utf-8"));
        assert_eq!(py_codec_name("ISO-8859-9"), Some("iso8859-9"));
    }
}
