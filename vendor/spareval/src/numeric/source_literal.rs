//! Source Literal facts and explicit source-identity keys.
//!
//! These values do not change physical RDF Eq/Hash. Facts originate in the
//! actual constructor or an explicit trusted object declaration; RDF imports
//! have unknown facts. No parser, query evaluator, or global value map lives here.
use super::*;
use oxrdf::BlankNode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueryLiteralSemantics {
    RdfIdentityV1,
    SourceLiteralV1,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceFact<T> {
    Unknown,
    Known(T),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceLiteralCacheKind {
    String,
    None,
    Numeric,
    ErrorObject,
    Boolean(bool),
    DateTime(Option<semantica_temporal::DateTime>),
    Date(Option<semantica_temporal::Date>),
    Time(Option<semantica_temporal::Time>),
    DayTimeDuration(Option<semantica_temporal::Timedelta>),
    Duration(std::sync::Arc<super::source_duration::SourceDurationCache>),
    /// Known None from a datatype absent from the fixed converter catalogue.
    UnmappedDatatype,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceLiteralMetadata {
    pub(crate) datatype: SourceDatatype,
    pub(crate) language_original: SourceFact<Option<String>>,
    pub(crate) ill_typed: SourceFact<Option<bool>>,
    pub(crate) cached_value_kind: SourceFact<SourceLiteralCacheKind>,
}
impl SourceLiteralMetadata {
    pub(crate) fn unknown() -> Self {
        Self {
            datatype: SourceDatatype::Unknown,
            language_original: SourceFact::Unknown,
            ill_typed: SourceFact::Unknown,
            cached_value_kind: SourceFact::Unknown,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LiteralSourceState {
    NotLiteral,
    Literal(SourceLiteralMetadata),
}
impl LiteralSourceState {
    pub(crate) fn for_import(rdf: &Term) -> Self {
        if matches!(rdf, Term::Literal(_)) {
            Self::Literal(SourceLiteralMetadata::unknown())
        } else {
            Self::NotLiteral
        }
    }
}

/// Equality is the fixed source term.Literal identity, never Python's random
/// hash output and never the source value-comparison/ordering operation.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum SourceTermKey {
    NamedNode(NamedNode),
    BlankNode(BlankNode),
    Variable(oxrdf::Variable),
    Literal {
        lexical: String,
        datatype: Option<NamedNode>,
        language_lower: Option<String>,
    },
}

impl RawExpressionValue {
    pub(crate) fn literal_source(&self) -> &LiteralSourceState {
        &self.literal_source
    }
    pub(crate) fn into_source_parts(self) -> Result<(Term, NumericCacheState, LiteralSourceState), RawNumericFault> {
        match self.source_term {
            RawSourceTerm::Rdf(rdf) => Ok((rdf, self.numeric, self.literal_source)),
            RawSourceTerm::Variable(_) => Err(RawNumericFault::SourceTermNotRdf("Variable")),
        }
    }
    /// Validate value facts; detached parts do not assert object identity.
    pub(crate) fn from_source_parts(
        rdf: Term,
        numeric: NumericCacheState,
        literal_source: LiteralSourceState,
    ) -> Result<Self, RawNumericFault> {
        let value = Self {
            source_term: RawSourceTerm::Rdf(rdf),
            numeric,
            literal_source,
            source_object: None,
            source_datatype_object: None,
            source_boolean_constructor: SourceBooleanConstructor::Object,
            source_string_cast_language: false,
        };
        validate_source_representation(&value)?;
        Ok(value)
    }
    pub(crate) fn with_source_metadata(
        mut self,
        metadata: SourceLiteralMetadata,
    ) -> Result<Self, RawNumericFault> {
        self.literal_source = LiteralSourceState::Literal(metadata);
        self.source_object = None;
        self.source_datatype_object = None;
        validate_source_representation(&self)?;
        Ok(self.with_new_source_object())
    }
}

pub(super) fn validate_source_representation(raw: &RawExpressionValue) -> Result<(), RawNumericFault> {
    let Some(literal) = raw.literal() else {
        return if matches!(raw.literal_source(), LiteralSourceState::NotLiteral)
            && matches!(raw.numeric(), NumericCacheState::NotNumeric)
        {
            Ok(())
        } else {
            Err(RawNumericFault::InvalidSourceOrigin)
        };
    };
    #[cfg(feature = "sparql-12")]
    if literal.direction().is_some() {
        return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
            "directional source literal",
        ));
    }
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        return Err(RawNumericFault::InvalidSourceOrigin);
    };
    // RDF cannot physically carry both a language and xsd:string. This
    // private constructor-owned projection retains both source facts while
    // detached/imported parts cannot assert that constructor provenance.
    if raw.source_string_cast_language
        && !(literal.language().is_none()
            && literal.datatype() == oxrdf::vocab::xsd::STRING
            && metadata.datatype
                == SourceDatatype::Specified(oxrdf::vocab::xsd::STRING.into_owned())
            && matches!(&metadata.language_original, SourceFact::Known(Some(language)) if source_language_tag_is_valid(language))
            && metadata.ill_typed == SourceFact::Known(None)
            && metadata.cached_value_kind == SourceFact::Known(SourceLiteralCacheKind::String)
            && matches!(raw.numeric(), NumericCacheState::NotNumeric))
    {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    match &metadata.datatype {
        SourceDatatype::Unknown => (),
        SourceDatatype::Absent => {
            // RDF 1.1 collapses plain text into xsd:string and language text
            // into rdf:langString. Only explicit source facts grant Absent.
            if literal.language().is_none() && literal.datatype().as_str() != format!("{XSD}string")
            {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
        }
        SourceDatatype::Specified(datatype) => {
            if datatype.as_ref() != literal.datatype() || literal.language().is_some() {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
        }
    }
    if let SourceFact::Known(language) = &metadata.language_original {
        if language
            .as_ref()
            .is_some_and(|tag| !source_language_tag_is_valid(tag))
            || (!raw.source_string_cast_language
                && language.as_ref().map(|x| x.to_ascii_lowercase()).as_deref()
                    != literal.language())
        {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
    }
    // A real exception cache is never silently reclassified as text or
    // unknown cache metadata during transport.
    if matches!(raw.numeric(), NumericCacheState::ErrorObject(_))
        && metadata.cached_value_kind != SourceFact::Known(SourceLiteralCacheKind::ErrorObject)
    {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    match &metadata.cached_value_kind {
        SourceFact::Unknown => (),
        SourceFact::Known(SourceLiteralCacheKind::Boolean(value)) => {
            super::source_boolean::validate_source_boolean(raw, metadata, *value)?;
        }
        SourceFact::Known(SourceLiteralCacheKind::Duration(value)) => {
            super::source_duration::validate(raw, metadata, value)?;
        }
        SourceFact::Known(SourceLiteralCacheKind::DateTime(value)) => {
            super::source_datetime::validate_source_datetime(raw, metadata, *value)?;
        }
        SourceFact::Known(
            SourceLiteralCacheKind::Date(_)
            | SourceLiteralCacheKind::Time(_)
            | SourceLiteralCacheKind::DayTimeDuration(_)
            | SourceLiteralCacheKind::UnmappedDatatype,
        ) => {
            super::source_datetime::validate_source_graph_value(raw, metadata)?;
        }
        SourceFact::Known(SourceLiteralCacheKind::ErrorObject) => {
            let NumericCacheState::ErrorObject(error) = raw.numeric() else {
                return Err(RawNumericFault::InvalidSourceOrigin);
            };
            if metadata.datatype != SourceDatatype::Absent
                || metadata.language_original != SourceFact::Known(None)
                || metadata.ill_typed != SourceFact::Known(None)
            {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
            let source = error
                .source_exception()
                .ok_or(RawNumericFault::InvalidSourceOrigin)?;
            let text = super::source_text::exception_text(source);
            if literal.value() != text {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
        }
        SourceFact::Known(SourceLiteralCacheKind::Numeric) => {
            let NumericCacheState::Known(cache) = raw.numeric() else {
                return Err(RawNumericFault::InvalidSourceOrigin);
            };
            if cache.datatype().as_ref() != literal.datatype()
                || numeric_kind(cache.datatype()).is_none()
            {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
            // The object's cached Python numeric family may differ from the
            // explicitly supplied numeric datatype. Do not re-parse it.
        }
        SourceFact::Known(SourceLiteralCacheKind::None) => {
            if !matches!(raw.numeric(), NumericCacheState::CastFailed)
                || numeric_kind(&literal.datatype().into_owned()).is_none()
            {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
        }
        SourceFact::Known(SourceLiteralCacheKind::String) => {
            if !matches!(raw.numeric(), NumericCacheState::NotNumeric)
                || (literal.language().is_none()
                    && literal.datatype().as_str() != format!("{XSD}string")
                    && literal.datatype() != oxrdf::vocab::xsd::ANY_URI)
            {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
        }
    }
    Ok(())
}

pub(crate) fn source_identity_key(
    raw: &RawExpressionValue,
) -> Result<SourceTermKey, RawNumericFault> {
    validate_source_representation(raw)?;
    if let Some(v) = raw.source_variable() { return Ok(SourceTermKey::Variable(v.clone())); }
    Ok(match raw.rdf()? {
        Term::NamedNode(node) => SourceTermKey::NamedNode(node.clone()),
        Term::BlankNode(node) => SourceTermKey::BlankNode(node.clone()),
        Term::Literal(literal) => {
            let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                return Err(RawNumericFault::InvalidSourceOrigin);
            };
            let datatype = match &metadata.datatype {
                SourceDatatype::Unknown => {
                    return Err(RawNumericFault::UnknownSourceLiteralIdentity);
                }
                SourceDatatype::Absent => None,
                SourceDatatype::Specified(datatype) => Some(datatype.clone()),
            };
            let SourceFact::Known(language) = &metadata.language_original else {
                return Err(RawNumericFault::UnknownSourceLiteralIdentity);
            };
            SourceTermKey::Literal {
                lexical: literal.value().to_owned(),
                datatype,
                language_lower: language.as_ref().map(|x| x.to_ascii_lowercase()),
            }
        }
        #[cfg(feature = "sparql-12")]
        Term::Triple(_) => {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "triple source identity",
            ));
        }
    })
}

/// Reached source identity comparisons, without eagerly reading metadata that
/// a different family or datatype makes irrelevant. Object identity is checked
/// by the actual carrier before this value-level operation.
pub(crate) fn source_context_equal(
    left: &RawExpressionValue,
    right: &RawExpressionValue,
) -> Result<bool, RawNumericFault> {
    if left.source_variable().is_some() || right.source_variable().is_some() {
        return Ok(left.source_variable() == right.source_variable());
    }
    match (left.rdf()?, right.rdf()?) {
        (Term::NamedNode(a), Term::NamedNode(b)) => Ok(a == b),
        (Term::BlankNode(a), Term::BlankNode(b)) => Ok(a == b),
        (Term::Literal(a), Term::Literal(b)) => {
            let (LiteralSourceState::Literal(am), LiteralSourceState::Literal(bm)) =
                (left.literal_source(), right.literal_source())
            else {
                return Err(RawNumericFault::InvalidSourceOrigin);
            };
            if am.datatype == SourceDatatype::Unknown || bm.datatype == SourceDatatype::Unknown {
                return Err(RawNumericFault::UnknownSourceLiteralIdentity);
            }
            if am.datatype != bm.datatype {
                return Ok(false);
            }
            let SourceFact::Known(al) = &am.language_original else {
                return Err(RawNumericFault::UnknownSourceLiteralIdentity);
            };
            let SourceFact::Known(bl) = &bm.language_original else {
                return Err(RawNumericFault::UnknownSourceLiteralIdentity);
            };
            let al = al.as_deref().filter(|s| !s.is_empty());
            let bl = bl.as_deref().filter(|s| !s.is_empty());
            let same_language = match (al, bl) {
                (None, None) => true,
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            };
            Ok(same_language && a.value() == b.value())
        }
        #[cfg(feature = "sparql-12")]
        (Term::Triple(_), _) | (_, Term::Triple(_)) => Err(
            RawNumericFault::UnsupportedSourceLiteralOperator("triple source equality"),
        ),
        _ => Ok(false),
    }
}

// Fixed term.py:111-115 regex ^[a-zA-Z]+(?:-[a-zA-Z0-9]+)*$ uses
// Python match(), not fullmatch(). CPython 3.13.11 _parser.py:878-879
// selects AT_END, and sre_lib.h:32-35 permits one unconsumed final LF.
// The caller handles empty-string-as-no-language before this predicate.
// No process regex engine, BCP47 policy, or Unicode whitespace trimming.
fn source_language_tag_is_valid(tag: &str) -> bool {
    let matched = tag.strip_suffix('\n').unwrap_or(tag);
    let mut pieces = matched.split('-');
    let Some(first) = pieces.next() else {
        return false;
    };
    !first.is_empty()
        && first.bytes().all(|byte| byte.is_ascii_alphabetic())
        && pieces.all(|piece| {
            !piece.is_empty() && piece.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

impl QueryNumericSession {
    pub(super) fn require_source_literal_semantics(&self) -> Result<(), RawNumericFault> {
        if self.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1 {
            Ok(())
        } else {
            Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "SourceLiteralV1 is not enabled",
            ))
        }
    }

    /// Literal.__bool__ reads its already constructed Python cache. This is
    /// translation-time object truth, not SPARQL EBV or a datatype cast.
    /// In particular a failed cast (cache None) falls back to lexical length,
    /// and Decimal NaN/sNaN are true without any decimal operation or flags.
    pub(crate) fn source_literal_python_truth(
        &self,
        raw: &RawExpressionValue,
    ) -> Result<bool, RawNumericFault> {
        self.require_source_literal_semantics()?;
        validate_source_representation(raw)?;
        let (Term::Literal(literal), LiteralSourceState::Literal(metadata)) =
            (raw.rdf()?, raw.literal_source())
        else {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "source literal Python truth requires a Literal",
            ));
        };
        match &metadata.cached_value_kind {
            SourceFact::Unknown => Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "source literal Python truth requires cached value facts",
            )),
            SourceFact::Known(SourceLiteralCacheKind::Boolean(value)) => Ok(*value),
            SourceFact::Known(SourceLiteralCacheKind::DateTime(value)) => {
                Ok(value.is_some() || !literal.value().is_empty())
            }
            SourceFact::Known(SourceLiteralCacheKind::Date(value)) => {
                Ok(value.is_some() || !literal.value().is_empty())
            }
            SourceFact::Known(SourceLiteralCacheKind::Time(value)) => {
                Ok(value.is_some() || !literal.value().is_empty())
            }
            SourceFact::Known(SourceLiteralCacheKind::Duration(value)) => {
                Ok(value.value.as_ref().map_or(!literal.value().is_empty(), semantica_temporal::DurationValue::python_truth))
            }
            SourceFact::Known(SourceLiteralCacheKind::DayTimeDuration(value)) => {
                Ok(value.map_or(!literal.value().is_empty(), |v| !v.is_zero()))
            }
            SourceFact::Known(SourceLiteralCacheKind::UnmappedDatatype) => {
                Ok(!literal.value().is_empty())
            }
            SourceFact::Known(SourceLiteralCacheKind::String | SourceLiteralCacheKind::None) => {
                Ok(!literal.value().is_empty())
            }
            SourceFact::Known(SourceLiteralCacheKind::ErrorObject) => Ok(true),
            SourceFact::Known(SourceLiteralCacheKind::Numeric) => {
                let NumericCacheState::Known(cache) = raw.numeric() else {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                };
                Ok(match cache.value() {
                    NumericValue::Integer(value) => value != &BigInt::from(0),
                    NumericValue::Decimal(value) => value.python_truth(),
                    NumericValue::Binary64(value) => *value != 0.0,
                })
            }
        }
    }

    /// Construct one parser numeric occurrence and retain that very result.
    /// The new prepared-literal bridge stores this value for later phase and
    /// compiler use instead of re-materializing its cache from an RDF key.
    pub(crate) fn construct_source_numeric_token(
        &self,
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        magnitude: &str,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.construct_source_numeric_token_phase(kind, sign, magnitude, false)
    }
    pub(super) fn construct_source_graph_numeric_token(
        &self,
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        magnitude: &str,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.construct_source_numeric_token_phase(kind, sign, magnitude, true)
    }
    fn construct_source_numeric_token_phase(
        &self,
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        magnitude: &str,
        source_error: bool,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        let raw =
            self.construct_numeric_token_with_source_error(kind, sign, magnitude, source_error)?;
        let Some(literal) = raw.literal() else {
            return Err(RawNumericFault::InvalidSourceOrigin);
        };
        let datatype = literal.datatype().into_owned();
        if !matches!(raw.numeric(), NumericCacheState::Known(_)) {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        raw.with_source_metadata(SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(datatype),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(if sign == NumericTokenSign::Negative {
                None
            } else {
                Some(false)
            }),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
        })
    }

    pub(crate) fn construct_source_direct_string(
        &self,
        lexical: &str,
        datatype: Option<NamedNode>,
        language: Option<String>,
        normalize: bool,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        self.check_text(lexical)?;
        // term.Literal.__new__ first normalizes the empty language to None.
        let language = language.filter(|x| !x.is_empty());
        if datatype.is_some() && language.is_some() {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        if let Some(language) = &language {
            self.check_text(language)?;
            if !source_language_tag_is_valid(language) {
                return Err(RawNumericFault::SourceLiteralLanguageValueError);
            }
        }
        if datatype
            .as_ref()
            .is_some_and(|dt| dt.as_str() == format!("{XSD}boolean"))
        {
            return self.construct_source_boolean_string(lexical, normalize);
        }
        let (raw, source_datatype, ill_typed, kind) = if let Some(datatype) = datatype {
            self.check_text(datatype.as_str())?;
            if numeric_kind(&datatype).is_some() {
                let raw = self.construct_literal(
                    &Literal::new_typed_literal(lexical, datatype.clone()),
                    NumericInputOrigin::DirectString { normalize },
                )?;
                let ill_typed = !source_numeric_well_formed(lexical, &datatype, raw.numeric());
                let kind = match raw.numeric() {
                    NumericCacheState::Known(_) => SourceLiteralCacheKind::Numeric,
                    NumericCacheState::CastFailed => SourceLiteralCacheKind::None,
                    _ => return Err(RawNumericFault::InvalidSourceOrigin),
                };
                (
                    raw,
                    SourceDatatype::Specified(datatype),
                    Some(ill_typed),
                    kind,
                )
            } else if datatype.as_str() == format!("{XSD}string") {
                (
                    RawExpressionValue::from_parts(
                        Literal::new_typed_literal(lexical, datatype.clone()).into(),
                        NumericCacheState::NotNumeric,
                    ),
                    SourceDatatype::Specified(datatype),
                    Some(false),
                    SourceLiteralCacheKind::String,
                )
            } else {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source string datatype constructor",
                ));
            }
        } else {
            let literal = match &language {
                // The fixed source constructor accepts a broader domain than
                // BCP47, including a single terminal LF. This is an explicitly
                // source-validated private string carrier, not a declaration of
                // standards-valid RDF language syntax. Keep the original facts
                // separately; the physical tag is ASCII-lowercase, never trimmed.
                Some(language) => Literal::new_language_tagged_literal_unchecked(
                    lexical,
                    language.to_ascii_lowercase(),
                ),
                None => Literal::new_simple_literal(lexical),
            };
            (
                RawExpressionValue::from_parts(literal.into(), NumericCacheState::NotNumeric),
                SourceDatatype::Absent,
                None,
                SourceLiteralCacheKind::String,
            )
        };
        raw.with_source_metadata(SourceLiteralMetadata {
            datatype: source_datatype,
            language_original: SourceFact::Known(language),
            ill_typed: SourceFact::Known(ill_typed),
            cached_value_kind: SourceFact::Known(kind),
        })
    }

    pub(crate) fn construct_source_literal(
        &self,
        parsed: &ParsedSourceLiteral,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        if parsed.occurrence.is_some() {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        self.check_text(parsed.rdf.value())?;
        let datatype = match &parsed.datatype {
            SourceDatatype::Unknown => return Err(RawNumericFault::InvalidSourceOrigin),
            SourceDatatype::Absent => None,
            SourceDatatype::Specified(datatype) => Some(datatype.clone()),
        };
        let (mut raw, ill_typed, cache_kind) = match &parsed.origin {
            SourceLiteralOrigin::QueryPlain | SourceLiteralOrigin::QueryLanguageExistingLiteral => {
                if datatype.is_some()
                    || (matches!(&parsed.origin, SourceLiteralOrigin::QueryPlain)
                        && parsed.language_original.is_some())
                    || (matches!(
                        &parsed.origin,
                        SourceLiteralOrigin::QueryLanguageExistingLiteral
                    ) && parsed.language_original.is_none())
                {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                let raw = self.construct_source_direct_string(
                    parsed.rdf.value(),
                    None,
                    parsed.language_original.clone(),
                    false,
                )?;
                (raw, None, SourceLiteralCacheKind::String)
            }
            SourceLiteralOrigin::QueryTypedExistingLiteral => {
                let Some(datatype) = &datatype else {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                };
                if parsed.language_original.is_some() {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                let raw = if numeric_kind(datatype).is_some() {
                    self.construct_literal(
                        &parsed.rdf,
                        NumericInputOrigin::QueryTypedExistingLiteral,
                    )?
                } else if datatype.as_str() == format!("{XSD}string") {
                    RawExpressionValue::from_parts(
                        parsed.rdf.clone().into(),
                        NumericCacheState::NotNumeric,
                    )
                } else {
                    return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "query source datatype constructor",
                    ));
                };
                let kind = match raw.numeric() {
                    NumericCacheState::Known(_) => SourceLiteralCacheKind::Numeric,
                    NumericCacheState::CastFailed => SourceLiteralCacheKind::None,
                    NumericCacheState::NotNumeric => SourceLiteralCacheKind::String,
                    NumericCacheState::UnknownNormalized | NumericCacheState::ErrorObject(_) => {
                        return Err(RawNumericFault::InvalidSourceOrigin);
                    }
                };
                (raw, None, kind)
            }
            SourceLiteralOrigin::QueryNumericToken {
                kind,
                sign,
                magnitude,
            } => {
                if parsed.language_original.is_some()
                    || !valid_magnitude(*kind, magnitude)
                    || (*kind == NumericTokenKind::Decimal && *sign == NumericTokenSign::Negative)
                {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                let expected = super::datatype(match kind {
                    NumericTokenKind::Integer => NumericKind::Integer,
                    NumericTokenKind::Decimal => NumericKind::Decimal,
                    NumericTokenKind::Double => NumericKind::Double,
                });
                if datatype.as_ref() != Some(&expected) {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                // The real numeric callback has already executed exactly once.
                // Reuse only its qualified lexical/cache closure; never call
                // construct_numeric_token or recreate its source side effects.
                let raw =
                    self.construct_literal(&parsed.rdf, NumericInputOrigin::PreparedQueryLiteral)?;
                if !matches!(raw.numeric(), NumericCacheState::Known(_)) {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                let ill_typed = if *sign == NumericTokenSign::Negative {
                    None
                } else {
                    Some(false)
                };
                (raw, ill_typed, SourceLiteralCacheKind::Numeric)
            }
            SourceLiteralOrigin::QueryBooleanToken => {
                return self.construct_source_boolean_token(parsed);
            }
        };
        if raw.rdf()? != &Term::Literal(parsed.rdf.clone()) {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        if matches!(
            &parsed.origin,
            SourceLiteralOrigin::QueryNumericToken { .. }
        ) {
            if let NumericCacheState::Known(cache) = &mut raw.numeric {
                Arc::make_mut(cache).origin = NumericValueOrigin::QueryNumericToken;
            }
        }
        raw.with_source_metadata(SourceLiteralMetadata {
            datatype: parsed.datatype.clone(),
            language_original: SourceFact::Known(parsed.language_original.clone()),
            ill_typed: SourceFact::Known(ill_typed),
            cached_value_kind: SourceFact::Known(cache_kind),
        })
    }

    /// operators.default_cast uses Literal(existing, datatype=XSD.string).
    /// Existing Literal leaves ill_typed=None and preserves language spelling;
    /// a URIRef takes the direct string constructor with ill_typed=false.
    pub(super) fn construct_source_string_cast(
        &self,
        raw: &RawExpressionValue,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        source_identity_key(raw)?;
        let (text, language, ill_typed) = match raw.rdf()? {
            Term::NamedNode(iri) => (iri.as_str(), None, Some(false)),
            Term::Literal(literal) => {
                let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                };
                let SourceFact::Known(language) = &metadata.language_original else {
                    return Err(RawNumericFault::UnknownSourceLiteralIdentity);
                };
                (literal.value(), language.clone(), None)
            }
            _ => return Err(RawNumericFault::InvalidSourceOrigin),
        };
        if text.chars().count() > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit {
                operation: "output_characters",
            }
            .into());
        }
        let mut result = RawExpressionValue::from_parts(
            Literal::new_typed_literal(text, oxrdf::vocab::xsd::STRING).into(),
            NumericCacheState::NotNumeric,
        );
        result.source_string_cast_language = language.is_some();
        result.with_source_metadata(SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(oxrdf::vocab::xsd::STRING.into_owned()),
            language_original: SourceFact::Known(language),
            ill_typed: SourceFact::Known(ill_typed),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::String),
        })
    }

    pub(crate) fn copy_source_literal(
        &self,
        raw: &RawExpressionValue,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "Literal(non-Literal object)",
            ));
        };
        let mut metadata = metadata.clone();
        metadata.ill_typed = SourceFact::Known(None);
        // Literal(existing) creates a new literal but reuses the exact stored
        // datatype object when no datatype override is supplied (term.py).
        let mut copied = raw.clone().with_source_metadata(metadata)?;
        copied.source_datatype_object = raw.source_datatype_object.clone();
        Ok(copied)
    }

    pub(crate) fn known_source_cached(
        &self,
        rdf: Term,
        value: NumericValue,
        metadata: SourceLiteralMetadata,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        if !matches!(metadata.datatype, SourceDatatype::Specified(_))
            || metadata.language_original != SourceFact::Known(None)
            || metadata.cached_value_kind != SourceFact::Known(SourceLiteralCacheKind::Numeric)
        {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        self.known_cached(rdf, value)?
            .with_source_metadata(metadata)
    }
}

// Fixed term.py 1869-1962 / 2209-2221: only these integer subtypes have
// dedicated well-formed checkers. In particular unsignedLong has no maximum
// check and long uses the ordinary non-None-value fallback in this source.
fn source_numeric_well_formed(
    lexical: &str,
    datatype: &NamedNode,
    cache: &NumericCacheState,
) -> bool {
    let NumericCacheState::Known(cache) = cache else {
        return false;
    };
    let name = datatype.as_str().strip_prefix(XSD).unwrap_or("");
    let NumericValue::Integer(value) = cache.value() else {
        return true;
    };
    let in_range =
        |low: i64, high: i64| value >= &BigInt::from(low) && value <= &BigInt::from(high);
    match name {
        "int" => !lexical.is_empty() && in_range(-2147483648, 2147483647),
        "unsignedInt" => !lexical.is_empty() && in_range(0, 4294967295),
        "short" => !lexical.is_empty() && in_range(-32768, 32767),
        "unsignedShort" => !lexical.is_empty() && in_range(0, 65535),
        "byte" => !lexical.is_empty() && in_range(-128, 127),
        "unsignedByte" => !lexical.is_empty() && in_range(0, 255),
        "unsignedLong" => !lexical.is_empty() && value >= &BigInt::from(0),
        "nonNegativeInteger" => value >= &BigInt::from(0),
        "positiveInteger" => value > &BigInt::from(0),
        "nonPositiveInteger" => value <= &BigInt::from(0),
        "negativeInteger" => value < &BigInt::from(0),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_session() -> QueryNumericSession {
        QueryNumericSession::new_with_literal_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
            Some(4300),
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
        )
        .unwrap()
    }
    fn metadata(raw: &RawExpressionValue) -> &SourceLiteralMetadata {
        match raw.literal_source() {
            LiteralSourceState::Literal(metadata) => metadata,
            _ => panic!("expected literal metadata"),
        }
    }
    #[derive(Default)]
    struct HashBytes(Vec<u8>);
    impl Hasher for HashBytes {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }
    fn bytes(value: &impl Hash) -> Vec<u8> {
        let mut hasher = HashBytes::default();
        value.hash(&mut hasher);
        hasher.0
    }

    #[test]
    fn source_identity_keeps_physical_hash_bytes_and_unknown_transport_separate() {
        let session = source_session();
        let plain = session
            .construct_source_direct_string("a", None, None, true)
            .unwrap();
        let typed = session
            .construct_source_direct_string(
                "a",
                Some(NamedNode::new_unchecked(format!("{XSD}string"))),
                None,
                true,
            )
            .unwrap();
        let unknown = RawExpressionValue::from_import(plain.rdf().unwrap().clone());
        // Old physical Eq/Hash is an unchanged RDF-only operation.
        assert_eq!(plain, typed);
        assert_eq!(plain, unknown);
        assert_eq!(bytes(&plain), bytes(plain.rdf().unwrap()));
        assert_eq!(bytes(&typed), bytes(typed.rdf().unwrap()));
        assert_eq!(bytes(&unknown), bytes(unknown.rdf().unwrap()));
        assert_ne!(
            source_identity_key(&plain).unwrap(),
            source_identity_key(&typed).unwrap()
        );
        assert_eq!(
            source_identity_key(&unknown),
            Err(RawNumericFault::UnknownSourceLiteralIdentity)
        );
        let (rdf, cache, facts) = unknown.into_source_parts().unwrap();
        let transported = RawExpressionValue::from_source_parts(rdf, cache, facts).unwrap();
        assert_eq!(metadata(&transported), &SourceLiteralMetadata::unknown());
    }

    #[test]
    fn source_metadata_factory_rejects_datatype_language_and_cache_mismatches() {
        let session = source_session();
        let lang = session
            .construct_source_direct_string("a", None, Some("EN".into()), true)
            .unwrap();
        let mut wrong = metadata(&lang).clone();
        wrong.language_original = SourceFact::Known(Some("fr".into()));
        assert_eq!(
            lang.clone().with_source_metadata(wrong).unwrap_err(),
            RawNumericFault::InvalidSourceOrigin
        );
        let mut wrong = metadata(&lang).clone();
        wrong.datatype =
            SourceDatatype::Specified(NamedNode::new_unchecked(format!("{XSD}string")));
        assert_eq!(
            lang.clone().with_source_metadata(wrong).unwrap_err(),
            RawNumericFault::InvalidSourceOrigin
        );
        let mut wrong = metadata(&lang).clone();
        wrong.cached_value_kind = SourceFact::Known(SourceLiteralCacheKind::Numeric);
        assert_eq!(
            lang.with_source_metadata(wrong).unwrap_err(),
            RawNumericFault::InvalidSourceOrigin
        );
        let numeric = session
            .construct_source_direct_string("1", Some(datatype(NumericKind::Integer)), None, true)
            .unwrap();
        let mut wrong = metadata(&numeric).clone();
        wrong.datatype = SourceDatatype::Absent;
        assert_eq!(
            numeric.with_source_metadata(wrong).unwrap_err(),
            RawNumericFault::InvalidSourceOrigin
        );
    }

    #[test]
    fn source_copy_resets_ill_typed_while_transport_preserves_value_facts() {
        let session = source_session();
        let invalid = session
            .construct_source_direct_string(
                "oops",
                Some(datatype(NumericKind::Integer)),
                None,
                true,
            )
            .unwrap();
        assert_eq!(metadata(&invalid).ill_typed, SourceFact::Known(Some(true)));
        assert_eq!(
            metadata(&invalid).cached_value_kind,
            SourceFact::Known(SourceLiteralCacheKind::None)
        );
        let (rdf, cache, facts) = invalid.clone().into_source_parts().unwrap();
        let transported = RawExpressionValue::from_source_parts(rdf, cache, facts).unwrap();
        assert_eq!(metadata(&transported), metadata(&invalid));
        let copied = session.copy_source_literal(&invalid).unwrap();
        assert_eq!(metadata(&copied).ill_typed, SourceFact::Known(None));
        assert_eq!(
            metadata(&copied).cached_value_kind,
            metadata(&invalid).cached_value_kind
        );
        assert_eq!(
            source_identity_key(&invalid).unwrap(),
            source_identity_key(&copied).unwrap()
        );
    }

    #[test]
    fn source_literal_axis_does_not_follow_numeric_or_group_capability() {
        let legacy = QueryNumericSession::new_with_group_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
            Some(4300),
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
        )
        .unwrap();
        assert_eq!(
            legacy.literal_semantics(),
            QueryLiteralSemantics::RdfIdentityV1
        );
        assert!(matches!(
            legacy.construct_source_direct_string("a", None, None, true),
            Err(RawNumericFault::UnsupportedSourceLiteralOperator(_))
        ));
        let result = legacy
            .construct_result(NumericValue::Integer(BigInt::from(1)), None)
            .unwrap();
        assert_eq!(metadata(&result), &SourceLiteralMetadata::unknown());
        let source = source_session()
            .construct_result(NumericValue::Integer(BigInt::from(1)), None)
            .unwrap();
        assert_eq!(metadata(&source).ill_typed, SourceFact::Known(None));
        assert_eq!(
            metadata(&source).cached_value_kind,
            SourceFact::Known(SourceLiteralCacheKind::Numeric)
        );
        assert_eq!(result, source);
        assert_eq!(bytes(&result), bytes(&source));
    }

    #[test]
    fn source_language_transport_checks_original_facts_without_trimming() {
        let session = source_session();
        let raw = session
            .construct_source_direct_string("a", None, Some("EN\n".into()), true)
            .unwrap();
        let Some(literal) = raw.literal() else {
            panic!("literal");
        };
        assert_eq!(literal.language(), Some("en\n"));
        assert_eq!(
            metadata(&raw).language_original,
            SourceFact::Known(Some("EN\n".into()))
        );
        let copied = session.copy_source_literal(&raw).unwrap();
        assert_eq!(
            source_identity_key(&raw).unwrap(),
            source_identity_key(&copied).unwrap()
        );
        let mut false_facts = metadata(&raw).clone();
        false_facts.language_original = SourceFact::Known(Some("en-".into()));
        let malformed_carrier = Literal::new_language_tagged_literal_unchecked("a", "en-");
        assert_eq!(
            RawExpressionValue::from_source_parts(
                malformed_carrier.into(),
                NumericCacheState::NotNumeric,
                LiteralSourceState::Literal(false_facts)
            )
            .unwrap_err(),
            RawNumericFault::InvalidSourceOrigin,
        );
        assert_eq!(
            session
                .construct_source_direct_string("a", None, Some("en-".into()), true)
                .unwrap_err(),
            RawNumericFault::SourceLiteralLanguageValueError,
        );
        // Validation does not erase a second LF, CR, spaces or arbitrary suffix.
        assert_eq!(
            session
                .construct_source_direct_string("a", None, Some("en\n\n".into()), true)
                .unwrap_err(),
            RawNumericFault::SourceLiteralLanguageValueError,
        );
    }
}
