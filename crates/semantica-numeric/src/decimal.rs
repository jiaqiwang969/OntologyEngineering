// Copyright (c) 2004 Python Software Foundation. All rights reserved.
// Rust adaptation: coefficient/exponent, HALF_EVEN arithmetic and signals.
// Source functions and changes are recorded in ../PROVENANCE.md.
use crate::{
    BigInt, BigUint, DecimalContext, DecimalOp, DecimalSignal as Signal, DecimalSignals,
    DecimalValue as Value, NumericFault, NumericLimits, NumericOutcome, half_even_quotient,
};
use num_bigint::Sign;
use num_integer::Integer;
use num_traits::{One, ToPrimitive, Zero};

impl DecimalContext {
    pub fn apply(
        &self,
        op: DecimalOp,
        a: &Value,
        b: Option<&Value>,
        limits: &NumericLimits,
    ) -> Result<NumericOutcome<Value>, NumericFault> {
        self.validate(limits)?;
        let unary = matches!(op, DecimalOp::Plus | DecimalOp::Minus);
        if unary != b.is_none() {
            return Err(NumericFault::InvalidOperands);
        }
        limits.check_value(a)?;
        if let Some(b) = b {
            limits.check_value(b)?;
        }
        // Subtraction must check NaNs before negating the right operand.
        if let Some((value, signals)) = self.nan_result(a, b) {
            return self.outcome(value, signals);
        }
        match op {
            DecimalOp::Plus | DecimalOp::Minus => {
                let value = match a {
                    Value::Finite {
                        negative,
                        coefficient,
                        exponent,
                    } => Value::Finite {
                        negative: if coefficient.is_zero() {
                            false
                        } else {
                            *negative ^ matches!(op, DecimalOp::Minus)
                        },
                        coefficient: coefficient.clone(),
                        exponent: exponent.clone(),
                    },
                    Value::Infinity { negative } => Value::Infinity {
                        negative: *negative ^ matches!(op, DecimalOp::Minus),
                    },
                    Value::NaN { .. } => return Err(NumericFault::InvalidOperands),
                };
                self.finish(value, DecimalSignals::EMPTY, limits)
            }
            DecimalOp::Add => self.add(a, b.ok_or(NumericFault::InvalidOperands)?, limits),
            DecimalOp::Subtract => self.add(
                a,
                &b.ok_or(NumericFault::InvalidOperands)?.copy_negate(),
                limits,
            ),
            DecimalOp::Multiply => {
                self.multiply(a, b.ok_or(NumericFault::InvalidOperands)?, limits)
            }
            DecimalOp::Divide => self.divide(a, b.ok_or(NumericFault::InvalidOperands)?, limits),
        }
    }

    fn nan_result(&self, a: &Value, b: Option<&Value>) -> Option<(Value, DecimalSignals)> {
        let operands = [Some(a), b];
        // Either signaling NaN takes precedence over either quiet NaN.
        let selected = operands
            .iter()
            .flatten()
            .copied()
            .find(|value| {
                matches!(
                    value,
                    Value::NaN {
                        signaling: true,
                        ..
                    }
                )
            })
            .or_else(|| {
                operands
                    .iter()
                    .flatten()
                    .copied()
                    .find(|value| matches!(value, Value::NaN { .. }))
            })?;
        let Value::NaN {
            negative,
            signaling,
            payload,
        } = selected
        else {
            return None;
        };
        let maximum = self.precision - usize::from(self.clamp);
        let payload = if payload.len() > maximum {
            payload[payload.len() - maximum..]
                .trim_start_matches('0')
                .to_owned()
        } else {
            payload.clone()
        };
        Some((
            Value::NaN {
                negative: *negative,
                signaling: false,
                payload,
            },
            if *signaling {
                Signal::InvalidOperation.into()
            } else {
                DecimalSignals::EMPTY
            },
        ))
    }

    fn invalid(&self) -> Result<NumericOutcome<Value>, NumericFault> {
        self.outcome(
            Value::NaN {
                negative: false,
                signaling: false,
                payload: String::new(),
            },
            Signal::InvalidOperation.into(),
        )
    }

    fn overflow(
        &self,
        negative: bool,
        mut signals: DecimalSignals,
    ) -> Result<NumericOutcome<Value>, NumericFault> {
        signals.insert(Signal::Overflow);
        signals.insert(Signal::Inexact);
        signals.insert(Signal::Rounded);
        self.outcome(Value::Infinity { negative }, signals)
    }

