//! Single neutral owner of JSON parsing and Python-compatible serialization.
//!
//! Wire parsing rejects duplicate keys and nonfinite floating-point values,
//! preserves object insertion order and arbitrary-precision integers, and does
//! not interpret serde's private number-marker names in real JSON objects.
//! Canonical serialization sorts object keys for existing Python v1 identities;
//! callers must not use it to reorder model requests or exact source bytes.

use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Deserializer, de};
use serde_json::{Map, Number, Value, value::RawValue};

mod python_unicode;
pub mod python_set;

const MAX_JSON_DEPTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonError {
    DuplicateJsonKey,
    NonfiniteJsonNumber,
    MalformedJson,
    NestingLimitExceeded,
}

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DuplicateJsonKey => "duplicate_json_key",
            Self::NonfiniteJsonNumber => "nonfinite_json_number",
            Self::MalformedJson => "malformed_json",
            Self::NestingLimitExceeded => "json_nesting_limit_exceeded",
        })
    }
}

impl std::error::Error for JsonError {}

struct ObjectMembers(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for ObjectMembers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = ObjectMembers;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("JSON object")
            }

            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    values.push((key, map.next_value::<Box<RawValue>>()?));
                }
                Ok(ObjectMembers(values))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// Parse finite JSON, including a duplicate-key check at every nesting level.
