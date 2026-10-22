//! Temporal constructors are admitted only by the sealed graph factory.
//! Earlier private/default constructors retain their existing admission.
use super::*;
use oxrdf::vocab::xsd;
use semantica_temporal::{DateTime, parse_datetime};

fn graph_value_cache(
    lexical: &str,
    datatype: &NamedNode,
) -> Result<SourceLiteralCacheKind, RawNumericFault> {
    if datatype == &xsd::TIME {
        return Ok(SourceLiteralCacheKind::Time(semantica_temporal::parse_time(lexical)));
    }
    if datatype == &xsd::DATE {
        return Ok(SourceLiteralCacheKind::Date(
            semantica_temporal::parse_date(lexical),
        ));
    }
    if datatype == &xsd::DAY_TIME_DURATION {
        // This shares the existing snapshot normalizer's original grammar and
        // timedelta accumulation. Calendar Duration contexts remain separate.
        if lexical.len() > 10_000 {
            return Err(NumericFault::ResourceLimit {
                operation: "source_duration_input_bytes",
            }
            .into());
        }
        let value = if let Some(parts) = semantica_temporal::duration_parts(lexical) {
            if parts.has_calendar_components() {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "calendar duration source cache",
                ));
            }
            parts.timedelta_micros().and_then(|value| {
                semantica_temporal::Timedelta::from_micros(if parts.negative {
                    -value
                } else {
                    value
                })
            })
        } else {
            if lexical.strip_prefix('P').and_then(parse_datetime).is_some() {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "calendar duration source cache",
                ));
            }
            None
        };
        return Ok(SourceLiteralCacheKind::DayTimeDuration(value));
    }
    if !super::source_datatype::has_registered_converter(datatype) {
        return Ok(SourceLiteralCacheKind::UnmappedDatatype);
    }
    Err(RawNumericFault::UnsupportedSourceLiteralOperator(
        "query source datatype constructor",
    ))
}

