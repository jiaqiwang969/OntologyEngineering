//! Python-compatible rendering of JSON-like values: `str()`, `repr()`, float formatting,
//! truthiness, equality and the whitespace rules of `str.strip()` / `re`'s `\s`.
//!
//! Semantica builds many messages with f-strings over dicts, lists and numbers; these helpers
//! reproduce CPython 3.11's output so ported messages keep the original text.

use std::fmt::Write as _;

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Number, Value};

use crate::exceptions::SemanticaError;

/// `str.isspace()` for one character (also what `re`'s Unicode `\s` matches).
///
/// Python counts the ASCII information separators U+001C..U+001F as whitespace; Rust's
/// `char::is_whitespace` does not.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `str.strip()` with no arguments.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `str.strip(chars)`.
pub fn strip_chars<'a>(s: &'a str, chars: &str) -> &'a str {
    s.trim_matches(|c| chars.contains(c))
}

/// A regex character class equivalent to Python's Unicode `\s` (use inside `[...]`).
pub const SPACE_CLASS: &str = r"\s\x{1c}-\x{1f}";

/// `float.__repr__` (also `str()` of a float and what `json.dumps` writes).
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.to_string();
    }
    // Rust's `{:e}` yields the shortest round-tripping digits, as Python's repr does.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exponent) = sci
        .split_once('e')
        .expect("LowerExp output has an exponent");
    let exponent: i32 = exponent.parse().expect("LowerExp exponent is an integer");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let decpt = exponent + 1;
    let mut out = String::new();
    if x < 0.0 {
        out.push('-');
    }
    if decpt <= -4 || decpt > 16 {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let e = decpt - 1;
        let _ = write!(out, "e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    } else if decpt <= 0 {
        out.push_str("0.");
        out.push_str(&"0".repeat((-decpt) as usize));
        out.push_str(&digits);
    } else if decpt as usize >= digits.len() {
        out.push_str(&digits);
        out.push_str(&"0".repeat(decpt as usize - digits.len()));
        out.push_str(".0");
    } else {
        out.push_str(&digits[..decpt as usize]);
        out.push('.');
        out.push_str(&digits[decpt as usize..]);
    }
    out
}

/// `format(x, ".{precision}f")` for a float.
pub fn format_fixed(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    format!("{x:.precision$}")
}

/// `str()` of a JSON number: integers print their digits, floats their `repr`.
pub fn number_str(n: &Number) -> String {
    if let Some(i) = n.as_i64() {
        i.to_string()
    } else if let Some(u) = n.as_u64() {
        u.to_string()
    } else {
        float_repr(n.as_f64().unwrap_or(f64::NAN))
    }
}

static NOT_PRINTABLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[\p{Cc}\p{Cf}\p{Co}\p{Cn}\p{Zl}\p{Zp}\p{Zs}]$").expect("valid class")
});

/// `str.isprintable()` for one character.
pub fn is_printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    let mut buf = [0u8; 4];
    !NOT_PRINTABLE.is_match(c.encode_utf8(&mut buf))
}

/// `repr()` of a string.
pub fn str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c if c.is_ascii() || is_printable(c) => out.push(c),
            c => {
                let code = c as u32;
                if code <= 0xff {
                    let _ = write!(out, "\\x{code:02x}");
                } else if code <= 0xffff {
                    let _ = write!(out, "\\u{code:04x}");
                } else {
                    let _ = write!(out, "\\U{code:08x}");
                }
            }
        }
    }
    out.push(quote);
    out
}

/// `repr()` of a JSON-like value (`None`, `True`, `'text'`, `[1, 'a']`, `{'k': 2.5}`).
pub fn py_repr(value: &Value) -> String {
    match value {
        Value::String(s) => str_repr(s),
        other => py_str(other),
    }
}

/// `str()` of a JSON-like value. Strings print as themselves; containers use `repr()` of
/// their items, exactly like Python.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(n) => number_str(n),
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(py_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => map_repr(map),
    }
}

/// `repr()` of a dict.
pub fn map_repr(map: &Map<String, Value>) -> String {
    let parts: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{}: {}", str_repr(k), py_repr(v)))
        .collect();
    format!("{{{}}}", parts.join(", "))
}

