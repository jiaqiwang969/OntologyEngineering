//! Exact IEEE 754 binary64 conversion, before any Decimal arithmetic rounding.
//! Derived from the integer-ratio method in the retained Python 3.13.11
//! Decimal.from_float; see PROVENANCE.md and LICENSE-PYTHON.txt.
use crate::{
    BigInt, BigUint, DecimalContext, DecimalSignal, DecimalSignals, DecimalValue, NumericFault,
    NumericLimits, NumericOutcome,
};
use num_bigint::Sign;
use num_traits::{One, ToPrimitive, Zero};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binary64BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binary64UnaryOp {
    Plus,
    Minus,
}

/// Python conversion/arithmetic failures, distinct from Decimal signals and
/// from the caller's explicit resource limits. No exception is inferred from
/// a formatted diagnostic by consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Binary64Fault {
    IntegerOverflow,
    SignalingNaNConversion,
    ZeroDivision,
    Core(NumericFault),
}

impl fmt::Display for Binary64Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IntegerOverflow => f.write_str("integer exceeds binary64 range"),
            Self::SignalingNaNConversion => f.write_str("cannot convert signaling Decimal NaN"),
            Self::ZeroDivision => f.write_str("binary64 division by zero"),
            Self::Core(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Binary64Fault {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}

impl From<NumericFault> for Binary64Fault {
    fn from(error: NumericFault) -> Self {
        Self::Core(error)
    }
}

/// Convert the actual integer cache with one nearest, ties-to-even rounding.
/// The Python integer-to-text limit and the output text budget do not apply:
/// no text is used as a conversion intermediary. Input coefficient admission
/// remains mandatory and may format bounded digits to check their count.
pub fn binary64_from_integer(value: &BigInt, limits: &NumericLimits) -> Result<f64, Binary64Fault> {
    limits.check_digits(value.magnitude(), limits.max_coefficient_digits)?;
    let result = positive_ratio_to_binary64(
        value.magnitude(),
        &BigUint::one(),
        value.sign() == Sign::Minus,
    )?;
    if result.is_infinite() {
        Err(Binary64Fault::IntegerOverflow)
    } else {
        Ok(result)
    }
}

/// Python integer true division with one nearest, ties-to-even rounding of
/// the exact ratio. Neither operand is independently converted to binary64:
/// two individually overflowing integers may still have a finite quotient.
///
/// Coefficient admission is this crate's explicit resource policy and precedes
/// zero-division detection. No input/output text or power-of-ten budget applies
/// to the ratio operation, and no Decimal context or signal is involved.
pub fn binary64_divide_integers(
    left: &BigInt,
    right: &BigInt,
    limits: &NumericLimits,
) -> Result<f64, Binary64Fault> {
    limits.check_digits(left.magnitude(), limits.max_coefficient_digits)?;
    limits.check_digits(right.magnitude(), limits.max_coefficient_digits)?;
    if right.is_zero() {
        return Err(Binary64Fault::ZeroDivision);
    }
    let result = positive_ratio_to_binary64(
        left.magnitude(),
        right.magnitude(),
        (left.sign() == Sign::Minus) != (right.sign() == Sign::Minus),
    )?;
    if result.is_infinite() {
        Err(Binary64Fault::IntegerOverflow)
    } else {
        Ok(result)
    }
}

/// The fixed CPython 3.13.11 builtin sum path for exactly two float items,
/// with its default integer-zero start. This is not an arbitrary-family sum
/// or a compensation state to retain across separate aggregate updates.
///
/// Adapted from Python/bltinmodule.c's int-to-float fallback, one Neumaier
/// step and finite nonzero correction guard; see LICENSE-PYTHON.txt and
/// PROVENANCE.md. Strict binary64 operations must retain this source order.
/// Decimal signals and resource allocation do not occur in this primitive.
pub fn python_sum_binary64_pair(first: f64, second: f64) -> f64 {
    // The source first performs int(0) + first. In particular, do not replace
    // this by `first`: a negative-zero first item loses its sign here.
    let first_sum = 0.0 + first;
    let mut result = first_sum + second;
    let mut correction = 0.0;
    if first_sum.abs() >= second.abs() {
        correction += (first_sum - result) + second;
    } else {
        correction += (second - result) + first_sum;
    }
    // Adding a zero correction can erase a negative result's sign. Adding a
    // nonfinite correction can turn a genuine infinite result into NaN.
    if correction != 0.0 && correction.is_finite() {
        result += correction;
    }
    result
}

/// Decimal-to-float does not round under, read or signal a Decimal context.
/// Overflow yields infinity, unlike integer conversion. The active resource
/// contract checks the original representation's |exponent| before allocating
/// a power of ten; cancelling trailing zeroes must not bypass that admission.
pub fn binary64_from_decimal(
    value: &DecimalValue,
    limits: &NumericLimits,
) -> Result<f64, Binary64Fault> {
    limits.check_value(value)?;
    let (negative, coefficient, exponent) = match value {
        DecimalValue::Infinity { negative } => return Ok(signed_infinity(*negative)),
        DecimalValue::NaN {
            signaling: true, ..
        } => {
            return Err(Binary64Fault::SignalingNaNConversion);
        }
        DecimalValue::NaN { negative, .. } => {
            return Ok(f64::from_bits(sign_bits(*negative) | 0x7ff8_0000_0000_0000));
        }
        DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        } => (*negative, coefficient, exponent),
    };
    if coefficient.is_zero() {
        return Ok(f64::from_bits(sign_bits(negative)));
    }
    let digits = limits.check_digits(coefficient, limits.max_coefficient_digits)?;
    let adjusted = exponent + BigInt::from(digits - 1);
    // These bounds are strict mathematical consequences, and need no power
    // allocation even for an extremely large original exponent.
    if adjusted >= BigInt::from(309) {
        return Ok(signed_infinity(negative));
    }
    if adjusted <= BigInt::from(-325) {
        return Ok(f64::from_bits(sign_bits(negative)));
    }
    let power = exponent
        .magnitude()
        .to_usize()
        .ok_or_else(|| NumericLimits::resource("materialized_power_of_ten"))?;
    let power = limits.power10(power)?;
    let result = if exponent.sign() == Sign::Minus {
        positive_ratio_to_binary64(coefficient, &power, negative)?
    } else {
        positive_ratio_to_binary64(&(coefficient * power), &BigUint::one(), negative)?
    };
    Ok(result)
}