/// Float tokens follow Python's finite IEEE-754 semantics. Integer tokens retain
/// their full precision. No input text, keys or source paths appear in errors.
pub fn strict_json(raw: &[u8]) -> Result<Value, JsonError> {
    let raw = serde_json::from_slice::<Box<RawValue>>(raw).map_err(|_| JsonError::MalformedJson)?;
    strict_value(raw.get(), 0, JsonMode::Strict)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonMode {
    Strict,
    Protocol,
    PythonFinite,
    // Derived DTO Value originally accepts every valid arbitrary-precision JSON
    // number, including finite-token magnitudes beyond IEEE-754. Provider
    // finite-number checks must not silently become this contract boundary.
    DtoIdentity,
}

fn strict_value(raw: &str, depth: usize, mode: JsonMode) -> Result<Value, JsonError> {
    if (mode == JsonMode::Strict && depth >= MAX_JSON_DEPTH)
        || (mode != JsonMode::Strict && depth > MAX_JSON_DEPTH)
    {
        return Err(JsonError::NestingLimitExceeded);
    }
    match raw.as_bytes().first() {
        Some(b'{') => {
            let fields = serde_json::from_str::<ObjectMembers>(raw).map_err(|error| {
                if error.to_string().starts_with("duplicate_json_key") {
                    JsonError::DuplicateJsonKey
                } else {
                    JsonError::MalformedJson
                }
            })?;
            let mut map = Map::new();
            let mut keys = BTreeSet::new();
            for (key, value) in fields.0 {
                if mode == JsonMode::Strict && !keys.insert(key.clone()) {
                    return Err(JsonError::DuplicateJsonKey);
                }
                map.insert(key, strict_value(value.get(), depth + 1, mode)?);
            }
            Ok(Value::Object(map))
        }
        Some(b'[') => {
            let values: Vec<Box<RawValue>> =
                serde_json::from_str(raw).map_err(|_| JsonError::MalformedJson)?;
            values
                .iter()
                .map(|value| strict_value(value.get(), depth + 1, mode))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        Some(b'-' | b'0'..=b'9') if matches!(mode, JsonMode::Protocol | JsonMode::DtoIdentity) => {
            if mode == JsonMode::Protocol
                && raw.contains(['.', 'e', 'E'])
                && !raw.parse::<f64>().is_ok_and(f64::is_finite)
            {
                return Err(JsonError::NonfiniteJsonNumber);
            }
            serde_json::from_str(raw).map_err(|_| JsonError::MalformedJson)
        }
        Some(b'-' | b'0'..=b'9') if raw.contains(['.', 'e', 'E']) => {
            let value = raw.parse::<f64>().map_err(|_| JsonError::MalformedJson)?;
            Number::from_f64(value)
                .map(Value::Number)
                .ok_or(JsonError::NonfiniteJsonNumber)
        }
        Some(b'-') if raw == "-0" => Ok(Value::Number(0.into())),
        _ => serde_json::from_str(raw).map_err(|_| JsonError::MalformedJson),
    }
}

/// Original protocol profile: last-key-wins, exact integer/JSON number tokens,
/// finite floating values and the existing protocol nesting bound. Callers
/// choose their existing byte budget; this is not an engineering acceptance.
pub fn protocol_json(text: &str, max_bytes: usize) -> Result<Value, JsonError> {
    if text.len() > max_bytes {
        return Err(JsonError::MalformedJson);
    }
    let raw: Box<RawValue> = serde_json::from_str(text).map_err(|_| JsonError::MalformedJson)?;
    strict_value(raw.get(), 0, JsonMode::Protocol)
}

/// Original Python JSON finite-number semantics for the Explorer HTTP boundary:
/// last-key-wins objects, arbitrary-precision integers and binary64 floats.
/// This does not alter strict source parsing or exact-token protocol profiles.
pub fn python_finite_json(text: &str, max_bytes: usize) -> Result<Value, JsonError> {
    if text.len() > max_bytes {
        return Err(JsonError::MalformedJson);
    }
    let raw: Box<RawValue> = serde_json::from_str(text).map_err(|_| JsonError::MalformedJson)?;
    strict_value(raw.get(), 0, JsonMode::PythonFinite)
}

/// Supported generated JSON leaves and their existing Vec/Option containers.
/// Conversion constructs actual values and never deserializes Value again.
pub trait PreservedJsonField: Sized {
    fn from_preserved_json(value: Value) -> Result<Self, JsonError>;
}
impl PreservedJsonField for Value {
    fn from_preserved_json(value: Value) -> Result<Self, JsonError> {
        Ok(value)
    }
}
impl PreservedJsonField for Number {
    fn from_preserved_json(value: Value) -> Result<Self, JsonError> {
        match value {
            Value::Number(number) => Ok(number),
            _ => Err(JsonError::MalformedJson),
        }
    }
}
impl<T: PreservedJsonField> PreservedJsonField for std::collections::BTreeMap<String, T> {
    fn from_preserved_json(value: Value) -> Result<Self, JsonError> {
        let Value::Object(values) = value else {
            return Err(JsonError::MalformedJson);
        };
        values
            .into_iter()
            .map(|(key, value)| T::from_preserved_json(value).map(|value| (key, value)))
            .collect()
    }
}
/// Original dictionaries keep input member order. A model's generated strong
/// view uses this map without turning JSON values into serde transport markers.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedObject<K, T> {
    entries: Vec<(K, T)>,
}
impl<T> OrderedObject<String, T> {
    pub fn get(&self, key: &str) -> Option<&T> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
impl<T: serde::Serialize> serde::Serialize for OrderedObject<String, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
impl<T: PreservedJsonField> PreservedJsonField for OrderedObject<String, T> {
    fn from_preserved_json(value: Value) -> Result<Self, JsonError> {
        let Value::Object(values) = value else {
            return Err(JsonError::MalformedJson);
        };
        Ok(Self {
            entries: values
                .into_iter()
                .map(|(k, v)| T::from_preserved_json(v).map(|v| (k, v)))
                .collect::<Result<_, _>>()?,
        })
    }
}
impl<'de, T: PreservedJsonField> Deserialize<'de> for OrderedObject<String, T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_preserved(deserializer)
    }
}
impl<T: PreservedJsonField> PreservedJsonField for Vec<T> {
    fn from_preserved_json(value: Value) -> Result<Self, JsonError> {
        let Value::Array(values) = value else {
            return Err(JsonError::MalformedJson);
        };
        values.into_iter().map(T::from_preserved_json).collect()
    }
}
impl<T: PreservedJsonField> PreservedJsonField for Option<T> {
    fn from_preserved_json(value: Value) -> Result<Self, JsonError> {
        if value.is_null() {
            Ok(None)
        } else {
            T::from_preserved_json(value).map(Some)
        }
    }
}

