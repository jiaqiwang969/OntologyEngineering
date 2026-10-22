//! Internal consumers of typed raw expression results. This lane has no public
//! query profile: parser lowering, context thaw and the result wrapper remain
//! separate obligations before a nullable query can be advertised.
use super::*;
#[cfg(feature = "rdflib760-nullable-internal")]
use crate::expression::build_numeric_expression_evaluator;
#[cfg(feature = "rdflib760-nullable-internal")]
use crate::numeric::{
    NumericAggregateState, NumericEvaluationError, QueryGroupSemantics, QueryLiteralSemantics,
    QueryNumericSession, RawExpressionValue, RawNumericFault, SourceExtremumKind, SourceTermKey,
    source_identity_key,
};
use crate::tuple::{ErrorValue, SparqlErrorKind};
#[cfg(feature = "rdflib760-nullable-internal")]
use semantica_numeric::{BigInt, NumericFault};

#[derive(Eq, PartialEq)]
pub(super) struct GroupKeyCell<T>(pub(super) Option<SolutionValue<T>>);

impl<T: Hash> Hash for GroupKeyCell<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            // Preserve the old Option<T> hash byte stream for every old key.
            // Extra enum tags would change unsorted GROUP iteration + LIMIT.
            None => Option::<&T>::None.hash(state),
            Some(SolutionValue::Rdf(term)) => Some(term).hash(state),
            Some(cell) => {
                2_u8.hash(state);
                cell.hash(state);
            }
        }
    }
}

pub(super) fn halt_after_error<T, E>(
    mut input: impl Iterator<Item = Result<T, E>>,
) -> impl Iterator<Item = Result<T, E>> {
    let mut stopped = false;
    std::iter::from_fn(move || {
        if stopped {
            return None;
        }
        let next = input.next();
        stopped = next.as_ref().is_none_or(Result::is_err);
        next
    })
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub(super) enum AggregateValue<T> {
    Internal(SolutionValue<T>),
    Expression(SolutionValue<ExpressionTerm>),
    #[cfg(feature = "rdflib760-nullable-internal")]
    RawExpression(SolutionValue<RawExpressionValue>),
}

impl<T> AggregateValue<T> {
    pub(super) fn is_returned_error(&self) -> bool {
        match self {
            Self::Internal(SolutionValue::ReturnedError(_))
            | Self::Expression(SolutionValue::ReturnedError(_)) => true,
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::RawExpression(SolutionValue::ReturnedError(_)) => true,
            _ => false,
        }
    }

    pub(super) fn into_cell(
        self,
        internalize: impl FnOnce(ExpressionTerm) -> Result<T, QueryEvaluationError>,
    ) -> Result<SolutionValue<T>, QueryEvaluationError> {
        Ok(match self {
            Self::Internal(value) => value,
            Self::Expression(SolutionValue::Rdf(value)) => SolutionValue::Rdf(internalize(value)?),
            Self::Expression(SolutionValue::ExplicitNone) => SolutionValue::ExplicitNone,
            Self::Expression(SolutionValue::ReturnedError(error)) => {
                SolutionValue::ReturnedError(error)
            }
            // Raw values carry per-cell construction provenance. Only the
            // numeric codec may internalize them; the legacy closure cannot
            // preserve that provenance, even if the RDF text looks numeric.
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::RawExpression(_) => {
                return Err(QueryEvaluationError::Unexpected(
                    "numeric aggregate requires its prepared-query codec".into(),
                ));
            }
        })
    }
}

pub(super) enum NullableAggregateEvaluator<'a, T> {
    Internal(RawEvaluator<'a, InternalTuple<T>, T, QueryEvaluationError>),
    Expression(RawEvaluator<'a, InternalTuple<T>, ExpressionTerm, QueryEvaluationError>),
    #[cfg(feature = "rdflib760-nullable-internal")]
    RawExpression(RawEvaluator<'a, InternalTuple<T>, RawExpressionValue, QueryEvaluationError>),
}

impl<T> Clone for NullableAggregateEvaluator<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Internal(evaluator) => Self::Internal(Rc::clone(evaluator)),
            Self::Expression(evaluator) => Self::Expression(Rc::clone(evaluator)),
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::RawExpression(evaluator) => Self::RawExpression(Rc::clone(evaluator)),
        }
    }
}