    /// The single finalization owner: precision and Etiny are applied in one
    /// rounding, including rounding a subnormal into the normal range.
    fn finish(
        &self,
        value: Value,
        mut signals: DecimalSignals,
        limits: &NumericLimits,
    ) -> Result<NumericOutcome<Value>, NumericFault> {
        let Value::Finite {
            negative,
            mut coefficient,
            mut exponent,
        } = value
        else {
            return self.outcome(value, signals);
        };
        let digits = limits.check_digits(&coefficient, limits.work_digits())?;
        let etiny = self.etiny();
        let etop = self.etop();
        if coefficient.is_zero() {
            let upper = if self.clamp { etop } else { self.emax.clone() };
            let fixed = exponent.clone().max(etiny).min(upper);
            if fixed != exponent {
                signals.insert(Signal::Clamped);
                exponent = fixed;
            }
            return self.outcome(
                Value::Finite {
                    negative,
                    coefficient,
                    exponent,
                },
                signals,
            );
        }

        let minimum = &exponent + BigInt::from(digits) - BigInt::from(self.precision);
        if minimum > etop {
            return self.overflow(negative, signals);
        }
        let subnormal = minimum < etiny;
        let target = minimum.max(etiny);
        if exponent < target {
            let shift = &target - &exponent;
            let (rounded, inexact) = if shift > BigInt::from(digits) {
                // Less than one tenth of the target quantum. No enormous
                // power is materialized merely to prove HALF_EVEN gives zero.
                (BigUint::zero(), true)
            } else {
                let shift = shift
                    .to_usize()
                    .ok_or_else(|| NumericLimits::resource("materialized_power_of_ten"))?;
                half_even_quotient(&coefficient, &limits.power10(shift)?)
            };
            coefficient = rounded;
            exponent = target;
            if limits.check_digits(&coefficient, limits.work_digits())? > self.precision {
                coefficient /= 10_u8;
                exponent += 1;
            }
            if exponent > etop {
                return self.overflow(negative, signals);
            }
            if inexact && subnormal {
                signals.insert(Signal::Underflow);
            }
            if subnormal {
                signals.insert(Signal::Subnormal);
            }
            if inexact {
                signals.insert(Signal::Inexact);
            }
            signals.insert(Signal::Rounded);
            if coefficient.is_zero() {
                signals.insert(Signal::Clamped);
            }
            return self.outcome(
                Value::Finite {
                    negative,
                    coefficient,
                    exponent,
                },
                signals,
            );
        }
        if subnormal {
            signals.insert(Signal::Subnormal);
        }
        if self.clamp && exponent > etop {
            let shift = (&exponent - &etop)
                .to_usize()
                .ok_or_else(|| NumericLimits::resource("materialized_power_of_ten"))?;
            coefficient *= limits.power10(shift)?;
            exponent = etop;
            signals.insert(Signal::Clamped);
        }
        self.outcome(
            Value::Finite {
                negative,
                coefficient,
                exponent,
            },
            signals,
        )
    }

