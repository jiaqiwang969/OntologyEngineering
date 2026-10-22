//! operators.default_cast over already evaluated operands.
//! Scoped registry dispatch and callbacks are owned by expression/source_functions.rs.
//! AST evaluation and repeated argument access belong to expression.rs.
use super::*;
use oxrdf::vocab::xsd;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceFunctionKind {
    String,
    Integer,
    Decimal,
    Float,
    Double,
    Boolean,
    DateTime,
}
impl SourceFunctionKind {
    pub(crate) fn registered(iri: &NamedNode) -> Option<Self> {
        Some(match iri.as_ref() {
            xsd::STRING => Self::String,
            xsd::INTEGER => Self::Integer,
            xsd::DECIMAL => Self::Decimal,
            xsd::FLOAT => Self::Float,
            xsd::DOUBLE => Self::Double,
            xsd::BOOLEAN => Self::Boolean,
            xsd::DATE_TIME => Self::DateTime,
            _ => return None,
        })
    }
}

pub(crate) fn source_function_error(message: impl Into<String>) -> NumericEvaluationError {
    let message = message.into();
    NumericEvaluationError::Sparql {
        kind: SparqlErrorKind::OtherSparql,
        diagnostic: Cow::Owned(message.clone()),
        source: Some(SourceException {
            class: SourceExceptionClass::Sparql,
            argument: SourceExceptionArgument::Text(message),
        }),
    }
}

impl QueryNumericSession {
    /// Builtin_DATATYPE reads source metadata, never a reconstructed cache.
    /// Expression/variable error propagation remains with the expression owner.
    pub(crate) fn source_datatype(
        &self,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        self.require_source_literal_semantics()?;
        if let SolutionValue::Rdf(raw) = value
            && raw.literal().is_some()
        {
            source_identity_key(raw)?;
            let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                return Err(RawNumericFault::InvalidSourceOrigin.into());
            };
            let (datatype, object) = match &metadata.language_original {
                SourceFact::Unknown => {
                    return Err(RawNumericFault::UnknownSourceLiteralIdentity.into());
                }
                SourceFact::Known(Some(language)) if !language.is_empty() => (
                    oxrdf::vocab::rdf::LANG_STRING.into_owned(),
                    self.source_language_datatype_object.clone(),
                ),
                _ => match &metadata.datatype {
                    SourceDatatype::Unknown => {
                        return Err(RawNumericFault::UnknownSourceLiteralIdentity.into());
                    }
                    SourceDatatype::Absent => (
                        xsd::STRING.into_owned(),
                        self.source_string_datatype_object.clone(),
                    ),
                    SourceDatatype::Specified(datatype) => (
                        datatype.clone(),
                        raw.source_datatype_object
                            .clone()
                            .ok_or(RawNumericFault::UnknownSourceLiteralIdentity)?,
                    ),
                },
            };
            let mut result = RawExpressionValue::from_import(datatype.into());
            result.source_object = Some(object);
            return Ok(result);
        }
        Err(source_function_error(format!(
            "Can only get datatype of literal: {}",
            self.source_repr(value)?
        )))
    }

    pub(crate) fn source_unknown_function(
        &self,
        iri: &NamedNode,
    ) -> Result<NumericEvaluationError, RawNumericFault> {
        let repr = self.source_repr(&SolutionValue::Rdf(RawExpressionValue::from_import(
            iri.clone().into(),
        )))?;
        Ok(source_function_error(format!("Unknown function {repr}")))
    }

    pub(crate) fn source_default_cast(
        &self,
        kind: SourceFunctionKind,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        let target = match kind {
            SourceFunctionKind::String => xsd::STRING,
            SourceFunctionKind::Integer => xsd::INTEGER,
            SourceFunctionKind::Decimal => xsd::DECIMAL,
            SourceFunctionKind::Float => xsd::FLOAT,
            SourceFunctionKind::Double => xsd::DOUBLE,
            SourceFunctionKind::Boolean => xsd::BOOLEAN,
            SourceFunctionKind::DateTime => xsd::DATE_TIME,
        }
        .into_owned();
        self.source_default_cast_for_iri(&target, value)?
            .ok_or(RawNumericFault::InvalidSourceOrigin.into())
    }
    pub(crate) fn source_default_cast_for_iri(
        &self,
        target: &NamedNode,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<Option<RawExpressionValue>, NumericEvaluationError> {
        let kind = SourceFunctionKind::registered(target);
        self.require_source_literal_semantics()?;
        if kind == Some(SourceFunctionKind::String) {
            if let SolutionValue::Rdf(raw) = value {
                if matches!(raw.rdf(), Ok(Term::NamedNode(_) | Term::Literal(_))) {
                    return Ok(Some(self.construct_source_string_cast(raw)?));
                }
            }
            return Err(source_function_error(format!(
                "Cannot cast term {} of type {}",
                self.source_repr(value)?,
                source_type_repr(value)?,
            )));
        }
        let SolutionValue::Rdf(raw) = value else {
            return Err(source_function_error(
                "Can only cast Literals to non-string data-types",
            ));
        };
        let Some(literal) = raw.literal() else {
            return Err(source_function_error(
                "Can only cast Literals to non-string data-types",
            ));
        };
        source_identity_key(raw)?;
        let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        };
        let dt = match &metadata.datatype {
            SourceDatatype::Unknown => {
                return Err(RawNumericFault::UnknownSourceLiteralIdentity.into());
            }
            SourceDatatype::Absent => None,
            SourceDatatype::Specified(dt) => Some(dt),
        };
        let dt_repr = |dt: &NamedNode| {
            self.source_repr(&SolutionValue::Rdf(RawExpressionValue::from_import(
                dt.clone().into(),
            )))
        };
        if let Some(dt) = dt {
            if !(numeric_kind(dt).is_some()
                || matches!(
                    dt.as_ref(),
                    xsd::STRING | xsd::BOOLEAN | xsd::DATE_TIME | xsd::DATE
                ))
            {
                return Err(source_function_error(format!(
                    "Cannot cast literal with unknown datatype: {}",
                    dt_repr(dt)?
                )));
            }
        }
        if kind == Some(SourceFunctionKind::DateTime) {
            if let Some(dt) = dt {
                if !matches!(dt.as_ref(), xsd::DATE_TIME | xsd::STRING) {
                    return Err(source_function_error(format!(
                        "Cannot cast {} to XSD:dateTime",
                        dt_repr(dt)?
                    )));
                }
            }
            return self.source_datetime_cast(value).map(Some);
        }
        if dt.is_some_and(|dt| dt == &xsd::DATE_TIME) {
            return Err(source_function_error(format!(
                "Cannot cast XSD.dateTime to {}",
                dt_repr(target)?
            )));
        }
        let Some(kind) = kind else {
            return Ok(None);
        };
        let failed = |family: &str| -> Result<NumericEvaluationError, RawNumericFault> {
            Ok(source_function_error(format!(
                "Cannot interpret '{}' as {family}",
                self.source_repr(value)?
            )))
        };
        let text = literal.value();
        if kind == SourceFunctionKind::Boolean {
            return match text.to_lowercase().as_str() {
                "1" | "true" => Ok(Some(self.construct_source_boolean_result(true)?)),
                "0" | "false" => Ok(Some(self.construct_source_boolean_result(false)?)),
                _ => Err(failed("bool")?),
            };
        }
        let (numeric, family) = match kind {
            SourceFunctionKind::Integer => (NumericKind::Integer, "int"),
            SourceFunctionKind::Decimal => (NumericKind::Decimal, "decimal"),
            SourceFunctionKind::Float => (NumericKind::Float, "float"),
            SourceFunctionKind::Double => (NumericKind::Double, "float"),
            _ => unreachable!(),
        };
        // This lexical guard precedes Decimal construction and its flags.
        if kind == SourceFunctionKind::Decimal && (text.contains('e') || text.contains('E')) {
            return Err(failed(family)?);
        }
        let Some(number) = self.parse_value(text, numeric)? else {
            return Err(failed(family)?);
        };
        let lexical = self.format_value(&number)?;
        Ok(Some(self.known_source_cached(
            Literal::new_typed_literal(lexical, target.clone()).into(),
            number,
            SourceLiteralMetadata {
                datatype: SourceDatatype::Specified(target.clone()),
                language_original: SourceFact::Known(None),
                ill_typed: SourceFact::Known(None),
                cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
            },
        )?))
    }
}

