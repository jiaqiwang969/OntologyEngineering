//! Source aggregate numeric state. Evaluation, DISTINCT and catches remain in
//! the real accumulator; this owner consumes already evaluated source values.
use super::*;
use semantica_numeric::{binary64_divide_integers, python_sum_binary64_pair};

pub(crate) struct NumericAggregateState {
    value: NumericValue,
    datatype: Option<NamedNode>,
}
impl NumericAggregateState {
    pub(crate) fn new() -> Self {
        Self {
            value: NumericValue::Integer(BigInt::from(0)),
            datatype: None,
        }
    }
    pub(crate) fn value(&self) -> &NumericValue {
        &self.value
    }
    pub(crate) fn datatype(&self) -> Option<&NamedNode> {
        self.datatype.as_ref()
    }
}

fn aggregate_datatype(
    value: &SolutionValue<RawExpressionValue>,
    semantics: QueryLiteralSemantics,
) -> Result<Option<NamedNode>, NumericEvaluationError> {
    if let SolutionValue::Rdf(value) = value {
        if let Some(literal) = value.literal() {
            if semantics == QueryLiteralSemantics::SourceLiteralV1 {
                let LiteralSourceState::Literal(metadata) = value.literal_source() else {
                    return Err(RawNumericFault::InvalidSourceOrigin.into());
                };
                return match &metadata.datatype {
                    SourceDatatype::Absent => Ok(None),
                    SourceDatatype::Specified(datatype) => Ok(Some(datatype.clone())),
                    SourceDatatype::Unknown => {
                        Err(RawNumericFault::UnknownSourceLiteralIdentity.into())
                    }
                };
            }
            return Ok(Some(literal.datatype().into_owned()));
        }
    }
    Err(RawNumericFault::AggregateDatatypeAccess.into())
}

fn aggregate_promote(
    old: Option<&NamedNode>,
    new: Option<NamedNode>,
) -> Result<Option<NamedNode>, NumericEvaluationError> {
    let Some(old) = old else {
        return Ok(new);
    };
    // Fixed datatypes.py type_promotion returns t1 immediately when t2 is
    // None, BEFORE integer subtype normalization. Absent source datatype is
    // not the explicit RDF 1.1 xsd:string datatype.
    let Some(new) = new else {
        return Ok(Some(old.clone()));
    };
    // datatypes.type_promotion normalizes integer subtypes first, and equal
    // nonnumeric types match too. numeric(value) is a separate source step.
    let normalize = |dt: &NamedNode| match numeric_kind(dt) {
        Some(NumericKind::Integer) => datatype(NumericKind::Integer),
        _ => dt.clone(),
    };
    let old = normalize(old);
    let new = normalize(&new);
    if old == new {
        return Ok(Some(old));
    }
    promote(&old, &new)
        .map(Some)
        .map_err(|_| RawNumericFault::AggregateTypePromotion.into())
}