impl<T> NullableAggregateEvaluator<'_, T> {
    pub(super) fn evaluate(
        &self,
        row: &InternalTuple<T>,
    ) -> Result<AggregateValue<T>, EvalFailure<QueryEvaluationError>> {
        match self {
            Self::Internal(evaluator) => evaluator(row).map(AggregateValue::Internal),
            Self::Expression(evaluator) => evaluator(row).map(AggregateValue::Expression),
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::RawExpression(evaluator) => evaluator(row).map(AggregateValue::RawExpression),
        }
    }
}

// A source identity is fallible and is therefore deliberately not the Eq or
// Hash implementation of RawExpressionValue or its physical RDF carrier.
#[cfg(feature = "rdflib760-nullable-internal")]
pub(super) fn aggregate_source_key<T>(
    value: &AggregateValue<T>,
) -> Result<SolutionValue<SourceTermKey>, QueryEvaluationError> {
    match value {
        AggregateValue::RawExpression(value) => value.try_map_rdf(|value| {
            source_identity_key(value)
                .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))
        }),
        _ => Err(QueryEvaluationError::Unexpected(Box::new(
            RawNumericFault::SourceMetadataWouldBeLost,
        ))),
    }
}

// Reuse the original double evaluation, catch and update algorithm with an
// explicit identity strategy. Old accumulator variants and constructors remain.
enum DistinctTracker<'s, T> {
    Rdf(&'s mut FxHashSet<AggregateValue<T>>),
    #[cfg(feature = "rdflib760-nullable-internal")]
    Source(&'s mut FxHashSet<SolutionValue<SourceTermKey>>),
}
impl<T: Eq + Hash> DistinctTracker<'_, T> {
    fn contains(&self, value: &AggregateValue<T>) -> Result<bool, QueryEvaluationError> {
        Ok(match self {
            Self::Rdf(seen) => seen.contains(value),
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Source(seen) => seen.contains(&aggregate_source_key(value)?),
        })
    }
    fn insert(&mut self, value: AggregateValue<T>) -> Result<(), QueryEvaluationError> {
        match self {
            Self::Rdf(seen) => {
                seen.insert(value);
            }
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Source(seen) => {
                seen.insert(aggregate_source_key(&value)?);
            }
        }
        Ok(())
    }
}

pub(super) enum NullableAccumulator<'a, T> {
    #[cfg(feature = "rdflib760-nullable-internal")]
    Extremum {
        evaluator: NullableAggregateEvaluator<'a, T>,
        session: Rc<QueryNumericSession>,
        selected: Option<SolutionValue<RawExpressionValue>>,
        kind: SourceExtremumKind,
    },
    Count {
        evaluator: NullableAggregateEvaluator<'a, T>,
        // None means non-distinct, not an empty set for a DISTINCT aggregate.
        seen: Option<FxHashSet<AggregateValue<T>>>,
        count: i64,
    },
    Sample {
        evaluator: NullableAggregateEvaluator<'a, T>,
        // Outer None is Pending. Selected(ExplicitNone) is a completed sample.
        selected: Option<AggregateValue<T>>,
    },
    #[cfg(feature = "rdflib760-nullable-internal")]
    Sum {
        evaluator: NullableAggregateEvaluator<'a, T>,
        session: Rc<QueryNumericSession>,
        state: NumericAggregateState,
        seen: Option<FxHashSet<AggregateValue<T>>>,
    },
    #[cfg(feature = "rdflib760-nullable-internal")]
    Average {
        evaluator: NullableAggregateEvaluator<'a, T>,
        session: Rc<QueryNumericSession>,
        state: NumericAggregateState,
        seen: Option<FxHashSet<AggregateValue<T>>>,
        count: BigInt,
    },
    #[cfg(feature = "rdflib760-nullable-internal")]
    GroupConcat {
        evaluator: NullableAggregateEvaluator<'a, T>,
        session: Rc<QueryNumericSession>,
        values: Vec<SolutionValue<RawExpressionValue>>,
        separator: Rc<str>,
    },
}

