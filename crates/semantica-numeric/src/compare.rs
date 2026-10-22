//! Exact numeric relations, without arithmetic rounding or RDF ordering.
//!
//! Independently implemented sign/significand and coefficient/exponent math.
//! CPython rich comparison sources constrain dispatch/signals; their C
//! implementations are reference-only. See ../PROVENANCE.md.
use crate::{
    BigInt, BigUint, DecimalContext, DecimalSignal, DecimalSignals, DecimalValue, NumericFault,
    NumericLimits, NumericValue, decimal_from_binary64, decimal_from_integer,
};
use num_bigint::Sign;
use num_traits::Zero;
use std::borrow::Cow;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericRelation {
    Equal,
    Less,
    Greater,
}

impl NumericRelation {
    fn accepts(self, order: Option<Ordering>) -> bool {
        matches!(
            (self, order),
            (Self::Equal, Some(Ordering::Equal))
                | (Self::Less, Some(Ordering::Less))
                | (Self::Greater, Some(Ordering::Greater))
        )
    }
}

/// Signals reached before *any* failure remain available to the session.
/// In particular, an untrapped FloatOperation survives a conversion resource
/// refusal, and equality ignores the FloatOperation trap without erasing it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericComparisonResult {
    pub result: Result<bool, NumericFault>,
    pub signals: DecimalSignals,
}

impl DecimalContext {
    pub fn compare_numeric(
        &self,
        op: NumericRelation,
        left: &NumericValue,
        right: &NumericValue,
        limits: &NumericLimits,
    ) -> NumericComparisonResult {
        let mut signals = DecimalSignals::EMPTY;
        let result = self.compare_numeric_inner(op, left, right, limits, &mut signals);
        NumericComparisonResult { result, signals }
    }

    fn compare_numeric_inner(
        &self,
        op: NumericRelation,
        left: &NumericValue,
        right: &NumericValue,
        limits: &NumericLimits,
        signals: &mut DecimalSignals,
    ) -> Result<bool, NumericFault> {
        // Active resource admission is explicit, left then right. Neither
        // numeric formatting nor the integer text-output limit is consumed.
        self.validate(limits)?;
        check_input(left, limits)?;
        check_input(right, limits)?;
        let decimal =
            matches!(left, NumericValue::Decimal(_)) || matches!(right, NumericValue::Decimal(_));
        if decimal {
            let mixed_float = matches!(left, NumericValue::Binary64(_))
                || matches!(right, NumericValue::Binary64(_));
            if mixed_float {
                signals.insert(DecimalSignal::FloatOperation);
                if op != NumericRelation::Equal
                    && self.traps.contains(DecimalSignal::FloatOperation)
                {
                    return Err(NumericFault::DecimalTrap {
                        primary: DecimalSignal::FloatOperation,
                        signals: *signals,
                    });
                }
            }
            // Conversion follows the FloatOperation stage, even for NaNs.
            let left = as_decimal(left, limits)?;
            let right = as_decimal(right, limits)?;
            if matches!(&*left, DecimalValue::NaN { .. })
                || matches!(&*right, DecimalValue::NaN { .. })
            {
                let signaling = matches!(
                    &*left,
                    DecimalValue::NaN {
                        signaling: true,
                        ..
                    }
                ) || matches!(
                    &*right,
                    DecimalValue::NaN {
                        signaling: true,
                        ..
                    }
                );
                if signaling || op != NumericRelation::Equal {
                    signals.insert(DecimalSignal::InvalidOperation);
                    if self.traps.contains(DecimalSignal::InvalidOperation) {
                        return Err(NumericFault::DecimalTrap {
                            primary: DecimalSignal::InvalidOperation,
                            signals: *signals,
                        });
                    }
                }
                return Ok(false);
            }
            return Ok(op.accepts(Some(decimal_order(&left, &right))));
        }
        let order = match (left, right) {
            (NumericValue::Integer(a), NumericValue::Integer(b)) => Some(a.cmp(b)),
            (NumericValue::Binary64(a), NumericValue::Binary64(b)) => a.partial_cmp(b),
            (NumericValue::Integer(a), NumericValue::Binary64(b)) => integer_float_order(a, *b),
            (NumericValue::Binary64(a), NumericValue::Integer(b)) => {
                integer_float_order(b, *a).map(Ordering::reverse)
            }
            _ => return Err(NumericFault::InvalidOperands),
        };
        Ok(op.accepts(order))
    }
}

