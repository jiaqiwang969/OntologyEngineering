//! Source rich comparison of already evaluated values. No AST or sorting.
//! The fixed term.py GT/LT/value-equality branches remain distinct; an active
//! unknown/resource refusal is never a source TypeError eligible for reflection.
use super::*;
use oxrdf::Variable;
use semantica_numeric::NumericRelation;
use std::cmp::Ordering;

/// New relational value caches are qualified independently from ORDER/extrema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LiteralComparisonDomain {
    EstablishedOrder,
    RelationalValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceExtremumKind {
    Minimum,
    Maximum,
}

/// The result of ORDER's parserutils.value(..., variables=True) followed by
/// _val. A bound error is raised by the compiler adapter before this boundary;
/// an Expr-returned error (or None) instead reaches the None key below.
#[derive(Clone, Debug)]
pub(crate) enum SourceOrderKey {
    UnboundVariable(Variable),
    Rdf(RawExpressionValue),
    None,
}

#[derive(Clone, Copy)]
enum CachedValue<'a> {
    None,
    Text(&'a str),
    Number(&'a NumericValue),
    Error(&'a ErrorValue),
    DateTime(semantica_temporal::DateTime),
    Date(semantica_temporal::Date),
    Time(semantica_temporal::Time),
    DayTimeDuration(semantica_temporal::Timedelta),
    Duration(&'a semantica_temporal::DurationValue),
    Boolean(bool),
}
impl CachedValue<'_> {
    fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

impl QueryNumericSession {
    /// Turtle list.sort compares Literal values directly. Reuse the already
    /// received relational caches (date/boolean included) without widening
    /// SPARQL ORDER's older EstablishedOrder admission or tuple equality.
    pub(crate) fn source_turtle_node_less(
        &self, left: &RawExpressionValue, right: &RawExpressionValue,
    ) -> Result<bool, RawNumericFault> {
        if left.literal().is_some() && right.literal().is_some() {
            return match self.literal_lt(left,right,LiteralComparisonDomain::RelationalValue)? {
                Some(value)=>Ok(value),
                None=>self.literal_gt(right,left,LiteralComparisonDomain::RelationalValue),
            };
        }
        self.source_graph_node_less(&SourceOrderKey::Rdf(left.clone()),&SourceOrderKey::Rdf(right.clone()))
    }
    /// Direct Identifier/Literal tuple comparison in algebra.reorderTriples.
    /// This is distinct from SPARQL ORDER's _val rank: BNode precedes Variable.
    pub(crate) fn source_graph_node_less(
        &self,
        left: &SourceOrderKey,
        right: &SourceOrderKey,
    ) -> Result<bool, RawNumericFault> {
        fn direct_rank(value: &SourceOrderKey) -> Result<u8, RawNumericFault> {
            match value {
                SourceOrderKey::UnboundVariable(_) => Ok(1),
                SourceOrderKey::Rdf(raw) if raw.source_variable().is_some() => Ok(1),
                SourceOrderKey::Rdf(raw) => match raw.rdf()? {
                    Term::BlankNode(_) => Ok(0),
                    Term::NamedNode(_) => Ok(2),
                    Term::Literal(_) => Ok(3),
                    #[cfg(feature = "sparql-12")]
                    Term::Triple(_) => Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "source graph triple node ordering",
                    )),
                },
                SourceOrderKey::None => Err(RawNumericFault::InvalidSourceOrigin),
            }
        }
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let a = direct_rank(left)?;
        let b = direct_rank(right)?;
        if a != b {
            return Ok(a < b);
        }
        if let (Some(a), Some(b)) = (variable_name(left), variable_name(right)) { return Ok(a < b); }
        match (left, right) {
            (SourceOrderKey::UnboundVariable(a), SourceOrderKey::UnboundVariable(b)) => {
                Ok(a.as_str() < b.as_str())
            }
            (SourceOrderKey::Rdf(a), SourceOrderKey::Rdf(b)) => {
                self.rdf_candidate_wins(SourceExtremumKind::Minimum, a, b)
            }
            _ => Err(RawNumericFault::InvalidSourceOrigin),
        }
    }
    pub(crate) fn source_order_key_less(
        &self,
        left: &SourceOrderKey,
        right: &SourceOrderKey,
    ) -> Result<bool, NumericEvaluationError> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "source order comparison",
            )
            .into());
        }
        if let (Some(a), Some(b)) = (variable_name(left), variable_name(right)) { return Ok(a < b); }
        let result = match (left, right) {
            // Python None is not an empty tuple or an equal key: sorted still
            // requests LT when both keys are None, and that raises TypeError.
            (SourceOrderKey::None, _) | (_, SourceOrderKey::None) => {
                Err(RawNumericFault::SourceComparisonTypeError)
            }
            (SourceOrderKey::UnboundVariable(a), SourceOrderKey::UnboundVariable(b)) => {
                Ok(a.as_str() < b.as_str())
            }
            (SourceOrderKey::UnboundVariable(_), SourceOrderKey::Rdf(value)) => {
                // Rank zero decides before any source Literal/cache fact.
                rank(value).map(|_| true)
            }
            (SourceOrderKey::Rdf(value), SourceOrderKey::UnboundVariable(_)) => {
                rank(value).map(|_| false)
            }
            (SourceOrderKey::Rdf(a), SourceOrderKey::Rdf(b)) => {
                // DESC is implemented by the sort's stable double reversal;
                // it never changes this LT request to source GT.
                self.rdf_candidate_wins(SourceExtremumKind::Minimum, a, b)
            }
        };
        result.map_err(Into::into)
    }

    pub(crate) fn source_extremum_candidate_wins(
        &self,
        kind: SourceExtremumKind,
        candidate: &SolutionValue<RawExpressionValue>,
        current: &SolutionValue<RawExpressionValue>,
    ) -> Result<bool, NumericEvaluationError> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "source extremum comparison",
            )
            .into());
        }
        self.extremum_candidate_wins(kind, candidate, current)
            .map_err(Into::into)
    }

    fn extremum_candidate_wins(
        &self,
        kind: SourceExtremumKind,
        candidate: &SolutionValue<RawExpressionValue>,
        current: &SolutionValue<RawExpressionValue>,
    ) -> Result<bool, RawNumericFault> {
        // _val(None/error) is Python None, not an equal empty tuple. min/max
        // still requests an ordered comparison, including None versus None.
        let (SolutionValue::Rdf(candidate), SolutionValue::Rdf(current)) = (candidate, current)
        else {
            return Err(RawNumericFault::SourceComparisonTypeError);
        };
        self.rdf_candidate_wins(kind, candidate, current)
    }

    fn rdf_candidate_wins(
        &self,
        kind: SourceExtremumKind,
        candidate: &RawExpressionValue,
        current: &RawExpressionValue,
    ) -> Result<bool, RawNumericFault> {
        let candidate_rank = rank(candidate)?;
        let current_rank = rank(current)?;
        if candidate_rank != current_rank {
            // Tuple rank decides before any source Literal/cache fact is read.
            return Ok(wins(kind, candidate_rank.cmp(&current_rank)));
        }
        if let (Some(a), Some(b)) = (candidate.source_variable(), current.source_variable()) { return Ok(wins(kind, a.as_str().cmp(b.as_str()))); }
        match (candidate.rdf()?, current.rdf()?) {
            (Term::NamedNode(a), Term::NamedNode(b)) => Ok(wins(kind, a.as_str().cmp(b.as_str()))),
            (Term::BlankNode(a), Term::BlankNode(b)) => Ok(wins(kind, a.as_str().cmp(b.as_str()))),
            (Term::Literal(_), Term::Literal(_)) => {
                // Tuple EQ uses Literal.__eq__, not Literal.eq or numeric EQ.
                if literal_identity_equal(candidate, current)? {
                    return Ok(false);
                }
                match kind {
                    SourceExtremumKind::Maximum => self.literal_gt(
                        candidate,
                        current,
                        LiteralComparisonDomain::EstablishedOrder,
                    ),
                    SourceExtremumKind::Minimum => match self.literal_lt(
                        candidate,
                        current,
                        LiteralComparisonDomain::EstablishedOrder,
                    )? {
                        Some(value) => Ok(value),
                        // CPython rich comparison reflects NotImplemented.
                        None => self.literal_gt(
                            current,
                            candidate,
                            LiteralComparisonDomain::EstablishedOrder,
                        ),
                    },
                }
            }
            _ => Err(RawNumericFault::InvalidSourceOrigin),
        }
    }

    pub(super) fn literal_gt(
        &self,
        left: &RawExpressionValue,
        right: &RawExpressionValue,
        domain: LiteralComparisonDomain,
    ) -> Result<bool, RawNumericFault> {
        if self.numeric_fast_path(left, right, domain)? {
            // The source numeric fast path is OUTSIDE the TypeError catch.
            return self.cached_relation(
                self.cached(left, domain)?,
                self.cached(right, domain)?,
                NumericRelation::Greater,
            );
        }
        let a = metadata(left)?;
        let b = metadata(right)?;
        let a_datatype = datatype_or_string(a)?;
        let b_datatype = datatype_or_string(b)?;
        if a_datatype != b_datatype {
            // This private source domain fixes DAWG_LITERAL_COLLATION=false.
            return Ok(a_datatype > b_datatype);
        }
        let a_language = language(a)?;
        let b_language = language(b)?;
        if a_language != b_language {
            return Ok(match (a_language, b_language) {
                (None, _) => false,
                (_, None) => true,
                (Some(a), Some(b)) => a > b,
            });
        }
        let a_value = self.cached(left, domain)?;
        if !a_value.is_none() {
            let b_value = self.cached(right, domain)?;
            if !b_value.is_none() {
                // RDFLib's _TOTAL_ORDER_CASTERS partitions time and datetime
                // by awareness before comparing values. This belongs to
                // Literal.__gt__; raw value equality/comparison stay separate.
                let awareness = match (a_value, b_value) {
                    (CachedValue::Time(a), CachedValue::Time(b)) => {
                        Some((a.offset_micros().is_some(), b.offset_micros().is_some()))
                    }
                    (CachedValue::DateTime(a), CachedValue::DateTime(b)) => {
                        Some((a.offset_micros().is_some(), b.offset_micros().is_some()))
                    }
                    _ => None,
                };
                if let Some((a, b)) = awareness {
                    match a.cmp(&b) {
                        Ordering::Less => return Ok(false),
                        Ordering::Greater => return Ok(true),
                        Ordering::Equal => (),
                    }
                }
                match self.cached_relation(a_value, b_value, NumericRelation::Greater) {
                    Ok(value) => return Ok(value),
                    Err(error) if source_type_error(&error) => (),
                    Err(error) => return Err(error),
                }
            }
        }
        let a_lexical = lexical(left)?;
        let b_lexical = lexical(right)?;
        if a_lexical != b_lexical {
            return Ok(a_lexical > b_lexical);
        }
        Ok(match (source_datatype(a)?, source_datatype(b)?) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(a), Some(b)) => a.as_str() > b.as_str(),
        })
    }

    pub(super) fn literal_lt(
        &self,
        left: &RawExpressionValue,
        right: &RawExpressionValue,
        domain: LiteralComparisonDomain,
    ) -> Result<Option<bool>, RawNumericFault> {
        let result = (|| {
            if self.literal_gt(left, right, domain)? {
                Ok(false)
            } else {
                self.literal_value_equal(left, right, domain)
                    .map(|equal| !equal)
            }
        })();
        match result {
            Ok(value) => Ok(Some(value)),
            Err(error) if source_type_error(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn literal_le(
        &self,
        left: &RawExpressionValue,
        right: &RawExpressionValue,
        domain: LiteralComparisonDomain,
    ) -> Result<Option<bool>, RawNumericFault> {
        // The pinned CPython 3.13 source tests r in boolean context. Its
        // NotImplemented result is truthy here; this is not a reflected GT.
        // Host-global warning-filter hooks remain a separate source protocol.
        if self.literal_lt(left, right, domain)? != Some(false) {
            return Ok(Some(true));
        }
        match self.literal_value_equal(left, right, domain) {
            Ok(value) => Ok(Some(value)),
            Err(error) if source_type_error(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn literal_ge(
        &self,
        left: &RawExpressionValue,
        right: &RawExpressionValue,
        domain: LiteralComparisonDomain,
    ) -> Result<Option<bool>, RawNumericFault> {
        if self.literal_gt(left, right, domain)? {
            return Ok(Some(true));
        }
        match self.literal_value_equal(left, right, domain) {
            Ok(value) => Ok(Some(value)),
            Err(error) if source_type_error(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn literal_value_equal(
        &self,
        left: &RawExpressionValue,
        right: &RawExpressionValue,
        domain: LiteralComparisonDomain,
    ) -> Result<bool, RawNumericFault> {
        if self.numeric_fast_path(left, right, domain)? {
            return self.cached_relation(
                self.cached(left, domain)?,
                self.cached(right, domain)?,
                NumericRelation::Equal,
            );
        }
        let a = metadata(left)?;
        let b = metadata(right)?;
        if language(a)?.map(str::to_ascii_lowercase) != language(b)?.map(str::to_ascii_lowercase) {
            return Ok(false);
        }
        let a_datatype = datatype_or_string(a)?;
        let b_datatype = datatype_or_string(b)?;
        if a_datatype == XSD_STRING && b_datatype == XSD_STRING {
            return Ok(lexical(left)? == lexical(right)?);
        }
        if a_datatype != b_datatype {
            return Ok(false);
        }
        let a_value = self.cached(left, domain)?;
        if !a_value.is_none() {
            let b_value = self.cached(right, domain)?;
            if !b_value.is_none() {
                return self.cached_relation(a_value, b_value, NumericRelation::Equal);
            }
        }
        if lexical(left)? == lexical(right)? {
            return Ok(true);
        }
        if source_datatype(a)?.is_some_and(|datatype| datatype.as_str() == XSD_STRING) {
            return Ok(false);
        }
        Err(RawNumericFault::SourceComparisonTypeError)
    }

    fn cached_relation(
        &self,
        left: CachedValue<'_>,
        right: CachedValue<'_>,
        relation: NumericRelation,
    ) -> Result<bool, RawNumericFault> {
        match (left, right) {
            (CachedValue::Time(a), CachedValue::Time(b)) => {
                if relation == NumericRelation::Equal {
                    Ok(a.source_value_eq(b))
                } else {
                    a.source_value_cmp(b)
                        .map(|order| relation_matches(order, relation))
                        .ok_or(RawNumericFault::SourceComparisonTypeError)
                }
            }
            (CachedValue::Date(a), CachedValue::Date(b)) => {
                Ok(relation_matches(a.cmp(&b), relation))
            }
            (CachedValue::Duration(a), CachedValue::Duration(b)) => {
                if relation == NumericRelation::Equal {self.calendar_values_equal(a,b)}
                else {match (a,b) {
                    (semantica_temporal::DurationValue::Fixed(a),semantica_temporal::DurationValue::Fixed(b))=>Ok(relation_matches(a.cmp(b),relation)),
                    _=>Err(RawNumericFault::SourceComparisonTypeError),
                }}
            }
            (CachedValue::DayTimeDuration(a), CachedValue::DayTimeDuration(b)) => {
                Ok(relation_matches(a.cmp(&b), relation))
            }
            (CachedValue::Boolean(a), CachedValue::Boolean(b)) => {
                Ok(relation_matches(a.cmp(&b), relation))
            }
            (CachedValue::DateTime(a), CachedValue::DateTime(b)) => {
                if relation == NumericRelation::Equal {
                    Ok(a.value_eq(b))
                } else {
                    let order = a
                        .value_cmp(b)
                        .ok_or(RawNumericFault::SourceComparisonTypeError)?;
                    Ok(order
                        == if relation == NumericRelation::Less {
                            Ordering::Less
                        } else {
                            Ordering::Greater
                        })
                }
            }
            (CachedValue::Number(a), CachedValue::Number(b)) => self.observe_comparison(
                self.decimal_context()
                    .compare_numeric(relation, a, b, self.limits()),
            ),
            (CachedValue::Text(a), CachedValue::Text(b)) => Ok(match relation {
                NumericRelation::Equal => a == b,
                NumericRelation::Less => a < b,
                NumericRelation::Greater => a > b,
            }),
            (CachedValue::Error(a), CachedValue::Error(b))
                if relation == NumericRelation::Equal =>
            {
                Ok(a == b)
            }
            // Non-numeric built-in value families do not coerce for equality.
            _ if relation == NumericRelation::Equal => Ok(false),
            _ => Err(RawNumericFault::SourceComparisonTypeError),
        }
    }
}

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn relation_matches(order: Ordering, relation: NumericRelation) -> bool {
    order
        == match relation {
            NumericRelation::Equal => Ordering::Equal,
            NumericRelation::Less => Ordering::Less,
            NumericRelation::Greater => Ordering::Greater,
        }
}

fn wins(kind: SourceExtremumKind, ordering: Ordering) -> bool {
    ordering
        == match kind {
            SourceExtremumKind::Minimum => Ordering::Less,
            SourceExtremumKind::Maximum => Ordering::Greater,
        }
}

fn variable_name(value: &SourceOrderKey) -> Option<&str> {
    match value { SourceOrderKey::UnboundVariable(v) => Some(v.as_str()), SourceOrderKey::Rdf(v) => v.source_variable().map(Variable::as_str), _ => None }
}
fn rank(value: &RawExpressionValue) -> Result<u8, RawNumericFault> {
    if value.source_variable().is_some() { return Ok(0); }
    Ok(match value.rdf()? {
        Term::BlankNode(_) => 1,
        Term::NamedNode(_) => 2,
        Term::Literal(_) => 3,
        #[cfg(feature = "sparql-12")]
        Term::Triple(_) => {
            return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "triple source ordering",
            ));
        }
    })
}

fn metadata(value: &RawExpressionValue) -> Result<&SourceLiteralMetadata, RawNumericFault> {
    match value.literal_source() {
        LiteralSourceState::Literal(metadata) => Ok(metadata),
        LiteralSourceState::NotLiteral => Err(RawNumericFault::InvalidSourceOrigin),
    }
}

fn source_datatype(
    metadata: &SourceLiteralMetadata,
) -> Result<Option<&NamedNode>, RawNumericFault> {
    match &metadata.datatype {
        SourceDatatype::Absent => Ok(None),
        SourceDatatype::Specified(datatype) => Ok(Some(datatype)),
        SourceDatatype::Unknown => Err(RawNumericFault::UnknownSourceLiteralIdentity),
    }
}

fn datatype_or_string(metadata: &SourceLiteralMetadata) -> Result<&str, RawNumericFault> {
    Ok(source_datatype(metadata)?.map_or(XSD_STRING, NamedNode::as_str))
}

fn language(metadata: &SourceLiteralMetadata) -> Result<Option<&str>, RawNumericFault> {
    match &metadata.language_original {
        SourceFact::Known(language) => Ok(language.as_deref()),
        SourceFact::Unknown => Err(RawNumericFault::UnknownSourceLiteralIdentity),
    }
}

fn lexical(value: &RawExpressionValue) -> Result<&str, RawNumericFault> {
    match value.rdf()? {
        Term::Literal(literal) => Ok(literal.value()),
        _ => Err(RawNumericFault::InvalidSourceOrigin),
    }
}

fn literal_identity_equal(
    left: &RawExpressionValue,
    right: &RawExpressionValue,
) -> Result<bool, RawNumericFault> {
    let a = metadata(left)?;
    let b = metadata(right)?;
    // Literal.__eq__ short-circuits datatype, language, then lexical. Unlike
    // constructing a DISTINCT key it need not demand an unused unknown fact.
    if source_datatype(a)? != source_datatype(b)? {
        return Ok(false);
    }
    if language(a)?.map(str::to_ascii_lowercase) != language(b)?.map(str::to_ascii_lowercase) {
        return Ok(false);
    }
    Ok(lexical(left)? == lexical(right)?)
}

fn is_ill_typed(value: &RawExpressionValue) -> Result<bool, RawNumericFault> {
    match metadata(value)?.ill_typed {
        SourceFact::Known(value) => Ok(value.unwrap_or(false)),
        SourceFact::Unknown => Err(RawNumericFault::UnknownNumericCache),
    }
}

impl QueryNumericSession {
    fn numeric_fast_path(
        &self,
        left: &RawExpressionValue,
        right: &RawExpressionValue,
        domain: LiteralComparisonDomain,
    ) -> Result<bool, RawNumericFault> {
        Ok(
            source_datatype(metadata(left)?)?.is_some_and(|datatype| numeric_kind(datatype).is_some())
                && source_datatype(metadata(right)?)?
                    .is_some_and(|datatype| numeric_kind(datatype).is_some())
                && !is_ill_typed(left)?
                && !is_ill_typed(right)?
                && !self.cached(left, domain)?.is_none()
                && !self.cached(right, domain)?.is_none(),
        )
    }

    fn cached<'a>(
        &self,
        value: &'a RawExpressionValue,
        domain: LiteralComparisonDomain,
    ) -> Result<CachedValue<'a>, RawNumericFault> {
        if domain == LiteralComparisonDomain::EstablishedOrder {
            match &metadata(value)?.cached_value_kind {
                SourceFact::Known(SourceLiteralCacheKind::Time(_)) if !self.source_time_comparisons_v1 => {
                    return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "unreceived source ordering cache",
                    ));
                }
                SourceFact::Known(SourceLiteralCacheKind::Boolean(_)) => {
                    return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "boolean source comparison",
                    ));
                }
                SourceFact::Known(
                    SourceLiteralCacheKind::Date(_)
                    | SourceLiteralCacheKind::DayTimeDuration(_)
                    | SourceLiteralCacheKind::UnmappedDatatype,
                ) => {
                    return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "unreceived source ordering cache",
                    ));
                }
                _ => (),
            }
        }
        match &metadata(value)?.cached_value_kind {
            SourceFact::Unknown => Err(RawNumericFault::UnknownNumericCache),
            SourceFact::Known(SourceLiteralCacheKind::String) => Ok(CachedValue::Text(lexical(value)?)),
            SourceFact::Known(
                SourceLiteralCacheKind::None
                | SourceLiteralCacheKind::DateTime(None)
                | SourceLiteralCacheKind::Date(None)
                | SourceLiteralCacheKind::DayTimeDuration(None)
                | SourceLiteralCacheKind::UnmappedDatatype,
            ) => Ok(CachedValue::None),
            SourceFact::Known(SourceLiteralCacheKind::Duration(cache)) => {
                if !self.source_calendar_duration_v1 {return Err(RawNumericFault::UnsupportedSourceLiteralOperator("calendar Duration comparison is not enabled"))}
                Ok(cache.value.as_ref().map_or(CachedValue::None,CachedValue::Duration))
            }
            SourceFact::Known(SourceLiteralCacheKind::DateTime(Some(value))) => {
                Ok(CachedValue::DateTime(*value))
            }
            SourceFact::Known(SourceLiteralCacheKind::Time(value)) if self.source_time_comparisons_v1 => {
                Ok(value.map_or(CachedValue::None, CachedValue::Time))
            }
            SourceFact::Known(SourceLiteralCacheKind::Time(_)) => {
                Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "unreceived source time comparison cache",
                ))
            }
            SourceFact::Known(SourceLiteralCacheKind::Date(Some(value))) => {
                Ok(CachedValue::Date(*value))
            }
            SourceFact::Known(SourceLiteralCacheKind::DayTimeDuration(Some(value))) => {
                Ok(CachedValue::DayTimeDuration(*value))
            }
            SourceFact::Known(SourceLiteralCacheKind::Numeric) => match value.numeric() {
                NumericCacheState::Known(cache) => Ok(CachedValue::Number(cache.value())),
                _ => Err(RawNumericFault::InvalidSourceOrigin),
            },
            SourceFact::Known(SourceLiteralCacheKind::ErrorObject) => match value.numeric() {
                NumericCacheState::ErrorObject(error) => Ok(CachedValue::Error(error)),
                _ => Err(RawNumericFault::InvalidSourceOrigin),
            },
            SourceFact::Known(SourceLiteralCacheKind::Boolean(value)) => {
                Ok(CachedValue::Boolean(*value))
            }
        }
    }

}

