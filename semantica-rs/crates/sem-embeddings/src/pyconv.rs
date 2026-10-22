//! Python built-in conversions the module relies on: `int(str)`, `float(str)`, `str.lower()`,
//! `str.upper()`, and the `TypeError` text of using a non-integer as a size.

use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::{is_space, strip, type_name};
use serde_json::Value;

/// `str.lower()`.
pub(crate) fn py_lower(s: &str) -> String {
    s.to_lowercase()
}

/// `str.upper()`.
pub(crate) fn py_upper(s: &str) -> String {
    s.to_uppercase()
}

/// Digits with single underscores between them (Python's numeric literal grouping), with the
/// underscores removed; `None` when the grouping is invalid or there are no digits.
fn ungroup_digits(s: &str) -> Option<String> {
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return None;
    }
    if !s.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return None;
    }
    Some(s.replace('_', ""))
}

/// `int(s)` for a string (base 10): surrounding whitespace, an optional sign, digits with
/// optional `_` grouping. `None` where Python raises `ValueError`.
pub(crate) fn py_int(s: &str) -> Option<i128> {
    let t = strip(s);
    let (negative, digits) = match t.as_bytes().first() {
        Some(b'+') => (false, &t[1..]),
        Some(b'-') => (true, &t[1..]),
        _ => (false, t),
    };
    let digits = ungroup_digits(digits)?;
    let magnitude: i128 = digits.parse().ok()?;
    Some(if negative { -magnitude } else { magnitude })
}

/// `float(s)` for a string: surrounding whitespace, an optional sign, a decimal literal (with
/// optional `_` grouping and exponent) or `inf`/`infinity`/`nan` in any case. `None` where
/// Python raises `ValueError`.
pub(crate) fn py_float(s: &str) -> Option<f64> {
    let t = strip(s);
    let (sign, body) = match t.as_bytes().first() {
        Some(b'+') => (1.0, &t[1..]),
        Some(b'-') => (-1.0, &t[1..]),
        _ => (1.0, t),
    };
    let lower = body.to_ascii_lowercase();
    if lower == "inf" || lower == "infinity" {
        return Some(sign * f64::INFINITY);
    }
    if lower == "nan" {
        return Some(f64::NAN);
    }
    // mantissa [e exponent]
    let (mantissa, exponent) = match lower.find('e') {
        Some(i) => (&lower[..i], Some(&lower[i + 1..])),
        None => (lower.as_str(), None),
    };
    let (int_part, frac_part) = match mantissa.find('.') {
        Some(i) => (&mantissa[..i], Some(&mantissa[i + 1..])),
        None => (mantissa, None),
    };
    let int_digits = if int_part.is_empty() {
        String::new()
    } else {
        ungroup_digits(int_part)?
    };
    let frac_digits = match frac_part {
        Some("") | None => String::new(),
        Some(f) => ungroup_digits(f)?,
    };
    if int_digits.is_empty() && frac_digits.is_empty() {
        return None;
    }
    let mut literal = format!("{int_digits}.{frac_digits}");
    if literal.starts_with('.') {
        literal.insert(0, '0');
    }
    if let Some(exp) = exponent {
        let (esign, edigits) = match exp.as_bytes().first() {
            Some(b'+') => ("", &exp[1..]),
            Some(b'-') => ("-", &exp[1..]),
            _ => ("", exp),
        };
        let edigits = ungroup_digits(edigits)?;
        literal = format!("{literal}e{esign}{edigits}");
    }
    let value: f64 = literal.parse().ok()?;
    Some(sign * value)
}

/// Whether `s` is empty or only whitespace (`not s or not s.strip()`).
pub(crate) fn is_blank(s: &str) -> bool {
    s.chars().all(is_space)
}

/// An integer size argument (`rng.rand(dim)`, `range(0, n, step)`): the integer value of a
/// JSON int/bool, or Python's `TypeError` for other types.
pub(crate) fn py_index(value: &Value) -> Result<i64, SemanticaError> {
    match value {
        Value::Bool(b) => Ok(*b as i64),
        Value::Number(n) if !n.is_f64() => n.as_i64().ok_or_else(|| {
            SemanticaError::Overflow("Python int too large to convert to C ssize_t".to_string())
        }),
        other => Err(SemanticaError::Type(format!(
            "'{}' object cannot be interpreted as an integer",
            type_name(other)
        ))),
    }
}
