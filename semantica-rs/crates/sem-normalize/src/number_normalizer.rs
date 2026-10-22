//! Number and Quantity Normalization Module (`semantica/normalize/number_normalizer.py`).
//!
//! Number parsing (separators, percentages, scientific notation, K/M/B/T suffixes), unit
//! normalization and conversion, currency parsing and scientific-notation helpers.

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::{PyNum, float_repr, type_name};
use sem_utils::{SemanticaError, ValidationError, get_logger, get_progress_tracker};
use serde_json::{Map, Value, json};

use crate::pystr::{self, PY_SPACE};

/// A Python `int | float` result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberValue {
    /// An `int` (integers beyond `i128` are returned as floats; see PORT-NOTES.md).
    Int(i128),
    /// A `float`.
    Float(f64),
}

impl NumberValue {
    /// `float(value)`.
    pub fn as_f64(self) -> f64 {
        match self {
            NumberValue::Int(i) => i as f64,
            NumberValue::Float(f) => f,
        }
    }

    /// The value as JSON (`int` stays an integer when it fits `i64`/`u64`).
    pub fn to_value(self) -> Value {
        match self {
            NumberValue::Int(i) => {
                if let Ok(v) = i64::try_from(i) {
                    json!(v)
                } else if let Ok(v) = u64::try_from(i) {
                    json!(v)
                } else {
                    json!(i as f64)
                }
            }
            NumberValue::Float(f) => serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        }
    }
}

impl PartialEq<f64> for NumberValue {
    fn eq(&self, other: &f64) -> bool {
        match *self {
            NumberValue::Int(i) => {
                PyNum::Int(i).compare(PyNum::Float(*other)) == Some(std::cmp::Ordering::Equal)
            }
            NumberValue::Float(f) => f == *other,
        }
    }
}

impl fmt::Display for NumberValue {
    /// `str(value)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumberValue::Int(i) => write!(f, "{i}"),
            NumberValue::Float(x) => f.write_str(&float_repr(*x)),
        }
    }
}

impl From<NumberValue> for Value {
    fn from(value: NumberValue) -> Value {
        value.to_value()
    }
}

/// The input of `normalize_number`: a string, an `int`, a `float`, or any other value
/// (rejected with Python's `ValidationError`).
#[derive(Debug, Clone, PartialEq)]
pub enum NumberInput {
    /// `str`.
    Str(String),
    /// `int` (`bool` is an `int` in Python).
    Int(i128),
    /// `float`.
    Float(f64),
    /// Anything else.
    Other(Value),
}

impl From<&str> for NumberInput {
    fn from(value: &str) -> Self {
        NumberInput::Str(value.to_string())
    }
}

impl From<String> for NumberInput {
    fn from(value: String) -> Self {
        NumberInput::Str(value)
    }
}

impl From<i64> for NumberInput {
    fn from(value: i64) -> Self {
        NumberInput::Int(i128::from(value))
    }
}

impl From<i32> for NumberInput {
    fn from(value: i32) -> Self {
        NumberInput::Int(i128::from(value))
    }
}

impl From<f64> for NumberInput {
    fn from(value: f64) -> Self {
        NumberInput::Float(value)
    }
}

impl From<&Value> for NumberInput {
    fn from(value: &Value) -> Self {
        match value {
            Value::String(s) => NumberInput::Str(s.clone()),
            other => match PyNum::of(other) {
                Some(PyNum::Int(i)) => NumberInput::Int(i),
                Some(PyNum::Float(f)) => NumberInput::Float(f),
                None => NumberInput::Other(other.clone()),
            },
        }
    }
}

impl From<Value> for NumberInput {
    fn from(value: Value) -> Self {
        NumberInput::from(&value)
    }
}

static QUANTITY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"([\d.,{PY_SPACE}]+)[{PY_SPACE}]*([a-zA-Z]+)")).expect("valid regex")
});
static NON_AMOUNT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\d.,]").expect("valid regex"));

