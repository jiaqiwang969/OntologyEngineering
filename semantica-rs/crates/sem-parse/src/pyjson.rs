//! `json.loads` as CPython 3.11 implements it (the `_json` C scanner): the same accepted
//! documents, the same values (insertion-ordered objects, last duplicate key wins) and the
//! same `JSONDecodeError` messages and positions (`Expecting value: line 1 column 1 (char
//! 0)`), which the parsers embed in their own error messages.
//!
//! Positions count code points, as Python's do. Values Python can produce but a
//! `serde_json::Value` cannot hold are mapped as follows (see `PORT-NOTES.md`): the constants
//! `NaN`/`Infinity`/`-Infinity` and out-of-range floats become `null`, integers beyond the
//! 64-bit range become floats, and lone surrogate escapes (`"\ud800"`) become U+FFFD.

use std::fmt;

use serde_json::{Map, Number, Value};

use sem_utils::SemanticaError;

/// `json.JSONDecodeError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JsonDecodeError {
    /// `e.msg`.
    pub msg: String,
    /// `e.pos` (code points).
    pub pos: usize,
    /// `e.lineno`.
    pub lineno: usize,
    /// `e.colno`.
    pub colno: usize,
    /// A `RecursionError` (nesting deeper than Python's recursion limit) rather than a
    /// `JSONDecodeError`.
    pub recursion: bool,
}

impl JsonDecodeError {
    fn new(msg: &str, doc: &[char], pos: usize) -> Self {
        let before = &doc[..pos.min(doc.len())];
        let lineno = before.iter().filter(|&&c| c == '\n').count() + 1;
        let colno = match before.iter().rposition(|&c| c == '\n') {
            Some(nl) => pos - nl,
            None => pos + 1,
        };
        JsonDecodeError {
            msg: msg.to_string(),
            pos,
            lineno,
            colno,
            recursion: false,
        }
    }

    fn recursion(kind: &str, pos: usize) -> Self {
        JsonDecodeError {
            msg: format!(
                "maximum recursion depth exceeded while decoding a JSON {kind} from a unicode string"
            ),
            pos,
            lineno: 0,
            colno: 0,
            recursion: true,
        }
    }
}

impl fmt::Display for JsonDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.recursion {
            return f.write_str(&self.msg);
        }
        write!(
            f,
            "{}: line {} column {} (char {})",
            self.msg, self.lineno, self.colno, self.pos
        )
    }
}

impl From<JsonDecodeError> for SemanticaError {
    fn from(e: JsonDecodeError) -> Self {
        if e.recursion {
            SemanticaError::Other {
                module: "builtins".to_string(),
                type_name: "RecursionError".to_string(),
                message: e.msg,
            }
        } else {
            SemanticaError::JsonDecode(e.to_string())
        }
    }
}

/// Raised internally when no JSON value starts at an index (`StopIteration(idx)`).
enum ScanError {
    Stop(usize),
    Decode(JsonDecodeError),
}

impl From<JsonDecodeError> for ScanError {
    fn from(e: JsonDecodeError) -> Self {
        ScanError::Decode(e)
    }
}

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// `json.loads(s)`.
pub(crate) fn loads(s: &str) -> Result<Value, JsonDecodeError> {
    let doc: Vec<char> = s.chars().collect();
    if doc.first() == Some(&'\u{feff}') {
        return Err(JsonDecodeError::new(
            "Unexpected UTF-8 BOM (decode using utf-8-sig)",
            &doc,
            0,
        ));
    }
    let mut idx = 0;
    while idx < doc.len() && is_ws(doc[idx]) {
        idx += 1;
    }
    let scanner = Scanner { doc: &doc };
    let (value, mut end) = match scanner.scan_once(idx, 0) {
        Ok(v) => v,
        Err(ScanError::Stop(at)) => return Err(JsonDecodeError::new("Expecting value", &doc, at)),
        Err(ScanError::Decode(e)) => return Err(e),
    };
    while end < doc.len() && is_ws(doc[end]) {
        end += 1;
    }
    if end != doc.len() {
        return Err(JsonDecodeError::new("Extra data", &doc, end));
    }
    Ok(value)
}

struct Scanner<'a> {
    doc: &'a [char],
}

/// Nesting depth at which Python's recursion guard would fire; deeper documents raise
/// `RecursionError` in Python.
const MAX_DEPTH: usize = 990;

