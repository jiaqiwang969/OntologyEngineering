//! Source Group lifecycle on the existing dataset, expression compiler and
//! accumulator owner. A key's identity never includes its cached numeric value.
use super::*;
use crate::numeric::{QueryGroupSemantics, SourceContextFault};
use crate::tuple::SparqlErrorKind;
use spargebra::algebra::{
    SourceGroupCondition, SourceGroupConditionContext, SourceGroupConditionSyntax, SourceGrouping,
};

#[derive(Eq, PartialEq, Hash)]
enum AggregateGroupIdentity<T> {
    Rdf(AggregateValue<T>),
    Source(SolutionValue<SourceTermKey>),
}

impl<'a, D: QueryableDataset<'a>> SimpleEvaluator<'a, D> {
    pub(super) fn source_group_evaluator(
        &self,
        inner: &GraphPattern,
        grouping: &SourceGrouping<Expression>,
        aggregates: &[(Variable, AggregateExpression)],
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        self.source_group_evaluator_impl(inner, grouping, aggregates, None, variables, stats)
    }

    /// The context annotation preserves the actual GroupAs translation. All
    /// generated Extends run in source order before any grouping key is read.
    /// A missing AS target executes that Extend's expression/catch/internalize
    /// path, but does not add support for arbitrary Python mapping keys: its
    /// subsequent None grouping key raises at the actual key-consumption step.
    /// Output tuples contain only the new aggregate local domain; the shared
    /// context runtime continues to own the immutable initial bindings.
    pub(super) fn source_context_group_evaluator(
        &self,
        inner: &GraphPattern,
        grouping: &SourceGrouping<Expression>,
        aggregates: &[(Variable, AggregateExpression)],
        conditions: &[SourceGroupConditionContext],
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        if self.source_context.is_none() {
            return Err(QueryEvaluationError::Unexpected(Box::new(
                RawNumericFault::InvalidSourceOrigin,
            )));
        }
        self.source_group_evaluator_impl(
            inner,
            grouping,
            aggregates,
            Some(conditions),
            variables,
            stats,
        )
    }

    fn source_group_evaluator_impl(
        &self,
        inner: &GraphPattern,
        grouping: &SourceGrouping<Expression>,
        aggregates: &[(Variable, AggregateExpression)],
        contexts: Option<&[SourceGroupConditionContext]>,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        if !self.nullable_expressions
            || !self.dataset.numeric.as_ref().is_some_and(|codec| {
                codec.session.group_semantics() == QueryGroupSemantics::SourceAggregateV1
            })
        {
            return Err(QueryEvaluationError::Unexpected(
                "SourceGroup requires its prepared numeric session".into(),
            ));
        }
        let implicit = matches!(grouping, SourceGrouping::Implicit);
        let conditions = match grouping {
            SourceGrouping::Implicit => &[][..],
            SourceGrouping::Explicit(conditions) if !conditions.is_empty() => conditions.as_slice(),
            SourceGrouping::Explicit(_) => {
                return Err(QueryEvaluationError::Unexpected(
                    "explicit source grouping requires at least one condition".into(),
                ));
            }
        };
        // These are parser-produced occurrence facts. Do not infer parentheses
        // from an expression after lowering, or accept a scope for another node.
        if let Some(contexts) = contexts {
            if contexts.len() != conditions.len()
                || !conditions.iter().zip(contexts).all(|(condition, context)| {
                    matches!(
                        (condition, context.syntax, &context.assignment_scope),
                        (
                            SourceGroupCondition::Raw(Expression::Variable(_)),
                            SourceGroupConditionSyntax::BareVariable,
                            None
                        ) | (
                            SourceGroupCondition::Raw(_),
                            SourceGroupConditionSyntax::BareFunction,
                            None
                        ) | (
                            SourceGroupCondition::Raw(_),
                            SourceGroupConditionSyntax::ParenthesizedWithoutAlias,
                            Some(_)
                        ) | (
                            SourceGroupCondition::As { .. },
                            SourceGroupConditionSyntax::ParenthesizedAlias,
                            Some(_)
                        )
                    )
                })
            {
                return Err(QueryEvaluationError::Unexpected(Box::new(
                    RawNumericFault::InvalidSourceOrigin,
                )));
            }
        }
        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
        stats.push(child_stats);
        let mut child = child?;
        // algebra.py constructs every AS Extend before evalAggregateJoin reads
        // any key. Use the exact same helper as an ordinary parsed Extend.
        for (index, condition) in conditions.iter().enumerate() {
            if let Some(contexts) = contexts {
                if let Some(scope) = &contexts[index].assignment_scope {
                    let (expression, variable) = match condition {
                        SourceGroupCondition::Raw(expression) => (expression, None),
                        SourceGroupCondition::As {
                            expression,
                            variable,
                        } => (expression, Some(variable)),
                    };
                    child = self.source_extend_evaluator(
                        child, variable, expression, scope, variables, stats,
                    )?;
                }
            } else if let SourceGroupCondition::As {
                expression,
                variable,
            } = condition
            {
                child = self.extend_evaluator(child, variable, expression, variables, stats)?;
            }
        }
        let keys = conditions
            .iter()
            .enumerate()
            .map(|(index, condition)| {
                if contexts.is_some_and(|contexts| {
                    contexts[index].syntax == SourceGroupConditionSyntax::ParenthesizedWithoutAlias
                }) {
                    // Keep this slot in the original key order. Deferring the
                    // failure preserves all prior Extend effects (including a
                    // trapped division's sticky flag) and earlier key failures.
                    return Ok(None);
                }
                let alias;
                let expression = match condition {
                    SourceGroupCondition::Raw(expression) => expression,
                    SourceGroupCondition::As { variable, .. } => {
                        alias = Expression::Variable(variable.clone());
                        &alias
                    }
                };
                let missing_is_none = matches!(expression, Expression::Variable(_));
                Ok(Some((
                    self.nullable_aggregate_evaluator(expression, variables, stats)?,
                    missing_is_none,
                )))
            })
            .collect::<Result<Vec<_>, QueryEvaluationError>>()?;
        let builders = aggregates
            .iter()
            .map(|(_, expression)| self.accumulator_builder(expression, variables, stats))
            .collect::<Result<Vec<_>, _>>()?;
        let targets: Vec<_> = aggregates
            .iter()
            .map(|(variable, _)| encode_variable(variables, variable))
            .collect();
        let tuple_size = variables.len();
        let dataset = self.dataset.clone();
        let source_identity = dataset.source_literal_codec().is_some();
        let source_context = self.source_context.is_some();
        let initialize: InternalTupleEvaluator<'a, D::InternalTerm> = Rc::new(move |from| {
            // RDFLib evalAggregateJoin freezes every finished group with the
            // context passed to this aggregate, never the last input row's ctx.
            let group_context = source_context.then(|| from.clone());
            let rows: InternalTuplesIterator<'a, D::InternalTerm> = {
                let mut index: FxHashMap<Vec<AggregateGroupIdentity<D::InternalTerm>>, usize> =
                    FxHashMap::default();
                let mut groups: Vec<Vec<AccumulatorWrapper<'a, D::InternalTerm>>> = Vec::new();
                if implicit {
                    groups.push(builders.iter().map(|builder| builder()).collect());
                }
                // All input/update failures precede any output. Do not append an
                // error after otherwise valid-looking groups from partial input.
                for row in child(from)? {
                    let row = match row {
                        Ok(row) => row,
                        Err(error) => return Ok(Box::new(once(Err(error)))),
                    };
                    if let Err(error) = dataset.cancellation_token.ensure_alive() {
                        return Ok(Box::new(once(Err(error))));
                    }
                    let position = if implicit {
                        0
                    } else {
                        let mut key = Vec::with_capacity(keys.len());
                        for key_evaluator in &keys {
                            let Some((evaluator, missing_is_none)) = key_evaluator else {
                                return Ok(Box::new(once(Err(QueryEvaluationError::Unexpected(
                                    Box::new(RawNumericFault::SourceContext(
                                        SourceContextFault::NullGroupKey,
                                    )),
                                )))));
                            };
                            let value = match evaluator.evaluate(&row) {
                                Ok(value) => value,
                                Err(EvalFailure::Sparql(error))
                                    if *missing_is_none
                                        && error.kind() == SparqlErrorKind::NotBound =>
                                {
                                    AggregateValue::RawExpression(SolutionValue::ExplicitNone)
                                }
                                Err(error) => {
                                    return Ok(Box::new(once(Err(nullable::uncaught(error)))));
                                }
                            };
                            key.push(if source_identity {
                                match nullable::aggregate_source_key(&value) {
                                    Ok(key) => AggregateGroupIdentity::Source(key),
                                    Err(error) => return Ok(Box::new(once(Err(error)))),
                                }
                            } else {
                                AggregateGroupIdentity::Rdf(value)
                            });
                        }
                        *index.entry(key).or_insert_with(|| {
                            let next = groups.len();
                            groups.push(builders.iter().map(|builder| builder()).collect());
                            next
                        })
                    };
                    for accumulator in &mut groups[position] {
                        if let Err(error) = accumulator.accumulate(&row) {
                            return Ok(Box::new(once(Err(error))));
                        }
                    }
                }
                if groups.is_empty() {
                    // Explicit GROUP over empty input yields one empty binding,
                    // not an artificial accumulator's zeros or pending SAMPLE.
                    let result = InternalTuple::with_capacity(tuple_size);
                    let result = group_context.as_ref().map_or_else(
                        || result.clone(),
                        |context| result.source_with_context_of(context),
                    );
                    return Ok(Box::new(once(Ok(result))));
                }
                let targets = targets.clone();
                let dataset = dataset.clone();
                // Finish remains lazy in first-seen order. LIMIT can stop before a
                // later group raises a trap; a failing group never yields half a row.
                Box::new(nullable::halt_after_error(groups.into_iter().map(
                    move |group| {
                        dataset.cancellation_token.ensure_alive()?;
                        let mut result = InternalTuple::with_capacity(tuple_size);
                        for (accumulator, target) in group.into_iter().zip(&targets) {
                            if let Some(value) = accumulator.finish()? {
                                result.set_cell(
                                    *target,
                                    dataset.internalize_aggregate_value(value, true)?,
                                );
                            }
                        }
                        if let Some(context) = &group_context {
                            result = result.source_with_context_of(context);
                        }
                        Ok(result)
                    },
                )))
            };
            Ok(rows)
        });
        Ok(Rc::new(move |from| {
            let initialize = Rc::clone(&initialize);
            // evalAggregateJoin is a source generator. Constructing its
            // iterator must not read input or raise numeric signals. The first
            // actual poll performs all updates before finishing the first group;
            // a non-polled LIMIT 0 never starts this work.
            Ok(deferred_start(initialize, from))
        }))
    }
}
