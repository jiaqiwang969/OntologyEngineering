//! Source str/repr for values already produced by the one query evaluator.
//!
//! RDFLib term.py:146-152,350-356,502-508,1752-1762 supplies the term
//! representation structure. Character quoting belongs to semantica-json.
//! CPython 3.13.11 BaseException_str/repr supplies single-argument exception
//! text. Diagnostic-only errors never acquire source constructor provenance.
use super::*;

fn require_source_text(session: &QueryNumericSession) -> Result<(), RawNumericFault> {
    if session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1 {
        Ok(())
    } else {
        Err(RawNumericFault::UnsupportedSourceLiteralOperator(
            "SourceLiteralV1 text is not enabled",
        ))
    }
}

fn source_quote(text: &str) -> Result<String, RawNumericFault> {
    // Reuse the existing Unicode/quote owner, including its single-vs-double
    // quote selection and nonprintable-character escapes. This is not JSON
    // serialization and does not format an RDF term as N3.
    semantica_json::python_repr(&serde_json::Value::String(text.to_owned()))
        .map_err(|_| RawNumericFault::InvalidSourceOrigin)
}

fn compose(parts: &[&str]) -> Result<String, RawNumericFault> {
    let length = parts.iter().try_fold(0_usize, |total, part| {
        total
            .checked_add(part.len())
            .ok_or(NumericFault::ResourceLimit {
                operation: "allocation",
            })
    })?;
    let mut result = String::new();
    result
        .try_reserve(length)
        .map_err(|_| NumericFault::ResourceLimit {
            operation: "allocation",
        })?;
    for part in parts {
        result.push_str(part);
    }
    Ok(result)
}

fn identifier_repr(class: &str, value: &str) -> Result<String, RawNumericFault> {
    let quoted = source_quote(value)?;
    compose(&[class, "(", &quoted, ")"])
}

pub(super) fn exception_text(source: &SourceException) -> Cow<'_, str> {
    match &source.argument {
        SourceExceptionArgument::None => Cow::Borrowed("None"),
        SourceExceptionArgument::Text(text) => Cow::Borrowed(text),
        SourceExceptionArgument::DecimalSignalClasses(signals) => Cow::Owned(
            super::source_exception::decimal_signal_classes_repr(signals),
        ),
    }
}

fn exception_repr(source: &SourceException) -> Result<String, RawNumericFault> {
    let class = match source.class {
        SourceExceptionClass::Sparql => "SPARQLError",
        SourceExceptionClass::SparqlType => "SPARQLTypeError",
        SourceExceptionClass::NotBound => "NotBoundError",
        SourceExceptionClass::AlreadyBound => "AlreadyBound",
    };
    let argument = match &source.argument {
        SourceExceptionArgument::None => Cow::Borrowed("None"),
        SourceExceptionArgument::Text(text) => Cow::Owned(source_quote(text)?),
        SourceExceptionArgument::DecimalSignalClasses(signals) => Cow::Owned(
            super::source_exception::decimal_signal_classes_repr(signals),
        ),
    };
    compose(&[class, "(", &argument, ")"])
}

