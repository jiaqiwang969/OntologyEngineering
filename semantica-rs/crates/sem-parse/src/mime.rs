//! The parts of Python 3.11's `email` package that `email_parser.py` exercises, with the
//! `compat32` policy that `email.message_from_string` / `email.message_from_bytes` use:
//! the feed parser (`email.feedparser`), the `Message` object model (`email.message`),
//! `email.header.decode_header` and `Header.__str__`, `email.utils.parsedate_to_datetime`,
//! the RFC 2231 parameter helpers (`email.utils.decode_params`,
//! `collapse_rfc2231_value`) and the `binascii` decoders behind `get_payload(decode=True)`.
//!
//! # Surrogate-escaped text
//!
//! `message_from_bytes` decodes its input as ASCII with `errors="surrogateescape"`, so every
//! byte `>= 0x80` becomes a lone surrogate `U+DC80..U+DCFF` inside a Python `str`. Rust
//! strings cannot hold surrogates, so this module represents the escaped byte `b` by the
//! private-use character `U+10FE00 + b` ([`esc_char`]). Like a lone surrogate, such a
//! character is not whitespace, has no case mapping, is not a line boundary and is outside
//! every ASCII class the parser's regular expressions use, so all string operations behave
//! as they do on the surrogate. `encode('ascii', 'surrogateescape')` maps it back to its byte
//! and `raw-unicode-escape` renders it as the surrogate's `\udcXX` escape. Input strings
//! passed to [`message_from_string`] are taken as they are (a Python `str` from Rust text
//! never contains surrogates; text that already contains `U+10FE80..U+10FEFF` would be
//! read as escaped bytes).

use std::collections::VecDeque;
use std::fmt;

use once_cell::sync::Lazy;
use regex::Regex;

use sem_utils::SemanticaError;

use crate::common::{DecodeErrors, PResult, py_decode};

// ------------------------------------------------------------------ surrogate escapes

const ESC_BASE: u32 = 0x10FE00;

/// The character standing for the surrogate-escaped byte `b` (`b >= 0x80`).
pub fn esc_char(b: u8) -> char {
    char::from_u32(ESC_BASE + b as u32).expect("private-use code point")
}

/// The byte a surrogate-escape character stands for.
pub fn esc_byte(c: char) -> Option<u8> {
    let cp = c as u32;
    if (ESC_BASE + 0x80..=ESC_BASE + 0xFF).contains(&cp) {
        Some((cp - ESC_BASE) as u8)
    } else {
        None
    }
}

/// `bytes.decode('ascii', 'surrogateescape')`.
pub fn decode_surrogateescape(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| if b < 0x80 { b as char } else { esc_char(b) })
        .collect()
}

/// `email.utils._has_surrogates(s)`.
pub fn has_surrogates(s: &str) -> bool {
    s.chars().any(|c| esc_byte(c).is_some())
}

/// `s.encode('ascii', 'surrogateescape')`; `None` when `s` holds other non-ASCII characters
/// (Python's `UnicodeEncodeError`).
pub fn encode_surrogateescape(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c as u8);
        } else if let Some(b) = esc_byte(c) {
            out.push(b);
        } else {
            return None;
        }
    }
    Some(out)
}

/// `s.encode('raw-unicode-escape')`: code points below 256 as one byte, others as
/// `\uXXXX` / `\UXXXXXXXX` (surrogate escapes as their `\udcXX` surrogate).
pub fn encode_raw_unicode_escape(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        let cp = match esc_byte(c) {
            Some(b) => 0xDC00 + b as u32,
            None => c as u32,
        };
        if cp < 0x100 {
            out.push(cp as u8);
        } else if cp < 0x10000 {
            out.extend_from_slice(format!("\\u{cp:04x}").as_bytes());
        } else {
            out.extend_from_slice(format!("\\U{cp:08x}").as_bytes());
        }
    }
    out
}

/// `bytes.decode('ascii', 'replace')`.
fn decode_ascii_replace(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| if b < 0x80 { b as char } else { '\u{fffd}' })
        .collect()
}

// --------------------------------------------------------------- Python str helpers

/// `str.isspace()` for one character (Unicode White_Space plus `\x1c`-`\x1f`).
pub fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// `s.isspace()`: non-empty and all whitespace.
pub fn py_str_isspace(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_isspace)
}

/// `s.strip()`.
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(py_isspace)
}

/// `s.lstrip()`.
pub fn py_lstrip(s: &str) -> &str {
    s.trim_start_matches(py_isspace)
}

/// `s.rstrip()`.
pub fn py_rstrip(s: &str) -> &str {
    s.trim_end_matches(py_isspace)
}

/// `s.split()` (whitespace runs, no empty strings).
pub fn py_split_whitespace(s: &str) -> Vec<&str> {
    s.split(py_isspace).filter(|p| !p.is_empty()).collect()
}

/// `str.splitlines()` (without line ends).
pub fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\x0b'
                | '\x0c'
                | '\x1c'
                | '\x1d'
                | '\x1e'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if is_break {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r'
                && let Some(&(j, '\n')) = chars.peek()
            {
                chars.next();
                end = j + 1;
            }
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// `bytes.splitlines()` (ASCII line boundaries `\n`, `\r`, `\r\n`), without line ends.
fn bytes_splitlines(b: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\n' || b[i] == b'\r' {
            out.push(&b[start..i]);
            if b[i] == b'\r' && i + 1 < b.len() && b[i + 1] == b'\n' {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < b.len() {
        out.push(&b[start..]);
    }
    out
}

/// Split text into lines keeping their ends, recognising `\r\n`, `\r` and `\n`
/// (`StringIO(newline='').readlines()`).
fn split_keep_ends(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            out.push(s[start..=i].to_string());
            start = i + 1;
        } else if bytes[i] == b'\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                out.push(s[start..i + 2].to_string());
                i += 1;
            } else {
                out.push(s[start..=i].to_string());
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < s.len() {
        out.push(s[start..].to_string());
    }
    out
}

/// The line end (`\r\n`, `\r` or `\n`) a string ends with (`NLCRE_eol`).
fn trailing_linesep(s: &str) -> Option<&'static str> {
    if s.ends_with("\r\n") {
        Some("\r\n")
    } else if s.ends_with('\r') {
        Some("\r")
    } else if s.ends_with('\n') {
        Some("\n")
    } else {
        None
    }
}

/// The line end a string starts with (`NLCRE.match` / `NLCRE_bol`).
fn leading_linesep(s: &str) -> Option<&'static str> {
    if s.starts_with("\r\n") {
        Some("\r\n")
    } else if s.starts_with('\r') {
        Some("\r")
    } else if s.starts_with('\n') {
        Some("\n")
    } else {
        None
    }
}

/// `int(s)` for the date parser: optional sign, ASCII digits with single underscores
/// between them, surrounding whitespace.
fn py_int(s: &str) -> Option<i64> {
    let s = py_strip(s);
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let clean: String = digits.chars().filter(|&c| c != '_').collect();
    let n: i64 = clean.parse().ok()?;
    Some(if neg { -n } else { n })
}

