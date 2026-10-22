//! Python object semantics for the JSON-like values the deduplication module works on.
//!
//! The original manipulates entity dictionaries with `dict.get`, `in`, `len`, `str()`,
//! comparisons and `set`s; these helpers reproduce what CPython 3.11 does for
//! `serde_json::Value`s, including the exception it raises for a value of the wrong type
//! (`AttributeError: 'list' object has no attribute 'get'`, `TypeError: unhashable type`, ...).

use std::cmp::Ordering;

use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::{PyNum, is_space, py_eq, strip, type_name};
use serde_json::{Map, Value};

pub(crate) use sem_utils::pycompat::fmt::{py_repr, py_str, truthy};

/// `AttributeError: '<type>' object has no attribute '<attr>'`.
pub(crate) fn attr_error(value: &Value, attr: &str) -> SemanticaError {
    SemanticaError::Attribute(format!(
        "'{}' object has no attribute '{attr}'",
        type_name(value)
    ))
}

/// `TypeError` with `message`.
pub(crate) fn type_error(message: impl Into<String>) -> SemanticaError {
    SemanticaError::Type(message.into())
}

/// `obj.get(key)` (`None` when the key is missing); `AttributeError` when `obj` is not a dict.
pub(crate) fn get<'a>(obj: &'a Value, key: &str) -> Result<Option<&'a Value>, SemanticaError> {
    match obj {
        Value::Object(map) => Ok(map.get(key)),
        other => Err(attr_error(other, "get")),
    }
}

/// `obj.get(key, default)`.
pub(crate) fn get_or(obj: &Value, key: &str, default: Value) -> Result<Value, SemanticaError> {
    Ok(get(obj, key)?.cloned().unwrap_or(default))
}

/// `obj.get(key)` read as Python's `None`-aware value: missing keys and `null` are `None`.
pub(crate) fn get_some<'a>(
    obj: &'a Value,
    key: &str,
) -> Result<Option<&'a Value>, SemanticaError> {
    Ok(get(obj, key)?.filter(|v| !v.is_null()))
}

/// `key in obj` for a dict.
pub(crate) fn has_key(obj: &Value, key: &str) -> Result<bool, SemanticaError> {
    match obj {
        Value::Object(map) => Ok(map.contains_key(key)),
        // `"k" in x` for other types: substring/element tests or a TypeError.
        other => contains(other, &Value::String(key.to_string())),
    }
}

/// `a or b`: `a` when truthy, else `b`.
pub(crate) fn or<'a>(a: Option<&'a Value>, b: &'a Value) -> &'a Value {
    match a {
        Some(v) if truthy(v) => v,
        _ => b,
    }
}

/// `entity.get("name") or entity.get("text") or ""`.
pub(crate) fn name_or_text(entity: &Value) -> Result<Value, SemanticaError> {
    let name = get(entity, "name")?;
    if let Some(v) = name.filter(|v| truthy(v)) {
        return Ok(v.clone());
    }
    let text = get(entity, "text")?;
    if let Some(v) = text.filter(|v| truthy(v)) {
        return Ok(v.clone());
    }
    Ok(Value::String(String::new()))
}

/// The string inside `value` for a `str` method call (`value.<method>()`), or the
/// `AttributeError` a non-string raises.
pub(crate) fn str_for<'a>(value: &'a Value, method: &str) -> Result<&'a str, SemanticaError> {
    value.as_str().ok_or_else(|| attr_error(value, method))
}

/// `str.lower()`.
pub(crate) fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// `str.upper()`.
pub(crate) fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// `s.lower().strip()`.
pub(crate) fn lower_strip(s: &str) -> String {
    strip(&lower(s)).to_string()
}

/// `str.split()` with no arguments.
pub(crate) fn split_ws(s: &str) -> Vec<&str> {
    s.split(is_space).filter(|p| !p.is_empty()).collect()
}

/// `" ".join(s.lower().split())`.
pub(crate) fn normalize_spaces_lower(s: &str) -> String {
    split_ws(&lower(s)).join(" ")
}

/// `s[:n]` over code points.
pub(crate) fn prefix_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// `len(value)`.
pub(crate) fn py_len(value: &Value) -> Result<usize, SemanticaError> {
    match value {
        Value::String(s) => Ok(s.chars().count()),
        Value::Array(a) => Ok(a.len()),
        Value::Object(o) => Ok(o.len()),
        other => Err(type_error(format!(
            "object of type '{}' has no len()",
            type_name(other)
        ))),
    }
}