#[derive(Debug)]
pub(crate) enum NullableAggregateError {
    UncaughtSparql(ErrorValue),
}
impl fmt::Display for NullableAggregateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UncaughtSparql(error) => write!(
                f,
                "uncaught nullable SPARQL {:?}: {}",
                error.kind(),
                error.diagnostic()
            ),
        }
    }
}
impl std::error::Error for NullableAggregateError {}
pub(crate) fn nullable_aggregate_error(
    error: &QueryEvaluationError,
) -> Option<&NullableAggregateError> {
    if let QueryEvaluationError::Unexpected(error) = error {
        error.downcast_ref()
    } else {
        None
    }
}

pub(super) fn uncaught(error: EvalFailure<QueryEvaluationError>) -> QueryEvaluationError {
    match error {
        EvalFailure::Fatal(error) => error,
        EvalFailure::Sparql(error) => QueryEvaluationError::Unexpected(Box::new(
            NullableAggregateError::UncaughtSparql(error),
        )),
    }
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(super) fn source_exists_failure(
    error: QueryEvaluationError,
) -> EvalFailure<QueryEvaluationError> {
    if let Some(NullableAggregateError::UncaughtSparql(value)) = nullable_aggregate_error(&error) {
        if value.source_exception().is_some() {
            return EvalFailure::Sparql(value.clone());
        }
    }
    EvalFailure::Fatal(error)
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(super) fn numeric_failure(error: NumericEvaluationError) -> EvalFailure<QueryEvaluationError> {
    match error.into_error_value() {
        Ok(error) => EvalFailure::Sparql(error),
        Err(error) => EvalFailure::Fatal(QueryEvaluationError::Unexpected(Box::new(error))),
    }
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn group_concat_resource(operation: &'static str) -> RawNumericFault {
    RawNumericFault::Core(NumericFault::ResourceLimit { operation })
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn accumulate_source_extremum<T>(
    evaluator: &NullableAggregateEvaluator<'_, T>,
    session: &QueryNumericSession,
    selected: &mut Option<SolutionValue<RawExpressionValue>>,
    kind: SourceExtremumKind,
    tuple: &InternalTuple<T>,
) -> Result<(), QueryEvaluationError> {
    let mut update = || {
        // Extremum disables DISTINCT.use_row. Evaluate exactly once, inside
        // its NotBound/SPARQLTypeError catch, including when still pending.
        let AggregateValue::RawExpression(candidate) = evaluator.evaluate(tuple)? else {
            return Err(EvalFailure::Fatal(QueryEvaluationError::Unexpected(
                Box::new(RawNumericFault::SourceMetadataWouldBeLost),
            )));
        };
        let wins = match selected.as_ref() {
            None | Some(SolutionValue::ExplicitNone) => true,
            Some(current) => session
                .source_extremum_candidate_wins(kind, &candidate, current)
                .map_err(numeric_failure)?,
        };
        if wins {
            *selected = Some(candidate);
        }
        Ok(())
    };
    match update() {
        Ok(()) => Ok(()),
        Err(EvalFailure::Sparql(error))
            if matches!(
                error.kind(),
                SparqlErrorKind::NotBound | SparqlErrorKind::Type
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(uncaught(error)),
    }
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn accumulate_source_group_concat<T>(
    evaluator: &NullableAggregateEvaluator<'_, T>,
    values: &mut Vec<SolutionValue<RawExpressionValue>>,
    tuple: &InternalTuple<T>,
    source_seen: Option<&mut FxHashSet<SolutionValue<SourceTermKey>>>,
) -> Result<(), QueryEvaluationError> {
    if let Some(seen) = source_seen.as_ref() {
        // Accumulator.use_row is outside GroupConcat.update's NotBound catch.
        // The update below must evaluate again and retain its own object.
        let candidate = evaluator.evaluate(tuple).map_err(uncaught)?;
        if seen.contains(&aggregate_source_key(&candidate)?) {
            return Ok(());
        }
    }
    let Some(value) = evaluated(evaluator.evaluate(tuple))? else {
        return Ok(());
    };
    let AggregateValue::RawExpression(raw) = value else {
        return Err(QueryEvaluationError::Unexpected(Box::new(
            RawNumericFault::SourceMetadataWouldBeLost,
        )));
    };
    if matches!(&raw, SolutionValue::ReturnedError(error) if error.kind() == SparqlErrorKind::NotBound)
    {
        return Ok(());
    }
    values.try_reserve(1).map_err(|_| {
        QueryEvaluationError::Unexpected(Box::new(group_concat_resource("allocation")))
    })?;
    values.push(raw);
    if let Some(seen) = source_seen {
        // The source appends first, then inserts the second evaluation into seen.
        let key = values.last().expect("just appended").try_map_rdf(|value| {
            source_identity_key(value)
                .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))
        })?;
        seen.try_reserve(1).map_err(|_| {
            QueryEvaluationError::Unexpected(Box::new(group_concat_resource("allocation")))
        })?;
        seen.insert(key);
    }
    Ok(())
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn finish_source_group_concat(
    session: &QueryNumericSession,
    values: &[SolutionValue<RawExpressionValue>],
    separator: &str,
) -> Result<RawExpressionValue, RawNumericFault> {
    let mut output = String::new();
    let mut characters = 0usize;
    // A separator is used only between retained values. Even a very long
    // unused separator must not reject an empty or singleton aggregate.
    let separator_characters = if values.len() > 1 {
        separator.chars().count()
    } else {
        0
    };
    for (index, value) in values.iter().enumerate() {
        let text = session.source_text(value)?;
        let prefix = if index == 0 { "" } else { separator };
        let added_characters = text
            .chars()
            .count()
            .checked_add(if index == 0 { 0 } else { separator_characters })
            .ok_or_else(|| group_concat_resource("output_characters"))?;
        characters = characters
            .checked_add(added_characters)
            .filter(|count| *count <= session.limits().max_output_characters)
            .ok_or_else(|| group_concat_resource("output_characters"))?;
        let added_bytes = text
            .len()
            .checked_add(prefix.len())
            .ok_or_else(|| group_concat_resource("allocation"))?;
        output
            .try_reserve(added_bytes)
            .map_err(|_| group_concat_resource("allocation"))?;
        output.push_str(prefix);
        output.push_str(&text);
    }
    // Aggregate output has its own limit; it is not a fresh external input.
    session.construct_source_plain_result(output)
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn accumulate_numeric<T: Clone + Eq + Hash>(
    evaluator: &NullableAggregateEvaluator<'_, T>,
    session: &QueryNumericSession,
    state: &mut NumericAggregateState,
    seen: &mut Option<FxHashSet<AggregateValue<T>>>,
    tuple: &InternalTuple<T>,
    average: bool,
    source_seen: Option<&mut FxHashSet<SolutionValue<SourceTermKey>>>,
) -> Result<bool, QueryEvaluationError> {
    let mut tracker = seen.as_mut().map(DistinctTracker::Rdf);
    if let Some(seen) = source_seen {
        tracker = Some(DistinctTracker::Source(seen));
    }
    if let Some(seen) = &tracker {
        // Source use_row is outside update's catch, including NotBound.
        let candidate = evaluator.evaluate(tuple).map_err(uncaught)?;
        if seen.contains(&candidate)? {
            return Ok(false);
        }
    }
    let mut update = || {
        let value = evaluator.evaluate(tuple)?;
        let AggregateValue::RawExpression(raw) = &value else {
            return Err(EvalFailure::Fatal(QueryEvaluationError::Unexpected(
                "source numeric aggregate requires its raw codec".into(),
            )));
        };
        if average {
            session.accumulate_average(state, raw)
        } else {
            session.accumulate_sum(state, raw)
        }
        .map_err(numeric_failure)?;
        Ok(value)
    };
    match update() {
        Ok(value) => {
            if let Some(seen) = &mut tracker {
                seen.insert(value)?;
            }
            Ok(true)
        }
        Err(EvalFailure::Sparql(error))
            if error.kind() == SparqlErrorKind::NotBound
                || (average && error.kind() == SparqlErrorKind::Type) =>
        {
            Ok(false)
        }
        Err(error) => Err(uncaught(error)),
    }
}

fn evaluated<T>(
    result: Result<T, EvalFailure<QueryEvaluationError>>,
) -> Result<Option<T>, QueryEvaluationError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(EvalFailure::Sparql(error)) if error.kind() == SparqlErrorKind::NotBound => Ok(None),
        Err(error) => Err(uncaught(error)),
    }
}

impl<T: Clone + Eq + Hash> NullableAccumulator<'_, T> {
    pub(super) fn accumulate(
        &mut self,
        tuple: &InternalTuple<T>,
    ) -> Result<(), QueryEvaluationError> {
        self.accumulate_inner(
            tuple,
            #[cfg(feature = "rdflib760-nullable-internal")]
            None,
        )
    }

    pub(super) fn accumulate_inner(
        &mut self,
        tuple: &InternalTuple<T>,
        #[cfg(feature = "rdflib760-nullable-internal")] source_seen: Option<
            &mut FxHashSet<SolutionValue<SourceTermKey>>,
        >,
    ) -> Result<(), QueryEvaluationError> {
        match self {
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Extremum {
                evaluator,
                session,
                selected,
                kind,
            } => {
                accumulate_source_extremum(evaluator, session, selected, *kind, tuple)?;
            }
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::GroupConcat {
                evaluator, values, ..
            } => accumulate_source_group_concat(evaluator, values, tuple, source_seen)?,
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Sum {
                evaluator,
                session,
                state,
                seen,
            } => {
                accumulate_numeric(evaluator, session, state, seen, tuple, false, source_seen)?;
            }
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Average {
                evaluator,
                session,
                state,
                seen,
                count,
            } => {
                if accumulate_numeric(evaluator, session, state, seen, tuple, true, source_seen)? {
                    *count = session
                        .increment_average_count(count)
                        .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))?;
                }
            }
            Self::Count {
                evaluator,
                seen,
                count,
            } => {
                let mut tracker = seen.as_mut().map(DistinctTracker::Rdf);
                #[cfg(feature = "rdflib760-nullable-internal")]
                if let Some(seen) = source_seen {
                    tracker = Some(DistinctTracker::Source(seen));
                }
                if let Some(seen) = &tracker {
                    // Original Counter.use_row evaluates once, then update
                    // evaluates again and stores that second object's identity.
                    let Some(candidate) = evaluated(evaluator.evaluate(tuple))? else {
                        return Ok(());
                    };
                    if seen.contains(&candidate)? {
                        return Ok(());
                    }
                }
                let Some(value) = evaluated(evaluator.evaluate(tuple))? else {
                    return Ok(());
                };
                *count = count.checked_add(1).ok_or_else(|| {
                    QueryEvaluationError::Unexpected(
                        "nullable COUNT exceeds supported integer range".into(),
                    )
                })?;
                if let Some(seen) = &mut tracker {
                    seen.insert(value)?;
                }
            }
            Self::Sample {
                evaluator,
                selected,
            } => {
                if selected.is_none() {
                    *selected = evaluated(evaluator.evaluate(tuple))?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<Option<AggregateValue<T>>, QueryEvaluationError> {
        Ok(Some(match self {
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Extremum {
                session, selected, ..
            } => {
                let Some(selected) = selected else {
                    return Ok(None);
                };
                return session
                    .construct_source_extremum_result(&selected)
                    .map(|raw| {
                        raw.map(|raw| AggregateValue::RawExpression(SolutionValue::Rdf(raw)))
                    })
                    .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)));
            }
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::GroupConcat {
                session,
                values,
                separator,
                ..
            } => AggregateValue::RawExpression(SolutionValue::Rdf(
                finish_source_group_concat(&session, &values, &separator)
                    .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))?,
            )),
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Sum { session, state, .. } => AggregateValue::RawExpression(SolutionValue::Rdf(
                session
                    .finish_sum(&state)
                    .map_err(|error| uncaught(numeric_failure(error)))?,
            )),
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Average {
                session,
                state,
                count,
                ..
            } => AggregateValue::RawExpression(SolutionValue::Rdf(
                session
                    .finish_average(&state, &count)
                    .map_err(|error| uncaught(numeric_failure(error)))?,
            )),
            Self::Count { count, .. } => AggregateValue::Expression(SolutionValue::Rdf(
                ExpressionTerm::IntegerLiteral(count.into()),
            )),
            Self::Sample { selected, .. } => {
                selected.unwrap_or(AggregateValue::Internal(SolutionValue::ExplicitNone))
            }
        }))
    }
}