/// Generated serde field hook. RawValue preserves the raw JSON fragment for
/// from_str/from_slice/from_reader; the pinned Value deserializer supplies the
/// actual serialized Value for from_value. Real object keys cannot be mistaken
/// for serde's private arbitrary-number or raw-value transport markers.
pub fn deserialize_preserved<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: PreservedJsonField,
{
    let raw = Box::<RawValue>::deserialize(deserializer)?;
    let value = strict_value(raw.get(), 0, JsonMode::DtoIdentity).map_err(de::Error::custom)?;
    T::from_preserved_json(value).map_err(de::Error::custom)
}

/// Match Python's `json.dumps(..., ensure_ascii=False, sort_keys=True,
/// separators=(",", ":"), allow_nan=False)` for JSON-compatible values.
/// This is a snapshot identity format, not RFC 8785 or a model-wire serializer.
pub fn canonical_json(value: &Value) -> Result<Vec<u8>, JsonError> {
    let mut output = Vec::new();
    emit(value, &mut output, 0, true)?;
    Ok(output)
}

/// Python-compatible compact JSON preserving map insertion order. Model wire
/// payloads and their existing order-sensitive request hashes use this format.
/// It shares scalar/numeric serialization with `canonical_json` without sorting
/// question IDs, options or structured instructions.
pub fn ordered_json(value: &Value) -> Result<Vec<u8>, JsonError> {
    let mut output = Vec::new();
    emit(value, &mut output, 0, false)?;
    Ok(output)
}

/// Pinned Python `str` for the finite JSON value domain. This is the single
/// representation owner used by model text validators and repair diagnostics.
/// Native Python instances and nonfinite floating values are outside this API.
pub fn python_str(value: &Value) -> Result<String, JsonError> {
    python_representation(value, false, 0)
}

/// Pinned Python `repr` for finite JSON values, preserving member order.
pub fn python_repr(value: &Value) -> Result<String, JsonError> {
    python_representation(value, true, 0)
}

/// Python 3.13.11's Unicode whitespace semantics, including U+001C..U+001F.
pub fn python_strip(text: &str) -> &str {
    text.trim_matches(python_unicode::is_whitespace)
}