/// `list(value)`: the items Python iteration yields (dict keys, string characters).
pub(crate) fn iterate(value: &Value) -> Result<Vec<Value>, SemanticaError> {
    match value {
        Value::Array(a) => Ok(a.clone()),
        Value::Object(o) => Ok(o.keys().map(|k| Value::String(k.clone())).collect()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string())).collect()),
        other => Err(type_error(format!(
            "'{}' object is not iterable",
            type_name(other)
        ))),
    }
}

/// `dict.keys()` of a mapping (`AttributeError` otherwise).
pub(crate) fn keys(value: &Value) -> Result<Vec<String>, SemanticaError> {
    match value {
        Value::Object(o) => Ok(o.keys().cloned().collect()),
        other => Err(attr_error(other, "keys")),
    }
}

/// `dict.items()` of a mapping (`AttributeError` otherwise).
pub(crate) fn items(value: &Value) -> Result<Vec<(String, Value)>, SemanticaError> {
    match value {
        Value::Object(o) => Ok(o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        other => Err(attr_error(other, "items")),
    }
}

/// `value.copy()` (dicts and lists; `AttributeError` otherwise).
pub(crate) fn copy(value: &Value) -> Result<Value, SemanticaError> {
    match value {
        Value::Object(_) | Value::Array(_) => Ok(value.clone()),
        other => Err(attr_error(other, "copy")),
    }
}

/// `item in container`.
pub(crate) fn contains(container: &Value, item: &Value) -> Result<bool, SemanticaError> {
    match container {
        Value::Object(map) => {
            HKey::scalar(item)?;
            Ok(item.as_str().is_some_and(|k| map.contains_key(k)))
        }
        Value::Array(items) => Ok(items.iter().any(|x| py_eq(x, item))),
        Value::String(s) => match item {
            Value::String(needle) => Ok(s.contains(needle.as_str())),
            other => Err(type_error(format!(
                "'in <string>' requires string as left operand, not {}",
                type_name(other)
            ))),
        },
        other => Err(type_error(format!(
            "argument of type '{}' is not iterable",
            type_name(other)
        ))),
    }
}

/// Python equality (`==`).
pub(crate) fn eq(a: &Value, b: &Value) -> bool {
    py_eq(a, b)
}

/// The numeric reading of a Python number (`bool` counts as `int`).
pub(crate) fn number(value: &Value) -> Option<f64> {
    PyNum::of(value).map(PyNum::to_f64)
}

/// A comparison operator, for Python's rich comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    fn symbol(self) -> &'static str {
        match self {
            Cmp::Lt => "<",
            Cmp::Le => "<=",
            Cmp::Gt => ">",
            Cmp::Ge => ">=",
        }
    }

    fn holds(self, ordering: Ordering) -> bool {
        match self {
            Cmp::Lt => ordering == Ordering::Less,
            Cmp::Le => ordering != Ordering::Greater,
            Cmp::Gt => ordering == Ordering::Greater,
            Cmp::Ge => ordering != Ordering::Less,
        }
    }
}

/// `a <op> b` with Python's rules: numbers (and bools) numerically, strings by code point,
/// lists lexicographically; anything else is the `TypeError` CPython raises.
pub(crate) fn compare(a: &Value, b: &Value, op: Cmp) -> Result<bool, SemanticaError> {
    if let (Some(x), Some(y)) = (PyNum::of(a), PyNum::of(b)) {
        // NaN compares false with every operator.
        return Ok(x.compare(y).is_some_and(|o| op.holds(o)));
    }
    match (a, b) {
        (Value::String(x), Value::String(y)) => Ok(op.holds(x.as_str().cmp(y.as_str()))),
        (Value::Array(x), Value::Array(y)) => {
            for (p, q) in x.iter().zip(y) {
                if !py_eq(p, q) {
                    return compare(p, q, op);
                }
            }
            Ok(op.holds(x.len().cmp(&y.len())))
        }
        _ => Err(type_error(format!(
            "'{}' not supported between instances of '{}' and '{}'",
            op.symbol(),
            type_name(a),
            type_name(b)
        ))),
    }
}

/// `x <op> threshold` for a float `x` and an arbitrary threshold value.
pub(crate) fn compare_f64(x: f64, threshold: &Value, op: Cmp) -> Result<bool, SemanticaError> {
    compare(&float_value(x), threshold, op)
}

