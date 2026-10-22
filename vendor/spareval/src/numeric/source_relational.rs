//! Source RelationalExpression over admitted source operands.
//! Relation guards and TypeError qualification are outside Literal.__gt__;
//! ORDER rank, identity equality and reflected LT are never used here.
use super::source_order::LiteralComparisonDomain;
use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum SourceRelationalOrder {
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
}

impl QueryNumericSession {
    /// Comparison flags include earlier non-trapping FloatOperation from
    /// equality. Only the currently raised signal is a constructor argument;
    /// filtering every accumulated flag by the trap mask would be incorrect.
    fn source_relational_fault(&self, error: RawNumericFault) -> NumericEvaluationError {
        use super::source_exception::{
            LanguageExceptionArgument, SourceLanguageException, SourceLanguageExceptionKind,
        };
        use super::source_registry::{SourceFunctionFailure, convert_nonraw_type_error};
        let primary = match &error {
            RawNumericFault::Core(NumericFault::DecimalTrap { primary, .. })
                if self.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1 =>
            {
                *primary
            }
            _ => return error.into(),
        };
        let source = SourceLanguageException {
            kind: SourceLanguageExceptionKind::Decimal(primary),
            arguments: vec![LanguageExceptionArgument::DecimalSignalClasses(vec![
                primary,
            ])],
        };
        if !source.kind.is_type_error() {
            return RawNumericFault::SourceLanguageException(source).into();
        }
        match convert_nonraw_type_error(source) {
            SourceFunctionFailure::Sparql(error) => NumericEvaluationError::Sparql {
                kind: error.kind(),
                diagnostic: Cow::Owned(error.diagnostic().to_owned()),
                source: error.source_exception().cloned(),
            },
            SourceFunctionFailure::Exception(error) => {
                RawNumericFault::SourceLanguageException(error).into()
            }
            SourceFunctionFailure::Unreceived(error) => error.into(),
            SourceFunctionFailure::Host => {
                unreachable!("constructor conversion cannot execute a host operation")
            }
        }
    }

    /// RelationalExpression `=` over actual source nodes. This reuses the
    /// Literal.eq value-space implementation, never the graph key's lexical
    /// identity. Exact lexical uncertainty remains a source SPARQLError.
    pub(crate) fn source_relational_equal(
        &self,
        left: &SolutionValue<RawExpressionValue>,
        right: &SolutionValue<RawExpressionValue>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        self.source_relational_equality(left, right, false)
    }

    pub(crate) fn source_relational_not_equal(
        &self,
        left: &SolutionValue<RawExpressionValue>,
        right: &SolutionValue<RawExpressionValue>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        self.source_relational_equality(left, right, true)
    }