pub(super) fn source_type_name(
    value: &SolutionValue<RawExpressionValue>,
) -> Result<&'static str, RawNumericFault> {
    let name = match value {
        SolutionValue::ExplicitNone => "NoneType",
        SolutionValue::Rdf(raw) if raw.source_variable().is_some() => "rdflib.term.Variable",
        SolutionValue::Rdf(raw) => match raw.rdf()? {
            Term::NamedNode(_) => "rdflib.term.URIRef",
            Term::BlankNode(_) => "rdflib.term.BNode",
            Term::Literal(_) => "rdflib.term.Literal",
            #[cfg(feature = "sparql-12")]
            Term::Triple(_) => {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source triple type",
                ));
            }
        },
        SolutionValue::ReturnedError(error) => match error
            .source_exception()
            .ok_or(RawNumericFault::UnknownSourceErrorText)?
            .class
        {
            SourceExceptionClass::Sparql => "rdflib.plugins.sparql.sparql.SPARQLError",
            SourceExceptionClass::SparqlType => "rdflib.plugins.sparql.sparql.SPARQLTypeError",
            SourceExceptionClass::NotBound => "rdflib.plugins.sparql.sparql.NotBoundError",
            SourceExceptionClass::AlreadyBound => "rdflib.plugins.sparql.sparql.AlreadyBound",
        },
    };
    Ok(name)
}

fn source_type_repr(value: &SolutionValue<RawExpressionValue>) -> Result<String, RawNumericFault> {
    Ok(format!("<class '{}'>", source_type_name(value)?))
}