/// CPython's codec-name normalisation (`_Py_normalize_encoding` /
/// `encodings.normalize_encoding`): ASCII letters lower-cased, every run of characters other
/// than ASCII alphanumerics and `.` collapsed to one `_`, leading/trailing runs dropped.
pub fn normalize_encoding(name: &str) -> String {
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

/// `bytes.decode(encoding, errors)` with Python's codec lookup: the name is normalised as
/// CPython does before the codec table is consulted, and an unknown codec raises
/// `LookupError: unknown encoding: <name as given>`.
pub fn bytes_decode(bytes: &[u8], encoding: &str, errors: DecodeErrors) -> PResult<String> {
    let normalized = normalize_encoding(encoding);
    match py_decode(bytes, &normalized, errors) {
        Err(e) if e.type_name() == "LookupError" => match supplementary_codec(&normalized) {
            Some((codec, label)) => decode_encoding_rs(bytes, codec, label, errors),
            None => Err(crate::common::unknown_encoding(encoding)),
        },
        other => other,
    }
}

/// Python codecs (by normalised alias) that `common::py_decode` does not know, decoded
/// with the WHATWG encoding that is a superset of them in `encoding_rs`: Python codec name
/// and `encoding_rs` label.
fn supplementary_codec(normalized: &str) -> Option<(&'static str, &'static str)> {
    Some(match normalized {
        "iso_8859_9" | "iso8859_9" | "latin5" | "l5" | "iso_ir_148" | "csisolatin5" => {
            ("iso8859-9", "iso-8859-9")
        }
        "iso_8859_11" | "iso8859_11" | "thai" | "iso_8859_11_2001" => ("iso8859-11", "windows-874"),
        "tis_620" | "tis620" | "tis_620_0" | "tis_620_2529_0" | "tis_620_2529_1" | "iso_ir_166" => {
            ("tis-620", "windows-874")
        }
        "cp874" => ("cp874", "windows-874"),
        "cp932" | "ms932" | "mskanji" | "ms_kanji" | "932" => ("cp932", "shift_jis"),
        "big5hkscs" | "big5_hkscs" | "hkscs" => ("big5hkscs", "big5"),
        "mac_cyrillic" | "maccyrillic" => ("mac-cyrillic", "x-mac-cyrillic"),
        "cp949" | "uhc" | "ms949" | "949" => ("cp949", "euc-kr"),
        _ => return None,
    })
}

fn decode_encoding_rs(
    bytes: &[u8],
    codec: &str,
    label: &str,
    errors: DecodeErrors,
) -> PResult<String> {
    let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
        .ok_or_else(|| crate::common::unknown_encoding(codec))?;
    let (text, had_errors) = encoding.decode_without_bom_handling(bytes);
    if !had_errors {
        return Ok(text.into_owned());
    }
    match errors {
        DecodeErrors::Strict => Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "UnicodeDecodeError".to_string(),
            message: format!("'{codec}' codec can't decode bytes: illegal multibyte sequence"),
        }),
        DecodeErrors::Ignore => Ok(text.replace('\u{fffd}', "")),
        DecodeErrors::Replace => Ok(text.into_owned()),
    }
}

// ------------------------------------------------------------------------ binascii

fn b64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// `binascii.a2b_base64(data, strict_mode=strict)`; `Err` carries `binascii.Error`'s text.
pub fn a2b_base64(data: &[u8], strict: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(data.len() / 4 * 3 + 3);
    let mut quad_pos = 0u8;
    let mut leftchar: u32 = 0;
    let mut pads = 0u32;
    let mut padding_started = false;
    for (i, &ch) in data.iter().enumerate() {
        if ch == b'=' {
            padding_started = true;
            if strict && quad_pos == 0 {
                return Err(if i == 0 {
                    "Leading padding not allowed".to_string()
                } else {
                    "Excess padding not allowed".to_string()
                });
            }
            if quad_pos >= 2 {
                pads += 1;
                if quad_pos as u32 + pads >= 4 {
                    if strict && i + 1 < data.len() {
                        return Err("Excess data after padding".to_string());
                    }
                    return Ok(out);
                }
            }
            continue;
        }
        let Some(v) = b64_value(ch) else {
            if strict {
                return Err("Only base64 data is allowed".to_string());
            }
            continue;
        };
        if strict && padding_started {
            return Err("Discontinuous padding not allowed".to_string());
        }
        pads = 0;
        match quad_pos {
            0 => {
                quad_pos = 1;
                leftchar = v as u32;
            }
            1 => {
                quad_pos = 2;
                out.push(((leftchar << 2) | (v as u32 >> 4)) as u8);
                leftchar = v as u32 & 0x0f;
            }
            2 => {
                quad_pos = 3;
                out.push(((leftchar << 4) | (v as u32 >> 2)) as u8);
                leftchar = v as u32 & 0x03;
            }
            _ => {
                quad_pos = 0;
                out.push(((leftchar << 6) | v as u32) as u8);
                leftchar = 0;
            }
        }
    }
    match quad_pos {
        0 => Ok(out),
        1 => Err(format!(
            "Invalid base64-encoded string: number of data characters ({}) cannot be 1 more than a multiple of 4",
            out.len() / 3 * 4 + 1
        )),
        _ => Err("Incorrect padding".to_string()),
    }
}

/// `binascii.a2b_qp(data, header=header)` (what `quopri.decodestring` calls).
pub fn a2b_qp(data: &[u8], header: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let n = data.len();
    let mut i = 0;
    let is_hex = |c: u8| c.is_ascii_hexdigit();
    let hex = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
    while i < n {
        if data[i] == b'=' {
            i += 1;
            if i >= n {
                break;
            }
            if data[i] == b'\n' || data[i] == b'\r' {
                // Soft line break.
                if data[i] != b'\n' {
                    while i < n && data[i] != b'\n' {
                        i += 1;
                    }
                }
                if i < n {
                    i += 1;
                }
            } else if data[i] == b'=' {
                out.push(b'=');
                i += 1;
            } else if i + 1 < n && is_hex(data[i]) && is_hex(data[i + 1]) {
                out.push((hex(data[i]) << 4) | hex(data[i + 1]));
                i += 2;
            } else {
                out.push(b'=');
            }
        } else if header && data[i] == b'_' {
            out.push(b' ');
            i += 1;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

/// `binascii.a2b_uu(line)`.
fn a2b_uu(line: &[u8]) -> Result<Vec<u8>, String> {
    let Some((&first, rest)) = line.split_first() else {
        return Ok(Vec::new());
    };
    let mut bin_len = (first.wrapping_sub(b' ') & 0o77) as usize;
    let mut out = Vec::with_capacity(bin_len);
    let mut idx = 0;
    let mut leftchar: u32 = 0;
    let mut leftbits = 0u32;
    while bin_len > 0 {
        let available = idx < rest.len();
        let mut ch = if available { rest[idx] } else { 0 };
        if ch == b'\n' || ch == b'\r' || !available {
            ch = 0;
        } else {
            if ch < b' ' || ch > b' ' + 64 {
                return Err("Illegal char".to_string());
            }
            ch = (ch - b' ') & 0o77;
        }
        leftchar = (leftchar << 6) | ch as u32;
        leftbits += 6;
        if leftbits >= 8 {
            leftbits -= 8;
            out.push(((leftchar >> leftbits) & 0xff) as u8);
            leftchar &= (1 << leftbits) - 1;
            bin_len -= 1;
        }
        idx += 1;
    }
    while idx < rest.len() {
        let ch = rest[idx];
        if ch != b' ' && ch != b' ' + 64 && ch != b'\n' && ch != b'\r' {
            return Err("Trailing garbage".to_string());
        }
        idx += 1;
    }
    Ok(out)
}

/// `email.message._decode_uu`.
fn decode_uu(encoded: &[u8]) -> Result<Vec<u8>, String> {
    let lines = bytes_splitlines(encoded);
    let mut iter = lines.into_iter();
    let mut found = false;
    for line in iter.by_ref() {
        if let Some(rest) = line.strip_prefix(b"begin ") {
            let mode = rest.split(|&b| b == b' ').next().unwrap_or(b"");
            let mode_str = String::from_utf8_lossy(mode);
            let trimmed = mode_str.trim_matches(|c: char| c.is_ascii_whitespace());
            let valid = !trimmed.is_empty()
                && trimmed
                    .trim_start_matches(['+', '-'])
                    .chars()
                    .all(|c| ('0'..='7').contains(&c) || c == '_')
                && !trimmed.trim_start_matches(['+', '-']).is_empty();
            if valid {
                found = true;
                break;
            }
        }
    }
    if !found {
        return Err("`begin` line not found".to_string());
    }
    let mut decoded = Vec::new();
    for line in iter {
        if line.is_empty() {
            return Err("Truncated input".to_string());
        }
        let stripped: &[u8] = {
            let s = line;
            let start = s
                .iter()
                .position(|b| !b" \t\r\n\x0c".contains(b))
                .unwrap_or(s.len());
            let end = s
                .iter()
                .rposition(|b| !b" \t\r\n\x0c".contains(b))
                .map(|p| p + 1)
                .unwrap_or(start);
            &s[start..end.max(start)]
        };
        if stripped == b"end" {
            break;
        }
        let part = match a2b_uu(line) {
            Ok(p) => p,
            Err(_) => {
                let nbytes = ((((line[0] as i64 - 32) & 63) * 4 + 5) / 3) as usize;
                a2b_uu(&line[..nbytes.min(line.len())])?
            }
        };
        decoded.extend_from_slice(&part);
    }
    Ok(decoded)
}

/// `email._encoded_words.decode_b(encoded)`: the decoded bytes (defects are not kept).
fn decode_b(encoded: &[u8]) -> Vec<u8> {
    let pad_err = encoded.len() % 4;
    let mut padded = encoded.to_vec();
    if pad_err != 0 {
        padded.extend_from_slice(&b"==="[..4 - pad_err]);
    }
    if let Ok(v) = a2b_base64(&padded, true) {
        return v;
    }
    if let Ok(v) = a2b_base64(encoded, false) {
        return v;
    }
    let mut extra = encoded.to_vec();
    extra.extend_from_slice(b"==");
    if let Ok(v) = a2b_base64(&extra, false) {
        return v;
    }
    encoded.to_vec()
}

// --------------------------------------------------------------------------- errors

/// `email.errors.HeaderParseError(message)`.
pub fn header_parse_error(message: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "email.errors".to_string(),
        type_name: "HeaderParseError".to_string(),
        message: message.to_string(),
    }
}

// ------------------------------------------------------------------- Header objects

/// The `unknown-8bit` charset name (`email.charset.UNKNOWN8BIT`).
pub const UNKNOWN8BIT: &str = "unknown-8bit";

/// An `email.header.Header` as `compat32` creates it for header values carrying
/// surrogate-escaped bytes: one chunk per appended string with its charset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    chunks: Vec<(String, String)>,
}

impl Header {
    /// `Header(value, charset=UNKNOWN8BIT, header_name=name)` as `_sanitize_header` builds it.
    pub fn unknown_8bit(value: &str) -> Header {
        Header {
            chunks: vec![(value.to_string(), UNKNOWN8BIT.to_string())],
        }
    }

    /// The chunks after `_normalize()`: runs of the same charset joined with spaces.
    fn normalized(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut last_charset: Option<&str> = None;
        let mut last_chunk: Vec<&str> = Vec::new();
        for (s, cs) in &self.chunks {
            if Some(cs.as_str()) == last_charset {
                last_chunk.push(s);
            } else {
                if let Some(lc) = last_charset {
                    out.push((last_chunk.join(" "), lc.to_string()));
                }
                last_chunk = vec![s];
                last_charset = Some(cs);
            }
        }
        if let Some(lc) = last_charset
            && !last_chunk.is_empty()
        {
            out.push((last_chunk.join(" "), lc.to_string()));
        }
        out
    }
}

impl fmt::Display for Header {
    /// `Header.__str__`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nonctext = |c: char| py_isspace(c) || c == '(' || c == ')' || c == '\\';
        let mut uchunks: Vec<String> = Vec::new();
        let mut lastcs: Option<String> = None;
        let mut lastspace = false;
        for (string, charset) in self.normalized() {
            let mut nextcs: Option<String> = Some(charset.clone());
            let string = if charset == UNKNOWN8BIT {
                let bytes = encode_surrogateescape(&string)
                    .unwrap_or_else(|| encode_raw_unicode_escape(&string));
                decode_ascii_replace(&bytes)
            } else {
                string
            };
            let is_ascii_cs = |cs: &Option<String>| cs.as_deref().is_none_or(|c| c == "us-ascii");
            if !uchunks.is_empty() {
                let hasspace = string.chars().next().is_some_and(nonctext);
                if !is_ascii_cs(&lastcs) {
                    if is_ascii_cs(&nextcs) && !hasspace {
                        uchunks.push(" ".to_string());
                        nextcs = None;
                    }
                } else if !is_ascii_cs(&nextcs) && !lastspace {
                    uchunks.push(" ".to_string());
                }
            }
            lastspace = string.chars().last().is_some_and(nonctext);
            lastcs = nextcs;
            uchunks.push(string);
        }
        f.write_str(&uchunks.concat())
    }
}

