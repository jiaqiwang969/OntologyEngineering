//! `json.loads` with CPython's behaviour: the same accepted syntax (including `NaN` /
//! `Infinity`) and the same `JSONDecodeError` messages and positions
//! (`"Expecting value: line 1 column 1 (char 0)"`), following the C scanner of CPython 3.11.
//!
//! Values JSON cannot hold in a `serde_json::Value` are approximated: integers beyond 64 bits
//! become floats, non-finite floats become `null`, lone surrogates become U+FFFD.

use serde_json::{Map, Number, Value};

/// A `json.JSONDecodeError`: `msg`, the document position (in characters) and the derived
/// line / column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonDecodeError {
    /// `msg`.
    pub msg: String,
    /// `pos`.
    pub pos: usize,
    /// `lineno`.
    pub lineno: usize,
    /// `colno`.
    pub colno: usize,
}

impl std::fmt::Display for JsonDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: line {} column {} (char {})",
            self.msg, self.lineno, self.colno, self.pos
        )
    }
}

impl From<JsonDecodeError> for sem_utils::SemanticaError {
    fn from(e: JsonDecodeError) -> Self {
        sem_utils::SemanticaError::JsonDecode(e.to_string())
    }
}

struct Parser<'a> {
    chars: &'a [char],
}

enum Scan {
    /// `StopIteration(idx)`: no value starts at `idx`.
    NoValue(usize),
    Error(JsonDecodeError),
}

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

impl<'a> Parser<'a> {
    fn error(&self, msg: &str, pos: usize) -> JsonDecodeError {
        let before = &self.chars[..pos.min(self.chars.len())];
        let lineno = before.iter().filter(|c| **c == '\n').count() + 1;
        let colno = match before.iter().rposition(|c| *c == '\n') {
            Some(i) => pos - i,
            None => pos + 1,
        };
        JsonDecodeError {
            msg: msg.to_string(),
            pos,
            lineno,
            colno,
        }
    }

    fn skip_ws(&self, mut idx: usize) -> usize {
        while idx < self.chars.len() && is_ws(self.chars[idx]) {
            idx += 1;
        }
        idx
    }

    fn at(&self, idx: usize) -> Option<char> {
        self.chars.get(idx).copied()
    }

    fn starts_with(&self, idx: usize, word: &str) -> bool {
        let w: Vec<char> = word.chars().collect();
        self.chars.len() >= idx + w.len() && self.chars[idx..idx + w.len()] == w[..]
    }

    fn scan_once(&self, idx: usize) -> Result<(Value, usize), Scan> {
        let Some(c) = self.at(idx) else {
            return Err(Scan::NoValue(idx));
        };
        match c {
            '"' => self.scan_string(idx + 1).map_err(Scan::Error).map(|(s, e)| (Value::String(s), e)),
            '{' => self.parse_object(idx + 1),
            '[' => self.parse_array(idx + 1),
            'n' if self.starts_with(idx, "null") => Ok((Value::Null, idx + 4)),
            't' if self.starts_with(idx, "true") => Ok((Value::Bool(true), idx + 4)),
            'f' if self.starts_with(idx, "false") => Ok((Value::Bool(false), idx + 5)),
            'N' if self.starts_with(idx, "NaN") => Ok((Value::Null, idx + 3)),
            'I' if self.starts_with(idx, "Infinity") => Ok((Value::Null, idx + 8)),
            '-' if self.starts_with(idx, "-Infinity") => Ok((Value::Null, idx + 9)),
            _ => self.match_number(idx),
        }
    }