fn source_type_error(error: &RawNumericFault) -> bool {
    matches!(
        error,
        RawNumericFault::SourceComparisonTypeError
            | RawNumericFault::Core(NumericFault::DecimalTrap {
                primary: DecimalSignal::FloatOperation,
                ..
            })
    )
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

    #[test]
    fn source_order_variable_rank_does_not_consume_unknown_literal_facts() {
        let session = make_session(true);
        let variable = SourceOrderKey::UnboundVariable(Variable::new("missing").unwrap());
        let unknown = SourceOrderKey::Rdf(RawExpressionValue::from_import(
            Literal::new_simple_literal("unknown provenance").into(),
        ));
        assert!(session.source_order_key_less(&variable, &unknown).unwrap());
        assert!(!session.source_order_key_less(&unknown, &variable).unwrap());
        assert!(!session.source_order_key_less(&variable, &variable).unwrap());
        assert!(matches!(
            session.source_order_key_less(&unknown, &unknown),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnknownSourceLiteralIdentity
            ))
        ));
    }

    #[test]
    fn source_order_none_comparison_raises_and_default_axis_stays_closed() {
        let session = make_session(true);
        let variable = SourceOrderKey::UnboundVariable(Variable::new("x").unwrap());
        for (a, b) in [
            (&SourceOrderKey::None, &SourceOrderKey::None),
            (&SourceOrderKey::None, &variable),
            (&variable, &SourceOrderKey::None),
        ] {
            assert!(matches!(
                session.source_order_key_less(a, b),
                Err(NumericEvaluationError::Fatal(
                    RawNumericFault::SourceComparisonTypeError
                ))
            ));
        }
        assert!(matches!(
            make_session(false).source_order_key_less(&variable, &variable),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnsupportedSourceLiteralOperator(_)
            ))
        ));
    }
}