impl<'a, D: QueryableDataset<'a>> SimpleEvaluator<'a, D> {
    pub(super) fn nullable_aggregate_evaluator(
        &self,
        expression: &Expression,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<NullableAggregateEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        let mut context = ExpressionContext {
            evaluator: self,
            encoded_variables: variables,
            stat_children: stats,
        };
        #[cfg(feature = "rdflib760-nullable-internal")]
        if let Some(hooks) = self.dataset.numeric_hooks() {
            // Literals and variables use the same numeric compiler too: the
            // old literal fast path would internalize early, while variables
            // must preserve their instance's cache through COUNT and SAMPLE.
            let evaluator = build_numeric_expression_evaluator(expression, &mut context, hooks)?;
            let token = self.dataset.cancellation_token.clone();
            return Ok(NullableAggregateEvaluator::RawExpression(Rc::new(
                move |row| {
                    token.ensure_alive().map_err(EvalFailure::Fatal)?;
                    evaluator(row)
                },
            )));
        }
        if let Expression::Variable(variable) = expression {
            let lookup = context.build_solution_variable_lookup(variable);
            let token = self.dataset.cancellation_token.clone();
            return Ok(NullableAggregateEvaluator::Internal(Rc::new(move |row| {
                token.ensure_alive().map_err(EvalFailure::Fatal)?;
                lookup(row).ok_or_else(|| {
                    EvalFailure::Sparql(ErrorValue::with_kind(
                        SparqlErrorKind::NotBound,
                        "variable has no local or initial binding",
                    ))
                })
            })));
        }
        let literal = match expression {
            Expression::NamedNode(value) => Some(context.internalize_named_node(value)?),
            Expression::Literal(value) => Some(context.internalize_literal(value)?),
            _ => None,
        };
        if let Some(value) = literal {
            let token = self.dataset.cancellation_token.clone();
            return Ok(NullableAggregateEvaluator::Internal(Rc::new(move |_| {
                token.ensure_alive().map_err(EvalFailure::Fatal)?;
                Ok(SolutionValue::Rdf(value.clone()))
            })));
        }
        let evaluator = build_raw_expression_evaluator(expression, &mut context)?;
        let token = self.dataset.cancellation_token.clone();
        Ok(NullableAggregateEvaluator::Expression(Rc::new(
            move |row| {
                token.ensure_alive().map_err(EvalFailure::Fatal)?;
                evaluator(row)
            },
        )))
    }

    pub(super) fn nullable_boolean_evaluator(
        &self,
        expression: &Expression,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<
        Rc<dyn Fn(&InternalTuple<D::InternalTerm>) -> Result<bool, QueryEvaluationError> + 'a>,
        QueryEvaluationError,
    > {
        let evaluator = self.nullable_aggregate_evaluator(expression, variables, stats)?;
        let dataset = self.dataset.clone();
        // The sealed graph extension consumes the original operators.EBV;
        // earlier numeric factories retain typed_ebv admission and semantics.
        #[cfg(feature = "rdflib760-nullable-internal")]
        let source_ebv = dataset.numeric.as_ref().is_some_and(|codec| {
            codec
                .prepared_literals
                .as_ref()
                .is_some_and(|registry| registry.admits_graph_execution(&codec.session))
        });
        Ok(Rc::new(move |row| {
            // Active safety contract: infrastructure failures propagate even
            // for bare variables, unlike the original Python broad except.
            match evaluator.evaluate(row) {
                Ok(AggregateValue::Internal(SolutionValue::Rdf(value))) => {
                    #[cfg(feature = "rdflib760-nullable-internal")]
                    if source_ebv {
                        let codec = dataset.numeric.as_ref().expect("admitted graph codec");
                        let value = (codec.externalize)(value)?;
                        return match codec
                            .session
                            .source_effective_boolean_value(&SolutionValue::Rdf(value))
                        {
                            Ok(value) => Ok(value),
                            Err(NumericEvaluationError::Sparql { .. }) => Ok(false),
                            Err(NumericEvaluationError::Fatal(error)) => {
                                Err(QueryEvaluationError::Unexpected(Box::new(error)))
                            }
                        };
                    }
                    Ok(dataset
                        .internal_term_effective_boolean_value(value)?
                        .unwrap_or(false))
                }
                Ok(AggregateValue::Expression(SolutionValue::Rdf(value))) => {
                    Ok(value.effective_boolean_value().unwrap_or(false))
                }
                #[cfg(feature = "rdflib760-nullable-internal")]
                Ok(AggregateValue::RawExpression(SolutionValue::Rdf(value))) => {
                    let codec = dataset.numeric.as_ref().ok_or_else(|| {
                        QueryEvaluationError::Unexpected(
                            "numeric Boolean evaluation requires its prepared-query codec".into(),
                        )
                    })?;
                    let evaluated = if source_ebv {
                        codec
                            .session
                            .source_effective_boolean_value(&SolutionValue::Rdf(value))
                    } else {
                        codec.session.typed_ebv(&value)
                    };
                    match evaluated {
                        Ok(value) => Ok(value),
                        Err(NumericEvaluationError::Sparql { .. }) => Ok(false),
                        Err(NumericEvaluationError::Fatal(error)) => {
                            Err(QueryEvaluationError::Unexpected(Box::new(error)))
                        }
                    }
                }
                Ok(_) | Err(EvalFailure::Sparql(_)) => Ok(false),
                Err(EvalFailure::Fatal(error)) => Err(error),
            }
        }))
    }

    pub(super) fn nullable_accumulator_builder(
        &self,
        expression: &AggregateExpression,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<Box<dyn Fn() -> AccumulatorWrapper<'a, D::InternalTerm> + 'a>, QueryEvaluationError>
    {
        match expression {
            #[cfg(feature = "rdflib760-nullable-internal")]
            AggregateExpression::FunctionCall {
                name: name @ (AggregateFunction::Min | AggregateFunction::Max),
                expr,
                ..
            } if self.dataset.source_literal_codec().is_some_and(|codec| {
                codec.session.group_semantics() == QueryGroupSemantics::SourceAggregateV1
            }) =>
            {
                let codec = self.dataset.source_literal_codec().ok_or_else(|| {
                    QueryEvaluationError::Unexpected(Box::new(
                        RawNumericFault::UnsupportedSourceLiteralOperator(
                            "MIN/MAX requires source literal semantics",
                        ),
                    ))
                })?;
                let evaluator = self.nullable_aggregate_evaluator(expr, variables, stats)?;
                let session = Rc::clone(&codec.session);
                let kind = if *name == AggregateFunction::Min {
                    SourceExtremumKind::Minimum
                } else {
                    SourceExtremumKind::Maximum
                };
                // The original Extremum.dont_care deliberately ignores DISTINCT.
                Ok(Box::new(move || {
                    AccumulatorWrapper::Nullable(NullableAccumulator::Extremum {
                        evaluator: evaluator.clone(),
                        session: Rc::clone(&session),
                        selected: None,
                        kind,
                    })
                }))
            }
            #[cfg(feature = "rdflib760-nullable-internal")]
            AggregateExpression::FunctionCall {
                name: AggregateFunction::GroupConcat { separator },
                expr,
                distinct,
            } if self.dataset.source_literal_codec().is_some_and(|codec| {
                codec.session.group_semantics() == QueryGroupSemantics::SourceAggregateV1
            }) =>
            {
                let codec = self
                    .dataset
                    .source_literal_codec()
                    .filter(|codec| {
                        codec.session.group_semantics() == QueryGroupSemantics::SourceAggregateV1
                    })
                    .ok_or_else(|| {
                        QueryEvaluationError::Unexpected(Box::new(
                            RawNumericFault::UnsupportedSourceLiteralOperator(
                                "GROUP_CONCAT requires source group and literal semantics",
                            ),
                        ))
                    })?;
                let evaluator = self.nullable_aggregate_evaluator(expr, variables, stats)?;
                let session = Rc::clone(&codec.session);
                let separator: Rc<str> = separator.as_deref().unwrap_or(" ").into();
                let distinct = *distinct;
                Ok(Box::new(move || {
                    let accumulator = NullableAccumulator::GroupConcat {
                        evaluator: evaluator.clone(),
                        session: Rc::clone(&session),
                        values: Vec::new(),
                        separator: Rc::clone(&separator),
                    };
                    if distinct {
                        AccumulatorWrapper::SourceDistinct {
                            accumulator,
                            seen: FxHashSet::default(),
                        }
                    } else {
                        AccumulatorWrapper::Nullable(accumulator)
                    }
                }))
            }
            #[cfg(feature = "rdflib760-nullable-internal")]
            AggregateExpression::FunctionCall {
                name: name @ (AggregateFunction::Sum | AggregateFunction::Avg),
                expr,
                distinct,
            } => {
                let codec = self
                    .dataset
                    .numeric
                    .as_ref()
                    .filter(|codec| {
                        codec.session.group_semantics() == QueryGroupSemantics::SourceAggregateV1
                    })
                    .ok_or_else(|| {
                        QueryEvaluationError::Unexpected(
                            "source numeric aggregates require bound group semantics".into(),
                        )
                    })?;
                let evaluator = self.nullable_aggregate_evaluator(expr, variables, stats)?;
                let session = Rc::clone(&codec.session);
                let distinct = *distinct;
                let average = *name == AggregateFunction::Avg;
                let source_distinct = distinct
                    && session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1;
                Ok(Box::new(move || {
                    let evaluator = evaluator.clone();
                    let session = Rc::clone(&session);
                    let state = NumericAggregateState::new();
                    let seen = (distinct && !source_distinct).then(FxHashSet::default);
                    let accumulator = if average {
                        NullableAccumulator::Average {
                            evaluator,
                            session,
                            state,
                            seen,
                            count: BigInt::from(0),
                        }
                    } else {
                        NullableAccumulator::Sum {
                            evaluator,
                            session,
                            state,
                            seen,
                        }
                    };
                    if source_distinct {
                        AccumulatorWrapper::SourceDistinct {
                            accumulator,
                            seen: FxHashSet::default(),
                        }
                    } else {
                        AccumulatorWrapper::Nullable(accumulator)
                    }
                }))
            }
            AggregateExpression::CountSolutions { distinct: false } => {
                Ok(Box::new(|| AccumulatorWrapper::CountTuple { count: 0 }))
            }
            AggregateExpression::CountSolutions { distinct: true } => {
                #[cfg(feature = "rdflib760-nullable-internal")]
                if let Some(codec) = self.dataset.source_literal_codec() {
                    let key = Rc::clone(&codec.source_identity);
                    return Ok(Box::new(move || {
                        AccumulatorWrapper::SourceCountDistinctTuple {
                            key: Rc::clone(&key),
                            count: 0,
                            seen: FxHashSet::default(),
                        }
                    }));
                }
                Ok(Box::new(|| AccumulatorWrapper::CountDistinctTuple {
                    count: 0,
                    seen: FxHashSet::default(),
                }))
            }
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Count,
                expr,
                distinct,
            } => {
                let evaluator = self.nullable_aggregate_evaluator(expr, variables, stats)?;
                let distinct = *distinct;
                #[cfg(feature = "rdflib760-nullable-internal")]
                if distinct && self.dataset.source_literal_codec().is_some() {
                    return Ok(Box::new(move || AccumulatorWrapper::SourceDistinct {
                        accumulator: NullableAccumulator::Count {
                            evaluator: evaluator.clone(),
                            seen: None,
                            count: 0,
                        },
                        seen: FxHashSet::default(),
                    }));
                }
                Ok(Box::new(move || {
                    AccumulatorWrapper::Nullable(NullableAccumulator::Count {
                        evaluator: evaluator.clone(),
                        seen: distinct.then(FxHashSet::default),
                        count: 0,
                    })
                }))
            }
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Sample,
                expr,
                ..
            } => {
                let evaluator = self.nullable_aggregate_evaluator(expr, variables, stats)?;
                Ok(Box::new(move || {
                    AccumulatorWrapper::Nullable(NullableAccumulator::Sample {
                        evaluator: evaluator.clone(),
                        selected: None,
                    })
                }))
            }
            _ => Err(QueryEvaluationError::Unexpected(
                "unsupported nullable aggregate".into(),
            )),
        }
    }
}
