//! `json.dumps` with CPython 3.11's exact output: separators, indentation, `sort_keys`,
//! `ensure_ascii` escaping and `float.__repr__` numbers.
//!
//! `serde_json` writes `{"a":1}` and `1e16`; Python writes `{"a": 1}` and `1e+16`. Hashes and
//! exported files computed from `json.dumps` output (e.g. `hash_data`) need the Python form.

use std::fmt::Write as _;

use serde_json::{Map, Value};

use super::fmt::{float_repr, number_str};

/// Keyword arguments of `json.dumps` that Semantica uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumpsOptions<'a> {
    /// `indent`: `None` writes everything on one line; `Some(n)` pretty-prints with `n` spaces.
    pub indent: Option<usize>,
    /// `ensure_ascii` (Python default `True`): escape every non-ASCII character.
    pub ensure_ascii: bool,
    /// `sort_keys` (Python default `False`).
    pub sort_keys: bool,
    /// `separators` as `(item_separator, key_separator)`; `None` picks Python's default
    /// (`", "`/`": "`, or `","`/`": "` when indenting).
    pub separators: Option<(&'a str, &'a str)>,
}

impl Default for DumpsOptions<'_> {
    fn default() -> Self {
        DumpsOptions {
            indent: None,
            ensure_ascii: true,
            sort_keys: false,
            separators: None,
        }
    }
}

/// `json.dumps(value, **options)`.
pub fn dumps(value: &Value, options: &DumpsOptions<'_>) -> String {
    let (item_sep, key_sep) = options.separators.unwrap_or(if options.indent.is_some() {
        (",", ": ")
    } else {
        (", ", ": ")
    });
    let mut out = String::new();
    let enc = Encoder {
        indent: options.indent,
        ensure_ascii: options.ensure_ascii,
        sort_keys: options.sort_keys,
        item_sep,
        key_sep,
    };
    enc.encode(value, 0, &mut out);
    out
}

/// `json.dumps(value)` with Python's defaults.
pub fn dumps_default(value: &Value) -> String {
    dumps(value, &DumpsOptions::default())
}

struct Encoder<'a> {
    indent: Option<usize>,
    ensure_ascii: bool,
    sort_keys: bool,
    item_sep: &'a str,
    key_sep: &'a str,
}

impl Encoder<'_> {
    fn newline(&self, level: usize, out: &mut String) {
        if let Some(width) = self.indent {
            out.push('\n');
            out.push_str(&" ".repeat(width * level));
        }
    }

    fn encode(&self, value: &Value, level: usize, out: &mut String) {
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Number(n) => {
                if n.is_f64() {
                    let f = n.as_f64().unwrap_or(f64::NAN);
                    if f.is_nan() {
                        out.push_str("NaN");
                    } else if f.is_infinite() {
                        out.push_str(if f > 0.0 { "Infinity" } else { "-Infinity" });
                    } else {
                        out.push_str(&float_repr(f));
                    }
                } else {
                    out.push_str(&number_str(n));
                }
            }
            Value::String(s) => encode_string(s, self.ensure_ascii, out),
            Value::Array(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                self.newline(level + 1, out);
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(self.item_sep);
                        self.newline(level + 1, out);
                    }
                    self.encode(item, level + 1, out);
                }
                self.newline(level, out);
                out.push(']');
            }
            Value::Object(map) => self.encode_map(map, level, out),
        }
    }

    fn encode_map(&self, map: &Map<String, Value>, level: usize, out: &mut String) {
        if map.is_empty() {
            out.push_str("{}");
            return;
        }
        let mut entries: Vec<(&String, &Value)> = map.iter().collect();
        if self.sort_keys {
            entries.sort_by(|a, b| a.0.cmp(b.0));
        }
        out.push('{');
        self.newline(level + 1, out);
        for (i, (key, value)) in entries.into_iter().enumerate() {
            if i > 0 {
                out.push_str(self.item_sep);
                self.newline(level + 1, out);
            }
            encode_string(key, self.ensure_ascii, out);
            out.push_str(self.key_sep);
            self.encode(value, level + 1, out);
        }
        self.newline(level, out);
        out.push('}');
    }
}

/// Write a JSON string literal the way `json.encoder` does.
pub fn encode_string(s: &str, ensure_ascii: bool, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c if ensure_ascii && !(' '..='~').contains(&c) => {
                let code = c as u32;
                if code < 0x10000 {
                    let _ = write!(out, "\\u{code:04x}");
                } else {
                    let v = code - 0x10000;
                    let high = 0xd800 | ((v >> 10) & 0x3ff);
                    let low = 0xdc00 | (v & 0x3ff);
                    let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_and_sorted_output() {
        let v = json!({"b": [1, 2.5, {"x": null}], "a": "\u{e9}\u{2028}\u{7f}\u{1f600}"});
        assert_eq!(
            dumps(
                &v,
                &DumpsOptions {
                    sort_keys: true,
                    ..Default::default()
                }
            ),
            r#"{"a": "\u00e9\u2028\u007f\ud83d\ude00", "b": [1, 2.5, {"x": null}]}"#
        );
    }

    #[test]
    fn indented_output() {
        let v = json!([[], {}, [1]]);
        assert_eq!(
            dumps(
                &v,
                &DumpsOptions {
                    indent: Some(2),
                    ..Default::default()
                }
            ),
            "[\n  [],\n  {},\n  [\n    1\n  ]\n]"
        );
    }
}
