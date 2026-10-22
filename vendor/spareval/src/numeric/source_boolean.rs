//! Actual SPARQL boolean tokens and Literal(bool) expression results.
//!
//! Fixed RDFLib 7.6.0 parser.py:416-418 constructs Literal(bool), whose object
//! constructor (term.py:667,695-705,2126) preserves the bool cache, uses the
//! canonical true/false lexical and xsd:boolean, and leaves ill_typed=None.
//! operators.py:1194-1195 reads that cache for EBV. The existing typed_ebv reads
//! the same canonical lexical; validation below requires exact agreement.
use super::*;
use oxrdf::vocab::xsd;

impl QueryNumericSession {
    /// Received well-formed XSD boolean strings. Original spelling survives
    /// normalize=false; string construction sets ill_typed=false, distinct
    /// from Literal(bool)'s None. Other source casts remain unqualified.
    pub(super) fn construct_source_boolean_string(
        &self,
        lexical: &str,
        normalize: bool,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.construct_source_boolean_lexical(lexical, normalize, SourceBooleanConstructor::DirectString)
    }
    /// Literal(existing, datatype=boolean) keeps spelling and ill_typed=None.
    /// Its constructor provenance is distinct from both string and bool inputs.
    pub(super) fn construct_source_boolean_existing(
        &self, lexical: &str,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.construct_source_boolean_lexical(lexical, false, SourceBooleanConstructor::ExistingLiteral)
    }
    fn construct_source_boolean_lexical(
        &self, lexical: &str, normalize: bool, constructor: SourceBooleanConstructor,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        self.check_text(lexical)?;
        let value = match lexical {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source noncanonical boolean string constructor",
                ));
            }
        };
        let output = if normalize {
            if value { "true" } else { "false" }
        } else {
            lexical
        };
        if output.len() > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit {
                operation: "output_characters",
            }
            .into());
        }
        let mut raw = RawExpressionValue::from_parts(
            Literal::new_typed_literal(output, xsd::BOOLEAN).into(),
            NumericCacheState::NotNumeric,
        );
        raw.source_boolean_constructor = constructor;
        raw.with_source_metadata(SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(xsd::BOOLEAN.into_owned()),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known((constructor == SourceBooleanConstructor::DirectString).then_some(false)),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Boolean(value)),
        })
    }
    pub(crate) fn construct_source_boolean_token(
        &self,
        parsed: &ParsedSourceLiteral,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "SourceLiteralV1 is not enabled",
            ));
        }
        if parsed.occurrence.is_some()
            || parsed.origin != SourceLiteralOrigin::QueryBooleanToken
            || parsed.datatype != SourceDatatype::Specified(xsd::BOOLEAN.into_owned())
            || parsed.language_original.is_some()
            || parsed.rdf.datatype() != xsd::BOOLEAN
            || parsed.rdf.language().is_some()
        {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let value = match parsed.rdf.value() {
            "true" => true,
            "false" => false,
            _ => return Err(RawNumericFault::InvalidSourceOrigin),
        };
        self.check_text(parsed.rdf.value())?;
        source_boolean_object(value)
    }

    /// Builtin_EXISTS constructs Literal(bool) as its result. This receives an
    /// actual bool, never a forged QueryBooleanToken or a datatype string cast.
    /// Each call creates a new source object; transport clones preserve it.
    pub(crate) fn construct_source_boolean_result(
        &self,
        value: bool,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "SourceLiteralV1 is not enabled",
            ));
        }
        if (if value { 4 } else { 5 }) > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit {
                operation: "output_characters",
            }
            .into());
        }
        source_boolean_object(value)
    }
}

fn source_boolean_object(value: bool) -> Result<RawExpressionValue, RawNumericFault> {
    RawExpressionValue::from_parts(
        Literal::new_typed_literal(if value { "true" } else { "false" }, xsd::BOOLEAN).into(),
        NumericCacheState::NotNumeric,
    )
    .with_source_metadata(SourceLiteralMetadata {
        datatype: SourceDatatype::Specified(xsd::BOOLEAN.into_owned()),
        language_original: SourceFact::Known(None),
        ill_typed: SourceFact::Known(None),
        cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Boolean(value)),
    })
}

