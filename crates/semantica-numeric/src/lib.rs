//! Shared exact numeric primitives. No RDF, expression AST, or query evaluation.
//!
//! Decimal arithmetic is a Rust adaptation of the pinned Python 3.13.11
//! _pydecimal reference; see PROVENANCE.md and LICENSE-PYTHON.txt.
//! Integer arithmetic and the coefficient/exponent representation use num-bigint.
//! HALF_EVEN rounding follows the earlier runtime duration primitive's method;
//! migration of that consumer is a separate integration step.
mod binary64;
mod compare;
mod decimal;
mod lexical;

pub use binary64::{
    Binary64BinaryOp, Binary64Fault, Binary64UnaryOp, apply_binary64_binary, apply_binary64_unary,
    binary64_divide_integers, binary64_from_decimal, binary64_from_integer, decimal_from_binary64,
    python_sum_binary64_pair,
};
pub use compare::{NumericComparisonResult, NumericRelation};
pub use lexical::{format_decimal_fixed, format_integer, parse_decimal_exact, parse_integer};
use num_bigint::Sign;
pub use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_traits::{One, Zero};
use std::fmt;
use std::ops::{BitOr, BitOrAssign};

/// Representation equality includes exponent and zero sign. RDF equality is
/// deliberately the responsibility of a separate consumer, never this type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecimalValue {
    Finite {
        negative: bool,
        coefficient: BigUint,
        exponent: BigInt,
    },
    Infinity {
        negative: bool,
    },
    NaN {
        negative: bool,
        signaling: bool,
        payload: String,
    },
}

impl DecimalValue {
    pub fn is_zero(&self) -> bool {
        matches!(self, Self::Finite { coefficient, .. } if coefficient.is_zero())
    }

    pub fn is_negative(&self) -> bool {
        match self {
            Self::Finite { negative, .. }
            | Self::Infinity { negative }
            | Self::NaN { negative, .. } => *negative,
        }
    }

    /// Python numeric truth, including true for either kind of NaN.
    pub fn python_truth(&self) -> bool {
        !self.is_zero()
    }