/// A float as a JSON value (`NaN`/`inf` cannot be stored and become `null`).
pub(crate) fn float_value(x: f64) -> Value {
    serde_json::Number::from_f64(x)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// `value * factor` for a float factor (`TypeError` for non-numbers).
pub(crate) fn mul_float(value: &Value, factor: f64) -> Result<f64, SemanticaError> {
    if let Some(x) = number(value) {
        return Ok(x * factor);
    }
    match value {
        Value::String(_) | Value::Array(_) => Err(type_error(
            "can't multiply sequence by non-int of type 'float'",
        )),
        other => Err(type_error(format!(
            "unsupported operand type(s) for *: '{}' and 'float'",
            type_name(other)
        ))),
    }
}

/// `a + b` for JSON-like values: numbers add, strings and lists concatenate.
pub(crate) fn add(a: &Value, b: &Value) -> Result<Value, SemanticaError> {
    if let (Some(x), Some(y)) = (PyNum::of(a), PyNum::of(b)) {
        return Ok(match (x, y) {
            (PyNum::Int(i), PyNum::Int(j)) => match i.checked_add(j) {
                Some(sum) => int_value(sum),
                None => float_value(i as f64 + j as f64),
            },
            _ => float_value(x.to_f64() + y.to_f64()),
        });
    }
    match (a, b) {
        (Value::String(x), Value::String(y)) => Ok(Value::String(format!("{x}{y}"))),
        (Value::Array(x), Value::Array(y)) => {
            let mut out = x.clone();
            out.extend(y.iter().cloned());
            Ok(Value::Array(out))
        }
        (Value::String(_), other) => Err(type_error(format!(
            "can only concatenate str (not \"{}\") to str",
            type_name(other)
        ))),
        (Value::Array(_), other) => Err(type_error(format!(
            "can only concatenate list (not \"{}\") to list",
            type_name(other)
        ))),
        _ => Err(type_error(format!(
            "unsupported operand type(s) for +: '{}' and '{}'",
            type_name(a),
            type_name(b)
        ))),
    }
}

/// `a * b` for JSON-like values: numbers multiply, an int repeats a string or list.
pub(crate) fn mul(a: &Value, b: &Value) -> Result<Value, SemanticaError> {
    if let (Some(x), Some(y)) = (PyNum::of(a), PyNum::of(b)) {
        return Ok(match (x, y) {
            (PyNum::Int(i), PyNum::Int(j)) => match i.checked_mul(j) {
                Some(product) => int_value(product),
                None => float_value(i as f64 * j as f64),
            },
            _ => float_value(x.to_f64() * y.to_f64()),
        });
    }
    let repeat = |seq: &Value, count: &Value| -> Option<Value> {
        let n = match PyNum::of(count) {
            Some(PyNum::Int(n)) => n.max(0) as usize,
            _ => return None,
        };
        match seq {
            Value::String(s) => Some(Value::String(s.repeat(n))),
            Value::Array(items) => Some(Value::Array(
                std::iter::repeat_n(items.iter().cloned(), n)
                    .flatten()
                    .collect(),
            )),
            _ => None,
        }
    };
    if let Some(v) = repeat(a, b).or_else(|| repeat(b, a)) {
        return Ok(v);
    }
    match (a, b) {
        (Value::String(_) | Value::Array(_), other) | (other, Value::String(_) | Value::Array(_)) => {
            Err(type_error(format!(
                "can't multiply sequence by non-int of type '{}'",
                type_name(other)
            )))
        }
        _ => Err(type_error(format!(
            "unsupported operand type(s) for *: '{}' and '{}'",
            type_name(a),
            type_name(b)
        ))),
    }
}

/// `a - b` for numbers (`TypeError` otherwise).
pub(crate) fn sub(a: &Value, b: &Value) -> Result<f64, SemanticaError> {
    match (number(a), number(b)) {
        (Some(x), Some(y)) => Ok(x - y),
        _ => Err(type_error(format!(
            "unsupported operand type(s) for -: '{}' and '{}'",
            type_name(a),
            type_name(b)
        ))),
    }
}

fn int_value(i: i128) -> Value {
    if let Ok(v) = i64::try_from(i) {
        Value::from(v)
    } else if let Ok(v) = u64::try_from(i) {
        Value::from(v)
    } else {
        float_value(i as f64)
    }
}

/// `isinstance(value, int)` (true for bools, as in Python).
pub(crate) fn is_int(value: &Value) -> bool {
    match value {
        Value::Bool(_) => true,
        Value::Number(n) => !n.is_f64(),
        _ => false,
    }
}

/// The `TypeError` for calling `callable(**value)` with a non-mapping.
pub(crate) fn mapping_error(callable: &str, value: &Value) -> SemanticaError {
    type_error(format!(
        "{callable}() argument after ** must be a mapping, not {}",
        type_name(value)
    ))
}

/// `callable(**value)`: the keyword arguments a mapping provides.
pub(crate) fn mapping(callable: &str, value: &Value) -> Result<Map<String, Value>, SemanticaError> {
    match value {
        Value::Object(map) => Ok(map.clone()),
        other => Err(mapping_error(callable, other)),
    }
}

/// `obj[key] = value` for a JSON container (only dicts accept string keys).
pub(crate) fn set_item(obj: &mut Value, key: &str, value: Value) -> Result<(), SemanticaError> {
    match obj {
        Value::Object(map) => {
            map.insert(key.to_string(), value);
            Ok(())
        }
        Value::Array(_) => Err(type_error(
            "list indices must be integers or slices, not str",
        )),
        other => Err(type_error(format!(
            "'{}' object does not support item assignment",
            type_name(other)
        ))),
    }
}

/// `container[key]` (read) for a string key.
pub(crate) fn subscript<'a>(container: &'a Value, key: &str) -> Result<&'a Value, SemanticaError> {
    match container {
        Value::Object(map) => map
            .get(key)
            .ok_or_else(|| SemanticaError::Key(key.to_string())),
        other => Err(subscript_error(other)),
    }
}

