//! Python behaviour the change-management port relies on that `sem_utils::pycompat` does not
//! cover:
//!
//! - `json.dumps` text (with `ensure_ascii`, `sort_keys` and custom separators) written fast
//!   enough for checksumming large graphs in debug builds;
//! - `datetime.fromisoformat` as implemented in C by CPython 3.11 (`_datetimemodule.c`);
//! - `urllib.parse.urlparse` / `urljoin` / `quote` and the `ipaddress` checks `urlsplit` applies to
//!   bracketed hosts (CPython 3.11.14);
//! - container semantics of `dict.get`, `list(x)`, `set(x)` and `<` over JSON-like values.
//!
//! Everything here is private to the crate; the public API lives in the original module files.

use std::fmt::Write as _;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;
use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::{PyNum, float_repr, hash_key, type_name};
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

// ---------------------------------------------------------------------------------------------
// json.dumps
// ---------------------------------------------------------------------------------------------

/// `json.dumps(value)` (default separators `", "` / `": "`, insertion order, `ensure_ascii`).
pub(crate) fn json_dumps_default(value: &Value) -> String {
    let mut out = String::with_capacity(128);
    write_json(&mut out, value, false, ", ", ": ");
    out
}

/// `json.dumps(dict)` with Python's defaults, for a dict held as a map.
pub(crate) fn json_dumps_map_default(map: &Map<String, Value>) -> String {
    let mut out = String::with_capacity(256);
    write_map(&mut out, map, false, ", ", ": ", None);
    out
}

/// `json.dumps({k: v for k, v in map.items() if k != skip}, sort_keys=True,
/// separators=(",", ":"))`: the canonical text `compute_checksum` hashes.
pub(crate) fn json_dumps_canonical(map: &Map<String, Value>, skip: Option<&str>) -> String {
    let mut out = String::with_capacity(256);
    write_map(&mut out, map, true, ",", ":", skip);
    out
}

fn write_map(
    out: &mut String,
    map: &Map<String, Value>,
    sort_keys: bool,
    item_sep: &str,
    key_sep: &str,
    skip: Option<&str>,
) {
    out.push('{');
    let mut first = true;
    let mut emit = |out: &mut String, key: &String, item: &Value| {
        if skip == Some(key.as_str()) {
            return;
        }
        if !first {
            out.push_str(item_sep);
        }
        first = false;
        write_json_str(out, key);
        out.push_str(key_sep);
        write_json(out, item, sort_keys, item_sep, key_sep);
    };
    if sort_keys {
        let mut entries: Vec<(&String, &Value)> = map.iter().collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
        for (key, item) in entries {
            emit(out, key, item);
        }
    } else {
        for (key, item) in map {
            emit(out, key, item);
        }
    }
    out.push('}');
}

fn write_json(out: &mut String, value: &Value, sort_keys: bool, item_sep: &str, key_sep: &str) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(u) = n.as_u64() {
                let _ = write!(out, "{u}");
            } else {
                write_float(out, n.as_f64().unwrap_or(f64::NAN));
            }
        }
        Value::String(s) => write_json_str(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                write_json(out, item, sort_keys, item_sep, key_sep);
            }
            out.push(']');
        }
        Value::Object(map) => write_map(out, map, sort_keys, item_sep, key_sep, None),
    }
}

/// A float as `json.dumps` writes it (`float.__repr__`; `NaN`/`Infinity` for non-finite
/// values). Where Python's repr is positional (`1e-4 <= |x| < 1e16`) it is core's shortest
/// round-trip `Display` output plus `.0` for integral values, which avoids an allocation per
/// float; elsewhere `sem_utils`' `float_repr` handles the exponent form.
fn write_float(out: &mut String, f: f64) {
    let magnitude = f.abs();
    if (1e-4..1e16).contains(&magnitude) {
        let start = out.len();
        let _ = write!(out, "{f}");
        if !out[start..].contains('.') {
            out.push_str(".0");
        }
    } else if f.is_nan() {
        out.push_str("NaN");
    } else if f.is_infinite() {
        out.push_str(if f > 0.0 { "Infinity" } else { "-Infinity" });
    } else {
        out.push_str(&float_repr(f));
    }
}

/// A JSON string literal as `json.encoder` writes it with `ensure_ascii=True`: everything
/// outside `' '..='~'` is escaped (`\n`-style escapes where JSON has one, else `\uXXXX`,
/// with surrogate pairs above the BMP).
fn write_json_str(out: &mut String, s: &str) {
    out.push('"');
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Plain comparisons: this loop runs per byte and `contains` is a call in debug builds.
        #[allow(clippy::manual_range_contains)]
        let plain = b >= 0x20 && b < 0x7f && b != b'"' && b != b'\\';
        if plain {
            i += 1;
            continue;
        }
        out.push_str(&s[start..i]);
        if b < 0x80 {
            match b {
                b'"' => out.push_str("\\\""),
                b'\\' => out.push_str("\\\\"),
                b'\n' => out.push_str("\\n"),
                b'\r' => out.push_str("\\r"),
                b'\t' => out.push_str("\\t"),
                0x08 => out.push_str("\\b"),
                0x0c => out.push_str("\\f"),
                other => {
                    let _ = write!(out, "\\u{:04x}", other);
                }
            }
            i += 1;
        } else {
            let c = s[i..].chars().next().expect("valid UTF-8 boundary");
            let code = c as u32;
            if code < 0x10000 {
                let _ = write!(out, "\\u{code:04x}");
            } else {
                let v = code - 0x10000;
                let high = 0xd800 | ((v >> 10) & 0x3ff);
                let low = 0xdc00 | (v & 0x3ff);
                let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
            }
            i += c.len_utf8();
        }
        start = i;
    }
    out.push_str(&s[start..]);
    out.push('"');
}