    pub fn copy_negate(&self) -> Self {
        let mut result = self.clone();
        match &mut result {
            Self::Finite { negative, .. }
            | Self::Infinity { negative }
            | Self::NaN { negative, .. } => *negative = !*negative,
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NumericValue {
    Integer(BigInt),
    Decimal(DecimalValue),
    Binary64(f64),
}

/// Only the rounding policy supplied by this implementation is constructible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundingMode {
    HalfEven,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecimalOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Plus,
    Minus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegerOp {
    Add,
    Subtract,
    Multiply,
    Plus,
    Minus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum DecimalSignal {
    InvalidOperation = 1,
    DivisionByZero = 2,
    Overflow = 4,
    Underflow = 8,
    Subnormal = 16,
    Inexact = 32,
    Rounded = 64,
    Clamped = 128,
    FloatOperation = 256,
}

impl DecimalSignal {
    pub const ALL: [Self; 9] = [
        Self::InvalidOperation,
        Self::DivisionByZero,
        Self::Overflow,
        Self::Underflow,
        Self::Subnormal,
        Self::Inexact,
        Self::Rounded,
        Self::Clamped,
        Self::FloatOperation,
    ];
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DecimalSignals(u16);

impl DecimalSignals {
    pub const EMPTY: Self = Self(0);

    pub fn contains(self, signal: DecimalSignal) -> bool {
        self.0 & signal as u16 != 0
    }

    pub fn insert(&mut self, signal: DecimalSignal) {
        self.0 |= signal as u16;
    }

    pub fn iter(self) -> impl Iterator<Item = DecimalSignal> {
        DecimalSignal::ALL
            .into_iter()
            .filter(move |signal| self.contains(*signal))
    }
}

impl From<DecimalSignal> for DecimalSignals {
    fn from(signal: DecimalSignal) -> Self {
        Self(signal as u16)
    }
}
impl FromIterator<DecimalSignal> for DecimalSignals {
    fn from_iter<T: IntoIterator<Item = DecimalSignal>>(iter: T) -> Self {
        let mut signals = Self::EMPTY;
        for signal in iter {
            signals.insert(signal);
        }
        signals
    }
}
impl BitOr for DecimalSignals {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl BitOrAssign for DecimalSignals {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericOutcome<T> {
    pub value: T,
    pub signals: DecimalSignals,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NumericFault {
    DecimalTrap {
        primary: DecimalSignal,
        signals: DecimalSignals,
    },
    ResourceLimit {
        operation: &'static str,
    },
    IntegerTextLimit,
    InvalidLexical,
    InvalidContext,
    InvalidOperands,
}

impl fmt::Display for NumericFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecimalTrap { primary, .. } => write!(f, "Decimal {primary:?}"),
            Self::ResourceLimit { operation } => write!(f, "numeric resource limit: {operation}"),
            Self::IntegerTextLimit => f.write_str("integer decimal text conversion limit"),
            Self::InvalidLexical => f.write_str("invalid numeric lexical form"),
            Self::InvalidContext => f.write_str("invalid Decimal context"),
            Self::InvalidOperands => f.write_str("invalid numeric operand arity or representation"),
        }
    }
}
impl std::error::Error for NumericFault {}

/// Explicit operational budgets, separate from mathematical range and traps.
/// Every entry point checks inputs and any requested materialized power of ten.
/// Decimal arithmetic may use a bounded two-input temporary before rounding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericLimits {
    pub max_input_characters: usize,
    pub max_coefficient_digits: usize,
    pub max_output_characters: usize,
    pub max_materialized_power_of_ten: usize,
}

impl Default for NumericLimits {
    fn default() -> Self {
        Self {
            max_input_characters: 4096,
            max_coefficient_digits: 4096,
            max_output_characters: 4096,
            max_materialized_power_of_ten: 4096,
        }
    }
}

impl NumericLimits {
    pub(crate) fn resource(operation: &'static str) -> NumericFault {
        NumericFault::ResourceLimit { operation }
    }

    pub(crate) fn check_digits(
        &self,
        coefficient: &BigUint,
        maximum: usize,
    ) -> Result<usize, NumericFault> {
        // Four bits per permitted decimal digit is a safe upper bound. Check
        // before allocating a decimal representation for externally built values.
        if u128::from(coefficient.bits()) > (maximum as u128).saturating_mul(4) {
            return Err(Self::resource("coefficient_digits"));
        }
        let digits = coefficient.to_str_radix(10).len();
        if digits > maximum {
            return Err(Self::resource("coefficient_digits"));
        }
        Ok(digits)
    }

    pub(crate) fn check_value(&self, value: &DecimalValue) -> Result<(), NumericFault> {
        match value {
            DecimalValue::Finite {
                coefficient,
                exponent,
                ..
            } => {
                self.check_digits(coefficient, self.max_coefficient_digits)?;
                self.check_digits(exponent.magnitude(), self.max_input_characters)?;
            }
            DecimalValue::NaN { payload, .. } => {
                if payload.len() > self.max_coefficient_digits {
                    return Err(Self::resource("coefficient_digits"));
                }
                if !payload.bytes().all(|b| b.is_ascii_digit()) || payload.starts_with('0') {
                    return Err(NumericFault::InvalidOperands);
                }
            }
            DecimalValue::Infinity { .. } => {}
        }
        Ok(())
    }

    pub(crate) fn power10(&self, power: usize) -> Result<BigUint, NumericFault> {
        if power > self.max_materialized_power_of_ten {
            return Err(Self::resource("materialized_power_of_ten"));
        }
        let power =
            u32::try_from(power).map_err(|_| Self::resource("materialized_power_of_ten"))?;
        Ok(BigUint::from(10_u8).pow(power))
    }

    pub(crate) fn work_digits(&self) -> usize {
        self.max_coefficient_digits
            .saturating_mul(2)
            .saturating_add(2)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecimalContext {
    pub precision: usize,
    pub rounding: RoundingMode,
    pub emin: BigInt,
    pub emax: BigInt,
    pub clamp: bool,
    pub traps: DecimalSignals,
}

impl Default for DecimalContext {
    fn default() -> Self {
        Self {
            precision: 28,
            rounding: RoundingMode::HalfEven,
            emin: BigInt::from(-999_999),
            emax: BigInt::from(999_999),
            clamp: false,
            traps: [
                DecimalSignal::InvalidOperation,
                DecimalSignal::DivisionByZero,
                DecimalSignal::Overflow,
            ]
            .into_iter()
            .collect(),
        }
    }
}

impl DecimalContext {
    pub fn etiny(&self) -> BigInt {
        &self.emin - BigInt::from(self.precision.saturating_sub(1))
    }

    pub fn etop(&self) -> BigInt {
        &self.emax - BigInt::from(self.precision.saturating_sub(1))
    }

    pub(crate) fn validate(&self, limits: &NumericLimits) -> Result<(), NumericFault> {
        if self.precision == 0 || self.emin > BigInt::zero() || self.emax < BigInt::zero() {
            return Err(NumericFault::InvalidContext);
        }
        if self.precision > limits.max_coefficient_digits {
            return Err(NumericLimits::resource("context_precision"));
        }
        limits.check_digits(self.emin.magnitude(), limits.max_input_characters)?;
        limits.check_digits(self.emax.magnitude(), limits.max_input_characters)?;
        Ok(())
    }

    pub(crate) fn outcome<T>(
        &self,
        value: T,
        signals: DecimalSignals,
    ) -> Result<NumericOutcome<T>, NumericFault> {
        // Deterministic signal precedence; complete trapped flags are a core
        // contract, not a claim about the historical C Decimal backend.
        if let Some(primary) = signals.iter().find(|signal| self.traps.contains(*signal)) {
            Err(NumericFault::DecimalTrap { primary, signals })
        } else {
            Ok(NumericOutcome { value, signals })
        }
    }

    /// Like exact construction: valid values do not undergo context rounding.
    /// Syntax failure follows the context InvalidOperation trap policy; budgets
    /// remain fatal resource errors and are never converted to numeric NaN.
    pub fn parse(
        &self,
        text: &str,
        limits: &NumericLimits,
    ) -> Result<NumericOutcome<DecimalValue>, NumericFault> {
        self.validate(limits)?;
        match parse_decimal_exact(text, limits) {
            Ok(value) => self.outcome(value, DecimalSignals::EMPTY),
            Err(NumericFault::InvalidLexical) => self.outcome(
                DecimalValue::NaN {
                    negative: false,
                    signaling: false,
                    payload: String::new(),
                },
                DecimalSignal::InvalidOperation.into(),
            ),
            Err(error) => Err(error),
        }
    }
}

pub fn decimal_from_integer(
    value: &BigInt,
    limits: &NumericLimits,
) -> Result<DecimalValue, NumericFault> {
    limits.check_digits(value.magnitude(), limits.max_coefficient_digits)?;
    Ok(DecimalValue::Finite {
        negative: value.sign() == Sign::Minus,
        coefficient: value.magnitude().clone(),
        exponent: BigInt::zero(),
    })
}

/// Exact integer arithmetic; the caller applies text-conversion limits only
/// at its actual construction/format boundary, not at every chain operation.
pub fn apply_integer(
    op: IntegerOp,
    lhs: &BigInt,
    rhs: Option<&BigInt>,
    limits: &NumericLimits,
) -> Result<BigInt, NumericFault> {
    limits.check_digits(lhs.magnitude(), limits.max_coefficient_digits)?;
    if let Some(rhs) = rhs {
        limits.check_digits(rhs.magnitude(), limits.max_coefficient_digits)?;
    }
    let value = match (op, rhs) {
        (IntegerOp::Plus, None) => lhs.clone(),
        (IntegerOp::Minus, None) => -lhs,
        (IntegerOp::Add, Some(rhs)) => lhs + rhs,
        (IntegerOp::Subtract, Some(rhs)) => lhs - rhs,
        (IntegerOp::Multiply, Some(rhs)) => lhs * rhs,
        _ => return Err(NumericFault::InvalidOperands),
    };
    limits.check_digits(value.magnitude(), limits.max_coefficient_digits)?;
    Ok(value)
}

/// The unique HALF_EVEN quotient primitive. Reused for division and all
/// coefficient shortening; no Decimal operation takes an f64 intermediate.
pub(crate) fn half_even_quotient(numerator: &BigUint, denominator: &BigUint) -> (BigUint, bool) {
    let (mut quotient, remainder) = numerator.div_rem(denominator);
    let inexact = !remainder.is_zero();
    let twice = &remainder << 1_usize;
    if twice > *denominator || (twice == *denominator && quotient.is_odd()) {
        quotient += BigUint::one();
    }
    (quotient, inexact)
}