/// A header value as `Message.get`/`items` return it under `compat32`: the raw string, or a
/// [`Header`] when the value carries surrogate-escaped bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderValue {
    /// A plain `str`.
    Str(String),
    /// An `email.header.Header` (`unknown-8bit`).
    Header(Header),
}

impl HeaderValue {
    /// `policy.header_fetch_parse(name, value)` (`_sanitize_header`).
    fn fetch(value: &str) -> HeaderValue {
        if has_surrogates(value) {
            HeaderValue::Header(Header::unknown_8bit(value))
        } else {
            HeaderValue::Str(value.to_string())
        }
    }

    /// `needle in value`: a substring test for strings; `Header` objects are not iterable
    /// (`TypeError: argument of type 'Header' is not iterable`).
    pub fn contains(&self, needle: &str) -> PResult<bool> {
        match self {
            HeaderValue::Str(s) => Ok(s.contains(needle)),
            HeaderValue::Header(_) => Err(SemanticaError::Type(
                "argument of type 'Header' is not iterable".to_string(),
            )),
        }
    }

    /// The value as a plain string if it is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            HeaderValue::Str(s) => Some(s),
            HeaderValue::Header(_) => None,
        }
    }

    /// Python truthiness (`bool(value)`): strings by emptiness, `Header` objects always.
    pub fn truthy(&self) -> bool {
        match self {
            HeaderValue::Str(s) => !s.is_empty(),
            HeaderValue::Header(_) => true,
        }
    }
}

impl fmt::Display for HeaderValue {
    /// `str(value)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderValue::Str(s) => f.write_str(s),
            HeaderValue::Header(h) => fmt::Display::fmt(h, f),
        }
    }
}

/// One element of `decode_header`'s result: Python returns `str` for a header without
/// encoded words and `bytes` otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedPart {
    /// A `str` part.
    Str(String),
    /// A `bytes` part.
    Bytes(Vec<u8>),
}

static ECRE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"=\?([^?]*?)\?([qQbB])\?(.*?)\?=").expect("valid regex"));
static Q_HEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"=[a-fA-F0-9]{2}").expect("valid regex"));

/// `email.header.decode_header(header)`: `(part, charset)` pairs.
pub fn decode_header(header: &HeaderValue) -> PResult<Vec<(DecodedPart, Option<String>)>> {
    let header = match header {
        HeaderValue::Header(h) => {
            return Ok(h
                .chunks
                .iter()
                .map(|(s, cs)| {
                    let bytes = if cs == UNKNOWN8BIT {
                        encode_surrogateescape(s).unwrap_or_else(|| encode_raw_unicode_escape(s))
                    } else {
                        s.as_bytes().to_vec()
                    };
                    (DecodedPart::Bytes(bytes), Some(cs.clone()))
                })
                .collect());
        }
        HeaderValue::Str(s) => s,
    };
    decode_header_str(header)
}