    fn add(
        &self,
        a: &Value,
        b: &Value,
        limits: &NumericLimits,
    ) -> Result<NumericOutcome<Value>, NumericFault> {
        match (a, b) {
            (Value::Infinity { negative: x }, Value::Infinity { negative: y }) if x != y => {
                return self.invalid();
            }
            (Value::Infinity { .. }, _) => return self.outcome(a.clone(), DecimalSignals::EMPTY),
            (_, Value::Infinity { .. }) => return self.outcome(b.clone(), DecimalSignals::EMPTY),
            _ => {}
        }
        let (
            Value::Finite {
                negative: an,
                coefficient: ac,
                exponent: ae,
            },
            Value::Finite {
                negative: bn,
                coefficient: bc,
                exponent: be,
            },
        ) = (a, b)
        else {
            return Err(NumericFault::InvalidOperands);
        };
        let preferred = ae.min(be).clone();
        if ac.is_zero() && bc.is_zero() {
            return self.finish(
                Value::Finite {
                    negative: *an && *bn,
                    coefficient: BigUint::zero(),
                    exponent: preferred,
                },
                DecimalSignals::EMPTY,
                limits,
            );
        }
        if ac.is_zero() || bc.is_zero() {
            let (negative, coefficient, exponent) = if ac.is_zero() {
                (*bn, bc, be)
            } else {
                (*an, ac, ae)
            };
            // _pydecimal's zero operand branch preserves preferred scale up to
            // sufficient guard digits, avoiding arbitrary exponent expansion.
            let target = preferred.max(exponent - BigInt::from(self.precision) - 1);
            let shift = (exponent - &target)
                .to_usize()
                .ok_or_else(|| NumericLimits::resource("materialized_power_of_ten"))?;
            return self.finish(
                Value::Finite {
                    negative,
                    coefficient: coefficient * limits.power10(shift)?,
                    exponent: target,
                },
                DecimalSignals::EMPTY,
                limits,
            );
        }
        let (mut high, mut low) = if ae >= be {
            ((*an, ac.clone(), ae.clone()), (*bn, bc.clone(), be.clone()))
        } else {
            ((*bn, bc.clone(), be.clone()), (*an, ac.clone(), ae.clone()))
        };
        // Source _normalize: a sufficiently tiny nonzero operand may be
        // replaced by a sticky guard quantum, for either addition or subtraction.
        let high_digits = limits.check_digits(&high.1, limits.max_coefficient_digits)?;
        let low_digits = limits.check_digits(&low.1, limits.max_coefficient_digits)?;
        let guard = &high.2
            + (BigInt::from(high_digits) - BigInt::from(self.precision) - BigInt::from(2))
                .min(BigInt::from(-1));
        if &low.2 + BigInt::from(low_digits) - BigInt::one() < guard {
            low.1 = BigUint::one();
            low.2 = guard;
        }
        let shift = (&high.2 - &low.2)
            .to_usize()
            .ok_or_else(|| NumericLimits::resource("materialized_power_of_ten"))?;
        high.1 *= limits.power10(shift)?;
        limits.check_digits(&high.1, limits.work_digits())?;
        let left = BigInt::from_biguint(if high.0 { Sign::Minus } else { Sign::Plus }, high.1);
        let right = BigInt::from_biguint(if low.0 { Sign::Minus } else { Sign::Plus }, low.1);
        let sum = left + right;
        let exponent = if sum.is_zero() { preferred } else { low.2 };
        self.finish(
            Value::Finite {
                negative: sum.sign() == Sign::Minus,
                coefficient: sum.magnitude().clone(),
                exponent,
            },
            DecimalSignals::EMPTY,
            limits,
        )
    }

    fn multiply(
        &self,
        a: &Value,
        b: &Value,
        limits: &NumericLimits,
    ) -> Result<NumericOutcome<Value>, NumericFault> {
        let negative = a.is_negative() ^ b.is_negative();
        if matches!(a, Value::Infinity { .. }) || matches!(b, Value::Infinity { .. }) {
            if a.is_zero() || b.is_zero() {
                return self.invalid();
            }
            return self.outcome(Value::Infinity { negative }, DecimalSignals::EMPTY);
        }
        let (
            Value::Finite {
                coefficient: ac,
                exponent: ae,
                ..
            },
            Value::Finite {
                coefficient: bc,
                exponent: be,
                ..
            },
        ) = (a, b)
        else {
            return Err(NumericFault::InvalidOperands);
        };
        self.finish(
            Value::Finite {
                negative,
                coefficient: ac * bc,
                exponent: ae + be,
            },
            DecimalSignals::EMPTY,
            limits,
        )
    }