/// Number and quantity normalization coordinator (`NumberNormalizer(config=None, **kwargs)`).
pub struct NumberNormalizer {
    /// `config` (with the keyword arguments merged in).
    pub config: Map<String, Value>,
    /// `unit_converter`.
    pub unit_converter: UnitConverter,
    /// `currency_normalizer`.
    pub currency_normalizer: CurrencyNormalizer,
    /// `scientific_handler`.
    pub scientific_handler: ScientificNotationHandler,
    /// `progress_tracker` (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for NumberNormalizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NumberNormalizer")
            .field("config", &self.config)
            .finish()
    }
}

impl Default for NumberNormalizer {
    fn default() -> Self {
        NumberNormalizer::new(Map::new())
    }
}

impl NumberNormalizer {
    /// `NumberNormalizer(config)`.
    pub fn new(config: Map<String, Value>) -> NumberNormalizer {
        let logger = get_logger("number_normalizer");
        let unit_converter = UnitConverter::new(config.clone());
        let currency_normalizer = CurrencyNormalizer::new(config.clone());
        let scientific_handler = ScientificNotationHandler::new(config.clone());
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug("Number normalizer initialized");
        NumberNormalizer {
            config,
            unit_converter,
            currency_normalizer,
            scientific_handler,
            progress_tracker,
            logger,
        }
    }

    /// `normalize_number(number_input, **options)`: numbers are returned unchanged; strings
    /// lose `,`, spaces and currency symbols, then percentages (`/ 100`), scientific notation
    /// (any `e`), K/M/B/T suffixes and plain `float`/`int` literals are parsed.
    pub fn normalize_number(
        &self,
        number_input: impl Into<NumberInput>,
    ) -> Result<NumberValue, SemanticaError> {
        let text = match number_input.into() {
            NumberInput::Int(i) => return Ok(NumberValue::Int(i)),
            NumberInput::Float(f) => return Ok(NumberValue::Float(f)),
            NumberInput::Other(value) => {
                return Err(ValidationError::new(format!(
                    "Unsupported number input type: <class '{}'>",
                    type_name(&value)
                ))
                .into());
            }
            NumberInput::Str(s) => s,
        };
        let mut cleaned = pystr::strip(&text.replace([',', ' '], "")).to_string();
        for symbol in self.currency_normalizer.currency_symbols.keys() {
            if cleaned.contains(symbol.as_str()) {
                cleaned = cleaned.replace(symbol.as_str(), "");
            }
        }
        if cleaned.contains('%') {
            cleaned = cleaned.replace('%', "");
            let value = pystr::py_float(&cleaned)? / 100.0;
            return Ok(NumberValue::Float(value));
        }
        if pystr::lower(&cleaned).contains('e') || cleaned.contains('E') {
            let parsed = self
                .scientific_handler
                .parse_scientific_notation(&cleaned)?;
            return Ok(NumberValue::Float(parsed));
        }
        if let Some(last) = cleaned.chars().last() {
            let suffix = pystr::lower(&last.to_string());
            let multiplier = match suffix.as_str() {
                "k" => Some(1_000.0),
                "m" => Some(1_000_000.0),
                "b" => Some(1_000_000_000.0),
                "t" => Some(1_000_000_000_000.0),
                _ => None,
            };
            if let Some(multiplier) = multiplier {
                let number_part: String = {
                    let mut chars: Vec<char> = cleaned.chars().collect();
                    chars.pop();
                    chars.into_iter().collect()
                };
                if let Ok(value) = pystr::py_float(&number_part) {
                    return Ok(NumberValue::Float(value * multiplier));
                }
            }
        }
        let unparsable = || -> SemanticaError {
            ValidationError::new(format!("Unable to parse number: {text}")).into()
        };
        if cleaned.contains('.') {
            pystr::py_float(&cleaned)
                .map(NumberValue::Float)
                .map_err(|_| unparsable())
        } else {
            match pystr::parse_int(&cleaned) {
                pystr::IntParse::Value(v) => Ok(NumberValue::Int(v)),
                pystr::IntParse::Overflow => pystr::py_float(&cleaned)
                    .map(NumberValue::Float)
                    .map_err(|_| unparsable()),
                pystr::IntParse::Invalid => Err(unparsable()),
            }
        }
    }

