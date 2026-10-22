//! Opt-in calendar Duration cache in the existing source query session.
//! Constructor origin and Decimal context bind immutable cached values; no
//! global cache, RDF re-interpretation, or alternate expression evaluator.
use super::source_exception::{
    LanguageExceptionArgument, SourceLanguageException, SourceLanguageExceptionKind,
};
use super::*;
use oxrdf::vocab::xsd;
use semantica_temporal::{DurationContext, DurationError, DurationValue};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceDurationCache {
    pub(crate) value: Option<DurationValue>,
    origin: String,
    normalize: bool,
    decimal: DecimalContext,
}
pub(super) fn is_duration(datatype: &NamedNode) -> bool {
    matches!(
        datatype.as_str(),
        "http://www.w3.org/2001/XMLSchema#duration"
            | "http://www.w3.org/2001/XMLSchema#yearMonthDuration"
            | "http://www.w3.org/2001/XMLSchema#dayTimeDuration"
    )
}
fn parsed(
    context: &DurationContext<'_>,
    text: &str,
) -> Result<Option<DurationValue>, RawNumericFault> {
    match context.parse(text) {
        Ok(v) => Ok(v),
        Err(e) => match e.downcast_ref::<NumericFault>() {
            Some(NumericFault::DecimalTrap { .. } | NumericFault::InvalidLexical) => Ok(None),
            Some(e) => Err(RawNumericFault::Core(e.clone())),
            None if matches!(
                e.downcast_ref::<DurationError>(),
                Some(DurationError::Value(_) | DurationError::Overflow(_))
            ) =>
            {
                Ok(None)
            }
            None => Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "unreceived calendar duration parse failure",
            )),
        },
    }
}
pub(super) fn validate(
    raw: &RawExpressionValue,
    metadata: &SourceLiteralMetadata,
    cache: &SourceDurationCache,
) -> Result<(), RawNumericFault> {
    let (Term::Literal(literal), SourceDatatype::Specified(datatype)) =
        (raw.rdf()?, &metadata.datatype)
    else {
        return Err(RawNumericFault::InvalidSourceOrigin);
    };
    if !is_duration(datatype)
        || metadata.language_original != SourceFact::Known(None)
        || !matches!(raw.numeric(), NumericCacheState::NotNumeric)
    {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    let context = DurationContext::new(&cache.decimal);
    if parsed(&context, &cache.origin)? != cache.value {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    let lexical = if cache.normalize {
        cache
            .value
            .as_ref()
            .map(|v| context.format(v))
            .transpose()
            .map_err(|_| RawNumericFault::InvalidSourceOrigin)?
            .unwrap_or_else(|| cache.origin.clone())
    } else {
        cache.origin.clone()
    };
    if lexical != literal.value()
        || (metadata.ill_typed != SourceFact::Known(None)
            && metadata.ill_typed != SourceFact::Known(Some(cache.value.is_none())))
    {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    Ok(())
}
impl QueryNumericSession {
    pub(crate) fn with_source_calendar_duration_v1(mut self) -> Result<Self, RawNumericFault> {
        self.require_source_literal_semantics()?;
        if self.context_semantics() != QueryContextSemantics::SourceContextV1 {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        self.source_calendar_duration_v1 = true;
        Ok(self)
    }
    fn calendar_fault(&self, error: anyhow::Error) -> RawNumericFault {
        if let Some(e) = error.downcast_ref::<NumericFault>() {
            return self.source_arithmetic_fault(e.clone());
        }
        match error.downcast_ref::<DurationError>() {
            Some(DurationError::Value(message)) => RawNumericFault::SourceLanguageException(
                SourceLanguageException::text(SourceLanguageExceptionKind::ValueError, *message),
            ),
            Some(DurationError::YearRange(_)) => {
                RawNumericFault::SourceLanguageException(SourceLanguageException::text(
                    SourceLanguageExceptionKind::ValueError,
                    error.to_string(),
                ))
            }
            Some(DurationError::Overflow(message)) => RawNumericFault::SourceLanguageException(
                SourceLanguageException::text(SourceLanguageExceptionKind::OverflowError, *message),
            ),
            Some(DurationError::DivisionImpossible) => {
                RawNumericFault::SourceLanguageException(SourceLanguageException {
                    kind: SourceLanguageExceptionKind::Decimal(DecimalSignal::InvalidOperation),
                    arguments: vec![LanguageExceptionArgument::DecimalDivisionImpossible],
                })
            }
            None => RawNumericFault::UnsupportedSourceLiteralOperator(
                "unreceived calendar duration operation failure",
            ),
        }
    }
    pub(super) fn construct_source_duration(
        &self,
        text: &str,
        datatype: NamedNode,
        normalize: bool,
        direct: bool,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        if !self.source_calendar_duration_v1 || !is_duration(&datatype) {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "calendar Duration values are not enabled",
            ));
        }
        self.check_text(text)?;
        self.check_text(datatype.as_str())?;
        if text.len() > 10_000 {
            return Err(NumericFault::ResourceLimit {
                operation: "source_duration_input_bytes",
            }
            .into());
        }
        let context = DurationContext::new(self.decimal_context());
        let value = parsed(&context, text);
        self.flags.set(self.flags.get() | context.signals());
        let value = value?;
        let output = if normalize {
            let output = value.as_ref().map(|v| context.format(v)).transpose();
            self.flags.set(self.flags.get() | context.signals());
            output
                .map_err(|e| self.calendar_fault(e))?
                .unwrap_or_else(|| text.to_owned())
        } else {
            text.to_owned()
        };
        if output.chars().count() > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit {
                operation: "output_characters",
            }
            .into());
        }
        let ill = if direct { Some(value.is_none()) } else { None };
        RawExpressionValue::from_parts(
            Literal::new_typed_literal(output, datatype.clone()).into(),
            NumericCacheState::NotNumeric,
        )
        .with_source_metadata(SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(datatype),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(ill),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Duration(Arc::new(
                SourceDurationCache {
                    value,
                    origin: text.into(),
                    normalize,
                    decimal: self.decimal.clone(),
                },
            ))),
        })
    }
    pub(super) fn calendar_values_equal(
        &self,
        a: &DurationValue,
        b: &DurationValue,
    ) -> Result<bool, RawNumericFault> {
        let context = DurationContext::new(self.decimal_context());
        let result = context.value_eq(a, b);
        self.flags.set(self.flags.get() | context.signals());
        result.map_err(|e| self.calendar_fault(e))
    }
}