    fn source_relational_equality(
        &self,
        left: &SolutionValue<RawExpressionValue>,
        right: &SolutionValue<RawExpressionValue>,
        negate: bool,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(
                RawNumericFault::UnsupportedSourceLiteralOperator("source relation").into(),
            );
        }
        for value in [left, right] {
            if !matches!(value, SolutionValue::Rdf(_)) {
                let text = format!(
                    "I cannot compare this non-node: {}",
                    self.source_repr(value)?
                );
                return Err(NumericEvaluationError::Sparql {
                    kind: SparqlErrorKind::OtherSparql,
                    diagnostic: Cow::Owned(text.clone()),
                    source: Some(SourceException {
                        class: SourceExceptionClass::Sparql,
                        argument: SourceExceptionArgument::Text(text),
                    }),
                });
            }
        }
        let (SolutionValue::Rdf(left), SolutionValue::Rdf(right)) = (left, right) else {
            unreachable!("node guards checked both operands");
        };
        if left.source_variable().is_some() || right.source_variable().is_some() {
            let equal = source_context_equal(left, right)?;
            return self.construct_source_boolean_result(if negate { !equal } else { equal }).map_err(Into::into);
        }
        let result = match (left.rdf()?, right.rdf()?) {
            (Term::Literal(_), Term::Literal(_)) => {
                self.literal_value_equal(left, right, LiteralComparisonDomain::RelationalValue)
                    .map_err(|error| match error {
                        RawNumericFault::SourceComparisonTypeError => {
                            let (Some(a), Some(b)) = (left.literal(), right.literal()) else { unreachable!() };
                            super::source_function::source_function_error(format!(
                                "I cannot know that these two lexical forms do not map to the same value: {} and {}", a.value(), b.value()
                            ))
                        }
                        error => self.source_relational_fault(error),
                    })?
            }
            _ => left.rdf()? == right.rdf()?,
        };
        self.construct_source_boolean_result(if negate { !result } else { result })
            .map_err(Into::into)
    }

    pub(crate) fn source_relational_greater(
        &self,
        left: &SolutionValue<RawExpressionValue>,
        right: &SolutionValue<RawExpressionValue>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        self.source_relational_order(left, right, SourceRelationalOrder::Greater)
    }

    pub(crate) fn source_relational_order(
        &self,
        left: &SolutionValue<RawExpressionValue>,
        right: &SolutionValue<RawExpressionValue>,
        operator: SourceRelationalOrder,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(
                RawNumericFault::UnsupportedSourceLiteralOperator("source relation").into(),
            );
        }
        for value in [left, right] {
            if !matches!(value, SolutionValue::Rdf(raw) if raw.literal().is_some()) {
                let text = format!(
                    "Compare other than =, != of non-literals is an error: {}",
                    self.source_repr(value)?
                );
                return Err(NumericEvaluationError::Sparql {
                    kind: SparqlErrorKind::OtherSparql,
                    diagnostic: Cow::Owned(text.clone()),
                    source: Some(SourceException {
                        class: SourceExceptionClass::Sparql,
                        argument: SourceExceptionArgument::Text(text),
                    }),
                });
            }
        }
        let (SolutionValue::Rdf(left), SolutionValue::Rdf(right)) = (left, right) else {
            unreachable!("literal guards checked both operands");
        };
        let datatype = |value: &RawExpressionValue| {
            let LiteralSourceState::Literal(metadata) = value.literal_source() else {
                return Err(RawNumericFault::InvalidSourceOrigin);
            };
            match &metadata.datatype {
                SourceDatatype::Absent => Ok(None),
                SourceDatatype::Specified(datatype) => Ok(Some(datatype.clone())),
                SourceDatatype::Unknown => Err(RawNumericFault::UnknownSourceLiteralIdentity),
            }
        };
        if let (Some(a), Some(b)) = (datatype(left)?, datatype(right)?) {
            if !super::source_datatype::is_relational_xsd(&a)
                && !super::source_datatype::is_relational_xsd(&b)
            {
                return Err(super::source_function::source_function_error(
                    "Can only do =,!= comparisons of non-XSD Literals",
                ));
            }
        }
        let result = match operator {
            SourceRelationalOrder::Greater => self
                .literal_gt(left, right, LiteralComparisonDomain::RelationalValue)
                .map(Some),
            SourceRelationalOrder::Less => {
                self.literal_lt(left, right, LiteralComparisonDomain::RelationalValue)
            }
            SourceRelationalOrder::GreaterOrEqual => {
                self.literal_ge(left, right, LiteralComparisonDomain::RelationalValue)
            }
            SourceRelationalOrder::LessOrEqual => {
                self.literal_le(left, right, LiteralComparisonDomain::RelationalValue)
            }
        }
        .map_err(|error| match error {
            RawNumericFault::SourceComparisonTypeError => {
                // Non-Decimal TypeError argument domains retain their own
                // qualification; do not invent their constructor from text.
                RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source relational TypeError arguments",
                )
                .into()
            }
            error => self.source_relational_fault(error),
        })?;
        let Some(result) = result else {
            return Err(super::source_function::source_function_error(
                "Error when comparing",
            ));
        };
        self.construct_source_boolean_result(result)
            .map_err(Into::into)
    }
}