/// [`decode_header`] for a `str` value.
pub fn decode_header_str(header: &str) -> PResult<Vec<(DecodedPart, Option<String>)>> {
    if !ECRE.is_match(header) {
        return Ok(vec![(DecodedPart::Str(header.to_string()), None)]);
    }
    // (text, encoding, charset)
    let mut words: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for line in py_splitlines(header) {
        let mut pieces: Vec<(String, Option<(String, String, String)>)> = Vec::new();
        let mut pos = 0;
        for caps in ECRE.captures_iter(line) {
            let m = caps.get(0).expect("match");
            pieces.push((
                line[pos..m.start()].to_string(),
                Some((
                    caps[1].to_string(),
                    caps[2].to_string(),
                    caps[3].to_string(),
                )),
            ));
            pos = m.end();
        }
        pieces.push((line[pos..].to_string(), None));
        let mut first = true;
        for (unencoded, encoded) in pieces {
            let unencoded = if first {
                first = false;
                py_lstrip(&unencoded).to_string()
            } else {
                unencoded
            };
            if !unencoded.is_empty() {
                words.push((unencoded, None, None));
            }
            if let Some((charset, encoding, text)) = encoded {
                words.push((
                    text,
                    Some(encoding.to_lowercase()),
                    Some(charset.to_lowercase()),
                ));
            }
        }
    }
    let mut droplist = Vec::new();
    for n in 2..words.len() {
        if words[n].1.is_some() && words[n - 2].1.is_some() && py_str_isspace(&words[n - 1].0) {
            droplist.push(n - 1);
        }
    }
    for d in droplist.into_iter().rev() {
        words.remove(d);
    }
    let mut decoded: Vec<(DecodedPart, Option<String>)> = Vec::new();
    for (text, encoding, charset) in words {
        match encoding.as_deref() {
            None => decoded.push((DecodedPart::Str(text), charset)),
            Some("q") => {
                let replaced = text.replace('_', " ");
                let word = Q_HEX.replace_all(&replaced, |caps: &regex::Captures| {
                    let v = u32::from_str_radix(&caps[0][1..3], 16).expect("hex");
                    char::from_u32(v).expect("byte value").to_string()
                });
                decoded.push((DecodedPart::Str(word.into_owned()), charset));
            }
            Some("b") => {
                let mut s = text;
                let paderr = s.chars().count() % 4;
                if paderr != 0 {
                    s.push_str(&"==="[..4 - paderr]);
                }
                let word = if s.is_empty() {
                    Vec::new()
                } else {
                    a2b_base64(&encode_raw_unicode_escape(&s), false)
                        .map_err(|_| header_parse_error("Base64 decoding error"))?
                };
                decoded.push((DecodedPart::Bytes(word), charset));
            }
            Some(other) => {
                return Err(SemanticaError::Other {
                    module: "builtins".to_string(),
                    type_name: "AssertionError".to_string(),
                    message: format!("Unexpected encoding: {other}"),
                });
            }
        }
    }
    let mut collapsed: Vec<(DecodedPart, Option<String>)> = Vec::new();
    let mut last_word: Option<Vec<u8>> = None;
    let mut last_charset: Option<String> = None;
    for (word, charset) in decoded {
        let word = match word {
            DecodedPart::Str(s) => encode_raw_unicode_escape(&s),
            DecodedPart::Bytes(b) => b,
        };
        match last_word.as_mut() {
            None => {
                last_word = Some(word);
                last_charset = charset;
            }
            Some(_) if charset != last_charset => {
                collapsed.push((
                    DecodedPart::Bytes(last_word.take().expect("word")),
                    last_charset.take(),
                ));
                last_word = Some(word);
                last_charset = charset;
            }
            Some(lw) => {
                if last_charset.is_none() {
                    lw.push(b' ');
                }
                lw.extend_from_slice(&word);
            }
        }
    }
    collapsed.push((
        DecodedPart::Bytes(last_word.unwrap_or_default()),
        last_charset,
    ));
    Ok(collapsed)
}

// ---------------------------------------------------------------------------- dates

/// A `datetime.datetime` as `parsedate_to_datetime` builds it (naive when `tz` is `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmailDateTime {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    /// UTC offset in seconds (`None` for `-0000`, i.e. a naive datetime).
    pub tz: Option<i64>,
}

impl EmailDateTime {
    /// `datetime.isoformat()`.
    pub fn isoformat(&self) -> String {
        let mut s = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        );
        if let Some(off) = self.tz {
            let sign = if off < 0 { '-' } else { '+' };
            let a = off.abs();
            let (hh, rest) = (a / 3600, a % 3600);
            let (mm, ss) = (rest / 60, rest % 60);
            s.push_str(&format!("{sign}{hh:02}:{mm:02}"));
            if ss != 0 {
                s.push_str(&format!(":{ss:02}"));
            }
        }
        s
    }
}

const MONTHNAMES: [&str; 24] = [
    "jan",
    "feb",
    "mar",
    "apr",
    "may",
    "jun",
    "jul",
    "aug",
    "sep",
    "oct",
    "nov",
    "dec",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
const DAYNAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

fn timezone_offset(name: &str) -> Option<i64> {
    Some(match name {
        "UT" | "UTC" | "GMT" | "Z" => 0,
        "AST" => -400,
        "ADT" => -300,
        "EST" => -500,
        "EDT" => -400,
        "CST" => -600,
        "CDT" => -500,
        "MST" => -700,
        "MDT" => -600,
        "PST" => -800,
        "PDT" => -700,
        _ => return None,
    })
}

/// `email._parseaddr._parsedate_tz(data)`: `[yy, mm, dd, thh, tmm, tss, 0, 1, -1, tz]` as
/// `(fields, tzoffset)`; `None` when unparseable.
fn parsedate_tz(data: &str) -> Option<([i64; 6], Option<i64>)> {
    if data.is_empty() {
        return None;
    }
    let mut data: Vec<String> = py_split_whitespace(data)
        .into_iter()
        .map(str::to_string)
        .collect();
    if data.is_empty() {
        return None;
    }
    if data[0].ends_with(',') || DAYNAMES.contains(&data[0].to_lowercase().as_str()) {
        data.remove(0);
    } else if let Some(i) = data[0].rfind(',') {
        data[0] = data[0][i + 1..].to_string();
    }
    if data.len() == 3 {
        let stuff: Vec<String> = data[0].split('-').map(str::to_string).collect();
        if stuff.len() == 3 {
            let mut v = stuff;
            v.extend(data[1..].iter().cloned());
            data = v;
        }
    }
    if data.len() == 4 {
        let s = data[3].clone();
        let mut i = s.find('+');
        if i.is_none() {
            i = s.find('-');
        }
        match i {
            Some(i) if i > 0 => {
                data.truncate(3);
                data.push(s[..i].to_string());
                data.push(s[i..].to_string());
            }
            _ => data.push(String::new()),
        }
    }
    if data.len() < 5 {
        return None;
    }
    data.truncate(5);
    let mut dd = data[0].clone();
    let mut mm = data[1].clone();
    let mut yy = data[2].clone();
    let mut tm = data[3].clone();
    let mut tz = data[4].clone();
    if dd.is_empty() || mm.is_empty() || yy.is_empty() {
        return None;
    }
    mm = mm.to_lowercase();
    if !MONTHNAMES.contains(&mm.as_str()) {
        let new_mm = dd.to_lowercase();
        dd = mm;
        mm = new_mm;
        if !MONTHNAMES.contains(&mm.as_str()) {
            return None;
        }
    }
    let mut month = MONTHNAMES
        .iter()
        .position(|m| *m == mm)
        .expect("known month") as i64
        + 1;
    if month > 12 {
        month -= 12;
    }
    if dd.ends_with(',') {
        dd.pop();
    }
    if let Some(i) = yy.find(':')
        && i > 0
    {
        std::mem::swap(&mut yy, &mut tm);
    }
    if yy.ends_with(',') {
        yy.pop();
        if yy.is_empty() {
            return None;
        }
    }
    if !yy.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        std::mem::swap(&mut yy, &mut tz);
    }
    if tm.ends_with(',') {
        tm.pop();
    }
    let parts: Vec<String> = tm.split(':').map(str::to_string).collect();
    let (thh, tmm, tss): (String, String, String) = match parts.len() {
        2 => (parts[0].clone(), parts[1].clone(), "0".to_string()),
        3 => (parts[0].clone(), parts[1].clone(), parts[2].clone()),
        1 if parts[0].contains('.') => {
            let p: Vec<String> = parts[0].split('.').map(str::to_string).collect();
            match p.len() {
                2 => (p[0].clone(), p[1].clone(), "0".to_string()),
                3 => (p[0].clone(), p[1].clone(), p[2].clone()),
                _ => return None,
            }
        }
        _ => return None,
    };
    let mut year = py_int(&yy)?;
    let day = py_int(&dd)?;
    let hour = py_int(&thh)?;
    let minute = py_int(&tmm)?;
    let second = py_int(&tss)?;
    if year < 100 {
        if year > 68 {
            year += 1900;
        } else {
            year += 2000;
        }
    }
    let tz_upper = tz.to_uppercase();
    let mut tzoffset: Option<i64> = timezone_offset(&tz_upper);
    if tzoffset.is_none() {
        tzoffset = py_int(&tz_upper);
        if tzoffset == Some(0) && tz_upper.starts_with('-') {
            tzoffset = None;
        }
    }
    if let Some(off) = tzoffset
        && off != 0
    {
        let (sign, a) = if off < 0 { (-1, -off) } else { (1, off) };
        tzoffset = Some(sign * ((a / 100) * 3600 + (a % 100) * 60));
    }
    Some(([year, month, day, hour, minute, second], tzoffset))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// `email.utils.parsedate_to_datetime(data)`; `Err` with the exception text otherwise
/// (`ValueError` for unparseable values or out-of-range fields).
pub fn parsedate_to_datetime(data: &str) -> Result<EmailDateTime, String> {
    let (f, tz) =
        parsedate_tz(data).ok_or_else(|| format!("Invalid date value or format \"{data}\""))?;
    let [year, month, day, hour, minute, second] = f;
    if !(1..=9999).contains(&year) {
        return Err(format!("year {year} is out of range"));
    }
    if !(1..=12).contains(&month) {
        return Err("month must be in 1..12".to_string());
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err("day is out of range for month".to_string());
    }
    if !(0..=23).contains(&hour) {
        return Err("hour must be in 0..23".to_string());
    }
    if !(0..=59).contains(&minute) {
        return Err("minute must be in 0..59".to_string());
    }
    if !(0..=59).contains(&second) {
        return Err("second must be in 0..59".to_string());
    }
    if let Some(off) = tz
        && off.abs() >= 86400
    {
        return Err(
            "offset must be a timedelta strictly between -timedelta(hours=24) and timedelta(hours=24)"
                .to_string(),
        );
    }
    Ok(EmailDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz,
    })
}