/// `container[key]` (for in-place updates) for a string key.
pub(crate) fn subscript_mut<'a>(
    container: &'a mut Value,
    key: &str,
) -> Result<&'a mut Value, SemanticaError> {
    match container {
        Value::Object(map) => map
            .get_mut(key)
            .ok_or_else(|| SemanticaError::Key(key.to_string())),
        other => Err(subscript_error(other)),
    }
}

fn subscript_error(container: &Value) -> SemanticaError {
    match container {
        Value::Array(_) => type_error("list indices must be integers or slices, not str"),
        Value::String(_) => type_error("string indices must be integers, not 'str'"),
        other => type_error(format!(
            "'{}' object is not subscriptable",
            type_name(other)
        )),
    }
}

/// A Python hashable value as a set element or dict key, with Python's equality: `1`, `1.0`
/// and `True` are the same key; tuples compare element-wise.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum HKey {
    None,
    Int(i128),
    /// A float that is not a whole number (by bit pattern).
    Float(u64),
    Str(String),
    Tuple(Vec<HKey>),
}

impl HKey {
    /// The key of a scalar value; lists and dicts are unhashable (`TypeError`).
    pub(crate) fn scalar(value: &Value) -> Result<HKey, SemanticaError> {
        match value {
            Value::Array(_) | Value::Object(_) => Err(type_error(format!(
                "unhashable type: '{}'",
                type_name(value)
            ))),
            other => Ok(Self::of_scalar(other)),
        }
    }

    fn of_scalar(value: &Value) -> HKey {
        match value {
            Value::Null => HKey::None,
            Value::String(s) => HKey::Str(s.clone()),
            other => match PyNum::of(other) {
                Some(PyNum::Int(i)) => HKey::Int(i),
                Some(PyNum::Float(f)) => Self::float(f),
                None => HKey::Str(other.to_string()),
            },
        }
    }

    /// The key of a float.
    pub(crate) fn float(f: f64) -> HKey {
        if f.is_finite() && f.fract() == 0.0 && f.abs() < 1.7e38 {
            HKey::Int(f as i128)
        } else {
            HKey::Float(f.to_bits())
        }
    }

    /// `SimilarityCalculator._make_hashable(item)`: dicts become sorted tuples of
    /// `(key, value)` pairs, lists become tuples, scalars stay as they are.
    pub(crate) fn hashable(value: &Value) -> HKey {
        match value {
            Value::Object(map) => {
                let mut pairs: Vec<(&String, &Value)> = map.iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(b.0));
                HKey::Tuple(
                    pairs
                        .into_iter()
                        .map(|(k, v)| HKey::Tuple(vec![HKey::Str(k.clone()), Self::hashable(v)]))
                        .collect(),
                )
            }
            Value::Array(items) => HKey::Tuple(items.iter().map(Self::hashable).collect()),
            other => Self::of_scalar(other),
        }
    }
}

/// `repr()` of a tuple of keys, as Python prints a tuple dict key (`('a1', 'a2')`).
pub(crate) fn tuple_repr(items: &[String]) -> String {
    if items.len() == 1 {
        format!("({},)", items[0])
    } else {
        format!("({})", items.join(", "))
    }
}