/// Python equality for finite JSON values. Numeric equality includes bool/int
/// and float/int, preserving arbitrary-size integer precision. This comparison
/// does not change retained JSON tokens or the content-hash serialization.
pub fn python_equal(left: &Value, right: &Value) -> Result<bool, JsonError> {
    enum Numeric {
        Integer(String),
        Float(f64),
    }
    fn integer(text: &str) -> Result<String, JsonError> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(digits) => (true, digits),
            None => (false, text),
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(JsonError::MalformedJson);
        }
        let digits = digits.trim_start_matches('0');
        if digits.is_empty() {
            return Ok("0".into());
        }
        Ok(if negative {
            format!("-{digits}")
        } else {
            digits.into()
        })
    }
    fn numeric(value: &Value) -> Result<Option<Numeric>, JsonError> {
        Ok(match value {
            Value::Bool(value) => Some(Numeric::Integer(if *value { "1" } else { "0" }.into())),
            Value::Number(value) => {
                let text = value.to_string();
                if text.contains(['.', 'e', 'E']) {
                    let value = value
                        .as_f64()
                        .filter(|n| n.is_finite())
                        .ok_or(JsonError::NonfiniteJsonNumber)?;
                    Some(Numeric::Float(value))
                } else {
                    Some(Numeric::Integer(integer(&text)?))
                }
            }
            _ => None,
        })
    }
    fn numeric_equal(left: Numeric, right: Numeric) -> bool {
        match (left, right) {
            (Numeric::Integer(a), Numeric::Integer(b)) => a == b,
            (Numeric::Float(a), Numeric::Float(b)) => a == b,
            (Numeric::Integer(integer), Numeric::Float(float))
            | (Numeric::Float(float), Numeric::Integer(integer)) => {
                float.fract() == 0.0
                    && integer
                        == if float == 0.0 {
                            "0".into()
                        } else {
                            format!("{float:.0}")
                        }
            }
        }
    }
    fn equal(left: &Value, right: &Value, depth: usize) -> Result<bool, JsonError> {
        if depth >= MAX_JSON_DEPTH {
            return Err(JsonError::NestingLimitExceeded);
        }
        if matches!(left, Value::Bool(_) | Value::Number(_))
            || matches!(right, Value::Bool(_) | Value::Number(_))
        {
            return Ok(match (numeric(left)?, numeric(right)?) {
                (Some(left), Some(right)) => numeric_equal(left, right),
                _ => false,
            });
        }
        Ok(match (left, right) {
            (Value::Null, Value::Null) => true,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.iter().zip(b) {
                    if !equal(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Object(a), Value::Object(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (key, a) in a {
                    let Some(b) = b.get(key) else {
                        return Ok(false);
                    };
                    if !equal(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => false,
        })
    }
    equal(left, right, 0)
}

#[doc(hidden)]
pub fn python_character_class(c: char) -> (bool, bool) {
    (
        python_unicode::is_printable(c),
        python_unicode::is_whitespace(c),
    )
}

/// Unicode decimal digit from the single pinned Python character table.
/// This is a lexical primitive; callers own their integer/decimal grammar.
pub fn python_decimal_digit(c: char) -> Option<u8> {
    python_unicode::decimal(c)
}

/// Pinned Unicode 15.1 categories used by RDFLib namespace splitting.
/// (split-start, name-character), before the caller's explicit punctuation.
/// Rust's host Unicode version does not change the source contract.
pub fn python_xml_name_class(c: char) -> (bool, bool) {
    use std::sync::OnceLock;
    static RANGES: OnceLock<Vec<(u32, u32, u8)>> = OnceLock::new();
    let ranges = RANGES.get_or_init(|| {
        #[derive(serde::Deserialize)]
        struct Data { ranges: Vec<(u32, u32, u8)> }
        serde_json::from_str::<Data>(include_str!("data/python_xml_name_unicode151.json"))
            .expect("pinned Unicode namespace categories").ranges
    });
    let cp = c as u32;
    let i = ranges.partition_point(|(_, end, _)| *end < cp);
    let mask = ranges.get(i).filter(|(start, _, _)| *start <= cp).map_or(0, |v| v.2);
    (mask & 1 != 0, mask & 2 != 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonFloatError {
    InvalidInput,
    IntegerOverflow,
}

/// Original built-in `float` conversion for JSON values. Model validators that
/// explicitly call Python float differ from Pydantic's ordinary scalar parser:
/// Unicode decimal digits are accepted, while oversized native integers raise.
pub fn python_builtin_float(value: &Value) -> Result<f64, PythonFloatError> {
    match value {
        Value::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        Value::Number(n) => {
            let text = n.to_string();
            let number = n.as_f64().ok_or(PythonFloatError::IntegerOverflow)?;
            if !text.contains(['.', 'e', 'E']) && !number.is_finite() {
                Err(PythonFloatError::IntegerOverflow)
            } else {
                Ok(number)
            }
        }
        Value::String(text) => {
            let text = text
                .chars()
                .map(|c| {
                    if let Some(digit) = python_unicode::decimal(c) {
                        (b'0' + digit) as char
                    } else if !c.is_ascii() && python_unicode::is_whitespace(c) {
                        ' '
                    } else {
                        c
                    }
                })
                .collect::<String>();
            let text = text.trim_matches(|c| matches!(c, '\u{9}'..='\u{d}' | ' '));
            let bytes = text.as_bytes();
            for (index, c) in bytes.iter().enumerate() {
                if *c == b'_'
                    && (index == 0
                        || index + 1 == bytes.len()
                        || !bytes[index - 1].is_ascii_digit()
                        || !bytes[index + 1].is_ascii_digit())
                {
                    return Err(PythonFloatError::InvalidInput);
                }
            }
            text.replace('_', "")
                .parse()
                .map_err(|_| PythonFloatError::InvalidInput)
        }
        _ => Err(PythonFloatError::InvalidInput),
    }
}

// One quote/Unicode-escape owner for both String and native codepoint text.
pub(crate) fn python_quote_points<I, E, F>(text: I, mut emit: F) -> Result<(), E>
where
    I: Iterator<Item = u32> + Clone,
    F: FnMut(u32) -> Result<(), E>,
{
    let mark = if text.clone().any(|cp| cp == 39) && !text.clone().any(|cp| cp == 34) {
        34
    } else {
        39
    };
    emit(mark)?;
    for cp in text {
        let escaped = match cp {
            10 => Some("\\n".to_owned()),
            13 => Some("\\r".to_owned()),
            9 => Some("\\t".to_owned()),
            92 => Some("\\\\".to_owned()),
            cp if cp == mark => Some(format!("\\{}", char::from_u32(cp).unwrap())),
            cp if !char::from_u32(cp).is_some_and(python_unicode::is_printable) => {
                Some(if cp <= 0xff {
                    format!("\\x{cp:02x}")
                } else if cp <= 0xffff {
                    format!("\\u{cp:04x}")
                } else {
                    format!("\\U{cp:08x}")
                })
            }
            _ => None,
        };
        if let Some(s) = escaped {
            for c in s.chars() {
                emit(c as u32)?;
            }
        } else {
            emit(cp)?;
        }
    }
    emit(mark)
}
fn python_quote(text: &str) -> String {
    let mut out = String::new();
    let result: Result<(), std::convert::Infallible> =
        python_quote_points(text.chars().map(u32::from), |cp| {
            out.push(char::from_u32(cp).expect("quoted scalar or ASCII escape"));
            Ok(())
        });
    match result {
        Ok(()) => out,
        Err(e) => match e {},
    }
}

fn python_representation(value: &Value, quoted: bool, depth: usize) -> Result<String, JsonError> {
    if depth >= MAX_JSON_DEPTH {
        return Err(JsonError::NestingLimitExceeded);
    }
    Ok(match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::String(text) => {
            if quoted {
                python_quote(text)
            } else {
                text.clone()
            }
        }
        Value::Number(number) => {
            let text = number.to_string();
            if text.contains(['.', 'e', 'E']) {
                python_float(number)?
            } else if text == "-0" {
                "0".into()
            } else {
                text
            }
        }
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|value| python_representation(value, true, depth + 1))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        Value::Object(members) => format!(
            "{{{}}}",
            members
                .iter()
                .map(|(name, value)| Ok(format!(
                    "{}: {}",
                    python_quote(name),
                    python_representation(value, true, depth + 1)?
                )))
                .collect::<Result<Vec<_>, JsonError>>()?
                .join(", ")
        ),
    })
}

fn emit(
    value: &Value,
    output: &mut Vec<u8>,
    depth: usize,
    sort_keys: bool,
) -> Result<(), JsonError> {
    if depth >= MAX_JSON_DEPTH {
        return Err(JsonError::NestingLimitExceeded);
    }
    match value {
        Value::Object(map) => {
            output.push(b'{');
            let mut fields: Vec<_> = map.iter().collect();
            if sort_keys {
                fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
            }
            for (index, (key, value)) in fields.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                output.extend(serde_json::to_vec(key).map_err(|_| JsonError::MalformedJson)?);
                output.push(b':');
                emit(value, output, depth + 1, sort_keys)?;
            }
            output.push(b'}');
        }
        Value::Array(items) => {
            output.push(b'[');
            for (index, value) in items.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                emit(value, output, depth + 1, sort_keys)?;
            }
            output.push(b']');
        }
        Value::Number(number) => {
            let rendered = number.to_string();
            if rendered.contains(['.', 'e', 'E']) {
                output.extend(python_float(number)?.bytes());
            } else if rendered == "-0" {
                output.push(b'0');
            } else {
                output.extend(rendered.bytes());
            }
        }
        _ => output.extend(serde_json::to_vec(value).map_err(|_| JsonError::MalformedJson)?),
    }
    Ok(())
}

fn python_float(number: &Number) -> Result<String, JsonError> {
    let value = number
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or(JsonError::NonfiniteJsonNumber)?;
    python_float_repr(value)
}

/// Python float text for the JSON, native-value and numeric-literal adapters.
/// Special values use Python str/repr spelling, independently of JSON admission.
pub fn python_float_repr(value: f64) -> Result<String, JsonError> {
    if value.is_nan() {
        return Ok("nan".into());
    }
    if value.is_infinite() {
        return Ok(if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into());
    }
    // Retain the oe-semantic formatter already checked against 25,000 finite
    // IEEE-754 samples with a Python oracle. That historical scope is not a
    // qualification of new mixed arithmetic; its formatting policy is unchanged.
    if value == 0.0 {
        return Ok(if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into());
    }
    let rendered = Number::from_f64(value)
        .ok_or(JsonError::NonfiniteJsonNumber)?
        .to_string();
    let (sign, magnitude) = rendered
        .strip_prefix('-')
        .map_or(("", rendered.as_str()), |number| ("-", number));
    let (mantissa, exponent) = if let Some((mantissa, exponent)) = magnitude.split_once(['e', 'E'])
    {
        (
            mantissa,
            exponent
                .parse::<i32>()
                .map_err(|_| JsonError::MalformedJson)?,
        )
    } else {
        (magnitude, 0)
    };
    let dot = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let all: String = mantissa
        .chars()
        .filter(|character| *character != '.')
        .collect();
    let leading = all.bytes().take_while(|byte| *byte == b'0').count();
    let digits = all[leading..].trim_end_matches('0');
    let scientific_exponent = dot - leading as i32 - 1 + exponent;
    if !(-4..16).contains(&scientific_exponent) {
        let tail = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        Ok(format!(
            "{sign}{}{tail}e{:+03}",
            &digits[..1],
            scientific_exponent
        ))
    } else {
        let point = scientific_exponent + 1;
        if point <= 0 {
            Ok(format!("{sign}0.{}{digits}", "0".repeat((-point) as usize)))
        } else if point as usize >= digits.len() {
            Ok(format!(
                "{sign}{digits}{}.0",
                "0".repeat(point as usize - digits.len())
            ))
        } else {
            Ok(format!(
                "{sign}{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            ))
        }
    }
}

pub mod native_value;

pub mod original_json_bytes;

pub mod original_json;

pub mod native_operations;

#[cfg(test)]
mod float_repr_tests {
    use super::*;

    #[test]
    fn shared_float_repr_keeps_python_boundaries_and_json_adapter() {
        // Source policy: fixed notation in [-4,16), signed exponent with two
        // digits, integral floats with .0, and a retained sign for zero.
        for (value, expected) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (f64::MAX, "1.7976931348623157e+308"),
            (f64::from_bits(1), "5e-324"),
        ] {
            assert_eq!(python_float_repr(value).unwrap(), expected);
            let number = Number::from_f64(value).unwrap();
            assert_eq!(python_float(&number).unwrap(), expected);
        }
    }

    #[test]
    fn shared_float_repr_specials_do_not_admit_nonfinite_json() {
        for (value, expected) in [
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::from_bits(0xfff8_0000_0000_0001), "nan"),
        ] {
            assert_eq!(python_float_repr(value).unwrap(), expected);
        }
        assert_eq!(
            python_finite_json("1e400", 100).unwrap_err(),
            JsonError::NonfiniteJsonNumber
        );
    }

    #[test]
    fn shared_float_repr_native_text_preserves_specials_and_output_budget() {
        use crate::native_operations::{OperationError, TextLimits, python_text};
        use crate::native_value::{NativeLimits, NativeNode, NativeValue};
        for (bits, expected) in [
            (0x8000_0000_0000_0000, "-0.0"),
            (0xfff0_0000_0000_0000, "-inf"),
            (0xfff8_0000_0000_0001, "nan"),
        ] {
            let value = NativeValue::new(
                vec![NativeNode::FloatBits(bits)],
                0,
                NativeLimits {
                    max_nodes: 1,
                    max_links: 0,
                    max_scalar_bytes: 8,
                    max_depth: 0,
                    max_wire_bytes: 256,
                },
            )
            .unwrap();
            let limits = TextLimits {
                max_codepoints: expected.len(),
                max_visits: 1,
                max_depth: 0,
            };
            assert_eq!(
                python_text(&value, 0, false, limits).unwrap(),
                expected.chars().map(u32::from).collect::<Vec<_>>()
            );
            assert_eq!(
                python_text(
                    &value,
                    0,
                    false,
                    TextLimits {
                        max_codepoints: expected.len() - 1,
                        ..limits
                    },
                )
                .unwrap_err(),
                OperationError::TextLimit
            );
        }
    }
}