    /// `normalize_quantity(quantity_input, **options)`: `{"value": float, "unit": normalized
    /// unit, "original": input}` from the first `<number> <letters>` in the text.
    pub fn normalize_quantity(
        &self,
        quantity_input: &str,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let Some(captures) = QUANTITY_RE.captures(quantity_input) else {
            return Err(
                ValidationError::new(format!("Invalid quantity format: {quantity_input}")).into(),
            );
        };
        let value_str = captures[1].replace([',', ' '], "");
        let unit = pystr::lower(&captures[2]);
        let value = pystr::py_float(&value_str).map_err(|_| -> SemanticaError {
            ValidationError::new(format!("Unable to parse quantity: {quantity_input}")).into()
        })?;
        let normalized_unit = self.unit_converter.normalize_unit(&unit);
        let mut result = Map::new();
        result.insert("value".to_string(), float_value(value));
        result.insert("unit".to_string(), json!(normalized_unit));
        result.insert("original".to_string(), json!(quantity_input));
        Ok(result)
    }

    /// `convert_units(value, from_unit, to_unit)`.
    pub fn convert_units(
        &self,
        value: f64,
        from_unit: &str,
        to_unit: &str,
    ) -> Result<f64, SemanticaError> {
        self.unit_converter.convert_units(value, from_unit, to_unit)
    }

    /// `process_currency(currency_input, default_currency="USD", **options)`.
    pub fn process_currency(
        &self,
        currency_input: &str,
        default_currency: Option<&str>,
    ) -> Map<String, Value> {
        self.currency_normalizer
            .normalize_currency(currency_input, default_currency)
    }