/// `json.loads(text)` for text the storage itself wrote; errors become `JSONDecodeError`.
pub(crate) fn json_loads(text: &str) -> Result<Value, SemanticaError> {
    serde_json::from_str(text).map_err(|e| SemanticaError::JsonDecode(e.to_string()))
}

// ---------------------------------------------------------------------------------------------
// dict / list / set semantics over JSON-like values
// ---------------------------------------------------------------------------------------------

/// The `None` that `dict.get` returns for a missing key.
pub(crate) static NONE: Value = Value::Null;

/// `d.get(key)` (`None` when missing).
pub(crate) fn get<'a>(map: &'a Map<String, Value>, key: &str) -> &'a Value {
    map.get(key).unwrap_or(&NONE)
}

/// `AttributeError: '<type>' object has no attribute '<name>'`.
pub(crate) fn attribute_error(value: &Value, name: &str) -> SemanticaError {
    SemanticaError::Attribute(format!(
        "'{}' object has no attribute '{name}'",
        type_name(value)
    ))
}

/// `value` used as a dict: the map, or the `AttributeError` calling `value.<method>` raises.
pub(crate) fn as_dict<'a>(
    value: &'a Value,
    method: &str,
) -> Result<&'a Map<String, Value>, SemanticaError> {
    match value {
        Value::Object(map) => Ok(map),
        other => Err(attribute_error(other, method)),
    }
}

fn not_iterable(value: &Value) -> SemanticaError {
    SemanticaError::Type(format!("'{}' object is not iterable", type_name(value)))
}

/// `list(value)`: a list's items, a dict's keys, a string's characters; anything else is a
/// `TypeError`.
pub(crate) fn py_list(value: &Value) -> Result<Vec<Value>, SemanticaError> {
    match value {
        Value::Array(items) => Ok(items.clone()),
        Value::Object(map) => Ok(map.keys().map(|k| Value::String(k.clone())).collect()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string())).collect()),
        other => Err(not_iterable(other)),
    }
}

/// `len(list(value))` without building the list.
pub(crate) fn py_list_len(value: &Value) -> Result<usize, SemanticaError> {
    match value {
        Value::Array(items) => Ok(items.len()),
        Value::Object(map) => Ok(map.len()),
        Value::String(s) => Ok(s.chars().count()),
        other => Err(not_iterable(other)),
    }
}

/// `list(value or [])`.
pub(crate) fn py_list_or_empty(value: &Value) -> Result<Vec<Value>, SemanticaError> {
    if sem_utils::pycompat::fmt::truthy(value) {
        py_list(value)
    } else {
        Ok(Vec::new())
    }
}

/// A Python `set` of JSON-like values: members are identified by Python equality
/// (`1 == 1.0 == True`), the first of equal members is kept, and iteration follows insertion
/// order (Python's own order for strings varies between runs, see PORT-NOTES.md).
#[derive(Debug, Clone, Default)]
pub(crate) struct PySet {
    members: IndexMap<String, Value>,
}

impl PySet {
    /// `set(items)`; an unhashable member (list, dict) is a `TypeError`.
    pub(crate) fn from_items<'a, I>(items: I) -> Result<PySet, SemanticaError>
    where
        I: IntoIterator<Item = &'a Value>,
    {
        let mut set = PySet::default();
        set.update(items)?;
        Ok(set)
    }

    /// `set(value)` for any iterable value.
    pub(crate) fn from_iterable(value: &Value) -> Result<PySet, SemanticaError> {
        let items = py_list(value)?;
        PySet::from_items(items.iter())
    }

    /// `set.update(items)`.
    pub(crate) fn update<'a, I>(&mut self, items: I) -> Result<(), SemanticaError>
    where
        I: IntoIterator<Item = &'a Value>,
    {
        for item in items {
            let key = hash_key(item)?;
            self.members.entry(key).or_insert_with(|| item.clone());
        }
        Ok(())
    }