/// Fixed source builtins over already evaluated nodes. General Unicode case
/// mapping is an explicit pending owner; ASCII case and reached nonliteral
/// errors are admitted without delegating to the host Unicode version.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SourceNodeUnary { IsIri, IsBlank, IsLiteral, IsNumeric, Lang, StrLen, Upper, Lower }
impl QueryNumericSession {
    pub(crate) fn source_node_unary(&self, op: SourceNodeUnary, value: &SolutionValue<RawExpressionValue>) -> Result<RawExpressionValue, NumericEvaluationError> {
        self.require_source_literal_semantics()?;
        use SourceNodeUnary::*;
        let raw = if let SolutionValue::Rdf(raw) = value { Some(raw) } else { None };
        let literal = raw.and_then(RawExpressionValue::literal);
        let predicate = match op {
            IsIri => Some(raw.is_some_and(|v| matches!(v.rdf(), Ok(Term::NamedNode(_))))),
            IsBlank => Some(raw.is_some_and(|v| matches!(v.rdf(), Ok(Term::BlankNode(_))))),
            IsLiteral => Some(literal.is_some()),
            IsNumeric => {
                let numeric = if let Some(raw) = raw.filter(|v| v.literal().is_some()) {
                    let LiteralSourceState::Literal(meta) = raw.literal_source() else { return Err(RawNumericFault::InvalidSourceOrigin.into()); };
                    match &meta.datatype {
                        SourceDatatype::Unknown => return Err(RawNumericFault::UnknownSourceLiteralIdentity.into()),
                        SourceDatatype::Absent => false,
                        SourceDatatype::Specified(dt) => numeric_kind(dt).is_some(),
                    }
                } else { false };
                Some(numeric)
            },
            _ => None,
        };
        if let Some(result) = predicate { return self.construct_source_boolean_result(result).map_err(Into::into); }
        let Some(literal) = literal else {
            let spelling = if matches!(op, Lang) { "passed" } else { "passes" };
            return Err(source_function_error(format!("Non-literal {spelling} as string: {}", self.source_repr(value)?)));
        };
        let raw = raw.ok_or(RawNumericFault::InvalidSourceOrigin)?;
        let LiteralSourceState::Literal(meta) = raw.literal_source() else { return Err(RawNumericFault::InvalidSourceOrigin.into()); };
        if matches!(op, Lang) {
            let SourceFact::Known(language) = &meta.language_original else { return Err(RawNumericFault::UnknownSourceLiteralIdentity.into()); };
            return self.construct_source_plain_result(language.clone().unwrap_or_default()).map_err(Into::into);
        }
        match &meta.datatype {
            SourceDatatype::Unknown => return Err(RawNumericFault::UnknownSourceLiteralIdentity.into()),
            SourceDatatype::Specified(dt) if dt != &xsd::STRING => return Err(source_function_error(format!("Non-string datatype-literal passes as string: {}", self.source_repr(value)?))),
            _ => (),
        }
        if matches!(op, StrLen) { return self.construct_result(NumericValue::Integer(BigInt::from(literal.value().chars().count())), None).map_err(Into::into); }
        if !literal.value().is_ascii() { return Err(RawNumericFault::UnsupportedSourceLiteralOperator("unreceived non-ASCII source case mapping").into()); }
        let lexical = match op { Upper => literal.value().to_ascii_uppercase(), Lower => literal.value().to_ascii_lowercase(), _ => unreachable!() };
        let datatype = match &meta.datatype { SourceDatatype::Specified(dt) => Some(dt.clone()), SourceDatatype::Absent => None, SourceDatatype::Unknown => return Err(RawNumericFault::UnknownSourceLiteralIdentity.into()) };
        let SourceFact::Known(language) = &meta.language_original else { return Err(RawNumericFault::UnknownSourceLiteralIdentity.into()); };
        self.construct_source_direct_string(&lexical, datatype, language.clone(), true).map_err(Into::into)
    }
}

#[cfg(test)]
pub(crate) mod observation {
    use super::*;
    use std::cell::RefCell;
    #[derive(Clone, Debug)]
    pub(crate) enum Event {
        Call(NamedNode),
        Return(NamedNode, SolutionValue<RawExpressionValue>),
    }
    thread_local! {
        static EVENTS: RefCell<Option<Vec<Event>>> = const { RefCell::new(None) };
    }
    pub(crate) struct Guard;
    impl Guard {
        pub(crate) fn start() -> Self {
            EVENTS.with(|events| assert!(events.borrow_mut().replace(Vec::new()).is_none()));
            Self
        }
        pub(crate) fn take(&self) -> Vec<Event> {
            EVENTS.with(|events| events.borrow_mut().take().unwrap())
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            EVENTS.with(|events| {
                events.borrow_mut().take();
            });
        }
    }
    pub(crate) fn call(iri: &NamedNode) {
        EVENTS.with(|events| {
            if let Some(events) = events.borrow_mut().as_mut() {
                events.push(Event::Call(iri.clone()));
            }
        });
    }
    pub(crate) fn result(iri: &NamedNode, result: &SolutionValue<RawExpressionValue>) {
        EVENTS.with(|events| {
            if let Some(events) = events.borrow_mut().as_mut() {
                events.push(Event::Return(iri.clone(), result.clone()));
            }
        });
    }
}
