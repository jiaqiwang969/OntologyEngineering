//! Exact finite decimal arithmetic with the original Python default context:
//! 28 significant digits and ROUND_HALF_EVEN after multiplication/addition.
//! No binary floating point is used for fee calculation or display.
use anyhow::{Context, Result, ensure};
use num_bigint::{BigInt, Sign};
use serde_json::Value;
const DIGITS: usize = 4096;
const EXPONENT: i32 = 4096;
const PRECISION: usize = 28;
#[derive(Clone, Debug)]
pub(super) struct Decimal {
    coefficient: BigInt,
    exponent: i32,
    negative_zero: bool,
}
fn pow10(power: usize) -> BigInt {
    BigInt::from(10u8).pow(u32::try_from(power).expect("bounded decimal power"))
}
fn magnitude(value: &BigInt) -> BigInt {
    if value.sign() == Sign::Minus {
        -value
    } else {
        value.clone()
    }
}
fn round_even(value: &BigInt, power: usize) -> BigInt {
    if power == 0 {
        return value.clone();
    }
    let divisor = pow10(power);
    let absolute = magnitude(value);
    let mut quotient = &absolute / &divisor;
    let remainder = &absolute % &divisor;
    let twice = remainder * 2u8;
    if twice > divisor || (twice == divisor && &quotient % 2u8 == BigInt::from(1u8)) {
        quotient += 1u8;
    }
    if value.sign() == Sign::Minus {
        -quotient
    } else {
        quotient
    }
}
pub(super) fn integer(value: &Value) -> Result<BigInt> {
    let n = value
        .as_number()
        .context("positive_integer_required")?
        .to_string();
    ensure!(
        !n.is_empty() && n.len() <= DIGITS && n.bytes().all(|b| b.is_ascii_digit()),
        "positive_integer_required"
    );
    let n = BigInt::parse_bytes(n.as_bytes(), 10).context("invalid_integer")?;
    ensure!(n > BigInt::from(0u8), "positive_integer_required");
    Ok(n)
}
impl Decimal {
    pub(super) fn zero() -> Self {
        Self {
            coefficient: BigInt::from(0u8),
            exponent: 0,
            negative_zero: false,
        }
    }
    pub(super) fn amount(value: &Value) -> Result<Option<Self>> {
        if value.is_null() {
            return Ok(None);
        }
        let raw = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => {
                let s = n.to_string();
                ensure!(
                    !s.contains(['.', 'e', 'E']),
                    "amounts_must_be_decimal_strings_or_integers"
                );
                s
            }
            _ => anyhow::bail!("amounts_must_be_decimal_strings_or_integers"),
        };
        ensure!(raw.len() <= DIGITS * 2 + 32, "decimal_input_too_large");
        // Decimal accepts surrounding whitespace and underscores. Restrict the
        // digit alphabet to ASCII; unsupported spellings fail explicitly.
        let raw = raw.trim().replace('_', "");
        let (negative, raw) = if let Some(s) = raw.strip_prefix('-') {
            (true, s)
        } else {
            (false, raw.strip_prefix('+').unwrap_or(&raw))
        };
        let parts: Vec<_> = raw.split(['e', 'E']).collect();
        ensure!(
            !parts.is_empty() && parts.len() <= 2,
            "invalid_decimal_amount"
        );
        let explicit = if parts.len() == 2 {
            let exp = parts[1];
            ensure!(
                !exp.is_empty() && exp.len() <= 8,
                "decimal_exponent_out_of_bounds"
            );
            let value = exp.parse::<i32>().context("invalid_decimal_exponent")?;
            ensure!(value.abs() <= EXPONENT, "decimal_exponent_out_of_bounds");
            value
        } else {
            0
        };
        let mantissa = parts[0];
        let pieces: Vec<_> = mantissa.split('.').collect();
        ensure!(pieces.len() <= 2, "invalid_decimal_amount");
        let digits = pieces.join("");
        ensure!(
            !digits.is_empty()
                && digits.len() <= DIGITS
                && digits.bytes().all(|b| b.is_ascii_digit()),
            "invalid_decimal_amount"
        );
        let fractional = pieces.get(1).map_or(0, |s| s.len()) as i32;
        let exponent = explicit - fractional;
        ensure!(exponent.abs() <= EXPONENT, "decimal_scale_out_of_bounds");
        let mut coefficient =
            BigInt::parse_bytes(digits.as_bytes(), 10).context("invalid_decimal_amount")?;
        if negative {
            coefficient = -coefficient;
        }
        Ok(Some(Self {
            negative_zero: negative && coefficient.sign() == Sign::NoSign,
            coefficient,
            exponent,
        }))
    }
    fn negative(&self) -> bool {
        self.coefficient.sign() == Sign::Minus
            || (self.coefficient.sign() == Sign::NoSign && self.negative_zero)
    }
    fn rounded(mut self) -> Self {
        let digits = magnitude(&self.coefficient).to_string().len();
        if digits > PRECISION {
            let count = digits - PRECISION;
            let negative = self.negative();
            self.coefficient = round_even(&self.coefficient, count);
            self.exponent += count as i32;
            if magnitude(&self.coefficient).to_string().len() > PRECISION {
                self.coefficient /= 10u8;
                self.exponent += 1;
            }
            self.negative_zero = negative && self.coefficient.sign() == Sign::NoSign;
        }
        self
    }
    pub(super) fn multiply(&self, factor: &BigInt) -> Self {
        Self {
            coefficient: &self.coefficient * factor,
            exponent: self.exponent,
            negative_zero: self.negative_zero,
        }
        .rounded()
    }
    pub(super) fn add(&self, other: &Self) -> Self {
        let exponent = self.exponent.min(other.exponent);
        let coefficient = &self.coefficient * pow10((self.exponent - exponent) as usize)
            + &other.coefficient * pow10((other.exponent - exponent) as usize);
        Self {
            negative_zero: coefficient.sign() == Sign::NoSign
                && self.negative()
                && other.negative(),
            coefficient,
            exponent,
        }
        .rounded()
    }
    pub(super) fn money(&self) -> String {
        let cents = if self.exponent >= -2 {
            &self.coefficient * pow10((self.exponent + 2) as usize)
        } else {
            round_even(&self.coefficient, (-self.exponent - 2) as usize)
        };
        let mut digits = magnitude(&cents).to_string();
        while digits.len() < 3 {
            digits.insert(0, '0');
        }
        digits.insert(digits.len() - 2, '.');
        if self.negative() {
            digits.insert(0, '-');
        }
        digits
    }
}