// ----------------------------------------------------------------------- parameters

/// `email.utils.unquote`.
pub fn unquote(s: &str) -> String {
    if s.chars().count() > 1 {
        if s.starts_with('"') && s.ends_with('"') {
            let inner = &s[1..s.len() - 1];
            return inner.replace("\\\\", "\\").replace("\\\"", "\"");
        }
        if s.starts_with('<') && s.ends_with('>') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// `email._parseaddr.quote`.
fn quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A parameter value: plain, or an RFC 2231 `(charset, language, value)` triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamValue {
    /// A plain value.
    Str(String),
    /// `(charset, language, value)`.
    Rfc2231(Option<String>, Option<String>, String),
}

/// `email.message._splitparam`.
fn splitparam(param: &str) -> (String, Option<String>) {
    match param.split_once(';') {
        None => (py_strip(param).to_string(), None),
        Some((a, b)) => (py_strip(a).to_string(), Some(py_strip(b).to_string())),
    }
}

/// `email.message._parseparam`.
fn parseparam(s: &str) -> Vec<String> {
    let mut s: String = format!(";{s}");
    let mut plist = Vec::new();
    while s.starts_with(';') {
        s = s[1..].to_string();
        let count_quotes =
            |t: &str| t.matches('"').count() as i64 - t.matches("\\\"").count() as i64;
        let mut end: i64 = s.find(';').map(|i| i as i64).unwrap_or(-1);
        while end > 0 && count_quotes(&s[..end as usize]) % 2 != 0 {
            end = s[end as usize + 1..]
                .find(';')
                .map(|i| i as i64 + end + 1)
                .unwrap_or(-1);
        }
        let end = if end < 0 { s.len() } else { end as usize };
        let mut f = s[..end].to_string();
        if let Some(i) = f.find('=') {
            f = format!(
                "{}={}",
                py_strip(&f[..i]).to_lowercase(),
                py_strip(&f[i + 1..])
            );
        }
        plist.push(py_strip(&f).to_string());
        s = s[end..].to_string();
    }
    plist
}

/// `urllib.parse.unquote(s, encoding="latin-1")`.
fn url_unquote_latin1(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // An ASCII run: percent-decode it to bytes and read them as latin-1.
        let start = i;
        while i < chars.len() && chars[i].is_ascii() {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        let bytes = run.as_bytes();
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j] == b'%'
                && j + 2 < bytes.len()
                && bytes[j + 1].is_ascii_hexdigit()
                && bytes[j + 2].is_ascii_hexdigit()
            {
                let v = u8::from_str_radix(&run[j + 1..j + 3], 16).expect("hex");
                out.push(v as char);
                j += 3;
            } else {
                out.push(bytes[j] as char);
                j += 1;
            }
        }
    }
    out
}

/// `email.utils.decode_rfc2231(s)`.
fn decode_rfc2231(s: &str) -> (Option<String>, Option<String>, String) {
    let parts: Vec<&str> = s.splitn(3, '\'').collect();
    if parts.len() <= 2 {
        return (None, None, s.to_string());
    }
    (
        Some(parts[0].to_string()),
        Some(parts[1].to_string()),
        parts[2].to_string(),
    )
}

static RFC2231_CONTINUATION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Za-z0-9_]+)\*(([0-9]+)\*?)?\n?\z").expect("valid regex"));

/// `email.utils.decode_params(params)`.
fn decode_params(params: Vec<(String, String)>) -> PResult<Vec<(String, ParamValue)>> {
    let mut iter = params.into_iter();
    let first = iter.next().expect("at least one parameter");
    let mut new_params = vec![(first.0, ParamValue::Str(first.1))];
    // name -> [(num, value, encoded)]
    let mut rfc2231: indexmap::IndexMap<String, Vec<(Option<i64>, String, bool)>> =
        indexmap::IndexMap::new();
    for (name, value) in iter {
        let encoded = name.ends_with('*');
        let value = unquote(&value);
        if let Some(caps) = RFC2231_CONTINUATION.captures(&name) {
            let base = caps[1].to_string();
            let num = caps
                .get(3)
                .map(|m| m.as_str().parse::<i64>().unwrap_or(i64::MAX));
            rfc2231.entry(base).or_default().push((num, value, encoded));
        } else {
            new_params.push((name, ParamValue::Str(format!("\"{}\"", quote(&value)))));
        }
    }
    for (name, mut continuations) in rfc2231 {
        let has_none = continuations.iter().any(|c| c.0.is_none());
        let has_some = continuations.iter().any(|c| c.0.is_some());
        if has_none && has_some {
            return Err(SemanticaError::Type(
                "'<' not supported between instances of 'int' and 'NoneType'".to_string(),
            ));
        }
        continuations.sort_by(|a, b| a.cmp(b));
        let mut value = String::new();
        let mut extended = false;
        for (_, s, encoded) in continuations {
            if encoded {
                value.push_str(&url_unquote_latin1(&s));
                extended = true;
            } else {
                value.push_str(&s);
            }
        }
        let value = quote(&value);
        if extended {
            let (charset, language, v) = decode_rfc2231(&value);
            new_params.push((
                name,
                ParamValue::Rfc2231(charset, language, format!("\"{v}\"")),
            ));
        } else {
            new_params.push((name, ParamValue::Str(format!("\"{value}\""))));
        }
    }
    Ok(new_params)
}

/// `email.utils.collapse_rfc2231_value(value)` (errors `"replace"`, fallback `us-ascii`).
pub fn collapse_rfc2231_value(value: &ParamValue) -> String {
    match value {
        ParamValue::Str(s) => unquote(s),
        ParamValue::Rfc2231(charset, _language, text) => {
            let charset = charset.clone().unwrap_or_else(|| "us-ascii".to_string());
            let rawbytes = encode_raw_unicode_escape(text);
            match bytes_decode(&rawbytes, &charset, DecodeErrors::Replace) {
                Ok(s) => s,
                Err(_) => unquote(text),
            }
        }
    }
}

// -------------------------------------------------------------------------- Message

/// A message payload: `None`, a string, or a list of sub-messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// No payload set.
    None,
    /// A string payload (surrogate escapes for raw bytes).
    Text(String),
    /// Sub-messages (multipart parts or the message inside a `message/*` part).
    Parts(Vec<Message>),
}

/// `email.message.Message` with the `compat32` policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    headers: Vec<(String, String)>,
    payload: Payload,
    unixfrom: Option<String>,
    /// `msg.preamble`.
    pub preamble: Option<String>,
    /// `msg.epilogue`.
    pub epilogue: Option<String>,
    /// `msg.defects` (class names of the registered defects).
    pub defects: Vec<String>,
    default_type: String,
}

impl Default for Message {
    fn default() -> Self {
        Message {
            headers: Vec::new(),
            payload: Payload::None,
            unixfrom: None,
            preamble: None,
            epilogue: None,
            defects: Vec::new(),
            default_type: "text/plain".to_string(),
        }
    }
}

impl Message {
    /// `Message()`.
    pub fn new() -> Self {
        Message::default()
    }