fn check_input(value: &NumericValue, limits: &NumericLimits) -> Result<(), NumericFault> {
    match value {
        NumericValue::Integer(value) => {
            limits.check_digits(value.magnitude(), limits.max_coefficient_digits)?;
            Ok(())
        }
        NumericValue::Decimal(value) => limits.check_value(value),
        NumericValue::Binary64(_) => Ok(()),
    }
}

fn as_decimal<'a>(
    value: &'a NumericValue,
    limits: &NumericLimits,
) -> Result<Cow<'a, DecimalValue>, NumericFault> {
    Ok(match value {
        NumericValue::Decimal(value) => Cow::Borrowed(value),
        NumericValue::Integer(value) => Cow::Owned(decimal_from_integer(value, limits)?),
        NumericValue::Binary64(value) => Cow::Owned(decimal_from_binary64(*value, limits)?),
    })
}

// Caller has excluded NaN. No exponent-sized power or padded string is built.
fn decimal_order(left: &DecimalValue, right: &DecimalValue) -> Ordering {
    use DecimalValue::{Finite, Infinity};
    match (left, right) {
        (Infinity { negative: a }, Infinity { negative: b }) => b.cmp(a),
        (Infinity { negative }, _) => {
            if *negative {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (_, Infinity { negative }) => {
            if *negative {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (
            Finite {
                negative: an,
                coefficient: ac,
                exponent: ae,
            },
            Finite {
                negative: bn,
                coefficient: bc,
                exponent: be,
            },
        ) => {
            if ac.is_zero() && bc.is_zero() {
                return Ordering::Equal;
            }
            let an = *an && !ac.is_zero();
            let bn = *bn && !bc.is_zero();
            if an != bn {
                return if an {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            let magnitude = if ac.is_zero() {
                Ordering::Less
            } else if bc.is_zero() {
                Ordering::Greater
            } else {
                // Inputs passed the bounded coefficient-digit admission.
                let a = ac.to_str_radix(10);
                let b = bc.to_str_radix(10);
                let adjusted_a = ae + BigInt::from(a.len());
                let adjusted_b = be + BigInt::from(b.len());
                adjusted_a.cmp(&adjusted_b).then_with(|| {
                    (0..a.len().max(b.len()))
                        .map(|i| {
                            a.as_bytes()
                                .get(i)
                                .copied()
                                .unwrap_or(b'0')
                                .cmp(&b.as_bytes().get(i).copied().unwrap_or(b'0'))
                        })
                        .find(|order| *order != Ordering::Equal)
                        .unwrap_or(Ordering::Equal)
                })
            };
            if an { magnitude.reverse() } else { magnitude }
        }
        // NaNs are removed by the relation owner, never coerced to equal.
        _ => unreachable!("NaN is handled before finite Decimal comparison"),
    }
}

fn integer_float_order(integer: &BigInt, float: f64) -> Option<Ordering> {
    if float.is_nan() {
        return None;
    }
    if float.is_infinite() {
        return Some(if float.is_sign_negative() {
            Ordering::Greater
        } else {
            Ordering::Less
        });
    }
    if float == 0.0 {
        return Some(integer.cmp(&BigInt::zero()));
    }
    let negative = float.is_sign_negative();
    let int_negative = integer.sign() == Sign::Minus;
    if integer.is_zero() || negative != int_negative {
        return Some(if negative {
            Ordering::Greater
        } else {
            Ordering::Less
        });
    }
    let bits = float.to_bits();
    let encoded = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (significand, exponent) = if encoded == 0 {
        (fraction, -1074)
    } else {
        ((1_u64 << 52) | fraction, encoded - 1023 - 52)
    };
    let significand = BigUint::from(significand);
    let magnitude = if exponent >= 0 {
        // This fixed-format temporary has at most 1024 bits.
        integer.magnitude().cmp(&(significand << exponent as usize))
    } else {
        let shift = (-exponent) as usize;
        let left_bits = u128::from(integer.magnitude().bits()) + shift as u128;
        let right_bits = u128::from(significand.bits());
        left_bits.cmp(&right_bits).then_with(|| {
            // Equal bit lengths restrict this allocation to at most 53 bits.
            (integer.magnitude() << shift).cmp(&significand)
        })
    };
    Some(if negative {
        magnitude.reverse()
    } else {
        magnitude
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(coefficient: u32, exponent: i32, negative: bool) -> NumericValue {
        NumericValue::Decimal(DecimalValue::Finite {
            negative,
            coefficient: BigUint::from(coefficient),
            exponent: BigInt::from(exponent),
        })
    }

    #[test]
    fn comparison_integer_float_keeps_exact_adjacent_and_subnormal_relations() {
        let context = DecimalContext::default();
        let limits = NumericLimits::default();
        for (integer, float, greater) in [
            (
                BigInt::from(9_007_199_254_740_993_u64),
                9_007_199_254_740_992.0,
                true,
            ),
            (BigInt::zero(), f64::from_bits(1), false),
            (BigInt::from(-1), -f64::from_bits(1), false),
            (BigInt::from(10_u8).pow(400), f64::MAX, true),
        ] {
            let result = context.compare_numeric(
                NumericRelation::Greater,
                &NumericValue::Integer(integer),
                &NumericValue::Binary64(float),
                &limits,
            );
            assert_eq!(result.result, Ok(greater));
            assert_eq!(result.signals, DecimalSignals::EMPTY);
        }
    }

    #[test]
    fn comparison_decimal_scale_zero_and_huge_exponent_need_no_power() {
        let context = DecimalContext {
            precision: 1,
            ..DecimalContext::default()
        };
        let limits = NumericLimits {
            max_materialized_power_of_ten: 0,
            max_output_characters: 0,
            ..NumericLimits::default()
        };
        for (a, b, op) in [
            (
                decimal(10, -1, false),
                decimal(100, -2, false),
                NumericRelation::Equal,
            ),
            (
                decimal(0, -7, true),
                decimal(0, 3, false),
                NumericRelation::Equal,
            ),
            (
                decimal(1001, -3, false),
                decimal(1000, -3, false),
                NumericRelation::Greater,
            ),
            (
                decimal(1, i32::MAX, false),
                decimal(9, 0, false),
                NumericRelation::Greater,
            ),
        ] {
            let result = context.compare_numeric(op, &a, &b, &limits);
            assert_eq!(result.result, Ok(true));
            assert_eq!(result.signals, DecimalSignals::EMPTY);
        }
    }

    #[test]
    fn comparison_float_signal_survives_conversion_failure_and_eq_ignores_trap() {
        let context = DecimalContext {
            precision: 3,
            traps: DecimalSignal::FloatOperation.into(),
            ..DecimalContext::default()
        };
        let limits = NumericLimits {
            max_coefficient_digits: 3,
            ..NumericLimits::default()
        };
        let result = context.compare_numeric(
            NumericRelation::Equal,
            &decimal(1, -1, false),
            &NumericValue::Binary64(0.1),
            &limits,
        );
        assert_eq!(
            result.result,
            Err(NumericFault::ResourceLimit {
                operation: "coefficient_digits"
            })
        );
        assert_eq!(result.signals, DecimalSignal::FloatOperation.into());
        let trapped = context.compare_numeric(
            NumericRelation::Less,
            &decimal(1, -1, false),
            &NumericValue::Binary64(0.1),
            &limits,
        );
        assert!(matches!(
            trapped.result,
            Err(NumericFault::DecimalTrap {
                primary: DecimalSignal::FloatOperation,
                ..
            })
        ));
    }

    #[test]
    fn comparison_nan_preserves_operation_specific_signal_priority() {
        let context = DecimalContext {
            traps: [
                DecimalSignal::FloatOperation,
                DecimalSignal::InvalidOperation,
            ]
            .into_iter()
            .collect(),
            ..DecimalContext::default()
        };
        let limits = NumericLimits::default();
        let qnan = NumericValue::Decimal(DecimalValue::NaN {
            negative: false,
            signaling: false,
            payload: "7".into(),
        });
        let float = NumericValue::Binary64(1.0);
        let equal = context.compare_numeric(NumericRelation::Equal, &qnan, &float, &limits);
        assert_eq!(equal.result, Ok(false));
        assert_eq!(equal.signals, DecimalSignal::FloatOperation.into());
        let less = context.compare_numeric(NumericRelation::Less, &qnan, &float, &limits);
        assert!(matches!(
            less.result,
            Err(NumericFault::DecimalTrap {
                primary: DecimalSignal::FloatOperation,
                ..
            })
        ));
        assert!(!less.signals.contains(DecimalSignal::InvalidOperation));
    }
}