impl QueryNumericSession {
    pub(crate) fn accumulate_sum(
        &self,
        state: &mut NumericAggregateState,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<(), NumericEvaluationError> {
        state.datatype = aggregate_promote(
            state.datatype.as_ref(),
            aggregate_datatype(value, self.literal_semantics())?,
        )?;
        let (value, _) = self.require_numeric(value)?;
        state.value = self.aggregate_pair(&state.value, &value)?;
        Ok(())
    }
    pub(crate) fn accumulate_average(
        &self,
        state: &mut NumericAggregateState,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<(), NumericEvaluationError> {
        let (numeric, _) = self.require_numeric(value)?;
        state.value = self.aggregate_pair(&state.value, &numeric)?;
        state.datatype = aggregate_promote(
            state.datatype.as_ref(),
            aggregate_datatype(value, self.literal_semantics())?,
        )?;
        Ok(())
    }
    pub(crate) fn increment_average_count(
        &self,
        count: &BigInt,
    ) -> Result<BigInt, RawNumericFault> {
        Ok(apply_integer(
            IntegerOp::Add,
            count,
            Some(&BigInt::from(1)),
            &self.limits,
        )?)
    }
    pub(crate) fn finish_sum(
        &self,
        state: &NumericAggregateState,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        self.construct_result(state.value.clone(), state.datatype.clone())
            .map_err(Into::into)
    }
    pub(crate) fn finish_average(
        &self,
        state: &NumericAggregateState,
        count: &BigInt,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        if *count == BigInt::from(0) {
            return self
                .construct_result(NumericValue::Integer(BigInt::from(0)), None)
                .map_err(Into::into);
        }
        let value = if state.datatype.as_ref().is_some_and(|dt| {
            matches!(
                numeric_kind(dt),
                Some(NumericKind::Float | NumericKind::Double)
            )
        }) {
            match &state.value {
                NumericValue::Integer(value) => NumericValue::Binary64(
                    binary64_divide_integers(value, count, &self.limits)
                        .map_err(RawNumericFault::from)?,
                ),
                NumericValue::Binary64(value) => {
                    let denominator = binary64_from_integer(count, &self.limits)
                        .map_err(RawNumericFault::from)?;
                    NumericValue::Binary64(
                        apply_binary64_binary(Binary64BinaryOp::Divide, *value, denominator)
                            .map_err(RawNumericFault::from)?,
                    )
                }
                NumericValue::Decimal(value) => {
                    let denominator =
                        decimal_from_integer(count, &self.limits).map_err(RawNumericFault::from)?;
                    NumericValue::Decimal(
                        self.observe(self.decimal.apply(
                            DecimalOp::Divide,
                            value,
                            Some(&denominator),
                            &self.limits,
                        ))
                        .map_err(RawNumericFault::from)?,
                    )
                }
            }
        } else {
            let numerator = match &state.value {
                NumericValue::Binary64(value) => self.decimal_from_float_observed(*value)?,
                value => exact_decimal(value.clone(), &self.limits)?,
            };
            let denominator =
                decimal_from_integer(count, &self.limits).map_err(RawNumericFault::from)?;
            NumericValue::Decimal(
                self.observe(self.decimal.apply(
                    DecimalOp::Divide,
                    &numerator,
                    Some(&denominator),
                    &self.limits,
                ))
                .map_err(RawNumericFault::from)?,
            )
        };
        self.construct_result(value, None).map_err(Into::into)
    }

    /// builtin sum restarts from integer zero for EACH two-item input. Decimal
    /// rounding is therefore observable twice; float compensation never spans
    /// updates. Float+Integer is not the exact-float compensation branch.
    fn aggregate_pair(
        &self,
        first: &NumericValue,
        second: &NumericValue,
    ) -> Result<NumericValue, NumericEvaluationError> {
        use NumericValue::{Binary64, Decimal, Integer};
        if self.capabilities() == QueryNumericCapabilities::IntegerDecimalV1
            && (matches!(first, Binary64(_)) || matches!(second, Binary64(_)))
        {
            return Err(RawNumericFault::UnsupportedNumericFamily.into());
        }
        if matches!(
            (first, second),
            (Binary64(_), Decimal(_)) | (Decimal(_), Binary64(_))
        ) {
            // type_safe_numbers returns a lazy map(float,...). The first
            // conversion and int0+first precede conversion of the second item.
            let first = match first {
                Decimal(v) => {
                    binary64_from_decimal(v, &self.limits).map_err(RawNumericFault::from)?
                }
                Binary64(v) => *v,
                _ => unreachable!(),
            };
            let first = 0.0 + first;
            let second = match second {
                Decimal(v) => {
                    binary64_from_decimal(v, &self.limits).map_err(RawNumericFault::from)?
                }
                Binary64(v) => *v,
                _ => unreachable!(),
            };
            return Ok(Binary64(python_sum_binary64_pair(first, second)));
        }
        if let (Binary64(first), Binary64(second)) = (first, second) {
            return Ok(Binary64(python_sum_binary64_pair(*first, *second)));
        }
        let first = match first {
            Integer(value) => Integer(
                apply_integer(IntegerOp::Add, &BigInt::from(0), Some(value), &self.limits)
                    .map_err(RawNumericFault::from)?,
            ),
            Decimal(value) => {
                let zero = decimal_from_integer(&BigInt::from(0), &self.limits)
                    .map_err(RawNumericFault::from)?;
                Decimal(
                    self.observe(self.decimal.apply(
                        DecimalOp::Add,
                        &zero,
                        Some(value),
                        &self.limits,
                    ))
                    .map_err(RawNumericFault::from)?,
                )
            }
            Binary64(value) => Binary64(0.0 + *value),
        };
        Ok(match (first, second) {
            (Integer(a), Integer(b)) => Integer(
                apply_integer(IntegerOp::Add, &a, Some(b), &self.limits)
                    .map_err(RawNumericFault::from)?,
            ),
            (Binary64(a), Integer(b)) => {
                Binary64(a + binary64_from_integer(b, &self.limits).map_err(RawNumericFault::from)?)
            }
            (Integer(a), Binary64(b)) => Binary64(
                binary64_from_integer(&a, &self.limits).map_err(RawNumericFault::from)? + b,
            ),
            (a, b) => {
                let a = exact_decimal(a, &self.limits)?;
                let b = exact_decimal(b.clone(), &self.limits)?;
                Decimal(
                    self.observe(
                        self.decimal
                            .apply(DecimalOp::Add, &a, Some(&b), &self.limits),
                    )
                    .map_err(RawNumericFault::from)?,
                )
            }
        })
    }
}

#[cfg(test)]
mod source_datatype_tests {
    use super::*;

    fn session(semantics: QueryLiteralSemantics) -> QueryNumericSession {
        QueryNumericSession::new_with_literal_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            semantics,
        )
        .unwrap()
    }

    #[test]
    fn source_aggregate_absent_datatype_preserves_original_promotion_order() {
        let source = session(QueryLiteralSemantics::SourceLiteralV1);
        let integer = source
            .construct_source_direct_string("1", Some(datatype(NumericKind::Integer)), None, true)
            .unwrap();
        let plain = source
            .construct_source_direct_string("bad", None, None, true)
            .unwrap();
        let typed = source
            .construct_source_direct_string(
                "bad",
                Some(NamedNode::new(format!("{XSD}string")).unwrap()),
                None,
                true,
            )
            .unwrap();
        let language = source
            .construct_source_direct_string("bad", None, Some("EN".into()), true)
            .unwrap();
        for invalid in [plain.clone(), language] {
            let mut state = NumericAggregateState::new();
            source
                .accumulate_sum(&mut state, &SolutionValue::Rdf(integer.clone()))
                .unwrap();
            assert!(matches!(
                source.accumulate_sum(&mut state, &SolutionValue::Rdf(invalid)),
                Err(NumericEvaluationError::Sparql {
                    kind: SparqlErrorKind::Type,
                    ..
                })
            ));
            assert_eq!(state.datatype(), Some(&datatype(NumericKind::Integer)));
        }
        let mut state = NumericAggregateState::new();
        source
            .accumulate_sum(&mut state, &SolutionValue::Rdf(integer))
            .unwrap();
        assert!(matches!(
            source.accumulate_sum(&mut state, &SolutionValue::Rdf(typed.clone())),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::AggregateTypePromotion
            ))
        ));
        for invalid in [plain, typed] {
            assert!(matches!(
                source.accumulate_sum(
                    &mut NumericAggregateState::new(),
                    &SolutionValue::Rdf(invalid)
                ),
                Err(NumericEvaluationError::Sparql {
                    kind: SparqlErrorKind::Type,
                    ..
                })
            ));
        }
        let subtype = NamedNode::new(format!("{XSD}unsignedByte")).unwrap();
        assert_eq!(
            aggregate_promote(Some(&subtype), None).unwrap(),
            Some(subtype)
        );
    }

    #[test]
    fn source_aggregate_datatype_choice_leaves_legacy_rdf_branch_active() {
        let source = session(QueryLiteralSemantics::SourceLiteralV1);
        let plain = source
            .construct_source_direct_string("bad", None, None, true)
            .unwrap();
        let cell = SolutionValue::Rdf(plain);
        assert_eq!(
            aggregate_datatype(&cell, QueryLiteralSemantics::SourceLiteralV1).unwrap(),
            None
        );
        assert_eq!(
            aggregate_datatype(&cell, QueryLiteralSemantics::RdfIdentityV1).unwrap(),
            Some(NamedNode::new(format!("{XSD}string")).unwrap())
        );
        let unknown = SolutionValue::Rdf(RawExpressionValue::from_import(
            Literal::new_simple_literal("bad").into(),
        ));
        assert!(matches!(
            aggregate_datatype(&unknown, QueryLiteralSemantics::SourceLiteralV1),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnknownSourceLiteralIdentity
            ))
        ));
        assert!(aggregate_datatype(&unknown, QueryLiteralSemantics::RdfIdentityV1).is_ok());
    }
}