impl Scanner<'_> {
    fn at(&self, i: usize) -> Option<char> {
        self.doc.get(i).copied()
    }

    fn starts_with(&self, i: usize, lit: &str) -> bool {
        let lit: Vec<char> = lit.chars().collect();
        self.doc.len() >= i + lit.len() && self.doc[i..i + lit.len()] == lit[..]
    }

    fn scan_once(&self, idx: usize, depth: usize) -> Result<(Value, usize), ScanError> {
        let Some(c) = self.at(idx) else {
            return Err(ScanError::Stop(idx));
        };
        match c {
            '"' => {
                let (s, end) = self.scanstring(idx + 1)?;
                return Ok((Value::String(s), end));
            }
            '{' => {
                if depth >= MAX_DEPTH {
                    return Err(ScanError::Decode(JsonDecodeError::recursion("object", idx)));
                }
                return self.parse_object(idx + 1, depth + 1);
            }
            '[' => {
                if depth >= MAX_DEPTH {
                    return Err(ScanError::Decode(JsonDecodeError::recursion("array", idx)));
                }
                return self.parse_array(idx + 1, depth + 1);
            }
            'n' if self.starts_with(idx, "null") => return Ok((Value::Null, idx + 4)),
            't' if self.starts_with(idx, "true") => return Ok((Value::Bool(true), idx + 4)),
            'f' if self.starts_with(idx, "false") => return Ok((Value::Bool(false), idx + 5)),
            'N' if self.starts_with(idx, "NaN") => return Ok((Value::Null, idx + 3)),
            'I' if self.starts_with(idx, "Infinity") => return Ok((Value::Null, idx + 8)),
            '-' if self.starts_with(idx, "-Infinity") => return Ok((Value::Null, idx + 9)),
            _ => {}
        }
        self.match_number(idx)
    }

    fn match_number(&self, start: usize) -> Result<(Value, usize), ScanError> {
        let digit = |i: usize| self.at(i).is_some_and(|c| c.is_ascii_digit());
        let mut idx = start;
        if self.at(idx) == Some('-') {
            idx += 1;
            if idx >= self.doc.len() {
                return Err(ScanError::Stop(start));
            }
        }
        match self.at(idx) {
            Some('1'..='9') => {
                idx += 1;
                while digit(idx) {
                    idx += 1;
                }
            }
            Some('0') => idx += 1,
            _ => return Err(ScanError::Stop(start)),
        }
        let mut is_float = false;
        if self.at(idx) == Some('.') && digit(idx + 1) {
            is_float = true;
            idx += 2;
            while digit(idx) {
                idx += 1;
            }
        }
        if matches!(self.at(idx), Some('e' | 'E')) && idx + 1 < self.doc.len() {
            let e_start = idx;
            idx += 1;
            if matches!(self.at(idx), Some('-' | '+')) && idx + 1 < self.doc.len() {
                idx += 1;
            }
            while digit(idx) {
                idx += 1;
            }
            if digit(idx - 1) {
                is_float = true;
            } else {
                idx = e_start;
            }
        }
        let text: String = self.doc[start..idx].iter().collect();
        let value = if is_float {
            match text.parse::<f64>() {
                Ok(f) => Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        } else if let Ok(i) = text.parse::<i64>() {
            Value::from(i)
        } else if let Ok(u) = text.parse::<u64>() {
            Value::from(u)
        } else {
            text.parse::<f64>()
                .ok()
                .and_then(Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        };
        Ok((value, idx))
    }

    fn skip_ws(&self, mut idx: usize) -> usize {
        while self.at(idx).is_some_and(is_ws) {
            idx += 1;
        }
        idx
    }

    fn parse_object(&self, idx: usize, depth: usize) -> Result<(Value, usize), ScanError> {
        let mut map = Map::new();
        let mut idx = self.skip_ws(idx);
        if self.at(idx) != Some('}') {
            loop {
                if self.at(idx) != Some('"') {
                    return Err(JsonDecodeError::new(
                        "Expecting property name enclosed in double quotes",
                        self.doc,
                        idx,
                    )
                    .into());
                }
                let (key, next) = self.scanstring(idx + 1)?;
                idx = self.skip_ws(next);
                if self.at(idx) != Some(':') {
                    return Err(
                        JsonDecodeError::new("Expecting ':' delimiter", self.doc, idx).into(),
                    );
                }
                idx = self.skip_ws(idx + 1);
                let (value, next) = self.scan_once(idx, depth)?;
                map.insert(key, value);
                idx = self.skip_ws(next);
                if self.at(idx) == Some('}') {
                    break;
                }
                if self.at(idx) != Some(',') {
                    return Err(
                        JsonDecodeError::new("Expecting ',' delimiter", self.doc, idx).into(),
                    );
                }
                idx = self.skip_ws(idx + 1);
            }
        }
        Ok((Value::Object(map), idx + 1))
    }

    fn parse_array(&self, idx: usize, depth: usize) -> Result<(Value, usize), ScanError> {
        let mut items = Vec::new();
        let mut idx = self.skip_ws(idx);
        if self.at(idx) != Some(']') {
            loop {
                let (value, next) = self.scan_once(idx, depth)?;
                items.push(value);
                idx = self.skip_ws(next);
                if self.at(idx) == Some(']') {
                    break;
                }
                if self.at(idx) != Some(',') {
                    return Err(
                        JsonDecodeError::new("Expecting ',' delimiter", self.doc, idx).into(),
                    );
                }
                idx = self.skip_ws(idx + 1);
            }
        }
        Ok((Value::Array(items), idx + 1))
    }

    /// `scanstring(s, end)`: the string starting after the opening quote at `end - 1`.
    fn scanstring(&self, end: usize) -> Result<(String, usize), JsonDecodeError> {
        let doc = self.doc;
        let len = doc.len();
        let begin = end - 1;
        let mut out = String::new();
        let mut end = end;
        loop {
            let mut next = end;
            let mut c = '\0';
            while next < len {
                c = doc[next];
                if c == '"' || c == '\\' {
                    break;
                }
                if (c as u32) <= 0x1f {
                    return Err(JsonDecodeError::new(
                        "Invalid control character at",
                        doc,
                        next,
                    ));
                }
                next += 1;
            }
            if next >= len || !(c == '"' || c == '\\') {
                return Err(JsonDecodeError::new(
                    "Unterminated string starting at",
                    doc,
                    begin,
                ));
            }
            out.extend(&doc[end..next]);
            next += 1;
            if c == '"' {
                end = next;
                break;
            }
            if next == len {
                return Err(JsonDecodeError::new(
                    "Unterminated string starting at",
                    doc,
                    begin,
                ));
            }
            let e = doc[next];
            if e != 'u' {
                end = next + 1;
                let decoded = match e {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => return Err(JsonDecodeError::new("Invalid \\escape", doc, end - 2)),
                };
                out.push(decoded);
            } else {
                next += 1;
                end = next + 4;
                if end >= len {
                    return Err(JsonDecodeError::new(
                        "Invalid \\uXXXX escape",
                        doc,
                        next - 1,
                    ));
                }
                let mut code: u32 = 0;
                while next < end {
                    let d = doc[next].to_digit(16).filter(|_| doc[next].is_ascii());
                    match d {
                        Some(v) => code = (code << 4) | v,
                        None => {
                            return Err(JsonDecodeError::new(
                                "Invalid \\uXXXX escape",
                                doc,
                                end - 5,
                            ));
                        }
                    }
                    next += 1;
                }
                if (0xd800..0xdc00).contains(&code)
                    && end + 6 < len
                    && doc[next] == '\\'
                    && doc[next + 1] == 'u'
                {
                    next += 2;
                    let mut low: u32 = 0;
                    let low_end = end + 6;
                    while next < low_end {
                        let d = doc[next].to_digit(16).filter(|_| doc[next].is_ascii());
                        match d {
                            Some(v) => low = (low << 4) | v,
                            None => {
                                return Err(JsonDecodeError::new(
                                    "Invalid \\uXXXX escape",
                                    doc,
                                    low_end - 5,
                                ));
                            }
                        }
                        next += 1;
                    }
                    if (0xdc00..0xe000).contains(&low) {
                        code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
                        end = low_end;
                    }
                }
                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
            }
        }
        Ok((out, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn err(s: &str) -> String {
        loads(s).unwrap_err().to_string()
    }

    #[test]
    fn decodes_like_python() {
        assert_eq!(
            loads(r#"{"a": [1, 2.5, "x\u00e9", null, true]}"#).unwrap(),
            json!({"a": [1, 2.5, "xé", null, true]})
        );
        assert_eq!(
            loads(r#"{"a": 1, "b": 2, "a": 3}"#).unwrap().to_string(),
            r#"{"a":3,"b":2}"#
        );
        assert_eq!(loads("\"\\ud83d\\ude00\"").unwrap(), json!("😀"));
        assert_eq!(loads(" 1e5 ").unwrap(), json!(100000.0));
    }

    #[test]
    fn errors_like_python() {
        assert_eq!(err(""), "Expecting value: line 1 column 1 (char 0)");
        assert_eq!(
            err("{"),
            "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"
        );
        assert_eq!(
            err("{\"a\" 1}"),
            "Expecting ':' delimiter: line 1 column 6 (char 5)"
        );
        assert_eq!(
            err("[1 2]"),
            "Expecting ',' delimiter: line 1 column 4 (char 3)"
        );
        assert_eq!(err("[1,]"), "Expecting value: line 1 column 4 (char 3)");
        assert_eq!(
            err("{\"a\":1,}"),
            "Expecting property name enclosed in double quotes: line 1 column 8 (char 7)"
        );
        assert_eq!(
            err("\"abc"),
            "Unterminated string starting at: line 1 column 1 (char 0)"
        );
        assert_eq!(
            err("\"a\tb\""),
            "Invalid control character at: line 1 column 3 (char 2)"
        );
        assert_eq!(err("\"\\x\""), "Invalid \\escape: line 1 column 2 (char 1)");
        assert_eq!(err("{}\n x"), "Extra data: line 2 column 2 (char 4)");
        assert_eq!(
            err("\u{feff}{}"),
            "Unexpected UTF-8 BOM (decode using utf-8-sig): line 1 column 1 (char 0)"
        );
        assert_eq!(err("-"), "Expecting value: line 1 column 1 (char 0)");
        assert_eq!(err("01"), "Extra data: line 1 column 2 (char 1)");
    }
}