/// `repr()` of a list of strings (`['a', 'b']`).
pub fn str_list_repr<S: AsRef<str>>(items: &[S]) -> String {
    let parts: Vec<String> = items.iter().map(|s| str_repr(s.as_ref())).collect();
    format!("[{}]", parts.join(", "))
}

/// `type(value).__name__` for a JSON-like value.
pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// Python truthiness (`bool(value)`).
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i != 0
            } else if let Some(u) = n.as_u64() {
                u != 0
            } else {
                n.as_f64().map(|f| f != 0.0).unwrap_or(true)
            }
        }
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// A Python number (`bool` counts as `int`, as in Python).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PyNum {
    /// An `int` (or `bool`).
    Int(i128),
    /// A `float`.
    Float(f64),
}

impl PyNum {
    /// The numeric reading of a value, if it is a number or a bool.
    pub fn of(value: &Value) -> Option<PyNum> {
        match value {
            Value::Bool(b) => Some(PyNum::Int(i128::from(*b))),
            Value::Number(n) => Some(if let Some(i) = n.as_i64() {
                PyNum::Int(i128::from(i))
            } else if let Some(u) = n.as_u64() {
                PyNum::Int(i128::from(u))
            } else {
                PyNum::Float(n.as_f64().unwrap_or(f64::NAN))
            }),
            _ => None,
        }
    }

    /// As a float (`float(x)`).
    pub fn to_f64(self) -> f64 {
        match self {
            PyNum::Int(i) => i as f64,
            PyNum::Float(f) => f,
        }
    }

    /// Exact numeric comparison, as Python compares `int` with `float`.
    pub fn compare(self, other: PyNum) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        match (self, other) {
            (PyNum::Int(a), PyNum::Int(b)) => Some(a.cmp(&b)),
            (PyNum::Float(a), PyNum::Float(b)) => a.partial_cmp(&b),
            (PyNum::Int(a), PyNum::Float(b)) => int_float_cmp(a, b),
            (PyNum::Float(a), PyNum::Int(b)) => int_float_cmp(b, a).map(Ordering::reverse),
        }
    }
}

fn int_float_cmp(i: i128, f: f64) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    if f.is_nan() {
        return None;
    }
    if f.is_infinite() {
        return Some(if f > 0.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let floor = f.floor();
    if !(-1.7e38..=1.7e38).contains(&floor) {
        return Some(if f > 0.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let fi = floor as i128;
    match i.cmp(&fi) {
        Ordering::Equal if f > floor => Some(Ordering::Less),
        other => Some(other),
    }
}

/// Python `==` between two JSON-like values (`1 == 1.0 == True`, containers element-wise).
pub fn py_eq(a: &Value, b: &Value) -> bool {
    if let (Some(x), Some(y)) = (PyNum::of(a), PyNum::of(b)) {
        return x.compare(y) == Some(std::cmp::Ordering::Equal);
    }
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| py_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).map(|w| py_eq(v, w)).unwrap_or(false))
        }
        _ => false,
    }
}

/// The key a hashable value takes in a Python `set`, or the `TypeError` an unhashable one
/// (`list`, `dict`) raises.
pub fn hash_key(value: &Value) -> Result<String, SemanticaError> {
    match value {
        Value::Array(_) | Value::Object(_) => Err(SemanticaError::Type(format!(
            "unhashable type: '{}'",
            type_name(value)
        ))),
        Value::Null => Ok("n".to_string()),
        Value::String(s) => Ok(format!("s:{s}")),
        other => match PyNum::of(other) {
            Some(PyNum::Int(i)) => Ok(format!("i:{i}")),
            Some(PyNum::Float(f)) if f.is_finite() && f.fract() == 0.0 && f.abs() < 1.7e38 => {
                Ok(format!("i:{}", f as i128))
            }
            Some(PyNum::Float(f)) => Ok(format!("f:{}", f.to_bits())),
            None => Ok(format!("o:{other}")),
        },
    }
}