    /// The logger (`semantica.number_normalizer`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// A float as JSON (`NaN`/`inf` have no JSON form and become `null`).
fn float_value(f: f64) -> Value {
    serde_json::Number::from_f64(f)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

const CONVERSION_FACTORS: &[(&str, f64)] = &[
    ("meter", 1.0),
    ("meters", 1.0),
    ("m", 1.0),
    ("kilometer", 1000.0),
    ("kilometers", 1000.0),
    ("km", 1000.0),
    ("centimeter", 0.01),
    ("centimeters", 0.01),
    ("cm", 0.01),
    ("millimeter", 0.001),
    ("millimeters", 0.001),
    ("mm", 0.001),
    ("inch", 0.0254),
    ("inches", 0.0254),
    ("in", 0.0254),
    ("foot", 0.3048),
    ("feet", 0.3048),
    ("ft", 0.3048),
    ("yard", 0.9144),
    ("yards", 0.9144),
    ("yd", 0.9144),
    ("mile", 1609.34),
    ("miles", 1609.34),
    ("mi", 1609.34),
    ("kilogram", 1.0),
    ("kilograms", 1.0),
    ("kg", 1.0),
    ("gram", 0.001),
    ("grams", 0.001),
    ("g", 0.001),
    ("pound", 0.453592),
    ("pounds", 0.453592),
    ("lb", 0.453592),
    ("ounce", 0.0283495),
    ("ounces", 0.0283495),
    ("oz", 0.0283495),
    ("liter", 1.0),
    ("liters", 1.0),
    ("l", 1.0),
    ("milliliter", 0.001),
    ("milliliters", 0.001),
    ("ml", 0.001),
    ("gallon", 3.78541),
    ("gallons", 3.78541),
    ("gal", 3.78541),
];

const UNIT_MAP: [(&str, &str); 32] = [
    ("m", "meter"),
    ("meter", "meter"),
    ("meters", "meter"),
    ("km", "kilometer"),
    ("kilometer", "kilometer"),
    ("kilometers", "kilometer"),
    ("cm", "centimeter"),
    ("centimeter", "centimeter"),
    ("centimeters", "centimeter"),
    ("mm", "millimeter"),
    ("millimeter", "millimeter"),
    ("millimeters", "millimeter"),
    ("kg", "kilogram"),
    ("kilogram", "kilogram"),
    ("kilograms", "kilogram"),
    ("kgs", "kilogram"),
    ("g", "gram"),
    ("gram", "gram"),
    ("grams", "gram"),
    ("lb", "pound"),
    ("pound", "pound"),
    ("pounds", "pound"),
    ("lbs", "pound"),
    ("oz", "ounce"),
    ("ounce", "ounce"),
    ("ounces", "ounce"),
    ("l", "liter"),
    ("liter", "liter"),
    ("liters", "liter"),
    ("ml", "milliliter"),
    ("milliliter", "milliliter"),
    ("milliliters", "milliliter"),
];

/// Unit conversion engine (`UnitConverter(**config)`).
#[derive(Debug, Clone)]
pub struct UnitConverter {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    /// `conversion_factors`: unit -> factor to the category's base unit.
    pub conversion_factors: IndexMap<String, f64>,
    /// `unit_categories`: `length`, `weight`, `volume` -> canonical unit names.
    pub unit_categories: IndexMap<String, Vec<String>>,
    logger: Logger,
}

impl Default for UnitConverter {
    fn default() -> Self {
        UnitConverter::new(Map::new())
    }
}

impl UnitConverter {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `UnitConverter(**config)`.
    pub fn new(config: Map<String, Value>) -> UnitConverter {
        let logger = get_logger("unit_converter");
        let conversion_factors = CONVERSION_FACTORS
            .iter()
            .map(|(name, factor)| (name.to_string(), *factor))
            .collect();
        let unit_categories = [
            (
                "length",
                vec![
                    "meter",
                    "kilometer",
                    "centimeter",
                    "millimeter",
                    "inch",
                    "foot",
                    "yard",
                    "mile",
                ],
            ),
            ("weight", vec!["kilogram", "gram", "pound", "ounce"]),
            ("volume", vec!["liter", "milliliter", "gallon"]),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.into_iter().map(str::to_string).collect()))
        .collect();
        logger.debug("Unit converter initialized");
        UnitConverter {
            config,
            conversion_factors,
            unit_categories,
            logger,
        }
    }

    /// `convert_units(value, from_unit, to_unit)`: `value * from_factor / to_factor`, or
    /// `ValidationError("Cannot convert from X to Y")` when the units are incompatible.
    pub fn convert_units(
        &self,
        value: f64,
        from_unit: &str,
        to_unit: &str,
    ) -> Result<f64, SemanticaError> {
        let from_unit = pystr::lower(from_unit);
        let to_unit = pystr::lower(to_unit);
        if !self.validate_units(&from_unit, &to_unit) {
            return Err(ValidationError::new(format!(
                "Cannot convert from {from_unit} to {to_unit}"
            ))
            .into());
        }
        let from_factor = self.get_conversion_factor(&from_unit, "base");
        let to_factor = self.get_conversion_factor(&to_unit, "base");
        let base_value = value * from_factor;
        Ok(base_value / to_factor)
    }

    /// `validate_units(from_unit, to_unit)`: both units known and in the same category (an
    /// abbreviation belongs to no category, so two abbreviations always "match").
    pub fn validate_units(&self, from_unit: &str, to_unit: &str) -> bool {
        let from_unit = pystr::lower(from_unit);
        let to_unit = pystr::lower(to_unit);
        if !self.conversion_factors.contains_key(&from_unit)
            || !self.conversion_factors.contains_key(&to_unit)
        {
            return false;
        }
        let mut from_category = None;
        let mut to_category = None;
        for (category, units) in &self.unit_categories {
            if units.contains(&from_unit) {
                from_category = Some(category);
            }
            if units.contains(&to_unit) {
                to_category = Some(category);
            }
        }
        from_category == to_category
    }

    /// `get_conversion_factor(from_unit, to_unit)`: the factor to the base unit (`to_unit ==
    /// "base"`) or between two units; unknown units count as 1.0.
    pub fn get_conversion_factor(&self, from_unit: &str, to_unit: &str) -> f64 {
        let from_unit = pystr::lower(from_unit);
        if to_unit == "base" {
            return self
                .conversion_factors
                .get(&from_unit)
                .copied()
                .unwrap_or(1.0);
        }
        let to_unit = pystr::lower(to_unit);
        let from_factor = self
            .conversion_factors
            .get(&from_unit)
            .copied()
            .unwrap_or(1.0);
        let to_factor = self
            .conversion_factors
            .get(&to_unit)
            .copied()
            .unwrap_or(1.0);
        from_factor / to_factor
    }

    /// `normalize_unit(unit)`: full unit name for a known abbreviation or plural, else the
    /// lowercased unit.
    pub fn normalize_unit(&self, unit: &str) -> String {
        let unit_lower = pystr::lower(unit);
        UNIT_MAP
            .iter()
            .find(|(k, _)| *k == unit_lower)
            .map(|(_, v)| v.to_string())
            .unwrap_or(unit_lower)
    }
}

/// Currency normalization engine (`CurrencyNormalizer(**config)`).
#[derive(Debug, Clone)]
pub struct CurrencyNormalizer {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    /// `currency_symbols`: symbol -> ISO code.
    pub currency_symbols: IndexMap<String, String>,
    /// `currency_codes`: supported ISO codes.
    pub currency_codes: Vec<String>,
    logger: Logger,
}

impl Default for CurrencyNormalizer {
    fn default() -> Self {
        CurrencyNormalizer::new(Map::new())
    }
}

impl CurrencyNormalizer {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `CurrencyNormalizer(**config)`.
    pub fn new(config: Map<String, Value>) -> CurrencyNormalizer {
        let logger = get_logger("currency_normalizer");
        let currency_symbols = [
            ("$", "USD"),
            ("\u{20ac}", "EUR"),
            ("\u{a3}", "GBP"),
            ("\u{a5}", "JPY"),
            ("\u{20b9}", "INR"),
            ("\u{20bd}", "RUB"),
            ("\u{20a9}", "KRW"),
            ("\u{20aa}", "ILS"),
            ("\u{20a6}", "NGN"),
            ("\u{20a8}", "PKR"),
        ]
        .into_iter()
        .map(|(s, c)| (s.to_string(), c.to_string()))
        .collect();
        let currency_codes = [
            "USD", "EUR", "GBP", "JPY", "CNY", "INR", "AUD", "CAD", "CHF", "SEK", "NOK", "DKK",
            "RUB", "KRW", "ILS", "NGN", "PKR",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        logger.debug("Currency normalizer initialized");
        CurrencyNormalizer {
            config,
            currency_symbols,
            currency_codes,
            logger,
        }
    }

    /// `normalize_currency(currency_input, default_currency="USD", **options)`: the currency
    /// from a symbol or a standalone code (not part of a longer run of capitals), the amount
    /// from the rest; `{"amount": float | None, "currency", "original"}`.
    pub fn normalize_currency(
        &self,
        currency_input: &str,
        default_currency: Option<&str>,
    ) -> Map<String, Value> {
        let mut currency_code: Option<String> = None;
        let mut amount: Option<f64> = None;

        for (symbol, code) in &self.currency_symbols {
            if currency_input.contains(symbol.as_str()) {
                currency_code = Some(code.clone());
                let amount_str = currency_input.replace(symbol.as_str(), "");
                let amount_str = pystr::strip(&amount_str).replace([',', ' '], "");
                amount = pystr::py_float(&amount_str).ok();
                break;
            }
        }

        if currency_code.as_deref().is_none_or(str::is_empty) {
            let upper: Vec<char> = pystr::upper(currency_input).chars().collect();
            for code in &self.currency_codes {
                if let Some((start, end)) = find_standalone_code(&upper, code) {
                    currency_code = Some(code.clone());
                    let input_chars: Vec<char> = currency_input.chars().collect();
                    let before: String =
                        input_chars[..start.min(input_chars.len())].iter().collect();
                    let after: String = input_chars[end.min(input_chars.len())..].iter().collect();
                    let amount_str = format!("{before}{after}");
                    let amount_str = pystr::strip(&amount_str).replace([',', ' '], "");
                    amount = pystr::py_float(&amount_str).ok();
                    break;
                }
            }
        }

        if amount.is_none() {
            let amount_str = NON_AMOUNT_RE.replace_all(currency_input, "");
            let amount_str = amount_str.replace([',', ' '], "");
            amount = pystr::py_float(&amount_str).ok();
        }

        let currency = match currency_code {
            Some(code) if !code.is_empty() => code,
            _ => default_currency.unwrap_or("USD").to_string(),
        };
        let mut result = Map::new();
        result.insert(
            "amount".to_string(),
            amount.map(float_value).unwrap_or(Value::Null),
        );
        result.insert("currency".to_string(), json!(currency));
        result.insert("original".to_string(), json!(currency_input));
        result
    }

    /// `convert_currency(amount, from_currency, to_currency)`: a placeholder that logs a
    /// warning and returns the amount unchanged.
    pub fn convert_currency(&self, amount: f64, _from_currency: &str, _to_currency: &str) -> f64 {
        self.logger
            .warning("Currency conversion requires exchange rate API");
        amount
    }

    /// `validate_currency_code(currency_code)`.
    pub fn validate_currency_code(&self, currency_code: &str) -> bool {
        let upper = pystr::upper(currency_code);
        self.currency_codes.contains(&upper)
    }
}

/// `re.search(rf"(?<![A-Z]){re.escape(code)}(?![A-Z])", text)` over code points: the
/// `(start, end)` code-point span of the first standalone occurrence.
fn find_standalone_code(text: &[char], code: &str) -> Option<(usize, usize)> {
    let code: Vec<char> = code.chars().collect();
    if code.is_empty() || code.len() > text.len() {
        return None;
    }
    for start in 0..=text.len() - code.len() {
        let end = start + code.len();
        if text[start..end] != code[..] {
            continue;
        }
        let before_ok = start == 0 || !text[start - 1].is_ascii_uppercase();
        let after_ok = end == text.len() || !text[end].is_ascii_uppercase();
        if before_ok && after_ok {
            return Some((start, end));
        }
    }
    None
}

/// Scientific notation processing engine (`ScientificNotationHandler(**config)`).
#[derive(Debug, Clone)]
pub struct ScientificNotationHandler {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for ScientificNotationHandler {
    fn default() -> Self {
        ScientificNotationHandler::new(Map::new())
    }
}

impl ScientificNotationHandler {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `ScientificNotationHandler(**config)`.
    pub fn new(config: Map<String, Value>) -> ScientificNotationHandler {
        let logger = get_logger("scientific_notation_handler");
        logger.debug("Scientific notation handler initialized");
        ScientificNotationHandler { config, logger }
    }