    /// `len(msg)`: the number of headers.
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// `len(msg) == 0`.
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// `msg.get(name, failobj=None)`: the first header with this name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<HeaderValue> {
        let lname = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lname)
            .map(|(_, v)| HeaderValue::fetch(v))
    }

    /// `name in msg`.
    pub fn contains(&self, name: &str) -> bool {
        let lname = name.to_lowercase();
        self.headers.iter().any(|(k, _)| k.to_lowercase() == lname)
    }

    /// `msg.items()`: all headers in order, values through `header_fetch_parse`.
    pub fn items(&self) -> Vec<(String, HeaderValue)> {
        self.headers
            .iter()
            .map(|(k, v)| (k.clone(), HeaderValue::fetch(v)))
            .collect()
    }

    /// `msg.keys()`.
    pub fn keys(&self) -> Vec<String> {
        self.headers.iter().map(|(k, _)| k.clone()).collect()
    }

    /// `msg.get_all(name)`.
    pub fn get_all(&self, name: &str) -> Option<Vec<HeaderValue>> {
        let lname = name.to_lowercase();
        let values: Vec<HeaderValue> = self
            .headers
            .iter()
            .filter(|(k, _)| k.to_lowercase() == lname)
            .map(|(_, v)| HeaderValue::fetch(v))
            .collect();
        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }

    /// `msg.set_raw(name, value)`.
    pub fn set_raw(&mut self, name: &str, value: &str) {
        self.headers.push((name.to_string(), value.to_string()));
    }

    /// `msg.get_unixfrom()`.
    pub fn get_unixfrom(&self) -> Option<&str> {
        self.unixfrom.as_deref()
    }

    /// `msg.is_multipart()`: the payload is a list.
    pub fn is_multipart(&self) -> bool {
        matches!(self.payload, Payload::Parts(_))
    }

    /// The payload (`msg.get_payload()` without decoding).
    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    /// The sub-messages of a multipart (or `message/*`) message.
    pub fn parts(&self) -> &[Message] {
        match &self.payload {
            Payload::Parts(parts) => parts,
            _ => &[],
        }
    }

    /// `msg.walk()`: this message and every sub-part, depth first.
    pub fn walk(&self) -> Vec<&Message> {
        let mut out = vec![self];
        if let Payload::Parts(parts) = &self.payload {
            for part in parts {
                out.extend(part.walk());
            }
        }
        out
    }

    /// `msg.get_default_type()`.
    pub fn get_default_type(&self) -> &str {
        &self.default_type
    }

    /// `msg.get_content_type()`.
    pub fn get_content_type(&self) -> String {
        match self.get("content-type") {
            None => self.default_type.clone(),
            Some(value) => {
                let ctype = splitparam(&value.to_string()).0.to_lowercase();
                if ctype.matches('/').count() != 1 {
                    "text/plain".to_string()
                } else {
                    ctype
                }
            }
        }
    }

    /// `msg.get_content_maintype()`.
    pub fn get_content_maintype(&self) -> String {
        self.get_content_type()
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    }

    /// `msg.get_content_subtype()`.
    pub fn get_content_subtype(&self) -> String {
        self.get_content_type()
            .split('/')
            .nth(1)
            .unwrap_or("")
            .to_string()
    }

    /// `msg._get_params_preserve(failobj, header)`.
    fn get_params_preserve(&self, header: &str) -> PResult<Option<Vec<(String, ParamValue)>>> {
        let Some(value) = self.get(header) else {
            return Ok(None);
        };
        let mut params = Vec::new();
        for p in parseparam(&value.to_string()) {
            match p.split_once('=') {
                Some((name, val)) => {
                    params.push((py_strip(name).to_string(), py_strip(val).to_string()));
                }
                None => params.push((py_strip(&p).to_string(), String::new())),
            }
        }
        decode_params(params).map(Some)
    }

    /// `msg.get_params(header=header)` (unquoted values).
    pub fn get_params(&self, header: &str) -> PResult<Option<Vec<(String, ParamValue)>>> {
        Ok(self.get_params_preserve(header)?.map(|params| {
            params
                .into_iter()
                .map(|(k, v)| (k, unquote_value(v)))
                .collect()
        }))
    }

    /// `msg.get_param(param, header=header)` (unquoted).
    pub fn get_param(&self, param: &str, header: &str) -> PResult<Option<ParamValue>> {
        if !self.contains(header) {
            return Ok(None);
        }
        let lparam = param.to_lowercase();
        if let Some(params) = self.get_params_preserve(header)? {
            for (k, v) in params {
                if k.to_lowercase() == lparam {
                    return Ok(Some(unquote_value(v)));
                }
            }
        }
        Ok(None)
    }

    /// `msg.get_filename()`.
    pub fn get_filename(&self) -> PResult<Option<String>> {
        let filename = match self.get_param("filename", "content-disposition")? {
            Some(f) => Some(f),
            None => self.get_param("name", "content-type")?,
        };
        Ok(filename.map(|f| py_strip(&collapse_rfc2231_value(&f)).to_string()))
    }

    /// `msg.get_boundary()`.
    pub fn get_boundary(&self) -> PResult<Option<String>> {
        Ok(self
            .get_param("boundary", "content-type")?
            .map(|b| py_rstrip(&collapse_rfc2231_value(&b)).to_string()))
    }

    /// `msg.get_content_charset()`: the lower-cased charset parameter (`None` when missing
    /// or not ASCII).
    pub fn get_content_charset(&self) -> PResult<Option<String>> {
        let Some(charset) = self.get_param("charset", "content-type")? else {
            return Ok(None);
        };
        let charset = match charset {
            ParamValue::Str(s) => s,
            ParamValue::Rfc2231(cs, _lang, text) => {
                let pcharset = cs
                    .filter(|c| !c.is_empty())
                    .unwrap_or_else(|| "us-ascii".to_string());
                let as_bytes = encode_raw_unicode_escape(&text);
                match bytes_decode(&as_bytes, &pcharset, DecodeErrors::Strict) {
                    Ok(s) => s,
                    Err(_) => text,
                }
            }
        };
        if !charset.is_ascii() {
            return Ok(None);
        }
        Ok(Some(charset.to_lowercase()))
    }

    /// `msg.get_payload(decode=True)`: the payload decoded by its
    /// `Content-Transfer-Encoding` (`None` for multiparts).
    pub fn get_payload_decoded(&self) -> Option<Vec<u8>> {
        let payload = match &self.payload {
            Payload::Parts(_) => return None,
            Payload::None => return None,
            Payload::Text(s) => s,
        };
        let cte = self
            .get("content-transfer-encoding")
            .map(|v| v.to_string())
            .unwrap_or_default()
            .to_lowercase();
        let bpayload =
            encode_surrogateescape(payload).unwrap_or_else(|| encode_raw_unicode_escape(payload));
        match cte.as_str() {
            "quoted-printable" => Some(a2b_qp(&bpayload, false)),
            "base64" => {
                let joined: Vec<u8> = bytes_splitlines(&bpayload).concat();
                Some(decode_b(&joined))
            }
            "x-uuencode" | "uuencode" | "uue" | "x-uue" => {
                Some(decode_uu(&bpayload).unwrap_or(bpayload))
            }
            _ => Some(bpayload),
        }
    }

    /// `msg.get_payload()` (no decoding) for a string payload: text carrying raw 8-bit
    /// bytes is decoded with the `charset` parameter (`ascii` by default, `ascii` for
    /// unknown codecs) replacing errors. `None` for multiparts and unset payloads.
    pub fn get_payload_text(&self) -> PResult<Option<String>> {
        let Payload::Text(payload) = &self.payload else {
            return Ok(None);
        };
        if !has_surrogates(payload) {
            return Ok(Some(payload.clone()));
        }
        let Some(bpayload) = encode_surrogateescape(payload) else {
            return Ok(Some(payload.clone()));
        };
        let charset = match self.get_param("charset", "content-type")? {
            None => "ascii".to_string(),
            Some(ParamValue::Str(s)) => s,
            Some(ParamValue::Rfc2231(..)) => {
                return Err(SemanticaError::Type(
                    "decode() argument 'encoding' must be str, not tuple".to_string(),
                ));
            }
        };
        match bytes_decode(&bpayload, &charset, DecodeErrors::Replace) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.type_name() == "LookupError" => Ok(Some(decode_ascii_replace(&bpayload))),
            Err(e) => Err(e),
        }
    }
}