/// `format(value, ".{precision}f")` for any JSON-like value, with the errors Python raises
/// for values that are not numbers.
pub fn format_value_fixed(value: &Value, precision: usize) -> Result<String, SemanticaError> {
    match value {
        Value::String(_) => Err(SemanticaError::Value(
            "Unknown format code 'f' for object of type 'str'".to_string(),
        )),
        other => match PyNum::of(other) {
            Some(n) => Ok(format_fixed(n.to_f64(), precision)),
            None => Err(SemanticaError::Type(format!(
                "unsupported format string passed to {}.__format__",
                type_name(other)
            ))),
        },
    }
}

/// `str[start:end]` with Python's slice rules (negative indices count from the end; indices
/// are clamped), over Unicode code points.
pub fn slice_chars(s: &str, start: Option<i64>, end: Option<i64>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let (lo, hi) = slice_bounds(chars.len(), start, end);
    if lo >= hi {
        return String::new();
    }
    chars[lo..hi].iter().collect()
}

/// Resolve Python slice bounds for a sequence of `len` items.
pub fn slice_bounds(len: usize, start: Option<i64>, end: Option<i64>) -> (usize, usize) {
    let len_i = len as i64;
    let norm = |i: i64| -> usize {
        if i < 0 {
            (len_i + i).max(0) as usize
        } else {
            i.min(len_i) as usize
        }
    };
    let lo = start.map(norm).unwrap_or(0);
    let hi = end.map(norm).unwrap_or(len);
    (lo, hi)
}

/// One parsed `%(key)...` conversion of a Python `%`-format string.
#[derive(Debug, Clone, Default)]
struct PercentSpec {
    left: bool,
    zero: bool,
    plus: bool,
    space: bool,
    alternate: bool,
    width: Option<usize>,
    precision: Option<usize>,
    conversion: char,
}

/// `format % mapping` (Python's printf-style formatting with `%(key)s` fields), resolving each
/// key through `lookup`. Errors are the ones Python raises: `KeyError` for a missing key,
/// `ValueError` for a bad conversion character, `TypeError` for a value of the wrong type.
pub fn percent_format(
    format: &str,
    lookup: &dyn Fn(&str) -> Option<Value>,
) -> Result<String, SemanticaError> {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let key = if chars.get(i) == Some(&'(') {
            let mut depth = 1;
            let mut j = i + 1;
            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if depth > 0 {
                return Err(SemanticaError::Value("incomplete format key".to_string()));
            }
            let key: String = chars[i + 1..j - 1].iter().collect();
            i = j;
            Some(key)
        } else {
            None
        };
        let mut spec = PercentSpec::default();
        while let Some(&f) = chars.get(i) {
            match f {
                '-' => spec.left = true,
                '0' => spec.zero = true,
                '+' => spec.plus = true,
                ' ' => spec.space = true,
                '#' => spec.alternate = true,
                _ => break,
            }
            i += 1;
        }
        let mut width = String::new();
        while let Some(&d) = chars.get(i).filter(|d| d.is_ascii_digit()) {
            width.push(d);
            i += 1;
        }
        spec.width = width.parse().ok();
        if chars.get(i) == Some(&'.') {
            i += 1;
            let mut precision = String::new();
            while let Some(&d) = chars.get(i).filter(|d| d.is_ascii_digit()) {
                precision.push(d);
                i += 1;
            }
            spec.precision = Some(precision.parse().unwrap_or(0));
        }
        while matches!(chars.get(i), Some('h' | 'l' | 'L')) {
            i += 1;
        }
        let Some(&conversion) = chars.get(i) else {
            return Err(SemanticaError::Value("incomplete format".to_string()));
        };
        let conversion_index = i;
        i += 1;
        spec.conversion = conversion;
        if conversion == '%' && key.is_none() {
            out.push('%');
            continue;
        }
        let value = match &key {
            Some(k) => lookup(k).ok_or_else(|| SemanticaError::Key(k.clone()))?,
            None => {
                return Err(SemanticaError::Type(
                    "format requires a mapping".to_string(),
                ));
            }
        };
        out.push_str(&render_percent(&spec, &value, conversion_index)?);
    }
    Ok(out)
}