    /// `self - other`, in this set's order.
    pub(crate) fn difference(&self, other: &PySet) -> Vec<Value> {
        self.members
            .iter()
            .filter(|(k, _)| !other.members.contains_key(*k))
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// `self < other` (proper subset).
    pub(crate) fn is_proper_subset(&self, other: &PySet) -> bool {
        self.members.len() < other.members.len()
            && self.members.keys().all(|k| other.members.contains_key(k))
    }

    /// `self != other` for sets.
    pub(crate) fn ne(&self, other: &PySet) -> bool {
        self.members.len() != other.members.len()
            || self.members.keys().any(|k| !other.members.contains_key(k))
    }

    /// The members, in insertion order.
    pub(crate) fn into_values(self) -> Vec<Value> {
        self.members.into_values().collect()
    }
}

/// `a < b` with Python's rules for the values a JSON document can hold: strings compare by
/// code point, numbers (and bools) numerically, lists lexicographically; anything else is a
/// `TypeError`.
pub(crate) fn py_lt(a: &Value, b: &Value) -> Result<bool, SemanticaError> {
    use std::cmp::Ordering;
    if let (Some(x), Some(y)) = (PyNum::of(a), PyNum::of(b)) {
        return Ok(x.compare(y) == Some(Ordering::Less));
    }
    match (a, b) {
        (Value::String(x), Value::String(y)) => Ok(x < y),
        (Value::Array(x), Value::Array(y)) => {
            for (p, q) in x.iter().zip(y) {
                if !sem_utils::pycompat::fmt::py_eq(p, q) {
                    return py_lt(p, q);
                }
            }
            Ok(x.len() < y.len())
        }
        _ => Err(SemanticaError::Type(format!(
            "'<' not supported between instances of '{}' and '{}'",
            type_name(a),
            type_name(b)
        ))),
    }
}

/// `items.sort(key=..., reverse=True)` where `key` yields a JSON-like value: a stable sort in
/// descending key order, or the `TypeError` Python raises when two keys cannot be compared.
pub(crate) fn sort_desc_by_key<T>(
    items: &mut Vec<T>,
    key: impl Fn(&T) -> Value,
) -> Result<(), SemanticaError> {
    if items.len() < 2 {
        return Ok(());
    }
    let keys: Vec<Value> = items.iter().map(&key).collect();
    // Python only fails when it compares two incomparable keys; with two or more keys every
    // key takes part in some comparison. `sort(reverse=True)` reverses the list and timsort
    // starts with `x[i-1] < x[i]` from the end of the original order, so check adjacent pairs
    // in that order to report the same operand types.
    for i in (1..keys.len()).rev() {
        py_lt(&keys[i - 1], &keys[i])?;
        py_lt(&keys[i], &keys[i - 1])?;
    }
    let mut indexed: Vec<(Value, T)> = keys.into_iter().zip(items.drain(..)).collect();
    // Stable sort by descending key: `b < a` puts larger keys first and keeps ties in order.
    indexed.sort_by(|(ka, _), (kb, _)| {
        if py_lt(kb, ka).unwrap_or(false) {
            std::cmp::Ordering::Less
        } else if py_lt(ka, kb).unwrap_or(false) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    items.extend(indexed.into_iter().map(|(_, item)| item));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// datetime.fromisoformat (CPython 3.11, Modules/_datetimemodule.c)
// ---------------------------------------------------------------------------------------------

/// A datetime as `datetime.fromisoformat` builds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IsoDateTime {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    pub microsecond: i64,
    /// The UTC offset in microseconds (`None` for a naive datetime).
    pub offset_us: Option<i64>,
}

fn invalid_isoformat(s: &str) -> SemanticaError {
    SemanticaError::Value(format!(
        "Invalid isoformat string: {}",
        sem_utils::pycompat::fmt::str_repr(s)
    ))
}

/// The byte at `i`, or the C string terminator past the end.
fn at(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

/// `parse_digits`: exactly `n` ASCII digits starting at `*pos`.
fn parse_digits(b: &[u8], pos: &mut usize, n: usize) -> Option<i64> {
    let mut value = 0i64;
    for _ in 0..n {
        let c = at(b, *pos);
        *pos += 1;
        if !c.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i64::from(c - b'0');
    }
    Some(value)
}

/// `_find_isoformat_datetime_separator` (`None` for the C function's `-1`).
fn find_isoformat_datetime_separator(b: &[u8]) -> Option<usize> {
    let len = b.len();
    if len == 7 {
        return Some(7);
    }
    if at(b, 4) == b'-' {
        if at(b, 5) == b'W' {
            if len < 8 {
                return None;
            }
            if len > 8 && at(b, 8) == b'-' {
                if len == 9 {
                    return None;
                }
                if len > 10 && at(b, 10).is_ascii_digit() {
                    return Some(8);
                }
                return Some(10);
            }
            Some(8)
        } else {
            Some(10)
        }
    } else if at(b, 4) == b'W' {
        let mut idx = 7;
        while idx < len {
            if !at(b, idx).is_ascii_digit() {
                break;
            }
            idx += 1;
        }
        if idx < 9 {
            return Some(idx);
        }
        if idx % 2 == 0 { Some(7) } else { Some(8) }
    } else {
        Some(8)
    }
}

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

const DAYS_BEFORE_MONTH: [i64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
const DAYS_IN_MONTH: [i64; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn days_in_month(year: i64, month: i64) -> i64 {
    if month == 2 && is_leap(year) {
        29
    } else {
        DAYS_IN_MONTH[month as usize]
    }
}

fn days_before_year(year: i64) -> i64 {
    let y = year - 1;
    y * 365 + y / 4 - y / 100 + y / 400
}

fn ymd_to_ord(year: i64, month: i64, day: i64) -> i64 {
    let mut days = DAYS_BEFORE_MONTH[month as usize];
    if month > 2 && is_leap(year) {
        days += 1;
    }
    days_before_year(year) + days + day
}

fn ord_to_ymd(ordinal: i64) -> Option<(i64, i64, i64)> {
    const DI400Y: i64 = 146_097;
    const DI100Y: i64 = 36_524;
    const DI4Y: i64 = 1_461;
    if ordinal < 1 {
        return None;
    }
    let ordinal = ordinal - 1;
    let n400 = ordinal / DI400Y;
    let mut n = ordinal % DI400Y;
    let mut year = n400 * 400 + 1;
    let n100 = n / DI100Y;
    n %= DI100Y;
    let n4 = n / DI4Y;
    n %= DI4Y;
    let n1 = n / 365;
    n %= 365;
    year += n100 * 100 + n4 * 4 + n1;
    if n1 == 4 || n100 == 4 {
        return Some((year - 1, 12, 31));
    }
    let leapyear = n1 == 3 && (n4 != 24 || n100 == 3);
    let mut month = (n + 50) >> 5;
    let mut preceding = DAYS_BEFORE_MONTH[month as usize] + i64::from(month > 2 && leapyear);
    if preceding > n {
        month -= 1;
        preceding -= days_in_month(year, month);
    }
    n -= preceding;
    Some((year, month, n + 1))
}

fn iso_week1_monday(year: i64) -> i64 {
    let first_day = ymd_to_ord(year, 1, 1);
    let first_weekday = (first_day + 6) % 7;
    let mut week1_monday = first_day - first_weekday;
    if first_weekday > 3 {
        week1_monday += 7;
    }
    week1_monday
}

/// `iso_to_ymd`. Week dates of ISO year 0 land on ordinals below 1, where the C conversion
/// yields year 1 with a month outside 1..12 (it even reads out of bounds); the datetime
/// constructor then reports `month must be in 1..12`, which `(1, 0, 0)` reproduces.
fn iso_to_ymd(iso_year: i64, iso_week: i64, iso_day: i64) -> Option<(i64, i64, i64)> {
    if iso_week <= 0 || iso_week >= 53 {
        let mut out_of_range = true;
        if iso_week == 53 {
            let first_weekday = (ymd_to_ord(iso_year, 1, 1) + 6) % 7;
            if first_weekday == 3 || (first_weekday == 2 && is_leap(iso_year)) {
                out_of_range = false;
            }
        }
        if out_of_range {
            return None;
        }
    }
    if iso_day <= 0 || iso_day >= 8 {
        return None;
    }
    let day_1 = iso_week1_monday(iso_year);
    let day_offset = (iso_week - 1) * 7 + iso_day - 1;
    Some(ord_to_ymd(day_1 + day_offset).unwrap_or((1, 0, 0)))
}

/// `repr(datetime.timedelta(microseconds=total_us))`.
fn timedelta_repr(total_us: i64) -> String {
    const DAY_US: i64 = 86_400_000_000;
    let days = total_us.div_euclid(DAY_US);
    let rest = total_us.rem_euclid(DAY_US);
    let (seconds, micros) = (rest / 1_000_000, rest % 1_000_000);
    let mut parts = Vec::new();
    if days != 0 {
        parts.push(format!("days={days}"));
    }
    if seconds != 0 {
        parts.push(format!("seconds={seconds}"));
    }
    if micros != 0 {
        parts.push(format!("microseconds={micros}"));
    }
    if parts.is_empty() {
        parts.push("0".to_string());
    }
    format!("datetime.timedelta({})", parts.join(", "))
}

/// `parse_isoformat_date(dtstr, len)`.
fn parse_isoformat_date(b: &[u8], len: usize) -> Option<(i64, i64, i64)> {
    let mut p = 0;
    let year = parse_digits(b, &mut p, 4)?;
    let uses_separator = at(b, p) == b'-';
    if uses_separator {
        p += 1;
    }
    if at(b, p) == b'W' {
        p += 1;
        let iso_week = parse_digits(b, &mut p, 2)?;
        let iso_day = if p < len {
            if uses_separator {
                let c = at(b, p);
                p += 1;
                if c != b'-' {
                    return None;
                }
            }
            parse_digits(b, &mut p, 1)?
        } else {
            1
        };
        return iso_to_ymd(year, iso_week, iso_day);
    }
    let month = parse_digits(b, &mut p, 2)?;
    if uses_separator {
        let c = at(b, p);
        p += 1;
        if c != b'-' {
            return None;
        }
    }
    let day = parse_digits(b, &mut p, 2)?;
    Some((year, month, day))
}

/// `parse_hh_mm_ss_ff(tstr, tstr_end, ...)`: `Some((not_at_end, [h, m, s, us]))`, where
/// `not_at_end` is the C function's return value 1 ("characters follow").
#[allow(clippy::needless_range_loop)]
fn parse_hh_mm_ss_ff(b: &[u8], start: usize, end: usize) -> Option<(bool, [i64; 4])> {
    const CORRECTION: [i64; 5] = [100_000, 10_000, 1_000, 100, 10];
    let mut vals = [0i64; 4];
    let mut p = start;
    let mut has_separator = true;
    for i in 0..3 {
        vals[i] = parse_digits(b, &mut p, 2)?;
        let c = at(b, p);
        p += 1;
        if i == 0 {
            has_separator = c == b':';
        }
        if p >= end {
            return Some((c != 0, vals));
        }
        if has_separator && c == b':' {
            continue;
        } else if c == b'.' || c == b',' {
            break;
        } else if !has_separator {
            p -= 1;
        } else {
            return None;
        }
    }
    let len_remains = end.saturating_sub(p);
    let to_parse = len_remains.min(6);
    let mut us = parse_digits(b, &mut p, to_parse)?;
    if (1..6).contains(&to_parse) {
        us *= CORRECTION[to_parse - 1];
    }
    while at(b, p).is_ascii_digit() {
        p += 1;
    }
    vals[3] = us;
    Some((at(b, p) != 0, vals))
}

/// `parse_isoformat_time`: `Ok(([h, m, s, us], tz))` where `tz` is `(offset_seconds, offset_us)`.
#[allow(clippy::type_complexity)]
fn parse_isoformat_time(
    b: &[u8],
    start: usize,
    end: usize,
) -> Option<([i64; 4], Option<(i64, i64)>)> {
    let mut tz = start;
    loop {
        let c = at(b, tz);
        if c == b'Z' || c == b'+' || c == b'-' {
            break;
        }
        tz += 1;
        if tz >= end {
            break;
        }
    }
    let (not_at_end, comps) = parse_hh_mm_ss_ff(b, start, tz)?;
    if tz == end {
        return if not_at_end {
            None
        } else {
            Some((comps, None))
        };
    }
    if at(b, tz) == b'Z' {
        return if at(b, tz + 1) != 0 {
            None
        } else {
            Some((comps, Some((0, 0))))
        };
    }
    let sign = if at(b, tz) == b'-' { -1 } else { 1 };
    let (tz_not_at_end, tzc) = parse_hh_mm_ss_ff(b, tz + 1, end)?;
    if tz_not_at_end {
        return None;
    }
    let offset = sign * (tzc[0] * 3600 + tzc[1] * 60 + tzc[2]);
    Some((comps, Some((offset, sign * tzc[3]))))
}

/// `datetime.fromisoformat(s)` (CPython 3.11): the parsed fields, or the `ValueError` Python
/// raises.
pub(crate) fn fromisoformat(s: &str) -> Result<IsoDateTime, SemanticaError> {
    if s.chars().count() < 7 {
        return Err(invalid_isoformat(s));
    }
    let b = s.as_bytes();
    let len = b.len();
    let separator = find_isoformat_datetime_separator(b).ok_or_else(|| invalid_isoformat(s))?;
    let (year, month, day) =
        parse_isoformat_date(b, separator).ok_or_else(|| invalid_isoformat(s))?;
    let (time, tz) = if len > separator {
        // The separator is one character: skip its UTF-8 bytes (judged by the lead byte).
        let lead = b[separator];
        let width = if lead & 0x80 == 0 {
            1
        } else {
            match lead & 0xf0 {
                0xe0 => 3,
                0xf0 => 4,
                _ => 2,
            }
        };
        let start = separator + width;
        if start > len {
            return Err(invalid_isoformat(s));
        }
        parse_isoformat_time(b, start, len).ok_or_else(|| invalid_isoformat(s))?
    } else {
        ([0, 0, 0, 0], None)
    };
    let offset_us = match tz {
        None => None,
        Some((0, _)) => Some(0),
        Some((seconds, micros)) => {
            let total = seconds * 1_000_000 + micros;
            if total <= -86_400_000_000 || total >= 86_400_000_000 {
                return Err(SemanticaError::Value(format!(
                    "offset must be a timedelta strictly between -timedelta(hours=24) and \
                     timedelta(hours=24), not {}.",
                    timedelta_repr(total)
                )));
            }
            Some(total)
        }
    };
    if !(1..=9999).contains(&year) {
        return Err(SemanticaError::Value(format!(
            "year {year} is out of range"
        )));
    }
    if !(1..=12).contains(&month) {
        return Err(SemanticaError::Value("month must be in 1..12".to_string()));
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err(SemanticaError::Value(
            "day is out of range for month".to_string(),
        ));
    }
    let [hour, minute, second, microsecond] = time;
    if !(0..=23).contains(&hour) {
        return Err(SemanticaError::Value("hour must be in 0..23".to_string()));
    }
    if !(0..=59).contains(&minute) {
        return Err(SemanticaError::Value("minute must be in 0..59".to_string()));
    }
    if !(0..=59).contains(&second) {
        return Err(SemanticaError::Value("second must be in 0..59".to_string()));
    }
    if !(0..=999_999).contains(&microsecond) {
        return Err(SemanticaError::Value(
            "microsecond must be in 0..999999".to_string(),
        ));
    }
    Ok(IsoDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        offset_us,
    })
}

// ---------------------------------------------------------------------------------------------
// urllib.parse (CPython 3.11.14)
// ---------------------------------------------------------------------------------------------

const USES_RELATIVE: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "imap", "wais", "file", "https", "shttp", "mms",
    "prospero", "rtsp", "rtsps", "rtspu", "sftp", "svn", "svn+ssh", "ws", "wss",
];

const USES_NETLOC: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "telnet", "imap", "wais", "file", "mms", "https", "shttp",
    "snews", "prospero", "rtsp", "rtsps", "rtspu", "rsync", "svn", "svn+ssh", "sftp", "nfs", "git",
    "git+ssh", "ws", "wss",
];

const USES_PARAMS: &[&str] = &[
    "", "ftp", "hdl", "prospero", "http", "imap", "https", "shttp", "rtsp", "rtsps", "rtspu",
    "sip", "sips", "mms", "sftp", "tel",
];

const SCHEME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-.";

/// `urlparse(...)` result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ParseResult {
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    pub params: String,
    pub query: String,
    pub fragment: String,
}

fn invalid_ipv6_url() -> SemanticaError {
    SemanticaError::Value("Invalid IPv6 URL".to_string())
}

fn is_c0_control_or_space(c: char) -> bool {
    c <= ' '
}

/// `_splitnetloc(url, start)`.
fn splitnetloc(url: &str, start: usize) -> (String, String) {
    let delim = url[start..]
        .find(['/', '?', '#'])
        .map(|i| i + start)
        .unwrap_or(url.len());
    (url[start..delim].to_string(), url[delim..].to_string())
}

/// `ipaddress.IPv4Address(text)` validity.
fn is_ipv4(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let octets: Vec<&str> = text.split('.').collect();
    if octets.len() != 4 {
        return false;
    }
    octets.iter().all(|octet| {
        !octet.is_empty()
            && octet.bytes().all(|b| b.is_ascii_digit())
            && octet.len() <= 3
            && (*octet == "0" || !octet.starts_with('0'))
            && octet.parse::<u32>().map(|v| v <= 255).unwrap_or(false)
    })
}

/// `ipaddress.IPv6Address(text)` validity (including a `%scope` suffix).
fn is_ipv6(text: &str) -> bool {
    let addr = match text.split_once('%') {
        Some((addr, scope)) => {
            if scope.is_empty() || scope.contains('%') {
                return false;
            }
            addr
        }
        None => text,
    };
    if addr.is_empty() {
        return false;
    }
    let mut parts: Vec<String> = addr.split(':').map(str::to_string).collect();
    if parts.len() < 3 {
        return false;
    }
    if parts.last().is_some_and(|p| p.contains('.')) {
        let last = parts.pop().unwrap_or_default();
        if !is_ipv4(&last) {
            return false;
        }
        parts.push("0".to_string());
        parts.push("0".to_string());
    }
    const HEXTETS: usize = 8;
    if parts.len() > HEXTETS + 1 {
        return false;
    }
    let mut skip_index = None;
    for (i, part) in parts.iter().enumerate().take(parts.len() - 1).skip(1) {
        if part.is_empty() {
            if skip_index.is_some() {
                return false;
            }
            skip_index = Some(i);
        }
    }
    let (parts_hi, parts_lo) = if let Some(skip) = skip_index {
        let mut hi = skip;
        let mut lo = parts.len() - skip - 1;
        if parts[0].is_empty() {
            hi -= 1;
            if hi > 0 {
                return false;
            }
        }
        if parts[parts.len() - 1].is_empty() {
            lo -= 1;
            if lo > 0 {
                return false;
            }
        }
        if HEXTETS < hi + lo + 1 {
            return false;
        }
        (hi, lo)
    } else {
        if parts.len() != HEXTETS || parts[0].is_empty() || parts[parts.len() - 1].is_empty() {
            return false;
        }
        (parts.len(), 0)
    };
    let hextet_ok =
        |h: &str| !h.is_empty() && h.len() <= 4 && h.bytes().all(|b| b.is_ascii_hexdigit());
    parts[..parts_hi].iter().all(|h| hextet_ok(h))
        && parts[parts.len() - parts_lo..].iter().all(|h| hextet_ok(h))
}

static IPV_FUTURE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^v[a-fA-F0-9]+\..+\z").expect("valid regex"));

/// `_check_bracketed_host(hostname)`.
fn check_bracketed_host(hostname: &str) -> Result<(), SemanticaError> {
    if hostname.starts_with('v') {
        if !IPV_FUTURE.is_match(hostname) {
            return Err(SemanticaError::Value(
                "IPvFuture address is invalid".to_string(),
            ));
        }
        return Ok(());
    }
    if is_ipv4(hostname) {
        return Err(SemanticaError::Value(
            "An IPv4 address cannot be in brackets".to_string(),
        ));
    }
    if !is_ipv6(hostname) {
        return Err(SemanticaError::Value(format!(
            "{} does not appear to be an IPv4 or IPv6 address",
            sem_utils::pycompat::fmt::str_repr(hostname)
        )));
    }
    Ok(())
}

/// `_check_bracketed_netloc(netloc)`.
fn check_bracketed_netloc(netloc: &str) -> Result<(), SemanticaError> {
    let hostname_and_port = match netloc.rfind('@') {
        Some(i) => &netloc[i + 1..],
        None => netloc,
    };
    let hostname = match hostname_and_port.split_once('[') {
        Some((before, bracketed)) => {
            if !before.is_empty() {
                return Err(invalid_ipv6_url());
            }
            let (hostname, port) = bracketed.split_once(']').unwrap_or((bracketed, ""));
            if !port.is_empty() && !port.starts_with(':') {
                return Err(invalid_ipv6_url());
            }
            hostname
        }
        None => hostname_and_port
            .split_once(':')
            .map(|(h, _)| h)
            .unwrap_or(hostname_and_port),
    };
    check_bracketed_host(hostname)
}

/// `_checknetloc(netloc)`: reject netlocs whose NFKC form introduces URL delimiters.
fn checknetloc(netloc: &str) -> Result<(), SemanticaError> {
    if netloc.is_empty() || netloc.is_ascii() {
        return Ok(());
    }
    let n: String = netloc
        .chars()
        .filter(|c| !matches!(c, '@' | ':' | '#' | '?'))
        .collect();
    let normalized: String = n.nfkc().collect();
    if n == normalized {
        return Ok(());
    }
    for c in ['/', '?', '#', '@', ':'] {
        if normalized.contains(c) {
            return Err(SemanticaError::Value(format!(
                "netloc '{netloc}' contains invalid characters under NFKC normalization"
            )));
        }
    }
    Ok(())
}

/// `urlsplit(url, scheme, allow_fragments=True)` as `(scheme, netloc, path, query, fragment)`.
fn urlsplit(url: &str, scheme: &str) -> Result<[String; 5], SemanticaError> {
    let mut url: String = url
        .trim_start_matches(is_c0_control_or_space)
        .replace(['\t', '\r', '\n'], "");
    let mut scheme: String = scheme
        .trim_matches(is_c0_control_or_space)
        .replace(['\t', '\r', '\n'], "");
    let mut netloc = String::new();
    let mut query = String::new();
    let mut fragment = String::new();
    if let Some(i) = url.find(':')
        && i > 0
        && url.as_bytes()[0].is_ascii_alphabetic()
        && url[..i].chars().all(|c| SCHEME_CHARS.contains(c))
    {
        scheme = url[..i].to_ascii_lowercase();
        url = url[i + 1..].to_string();
    }
    if url.starts_with("//") {
        let (n, rest) = splitnetloc(&url, 2);
        netloc = n;
        url = rest;
        let open = netloc.contains('[');
        let close = netloc.contains(']');
        if open != close {
            return Err(invalid_ipv6_url());
        }
        if open && close {
            check_bracketed_netloc(&netloc)?;
        }
    }
    if let Some((before, after)) = url.split_once('#') {
        fragment = after.to_string();
        url = before.to_string();
    }
    if let Some((before, after)) = url.split_once('?') {
        query = after.to_string();
        url = before.to_string();
    }
    checknetloc(&netloc)?;
    Ok([scheme, netloc, url, query, fragment])
}

/// `_splitparams(url)`.
fn splitparams(url: &str) -> (String, String) {
    let i = if let Some(slash) = url.rfind('/') {
        match url[slash..].find(';') {
            Some(j) => slash + j,
            None => return (url.to_string(), String::new()),
        }
    } else {
        match url.find(';') {
            Some(i) => i,
            None => return (url.to_string(), String::new()),
        }
    };
    (url[..i].to_string(), url[i + 1..].to_string())
}

/// `urlparse(url, scheme)`.
pub(crate) fn urlparse(url: &str, scheme: &str) -> Result<ParseResult, SemanticaError> {
    let [scheme, netloc, path, query, fragment] = urlsplit(url, scheme)?;
    let (path, params) = if USES_PARAMS.contains(&scheme.as_str()) && path.contains(';') {
        splitparams(&path)
    } else {
        (path, String::new())
    };
    Ok(ParseResult {
        scheme,
        netloc,
        path,
        params,
        query,
        fragment,
    })
}

/// `urlunsplit((scheme, netloc, url, query, fragment))`.
fn urlunsplit(scheme: &str, netloc: &str, url: &str, query: &str, fragment: &str) -> String {
    let mut url = url.to_string();
    if !netloc.is_empty()
        || (!scheme.is_empty() && USES_NETLOC.contains(&scheme))
        || url.starts_with("//")
    {
        if !url.is_empty() && !url.starts_with('/') {
            url = format!("/{url}");
        }
        url = format!("//{netloc}{url}");
    }
    if !scheme.is_empty() {
        url = format!("{scheme}:{url}");
    }
    if !query.is_empty() {
        url = format!("{url}?{query}");
    }
    if !fragment.is_empty() {
        url = format!("{url}#{fragment}");
    }
    url
}

/// `urlunparse((scheme, netloc, url, params, query, fragment))`.
fn urlunparse(p: &ParseResult) -> String {
    let url = if p.params.is_empty() {
        p.path.clone()
    } else {
        format!("{};{}", p.path, p.params)
    };
    urlunsplit(&p.scheme, &p.netloc, &url, &p.query, &p.fragment)
}

/// `urllib.parse.urljoin(base, url)`.
pub(crate) fn urljoin(base: &str, url: &str) -> Result<String, SemanticaError> {
    if base.is_empty() {
        return Ok(url.to_string());
    }
    if url.is_empty() {
        return Ok(base.to_string());
    }
    let b = urlparse(base, "")?;
    let mut u = urlparse(url, &b.scheme)?;
    if u.scheme != b.scheme || !USES_RELATIVE.contains(&u.scheme.as_str()) {
        return Ok(url.to_string());
    }
    if USES_NETLOC.contains(&u.scheme.as_str()) {
        if !u.netloc.is_empty() {
            return Ok(urlunparse(&u));
        }
        u.netloc = b.netloc.clone();
    }
    if u.path.is_empty() && u.params.is_empty() {
        u.path = b.path.clone();
        u.params = b.params.clone();
        if u.query.is_empty() {
            u.query = b.query.clone();
        }
        return Ok(urlunparse(&u));
    }
    let mut base_parts: Vec<&str> = b.path.split('/').collect();
    if base_parts.last().is_some_and(|last| !last.is_empty()) {
        base_parts.pop();
    }
    let segments: Vec<&str> = if u.path.starts_with('/') {
        u.path.split('/').collect()
    } else {
        let mut segments: Vec<&str> = base_parts;
        segments.extend(u.path.split('/'));
        if segments.len() > 2 {
            let last = segments[segments.len() - 1];
            let middle: Vec<&str> = segments[1..segments.len() - 1]
                .iter()
                .copied()
                .filter(|s| !s.is_empty())
                .collect();
            let first = segments[0];
            segments = std::iter::once(first)
                .chain(middle)
                .chain(std::iter::once(last))
                .collect();
        }
        segments
    };
    let mut resolved: Vec<&str> = Vec::new();
    for seg in &segments {
        match *seg {
            ".." => {
                resolved.pop();
            }
            "." => {}
            other => resolved.push(other),
        }
    }
    if matches!(segments.last(), Some(&".") | Some(&"..")) {
        resolved.push("");
    }
    let joined = resolved.join("/");
    u.path = if joined.is_empty() {
        "/".to_string()
    } else {
        joined
    };
    Ok(urlunparse(&u))
}

/// `urllib.parse.quote(string, safe)` (UTF-8, upper-case escapes).
pub(crate) fn quote(string: &str, safe: &str) -> String {
    if string.is_empty() {
        return String::new();
    }
    let safe: Vec<u8> = safe.bytes().filter(|b| *b < 128).collect();
    let mut out = String::with_capacity(string.len());
    for b in string.bytes() {
        if b.is_ascii_alphanumeric() || b"_.-~".contains(&b) || safe.contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Differential tests against `tests/fixtures/oracle.json` (computed by the original Python
    //! code, see `tests/fixtures/generate_oracle.py`).

    use super::*;
    use std::sync::OnceLock;

    fn oracle() -> &'static Value {
        static ORACLE: OnceLock<Value> = OnceLock::new();
        ORACLE.get_or_init(|| {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/oracle.json");
            serde_json::from_str(&std::fs::read_to_string(path).expect("oracle fixture"))
                .expect("valid oracle JSON")
        })
    }

    fn outcome<T: Into<Value>>(result: Result<T, SemanticaError>) -> Value {
        match result {
            Ok(v) => serde_json::json!({"ok": v.into()}),
            Err(e) => serde_json::json!({"error": e.type_name(), "message": e.to_string()}),
        }
    }

    #[test]
    fn fromisoformat_matches_cpython_on_oracle_corpus() {
        let cases = oracle()["fromisoformat"].as_array().unwrap();
        assert!(cases.len() > 4000);
        let mut mismatches = Vec::new();
        for case in cases {
            let s = case["s"].as_str().unwrap();
            let actual = match fromisoformat(s) {
                Ok(dt) => serde_json::json!({"ok": [dt.year, dt.month, dt.day, dt.hour, dt.minute,
                    dt.second, dt.microsecond, dt.offset_us]}),
                Err(e) => serde_json::json!({"error": e.to_string()}),
            };
            let expected = match case.get("ok") {
                Some(ok) => serde_json::json!({"ok": ok}),
                None => serde_json::json!({"error": case["error"]}),
            };
            if actual != expected {
                mismatches.push(format!("{s:?}: expected {expected}, got {actual}"));
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} mismatches:\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
    }

    #[test]
    fn json_dumps_floats_matches_python() {
        let case = &oracle()["json_floats"];
        assert_eq!(
            json_dumps_default(&case["floats"]),
            case["dumps"].as_str().unwrap()
        );
        let mut wrapped = Map::new();
        wrapped.insert("v".into(), case["floats"].clone());
        let text = json_dumps_canonical(&wrapped, None);
        use sha2::{Digest, Sha256};
        assert_eq!(
            hex::encode(Sha256::digest(text.as_bytes())),
            case["checksum"].as_str().unwrap()
        );
    }

    #[test]
    fn json_dumps_strings_matches_python() {
        let case = &oracle()["json_strings"];
        assert_eq!(
            json_dumps_default(&case["strings"]),
            case["dumps"].as_str().unwrap()
        );
        let map: Map<String, Value> = case["strings"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str().unwrap().to_string(), Value::from(i)))
            .collect();
        assert_eq!(
            json_dumps_map_default(&map),
            case["dumps_map"].as_str().unwrap()
        );
    }

    #[test]
    fn write_float_fast_path_matches_float_repr() {
        let mut x = 1.0e-6f64;
        while x < 1.0e18 {
            for v in [x, -x, x * 1.234_567_890_123, x / 3.0, x + 0.5] {
                let mut out = String::new();
                write_float(&mut out, v);
                assert_eq!(out, float_repr(v), "{v:e}");
            }
            x *= 1.7;
        }
    }

    #[test]
    fn urljoin_matches_python() {
        let mut mismatches = Vec::new();
        for case in oracle()["urljoin"].as_array().unwrap() {
            let (base, url) = (
                case["base"].as_str().unwrap(),
                case["url"].as_str().unwrap(),
            );
            let actual = outcome(urljoin(base, url));
            if actual != case["result"] {
                mismatches.push(format!(
                    "urljoin({base:?}, {url:?}): expected {}, got {actual}",
                    case["result"]
                ));
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    #[test]
    fn urlparse_matches_python() {
        let mut mismatches = Vec::new();
        for case in oracle()["urlparse"].as_array().unwrap() {
            let url = case["url"].as_str().unwrap();
            let actual = outcome(urlparse(url, "").map(|p| {
                serde_json::json!([p.scheme, p.netloc, p.path, p.params, p.query, p.fragment])
            }));
            if actual != case["result"] {
                mismatches.push(format!(
                    "urlparse({url:?}): expected {}, got {actual}",
                    case["result"]
                ));
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    #[test]
    fn quote_matches_python() {
        for case in oracle()["quote"].as_array().unwrap() {
            let s = case["s"].as_str().unwrap();
            assert_eq!(
                quote(s, "/:?&=@[]!$'()*+,%-._~"),
                case["result"].as_str().unwrap(),
                "{s:?}"
            );
        }
    }

    #[test]
    fn py_set_keeps_first_equal_member() {
        let items = [
            Value::from(1),
            Value::from(1.0),
            Value::Bool(true),
            Value::from("1"),
        ];
        let set = PySet::from_items(items.iter()).unwrap();
        assert_eq!(set.into_values(), vec![Value::from(1), Value::from("1")]);
        let err = PySet::from_items([serde_json::json!([1])].iter()).unwrap_err();
        assert_eq!(err.to_string(), "unhashable type: 'list'");
    }

    #[test]
    fn sort_desc_is_stable_and_reports_python_type_error() {
        let mut items = vec!["x", "x2", "y"];
        let keys = |s: &&str| Value::from(if s.starts_with('x') { "x" } else { "y" });
        sort_desc_by_key(&mut items, keys).unwrap();
        assert_eq!(items, vec!["y", "x", "x2"]);
        let mut mixed = vec![Value::Null, Value::from("2024")];
        let err = sort_desc_by_key(&mut mixed, |v| v.clone()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "'<' not supported between instances of 'NoneType' and 'str'"
        );
    }
}