    /// `parse_scientific_notation(number_string)`: `float(number_string)`, or
    /// `ValidationError("Invalid scientific notation: ...")`.
    pub fn parse_scientific_notation(&self, number_string: &str) -> Result<f64, SemanticaError> {
        pystr::py_float(number_string).map_err(|_| {
            ValidationError::new(format!("Invalid scientific notation: {number_string}")).into()
        })
    }

    /// `convert_to_scientific(number, precision=None)`: Python's `format(number, "e")` /
    /// `format(number, ".{precision}e")` (`1.500000e+03`).
    pub fn convert_to_scientific(&self, number: f64, precision: Option<usize>) -> String {
        format_exponent(number, precision.unwrap_or(6))
    }

    /// `normalize_precision(number, significant_digits)`: scale into `[1, 10)`, `round()` to
    /// `significant_digits - 1` decimals (round-half-even on the exact value), scale back.
    pub fn normalize_precision(&self, number: f64, significant_digits: i64) -> f64 {
        if number == 0.0 {
            return 0.0;
        }
        let mut magnitude = number.abs();
        let mut order: i32 = 0;
        while magnitude >= 10.0 {
            magnitude /= 10.0;
            order += 1;
        }
        while magnitude < 1.0 {
            magnitude *= 10.0;
            order -= 1;
        }
        let rounded = py_round(magnitude, significant_digits - 1);
        if order >= 0 {
            // `10 ** order` is an exact int, converted to the nearest float.
            let power: f64 = format!("1e{order}").parse().expect("valid float literal");
            rounded * power
        } else {
            // `10 ** -n` is a float in Python: compute it the way CPython's float `**` does.
            rounded * 10f64.powf(f64::from(order))
        }
    }
}

/// `format(x, ".{precision}e")`.
pub(crate) fn format_exponent(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let formatted = format!("{x:.precision$e}");
    let (mantissa, exponent) = formatted
        .split_once('e')
        .expect("LowerExp output has an exponent");
    let exponent: i32 = exponent.parse().expect("integer exponent");
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", exponent.abs())
}

/// Python's `round(x, ndigits)` for a float: correctly rounded (half-even on the exact binary
/// value) to `ndigits` decimal places (negative values round to tens, hundreds, ...).
pub(crate) fn py_round(x: f64, ndigits: i64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    if ndigits > 400 {
        return x;
    }
    if ndigits >= 0 {
        let digits = usize::try_from(ndigits).unwrap_or(400);
        let text = format!("{x:.digits$}");
        return text.parse().unwrap_or(x);
    }
    // Negative ndigits: round x / 10^k to an integer exactly, using the exact decimal
    // expansion of x.
    let k = usize::try_from(-ndigits).unwrap_or(400);
    let exact = format!("{:.0}", x.abs().trunc());
    let frac_nonzero = x.abs().fract() != 0.0;
    let negative = x < 0.0;
    if exact.len() <= k {
        // |x| < 10^k: the result is 0 or 10^k.
        let half = {
            let mut h = String::from("5");
            h.push_str(&"0".repeat(k - 1));
            h
        };
        let padded = format!("{exact:0>k$}");
        let cmp = padded.cmp(&half);
        let up = match cmp {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Equal => frac_nonzero,
            std::cmp::Ordering::Less => false,
        };
        let value = if up { 10f64.powi(k as i32) } else { 0.0 };
        return if negative { -value } else { value };
    }
    let (head, tail) = exact.split_at(exact.len() - k);
    let mut head_digits: Vec<u8> = head.bytes().map(|b| b - b'0').collect();
    let first_tail = tail.as_bytes()[0] - b'0';
    let rest_nonzero = tail[1..].bytes().any(|b| b != b'0') || frac_nonzero;
    let last_odd = head_digits.last().is_some_and(|d| d % 2 == 1);
    let round_up = first_tail > 5 || (first_tail == 5 && (rest_nonzero || last_odd));
    if round_up {
        let mut i = head_digits.len();
        loop {
            if i == 0 {
                head_digits.insert(0, 1);
                break;
            }
            i -= 1;
            if head_digits[i] == 9 {
                head_digits[i] = 0;
            } else {
                head_digits[i] += 1;
                break;
            }
        }
    }
    let mut text: String = head_digits.iter().map(|d| char::from(b'0' + d)).collect();
    text.push_str(&"0".repeat(k));
    let value: f64 = text.parse().unwrap_or(x);
    if negative { -value } else { value }
}