fn unquote_value(v: ParamValue) -> ParamValue {
    match v {
        ParamValue::Str(s) => ParamValue::Str(unquote(&s)),
        ParamValue::Rfc2231(c, l, t) => ParamValue::Rfc2231(c, l, unquote(&t)),
    }
}

// ----------------------------------------------------------------------- feedparser

/// A false-EOF predicate pushed by the parser.
#[derive(Debug, Clone)]
enum EofMatcher {
    /// `NLCRE.match` (blank line; `message/delivery-status` blocks).
    BlankLine,
    /// `boundaryre.match` for a multipart boundary.
    Boundary(Regex),
}

impl EofMatcher {
    fn matches(&self, line: &str) -> bool {
        match self {
            EofMatcher::BlankLine => leading_linesep(line).is_some(),
            EofMatcher::Boundary(re) => re.is_match(line),
        }
    }
}

/// `email.feedparser.BufferedSubFile` with all input already pushed and closed.
struct BufferedSubFile {
    lines: VecDeque<String>,
    eofstack: Vec<EofMatcher>,
}

impl BufferedSubFile {
    fn readline(&mut self) -> String {
        let Some(line) = self.lines.pop_front() else {
            return String::new();
        };
        for ateof in self.eofstack.iter().rev() {
            if ateof.matches(&line) {
                self.lines.push_front(line);
                return String::new();
            }
        }
        line
    }

    fn unreadline(&mut self, line: String) {
        self.lines.push_front(line);
    }
}

fn header_re_match(line: &str) -> bool {
    if line.starts_with("From ") {
        return true;
    }
    if line.starts_with('\t') || line.starts_with(' ') {
        return true;
    }
    for c in line.chars() {
        if c == ':' {
            return true;
        }
        let cp = c as u32;
        if !((0x21..=0x39).contains(&cp) || (0x3b..=0x7e).contains(&cp)) {
            return false;
        }
    }
    false
}

/// `email.feedparser.FeedParser` over a fully available input (`Parser.parsestr`).
struct FeedParser {
    input: BufferedSubFile,
    arena: Vec<Message>,
    children: Vec<Vec<usize>>,
    msgstack: Vec<usize>,
    cur: Option<usize>,
    last: Option<usize>,
}

impl FeedParser {
    fn new(text: &str) -> Self {
        FeedParser {
            input: BufferedSubFile {
                lines: split_keep_ends(text).into(),
                eofstack: Vec::new(),
            },
            arena: Vec::new(),
            children: Vec::new(),
            msgstack: Vec::new(),
            cur: None,
            last: None,
        }
    }

    fn defect(&mut self, idx: usize, name: &str) {
        self.arena[idx].defects.push(name.to_string());
    }

    fn new_message(&mut self) {
        let mut msg = Message::new();
        if let Some(cur) = self.cur {
            let parent = &self.arena[cur];
            if !parent.is_empty() && parent.get_content_type() == "multipart/digest" {
                msg.default_type = "message/rfc822".to_string();
            }
        }
        let idx = self.arena.len();
        self.arena.push(msg);
        self.children.push(Vec::new());
        if let Some(&top) = self.msgstack.last() {
            self.children[top].push(idx);
            // `attach` makes the parent's payload a list.
            if !matches!(self.arena[top].payload, Payload::Parts(_)) {
                self.arena[top].payload = Payload::Parts(Vec::new());
            }
        }
        self.msgstack.push(idx);
        self.cur = Some(idx);
        self.last = Some(idx);
    }

    fn pop_message(&mut self) -> usize {
        let retval = self.msgstack.pop().expect("message stack");
        self.cur = self.msgstack.last().copied();
        retval
    }

    fn set_payload(&mut self, idx: usize, text: String) {
        self.arena[idx].payload = Payload::Text(text);
        // A string payload replaces any attached parts.
        self.children[idx].clear();
    }

    fn parse(&mut self) -> PResult<()> {
        self.new_message();
        let cur = self.cur.expect("current message");
        let mut headers = Vec::new();
        loop {
            let line = self.input.readline();
            if line.is_empty() {
                break;
            }
            if !header_re_match(&line) {
                if leading_linesep(&line).is_none() {
                    self.defect(cur, "MissingHeaderBodySeparatorDefect");
                    self.input.unreadline(line);
                }
                break;
            }
            headers.push(line);
        }
        self.parse_headers(cur, headers);
        let ctype = self.arena[cur].get_content_type();
        if ctype == "message/delivery-status" {
            loop {
                self.input.eofstack.push(EofMatcher::BlankLine);
                self.parse()?;
                self.pop_message();
                self.input.eofstack.pop();
                let _ = self.input.readline();
                let line = self.input.readline();
                if line.is_empty() {
                    break;
                }
                self.input.unreadline(line);
            }
            return Ok(());
        }
        let maintype = ctype.split('/').next().unwrap_or("").to_string();
        if maintype == "message" {
            self.parse()?;
            self.pop_message();
            return Ok(());
        }
        if maintype == "multipart" {
            return self.parse_multipart(cur);
        }
        let mut lines = Vec::new();
        loop {
            let line = self.input.readline();
            if line.is_empty() {
                break;
            }
            lines.push(line);
        }
        self.set_payload(cur, lines.concat());
        Ok(())
    }