pub(super) fn validate_source_graph_value(
    raw: &RawExpressionValue,
    metadata: &SourceLiteralMetadata,
) -> Result<(), RawNumericFault> {
    let (Term::Literal(literal), SourceDatatype::Specified(datatype)) =
        (raw.rdf()?, &metadata.datatype)
    else {
        return Err(RawNumericFault::InvalidSourceOrigin);
    };
    let cache = graph_value_cache(literal.value(), datatype)?;
    let direct_ill_typed = match cache {
        SourceLiteralCacheKind::Date(value) => Some(value.is_none()),
        SourceLiteralCacheKind::Time(value) => Some(value.is_none()),
        SourceLiteralCacheKind::DayTimeDuration(value) => Some(value.is_none()),
        SourceLiteralCacheKind::UnmappedDatatype => None,
        _ => return Err(RawNumericFault::InvalidSourceOrigin),
    };
    if !matches!(raw.numeric(), NumericCacheState::NotNumeric)
        || metadata.language_original != SourceFact::Known(None)
        || (metadata.ill_typed != SourceFact::Known(None)
            && metadata.ill_typed != SourceFact::Known(direct_ill_typed))
        || cache
            != match metadata.cached_value_kind.clone() {
                SourceFact::Known(value) => value,
                SourceFact::Unknown => return Err(RawNumericFault::InvalidSourceOrigin),
            }
    {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    Ok(())
}

pub(super) fn validate_source_datetime(
    raw: &RawExpressionValue,
    metadata: &SourceLiteralMetadata,
    value: Option<DateTime>,
) -> Result<(), RawNumericFault> {
    let Term::Literal(literal) = raw.rdf()? else {
        return Err(RawNumericFault::InvalidSourceOrigin);
    };
    if !matches!(raw.numeric(), NumericCacheState::NotNumeric)
        || metadata.datatype != SourceDatatype::Specified(xsd::DATE_TIME.into_owned())
        || metadata.language_original != SourceFact::Known(None)
        || !matches!(metadata.ill_typed, SourceFact::Known(None))
            && metadata.ill_typed != SourceFact::Known(Some(value.is_none()))
        || parse_datetime(literal.value()) != value
    {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    Ok(())
}
impl QueryNumericSession {
    /// Additional private consumer admission. TimeValuesV1 remains a distinct
    /// construction-only capability; no default or public factory opts in.
    pub(crate) fn with_source_time_comparisons_v1(self) -> Result<Self, RawNumericFault> {
        let mut session = self.with_source_time_values_v1()?;
        session.source_time_comparisons_v1 = true;
        Ok(session)
    }
    pub(crate) fn with_source_time_values_v1(mut self) -> Result<Self, RawNumericFault> {
        self.require_source_literal_semantics()?;
        if self.context_semantics() != QueryContextSemantics::SourceContextV1 {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        self.source_time_values_v1 = true;
        Ok(self)
    }
    fn construct_source_datetime(
        &self,
        lexical: &str,
        normalize: bool,
        direct_string: bool,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        self.check_text(lexical)?;
        if direct_string {
            self.check_text(xsd::DATE_TIME.as_str())?;
        }
        let value = parse_datetime(lexical);
        let output = if normalize {
            value
                .map(DateTime::isoformat)
                .unwrap_or_else(|| lexical.to_owned())
        } else {
            lexical.to_owned()
        };
        self.finish_source_datetime(output, value, direct_string.then_some(value.is_none()))
    }
    fn finish_source_datetime(
        &self,
        output: String,
        value: Option<DateTime>,
        ill_typed: Option<bool>,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        if output.chars().count() > self.limits.max_output_characters {
            return Err(RawNumericFault::Core(NumericFault::ResourceLimit {
                operation: "output_characters",
            }));
        }
        RawExpressionValue::from_parts(
            Literal::new_typed_literal(output, xsd::DATE_TIME).into(),
            NumericCacheState::NotNumeric,
        )
        .with_source_metadata(SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(xsd::DATE_TIME.into_owned()),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(ill_typed),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::DateTime(value)),
        })
    }
    // term.py maps anyURI to str, without URI validation or normalization.
    // Direct strings set ill_typed=false; Literal(existing) leaves None.
    fn construct_source_any_uri(&self, lexical: &str, direct: bool) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        self.check_text(lexical)?;
        self.check_text(xsd::ANY_URI.as_str())?;
        if lexical.chars().count() > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit { operation: "output_characters" }.into());
        }
        RawExpressionValue::from_parts(Literal::new_typed_literal(lexical, xsd::ANY_URI).into(), NumericCacheState::NotNumeric)
            .with_source_metadata(SourceLiteralMetadata {
                datatype: SourceDatatype::Specified(xsd::ANY_URI.into_owned()),
                language_original: SourceFact::Known(None),
                ill_typed: SourceFact::Known(direct.then_some(false)),
                cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::String),
            })
    }
    pub(super) fn construct_source_graph_literal(
        &self,
        parsed: &ParsedSourceLiteral,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        if matches!(
            parsed.origin,
            SourceLiteralOrigin::QueryTypedExistingLiteral
        ) && parsed.datatype == SourceDatatype::Specified(xsd::DATE_TIME.into_owned())
        {
            if parsed.occurrence.is_some()
                || parsed.language_original.is_some()
                || parsed.rdf.datatype() != xsd::DATE_TIME
                || parsed.rdf.language().is_some()
            {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
            self.construct_source_datetime(parsed.rdf.value(), false, false)
        } else if matches!(
            parsed.origin,
            SourceLiteralOrigin::QueryTypedExistingLiteral
        ) && let SourceDatatype::Specified(datatype) = &parsed.datatype
            && numeric_kind(datatype).is_none()
            && datatype != &xsd::STRING
        {
            self.require_source_literal_semantics()?;
            self.check_text(parsed.rdf.value())?;
            self.check_text(datatype.as_str())?;
            if parsed.occurrence.is_some()
                || parsed.language_original.is_some()
                || parsed.rdf.datatype() != datatype.as_ref()
                || parsed.rdf.language().is_some()
            {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
            if datatype == &xsd::BOOLEAN {
                return self.construct_source_boolean_existing(parsed.rdf.value());
            }
            if datatype == &xsd::ANY_URI {
                return self.construct_source_any_uri(parsed.rdf.value(), false);
            }
            if self.source_calendar_duration_v1 && super::source_duration::is_duration(datatype) {
                return self.construct_source_duration(parsed.rdf.value(),datatype.clone(),false,false);
            }
            if datatype == &xsd::TIME && !self.source_time_values_v1 {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "query source datatype constructor",
                ));
            }
            let cache = graph_value_cache(parsed.rdf.value(), datatype)?;
            RawExpressionValue::from_parts(parsed.rdf.clone().into(), NumericCacheState::NotNumeric)
                .with_source_metadata(SourceLiteralMetadata {
                    datatype: parsed.datatype.clone(),
                    language_original: SourceFact::Known(None),
                    ill_typed: SourceFact::Known(None),
                    cached_value_kind: SourceFact::Known(cache),
                })
        } else {
            self.construct_source_literal(parsed)
        }
    }
    pub(crate) fn construct_source_graph_initial(
        &self,
        input: NumericInitialValue,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        match input {
            NumericInitialValue::SourceDirectString { lexical, datatype: Some(datatype), language, .. }
                if datatype == xsd::ANY_URI => {
                if language.as_deref().is_some_and(|v| !v.is_empty()) {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                self.construct_source_any_uri(&lexical, true)
            }
            NumericInitialValue::SourceDirectString { lexical, datatype:Some(datatype), language, normalize }
                if self.source_calendar_duration_v1 && super::source_duration::is_duration(&datatype) => {
                if language.as_deref().is_some_and(|v|!v.is_empty()) {return Err(RawNumericFault::InvalidSourceOrigin)}
                self.construct_source_duration(&lexical,datatype,normalize,true)
            }
            NumericInitialValue::SourceDirectString {
                lexical,
                datatype: Some(dt),
                language,
                normalize,
            } if dt == xsd::DATE_TIME => {
                if language.as_deref().is_some_and(|v| !v.is_empty()) {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                self.construct_source_datetime(&lexical, normalize, true)
            }
            NumericInitialValue::SourceDirectString {
                lexical,
                datatype: Some(datatype),
                language,
                normalize,
            } if (datatype == xsd::TIME && self.source_time_values_v1)
                || datatype == xsd::DATE
                || datatype == xsd::DAY_TIME_DURATION
                || !super::source_datatype::has_registered_converter(&datatype) =>
            {
                self.require_source_literal_semantics()?;
                self.check_text(&lexical)?;
                self.check_text(datatype.as_str())?;
                if language.as_deref().is_some_and(|v| !v.is_empty()) {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                let cache = graph_value_cache(&lexical, &datatype)?;
                let (ill_typed, normalized) = match cache {
                    SourceLiteralCacheKind::Time(value) => (
                        Some(value.is_none()),
                        value.map(semantica_temporal::Time::isoformat),
                    ),
                    SourceLiteralCacheKind::Date(value) => (
                        Some(value.is_none()),
                        value.map(semantica_temporal::Date::isoformat),
                    ),
                    SourceLiteralCacheKind::DayTimeDuration(value) => (
                        Some(value.is_none()),
                        value.map(semantica_temporal::Timedelta::isoformat),
                    ),
                    SourceLiteralCacheKind::UnmappedDatatype => (None, None),
                    _ => return Err(RawNumericFault::InvalidSourceOrigin),
                };
                let output = if normalize {
                    normalized.unwrap_or(lexical)
                } else {
                    lexical
                };
                if output.chars().count() > self.limits.max_output_characters {
                    return Err(NumericFault::ResourceLimit {
                        operation: "output_characters",
                    }
                    .into());
                }
                RawExpressionValue::from_parts(
                    Literal::new_typed_literal(output, datatype.clone()).into(),
                    NumericCacheState::NotNumeric,
                )
                .with_source_metadata(SourceLiteralMetadata {
                    datatype: SourceDatatype::Specified(datatype),
                    language_original: SourceFact::Known(None),
                    ill_typed: SourceFact::Known(ill_typed),
                    cached_value_kind: SourceFact::Known(cache),
                })
            }
            input => self.construct_initial(input),
        }
    }
    pub(super) fn source_datetime_cast(
        &self,
        input: &SolutionValue<RawExpressionValue>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        self.require_source_literal_semantics()?;
        let SolutionValue::Rdf(raw) = input else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        let Term::Literal(literal) = raw.rdf()? else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        let lexical = literal.value();
        self.check_text(lexical)?;
        let Some(value) = parse_datetime(lexical) else {
            let repr = self.source_repr(input)?;
            return Err(super::source_function::source_function_error(format!(
                "Cannot interpret '{repr}' as datetime"
            )));
        };
        // Literal(datetime_object, datatype=dateTime) canonicalizes the string
        // and leaves ill_typed=None, distinct from Literal(str, datatype=...).
        self.finish_source_datetime(value.isoformat(), Some(value), None)
            .map_err(Into::into)
    }
}