impl QueryNumericSession {
    /// Extremum.set_value calls Literal on the selected original object.
    /// Copying a Literal preserves its cache and does not re-budget input or
    /// output. New node/error Literals use the result's character budget.
    pub(crate) fn construct_source_extremum_result(
        &self,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<Option<RawExpressionValue>, RawNumericFault> {
        require_source_text(self)?;
        if matches!(value, SolutionValue::ExplicitNone) {
            return Ok(None);
        }
        if let SolutionValue::Rdf(raw) = value {
            if raw.literal().is_some() {
                return self.copy_source_literal(raw).map(Some);
            }
        }
        let text = self.source_text(value)?;
        if text.chars().count() > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit {
                operation: "output_characters",
            }
            .into());
        }
        let mut lexical = String::new();
        lexical
            .try_reserve(text.len())
            .map_err(|_| NumericFault::ResourceLimit {
                operation: "allocation",
            })?;
        lexical.push_str(&text);
        let mut result = self.construct_source_plain_result(lexical)?;
        if let SolutionValue::ReturnedError(error) = value {
            result.numeric = NumericCacheState::ErrorObject(error.clone());
            let LiteralSourceState::Literal(metadata) = &mut result.literal_source else {
                return Err(RawNumericFault::InvalidSourceOrigin);
            };
            metadata.cached_value_kind = SourceFact::Known(SourceLiteralCacheKind::ErrorObject);
            let (rdf, cache, metadata) = result.into_source_parts()?;
            result = RawExpressionValue::from_source_parts(rdf, cache, metadata)?
                .with_new_source_object();
        }
        Ok(Some(result))
    }

    /// Source str uses an RDF term's actual lexical text. It does not consume
    /// unknown datatype-presence or numeric-cache facts, unlike DISTINCT/repr.
    /// GROUP_CONCAT calls this only when its group is actually finalized.
    pub(crate) fn source_text<'v>(
        &self,
        value: &'v SolutionValue<RawExpressionValue>,
    ) -> Result<Cow<'v, str>, RawNumericFault> {
        require_source_text(self)?;
        if let SolutionValue::Rdf(raw) = value && let Some(v) = raw.source_variable() { return Ok(Cow::Borrowed(v.as_str())); }
        Ok(Cow::Borrowed(match value {
            SolutionValue::Rdf(raw) => match raw.rdf()? {
                Term::NamedNode(node) => node.as_str(),
                Term::BlankNode(node) => node.as_str(),
                Term::Literal(literal) => literal.value(),
                #[cfg(feature = "sparql-12")]
                Term::Triple(_) => {
                    return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "triple source text",
                    ));
                }
            },
            SolutionValue::ExplicitNone => "None",
            SolutionValue::ReturnedError(error) => {
                return Ok(exception_text(
                    error
                        .source_exception()
                        .ok_or(RawNumericFault::UnknownSourceErrorText)?,
                ));
            }
        }))
    }

    /// Source repr is used where the original operator builds a %r diagnostic.
    /// Literal metadata is read from its occurrence/object, never inferred
    /// from the RDF 1.1 datatype which merges plain text with xsd:string.
    pub(crate) fn source_repr(
        &self,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<String, RawNumericFault> {
        require_source_text(self)?;
        match value {
            SolutionValue::ExplicitNone => Ok("None".to_owned()),
            SolutionValue::ReturnedError(error) => exception_repr(
                error
                    .source_exception()
                    .ok_or(RawNumericFault::UnknownSourceErrorText)?,
            ),
            SolutionValue::Rdf(raw) => {
                // This validates actual source facts/representation. Repr
                // needs the same known datatype and language facts as a key,
                // but prints the original language spelling, not its key.
                source_identity_key(raw)?;
                if let Some(v) = raw.source_variable() { return identifier_repr("rdflib.term.Variable", v.as_str()); }
                match raw.rdf()? {
                    Term::NamedNode(node) => identifier_repr("rdflib.term.URIRef", node.as_str()),
                    Term::BlankNode(node) => identifier_repr("rdflib.term.BNode", node.as_str()),
                    Term::Literal(literal) => {
                        let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                            return Err(RawNumericFault::InvalidSourceOrigin);
                        };
                        let SourceFact::Known(language) = &metadata.language_original else {
                            return Err(RawNumericFault::UnknownSourceLiteralIdentity);
                        };
                        let lexical = source_quote(literal.value())?;
                        let language = language.as_deref().map(source_quote).transpose()?;
                        let datatype = match &metadata.datatype {
                            SourceDatatype::Unknown => {
                                return Err(RawNumericFault::UnknownSourceLiteralIdentity);
                            }
                            SourceDatatype::Absent => None,
                            SourceDatatype::Specified(datatype) => {
                                Some(identifier_repr("rdflib.term.URIRef", datatype.as_str())?)
                            }
                        };
                        let mut parts = vec!["rdflib.term.Literal(", &lexical];
                        if let Some(language) = &language {
                            parts.extend([", lang=", language]);
                        }
                        if let Some(datatype) = &datatype {
                            parts.extend([", datatype=", datatype]);
                        }
                        parts.push(")");
                        compose(&parts)
                    }
                    #[cfg(feature = "sparql-12")]
                    Term::Triple(_) => Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "triple source repr",
                    )),
                }
            }
        }
    }

    /// A new source Literal(str) result. Output limits apply here; applying
    /// DirectString's input limit again would reject valid joined results.
    pub(crate) fn construct_source_plain_result(
        &self,
        lexical: String,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        require_source_text(self)?;
        if lexical.chars().count() > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit {
                operation: "output_characters",
            }
            .into());
        }
        RawExpressionValue::from_source_parts(
            Literal::new_simple_literal(lexical).into(),
            NumericCacheState::NotNumeric,
            LiteralSourceState::Literal(SourceLiteralMetadata {
                datatype: SourceDatatype::Absent,
                language_original: SourceFact::Known(None),
                ill_typed: SourceFact::Known(None),
                cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::String),
            }),
        )
        .map(RawExpressionValue::with_new_source_object)
    }

    pub(super) fn numeric_type_error(
        &self,
        value: &SolutionValue<RawExpressionValue>,
        is_literal: bool,
    ) -> NumericEvaluationError {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return type_error(if is_literal {
                "Operand does not have a numeric datatype"
            } else {
                "Numeric operand is not a literal"
            });
        }
        let message = self.source_repr(value).and_then(|value| {
            compose(&[
                &value,
                if is_literal {
                    " does not have a numeric datatype!"
                } else {
                    " is not a literal!"
                },
            ])
        });
        match message {
            Ok(message) => NumericEvaluationError::Sparql {
                kind: SparqlErrorKind::Type,
                diagnostic: Cow::Owned(message.clone()),
                source: Some(SourceException {
                    class: SourceExceptionClass::SparqlType,
                    argument: SourceExceptionArgument::Text(message),
                }),
            },
            Err(error) => NumericEvaluationError::Fatal(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::BlankNode;

    fn session(limits: NumericLimits, source: bool) -> QueryNumericSession {
        QueryNumericSession::new_with_literal_semantics(
            DecimalContext::default(),
            limits,
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            if source {
                QueryLiteralSemantics::SourceLiteralV1
            } else {
                QueryLiteralSemantics::RdfIdentityV1
            },
        )
        .unwrap()
    }

    #[test]
    fn source_text_reads_lexical_without_cache_or_identity_recovery() {
        let session = session(NumericLimits::default(), true);
        let values = [
            (
                Term::Literal(Literal::new_typed_literal(
                    "01",
                    datatype(NumericKind::Integer),
                )),
                "01",
            ),
            (Term::NamedNode(NamedNode::new("urn:x").unwrap()), "urn:x"),
            (Term::BlankNode(BlankNode::new("node-a").unwrap()), "node-a"),
        ];
        for (rdf, text) in values {
            let value = SolutionValue::Rdf(RawExpressionValue::from_import(rdf));
            assert_eq!(session.source_text(&value).unwrap(), text);
        }
        let unknown = SolutionValue::Rdf(RawExpressionValue::from_import(
            Literal::new_simple_literal("x").into(),
        ));
        assert!(matches!(
            session.source_repr(&unknown),
            Err(RawNumericFault::UnknownSourceLiteralIdentity)
        ));
    }

    #[test]
    fn source_repr_reuses_python_quotes_and_original_language() {
        let session = session(NumericLimits::default(), true);
        for (lexical, datatype, language, expected) in [
            ("a'b", None, None, "rdflib.term.Literal(\"a'b\")"),
            (
                "a\nb\t\\",
                None,
                Some("EN".into()),
                "rdflib.term.Literal('a\\nb\\t\\\\', lang='EN')",
            ),
            (
                "猫\u{a0}",
                Some(NamedNode::new(format!("{XSD}string")).unwrap()),
                None,
                "rdflib.term.Literal('猫\\xa0', datatype=rdflib.term.URIRef('http://www.w3.org/2001/XMLSchema#string'))",
            ),
        ] {
            let value = session
                .construct_source_direct_string(lexical, datatype, language, true)
                .unwrap();
            assert_eq!(
                session.source_repr(&SolutionValue::Rdf(value)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn source_exception_text_requires_constructor_facts_and_preserves_identity() {
        let session = session(NumericLimits::default(), true);
        let error = ErrorValue::with_source(
            SparqlErrorKind::OtherSparql,
            "a diagnostic is not the source argument",
            SourceException {
                class: SourceExceptionClass::Sparql,
                argument: SourceExceptionArgument::None,
            },
        );
        assert_eq!(error, error.clone());
        assert_ne!(
            error,
            ErrorValue::with_source(
                error.kind(),
                error.diagnostic(),
                error.source_exception().unwrap().clone(),
            )
        );
        let value = SolutionValue::ReturnedError(error);
        assert_eq!(session.source_text(&value).unwrap(), "None");
        assert_eq!(session.source_repr(&value).unwrap(), "SPARQLError(None)");
        let opaque = SolutionValue::ReturnedError(ErrorValue::new("None"));
        assert!(matches!(
            session.source_text(&opaque),
            Err(RawNumericFault::UnknownSourceErrorText)
        ));
    }

    #[test]
    fn source_plain_result_has_output_budget_without_input_reconstruction() {
        let session = session(
            NumericLimits {
                max_input_characters: 6,
                max_output_characters: 7,
                ..NumericLimits::default()
            },
            true,
        );
        let value = session
            .construct_source_plain_result("猫🙂犬abcd".into())
            .unwrap();
        assert_eq!(
            value.rdf().unwrap(),
            &Term::Literal(Literal::new_simple_literal("猫🙂犬abcd"))
        );
        assert_eq!(
            value.literal_source(),
            &LiteralSourceState::Literal(SourceLiteralMetadata {
                datatype: SourceDatatype::Absent,
                language_original: SourceFact::Known(None),
                ill_typed: SourceFact::Known(None),
                cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::String),
            })
        );
        assert!(matches!(
            session.construct_source_plain_result("猫🙂犬abcdx".into()),
            Err(RawNumericFault::Core(NumericFault::ResourceLimit {
                operation: "output_characters"
            }))
        ));
    }

    #[test]
    fn numeric_type_error_uses_source_repr_and_keeps_default_diagnostic() {
        let source = session(NumericLimits::default(), true);
        let plain = source
            .construct_source_direct_string("x", None, None, true)
            .unwrap();
        let value = SolutionValue::Rdf(plain);
        let error = source
            .require_numeric(&value)
            .unwrap_err()
            .into_error_value()
            .unwrap();
        assert_eq!(
            source
                .source_text(&SolutionValue::ReturnedError(error.clone()))
                .unwrap(),
            "rdflib.term.Literal('x') does not have a numeric datatype!"
        );
        assert_eq!(error.kind(), SparqlErrorKind::Type);
        let legacy = session(NumericLimits::default(), false);
        let old = legacy
            .require_numeric(&value)
            .unwrap_err()
            .into_error_value()
            .unwrap();
        assert_eq!(old.diagnostic(), "Operand does not have a numeric datatype");
        assert!(old.source_exception().is_none());
    }

    #[test]
    fn numeric_division_source_facts_do_not_change_old_diagnostic() {
        for source in [false, true] {
            let error = divide_by_zero_error(if source {
                QueryLiteralSemantics::SourceLiteralV1
            } else {
                QueryLiteralSemantics::RdfIdentityV1
            })
            .into_error_value()
            .unwrap();
            assert_eq!(error.diagnostic(), "divide by 0");
            assert_eq!(error.source_exception().is_some(), source);
        }
    }
}