fn temporal_datatype(value: &SolutionValue<RawExpressionValue>) -> Option<&NamedNode> {
    let SolutionValue::Rdf(raw) = value else {
        return None;
    };
    let LiteralSourceState::Literal(meta) = raw.literal_source() else {
        return None;
    };
    match &meta.datatype {
        SourceDatatype::Specified(dt) => Some(dt),
        _ => None,
    }
}
fn is_date(dt: &NamedNode) -> bool {
    matches!(
        dt.as_str(),
        "http://www.w3.org/2001/XMLSchema#date"
            | "http://www.w3.org/2001/XMLSchema#dateTime"
            | "http://www.w3.org/2001/XMLSchema#time"
    )
}
fn datetime_error(message: &'static str) -> NumericEvaluationError {
    NumericEvaluationError::Sparql {
        kind: SparqlErrorKind::OtherSparql,
        diagnostic: Cow::Borrowed(message),
        source: Some(SourceException {
            class: SourceExceptionClass::Sparql,
            argument: SourceExceptionArgument::Text(message.into()),
        }),
    }
}
impl QueryNumericSession {
    fn temporal_operand_datatype<'a>(
        &self,
        value: &'a SolutionValue<RawExpressionValue>,
    ) -> Result<Option<&'a NamedNode>, NumericEvaluationError> {
        self.require_source_literal_semantics()?;
        if let SolutionValue::Rdf(raw) = value {
            super::source_literal::validate_source_representation(raw)?;
            if let LiteralSourceState::Literal(metadata) = raw.literal_source() {
                return match &metadata.datatype {
                    SourceDatatype::Specified(datatype) => Ok(Some(datatype)),
                    SourceDatatype::Absent => Ok(None),
                    SourceDatatype::Unknown => {
                        Err(RawNumericFault::UnknownSourceLiteralIdentity.into())
                    }
                };
            }
        }
        // AdditiveExpression reads term.datatype directly. A returned error
        // or non-Literal operand must retain its actual source class.
        let class = super::source_function::source_type_name(value)?
            .rsplit('.')
            .next()
            .ok_or(RawNumericFault::InvalidSourceOrigin)?;
        Err(
            RawNumericFault::SourceLanguageException(SourceLanguageException::text(
                SourceLanguageExceptionKind::AttributeError,
                format!("'{class}' object has no attribute 'datatype'"),
            ))
            .into(),
        )
    }
    fn temporal_language_error(
        kind: SourceLanguageExceptionKind,
        message: &'static str,
    ) -> NumericEvaluationError {
        RawNumericFault::SourceLanguageException(SourceLanguageException::text(kind, message))
            .into()
    }
    fn temporal_difference(
        &self,
        first: &SolutionValue<RawExpressionValue>,
        rhs: &SolutionValue<RawExpressionValue>,
    ) -> Result<SolutionValue<RawExpressionValue>, NumericEvaluationError> {
        self.require_source_literal_semantics()?;
        let (SolutionValue::Rdf(left), SolutionValue::Rdf(right)) = (first, rhs) else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        super::source_literal::validate_source_representation(left)?;
        super::source_literal::validate_source_representation(right)?;
        let (LiteralSourceState::Literal(left), LiteralSourceState::Literal(right)) =
            (left.literal_source(), right.literal_source())
        else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        let (SourceFact::Known(left), SourceFact::Known(right)) =
            (&left.cached_value_kind, &right.cached_value_kind)
        else {
            return Err(RawNumericFault::UnknownNumericCache.into());
        };
        use SourceLanguageExceptionKind as Error;
        use SourceLiteralCacheKind as Cache;
        let delta = match (left, right) {
            (Cache::DateTime(Some(a)), Cache::DateTime(Some(b))) => {
                semantica_temporal::datetime_difference(*a, *b).ok_or_else(|| {
                    Self::temporal_language_error(
                        Error::TypeError,
                        "can't subtract offset-naive and offset-aware datetimes",
                    )
                })?
            }
            (Cache::Date(Some(a)), Cache::Date(Some(b))) => {
                semantica_temporal::date_difference(*a, *b)
            }
            (Cache::DateTime(None), Cache::DateTime(_)) | (Cache::Date(None), Cache::Date(_)) => {
                // The invalid left Literal calls its own __sub__ again. Receive
                // the actual pinned original terminal error, without recursing
                // the Rust stack or treating it as cancellation/resource failure.
                return Err(Self::temporal_language_error(
                    Error::RecursionError,
                    "maximum recursion depth exceeded",
                ));
            }
            (Cache::DateTime(Some(_)), Cache::DateTime(None)) => {
                return Err(Self::temporal_language_error(
                    Error::TypeError,
                    "unsupported operand type(s) for -: 'datetime.datetime' and 'Literal'",
                ));
            }
            (Cache::Date(Some(_)), Cache::Date(None)) => {
                return Err(Self::temporal_language_error(
                    Error::TypeError,
                    "unsupported operand type(s) for -: 'datetime.date' and 'Literal'",
                ));
            }
            (Cache::Time(None), Cache::Time(_)) => {
                return Err(Self::temporal_language_error(
                    Error::TypeError,
                    "combine() argument 2 must be datetime.time, not Literal",
                ));
            }
            (Cache::Time(Some(_)), Cache::Time(None)) => {
                return Err(Self::temporal_language_error(
                    Error::TypeError,
                    "unsupported operand type(s) for -: 'datetime.time' and 'Literal'",
                ));
            }
            (Cache::Time(Some(_)), Cache::Time(Some(_))) => {
                return Err(Self::temporal_language_error(
                    Error::TypeError,
                    "unsupported operand type(s) for -: 'datetime.time' and 'datetime.time'",
                ));
            }
            _ => {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "unreceived temporal difference cache",
                )
                .into());
            }
        };
        // Original Literal(timedelta, datatype=duration) preserves the exact
        // value and ill_typed=None. Do not introduce Decimal context operations
        // by reconstructing the result as a caller string literal.
        let text = delta.isoformat();
        if text.chars().count() > self.limits.max_output_characters {
            return Err(RawNumericFault::Core(NumericFault::ResourceLimit {
                operation: "output_characters",
            })
            .into());
        }
        RawExpressionValue::from_parts(
            Literal::new_typed_literal(text.clone(), xsd::DURATION).into(),
            NumericCacheState::NotNumeric,
        )
        .with_source_metadata(SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(xsd::DURATION.into_owned()),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(None),
            cached_value_kind: SourceFact::Known(Cache::Duration(Arc::new(SourceDurationCache {
                value: Some(DurationValue::Fixed(delta)),
                origin: text,
                normalize: false,
                decimal: self.decimal.clone(),
            }))),
        })
        .map(SolutionValue::Rdf)
        .map_err(Into::into)
    }
    fn time_duration_operation(
        &self,
        first: &SolutionValue<RawExpressionValue>,
        rhs: &SolutionValue<RawExpressionValue>,
        subtract: bool,
    ) -> Result<SolutionValue<RawExpressionValue>, NumericEvaluationError> {
        // operators.isCompatibleDateTimeDatatype checks str(value)[1] only
        // for dayTimeDuration (the distinct capitalized XSD.Duration typo is
        // not in the admitted duration datatype set). Keep that source rule.
        let SolutionValue::Rdf(right) = rhs else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        super::source_literal::validate_source_representation(right)?;
        let LiteralSourceState::Literal(meta) = right.literal_source() else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        let SourceFact::Known(SourceLiteralCacheKind::Duration(cache)) = &meta.cached_value_kind
        else {
            return Err(RawNumericFault::UnknownNumericCache.into());
        };
        if cache.value.is_some() {
            return Err(datetime_error(
                "Incompatible Data types to DateTime Operations",
            ));
        }
        let Term::Literal(literal) = right.rdf()? else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        match literal.value().chars().nth(1) {
            None => {
                return Err(Self::temporal_language_error(
                    SourceLanguageExceptionKind::IndexError,
                    "string index out of range",
                ));
            }
            Some('T') => (),
            _ => {
                return Err(datetime_error(
                    "Incompatible Data types to DateTime Operations",
                ));
            }
        }
        let SolutionValue::Rdf(left) = first else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        super::source_literal::validate_source_representation(left)?;
        let LiteralSourceState::Literal(meta) = left.literal_source() else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        match meta.cached_value_kind {
            SourceFact::Known(SourceLiteralCacheKind::Time(Some(_))) => (),
            SourceFact::Known(SourceLiteralCacheKind::Time(None)) => {
                return Err(Self::temporal_language_error(
                    SourceLanguageExceptionKind::TypeError,
                    "combine() argument 2 must be datetime.time, not Literal",
                ));
            }
            _ => return Err(RawNumericFault::UnknownNumericCache.into()),
        }
        Err(Self::temporal_language_error(
            SourceLanguageExceptionKind::TypeError,
            if subtract {
                "unsupported operand type(s) for -: 'datetime.time' and 'Literal'"
            } else {
                "unsupported operand type(s) for +: 'datetime.time' and 'Literal'"
            },
        ))
    }
    pub(crate) fn admits_source_calendar_duration(&self) -> bool {
        self.source_calendar_duration_v1
    }
    pub(crate) fn evaluate_chain_value(
        &self,
        kind: NumericChainKind,
        first: SolutionValue<RawExpressionValue>,
        tail: Vec<(NumericBinaryOp, SolutionValue<RawExpressionValue>)>,
    ) -> Result<SolutionValue<RawExpressionValue>, NumericEvaluationError> {
        if !self.source_calendar_duration_v1
            || kind != NumericChainKind::Additive
            || !temporal_datatype(&first).is_some_and(|dt| is_date(dt) || is_duration(dt))
        {
            return self
                .evaluate_chain(kind, first, tail)
                .map(SolutionValue::Rdf);
        }
        if tail.is_empty()
            || tail
                .iter()
                .any(|(op, _)| !matches!(op, NumericBinaryOp::Add | NumericBinaryOp::Subtract))
        {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        }
        let dt = temporal_datatype(&first).ok_or(RawNumericFault::InvalidSourceOrigin)?;
        for (op, rhs) in &tail {
            let Some(rhs_dt) = self.temporal_operand_datatype(rhs)? else {
                return Err(datetime_error("Invalid DateTime Operations"));
            };
            if is_date(dt) && dt == rhs_dt && *op == NumericBinaryOp::Subtract {
                if tail.len() > 1 {
                    // The original constructs this error by reading dt.datatype
                    // from an URIRef, before attempting the subtraction.
                    return Err(Self::temporal_language_error(
                        SourceLanguageExceptionKind::AttributeError,
                        "'URIRef' object has no attribute 'datatype'",
                    ));
                }
                return self.temporal_difference(&first, rhs);
            } else if is_date(dt) && is_duration(rhs_dt) {
                if dt == &xsd::TIME && rhs_dt == &xsd::DAY_TIME_DURATION {
                    return self.time_duration_operation(
                        &first,
                        rhs,
                        *op == NumericBinaryOp::Subtract,
                    );
                }
                if dt != &xsd::DATE_TIME
                    && !(dt == &xsd::DATE
                        && (rhs_dt == &xsd::YEAR_MONTH_DURATION
                            || rhs_dt == &xsd::DAY_TIME_DURATION))
                {
                    return Err(datetime_error(
                        "Incompatible Data types to DateTime Operations",
                    ));
                }
                let (SolutionValue::Rdf(left), SolutionValue::Rdf(right)) = (&first, rhs) else {
                    return Err(RawNumericFault::InvalidSourceOrigin.into());
                };
                super::source_literal::validate_source_representation(left)?;
                super::source_literal::validate_source_representation(right)?;
                let Term::Literal(right_literal) = right.rdf()? else {
                    return Err(RawNumericFault::InvalidSourceOrigin.into());
                };
                let (LiteralSourceState::Literal(left), LiteralSourceState::Literal(right)) =
                    (left.literal_source(), right.literal_source())
                else {
                    return Err(RawNumericFault::InvalidSourceOrigin.into());
                };
                let SourceFact::Known(SourceLiteralCacheKind::Duration(cache)) =
                    &right.cached_value_kind
                else {
                    return Err(RawNumericFault::UnknownNumericCache.into());
                };
                // The source tests str(toPython()) rather than the original
                // duration lexical form. Only failed conversion retains the
                // Literal text here; valid timedelta/Duration text has no T.
                if dt == &xsd::DATE
                    && rhs_dt == &xsd::DAY_TIME_DURATION
                    && cache.value.is_none()
                    && right_literal.value().contains('T')
                {
                    return Err(datetime_error(
                        "Incompatible Data types to DateTime Operations",
                    ));
                }
                if matches!(
                    left.cached_value_kind,
                    SourceFact::Known(
                        SourceLiteralCacheKind::Date(None) | SourceLiteralCacheKind::DateTime(None)
                    )
                ) {
                    // A converted timedelta/Duration is wrapped as a duration
                    // Literal by Literal.__add__/__sub__. A failed left cache
                    // then calls the same operator recursively. An unconverted
                    // yearMonthDuration is outside term._TIME_DELTA_TYPES and
                    // instead reaches its explicit datatype error.
                    if cache.value.is_none() && rhs_dt == &xsd::YEAR_MONTH_DURATION {
                        let left_type = dt.as_str();
                        let right_type = rhs_dt.as_str();
                        let message = if *op == NumericBinaryOp::Subtract {
                            format!(
                                "Cannot subtract a Literal of datatype {right_type} from a Literal of datatype {left_type}"
                            )
                        } else {
                            format!(
                                "Cannot add a Literal of datatype {right_type} to a Literal of datatype {left_type}"
                            )
                        };
                        return Err(RawNumericFault::SourceLanguageException(
                            SourceLanguageException::text(
                                SourceLanguageExceptionKind::TypeError,
                                message,
                            ),
                        )
                        .into());
                    }
                    return Err(Self::temporal_language_error(
                        SourceLanguageExceptionKind::RecursionError,
                        "maximum recursion depth exceeded",
                    ));
                }
                let Some(duration) = &cache.value else {
                    let class = match &left.cached_value_kind {
                        SourceFact::Known(SourceLiteralCacheKind::DateTime(Some(_))) => {
                            "datetime.datetime"
                        }
                        SourceFact::Known(SourceLiteralCacheKind::Date(Some(_))) => "datetime.date",
                        _ => {
                            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                                "unreceived temporal arithmetic cache",
                            )
                            .into());
                        }
                    };
                    let sign = if *op == NumericBinaryOp::Subtract {
                        "-"
                    } else {
                        "+"
                    };
                    return Err(RawNumericFault::SourceLanguageException(
                        SourceLanguageException::text(
                            SourceLanguageExceptionKind::TypeError,
                            format!(
                                "unsupported operand type(s) for {sign}: '{class}' and 'Literal'"
                            ),
                        ),
                    )
                    .into());
                };
                let context = DurationContext::new(self.decimal_context());
                let subtract = *op == NumericBinaryOp::Subtract;
                let result = match &left.cached_value_kind {
                    SourceFact::Known(SourceLiteralCacheKind::DateTime(Some(date))) => context
                        .apply_to_datetime(*date, duration, subtract)
                        .map(|v| (v.isoformat(), SourceLiteralCacheKind::DateTime(Some(v)))),
                    SourceFact::Known(SourceLiteralCacheKind::Date(Some(date))) => context
                        .apply_to_date(*date, duration, subtract)
                        .map(|v| (v.isoformat(), SourceLiteralCacheKind::Date(Some(v)))),
                    _ => {
                        return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                            "unreceived temporal arithmetic cache",
                        )
                        .into());
                    }
                };
                self.flags.set(self.flags.get() | context.signals());
                let (text, cache) = result.map_err(|e| self.calendar_fault(e))?;
                if text.chars().count() > self.limits.max_output_characters {
                    return Err(RawNumericFault::Core(NumericFault::ResourceLimit {
                        operation: "output_characters",
                    })
                    .into());
                }
                return RawExpressionValue::from_parts(
                    Literal::new_typed_literal(text, dt.clone()).into(),
                    NumericCacheState::NotNumeric,
                )
                .with_source_metadata(SourceLiteralMetadata {
                    datatype: SourceDatatype::Specified(dt.clone()),
                    language_original: SourceFact::Known(None),
                    ill_typed: SourceFact::Known(None),
                    cached_value_kind: SourceFact::Known(cache),
                })
                .map(SolutionValue::Rdf)
                .map_err(Into::into);
            } else if is_duration(dt) && is_date(rhs_dt) {
                if *op == NumericBinaryOp::Add {
                    return Err(datetime_error(
                        "Incompatible Data types to DateTime Operations",
                    ));
                }
                // The source loop falls through on subtraction. A later
                // operand may still produce an error; None is returned at end.
            } else {
                return Err(datetime_error("Invalid DateTime Operations"));
            }
        }
        Ok(SolutionValue::ExplicitNone)
    }
}
