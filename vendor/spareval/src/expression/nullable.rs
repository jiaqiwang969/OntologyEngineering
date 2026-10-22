//! Typed boundaries around the existing expression compiler and arithmetic.
//! This is a deliberately limited private capability, not another interpreter.
use super::{ExpressionTerm, NumericBinaryOperands};
use crate::tuple::{ErrorValue, SolutionValue, SparqlErrorKind};
use oxsdatatypes::{Decimal, Integer};
use std::rc::Rc;

#[derive(Debug)]
pub(crate) enum EvalFailure<F> {
    Sparql(ErrorValue),
    Fatal(F),
}

pub(crate) type RawEvaluation<T, F> = Result<SolutionValue<T>, EvalFailure<F>>;
pub(crate) type RawEvaluator<'a, I, T, F> = Rc<dyn Fn(&I) -> RawEvaluation<T, F> + 'a>;

pub(crate) fn raw_variable<T, F>(value: Option<SolutionValue<T>>) -> RawEvaluation<T, F> {
    value.ok_or_else(|| sparql(SparqlErrorKind::NotBound, "Variable is not bound"))
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn source_not_bound<F>() -> EvalFailure<F> {
    use crate::tuple::{SourceException, SourceExceptionArgument, SourceExceptionClass};
    EvalFailure::Sparql(ErrorValue::with_source(
        SparqlErrorKind::NotBound,
        "Variable is not bound",
        SourceException {
            class: SourceExceptionClass::NotBound,
            argument: SourceExceptionArgument::None,
        },
    ))
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(super) fn source_raw_variable<T, F>(value: Option<SolutionValue<T>>) -> RawEvaluation<T, F> {
    // parserutils.value raises NotBoundError(None) for a syntactic operand.
    value.ok_or_else(source_not_bound)
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(super) fn source_evaluated_variable<T, F>(
    value: Option<SolutionValue<T>>,
    variable: &str,
) -> RawEvaluation<T, F> {
    use crate::tuple::{SourceException, SourceExceptionArgument, SourceExceptionClass};
    // Bare _eval uses a different actual constructor from parserutils.value.
    value.ok_or_else(|| EvalFailure::Sparql(ErrorValue::with_source(
        SparqlErrorKind::NotBound,
        "Variable is not bound",
        SourceException {
            class: SourceExceptionClass::NotBound,
            argument: SourceExceptionArgument::Text(format!("Variable {variable} is not bound")),
        },
    )))
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(super) fn source_variable_operand<T, F>(value: SolutionValue<T>) -> RawEvaluation<T, F> {
    match value {
        SolutionValue::ExplicitNone => Err(source_not_bound()),
        SolutionValue::ReturnedError(error) => Err(EvalFailure::Sparql(error)),
        value => Ok(value),
    }
}

// parserutils.value treats variables differently from Expr children. This
// adapter is used only for a syntactic variable operand, never every child.
pub(super) fn variable_operand<T, F>(value: SolutionValue<T>) -> RawEvaluation<T, F> {
    match value {
        SolutionValue::ExplicitNone => {
            Err(sparql(SparqlErrorKind::NotBound, "Variable is not bound"))
        }
        SolutionValue::ReturnedError(error) => Err(EvalFailure::Sparql(error)),
        value => Ok(value),
    }
}

pub(super) fn capture_sparql<T, F>(result: RawEvaluation<T, F>) -> RawEvaluation<T, F> {
    match result {
        Err(EvalFailure::Sparql(error)) => Ok(SolutionValue::ReturnedError(error)),
        other => other,
    }
}

fn sparql<F>(kind: SparqlErrorKind, diagnostic: &str) -> EvalFailure<F> {
    EvalFailure::Sparql(ErrorValue::with_kind(kind, diagnostic))
}

#[derive(Clone, Copy)]
pub(super) enum ArithmeticOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}
#[derive(Clone, Copy)]
pub(super) enum UnaryArithmeticOp {
    Plus,
    Minus,
}

#[derive(Clone, Copy)]
pub(super) enum ArithmeticChainKind {
    Additive,
    Multiplicative,
}

// Certification comes from the compiler, never the RDF datatype alone. A
// Decimal produced by the bounded arithmetic below has an integral value and
// no external lexical scale to preserve. A leaf or variable has no such proof.
pub(super) struct ArithmeticOperand {
    value: SolutionValue<ExpressionTerm>,
    numeric_result: bool,
}

impl ArithmeticOperand {
    pub(super) fn new(value: SolutionValue<ExpressionTerm>, numeric_result: bool) -> Self {
        Self {
            value,
            numeric_result,
        }
    }
}

impl From<SolutionValue<ExpressionTerm>> for ArithmeticOperand {
    fn from(value: SolutionValue<ExpressionTerm>) -> Self {
        Self::new(value, false)
    }
}

struct BoundedNumber {
    value: Integer,
    decimal: bool,
}

#[derive(Debug)]
pub(super) enum ArithmeticFailure {
    InvalidOperands,
    Range,
}

impl ArithmeticOp {
    pub(super) fn evaluate(
        self,
        a: ExpressionTerm,
        b: ExpressionTerm,
    ) -> Result<ExpressionTerm, ArithmeticFailure> {
        match self {
            Self::Add => add(a, b),
            Self::Subtract => subtract(a, b),
            Self::Multiply => multiply(a, b),
            Self::Divide => divide(a, b),
        }
    }
}
impl UnaryArithmeticOp {
    pub(super) fn evaluate(
        self,
        value: ExpressionTerm,
    ) -> Result<ExpressionTerm, ArithmeticFailure> {
        match self {
            Self::Plus => plus(value),
            Self::Minus => minus(value),
        }
    }
}

fn bounded_operand<F>(
    operand: ArithmeticOperand,
    unsupported: fn(&'static str) -> F,
) -> Result<BoundedNumber, EvalFailure<F>> {
    match operand.value {
        SolutionValue::Rdf(ExpressionTerm::IntegerLiteral(value)) => Ok(BoundedNumber {
            value,
            decimal: false,
        }),
        SolutionValue::Rdf(ExpressionTerm::DecimalLiteral(value)) if operand.numeric_result => {
            let integral = Integer::try_from(value)
                .ok()
                .filter(|i| Decimal::from(*i) == value);
            integral
                .map(|value| BoundedNumber {
                    value,
                    decimal: true,
                })
                .ok_or_else(|| {
                    EvalFailure::Fatal(unsupported(
                        "Arithmetic child is outside the proven integral Decimal range",
                    ))
                })
        }
        SolutionValue::ExplicitNone | SolutionValue::ReturnedError(_) => Err(sparql(
            SparqlErrorKind::Type,
            "Numeric operand is not a literal",
        )),
        SolutionValue::Rdf(
            ExpressionTerm::NamedNode(_)
            | ExpressionTerm::BlankNode(_)
            | ExpressionTerm::StringLiteral(_)
            | ExpressionTerm::LangStringLiteral { .. }
            | ExpressionTerm::BooleanLiteral(_),
        ) => Err(sparql(
            SparqlErrorKind::Type,
            "Operand does not have a numeric datatype",
        )),
        // External Decimal/float promotion, dates, invalid numeric lexical forms and
        // other datatypes need their own original evidence. Never guess from
        // the legacy evaluator's None, and never turn this limit into SPARQL.
        _ => Err(EvalFailure::Fatal(unsupported(
            "Private nullable arithmetic currently accepts integer operands only",
        ))),
    }
}

impl BoundedNumber {
    fn finish<F>(self) -> RawEvaluation<ExpressionTerm, F> {
        Ok(SolutionValue::Rdf(if self.decimal {
            ExpressionTerm::DecimalLiteral(Decimal::from(self.value))
        } else {
            ExpressionTerm::IntegerLiteral(self.value)
        }))
    }

    fn apply<F>(
        self,
        op: ArithmeticOp,
        rhs: Self,
        unsupported: fn(&'static str) -> F,
    ) -> Result<Self, EvalFailure<F>> {
        let multiplicative = matches!(op, ArithmeticOp::Multiply | ArithmeticOp::Divide);
        if matches!(op, ArithmeticOp::Divide) {
            let divisor = i64::from(rhs.value);
            if divisor == 0 {
                return Err(sparql(SparqlErrorKind::OtherSparql, "divide by 0"));
            }
            // Python Decimal rounding is not provided by this bounded state.
            // i64::MIN / -1 also lies outside its proven range.
            if i64::from(self.value).checked_rem(divisor) != Some(0) {
                return Err(EvalFailure::Fatal(unsupported(
                    "Private nullable division currently requires an exact integral quotient",
                )));
            }
        }
        let negative_product = self.value.is_negative() != rhs.value.is_negative();
        let decimal = self.decimal || rhs.decimal || multiplicative;
        // Reuse the same checked kernels as the legacy evaluator. Retain the
        // integer accumulator between operations rather than materializing a
        // Decimal RDF value and revalidating it as an external operand.
        let result = typed_kernel_result(
            op.evaluate(
                ExpressionTerm::IntegerLiteral(self.value),
                ExpressionTerm::IntegerLiteral(rhs.value),
            ),
            unsupported,
        )?;
        let mut result = bounded_operand(ArithmeticOperand::new(result, true), unsupported)?;
        if multiplicative && result.value == Integer::from(0) && negative_product {
            return Err(EvalFailure::Fatal(unsupported(
                "Private nullable arithmetic does not yet preserve Decimal negative zero",
            )));
        }
        result.decimal = decimal;
        Ok(result)
    }
}

fn typed_kernel_result<F>(
    result: Result<ExpressionTerm, ArithmeticFailure>,
    unsupported: fn(&'static str) -> F,
) -> RawEvaluation<ExpressionTerm, F> {
    match result {
        Ok(value) => Ok(SolutionValue::Rdf(value)),
        Err(ArithmeticFailure::InvalidOperands) => {
            Err(sparql(SparqlErrorKind::Type, "Invalid arithmetic operands"))
        }
        Err(ArithmeticFailure::Range) => Err(EvalFailure::Fatal(unsupported(
            "Arithmetic result exceeds the proven private nullable numeric range",
        ))),
    }
}

pub(super) fn typed_binary<F>(
    op: ArithmeticOp,
    a: impl Into<ArithmeticOperand>,
    b: impl Into<ArithmeticOperand>,
    unsupported: fn(&'static str) -> F,
) -> RawEvaluation<ExpressionTerm, F> {
    let a = bounded_operand(a.into(), unsupported)?;
    let b = bounded_operand(b.into(), unsupported)?;
    a.apply(op, b, unsupported)?.finish()
}

pub(super) fn typed_unary<F>(
    op: UnaryArithmeticOp,
    value: impl Into<ArithmeticOperand>,
    unsupported: fn(&'static str) -> F,
) -> RawEvaluation<ExpressionTerm, F> {
    let value = bounded_operand(value.into(), unsupported)?;
    let result = typed_kernel_result(
        op.evaluate(ExpressionTerm::IntegerLiteral(value.value)),
        unsupported,
    )?;
    let mut result = bounded_operand(ArithmeticOperand::new(result, true), unsupported)?;
    result.decimal = value.decimal;
    result.finish()
}

pub(super) fn typed_chain<F>(
    kind: ArithmeticChainKind,
    first: ArithmeticOperand,
    tail: Vec<(ArithmeticOp, ArithmeticOperand)>,
    unsupported: fn(&'static str) -> F,
) -> RawEvaluation<ExpressionTerm, F> {
    if tail.is_empty() {
        return Err(EvalFailure::Fatal(unsupported(
            "A source arithmetic chain must contain at least one operation",
        )));
    }
    let mut state = bounded_operand(first, unsupported)?;
    if matches!(kind, ArithmeticChainKind::Multiplicative) {
        state.decimal = true;
    }
    for (op, operand) in tail {
        if !matches!(
            (kind, op),
            (
                ArithmeticChainKind::Additive,
                ArithmeticOp::Add | ArithmeticOp::Subtract
            ) | (
                ArithmeticChainKind::Multiplicative,
                ArithmeticOp::Multiply | ArithmeticOp::Divide
            )
        ) {
            return Err(EvalFailure::Fatal(unsupported(
                "Arithmetic operator does not belong to its source chain",
            )));
        }
        // The compiler has already fetched the complete raw operand list;
        // numeric conversion still happens at each original operation.
        let operand = bounded_operand(operand, unsupported)?;
        state = state.apply(op, operand, unsupported)?;
    }
    state.finish()
}

// The kernels below were moved from the existing compiler branches. Legacy
// callers map these local failures back to None; typed callers retain them.

fn add(a: ExpressionTerm, b: ExpressionTerm) -> Result<ExpressionTerm, ArithmeticFailure> {
    Ok(
        match NumericBinaryOperands::new(a, b).ok_or(ArithmeticFailure::InvalidOperands)? {
            NumericBinaryOperands::Float(v1, v2) => ExpressionTerm::FloatLiteral(v1 + v2),
            NumericBinaryOperands::Double(v1, v2) => ExpressionTerm::DoubleLiteral(v1 + v2),
            NumericBinaryOperands::Integer(v1, v2) => {
                ExpressionTerm::IntegerLiteral(v1.checked_add(v2).ok_or(ArithmeticFailure::Range)?)
            }
            NumericBinaryOperands::Decimal(v1, v2) => {
                ExpressionTerm::DecimalLiteral(v1.checked_add(v2).ok_or(ArithmeticFailure::Range)?)
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::Duration(v1, v2) => {
                ExpressionTerm::DurationLiteral(v1.checked_add(v2).ok_or(ArithmeticFailure::Range)?)
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::YearMonthDuration(v1, v2) => {
                ExpressionTerm::YearMonthDurationLiteral(
                    v1.checked_add(v2).ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DayTimeDuration(v1, v2) => {
                ExpressionTerm::DayTimeDurationLiteral(
                    v1.checked_add(v2).ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTimeDuration(v1, v2) => ExpressionTerm::DateTimeLiteral(
                v1.checked_add_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTimeYearMonthDuration(v1, v2) => {
                ExpressionTerm::DateTimeLiteral(
                    v1.checked_add_year_month_duration(v2)
                        .ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTimeDayTimeDuration(v1, v2) => {
                ExpressionTerm::DateTimeLiteral(
                    v1.checked_add_day_time_duration(v2)
                        .ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateDuration(v1, v2) => ExpressionTerm::DateLiteral(
                v1.checked_add_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateYearMonthDuration(v1, v2) => ExpressionTerm::DateLiteral(
                v1.checked_add_year_month_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateDayTimeDuration(v1, v2) => ExpressionTerm::DateLiteral(
                v1.checked_add_day_time_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::TimeDuration(v1, v2) => ExpressionTerm::TimeLiteral(
                v1.checked_add_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::TimeDayTimeDuration(v1, v2) => ExpressionTerm::TimeLiteral(
                v1.checked_add_day_time_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTime(_, _)
            | NumericBinaryOperands::Time(_, _)
            | NumericBinaryOperands::Date(_, _) => return Err(ArithmeticFailure::InvalidOperands),
        },
    )
}

fn subtract(a: ExpressionTerm, b: ExpressionTerm) -> Result<ExpressionTerm, ArithmeticFailure> {
    Ok(
        match NumericBinaryOperands::new(a, b).ok_or(ArithmeticFailure::InvalidOperands)? {
            NumericBinaryOperands::Float(v1, v2) => ExpressionTerm::FloatLiteral(v1 - v2),
            NumericBinaryOperands::Double(v1, v2) => ExpressionTerm::DoubleLiteral(v1 - v2),
            NumericBinaryOperands::Integer(v1, v2) => {
                ExpressionTerm::IntegerLiteral(v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?)
            }
            NumericBinaryOperands::Decimal(v1, v2) => {
                ExpressionTerm::DecimalLiteral(v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?)
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTime(v1, v2) => ExpressionTerm::DayTimeDurationLiteral(
                v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::Date(v1, v2) => ExpressionTerm::DayTimeDurationLiteral(
                v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::Time(v1, v2) => ExpressionTerm::DayTimeDurationLiteral(
                v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::Duration(v1, v2) => {
                ExpressionTerm::DurationLiteral(v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?)
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::YearMonthDuration(v1, v2) => {
                ExpressionTerm::YearMonthDurationLiteral(
                    v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DayTimeDuration(v1, v2) => {
                ExpressionTerm::DayTimeDurationLiteral(
                    v1.checked_sub(v2).ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTimeDuration(v1, v2) => ExpressionTerm::DateTimeLiteral(
                v1.checked_sub_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTimeYearMonthDuration(v1, v2) => {
                ExpressionTerm::DateTimeLiteral(
                    v1.checked_sub_year_month_duration(v2)
                        .ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateTimeDayTimeDuration(v1, v2) => {
                ExpressionTerm::DateTimeLiteral(
                    v1.checked_sub_day_time_duration(v2)
                        .ok_or(ArithmeticFailure::Range)?,
                )
            }
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateDuration(v1, v2) => ExpressionTerm::DateLiteral(
                v1.checked_sub_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateYearMonthDuration(v1, v2) => ExpressionTerm::DateLiteral(
                v1.checked_sub_year_month_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::DateDayTimeDuration(v1, v2) => ExpressionTerm::DateLiteral(
                v1.checked_sub_day_time_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::TimeDuration(v1, v2) => ExpressionTerm::TimeLiteral(
                v1.checked_sub_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            #[cfg(feature = "sep-0002")]
            NumericBinaryOperands::TimeDayTimeDuration(v1, v2) => ExpressionTerm::TimeLiteral(
                v1.checked_sub_day_time_duration(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
        },
    )
}

fn multiply(a: ExpressionTerm, b: ExpressionTerm) -> Result<ExpressionTerm, ArithmeticFailure> {
    Ok(
        match NumericBinaryOperands::new(a, b).ok_or(ArithmeticFailure::InvalidOperands)? {
            NumericBinaryOperands::Float(v1, v2) => ExpressionTerm::FloatLiteral(v1 * v2),
            NumericBinaryOperands::Double(v1, v2) => ExpressionTerm::DoubleLiteral(v1 * v2),
            NumericBinaryOperands::Integer(v1, v2) => {
                ExpressionTerm::IntegerLiteral(v1.checked_mul(v2).ok_or(ArithmeticFailure::Range)?)
            }
            NumericBinaryOperands::Decimal(v1, v2) => {
                ExpressionTerm::DecimalLiteral(v1.checked_mul(v2).ok_or(ArithmeticFailure::Range)?)
            }
            #[cfg(feature = "sep-0002")]
            _ => return Err(ArithmeticFailure::InvalidOperands),
        },
    )
}

fn divide(a: ExpressionTerm, b: ExpressionTerm) -> Result<ExpressionTerm, ArithmeticFailure> {
    Ok(
        match NumericBinaryOperands::new(a, b).ok_or(ArithmeticFailure::InvalidOperands)? {
            NumericBinaryOperands::Float(v1, v2) => ExpressionTerm::FloatLiteral(v1 / v2),
            NumericBinaryOperands::Double(v1, v2) => ExpressionTerm::DoubleLiteral(v1 / v2),
            NumericBinaryOperands::Integer(v1, v2) => ExpressionTerm::DecimalLiteral(
                Decimal::from(v1)
                    .checked_div(v2)
                    .ok_or(ArithmeticFailure::Range)?,
            ),
            NumericBinaryOperands::Decimal(v1, v2) => {
                ExpressionTerm::DecimalLiteral(v1.checked_div(v2).ok_or(ArithmeticFailure::Range)?)
            }
            #[cfg(feature = "sep-0002")]
            _ => return Err(ArithmeticFailure::InvalidOperands),
        },
    )
}

fn plus(value: ExpressionTerm) -> Result<ExpressionTerm, ArithmeticFailure> {
    Ok(match value {
        ExpressionTerm::FloatLiteral(value) => ExpressionTerm::FloatLiteral(value),
        ExpressionTerm::DoubleLiteral(value) => ExpressionTerm::DoubleLiteral(value),
        ExpressionTerm::IntegerLiteral(value) => ExpressionTerm::IntegerLiteral(value),
        ExpressionTerm::DecimalLiteral(value) => ExpressionTerm::DecimalLiteral(value),
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DurationLiteral(value) => ExpressionTerm::DurationLiteral(value),
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::YearMonthDurationLiteral(value) => {
            ExpressionTerm::YearMonthDurationLiteral(value)
        }
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DayTimeDurationLiteral(value) => {
            ExpressionTerm::DayTimeDurationLiteral(value)
        }
        _ => return Err(ArithmeticFailure::InvalidOperands),
    })
}

fn minus(value: ExpressionTerm) -> Result<ExpressionTerm, ArithmeticFailure> {
    Ok(match value {
        ExpressionTerm::FloatLiteral(value) => ExpressionTerm::FloatLiteral(-value),
        ExpressionTerm::DoubleLiteral(value) => ExpressionTerm::DoubleLiteral(-value),
        ExpressionTerm::IntegerLiteral(value) => {
            ExpressionTerm::IntegerLiteral(value.checked_neg().ok_or(ArithmeticFailure::Range)?)
        }
        ExpressionTerm::DecimalLiteral(value) => {
            ExpressionTerm::DecimalLiteral(value.checked_neg().ok_or(ArithmeticFailure::Range)?)
        }
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DurationLiteral(value) => {
            ExpressionTerm::DurationLiteral(value.checked_neg().ok_or(ArithmeticFailure::Range)?)
        }
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::YearMonthDurationLiteral(value) => {
            ExpressionTerm::YearMonthDurationLiteral(
                value.checked_neg().ok_or(ArithmeticFailure::Range)?,
            )
        }
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DayTimeDurationLiteral(value) => ExpressionTerm::DayTimeDurationLiteral(
            value.checked_neg().ok_or(ArithmeticFailure::Range)?,
        ),
        _ => return Err(ArithmeticFailure::InvalidOperands),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integer(value: i64) -> SolutionValue<ExpressionTerm> {
        SolutionValue::Rdf(ExpressionTerm::IntegerLiteral(value.into()))
    }

    fn unsupported(message: &'static str) -> &'static str {
        message
    }

    #[test]
    fn raw_none_and_operand_none_have_distinct_outcomes() {
        let raw = raw_variable::<ExpressionTerm, ()>(Some(SolutionValue::ExplicitNone)).unwrap();
        assert!(matches!(raw, SolutionValue::ExplicitNone));
        let Err(EvalFailure::Sparql(error)) = variable_operand::<_, ()>(raw) else {
            panic!("operand must raise")
        };
        assert_eq!(error.kind(), SparqlErrorKind::NotBound);
        let Ok(SolutionValue::ReturnedError(captured)) =
            capture_sparql::<ExpressionTerm, ()>(Err(EvalFailure::Sparql(error.clone())))
        else {
            panic!("Expr must return the error")
        };
        assert_eq!(captured, error);
    }

    #[test]
    fn repeated_unbound_evaluation_allocates_distinct_errors() {
        let Err(EvalFailure::Sparql(first)) = raw_variable::<ExpressionTerm, ()>(None) else {
            panic!("missing variable must raise")
        };
        let Err(EvalFailure::Sparql(second)) = raw_variable::<ExpressionTerm, ()>(None) else {
            panic!("missing variable must raise")
        };
        assert_eq!(first.kind(), SparqlErrorKind::NotBound);
        assert_ne!(first, second);
    }

    #[test]
    fn variable_error_is_raised_unchanged_but_expr_error_is_a_numeric_operand() {
        let stored = ErrorValue::with_kind(SparqlErrorKind::NotBound, "stored missing value");
        let Err(EvalFailure::Sparql(raised)) =
            variable_operand::<ExpressionTerm, ()>(SolutionValue::ReturnedError(stored.clone()))
        else {
            panic!("variable must raise the stored error")
        };
        assert_eq!(raised, stored);
        let Err(EvalFailure::Sparql(numeric)) = typed_binary(
            ArithmeticOp::Add,
            SolutionValue::ReturnedError(stored.clone()),
            integer(1),
            unsupported,
        ) else {
            panic!("Expr child error reaches numeric validation")
        };
        assert_eq!(numeric.kind(), SparqlErrorKind::Type);
        assert_ne!(numeric, stored);
    }

    #[test]
    fn typed_multiplication_preserves_python_decimal_result_and_legacy_integer() {
        let legacy = ArithmeticOp::Multiply
            .evaluate(
                ExpressionTerm::IntegerLiteral(2.into()),
                ExpressionTerm::IntegerLiteral(3.into()),
            )
            .unwrap();
        assert!(matches!(legacy, ExpressionTerm::IntegerLiteral(value) if value == 6.into()));
        let Ok(SolutionValue::Rdf(ExpressionTerm::DecimalLiteral(value))) =
            typed_binary(ArithmeticOp::Multiply, integer(2), integer(3), unsupported)
        else {
            panic!("typed integer multiplication returns Decimal")
        };
        assert_eq!(value, Decimal::from(6));
    }

    #[test]
    fn division_by_zero_returns_fresh_sparql_errors() {
        let evaluate = || {
            capture_sparql(typed_binary(
                ArithmeticOp::Divide,
                integer(1),
                integer(0),
                unsupported,
            ))
        };
        let Ok(SolutionValue::ReturnedError(first)) = evaluate() else {
            panic!("Expr catches divide by zero")
        };
        let Ok(SolutionValue::ReturnedError(second)) = evaluate() else {
            panic!("Expr catches divide by zero")
        };
        assert_eq!(first.kind(), SparqlErrorKind::OtherSparql);
        assert_ne!(first, second);
    }

    #[test]
    fn unsupported_range_and_rounding_remain_fatal() {
        let overflow = typed_binary(
            ArithmeticOp::Add,
            integer(i64::MAX),
            integer(1),
            unsupported,
        );
        assert!(matches!(
            capture_sparql(overflow),
            Err(EvalFailure::Fatal(_))
        ));
        let fractional = typed_binary(ArithmeticOp::Divide, integer(1), integer(2), unsupported);
        assert!(matches!(
            capture_sparql(fractional),
            Err(EvalFailure::Fatal(_))
        ));
        let explicit =
            capture_sparql::<ExpressionTerm, _>(Err(EvalFailure::Fatal("dataset failed")));
        assert!(matches!(
            explicit,
            Err(EvalFailure::Fatal("dataset failed"))
        ));
        let Ok(SolutionValue::Rdf(ExpressionTerm::DecimalLiteral(value))) =
            typed_binary(ArithmeticOp::Divide, integer(4), integer(2), unsupported)
        else {
            panic!("exact integral quotient is supported")
        };
        assert_eq!(value, Decimal::from(2));
    }

    #[test]
    fn multiplicative_chain_retains_state_until_the_final_decimal() {
        let result = typed_chain(
            ArithmeticChainKind::Multiplicative,
            integer(2).into(),
            vec![
                (ArithmeticOp::Multiply, integer(3).into()),
                (ArithmeticOp::Multiply, integer(4).into()),
                (ArithmeticOp::Divide, integer(6).into()),
            ],
            unsupported,
        );
        let Ok(SolutionValue::Rdf(ExpressionTerm::DecimalLiteral(value))) = result else {
            panic!("bounded multiplication and exact division must share one state")
        };
        assert_eq!(value, Decimal::from(4));
    }

    #[test]
    fn additive_chain_retains_left_to_right_subtraction() {
        let result = typed_chain(
            ArithmeticChainKind::Additive,
            integer(10).into(),
            vec![
                (ArithmeticOp::Subtract, integer(3).into()),
                (ArithmeticOp::Subtract, integer(2).into()),
            ],
            unsupported,
        );
        assert!(matches!(
            result,
            Ok(SolutionValue::Rdf(ExpressionTerm::IntegerLiteral(value))) if value == 5.into()
        ));
    }

    #[test]
    fn arithmetic_child_decimal_is_accepted_without_accepting_external_decimal() {
        let child = typed_chain(
            ArithmeticChainKind::Multiplicative,
            integer(3).into(),
            vec![(ArithmeticOp::Multiply, integer(4).into())],
            unsupported,
        )
        .unwrap();
        let result = typed_chain(
            ArithmeticChainKind::Additive,
            integer(2).into(),
            vec![(ArithmeticOp::Add, ArithmeticOperand::new(child, true))],
            unsupported,
        );
        let Ok(SolutionValue::Rdf(ExpressionTerm::DecimalLiteral(value))) = result else {
            panic!("a proven arithmetic child retains Decimal promotion")
        };
        assert_eq!(value, Decimal::from(14));

        let external = SolutionValue::Rdf(ExpressionTerm::DecimalLiteral(Decimal::from(12)));
        assert!(matches!(
            typed_binary(ArithmeticOp::Add, integer(2), external, unsupported),
            Err(EvalFailure::Fatal(_))
        ));
        let fractional = SolutionValue::Rdf(ExpressionTerm::DecimalLiteral("1.5".parse().unwrap()));
        assert!(matches!(
            typed_unary(
                UnaryArithmeticOp::Plus,
                ArithmeticOperand::new(fractional, true),
                unsupported,
            ),
            Err(EvalFailure::Fatal(_))
        ));
    }

    #[test]
    fn flat_chain_stops_math_before_validating_later_numeric_operands() {
        let child_error = ErrorValue::with_kind(SparqlErrorKind::NotBound, "child error");
        let result = capture_sparql(typed_chain(
            ArithmeticChainKind::Multiplicative,
            integer(1).into(),
            vec![
                (ArithmeticOp::Divide, integer(0).into()),
                (
                    ArithmeticOp::Multiply,
                    ArithmeticOperand::new(SolutionValue::ReturnedError(child_error), true),
                ),
            ],
            unsupported,
        ));
        let Ok(SolutionValue::ReturnedError(error)) = result else {
            panic!("the first divide by zero is the chain's error")
        };
        assert_eq!(error.kind(), SparqlErrorKind::OtherSparql);
    }

    #[test]
    fn nested_chain_error_becomes_a_new_outer_type_error() {
        let inner = capture_sparql(typed_chain(
            ArithmeticChainKind::Multiplicative,
            integer(1).into(),
            vec![(ArithmeticOp::Divide, integer(0).into())],
            unsupported,
        ))
        .unwrap();
        let SolutionValue::ReturnedError(inner_error) = &inner else {
            panic!("inner chain must return its divide error")
        };
        let inner_error = inner_error.clone();
        let outer = capture_sparql(typed_chain(
            ArithmeticChainKind::Multiplicative,
            ArithmeticOperand::new(inner, true),
            vec![(ArithmeticOp::Multiply, integer(1).into())],
            unsupported,
        ));
        let Ok(SolutionValue::ReturnedError(outer_error)) = outer else {
            panic!("outer chain must reject the returned error as numeric")
        };
        assert_eq!(outer_error.kind(), SparqlErrorKind::Type);
        assert_ne!(inner_error, outer_error);
    }

    #[test]
    fn chain_fractional_intermediate_and_range_remain_fatal() {
        for (first, tail) in [
            (
                1,
                vec![
                    (ArithmeticOp::Divide, integer(2).into()),
                    (ArithmeticOp::Multiply, integer(2).into()),
                ],
            ),
            (i64::MAX, vec![(ArithmeticOp::Multiply, integer(2).into())]),
        ] {
            assert!(matches!(
                capture_sparql(typed_chain(
                    ArithmeticChainKind::Multiplicative,
                    integer(first).into(),
                    tail,
                    unsupported,
                )),
                Err(EvalFailure::Fatal(_))
            ));
        }
    }

    #[test]
    fn signed_decimal_zero_is_refused_at_the_multiplicative_operation() {
        // Source-derived controls: Decimal(0) * -1, Decimal(0) / -1 and
        // Decimal(-1) * 0 produce -0. The current carrier cannot retain it.
        for (first, op, last) in [
            (0, ArithmeticOp::Multiply, -1),
            (0, ArithmeticOp::Divide, -1),
            (-1, ArithmeticOp::Multiply, 0),
        ] {
            assert!(matches!(
                capture_sparql(typed_chain(
                    ArithmeticChainKind::Multiplicative,
                    integer(first).into(),
                    vec![(op, integer(last).into())],
                    unsupported,
                )),
                Err(EvalFailure::Fatal(
                    "Private nullable arithmetic does not yet preserve Decimal negative zero"
                ))
            ));
        }
        let positive_zero = typed_chain(
            ArithmeticChainKind::Multiplicative,
            integer(0).into(),
            vec![(ArithmeticOp::Multiply, integer(1).into())],
            unsupported,
        );
        assert!(matches!(
            positive_zero,
            Ok(SolutionValue::Rdf(ExpressionTerm::DecimalLiteral(value))) if value == Decimal::from(0)
        ));
    }
}