    fn divide(
        &self,
        a: &Value,
        b: &Value,
        limits: &NumericLimits,
    ) -> Result<NumericOutcome<Value>, NumericFault> {
        let negative = a.is_negative() ^ b.is_negative();
        if matches!(a, Value::Infinity { .. }) {
            if matches!(b, Value::Infinity { .. }) {
                return self.invalid();
            }
            // Infinity / zero is Infinity, without DivisionByZero.
            return self.outcome(Value::Infinity { negative }, DecimalSignals::EMPTY);
        }
        if matches!(b, Value::Infinity { .. }) {
            return self.outcome(
                Value::Finite {
                    negative,
                    coefficient: BigUint::zero(),
                    exponent: self.etiny(),
                },
                Signal::Clamped.into(),
            );
        }
        let (
            Value::Finite {
                coefficient: ac,
                exponent: ae,
                ..
            },
            Value::Finite {
                coefficient: bc,
                exponent: be,
                ..
            },
        ) = (a, b)
        else {
            return Err(NumericFault::InvalidOperands);
        };
        if bc.is_zero() {
            if ac.is_zero() {
                return self.invalid();
            }
            return self.outcome(Value::Infinity { negative }, Signal::DivisionByZero.into());
        }
        let ideal = ae - be;
        if ac.is_zero() {
            return self.finish(
                Value::Finite {
                    negative,
                    coefficient: BigUint::zero(),
                    exponent: ideal,
                },
                DecimalSignals::EMPTY,
                limits,
            );
        }
        let ad = limits.check_digits(ac, limits.max_coefficient_digits)?;
        let bd = limits.check_digits(bc, limits.max_coefficient_digits)?;
        // Compute two guard digits. The remainder's sticky replacement is
        // taken from the source __truediv__; finish performs the sole rounding,
        // also when the result is subnormal. Exponent magnitude does not
        // control any allocation in this step.
        let shift =
            BigInt::from(bd) - BigInt::from(ad) + BigInt::from(self.precision) + BigInt::one();
        let exponent = &ideal - &shift;
        let (mut coefficient, remainder) = if shift >= BigInt::zero() {
            let power = shift
                .to_usize()
                .ok_or_else(|| NumericLimits::resource("materialized_power_of_ten"))?;
            (ac * limits.power10(power)?).div_rem(bc)
        } else {
            let power = (-shift)
                .to_usize()
                .ok_or_else(|| NumericLimits::resource("materialized_power_of_ten"))?;
            ac.div_rem(&(bc * limits.power10(power)?))
        };
        let mut exponent = exponent;
        if !remainder.is_zero() {
            if (&coefficient % 5_u8).is_zero() {
                coefficient += 1_u8;
            }
        } else {
            while exponent < ideal && (&coefficient % 10_u8).is_zero() {
                coefficient /= 10_u8;
                exponent += 1;
            }
        }
        self.finish(
            Value::Finite {
                negative,
                coefficient,
                exponent,
            },
            DecimalSignals::EMPTY,
            limits,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format_decimal_fixed, parse_decimal_exact};

    fn parse(text: &str) -> Value {
        parse_decimal_exact(text, &NumericLimits::default()).unwrap()
    }

    #[test]
    fn chain_preserves_the_numeric_value_without_a_text_round_trip() {
        let limits = NumericLimits::default();
        let context = DecimalContext::default();
        let numerator = parse("100");
        let denominator = parse("1E+2");
        let result = context
            .apply(DecimalOp::Divide, &numerator, Some(&denominator), &limits)
            .unwrap();
        assert_eq!(
            format_decimal_fixed(&result.value, &limits).unwrap(),
            "1.00"
        );
        let rebound = context
            .apply(DecimalOp::Divide, &parse("1"), Some(&result.value), &limits)
            .unwrap();
        assert_eq!(format_decimal_fixed(&rebound.value, &limits).unwrap(), "1");
    }

    #[test]
    fn precision_and_etiny_round_together() {
        let limits = NumericLimits::default();
        let context = DecimalContext {
            precision: 3,
            emin: (-2).into(),
            emax: 2.into(),
            traps: DecimalSignals::EMPTY,
            ..DecimalContext::default()
        };
        let result = context
            .apply(DecimalOp::Plus, &parse("0.00999"), None, &limits)
            .unwrap();
        assert_eq!(
            format_decimal_fixed(&result.value, &limits).unwrap(),
            "0.0100"
        );
        assert!(result.signals.contains(Signal::Underflow));
        assert!(result.signals.contains(Signal::Subnormal));
    }

    #[test]
    fn huge_exponent_underflow_does_not_materialize_a_power() {
        let limits = NumericLimits::default();
        let context = DecimalContext::default();
        let value = parse("-1E-99999999999999999999999999999999999");
        let result = context
            .apply(DecimalOp::Plus, &value, None, &limits)
            .unwrap();
        assert_eq!(
            result.value,
            Value::Finite {
                negative: true,
                coefficient: BigUint::zero(),
                exponent: context.etiny(),
            }
        );
        assert!(result.signals.contains(Signal::Underflow));
    }

    #[test]
    fn distant_addend_uses_guard_quantum_instead_of_unbounded_alignment() {
        let limits = NumericLimits::default();
        let context = DecimalContext::default();
        let result = context
            .apply(
                DecimalOp::Add,
                &parse("1E+999999"),
                Some(&parse("1")),
                &limits,
            )
            .unwrap();
        assert_eq!(
            result.value,
            Value::Finite {
                negative: false,
                coefficient: BigUint::from(10_u8).pow(27),
                exponent: BigInt::from(999972),
            }
        );
        assert!(result.signals.contains(Signal::Inexact));
    }
}