fn pad_field(spec: &PercentSpec, body: String, numeric: bool) -> String {
    let width = spec.width.unwrap_or(0);
    let len = body.chars().count();
    if len >= width {
        return body;
    }
    let fill = width - len;
    if spec.left {
        format!("{body}{}", " ".repeat(fill))
    } else if spec.zero && numeric {
        let (sign, digits) = match body.chars().next() {
            Some(s @ ('-' | '+' | ' ')) => (s.to_string(), body[1..].to_string()),
            _ => (String::new(), body.clone()),
        };
        let (prefix, rest) =
            if digits.starts_with("0x") || digits.starts_with("0X") || digits.starts_with("0o") {
                (digits[..2].to_string(), digits[2..].to_string())
            } else {
                (String::new(), digits)
            };
        format!("{sign}{prefix}{}{rest}", "0".repeat(fill))
    } else {
        format!("{}{body}", " ".repeat(fill))
    }
}

fn sign_prefix(spec: &PercentSpec, negative: bool) -> &'static str {
    if negative {
        "-"
    } else if spec.plus {
        "+"
    } else if spec.space {
        " "
    } else {
        ""
    }
}

fn exp_format(x: f64, precision: usize, upper: bool) -> String {
    let s = format!("{:.*e}", precision, x.abs());
    let (mantissa, exponent) = s.split_once('e').unwrap_or((&s, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let e = if upper { 'E' } else { 'e' };
    format!(
        "{mantissa}{e}{}{:02}",
        if exponent < 0 { '-' } else { '+' },
        exponent.abs()
    )
}

fn general_format(x: f64, precision: usize, upper: bool, alternate: bool) -> String {
    let p = if precision == 0 { 1 } else { precision };
    if x == 0.0 {
        let base = if alternate {
            format!("{:.*}", p - 1, 0.0)
        } else {
            "0".to_string()
        };
        return base;
    }
    let sci = format!("{:.*e}", p - 1, x.abs());
    let exponent: i32 = sci
        .split_once('e')
        .and_then(|(_, e)| e.parse().ok())
        .unwrap_or(0);
    let mut body = if exponent >= -4 && exponent < p as i32 {
        format!("{:.*}", (p as i32 - 1 - exponent).max(0) as usize, x.abs())
    } else {
        exp_format(x.abs(), p - 1, upper)
    };
    if !alternate {
        let (mantissa, tail) = match body.find(['e', 'E']) {
            Some(pos) => (body[..pos].to_string(), body[pos..].to_string()),
            None => (body.clone(), String::new()),
        };
        let mantissa = if mantissa.contains('.') {
            mantissa
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        } else {
            mantissa
        };
        body = format!("{mantissa}{tail}");
    }
    body
}

fn render_percent(
    spec: &PercentSpec,
    value: &Value,
    index: usize,
) -> Result<String, SemanticaError> {
    let real_required = |what: &str| {
        SemanticaError::Type(format!(
            "{what} format: a real number is required, not {}",
            type_name(value)
        ))
    };
    match spec.conversion {
        's' | 'r' | 'a' => {
            let mut text = if spec.conversion == 's' {
                py_str(value)
            } else {
                py_repr(value)
            };
            if let Some(p) = spec.precision {
                text = text.chars().take(p).collect();
            }
            Ok(pad_field(spec, text, false))
        }
        'd' | 'i' | 'u' => {
            let n = PyNum::of(value).ok_or_else(|| real_required("%d"))?;
            let int = match n {
                PyNum::Int(i) => i,
                PyNum::Float(f) if f.is_nan() => {
                    return Err(SemanticaError::Value(
                        "cannot convert float NaN to integer".to_string(),
                    ));
                }
                PyNum::Float(f) if f.is_infinite() => {
                    return Err(SemanticaError::Overflow(
                        "cannot convert float infinity to integer".to_string(),
                    ));
                }
                PyNum::Float(f) => f.trunc() as i128,
            };
            let mut digits = int.unsigned_abs().to_string();
            if let Some(p) = spec.precision
                && digits.len() < p
            {
                digits = format!("{}{digits}", "0".repeat(p - digits.len()));
            }
            Ok(pad_field(
                spec,
                format!("{}{digits}", sign_prefix(spec, int < 0)),
                true,
            ))
        }
        'x' | 'X' | 'o' => {
            let int = match PyNum::of(value) {
                Some(PyNum::Int(i)) => i,
                _ => {
                    return Err(SemanticaError::Type(format!(
                        "%{} format: an integer is required, not {}",
                        spec.conversion,
                        type_name(value)
                    )));
                }
            };
            let abs = int.unsigned_abs();
            let (digits, prefix) = match spec.conversion {
                'x' => (format!("{abs:x}"), "0x"),
                'X' => (format!("{abs:X}"), "0X"),
                _ => (format!("{abs:o}"), "0o"),
            };
            let prefix = if spec.alternate { prefix } else { "" };
            Ok(pad_field(
                spec,
                format!("{}{prefix}{digits}", sign_prefix(spec, int < 0)),
                true,
            ))
        }
        'f' | 'F' | 'e' | 'E' | 'g' | 'G' => {
            let x = match value {
                Value::String(_) | Value::Null | Value::Array(_) | Value::Object(_) => {
                    return Err(SemanticaError::Type(format!(
                        "must be real number, not {}",
                        type_name(value)
                    )));
                }
                other => PyNum::of(other).map(PyNum::to_f64).unwrap_or(f64::NAN),
            };
            let precision = spec.precision.unwrap_or(6);
            let upper = spec.conversion.is_ascii_uppercase();
            let body = if x.is_nan() {
                if upper { "NAN" } else { "nan" }.to_string()
            } else if x.is_infinite() {
                if upper { "INF" } else { "inf" }.to_string()
            } else {
                match spec.conversion {
                    'f' | 'F' => format!("{:.*}", precision, x.abs()),
                    'e' | 'E' => exp_format(x, precision, upper),
                    _ => general_format(x, precision, upper, spec.alternate),
                }
            };
            let negative = x.is_sign_negative() && !x.is_nan();
            Ok(pad_field(
                spec,
                format!("{}{body}", sign_prefix(spec, negative)),
                x.is_finite(),
            ))
        }
        'c' => match value {
            Value::String(s) if s.chars().count() == 1 => Ok(pad_field(spec, s.clone(), false)),
            other => match PyNum::of(other) {
                Some(PyNum::Int(i)) => {
                    let c = u32::try_from(i)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| {
                            SemanticaError::Overflow("%c arg not in range(0x110000)".to_string())
                        })?;
                    Ok(pad_field(spec, c.to_string(), false))
                }
                _ => Err(SemanticaError::Type("%c requires int or char".to_string())),
            },
        },
        '%' => Ok("%".to_string()),
        other => Err(SemanticaError::Value(format!(
            "unsupported format character '{other}' ({:#x}) at index {index}",
            other as u32
        ))),
    }
}

/// `PurePosixPath(path).name`: the last path component, ignoring empty and `.` components.
pub fn path_name(path: &str) -> String {
    path.split('/')
        .rfind(|part| !part.is_empty() && *part != ".")
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn float_repr_matches_python() {
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(float_repr(1e15), "1000000000000000.0");
        assert_eq!(float_repr(0.0001), "0.0001");
        assert_eq!(float_repr(0.00001), "1e-05");
        assert_eq!(float_repr(-2.5), "-2.5");
        assert_eq!(float_repr(1.2345678901234568e17), "1.2345678901234568e+17");
        assert_eq!(float_repr(5e-324), "5e-324");
    }

    #[test]
    fn repr_of_containers() {
        let v = json!({"a": [1, 2.5, null, true, "it's"], "b": {"c": "x"}});
        assert_eq!(
            py_str(&v),
            "{'a': [1, 2.5, None, True, \"it's\"], 'b': {'c': 'x'}}"
        );
    }

    #[test]
    fn numeric_equality_follows_python() {
        assert!(py_eq(&json!(1), &json!(1.0)));
        assert!(py_eq(&json!(true), &json!(1)));
        assert!(!py_eq(&json!("1"), &json!(1)));
        assert!(!py_eq(
            &json!(9007199254740993u64),
            &json!(9007199254740992.0)
        ));
    }

    #[test]
    fn slices_follow_python() {
        assert_eq!(slice_chars("abcdef", None, Some(-2)), "abcd");
        assert_eq!(slice_chars("abcdef", Some(4), None), "ef");
        assert_eq!(slice_chars("abc", None, Some(10)), "abc");
        assert_eq!(path_name("a/b/"), "b");
        assert_eq!(path_name("/"), "");
    }
}