/// Constructor-owned bool and received string-bool carriers. Metadata alone
/// cannot turn a bool result/import into a string-constructor occurrence.
pub(super) fn validate_source_boolean(
    raw: &RawExpressionValue,
    metadata: &SourceLiteralMetadata,
    value: bool,
) -> Result<(), RawNumericFault> {
    let Term::Literal(literal) = raw.rdf()? else {
        return Err(RawNumericFault::InvalidSourceOrigin);
    };
    if literal.datatype() != xsd::BOOLEAN
        || literal.language().is_some()
        || !(if raw.source_boolean_constructor != SourceBooleanConstructor::Object {
            matches!(
                (literal.value(), value),
                ("true" | "1", true) | ("false" | "0", false)
            )
        } else {
            literal.value() == if value { "true" } else { "false" }
        })
        || metadata.datatype != SourceDatatype::Specified(xsd::BOOLEAN.into_owned())
        || metadata.language_original != SourceFact::Known(None)
        || metadata.ill_typed
            != SourceFact::Known(if raw.source_boolean_constructor == SourceBooleanConstructor::DirectString {
                Some(false)
            } else {
                None
            })
        || metadata.cached_value_kind != SourceFact::Known(SourceLiteralCacheKind::Boolean(value))
        || !matches!(raw.numeric(), NumericCacheState::NotNumeric)
    {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(source: bool) -> QueryNumericSession {
        QueryNumericSession::new_with_literal_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
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

    fn token(lexical: &str) -> ParsedSourceLiteral {
        ParsedSourceLiteral {
            occurrence: None,
            rdf: Literal::new_typed_literal(lexical, xsd::BOOLEAN),
            datatype: SourceDatatype::Specified(xsd::BOOLEAN.into_owned()),
            language_original: None,
            origin: SourceLiteralOrigin::QueryBooleanToken,
        }
    }

    fn metadata(raw: &RawExpressionValue) -> &SourceLiteralMetadata {
        let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
            panic!("source token must retain literal facts");
        };
        metadata
    }

    #[test]
    fn source_boolean_tokens_preserve_object_facts_and_ebv() {
        let session = make_session(true);
        // Source-derived constructor/EBV facts, not candidate-derived outputs.
        for (lexical, expected) in [("false", false), ("true", true)] {
            let parsed = token(lexical);
            let raw = session.construct_source_literal(&parsed).unwrap();
            assert_eq!(raw.rdf().unwrap(), &Term::Literal(parsed.rdf));
            assert!(matches!(raw.numeric(), NumericCacheState::NotNumeric));
            assert_eq!(
                metadata(&raw),
                &SourceLiteralMetadata {
                    datatype: SourceDatatype::Specified(xsd::BOOLEAN.into_owned()),
                    language_original: SourceFact::Known(None),
                    ill_typed: SourceFact::Known(None),
                    cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Boolean(expected)),
                }
            );
            assert_eq!(session.typed_ebv(&raw).unwrap(), expected);
            let copied = session.copy_source_literal(&raw).unwrap();
            assert_eq!(metadata(&copied), metadata(&raw));
            assert_eq!(session.typed_ebv(&copied).unwrap(), expected);
            assert_eq!(session.flags(), DecimalSignals::EMPTY);
        }
    }

    #[test]
    fn source_boolean_token_rejects_forged_origin_and_non_token_spellings() {
        let session = make_session(true);
        // Active provenance boundary: legal boolean datatype strings such as
        // "1" are not actual parser true/false tokens in this constructor.
        for lexical in ["TRUE", "False", "1", "0", " false", "false ", ""] {
            assert!(matches!(
                session.construct_source_boolean_token(&token(lexical)),
                Err(RawNumericFault::InvalidSourceOrigin)
            ));
        }
        let valid = token("false");
        let mut wrong_origin = valid.clone();
        wrong_origin.origin = SourceLiteralOrigin::QueryTypedExistingLiteral;
        let mut absent_datatype = valid.clone();
        absent_datatype.datatype = SourceDatatype::Absent;
        let mut unknown_datatype = valid.clone();
        unknown_datatype.datatype = SourceDatatype::Unknown;
        let mut wrong_datatype = valid.clone();
        wrong_datatype.datatype = SourceDatatype::Specified(xsd::STRING.into_owned());
        let mut wrong_rdf = valid.clone();
        wrong_rdf.rdf = Literal::new_simple_literal("false");
        let mut wrong_language = valid.clone();
        wrong_language.language_original = Some("en".into());
        for parsed in [
            wrong_origin,
            absent_datatype,
            unknown_datatype,
            wrong_datatype,
            wrong_rdf,
            wrong_language,
        ] {
            assert!(matches!(
                session.construct_source_boolean_token(&parsed),
                Err(RawNumericFault::InvalidSourceOrigin)
            ));
        }
        assert!(matches!(
            make_session(false).construct_source_boolean_token(&valid),
            Err(RawNumericFault::UnsupportedSourceLiteralOperator(_))
        ));
        // This is a Rust admission boundary, not an original frozen outcome.
        // Well-formed strings are now received; malformed casts still gated.
        assert!(matches!(
            session.construct_source_direct_string(
                "FALSE",
                Some(xsd::BOOLEAN.into_owned()),
                None,
                true
            ),
            Err(RawNumericFault::UnsupportedSourceLiteralOperator(_))
        ));
    }

    #[test]
    fn source_boolean_carrier_rejects_inconsistent_cache_and_metadata() {
        let session = make_session(true);
        let raw = session.construct_source_literal(&token("false")).unwrap();
        let original = metadata(&raw).clone();
        let mut mismatched_cache = original.clone();
        mismatched_cache.cached_value_kind =
            SourceFact::Known(SourceLiteralCacheKind::Boolean(true));
        let mut mismatched_ill_typed = original.clone();
        mismatched_ill_typed.ill_typed = SourceFact::Known(Some(false));
        let mut unknown_language = original.clone();
        unknown_language.language_original = SourceFact::Unknown;
        for metadata in [mismatched_cache, mismatched_ill_typed, unknown_language] {
            assert!(matches!(
                raw.clone().with_source_metadata(metadata),
                Err(RawNumericFault::InvalidSourceOrigin)
            ));
        }
        assert!(matches!(
            RawExpressionValue::from_parts(raw.rdf().unwrap().clone(), NumericCacheState::CastFailed)
                .with_source_metadata(original),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
    }

    #[test]
    fn source_boolean_cached_order_comparison_remains_unreceived() {
        let session = make_session(true);
        let falsity = session.construct_source_literal(&token("false")).unwrap();
        let truth = session.construct_source_literal(&token("true")).unwrap();
        // Explicit active capability boundary, not the original bool ordering.
        for (left, right) in [(&falsity, &truth), (&truth, &falsity)] {
            assert!(matches!(
                session.source_order_key_less(
                    &SourceOrderKey::Rdf(left.clone()),
                    &SourceOrderKey::Rdf(right.clone())
                ),
                Err(NumericEvaluationError::Fatal(
                    RawNumericFault::UnsupportedSourceLiteralOperator("boolean source comparison")
                ))
            ));
            for kind in [SourceExtremumKind::Minimum, SourceExtremumKind::Maximum] {
                assert!(matches!(
                    session.source_extremum_candidate_wins(
                        kind,
                        &SolutionValue::Rdf(left.clone()),
                        &SolutionValue::Rdf(right.clone())
                    ),
                    Err(NumericEvaluationError::Fatal(
                        RawNumericFault::UnsupportedSourceLiteralOperator(
                            "boolean source comparison"
                        )
                    ))
                ));
            }
        }
        // Existing tuple identity short-circuit does not read cached ordering.
        assert!(
            !session
                .source_order_key_less(
                    &SourceOrderKey::Rdf(falsity.clone()),
                    &SourceOrderKey::Rdf(falsity)
                )
                .unwrap()
        );
    }
    #[test]
    fn source_temporal_domain_constructor_origin_is_checked() {
        let session = make_session(true);
        let mut parsed = token("1");
        parsed.origin = SourceLiteralOrigin::QueryTypedExistingLiteral;
        let raw = session.construct_source_graph_literal(&parsed).unwrap();
        assert_eq!(raw.rdf().unwrap(), &Term::Literal(parsed.rdf.clone()));
        assert_eq!(metadata(&raw).ill_typed, SourceFact::Known(None));
        let mut forged = metadata(&raw).clone();
        forged.ill_typed = SourceFact::Known(Some(false));
        assert!(raw.clone().with_source_metadata(forged).is_err());
        assert!(RawExpressionValue::from_source_parts(raw.rdf().unwrap().clone(), raw.numeric().clone(), raw.literal_source().clone()).is_err());
        assert!(session.construct_source_literal(&parsed).is_err()); // Older admission unchanged.
        parsed.language_original = Some("en".into());
        assert!(session.construct_source_graph_literal(&parsed).is_err());
        assert!(make_session(false).construct_source_boolean_existing("1").is_err());
        assert!(session.construct_source_boolean_existing("FALSE").is_err());
    }

}