/// Ordinary strict binary64 operations, in source order. Python checks both
/// signs of zero before division, including when the numerator is NaN or Inf.
/// Arithmetic overflow/underflow and NaNs otherwise remain floating values.
pub fn apply_binary64_binary(
    op: Binary64BinaryOp,
    left: f64,
    right: f64,
) -> Result<f64, Binary64Fault> {
    Ok(match op {
        Binary64BinaryOp::Add => left + right,
        Binary64BinaryOp::Subtract => left - right,
        Binary64BinaryOp::Multiply => left * right,
        Binary64BinaryOp::Divide => {
            if right == 0.0 {
                return Err(Binary64Fault::ZeroDivision);
            }
            left / right
        }
    })
}

pub fn apply_binary64_unary(op: Binary64UnaryOp, value: f64) -> f64 {
    match op {
        Binary64UnaryOp::Plus => value,
        Binary64UnaryOp::Minus => -value,
    }
}

fn sign_bits(negative: bool) -> u64 {
    u64::from(negative) << 63
}

fn signed_infinity(negative: bool) -> f64 {
    f64::from_bits(sign_bits(negative) | 0x7ff0_0000_0000_0000)
}

/// Round an admitted exact positive ratio directly to IEEE bits. There is no
/// intermediate float or decimal-string conversion (and thus no double
/// rounding). All callers perform input/power admission before reaching here.
fn positive_ratio_to_binary64(
    numerator: &BigUint,
    denominator: &BigUint,
    negative: bool,
) -> Result<f64, NumericFault> {
    if denominator.is_zero() {
        return Err(NumericFault::InvalidOperands);
    }
    if numerator.is_zero() {
        return Ok(f64::from_bits(sign_bits(negative)));
    }
    // floor(log2(n/d)) is either this difference or one less. Check extremes
    // before any shift, so even a very large admitted integer costs no large
    // temporary solely to prove overflow/underflow.
    let difference = i128::from(numerator.bits()) - i128::from(denominator.bits());
    if difference > 1024 {
        return Ok(signed_infinity(negative));
    }
    if difference < -1075 {
        return Ok(f64::from_bits(sign_bits(negative)));
    }
    let mut exponent =
        i32::try_from(difference).map_err(|_| NumericLimits::resource("binary64_ratio_shift"))?;
    let lower = if exponent >= 0 {
        numerator < &(denominator << bounded_shift(exponent)?)
    } else {
        &(numerator << bounded_shift(-exponent)?) < denominator
    };
    if lower {
        exponent -= 1;
    }
    if exponent > 1023 {
        return Ok(signed_infinity(negative));
    }
    if exponent < -1075 {
        return Ok(f64::from_bits(sign_bits(negative)));
    }
    // Subnormals all share the 2^-1074 quantum. For normals retain 53 bits.
    // These scaling shifts are at most 1074 (numerator) or 971 (denominator).
    let mut unit = (exponent - 52).max(-1074);
    let (rounded, _) = if unit <= 0 {
        crate::half_even_quotient(&(numerator << bounded_shift(-unit)?), denominator)
    } else {
        crate::half_even_quotient(numerator, &(denominator << bounded_shift(unit)?))
    };
    let mut significand = rounded.to_u64().ok_or(NumericFault::InvalidOperands)?;
    if significand == 0 {
        return Ok(f64::from_bits(sign_bits(negative)));
    }
    if significand == 1_u64 << 53 {
        significand >>= 1;
        unit += 1;
    }
    if significand < 1_u64 << 52 {
        // Here unit must be -1074; a normal ratio cannot round below 2^52.
        if unit != -1074 {
            return Err(NumericFault::InvalidOperands);
        }
        return Ok(f64::from_bits(sign_bits(negative) | significand));
    }
    if significand >= 1_u64 << 53 {
        return Err(NumericFault::InvalidOperands);
    }
    let exponent = unit + 52;
    if exponent > 1023 {
        return Ok(signed_infinity(negative));
    }
    let encoded_exponent =
        u64::try_from(exponent + 1023).map_err(|_| NumericFault::InvalidOperands)?;
    Ok(f64::from_bits(
        sign_bits(negative) | (encoded_exponent << 52) | (significand - (1_u64 << 52)),
    ))
}