    fn match_number(&self, start: usize) -> Result<(Value, usize), Scan> {
        let n = self.chars.len();
        let digit = |i: usize| i < n && self.chars[i].is_ascii_digit();
        let mut idx = start;
        if idx < n && self.chars[idx] == '-' {
            idx += 1;
            if idx >= n {
                return Err(Scan::NoValue(start));
            }
        }
        if idx < n && ('1'..='9').contains(&self.chars[idx]) {
            idx += 1;
            while digit(idx) {
                idx += 1;
            }
        } else if idx < n && self.chars[idx] == '0' {
            idx += 1;
        } else {
            return Err(Scan::NoValue(start));
        }
        let mut is_float = false;
        if idx < n && self.chars[idx] == '.' && digit(idx + 1) {
            is_float = true;
            idx += 2;
            while digit(idx) {
                idx += 1;
            }
        }
        if idx < n && (self.chars[idx] == 'e' || self.chars[idx] == 'E') {
            let e_start = idx;
            idx += 1;
            if idx < n && (self.chars[idx] == '-' || self.chars[idx] == '+') {
                idx += 1;
            }
            let digits_start = idx;
            while digit(idx) {
                idx += 1;
            }
            if idx > digits_start {
                is_float = true;
            } else {
                idx = e_start;
            }
        }
        let text: String = self.chars[start..idx].iter().collect();
        let value = if is_float {
            text.parse::<f64>()
                .ok()
                .and_then(Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::Null)
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

    fn decode_u4(&self, pos: usize) -> Result<u32, JsonDecodeError> {
        // `pos` is the index of the `u`.
        let hex: String = self.chars.iter().skip(pos + 1).take(4).collect();
        if hex.chars().count() == 4
            && !hex[1..].starts_with(['x', 'X'])
            && let Ok(v) = u32::from_str_radix(&hex, 16)
            && hex.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Ok(v);
        }
        Err(self.error("Invalid \\uXXXX escape", pos))
    }

    /// `scanstring(s, end)`: `end` is just after the opening quote.
    fn scan_string(&self, start: usize) -> Result<(String, usize), JsonDecodeError> {
        let begin = start - 1;
        let mut out = String::new();
        let mut idx = start;
        loop {
            let Some(c) = self.at(idx) else {
                return Err(self.error("Unterminated string starting at", begin));
            };
            if c == '"' {
                return Ok((out, idx + 1));
            }
            if (c as u32) < 0x20 {
                return Err(self.error("Invalid control character at", idx));
            }
            if c != '\\' {
                out.push(c);
                idx += 1;
                continue;
            }
            let Some(esc) = self.at(idx + 1) else {
                return Err(self.error("Unterminated string starting at", begin));
            };
            if esc != 'u' {
                let decoded = match esc {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => return Err(self.error("Invalid \\escape", idx)),
                };
                out.push(decoded);
                idx += 2;
                continue;
            }
            let mut uni = self.decode_u4(idx + 1)?;
            idx += 6;
            if (0xd800..=0xdbff).contains(&uni) && self.at(idx) == Some('\\') && self.at(idx + 1) == Some('u') {
                let uni2 = self.decode_u4(idx + 1)?;
                if (0xdc00..=0xdfff).contains(&uni2) {
                    uni = 0x10000 + (((uni - 0xd800) << 10) | (uni2 - 0xdc00));
                    idx += 6;
                }
            }
            out.push(char::from_u32(uni).unwrap_or('\u{fffd}'));
        }
    }

    fn parse_object(&self, mut end: usize) -> Result<(Value, usize), Scan> {
        let mut map = Map::new();
        let mut nextchar = self.at(end);
        if nextchar != Some('"') {
            if nextchar.is_some_and(is_ws) {
                end = self.skip_ws(end);
                nextchar = self.at(end);
            }
            if nextchar == Some('}') {
                return Ok((Value::Object(map), end + 1));
            }
            if nextchar != Some('"') {
                return Err(Scan::Error(
                    self.error("Expecting property name enclosed in double quotes", end),
                ));
            }
        }
        end += 1;
        loop {
            let (key, after_key) = self.scan_string(end).map_err(Scan::Error)?;
            end = after_key;
            if self.at(end) != Some(':') {
                end = self.skip_ws(end);
                if self.at(end) != Some(':') {
                    return Err(Scan::Error(self.error("Expecting ':' delimiter", end)));
                }
            }
            end += 1;
            end = self.skip_ws(end);
            let (value, after_value) = match self.scan_once(end) {
                Ok(v) => v,
                Err(Scan::NoValue(i)) => return Err(Scan::Error(self.error("Expecting value", i))),
                Err(e) => return Err(e),
            };
            map.insert(key, value);
            end = self.skip_ws(after_value);
            let nextchar = self.at(end);
            end += 1;
            match nextchar {
                Some('}') => break,
                Some(',') => {}
                _ => return Err(Scan::Error(self.error("Expecting ',' delimiter", end - 1))),
            }
            end = self.skip_ws(end);
            let nextchar = self.at(end);
            end += 1;
            if nextchar != Some('"') {
                return Err(Scan::Error(
                    self.error("Expecting property name enclosed in double quotes", end - 1),
                ));
            }
        }
        Ok((Value::Object(map), end))
    }

    fn parse_array(&self, mut end: usize) -> Result<(Value, usize), Scan> {
        let mut values = Vec::new();
        end = self.skip_ws(end);
        if self.at(end) == Some(']') {
            return Ok((Value::Array(values), end + 1));
        }
        loop {
            let (value, after) = match self.scan_once(end) {
                Ok(v) => v,
                Err(Scan::NoValue(i)) => return Err(Scan::Error(self.error("Expecting value", i))),
                Err(e) => return Err(e),
            };
            values.push(value);
            end = self.skip_ws(after);
            let nextchar = self.at(end);
            end += 1;
            match nextchar {
                Some(']') => break,
                Some(',') => {}
                _ => return Err(Scan::Error(self.error("Expecting ',' delimiter", end - 1))),
            }
            end = self.skip_ws(end);
        }
        Ok((Value::Array(values), end))
    }
}

/// `json.loads(s)`.
pub fn loads(s: &str) -> Result<Value, JsonDecodeError> {
    let chars: Vec<char> = s.chars().collect();
    let parser = Parser { chars: &chars };
    if chars.first() == Some(&'\u{feff}') {
        return Err(parser.error("Unexpected UTF-8 BOM (decode using utf-8-sig)", 0));
    }
    let start = parser.skip_ws(0);
    let (value, end) = match parser.scan_once(start) {
        Ok(v) => v,
        Err(Scan::NoValue(i)) => return Err(parser.error("Expecting value", i)),
        Err(Scan::Error(e)) => return Err(e),
    };
    let end = parser.skip_ws(end);
    if end != chars.len() {
        return Err(parser.error("Extra data", end));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(s: &str) -> String {
        loads(s).unwrap_err().to_string()
    }

    #[test]
    fn error_messages_match_cpython() {
        // Expected strings from CPython 3.11 `json.loads`.
        let cases = [
            ("", "Expecting value: line 1 column 1 (char 0)"),
            ("   ", "Expecting value: line 1 column 4 (char 3)"),
            ("not json", "Expecting value: line 1 column 1 (char 0)"),
            ("{", "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"),
            ("{\"a\"", "Expecting ':' delimiter: line 1 column 5 (char 4)"),
            ("{\"a\":", "Expecting value: line 1 column 6 (char 5)"),
            ("{\"a\":1", "Expecting ',' delimiter: line 1 column 7 (char 6)"),
            ("{\"a\":1,", "Expecting property name enclosed in double quotes: line 1 column 8 (char 7)"),
            ("{\"a\":1,}", "Expecting property name enclosed in double quotes: line 1 column 8 (char 7)"),
            ("{a:1}", "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"),
            ("[1,2", "Expecting ',' delimiter: line 1 column 5 (char 4)"),
            ("[1,]", "Expecting value: line 1 column 4 (char 3)"),
            ("[", "Expecting value: line 1 column 2 (char 1)"),
            ("]", "Expecting value: line 1 column 1 (char 0)"),
            ("\"abc", "Unterminated string starting at: line 1 column 1 (char 0)"),
            ("\"a\\qb\"", "Invalid \\escape: line 1 column 3 (char 2)"),
            ("\"a\\u12g4\"", "Invalid \\uXXXX escape: line 1 column 4 (char 3)"),
            ("\"a\u{1}b\"", "Invalid control character at: line 1 column 3 (char 2)"),
            ("{\"a\" 1}", "Expecting ':' delimiter: line 1 column 6 (char 5)"),
            ("[1 2]", "Expecting ',' delimiter: line 1 column 4 (char 3)"),
            ("1 2", "Extra data: line 1 column 3 (char 2)"),
            ("{} x", "Extra data: line 1 column 4 (char 3)"),
            ("nul", "Expecting value: line 1 column 1 (char 0)"),
            ("tru", "Expecting value: line 1 column 1 (char 0)"),
            ("-", "Expecting value: line 1 column 1 (char 0)"),
            ("01", "Extra data: line 1 column 2 (char 1)"),
            ("1.", "Extra data: line 1 column 2 (char 1)"),
            ("1e", "Extra data: line 1 column 2 (char 1)"),
            ("[1,\n 2,\n x]", "Expecting value: line 3 column 2 (char 9)"),
            ("[-]", "Expecting value: line 1 column 2 (char 1)"),
            ("\"\\", "Unterminated string starting at: line 1 column 1 (char 0)"),
            ("{\"a\":1 \"b\":2}", "Expecting ',' delimiter: line 1 column 8 (char 7)"),
            ("\u{feff}{}", "Unexpected UTF-8 BOM (decode using utf-8-sig): line 1 column 1 (char 0)"),
        ];
        for (input, expected) in cases {
            assert_eq!(err(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn values_match_cpython() {
        assert_eq!(loads("\"\\ud83d\\ude00\"").unwrap(), Value::String("😀".to_string()));
        assert_eq!(loads("{\"a\":1,\"a\":2,\"b\":3}").unwrap().to_string(), "{\"a\":2,\"b\":3}");
        assert_eq!(loads("1.5e3").unwrap(), serde_json::json!(1500.0));
        assert_eq!(loads("-0").unwrap(), serde_json::json!(0));
        assert_eq!(loads("  {\"x\": [true, false, null]}  ").unwrap(), serde_json::json!({"x": [true, false, null]}));
        assert_eq!(loads("{\"\\u00e9\": \"caf\\u00e9\"}").unwrap(), serde_json::json!({"é": "café"}));
        assert_eq!(loads("1E+2").unwrap(), serde_json::json!(100.0));
        assert_eq!(loads("[\"a\"\n,\"b\"]").unwrap(), serde_json::json!(["a", "b"]));
    }
}