    fn parse_multipart(&mut self, cur: usize) -> PResult<()> {
        let Some(boundary) = self.arena[cur].get_boundary()? else {
            self.defect(cur, "NoBoundaryInMultipartDefect");
            let mut lines = Vec::new();
            loop {
                let line = self.input.readline();
                if line.is_empty() {
                    break;
                }
                lines.push(line);
            }
            self.set_payload(cur, lines.concat());
            return Ok(());
        };
        let cte = self.arena[cur]
            .get("content-transfer-encoding")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "8bit".to_string())
            .to_lowercase();
        if !["7bit", "8bit", "binary"].contains(&cte.as_str()) {
            self.defect(cur, "InvalidMultipartContentTransferEncodingDefect");
        }
        let separator = format!("--{boundary}");
        let boundaryre = Regex::new(&format!(
            r"^(?:{})(--)?[ \t]*(\r\n|\r|\n)?\n?\z",
            regex::escape(&separator)
        ))
        .expect("valid boundary regex");
        let mut capturing_preamble = true;
        let mut preamble: Vec<String> = Vec::new();
        let mut linesep = false;
        let mut close_boundary_seen = false;
        loop {
            let line = self.input.readline();
            if line.is_empty() {
                break;
            }
            if let Some(mo) = boundaryre.captures(&line) {
                if mo.get(1).is_some() {
                    close_boundary_seen = true;
                    linesep = mo.get(2).is_some_and(|m| !m.as_str().is_empty());
                    break;
                }
                if capturing_preamble {
                    if !preamble.is_empty() {
                        let last = preamble.last_mut().expect("preamble line");
                        if let Some(sep) = trailing_linesep(last) {
                            last.truncate(last.len() - sep.len());
                        }
                        self.arena[cur].preamble = Some(preamble.concat());
                    }
                    capturing_preamble = false;
                    self.input.unreadline(line);
                    continue;
                }
                loop {
                    let line = self.input.readline();
                    if !boundaryre.is_match(&line) {
                        self.input.unreadline(line);
                        break;
                    }
                }
                self.input
                    .eofstack
                    .push(EofMatcher::Boundary(boundaryre.clone()));
                self.parse()?;
                let last = self.last.expect("last message");
                if self.arena[last].get_content_maintype() == "multipart" {
                    let epilogue = self.arena[last].epilogue.clone();
                    match epilogue {
                        Some(e) if e.is_empty() => self.arena[last].epilogue = None,
                        Some(mut e) => {
                            if let Some(sep) = trailing_linesep(&e) {
                                e.truncate(e.len() - sep.len());
                                self.arena[last].epilogue = Some(e);
                            }
                        }
                        None => {}
                    }
                } else if let Payload::Text(payload) = &mut self.arena[last].payload
                    && let Some(sep) = trailing_linesep(payload)
                {
                    let n = payload.len() - sep.len();
                    payload.truncate(n);
                }
                self.input.eofstack.pop();
                self.pop_message();
                self.last = self.cur;
            } else {
                if !capturing_preamble {
                    return Err(SemanticaError::Other {
                        module: "builtins".to_string(),
                        type_name: "AssertionError".to_string(),
                        message: String::new(),
                    });
                }
                preamble.push(line);
            }
        }
        if capturing_preamble {
            self.defect(cur, "StartBoundaryNotFoundDefect");
            self.set_payload(cur, preamble.concat());
            loop {
                let line = self.input.readline();
                if line.is_empty() {
                    break;
                }
            }
            self.arena[cur].epilogue = Some(String::new());
            return Ok(());
        }
        if !close_boundary_seen {
            self.defect(cur, "CloseBoundaryNotFoundDefect");
            return Ok(());
        }
        let mut epilogue: Vec<String> = if linesep {
            vec![String::new()]
        } else {
            Vec::new()
        };
        loop {
            let line = self.input.readline();
            if line.is_empty() {
                break;
            }
            epilogue.push(line);
        }
        if let Some(first) = epilogue.first_mut()
            && let Some(sep) = leading_linesep(first)
        {
            *first = first[sep.len()..].to_string();
        }
        self.arena[cur].epilogue = Some(epilogue.concat());
        Ok(())
    }

    fn parse_headers(&mut self, cur: usize, lines: Vec<String>) {
        let mut lastheader = String::new();
        let mut lastvalue: Vec<String> = Vec::new();
        let n = lines.len();
        for (lineno, line) in lines.into_iter().enumerate() {
            if line.starts_with(' ') || line.starts_with('\t') {
                if lastheader.is_empty() {
                    self.defect(cur, "FirstHeaderLineIsContinuationDefect");
                    continue;
                }
                lastvalue.push(line);
                continue;
            }
            if !lastheader.is_empty() {
                let (name, value) = header_source_parse(&lastvalue);
                self.arena[cur].set_raw(&name, &value);
                lastheader.clear();
                lastvalue.clear();
            }
            if line.starts_with("From ") {
                if lineno == 0 {
                    let mut l = line;
                    if let Some(sep) = trailing_linesep(&l) {
                        l.truncate(l.len() - sep.len());
                    }
                    self.arena[cur].unixfrom = Some(l);
                    continue;
                } else if lineno == n - 1 {
                    self.input.unreadline(line);
                    return;
                } else {
                    self.defect(cur, "MisplacedEnvelopeHeaderDefect");
                    continue;
                }
            }
            let i = line.find(':').unwrap_or(0);
            if i == 0 {
                self.defect(cur, "InvalidHeaderDefect");
                continue;
            }
            lastheader = line[..i].to_string();
            lastvalue = vec![line];
        }
        if !lastheader.is_empty() {
            let (name, value) = header_source_parse(&lastvalue);
            self.arena[cur].set_raw(&name, &value);
        }
    }

    /// Assemble the arena into the message tree rooted at `idx`.
    fn build(&mut self, idx: usize) -> Message {
        let children = std::mem::take(&mut self.children[idx]);
        let mut msg = std::mem::take(&mut self.arena[idx]);
        if matches!(msg.payload, Payload::Parts(_)) {
            let parts: Vec<Message> = children.into_iter().map(|c| self.build(c)).collect();
            msg.payload = Payload::Parts(parts);
        }
        msg
    }
}

/// `Compat32.header_source_parse(sourcelines)`.
fn header_source_parse(sourcelines: &[String]) -> (String, String) {
    let first = &sourcelines[0];
    let (name, rest) = first.split_once(':').unwrap_or((first.as_str(), ""));
    let mut value = rest.trim_start_matches([' ', '\t']).to_string();
    for line in &sourcelines[1..] {
        value.push_str(line);
    }
    let value = value.trim_end_matches(['\r', '\n']).to_string();
    (name.to_string(), value)
}

/// `email.message_from_string(s)` (compat32).
pub fn message_from_string(s: &str) -> PResult<Message> {
    let mut parser = FeedParser::new(s);
    parser.parse()?;
    let root = parser.pop_message();
    let mut msg = parser.build(root);
    if msg.get_content_maintype() == "multipart" && !msg.is_multipart() {
        msg.defects
            .push("MultipartInvariantViolationDefect".to_string());
    }
    Ok(msg)
}

/// `email.message_from_bytes(b)` (compat32): the bytes are read as ASCII with
/// surrogate escapes.
pub fn message_from_bytes(b: &[u8]) -> PResult<Message> {
    message_from_string(&decode_surrogateescape(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(
            py_splitlines("a\r\nb\rc\nd\x0be"),
            vec!["a", "b", "c", "d", "e"]
        );
        assert_eq!(py_splitlines("a\n"), vec!["a"]);
        assert_eq!(
            split_keep_ends("a\r\nb\rc\nd"),
            vec!["a\r\n", "b\r", "c\n", "d"]
        );
    }

    #[test]
    fn base64_modes() {
        assert_eq!(a2b_base64(b"aGk=", true).unwrap(), b"hi");
        assert_eq!(a2b_base64(b"aG k=", false).unwrap(), b"hi");
        assert!(a2b_base64(b"aG k=", true).is_err());
        assert_eq!(a2b_base64(b"aGk", false).unwrap_err(), "Incorrect padding");
        assert_eq!(decode_b(b"aGk"), b"hi");
        assert_eq!(decode_b(b"a"), b"a");
    }

    #[test]
    fn qp_decoding() {
        assert_eq!(a2b_qp(b"a=3Db=\r\nc==d=4", false), b"a=bc=d=4");
        assert_eq!(a2b_qp(b"a_b", true), b"a b");
    }

    fn oracle() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/email/oracle.json"))
            .expect("oracle fixture")
    }

    fn bytes_of(v: &serde_json::Value) -> Vec<u8> {
        v.as_array()
            .expect("byte array")
            .iter()
            .map(|b| b.as_u64().expect("byte") as u8)
            .collect()
    }

    #[test]
    fn base64_matches_oracle() {
        let oracle = oracle();
        for row in oracle["base64"].as_array().expect("rows") {
            let input = bytes_of(&row["input"]);
            for (key, strict) in [("lenient", false), ("strict", true)] {
                let expected = &row[key];
                match a2b_base64(&input, strict) {
                    Ok(v) => assert_eq!(v, bytes_of(&expected["ok"]), "{input:?} {key}"),
                    Err(msg) => {
                        assert_eq!(expected["error"], "Error", "{input:?} {key}");
                        assert_eq!(expected["message"], msg.as_str(), "{input:?} {key}");
                    }
                }
            }
            assert_eq!(
                decode_b(&input),
                bytes_of(&row["decode_b"]),
                "{input:?} decode_b"
            );
        }
    }

    #[test]
    fn qp_matches_oracle() {
        let oracle = oracle();
        for row in oracle["qp"].as_array().expect("rows") {
            let input = bytes_of(&row["input"]);
            assert_eq!(a2b_qp(&input, false), bytes_of(&row["body"]), "{input:?}");
            assert_eq!(
                a2b_qp(&input, true),
                bytes_of(&row["header"]),
                "{input:?} header"
            );
        }
    }

    #[test]
    fn codec_names_follow_python() {
        assert_eq!(normalize_encoding("UTF-8"), "utf_8");
        assert_eq!(normalize_encoding(" utf\"--8 "), "utf_8");
        assert_eq!(normalize_encoding("ANSI_X3.4-1968"), "ansi_x3.4_1968");
        assert_eq!(
            bytes_decode(b"caf\xc3\xa9", "utf\"-8", DecodeErrors::Strict).unwrap(),
            "caf\u{e9}"
        );
        assert_eq!(
            bytes_decode(b"\xfd", "latin5", DecodeErrors::Strict).unwrap(),
            "\u{131}"
        );
        assert_eq!(
            bytes_decode(b"x", "x-nope", DecodeErrors::Ignore)
                .unwrap_err()
                .to_string(),
            "unknown encoding: x-nope"
        );
    }

    #[test]
    fn dates() {
        let dt = parsedate_to_datetime("Mon, 1 Jan 2024 10:00:00 +0530").unwrap();
        assert_eq!(dt.isoformat(), "2024-01-01T10:00:00+05:30");
        let dt = parsedate_to_datetime("1 Jan 24 10:00 -0000").unwrap();
        assert_eq!(dt.isoformat(), "2024-01-01T10:00:00");
        assert!(parsedate_to_datetime("nonsense").is_err());
    }
}