fn bounded_shift(shift: i32) -> Result<usize, NumericFault> {
    usize::try_from(shift).map_err(|_| NumericLimits::resource("binary64_ratio_shift"))
}

/// Explicit `Decimal.from_float` semantics: exact value, no FloatOperation
/// signal and no context rounding. Finite conversion has at most 1074 binary
/// denominator powers and a 767-digit coefficient, independently of input
/// text or a caller-supplied exponent. The resulting representation must also
/// meet the caller's coefficient and exponent-input limits.
pub fn decimal_from_binary64(
    value: f64,
    limits: &NumericLimits,
) -> Result<DecimalValue, NumericFault> {
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let encoded_exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let result = if encoded_exponent == 0x7ff {
        if fraction == 0 {
            DecimalValue::Infinity { negative }
        } else {
            // Python float repr is "nan" for either sign and all NaN payloads.
            // Constructing Decimal from that repr produces a positive quiet NaN.
            DecimalValue::NaN {
                negative: false,
                signaling: false,
                payload: String::new(),
            }
        }
    } else if encoded_exponent == 0 && fraction == 0 {
        DecimalValue::Finite {
            negative,
            coefficient: BigUint::from(0_u8),
            exponent: BigInt::from(0),
        }
    } else {
        let (mut numerator, mut power) = if encoded_exponent == 0 {
            (fraction, -1074_i32)
        } else {
            ((1_u64 << 52) | fraction, encoded_exponent - 1023 - 52)
        };
        // as_integer_ratio uses the reduced denominator. Reducing here is
        // necessary to preserve Decimal.from_float's exact exponent/scale.
        if power < 0 {
            let cancelled = numerator.trailing_zeros().min((-power) as u32);
            numerator >>= cancelled;
            power += cancelled as i32;
        }
        let (coefficient, exponent) = if power >= 0 {
            (BigUint::from(numerator) << power as usize, 0)
        } else {
            (
                BigUint::from(numerator) * BigUint::from(5_u8).pow((-power) as u32),
                power,
            )
        };
        DecimalValue::Finite {
            negative,
            coefficient,
            exponent: BigInt::from(exponent),
        }
    };
    limits.check_value(&result)?;
    Ok(result)
}

impl DecimalContext {
    /// Implicit `Decimal(float)` construction. FloatOperation is signaled
    /// before conversion, including for NaN/Infinity. Construction itself is
    /// exact and does not apply this context's precision or exponent rounding.
    pub fn from_binary64(
        &self,
        value: f64,
        limits: &NumericLimits,
    ) -> Result<NumericOutcome<DecimalValue>, NumericFault> {
        self.validate(limits)?;
        let signals = DecimalSignals::from(DecimalSignal::FloatOperation);
        self.outcome((), signals)?;
        Ok(NumericOutcome {
            value: decimal_from_binary64(value, limits)?,
            signals,
        })
    }
}
