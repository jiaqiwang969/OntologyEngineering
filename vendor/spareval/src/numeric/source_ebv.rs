//! Fixed operators.EBV over already evaluated source values. This is distinct
//! from Literal.__bool__ and never walks an expression or reparses a cache.
use super::*;
use oxrdf::vocab::xsd;

const NON_LITERAL: &str = "http://www.w3.org/TR/rdf-sparql-query/#ebv - ' +             'Only literals have Boolean values! ";
const UNCERTAIN_LITERAL: &str = "http://www.w3.org/TR/rdf-sparql-query/#ebv - ' +                     'Could not determine the EBV for : ";

impl QueryNumericSession {
    pub(crate) fn source_effective_boolean_value(
        &self,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<bool, NumericEvaluationError> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator("source EBV").into());
        }
        let SolutionValue::Rdf(raw) = value else {
            return Err(self.source_ebv_type_error(value, NON_LITERAL)?);
        };
        let Some(literal) = raw.literal() else {
            return Err(self.source_ebv_type_error(value, NON_LITERAL)?);
        };
        let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        match &metadata.datatype {
            SourceDatatype::Unknown => {
                return Err(RawNumericFault::UnknownSourceLiteralIdentity.into());
            }
            SourceDatatype::Absent => return Ok(!literal.value().is_empty()),
            SourceDatatype::Specified(datatype) if datatype == &xsd::STRING => {
                return Ok(!literal.value().is_empty());
            }
            SourceDatatype::Specified(datatype) if datatype == &xsd::BOOLEAN => {
                // Fixed EBV returns toPython() directly for booleans. An
                // uncached boolean can return a Literal, not the generic
                // failed-numeric TypeError below. This increment admits only
                // the received real bool cache and leaves that fallback open.
                return match metadata.cached_value_kind {
                    SourceFact::Known(SourceLiteralCacheKind::Boolean(value)) => Ok(value),
                    SourceFact::Unknown => Err(RawNumericFault::UnknownNumericCache.into()),
                    _ => Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "uncached boolean source EBV",
                    )
                    .into()),
                };
            }
            _ => (),
        }
        match &metadata.cached_value_kind {
            SourceFact::Unknown => Err(RawNumericFault::UnknownNumericCache.into()),
            SourceFact::Known(
                SourceLiteralCacheKind::DateTime(Some(_)) | SourceLiteralCacheKind::Date(Some(_))
                | SourceLiteralCacheKind::Time(Some(_)),
            ) => Ok(true),
            SourceFact::Known(SourceLiteralCacheKind::Duration(cache)) => {
                match &cache.value {Some(v)=>Ok(v.python_truth()),None=>Err(self.source_ebv_type_error(value,UNCERTAIN_LITERAL)?)}
            }
            SourceFact::Known(SourceLiteralCacheKind::DayTimeDuration(Some(value))) => {
                Ok(!value.is_zero())
            }
            SourceFact::Known(
                SourceLiteralCacheKind::None
                | SourceLiteralCacheKind::DateTime(None)
                | SourceLiteralCacheKind::Date(None)
                | SourceLiteralCacheKind::Time(None)
                | SourceLiteralCacheKind::DayTimeDuration(None)
                | SourceLiteralCacheKind::UnmappedDatatype,
            ) => {
                // Literal.toPython() returns the literal itself when the
                // cached value is None; EBV creates a new source TypeError.
                Err(self.source_ebv_type_error(value, UNCERTAIN_LITERAL)?)
            }
            SourceFact::Known(SourceLiteralCacheKind::Boolean(value)) => Ok(*value),
            SourceFact::Known(SourceLiteralCacheKind::String) => Ok(!literal.value().is_empty()),
            SourceFact::Known(SourceLiteralCacheKind::ErrorObject) => {
                if matches!(raw.numeric(), NumericCacheState::ErrorObject(_)) {
                    Ok(true)
                } else {
                    Err(RawNumericFault::InvalidSourceOrigin.into())
                }
            }
            SourceFact::Known(SourceLiteralCacheKind::Numeric) => match raw.numeric() {
                NumericCacheState::Known(cache)
                    if cache.datatype() == &literal.datatype().into_owned() =>
                {
                    Ok(match cache.value() {
                        NumericValue::Integer(value) => value != &BigInt::from(0),
                        NumericValue::Decimal(value) => value.python_truth(),
                        NumericValue::Binary64(value) => *value != 0.0,
                    })
                }
                _ => Err(RawNumericFault::InvalidSourceOrigin.into()),
            },
        }
    }

    fn source_ebv_type_error(
        &self,
        value: &SolutionValue<RawExpressionValue>,
        prefix: &str,
    ) -> Result<NumericEvaluationError, RawNumericFault> {
        let text = format!("{prefix}{}", self.source_repr(value)?);
        Ok(NumericEvaluationError::Sparql {
            kind: SparqlErrorKind::Type,
            diagnostic: Cow::Owned(text.clone()),
            source: Some(SourceException {
                class: SourceExceptionClass::SparqlType,
                argument: SourceExceptionArgument::Text(text),
            }),
        })
    }
}
