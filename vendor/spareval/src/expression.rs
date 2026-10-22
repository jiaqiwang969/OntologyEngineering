#[cfg(feature = "sparql-12")]
use crate::ExpressionTriple;
use crate::dataset::ExpressionTerm;
#[cfg(feature = "rdflib760-nullable-internal")]
use crate::numeric::{
    NumericBinaryOp, NumericChainKind, NumericEvaluationError, NumericInputOrigin, NumericUnaryOp,
    QueryLiteralSemantics, QueryNumericSession, RawExpressionValue, RawNumericFault,
    SourceOrderKey,
};
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
use crate::tuple::ErrorValue;
use crate::tuple::SolutionValue;
mod nullable;
#[cfg(feature = "rdflib760-nullable-internal")]
pub(crate) mod source_functions;
use md5::{Digest, Md5};
use nullable::{ArithmeticChainKind, ArithmeticOp, ArithmeticOperand, UnaryArithmeticOp};
pub(crate) use nullable::{EvalFailure, RawEvaluator};
use oxiri::Iri;
use oxrdf::vocab::{rdf, xsd};
#[cfg(feature = "sparql-12")]
use oxrdf::{BaseDirection, NamedOrBlankNode};
use oxrdf::{BlankNode, Literal, NamedNode, Term, Variable};
#[cfg(feature = "sep-0002")]
use oxsdatatypes::{Date, DayTimeDuration, Duration, Time, TimezoneOffset, YearMonthDuration};
use oxsdatatypes::{DateTime, Decimal, Double, Float, Integer};
use rand::random;
use regex::{Regex, RegexBuilder};
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};
use spargebra::algebra::{AdditiveOperator, Function, MultiplicativeOperator};
use sparopt::algebra::{Expression, GraphPattern};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::Arc;
use thiserror::Error;

pub type CustomFunctionRegistry =
    HashMap<NamedNode, Arc<dyn (Fn(&[Term]) -> Option<Term>) + Send + Sync>>;

const REGEX_SIZE_LIMIT: usize = 1_000_000;

pub trait ExpressionEvaluatorContext<'a> {
    type Term: Clone + Eq + 'a;
    type Tuple: 'a;
    type Error: 'a;

    fn build_variable_lookup(
        &mut self,
        variable: &Variable,
    ) -> impl Fn(&Self::Tuple) -> Option<Self::Term> + 'a;
    fn build_solution_variable_lookup(
        &mut self,
        variable: &Variable,
    ) -> impl Fn(&Self::Tuple) -> Option<SolutionValue<Self::Term>> + 'a;
    fn build_internalize_expression_term_result(
        &mut self,
    ) -> impl Fn(ExpressionTerm) -> Result<Self::Term, Self::Error> + 'a;
    fn build_externalize_expression_term_result(
        &mut self,
    ) -> impl Fn(Self::Term) -> Result<ExpressionTerm, Self::Error> + 'a;
    fn unsupported_nullable_evaluation(message: &'static str) -> Self::Error;
    fn build_is_variable_bound(
        &mut self,
        variable: &Variable,
    ) -> impl Fn(&Self::Tuple) -> bool + 'a;
    fn build_exists(
        &mut self,
        plan: &GraphPattern,
    ) -> Result<impl Fn(&Self::Tuple) -> bool + 'a, Self::Error>;
    #[cfg(feature = "rdflib760-nullable-internal")]
    fn build_source_exists(
        &mut self,
        _: &spargebra::SourceExistsExpression<GraphPattern>,
        _: &Rc<QueryNumericSession>,
        _: &Rc<crate::numeric::PreparedSourceLiteralRegistry>,
    ) -> Result<Option<SourceExistsEvaluator<'a, Self::Tuple, Self::Error>>, Self::Error> {
        Ok(None)
    }
    #[cfg(feature = "rdflib760-nullable-internal")]
    fn admits_source_execution(
        &self,
        _: &Rc<QueryNumericSession>,
        _: &Rc<crate::numeric::PreparedSourceLiteralRegistry>,
    ) -> bool {
        false
    }
    #[cfg(feature = "rdflib760-nullable-internal")]
    fn source_native_fault(_: &Self::Error) -> Option<&RawNumericFault> {
        None
    }
    #[cfg(feature = "rdflib760-nullable-internal")]
    fn build_source_function_bindings(
        &mut self,
    ) -> Option<SourceFunctionBindingLookup<'a, Self::Tuple, Self::Term, Self::Error>> {
        None
    }
    fn internalize_named_node(&mut self, term: &NamedNode) -> Result<Self::Term, Self::Error>;
    fn internalize_literal(&mut self, term: &Literal) -> Result<Self::Term, Self::Error>;
    fn build_internalize_expression_term(
        &mut self,
    ) -> impl Fn(ExpressionTerm) -> Option<Self::Term> + 'a; // TODO: return result
    fn build_externalize_expression_term(
        &mut self,
    ) -> impl Fn(Self::Term) -> Option<ExpressionTerm> + 'a; // TODO: return result
    fn now(&mut self) -> DateTime;
    fn base_iri(&mut self) -> Option<Arc<Iri<String>>>;
    fn custom_functions(&mut self) -> &CustomFunctionRegistry;
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(crate) type SourceFunctionBindingLookup<'a, I, T, F> =
    Rc<dyn Fn(&I, &Variable) -> Result<Option<SolutionValue<T>>, F> + 'a>;

pub type ExpressionEvaluator<'a, I, O> = Rc<dyn (Fn(&I) -> Option<O>) + 'a>;

#[cfg(feature = "rdflib760-nullable-internal")]
pub(crate) type SourceExistsEvaluator<'a, I, F> =
    Rc<dyn Fn(&I) -> Result<bool, EvalFailure<F>> + 'a>;

#[derive(Debug, Error)]
pub enum ExpressionEvaluationError<C> {
    #[error("This expression is not supported by the private nullable expression boundary")]
    UnsupportedNullableExpression,
    /// Error from the evaluation context
    #[error(transparent)]
    Context(C),
    /// The given custom function is not supported
    #[error("The custom function {0} is not supported")]
    UnsupportedCustomFunction(NamedNode),
    /// The given custom function arity is not supported
    #[error("The custom function {name} requires between {} and {} arguments, but {actual} were given", .expected.start(), .expected.end())]
    UnsupportedCustomFunctionArity {
        name: NamedNode,
        expected: RangeInclusive<usize>,
        actual: usize,
    },
}

pub fn build_expression_evaluator<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
) -> Result<ExpressionEvaluator<'a, C::Tuple, ExpressionTerm>, ExpressionEvaluationError<C::Error>>
{
    build_compiled_expression(expression, context, false, None)?
        .legacy
        .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)
}

pub(crate) fn build_raw_expression_evaluator<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
) -> Result<RawEvaluator<'a, C::Tuple, ExpressionTerm, C::Error>, ExpressionEvaluationError<C::Error>>
{
    build_compiled_expression(expression, context, true, None)?
        .raw
        .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(crate) struct NumericExpressionHooks<'a, T, F> {
    pub(crate) session: Rc<QueryNumericSession>,
    pub(crate) prepared_literals: Option<Rc<crate::numeric::PreparedSourceLiteralRegistry>>,
    pub(crate) externalize: Rc<dyn Fn(T) -> Result<RawExpressionValue, F> + 'a>,
    pub(crate) failure: fn(RawNumericFault) -> F,
}

// The shared dispatcher keeps the same recursive signature without enabling
// any numeric dependency or construction capability in a default build.
#[cfg(not(feature = "rdflib760-nullable-internal"))]
type NumericExpressionHooks<'a, T, F> = std::marker::PhantomData<(&'a (), fn(T) -> F)>;

#[cfg(feature = "rdflib760-nullable-internal")]
pub(crate) fn build_numeric_expression_evaluator<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
    hooks: NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<
    RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error>,
    ExpressionEvaluationError<C::Error>,
> {
    build_compiled_expression(expression, context, false, Some(&hooks))?
        .numeric
        .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(crate) type SourceOrderKeyEvaluator<'a, I, F> =
    Rc<dyn Fn(&I) -> Result<SourceOrderKey, EvalFailure<F>> + 'a>;

/// parserutils.value treats a bare Variable differently from an Expr. Keep
/// this consumer boundary, then reuse the one existing expression compiler.
#[cfg(feature = "rdflib760-nullable-internal")]
pub(crate) fn build_source_order_key_evaluator<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
    hooks: NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<SourceOrderKeyEvaluator<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    if let Expression::Variable(variable) = expression {
        let lookup = context.build_solution_variable_lookup(variable);
        let variable = variable.clone();
        return Ok(Rc::new(move |tuple| match lookup(tuple) {
            None | Some(SolutionValue::ExplicitNone) => {
                Ok(SourceOrderKey::UnboundVariable(variable.clone()))
            }
            Some(SolutionValue::ReturnedError(error)) => Err(EvalFailure::Sparql(error)),
            Some(SolutionValue::Rdf(term)) => (hooks.externalize)(term)
                .map(SourceOrderKey::Rdf)
                .map_err(EvalFailure::Fatal),
        }));
    }
    let expression = build_numeric_expression_evaluator(expression, context, hooks)?;
    Ok(Rc::new(move |tuple| {
        Ok(match expression(tuple)? {
            SolutionValue::Rdf(value) => SourceOrderKey::Rdf(value),
            SolutionValue::ExplicitNone | SolutionValue::ReturnedError(_) => SourceOrderKey::None,
        })
    }))
}

struct CompiledExpression<'a, I, F> {
    legacy: Option<ExpressionEvaluator<'a, I, ExpressionTerm>>,
    raw: Option<RawEvaluator<'a, I, ExpressionTerm, F>>,
    #[cfg(feature = "rdflib760-nullable-internal")]
    numeric: Option<RawEvaluator<'a, I, RawExpressionValue, F>>,
    // Only our bounded arithmetic can certify an integral Decimal result.
    // A Decimal read from an RDF leaf has not preserved its original scale.
    raw_numeric_result: bool,
}

// One AST dispatch owns both boundaries. Unsupported typed branches retain
// their existing legacy implementation, but never manufacture a raw result.
fn build_compiled_expression<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
    raw_requested: bool,
    numeric_hooks: Option<&NumericExpressionHooks<'a, C::Term, C::Error>>,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    let mut raw: Option<RawEvaluator<'a, C::Tuple, ExpressionTerm, C::Error>> = None;
    #[cfg(feature = "rdflib760-nullable-internal")]
    let mut numeric: Option<RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error>> = None;
    let legacy: ExpressionEvaluator<'a, C::Tuple, ExpressionTerm> = match expression {
        Expression::NamedNode(t) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if numeric_hooks.is_some() {
                let value = RawExpressionValue::from_import(t.clone().into());
                numeric = Some(Rc::new(move |_| Ok(SolutionValue::Rdf(value.clone()))));
            }
            let t = ExpressionTerm::from(Term::from(t.clone()));
            if raw_requested {
                let t = t.clone();
                raw = Some(Rc::new(move |_| Ok(SolutionValue::Rdf(t.clone()))));
            }
            Rc::new(move |_| Some(t.clone()))
        }
        Expression::SourceLiteral(literal) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if hooks.session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1 {
                    // Only the actual source parser supplied this occurrence.
                    // Construct once; evaluation delivers any original failure.
                    let value = crate::numeric::resolve_source_literal(
                        &hooks.session,
                        hooks.prepared_literals.as_deref(),
                        literal,
                    );
                    let failure = hooks.failure;
                    return Ok(CompiledExpression {
                        legacy: None,
                        raw: None,
                        numeric: Some(Rc::new(move |_| {
                            value
                                .clone()
                                .map(SolutionValue::Rdf)
                                .map_err(|error| EvalFailure::Fatal(failure(error)))
                        })),
                        raw_numeric_result: false,
                    });
                }
            }
            let _ = literal;
            return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
        }
        Expression::Literal(t) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if hooks.session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1 {
                    return Err(ExpressionEvaluationError::Context((hooks.failure)(
                        RawNumericFault::UnsupportedSourceLiteralOperator(
                            "literal without occurrence facts",
                        ),
                    )));
                }
                // Literal construction happens once. A construction failure is
                // delivered at actual raw evaluation, not erased into None.
                let value = hooks
                    .session
                    .construct_literal(t, NumericInputOrigin::PreparedQueryLiteral);
                let failure = hooks.failure;
                numeric = Some(Rc::new(move |_| {
                    value
                        .clone()
                        .map(SolutionValue::Rdf)
                        .map_err(|error| EvalFailure::Fatal(failure(error)))
                }));
            }
            let t = ExpressionTerm::from(Term::from(t.clone()));
            if raw_requested {
                let t = t.clone();
                raw = Some(Rc::new(move |_| Ok(SolutionValue::Rdf(t.clone()))));
            }
            Rc::new(move |_| Some(t.clone()))
        }
        Expression::Variable(v) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                numeric = Some(build_numeric_variable_evaluator(v, context, hooks, false));
            }
            if raw_requested {
                let lookup = context.build_solution_variable_lookup(v);
                let externalize = context.build_externalize_expression_term_result();
                raw = Some(Rc::new(move |tuple| {
                    match nullable::raw_variable(lookup(tuple))? {
                        SolutionValue::Rdf(term) => externalize(term)
                            .map(SolutionValue::Rdf)
                            .map_err(EvalFailure::Fatal),
                        SolutionValue::ExplicitNone => Ok(SolutionValue::ExplicitNone),
                        SolutionValue::ReturnedError(error) => {
                            Ok(SolutionValue::ReturnedError(error))
                        }
                    }
                }));
            }
            let lookup = context.build_variable_lookup(v);
            let externalize = context.build_externalize_expression_term();
            Rc::new(move |t| externalize(lookup(t)?))
        }
        Expression::Bound(v) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if raw_requested || !source_execution_hooks(context, hooks) {
                    return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
                }
                let lookup = context.build_solution_variable_lookup(v);
                let session = Rc::clone(&hooks.session);
                let externalize = Rc::clone(&hooks.externalize);
                let failure = hooks.failure;
                return Ok(CompiledExpression {
                    legacy: None,
                    raw: None,
                    numeric: Some(Rc::new(move |tuple| {
                        nullable::capture_sparql((|| {
                            let bound = match lookup(tuple) {
                                None | Some(SolutionValue::ExplicitNone) => false,
                                Some(SolutionValue::Rdf(value)) => externalize(value)
                                    .map_err(EvalFailure::Fatal)?.source_variable().is_none(),
                                Some(SolutionValue::ReturnedError(error)) => {
                                    return Err(EvalFailure::Sparql(error));
                                }
                            };
                            session
                                .construct_source_boolean_result(bound)
                                .map(SolutionValue::Rdf)
                                .map_err(|error| EvalFailure::Fatal(failure(error)))
                        })())
                    })),
                    raw_numeric_result: false,
                });
            }
            let lookup = context.build_is_variable_bound(v);
            Rc::new(move |tuple| Some(lookup(tuple).into()))
        }
        Expression::SourceExists(source) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if !raw_requested
                    && hooks.session.context_semantics()
                        == crate::numeric::QueryContextSemantics::SourceContextV1
                    && hooks.session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1
                {
                    if let Some(registry) = hooks
                        .prepared_literals
                        .as_ref()
                        .filter(|registry| registry.admits_exists_execution(&hooks.session))
                    {
                        let exists = context
                            .build_source_exists(source, &hooks.session, registry)
                            .map_err(ExpressionEvaluationError::Context)?
                            .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
                        let session = Rc::clone(&hooks.session);
                        let failure = hooks.failure;
                        let negate = source.polarity == spargebra::SourceExistsPolarity::NotExists;
                        return Ok(CompiledExpression {
                            legacy: None,
                            raw: None,
                            numeric: Some(Rc::new(move |tuple| {
                                nullable::capture_sparql((|| {
                                    let present = exists(tuple)?;
                                    session
                                        .construct_source_boolean_result(if negate {
                                            !present
                                        } else {
                                            present
                                        })
                                        .map(SolutionValue::Rdf)
                                        .map_err(|error| EvalFailure::Fatal(failure(error)))
                                })())
                            })),
                            raw_numeric_result: false,
                        });
                    }
                }
            }
            let _ = source;
            return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
        }
        Expression::SourceSyntax(_) => {
            return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
        }
        Expression::Exists(plan) => {
            // The legacy bool interface cannot carry a source/numeric fault.
            // Reject before compiling its graph; do not silently enter it.
            if raw_requested || numeric_hooks.is_some() {
                return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
            }
            let exists = context
                .build_exists(plan)
                .map_err(ExpressionEvaluationError::Context)?;
            Rc::new(move |tuple| Some(exists(tuple).into()))
        }
        Expression::Or(children) => {
            let children = children
                .iter()
                .map(|i| build_expression_evaluator(i, context))
                .collect::<Result<Vec<_>, _>>()?;
            Rc::new(move |tuple| {
                let mut error = false;
                for child in &children {
                    match child(tuple).and_then(|e| e.effective_boolean_value()) {
                        Some(true) => return Some(true.into()),
                        Some(false) => (),
                        None => error = true,
                    }
                }
                if error { None } else { Some(false.into()) }
            })
        }
        Expression::And(children) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                return build_numeric_conjunction(children, context, hooks);
            }
            let children = children
                .iter()
                .map(|i| build_expression_evaluator(i, context))
                .collect::<Result<Vec<_>, _>>()?;
            build_legacy_conjunction(children)
        }
        Expression::Equal(a, b) | Expression::SourceNotEqual(a, b) => {
            let source_not_equal = matches!(expression, Expression::SourceNotEqual(_, _));
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if raw_requested
                    || !source_execution_hooks(context, hooks)
                    || !hooks
                        .prepared_literals
                        .as_ref()
                        .is_some_and(|registry| registry.admits_graph_execution(&hooks.session))
                {
                    return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
                }
                let a = build_numeric_arithmetic_operand(a, context, hooks)?;
                let b = build_numeric_arithmetic_operand(b, context, hooks)?;
                let session = Rc::clone(&hooks.session);
                let failure = hooks.failure;
                return Ok(CompiledExpression {
                    legacy: None,
                    raw: None,
                    numeric: Some(Rc::new(move |tuple| {
                        nullable::capture_sparql((|| {
                            let left = a.evaluate(tuple)?;
                            let right = b.evaluate(tuple)?;
                            (if source_not_equal {
                                session.source_relational_not_equal(&left, &right)
                            } else {
                                session.source_relational_equal(&left, &right)
                            })
                            .map(SolutionValue::Rdf)
                            .map_err(|error| numeric_evaluation_failure(error, failure))
                        })())
                    })),
                    raw_numeric_result: false,
                });
            }
            if source_not_equal {
                return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
            }
            let a = build_expression_evaluator(a, context)?;
            let b = build_expression_evaluator(b, context)?;
            Rc::new(move |tuple| equals(&a(tuple)?, &b(tuple)?).map(Into::into))
        }
        Expression::SameTerm(a, b) => {
            match (
                try_build_internal_expression_evaluator(a, context)?,
                try_build_internal_expression_evaluator(b, context)?,
            ) {
                (Some(a), Some(b)) => Rc::new(move |tuple| Some((a(tuple)? == b(tuple)?).into())),
                (Some(a), None) => {
                    let b = build_expression_evaluator(b, context)?;
                    let internalize = context.build_internalize_expression_term();
                    Rc::new(move |tuple| Some((a(tuple)? == internalize(b(tuple)?)?).into()))
                }
                (None, Some(b)) => {
                    let a = build_expression_evaluator(a, context)?;
                    let internalize = context.build_internalize_expression_term();
                    Rc::new(move |tuple| Some((internalize(a(tuple)?)? == b(tuple)?).into()))
                }
                (None, None) => {
                    let a = build_expression_evaluator(a, context)?;
                    let b = build_expression_evaluator(b, context)?;
                    Rc::new(move |tuple| Some((a(tuple)? == b(tuple)?).into()))
                }
            }
        }
        Expression::Greater(a, b) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if raw_requested || !source_execution_hooks(context, hooks) {
                    return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
                }
                let a = build_numeric_arithmetic_operand(a, context, hooks)?;
                let b = build_numeric_arithmetic_operand(b, context, hooks)?;
                let session = Rc::clone(&hooks.session);
                let failure = hooks.failure;
                return Ok(CompiledExpression {
                    legacy: None,
                    raw: None,
                    numeric: Some(Rc::new(move |tuple| {
                        nullable::capture_sparql((|| {
                            let left = a.evaluate(tuple)?;
                            let right = b.evaluate(tuple)?;
                            session
                                .source_relational_greater(&left, &right)
                                .map(SolutionValue::Rdf)
                                .map_err(|error| numeric_evaluation_failure(error, failure))
                        })())
                    })),
                    raw_numeric_result: false,
                });
            }
            let a = build_expression_evaluator(a, context)?;
            let b = build_expression_evaluator(b, context)?;
            Rc::new(move |tuple| {
                Some((partial_cmp(&a(tuple)?, &b(tuple)?)? == Ordering::Greater).into())
            })
        }
        Expression::GreaterOrEqual(a, b) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                return build_source_relational_order(
                    a,
                    b,
                    crate::numeric::SourceRelationalOrder::GreaterOrEqual,
                    context,
                    hooks,
                    raw_requested,
                );
            }
            let a = build_expression_evaluator(a, context)?;
            let b = build_expression_evaluator(b, context)?;
            Rc::new(move |tuple| {
                Some(
                    match partial_cmp(&a(tuple)?, &b(tuple)?)? {
                        Ordering::Greater | Ordering::Equal => true,
                        Ordering::Less => false,
                    }
                    .into(),
                )
            })
        }
        Expression::Less(a, b) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                return build_source_relational_order(
                    a,
                    b,
                    crate::numeric::SourceRelationalOrder::Less,
                    context,
                    hooks,
                    raw_requested,
                );
            }
            let a = build_expression_evaluator(a, context)?;
            let b = build_expression_evaluator(b, context)?;
            Rc::new(move |tuple| {
                Some((partial_cmp(&a(tuple)?, &b(tuple)?)? == Ordering::Less).into())
            })
        }
        Expression::LessOrEqual(a, b) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                return build_source_relational_order(
                    a,
                    b,
                    crate::numeric::SourceRelationalOrder::LessOrEqual,
                    context,
                    hooks,
                    raw_requested,
                );
            }
            let a = build_expression_evaluator(a, context)?;
            let b = build_expression_evaluator(b, context)?;
            Rc::new(move |tuple| {
                Some(
                    match partial_cmp(&a(tuple)?, &b(tuple)?)? {
                        Ordering::Less | Ordering::Equal => true,
                        Ordering::Greater => false,
                    }
                    .into(),
                )
            })
        }
        Expression::Add(a, b) => {
            return build_binary_arithmetic(
                a,
                b,
                context,
                raw_requested,
                numeric_hooks,
                ArithmeticOp::Add,
            );
        }
        Expression::Subtract(a, b) => {
            return build_binary_arithmetic(
                a,
                b,
                context,
                raw_requested,
                numeric_hooks,
                ArithmeticOp::Subtract,
            );
        }
        Expression::Multiply(a, b) => {
            return build_binary_arithmetic(
                a,
                b,
                context,
                raw_requested,
                numeric_hooks,
                ArithmeticOp::Multiply,
            );
        }
        Expression::Divide(a, b) => {
            return build_binary_arithmetic(
                a,
                b,
                context,
                raw_requested,
                numeric_hooks,
                ArithmeticOp::Divide,
            );
        }
        Expression::UnaryPlus(e) => {
            return build_unary_arithmetic(
                e,
                context,
                raw_requested,
                numeric_hooks,
                UnaryArithmeticOp::Plus,
            );
        }
        Expression::UnaryMinus(e) => {
            return build_unary_arithmetic(
                e,
                context,
                raw_requested,
                numeric_hooks,
                UnaryArithmeticOp::Minus,
            );
        }
        Expression::Not(e) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if raw_requested || !source_execution_hooks(context, hooks) {
                    return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
                }
                let e = build_numeric_arithmetic_operand(e, context, hooks)?;
                let session = Rc::clone(&hooks.session);
                let failure = hooks.failure;
                return Ok(CompiledExpression {
                    legacy: None,
                    raw: None,
                    numeric: Some(Rc::new(move |tuple| {
                        nullable::capture_sparql((|| {
                            let value = e.evaluate(tuple)?;
                            let boolean = session
                                .source_effective_boolean_value(&value)
                                .map_err(|error| numeric_evaluation_failure(error, failure))?;
                            session
                                .construct_source_boolean_result(!boolean)
                                .map(SolutionValue::Rdf)
                                .map_err(|error| EvalFailure::Fatal(failure(error)))
                        })())
                    })),
                    raw_numeric_result: false,
                });
            }
            let e = build_expression_evaluator(e, context)?;
            Rc::new(move |tuple| Some((!e(tuple)?.effective_boolean_value()?).into()))
        }
        Expression::Coalesce(l) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if !source_execution_hooks(context, hooks)
                    || !hooks
                        .prepared_literals
                        .as_ref()
                        .is_some_and(|registry| registry.admits_graph_execution(&hooks.session))
                {
                    return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
                }
                let mut children: Vec<RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error>> =
                    Vec::new();
                for child in l {
                    if let Expression::Variable(variable) = child {
                        let lookup = context.build_solution_variable_lookup(variable);
                        let externalize = Rc::clone(&hooks.externalize);
                        // value(..., variables=True) preserves unbound variables;
                        // COALESCE then skips them. A variable-bound error raises.
                        children.push(Rc::new(move |tuple| match lookup(tuple) {
                            None | Some(SolutionValue::ExplicitNone) => {
                                Ok(SolutionValue::ExplicitNone)
                            }
                            Some(SolutionValue::ReturnedError(error)) => {
                                Err(EvalFailure::Sparql(error))
                            }
                            Some(SolutionValue::Rdf(value)) => externalize(value)
                                .map(SolutionValue::Rdf)
                                .map_err(EvalFailure::Fatal),
                        }));
                    } else {
                        children.push(
                            build_compiled_expression(child, context, false, Some(hooks))?
                                .numeric
                                .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?,
                        );
                    }
                }
                let failure = hooks.failure;
                return Ok(CompiledExpression {
                    legacy: None,
                    raw: None,
                    raw_numeric_result: false,
                    numeric: Some(Rc::new(move |tuple| {
                        nullable::capture_sparql((|| {
                            // CompValue.get evaluates the entire list before the
                            // builtin chooses its first usable element.
                            let values = children
                                .iter()
                                .map(|child| child(tuple))
                                .collect::<Result<Vec<_>, _>>()?;
                            for value in values {
                                if matches!(value, SolutionValue::Rdf(_)) {
                                    return Ok(value);
                                }
                            }
                            Err(numeric_evaluation_failure(
                                crate::numeric::source_function::source_function_error(
                                    "COALESCE got no arguments that did not evaluate to an error",
                                ),
                                failure,
                            ))
                        })())
                    })),
                });
            }
            let l = l
                .iter()
                .map(|e| build_expression_evaluator(e, context))
                .collect::<Result<Vec<_>, _>>()?;
            Rc::new(move |tuple| {
                for e in &l {
                    if let Some(result) = e(tuple) {
                        return Some(result);
                    }
                }
                None
            })
        }
        Expression::If(a, b, c) => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if raw_requested || !source_execution_hooks(context, hooks)
                    || !hooks.prepared_literals.as_ref()
                        .is_some_and(|registry| registry.admits_graph_execution(&hooks.session))
                {
                    return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
                }
                let a = build_numeric_arithmetic_operand(a, context, hooks)?;
                let b = build_numeric_arithmetic_operand(b, context, hooks)?;
                let c = build_numeric_arithmetic_operand(c, context, hooks)?;
                let session = Rc::clone(&hooks.session);
                let failure = hooks.failure;
                return Ok(CompiledExpression {
                    legacy: None,
                    raw: None,
                    numeric: Some(Rc::new(move |tuple| {
                        nullable::capture_sparql((|| {
                            let value = a.evaluate(tuple)?;
                            let condition = session.source_effective_boolean_value(&value)
                                .map_err(|error| numeric_evaluation_failure(error, failure))?;
                            // CompValue accesses only the selected branch. A
                            // returned source error is not a host exception.
                            if condition { b.evaluate(tuple) } else { c.evaluate(tuple) }
                        })())
                    })),
                    raw_numeric_result: false,
                });
            }
            let a = build_expression_evaluator(a, context)?;
            let b = build_expression_evaluator(b, context)?;
            let c = build_expression_evaluator(c, context)?;
            Rc::new(move |tuple| {
                if a(tuple)?.effective_boolean_value()? {
                    b(tuple)
                } else {
                    c(tuple)
                }
            })
        }
        Expression::SourceFunction {
            function,
            arguments,
            syntax,
        } => {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if let Some(hooks) = numeric_hooks {
                if source_execution_hooks(context, hooks)
                    && hooks
                        .prepared_literals
                        .as_ref()
                        .is_some_and(|registry| registry.admits_graph_execution(&hooks.session))
                {
                    match function {
                        Function::Custom(iri) => {
                            return if hooks.session.source_functions().is_some() {
                                source_functions::build(iri, arguments, syntax, context, hooks)
                            } else {
                                build_source_function(iri, arguments, context, hooks)
                            };
                        }
                        Function::Str => {
                            return source_functions::build_str(arguments, syntax, context, hooks);
                        }
                        Function::Datatype => {
                            return source_functions::build_datatype(arguments, syntax, context, hooks);
                        }
                        Function::IsIri => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::IsIri, "Builtin_isIRI", arguments, syntax, context, hooks),
                        Function::IsBlank => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::IsBlank, "Builtin_isBLANK", arguments, syntax, context, hooks),
                        Function::IsLiteral => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::IsLiteral, "Builtin_isLITERAL", arguments, syntax, context, hooks),
                        Function::IsNumeric => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::IsNumeric, "Builtin_isNUMERIC", arguments, syntax, context, hooks),
                        Function::Lang => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::Lang, "Builtin_LANG", arguments, syntax, context, hooks),
                        Function::StrLen => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::StrLen, "Builtin_STRLEN", arguments, syntax, context, hooks),
                        Function::UCase => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::Upper, "Builtin_UCASE", arguments, syntax, context, hooks),
                        Function::LCase => return source_functions::build_node_unary(crate::numeric::source_function::SourceNodeUnary::Lower, "Builtin_LCASE", arguments, syntax, context, hooks),

                        _ => (),
                    }
                }
            }
            return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
        }
        Expression::FunctionCall(function, parameters) => match function {
            Function::Str => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::StringLiteral(match e(tuple)?.into() {
                        Term::NamedNode(term) => term.into_string(),
                        Term::BlankNode(_) => return None,
                        Term::Literal(term) => term.destruct().0,
                        #[cfg(feature = "sparql-12")]
                        Term::Triple(_) => return None,
                    }))
                })
            }
            Function::Lang => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::StringLiteral(match e(tuple)? {
                        ExpressionTerm::LangStringLiteral { language, .. } => language,
                        #[cfg(feature = "sparql-12")]
                        ExpressionTerm::DirLangStringLiteral { language, .. } => language,
                        ExpressionTerm::NamedNode(_) | ExpressionTerm::BlankNode(_) => {
                            return None;
                        }
                        #[cfg(feature = "sparql-12")]
                        ExpressionTerm::Triple(_) => return None,
                        _ => String::new(),
                    }))
                })
            }
            Function::LangMatches => {
                let language_tag = build_expression_evaluator(&parameters[0], context)?;
                let language_range = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let ExpressionTerm::StringLiteral(mut language_tag) = language_tag(tuple)?
                    else {
                        return None;
                    };
                    language_tag.make_ascii_lowercase();
                    let ExpressionTerm::StringLiteral(mut language_range) = language_range(tuple)?
                    else {
                        return None;
                    };
                    language_range.make_ascii_lowercase();
                    Some(
                        if &*language_range == "*" {
                            !language_tag.is_empty()
                        } else {
                            !ZipLongest::new(language_range.split('-'), language_tag.split('-'))
                                .any(|parts| match parts {
                                    (Some(range_subtag), Some(language_subtag)) => {
                                        range_subtag != language_subtag
                                    }
                                    (Some(_), None) => true,
                                    (None, _) => false,
                                })
                        }
                        .into(),
                    )
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::LangDir => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::StringLiteral(match e(tuple)? {
                        ExpressionTerm::DirLangStringLiteral { direction, .. } => match direction {
                            BaseDirection::Ltr => "ltr".into(),
                            BaseDirection::Rtl => "rtl".into(),
                        },
                        ExpressionTerm::NamedNode(_) | ExpressionTerm::BlankNode(_) => {
                            return None;
                        }
                        #[cfg(feature = "sparql-12")]
                        ExpressionTerm::Triple(_) => return None,
                        _ => String::new(),
                    }))
                })
            }
            Function::Datatype => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::NamedNode(match e(tuple)? {
                        ExpressionTerm::StringLiteral(_) => xsd::STRING.into(),
                        ExpressionTerm::LangStringLiteral { .. } => rdf::LANG_STRING.into(),
                        #[cfg(feature = "sparql-12")]
                        ExpressionTerm::DirLangStringLiteral { .. } => rdf::DIR_LANG_STRING.into(),
                        ExpressionTerm::BooleanLiteral(_) => xsd::BOOLEAN.into(),
                        ExpressionTerm::IntegerLiteral(_) => xsd::INTEGER.into(),
                        ExpressionTerm::DecimalLiteral(_) => xsd::DECIMAL.into(),
                        ExpressionTerm::FloatLiteral(_) => xsd::FLOAT.into(),
                        ExpressionTerm::DoubleLiteral(_) => xsd::DOUBLE.into(),
                        ExpressionTerm::DateTimeLiteral(_) => xsd::DATE_TIME.into(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::DateLiteral(_) => xsd::DATE.into(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::TimeLiteral(_) => xsd::TIME.into(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearLiteral(_) => xsd::G_YEAR.into(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearMonthLiteral(_) => xsd::G_YEAR_MONTH.into(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthLiteral(_) => xsd::G_MONTH.into(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthDayLiteral(_) => xsd::G_MONTH_DAY.into(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GDayLiteral(_) => xsd::G_DAY.into(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::DurationLiteral(_) => xsd::DURATION.into(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::YearMonthDurationLiteral(_) => {
                            xsd::YEAR_MONTH_DURATION.into()
                        }
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::DayTimeDurationLiteral(_) => xsd::DAY_TIME_DURATION.into(),
                        ExpressionTerm::OtherTypedLiteral { datatype, .. } => datatype,
                        ExpressionTerm::NamedNode(_) | ExpressionTerm::BlankNode(_) => {
                            return None;
                        }
                        #[cfg(feature = "sparql-12")]
                        ExpressionTerm::Triple(_) => return None,
                    }))
                })
            }
            Function::Iri => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                let base_iri = context.base_iri();
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::NamedNode(match e(tuple)? {
                        ExpressionTerm::NamedNode(iri) => iri,
                        ExpressionTerm::StringLiteral(iri) => if let Some(base_iri) = &base_iri {
                            base_iri.resolve(&iri)
                        } else {
                            Iri::parse(iri)
                        }
                        .ok()?
                        .into(),
                        _ => return None,
                    }))
                })
            }
            Function::BNode => match parameters.first() {
                Some(id) => {
                    let id = build_expression_evaluator(id, context)?;
                    Rc::new(move |tuple| {
                        let ExpressionTerm::StringLiteral(id) = id(tuple)? else {
                            return None;
                        };
                        Some(ExpressionTerm::BlankNode(BlankNode::new(id).ok()?))
                    })
                }
                None => Rc::new(|_| Some(ExpressionTerm::BlankNode(BlankNode::default()))),
            },
            Function::Rand => {
                Rc::new(|_| Some(ExpressionTerm::DoubleLiteral(random::<f64>().into())))
            }
            Function::Abs => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| match e(tuple)? {
                    ExpressionTerm::IntegerLiteral(value) => {
                        Some(ExpressionTerm::IntegerLiteral(value.checked_abs()?))
                    }
                    ExpressionTerm::DecimalLiteral(value) => {
                        Some(ExpressionTerm::DecimalLiteral(value.checked_abs()?))
                    }
                    ExpressionTerm::FloatLiteral(value) => {
                        Some(ExpressionTerm::FloatLiteral(value.abs()))
                    }
                    ExpressionTerm::DoubleLiteral(value) => {
                        Some(ExpressionTerm::DoubleLiteral(value.abs()))
                    }
                    _ => None,
                })
            }
            Function::Ceil => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| match e(tuple)? {
                    ExpressionTerm::IntegerLiteral(value) => {
                        Some(ExpressionTerm::IntegerLiteral(value))
                    }
                    ExpressionTerm::DecimalLiteral(value) => {
                        Some(ExpressionTerm::DecimalLiteral(value.checked_ceil()?))
                    }
                    ExpressionTerm::FloatLiteral(value) => {
                        Some(ExpressionTerm::FloatLiteral(value.ceil()))
                    }
                    ExpressionTerm::DoubleLiteral(value) => {
                        Some(ExpressionTerm::DoubleLiteral(value.ceil()))
                    }
                    _ => None,
                })
            }
            Function::Floor => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| match e(tuple)? {
                    ExpressionTerm::IntegerLiteral(value) => {
                        Some(ExpressionTerm::IntegerLiteral(value))
                    }
                    ExpressionTerm::DecimalLiteral(value) => {
                        Some(ExpressionTerm::DecimalLiteral(value.checked_floor()?))
                    }
                    ExpressionTerm::FloatLiteral(value) => {
                        Some(ExpressionTerm::FloatLiteral(value.floor()))
                    }
                    ExpressionTerm::DoubleLiteral(value) => {
                        Some(ExpressionTerm::DoubleLiteral(value.floor()))
                    }
                    _ => None,
                })
            }
            Function::Round => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| match e(tuple)? {
                    ExpressionTerm::IntegerLiteral(value) => {
                        Some(ExpressionTerm::IntegerLiteral(value))
                    }
                    ExpressionTerm::DecimalLiteral(value) => {
                        Some(ExpressionTerm::DecimalLiteral(value.checked_round()?))
                    }
                    ExpressionTerm::FloatLiteral(value) => {
                        Some(ExpressionTerm::FloatLiteral(value.round()))
                    }
                    ExpressionTerm::DoubleLiteral(value) => {
                        Some(ExpressionTerm::DoubleLiteral(value.round()))
                    }
                    _ => None,
                })
            }
            Function::Concat => {
                let l = parameters
                    .iter()
                    .map(|e| build_expression_evaluator(e, context))
                    .collect::<Result<Vec<_>, _>>()?;
                Rc::new(move |tuple| {
                    let mut result = String::default();
                    let mut language = None;
                    for e in &l {
                        let (value, e_language) = to_string_and_language(e(tuple)?)?;
                        if let Some(lang) = &language {
                            if *lang != e_language {
                                language = Some(None)
                            }
                        } else {
                            language = Some(e_language)
                        }
                        result += &value
                    }
                    Some(build_plain_literal(result, language.flatten()))
                })
            }
            Function::SubStr => {
                let source = build_expression_evaluator(&parameters[0], context)?;
                let starting_loc = build_expression_evaluator(&parameters[1], context)?;
                let length = parameters
                    .get(2)
                    .map(|l| build_expression_evaluator(l, context))
                    .transpose()?;
                Rc::new(move |tuple| {
                    let (source, language) = to_string_and_language(source(tuple)?)?;

                    let starting_location: usize =
                        if let ExpressionTerm::IntegerLiteral(v) = starting_loc(tuple)? {
                            usize::try_from(i64::from(v)).ok()?
                        } else {
                            return None;
                        };
                    let length = if let Some(length) = &length {
                        if let ExpressionTerm::IntegerLiteral(v) = length(tuple)? {
                            Some(usize::try_from(i64::from(v)).ok()?)
                        } else {
                            return None;
                        }
                    } else {
                        None
                    };

                    // We want to slice on char indices, not byte indices
                    let mut start_iter = source
                        .char_indices()
                        .skip(starting_location.checked_sub(1)?)
                        .peekable();
                    let result = if let Some((start_position, _)) = start_iter.peek().copied() {
                        if let Some(length) = length {
                            let mut end_iter = start_iter.skip(length).peekable();
                            if let Some((end_position, _)) = end_iter.peek() {
                                &source[start_position..*end_position]
                            } else {
                                &source[start_position..]
                            }
                        } else {
                            &source[start_position..]
                        }
                    } else {
                        ""
                    };
                    Some(build_plain_literal(result.into(), language))
                })
            }
            Function::StrLen => {
                let arg = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    let (string, _) = to_string_and_language(arg(tuple)?)?;
                    Some(ExpressionTerm::IntegerLiteral(
                        i64::try_from(string.chars().count()).ok()?.into(),
                    ))
                })
            }
            Function::Replace => {
                let arg = build_expression_evaluator(&parameters[0], context)?;
                let replacement = build_expression_evaluator(&parameters[2], context)?;
                if let Some(regex) =
                    compile_static_pattern_if_exists(&parameters[1], parameters.get(3))
                {
                    Rc::new(move |tuple| {
                        let (text, language) = to_string_and_language(arg(tuple)?)?;
                        let ExpressionTerm::StringLiteral(replacement) = replacement(tuple)? else {
                            return None;
                        };
                        Some(build_plain_literal(
                            match regex.replace_all(&text, &replacement) {
                                Cow::Owned(replaced) => replaced,
                                Cow::Borrowed(_) => text,
                            },
                            language,
                        ))
                    })
                } else {
                    let pattern = build_expression_evaluator(&parameters[1], context)?;
                    let flags = parameters
                        .get(3)
                        .map(|flags| build_expression_evaluator(flags, context))
                        .transpose()?;
                    Rc::new(move |tuple| {
                        let ExpressionTerm::StringLiteral(pattern) = pattern(tuple)? else {
                            return None;
                        };
                        let options = if let Some(flags) = &flags {
                            let ExpressionTerm::StringLiteral(options) = flags(tuple)? else {
                                return None;
                            };
                            Some(options)
                        } else {
                            None
                        };
                        let regex = compile_pattern(&pattern, options.as_deref())?;
                        let (text, language) = to_string_and_language(arg(tuple)?)?;
                        let ExpressionTerm::StringLiteral(replacement) = replacement(tuple)? else {
                            return None;
                        };
                        Some(build_plain_literal(
                            match regex.replace_all(&text, &replacement) {
                                Cow::Owned(replaced) => replaced,
                                Cow::Borrowed(_) => text,
                            },
                            language,
                        ))
                    })
                }
            }
            Function::UCase => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    let (value, language) = to_string_and_language(e(tuple)?)?;
                    Some(build_plain_literal(value.to_uppercase(), language))
                })
            }
            Function::LCase => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    let (value, language) = to_string_and_language(e(tuple)?)?;
                    Some(build_plain_literal(value.to_lowercase(), language))
                })
            }
            Function::StrStarts => {
                let arg1 = build_expression_evaluator(&parameters[0], context)?;
                let arg2 = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let (arg1, arg2, _) =
                        to_argument_compatible_strings(arg1(tuple)?, arg2(tuple)?)?;
                    Some(arg1.starts_with(arg2.as_str()).into())
                })
            }
            Function::EncodeForUri => {
                let ltrl = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    let (ltlr, _) = to_string_and_language(ltrl(tuple)?)?;
                    let mut result = Vec::with_capacity(ltlr.len());
                    for c in ltlr.bytes() {
                        match c {
                            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                                result.push(c)
                            }
                            _ => {
                                result.push(b'%');
                                let high = c / 16;
                                let low = c % 16;
                                result.push(if high < 10 {
                                    b'0' + high
                                } else {
                                    b'A' + (high - 10)
                                });
                                result.push(if low < 10 {
                                    b'0' + low
                                } else {
                                    b'A' + (low - 10)
                                });
                            }
                        }
                    }
                    Some(ExpressionTerm::StringLiteral(
                        String::from_utf8(result).ok()?,
                    ))
                })
            }
            Function::StrEnds => {
                let arg1 = build_expression_evaluator(&parameters[0], context)?;
                let arg2 = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let (arg1, arg2, _) =
                        to_argument_compatible_strings(arg1(tuple)?, arg2(tuple)?)?;
                    Some(arg1.ends_with(arg2.as_str()).into())
                })
            }
            Function::Contains => {
                let arg1 = build_expression_evaluator(&parameters[0], context)?;
                let arg2 = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let (arg1, arg2, _) =
                        to_argument_compatible_strings(arg1(tuple)?, arg2(tuple)?)?;
                    Some(arg1.contains(arg2.as_str()).into())
                })
            }
            Function::StrBefore => {
                let arg1 = build_expression_evaluator(&parameters[0], context)?;
                let arg2 = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let (arg1, arg2, language) =
                        to_argument_compatible_strings(arg1(tuple)?, arg2(tuple)?)?;
                    Some(if let Some(position) = arg1.find(arg2.as_str()) {
                        build_plain_literal(arg1[..position].into(), language)
                    } else {
                        ExpressionTerm::StringLiteral(String::new())
                    })
                })
            }
            Function::StrAfter => {
                let arg1 = build_expression_evaluator(&parameters[0], context)?;
                let arg2 = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let (arg1, arg2, language) =
                        to_argument_compatible_strings(arg1(tuple)?, arg2(tuple)?)?;
                    Some(if let Some(position) = arg1.find(arg2.as_str()) {
                        build_plain_literal(arg1[position + arg2.len()..].into(), language)
                    } else {
                        ExpressionTerm::StringLiteral(String::new())
                    })
                })
            }
            Function::Year => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::IntegerLiteral(
                        match e(tuple)? {
                            ExpressionTerm::DateTimeLiteral(date_time) => date_time.year(),
                            #[cfg(feature = "sep-0002")]
                            ExpressionTerm::DateLiteral(date) => date.year(),
                            #[cfg(feature = "calendar-ext")]
                            ExpressionTerm::GYearMonthLiteral(year_month) => year_month.year(),
                            #[cfg(feature = "calendar-ext")]
                            ExpressionTerm::GYearLiteral(year) => year.year(),
                            _ => return None,
                        }
                        .into(),
                    ))
                })
            }
            Function::Month => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::IntegerLiteral(
                        match e(tuple)? {
                            ExpressionTerm::DateTimeLiteral(date_time) => date_time.month(),
                            #[cfg(feature = "sep-0002")]
                            ExpressionTerm::DateLiteral(date) => date.month(),
                            #[cfg(feature = "calendar-ext")]
                            ExpressionTerm::GYearMonthLiteral(year_month) => year_month.month(),
                            #[cfg(feature = "calendar-ext")]
                            ExpressionTerm::GMonthDayLiteral(month_day) => month_day.month(),
                            #[cfg(feature = "calendar-ext")]
                            ExpressionTerm::GMonthLiteral(month) => month.month(),
                            _ => return None,
                        }
                        .into(),
                    ))
                })
            }
            Function::Day => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::IntegerLiteral(
                        match e(tuple)? {
                            ExpressionTerm::DateTimeLiteral(date_time) => date_time.day(),
                            #[cfg(feature = "sep-0002")]
                            ExpressionTerm::DateLiteral(date) => date.day(),
                            #[cfg(feature = "calendar-ext")]
                            ExpressionTerm::GMonthDayLiteral(month_day) => month_day.day(),
                            #[cfg(feature = "calendar-ext")]
                            ExpressionTerm::GDayLiteral(day) => day.day(),
                            _ => return None,
                        }
                        .into(),
                    ))
                })
            }
            Function::Hours => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::IntegerLiteral(
                        match e(tuple)? {
                            ExpressionTerm::DateTimeLiteral(date_time) => date_time.hour(),
                            #[cfg(feature = "sep-0002")]
                            ExpressionTerm::TimeLiteral(time) => time.hour(),
                            _ => return None,
                        }
                        .into(),
                    ))
                })
            }
            Function::Minutes => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::IntegerLiteral(
                        match e(tuple)? {
                            ExpressionTerm::DateTimeLiteral(date_time) => date_time.minute(),
                            #[cfg(feature = "sep-0002")]
                            ExpressionTerm::TimeLiteral(time) => time.minute(),
                            _ => return None,
                        }
                        .into(),
                    ))
                })
            }
            Function::Seconds => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTerm::DecimalLiteral(match e(tuple)? {
                        ExpressionTerm::DateTimeLiteral(date_time) => date_time.second(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::TimeLiteral(time) => time.second(),
                        _ => return None,
                    }))
                })
            }
            Function::Timezone => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    let result = match e(tuple)? {
                        ExpressionTerm::DateTimeLiteral(date_time) => date_time.timezone(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::TimeLiteral(time) => time.timezone(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::DateLiteral(date) => date.timezone(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearMonthLiteral(year_month) => year_month.timezone(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearLiteral(year) => year.timezone(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthDayLiteral(month_day) => month_day.timezone(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GDayLiteral(day) => day.timezone(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthLiteral(month) => month.timezone(),
                        _ => None,
                    }?;
                    #[cfg(feature = "sep-0002")]
                    {
                        Some(ExpressionTerm::DayTimeDurationLiteral(result))
                    }
                    #[cfg(not(feature = "sep-0002"))]
                    {
                        Some(ExpressionTerm::OtherTypedLiteral {
                            value: result.to_string(),
                            datatype: xsd::DAY_TIME_DURATION.into(),
                        })
                    }
                })
            }
            Function::Tz => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    let timezone_offset = match e(tuple)? {
                        ExpressionTerm::DateTimeLiteral(date_time) => date_time.timezone_offset(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::TimeLiteral(time) => time.timezone_offset(),
                        #[cfg(feature = "sep-0002")]
                        ExpressionTerm::DateLiteral(date) => date.timezone_offset(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearMonthLiteral(year_month) => {
                            year_month.timezone_offset()
                        }
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearLiteral(year) => year.timezone_offset(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthDayLiteral(month_day) => month_day.timezone_offset(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GDayLiteral(day) => day.timezone_offset(),
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthLiteral(month) => month.timezone_offset(),
                        _ => return None,
                    };
                    Some(ExpressionTerm::StringLiteral(
                        timezone_offset.map_or_else(String::new, |o| o.to_string()),
                    ))
                })
            }
            #[cfg(feature = "sep-0002")]
            Function::Adjust => {
                let dt = build_expression_evaluator(&parameters[0], context)?;
                let tz = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let timezone_offset = Some(
                        match tz(tuple)? {
                            ExpressionTerm::DayTimeDurationLiteral(tz) => {
                                TimezoneOffset::try_from(tz)
                            }
                            ExpressionTerm::DurationLiteral(tz) => TimezoneOffset::try_from(tz),
                            _ => return None,
                        }
                        .ok()?,
                    );
                    Some(match dt(tuple)? {
                        ExpressionTerm::DateTimeLiteral(date_time) => {
                            ExpressionTerm::DateTimeLiteral(date_time.adjust(timezone_offset)?)
                        }
                        ExpressionTerm::TimeLiteral(time) => {
                            ExpressionTerm::TimeLiteral(time.adjust(timezone_offset)?)
                        }
                        ExpressionTerm::DateLiteral(date) => {
                            ExpressionTerm::DateLiteral(date.adjust(timezone_offset)?)
                        }
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearMonthLiteral(year_month) => {
                            ExpressionTerm::GYearMonthLiteral(year_month.adjust(timezone_offset)?)
                        }
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GYearLiteral(year) => {
                            ExpressionTerm::GYearLiteral(year.adjust(timezone_offset)?)
                        }
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthDayLiteral(month_day) => {
                            ExpressionTerm::GMonthDayLiteral(month_day.adjust(timezone_offset)?)
                        }
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GDayLiteral(day) => {
                            ExpressionTerm::GDayLiteral(day.adjust(timezone_offset)?)
                        }
                        #[cfg(feature = "calendar-ext")]
                        ExpressionTerm::GMonthLiteral(month) => {
                            ExpressionTerm::GMonthLiteral(month.adjust(timezone_offset)?)
                        }
                        _ => return None,
                    })
                })
            }
            Function::Now => {
                let now = context.now();
                Rc::new(move |_| Some(ExpressionTerm::DateTimeLiteral(now)))
            }
            Function::Uuid => Rc::new(move |_| {
                let mut buffer = String::with_capacity(44);
                buffer.push_str("urn:uuid:");
                generate_uuid(&mut buffer);
                Some(ExpressionTerm::NamedNode(NamedNode::new_unchecked(buffer)))
            }),
            Function::StrUuid => Rc::new(move |_| {
                let mut buffer = String::with_capacity(36);
                generate_uuid(&mut buffer);
                Some(ExpressionTerm::StringLiteral(buffer))
            }),
            Function::Md5 => build_hash_expression_evaluator::<_, Md5>(parameters, context)?,
            Function::Sha1 => build_hash_expression_evaluator::<_, Sha1>(parameters, context)?,
            Function::Sha256 => build_hash_expression_evaluator::<_, Sha256>(parameters, context)?,
            Function::Sha384 => build_hash_expression_evaluator::<_, Sha384>(parameters, context)?,
            Function::Sha512 => build_hash_expression_evaluator::<_, Sha512>(parameters, context)?,
            Function::StrLang => {
                let lexical_form = build_expression_evaluator(&parameters[0], context)?;
                let lang_tag = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let ExpressionTerm::StringLiteral(value) = lexical_form(tuple)? else {
                        return None;
                    };
                    let ExpressionTerm::StringLiteral(language) = lang_tag(tuple)? else {
                        return None;
                    };
                    Some(
                        Term::from(Literal::new_language_tagged_literal(value, language).ok()?)
                            .into(),
                    )
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::StrLangDir => {
                let lexical_form = build_expression_evaluator(&parameters[0], context)?;
                let lang_tag = build_expression_evaluator(&parameters[1], context)?;
                let direction = build_expression_evaluator(&parameters[2], context)?;
                Rc::new(move |tuple| {
                    let ExpressionTerm::StringLiteral(value) = lexical_form(tuple)? else {
                        return None;
                    };
                    let ExpressionTerm::StringLiteral(language) = lang_tag(tuple)? else {
                        return None;
                    };
                    let ExpressionTerm::StringLiteral(direction) = direction(tuple)? else {
                        return None;
                    };
                    let direction = match direction.as_str() {
                        "ltr" => BaseDirection::Ltr,
                        "rtl" => BaseDirection::Rtl,
                        _ => return None,
                    };
                    Some(
                        Term::from(
                            Literal::new_directional_language_tagged_literal(
                                value, language, direction,
                            )
                            .ok()?,
                        )
                        .into(),
                    )
                })
            }
            Function::StrDt => {
                let lexical_form = build_expression_evaluator(&parameters[0], context)?;
                let datatype = build_expression_evaluator(&parameters[1], context)?;
                Rc::new(move |tuple| {
                    let ExpressionTerm::StringLiteral(value) = lexical_form(tuple)? else {
                        return None;
                    };
                    let ExpressionTerm::NamedNode(datatype) = datatype(tuple)? else {
                        return None;
                    };
                    Some(Term::from(Literal::new_typed_literal(value, datatype)).into())
                })
            }

            Function::IsIri => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| Some(matches!(e(tuple)?, ExpressionTerm::NamedNode(_)).into()))
            }
            Function::IsBlank => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| Some(matches!(e(tuple)?, ExpressionTerm::BlankNode(_)).into()))
            }
            Function::IsLiteral => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(
                        match e(tuple)? {
                            ExpressionTerm::NamedNode(_) | ExpressionTerm::BlankNode(_) => false,
                            #[cfg(feature = "sparql-12")]
                            ExpressionTerm::Triple(_) => false,
                            _ => true,
                        }
                        .into(),
                    )
                })
            }
            Function::IsNumeric => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(
                        matches!(
                            e(tuple)?,
                            ExpressionTerm::IntegerLiteral(_)
                                | ExpressionTerm::DecimalLiteral(_)
                                | ExpressionTerm::FloatLiteral(_)
                                | ExpressionTerm::DoubleLiteral(_)
                        )
                        .into(),
                    )
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::HasLang => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(
                        matches!(
                            e(tuple)?,
                            ExpressionTerm::LangStringLiteral { .. }
                                | ExpressionTerm::DirLangStringLiteral { .. }
                        )
                        .into(),
                    )
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::HasLangDir => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    Some(matches!(e(tuple)?, ExpressionTerm::DirLangStringLiteral { .. }).into())
                })
            }
            Function::Regex => {
                let text = build_expression_evaluator(&parameters[0], context)?;
                if let Some(regex) =
                    compile_static_pattern_if_exists(&parameters[1], parameters.get(2))
                {
                    Rc::new(move |tuple| {
                        let (text, _) = to_string_and_language(text(tuple)?)?;
                        Some(regex.is_match(&text).into())
                    })
                } else {
                    let pattern = build_expression_evaluator(&parameters[1], context)?;
                    let flags = parameters
                        .get(2)
                        .map(|flags| build_expression_evaluator(flags, context))
                        .transpose()?;
                    Rc::new(move |tuple| {
                        let ExpressionTerm::StringLiteral(pattern) = pattern(tuple)? else {
                            return None;
                        };
                        let options = if let Some(flags) = &flags {
                            let ExpressionTerm::StringLiteral(options) = flags(tuple)? else {
                                return None;
                            };
                            Some(options)
                        } else {
                            None
                        };
                        let regex = compile_pattern(&pattern, options.as_deref())?;
                        let (text, _) = to_string_and_language(text(tuple)?)?;
                        Some(regex.is_match(&text).into())
                    })
                }
            }
            #[cfg(feature = "sparql-12")]
            Function::Triple => {
                let s = build_expression_evaluator(&parameters[0], context)?;
                let p = build_expression_evaluator(&parameters[1], context)?;
                let o = build_expression_evaluator(&parameters[2], context)?;
                Rc::new(move |tuple| {
                    Some(ExpressionTriple::new(s(tuple)?, p(tuple)?, o(tuple)?)?.into())
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::Subject => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    if let ExpressionTerm::Triple(t) = e(tuple)? {
                        Some(t.subject.into())
                    } else {
                        None
                    }
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::Predicate => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    if let ExpressionTerm::Triple(t) = e(tuple)? {
                        Some(t.predicate.into())
                    } else {
                        None
                    }
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::Object => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| {
                    if let ExpressionTerm::Triple(t) = e(tuple)? {
                        Some(t.object)
                    } else {
                        None
                    }
                })
            }
            #[cfg(feature = "sparql-12")]
            Function::IsTriple => {
                let e = build_expression_evaluator(&parameters[0], context)?;
                Rc::new(move |tuple| Some(matches!(e(tuple)?, ExpressionTerm::Triple(_)).into()))
            }
            Function::Custom(function_name) => {
                #[cfg(feature = "rdflib760-nullable-internal")]
                if let Some(hooks) = numeric_hooks {
                    if source_execution_hooks(context, hooks)
                        && hooks
                            .prepared_literals
                            .as_ref()
                            .is_some_and(|registry| registry.admits_graph_execution(&hooks.session))
                    {
                        return build_source_function(function_name, parameters, context, hooks);
                    }
                }
                if raw_requested || numeric_hooks.is_some() {
                    return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
                }
                if let Some(function) = context.custom_functions().get(function_name).cloned() {
                    let args = parameters
                        .iter()
                        .map(|e| build_expression_evaluator(e, context))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(CompiledExpression {
                        legacy: Some(Rc::new(move |tuple| {
                            let args = args
                                .iter()
                                .map(|f| Some(f(tuple)?.into()))
                                .collect::<Option<Vec<_>>>()?;
                            Some(function(&args)?.into())
                        })),
                        raw: None,
                        #[cfg(feature = "rdflib760-nullable-internal")]
                        numeric: None,
                        raw_numeric_result: false,
                    });
                }

                macro_rules! cast_fn {
                    ($eval:expr) => {{
                        if parameters.len() != 1 {
                            return Err(
                                ExpressionEvaluationError::UnsupportedCustomFunctionArity {
                                    name: function_name.clone(),
                                    expected: 1..=1,
                                    actual: parameters.len(),
                                },
                            );
                        }
                        let e = build_expression_evaluator(&parameters[0], context)?;
                        Rc::new(move |tuple| ($eval)(e(tuple)?))
                    }};
                }

                match function_name.as_ref() {
                    xsd::STRING => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::StringLiteral(
                            match t.into() {
                                Term::NamedNode(term) => term.into_string(),
                                Term::BlankNode(_) => return None,
                                Term::Literal(term) => term.destruct().0,
                                #[cfg(feature = "sparql-12")]
                                Term::Triple(_) => return None,
                            }
                        )))
                    }
                    xsd::BOOLEAN => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::BooleanLiteral(
                            match t {
                                ExpressionTerm::BooleanLiteral(value) => value,
                                ExpressionTerm::FloatLiteral(value) => value.into(),
                                ExpressionTerm::DoubleLiteral(value) => value.into(),
                                ExpressionTerm::IntegerLiteral(value) => value.into(),
                                ExpressionTerm::DecimalLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }
                        )))
                    }
                    xsd::DOUBLE => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::DoubleLiteral(match t {
                                ExpressionTerm::FloatLiteral(value) => value.into(),
                                ExpressionTerm::DoubleLiteral(value) => value,
                                ExpressionTerm::IntegerLiteral(value) => value.into(),
                                ExpressionTerm::DecimalLiteral(value) => value.into(),
                                ExpressionTerm::BooleanLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }))
                        )
                    }
                    xsd::FLOAT => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::FloatLiteral(match t {
                                ExpressionTerm::FloatLiteral(value) => value,
                                ExpressionTerm::DoubleLiteral(value) => value.into(),
                                ExpressionTerm::IntegerLiteral(value) => value.into(),
                                ExpressionTerm::DecimalLiteral(value) => value.into(),
                                ExpressionTerm::BooleanLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }))
                        )
                    }
                    xsd::INTEGER => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::IntegerLiteral(
                            match t {
                                ExpressionTerm::FloatLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::DoubleLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::IntegerLiteral(value) => value,
                                ExpressionTerm::DecimalLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::BooleanLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }
                        )))
                    }
                    xsd::DECIMAL => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::DecimalLiteral(
                            match t {
                                ExpressionTerm::FloatLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::DoubleLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::IntegerLiteral(value) => value.into(),
                                ExpressionTerm::DecimalLiteral(value) => value,
                                ExpressionTerm::BooleanLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }
                        )))
                    }
                    #[cfg(feature = "sep-0002")]
                    xsd::DATE => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::DateLiteral(match t {
                                ExpressionTerm::DateLiteral(value) => value,
                                ExpressionTerm::DateTimeLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }))
                        )
                    }
                    #[cfg(feature = "sep-0002")]
                    xsd::TIME => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::TimeLiteral(match t {
                                ExpressionTerm::TimeLiteral(value) => value,
                                ExpressionTerm::DateTimeLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }))
                        )
                    }
                    xsd::DATE_TIME => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::DateTimeLiteral(
                            match t {
                                ExpressionTerm::DateTimeLiteral(value) => value,
                                #[cfg(feature = "sep-0002")]
                                ExpressionTerm::DateLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }
                        )))
                    }
                    #[cfg(feature = "sep-0002")]
                    xsd::DURATION => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::DurationLiteral(
                            match t {
                                ExpressionTerm::DurationLiteral(value) => value,
                                ExpressionTerm::YearMonthDurationLiteral(value) => value.into(),
                                ExpressionTerm::DayTimeDurationLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }
                        )))
                    }
                    #[cfg(feature = "sep-0002")]
                    xsd::YEAR_MONTH_DURATION => {
                        cast_fn!(|t: ExpressionTerm| Some(
                            ExpressionTerm::YearMonthDurationLiteral(match t {
                                ExpressionTerm::DurationLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::YearMonthDurationLiteral(value) => value,
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            })
                        ))
                    }
                    #[cfg(feature = "sep-0002")]
                    xsd::DAY_TIME_DURATION => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::DayTimeDurationLiteral(
                                match t {
                                    ExpressionTerm::DurationLiteral(value) =>
                                        value.try_into().ok()?,
                                    ExpressionTerm::DayTimeDurationLiteral(value) => value,
                                    ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                    _ => return None,
                                }
                            ))
                        )
                    }
                    #[cfg(feature = "calendar-ext")]
                    xsd::G_YEAR => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::GYearLiteral(match t {
                                ExpressionTerm::GYearLiteral(value) => value,
                                ExpressionTerm::GYearMonthLiteral(value) => {
                                    value.try_into().ok()?
                                }
                                ExpressionTerm::DateLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::DateTimeLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }))
                        )
                    }
                    #[cfg(feature = "calendar-ext")]
                    xsd::G_YEAR_MONTH => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::GYearMonthLiteral(
                            match t {
                                ExpressionTerm::GYearMonthLiteral(value) => value,
                                ExpressionTerm::DateLiteral(value) => value.into(),
                                ExpressionTerm::DateTimeLiteral(value) => value.try_into().ok()?,
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }
                        )))
                    }
                    #[cfg(feature = "calendar-ext")]
                    xsd::G_MONTH => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::GMonthLiteral(match t {
                                ExpressionTerm::GMonthLiteral(value) => value,
                                ExpressionTerm::GYearMonthLiteral(value) => value.into(),
                                ExpressionTerm::GMonthDayLiteral(value) => value.into(),
                                ExpressionTerm::DateLiteral(value) => value.into(),
                                ExpressionTerm::DateTimeLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }))
                        )
                    }
                    #[cfg(feature = "calendar-ext")]
                    xsd::G_MONTH_DAY => {
                        cast_fn!(|t: ExpressionTerm| Some(ExpressionTerm::GMonthDayLiteral(
                            match t {
                                ExpressionTerm::GMonthDayLiteral(value) => value,
                                ExpressionTerm::DateLiteral(value) => value.into(),
                                ExpressionTerm::DateTimeLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }
                        )))
                    }
                    #[cfg(feature = "calendar-ext")]
                    xsd::G_DAY => {
                        cast_fn!(
                            |t: ExpressionTerm| Some(ExpressionTerm::GDayLiteral(match t {
                                ExpressionTerm::GDayLiteral(value) => value,
                                ExpressionTerm::GMonthDayLiteral(value) => value.into(),
                                ExpressionTerm::DateLiteral(value) => value.into(),
                                ExpressionTerm::DateTimeLiteral(value) => value.into(),
                                ExpressionTerm::StringLiteral(value) => value.parse().ok()?,
                                _ => return None,
                            }))
                        )
                    }
                    _ => {
                        return Err(ExpressionEvaluationError::UnsupportedCustomFunction(
                            function_name.clone(),
                        ));
                    }
                }
            }
        },
        Expression::AdditiveChain { first, tail } => {
            return build_chain_arithmetic(
                first,
                tail.iter().map(|(op, operand)| {
                    (
                        match op {
                            AdditiveOperator::Add => ArithmeticOp::Add,
                            AdditiveOperator::Subtract => ArithmeticOp::Subtract,
                        },
                        operand,
                    )
                }),
                context,
                raw_requested,
                numeric_hooks,
                ArithmeticChainKind::Additive,
            );
        }
        Expression::MultiplicativeChain { first, tail } => {
            return build_chain_arithmetic(
                first,
                tail.iter().map(|(op, operand)| {
                    (
                        match op {
                            MultiplicativeOperator::Multiply => ArithmeticOp::Multiply,
                            MultiplicativeOperator::Divide => ArithmeticOp::Divide,
                        },
                        operand,
                    )
                }),
                context,
                raw_requested,
                numeric_hooks,
                ArithmeticChainKind::Multiplicative,
            );
        }
    };
    if raw_requested && raw.is_none() {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    #[cfg(feature = "rdflib760-nullable-internal")]
    if numeric_hooks.is_some() && numeric.is_none() {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    Ok(CompiledExpression {
        legacy: Some(legacy),
        raw,
        #[cfg(feature = "rdflib760-nullable-internal")]
        numeric,
        raw_numeric_result: false,
    })
}

fn build_legacy_conjunction<'a, I: 'a>(
    children: Vec<ExpressionEvaluator<'a, I, ExpressionTerm>>,
) -> ExpressionEvaluator<'a, I, ExpressionTerm> {
    Rc::new(move |tuple| {
        let mut error = false;
        for child in &children {
            match child(tuple).and_then(|e| e.effective_boolean_value()) {
                Some(true) => (),
                Some(false) => return Some(false.into()),
                None => error = true,
            }
        }
        if error { None } else { Some(true.into()) }
    })
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn build_numeric_conjunction<'a, C: ExpressionEvaluatorContext<'a>>(
    children: &[Expression],
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    // The current source AST transport preserves only binary And. A nested
    // node cannot distinguish a flat three-item source list from parentheses;
    // both operand evaluation and Expr catch boundaries depend on that fact.
    if children.len() != 2 || children.iter().any(|e| matches!(e, Expression::And(_))) {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    let mut numeric_children = Vec::with_capacity(children.len());
    let mut legacy_children = Some(Vec::with_capacity(children.len()));
    for expression in children {
        let compiled = build_compiled_expression(expression, context, false, Some(hooks))?;
        numeric_children.push(NumericArithmeticOperand {
            evaluator: if let Expression::Variable(variable) = expression {
                build_numeric_variable_evaluator(variable, context, hooks, true)
            } else {
                compiled
                    .numeric
                    .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?
            },
            is_variable: matches!(expression, Expression::Variable(_)),
            source_literal: hooks.session.literal_semantics()
                == QueryLiteralSemantics::SourceLiteralV1,
        });
        match (legacy_children.as_mut(), compiled.legacy) {
            (Some(children), Some(child)) => children.push(child),
            _ => legacy_children = None,
        }
    }
    let session = Rc::clone(&hooks.session);
    let failure = hooks.failure;
    let evaluator: RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error> =
        Rc::new(move |tuple| {
            evaluate_numeric_conjunction(&numeric_children, tuple, &session, failure)
        });
    Ok(CompiledExpression {
        legacy: legacy_children.map(build_legacy_conjunction),
        raw: None,
        numeric: Some(evaluator),
        raw_numeric_result: false,
    })
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn evaluate_numeric_conjunction<I, F>(
    children: &[NumericArithmeticOperand<'_, I, F>],
    tuple: &I,
    session: &QueryNumericSession,
    failure: fn(RawNumericFault) -> F,
) -> Result<SolutionValue<RawExpressionValue>, EvalFailure<F>> {
    nullable::capture_sparql((|| {
        let values = children
            .iter()
            .map(|child| child.evaluate(tuple))
            .collect::<Result<Vec<_>, _>>()?;
        session
            .evaluate_conjunction(values)
            .map(SolutionValue::Rdf)
            .map_err(|error| numeric_evaluation_failure(error, failure))
    })())
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod numeric_conjunction_tests {
    use super::*;
    use crate::tuple::SparqlErrorKind;
    use semantica_numeric::{DecimalContext, NumericLimits};
    use std::cell::RefCell;

    type Operand = NumericArithmeticOperand<'static, (), &'static str>;

    fn session() -> QueryNumericSession {
        QueryNumericSession::new(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
        )
        .unwrap()
    }

    fn boolean(value: bool) -> SolutionValue<RawExpressionValue> {
        SolutionValue::Rdf(RawExpressionValue::from_import(Literal::from(value).into()))
    }

    fn operand(
        value: SolutionValue<RawExpressionValue>,
        is_variable: bool,
        label: &'static str,
        trace: &Rc<RefCell<Vec<&'static str>>>,
    ) -> Operand {
        let trace = Rc::clone(trace);
        Operand {
            evaluator: Rc::new(move |_| {
                trace.borrow_mut().push(label);
                Ok(value.clone())
            }),
            is_variable,
            source_literal: false,
        }
    }

    #[test]
    fn numeric_and_evaluates_all_raw_operands_before_ebv_and_preserves_fatal() {
        let s = session();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let first = operand(boolean(false), false, "first", &trace);
        let tail_trace = Rc::clone(&trace);
        let fatal = Operand {
            evaluator: Rc::new(move |_| {
                tail_trace.borrow_mut().push("fatal tail");
                Err(EvalFailure::Fatal("dataset failure"))
            }),
            is_variable: false,
            source_literal: false,
        };
        let result = evaluate_numeric_conjunction(&[first, fatal], &(), &s, |_| "core failure");
        assert!(matches!(result, Err(EvalFailure::Fatal("dataset failure"))));
        assert_eq!(&*trace.borrow(), &["first", "fatal tail"]);

        // A successfully evaluated unknown numeric value is different from a
        // raised failure: EBV(false) stops before consulting that value's cache.
        trace.borrow_mut().clear();
        let unknown =
            RawExpressionValue::from_import(Literal::new_typed_literal("100", xsd::DECIMAL).into());
        let values = [
            operand(boolean(false), false, "first", &trace),
            operand(SolutionValue::Rdf(unknown), false, "unknown tail", &trace),
        ];
        assert_eq!(
            evaluate_numeric_conjunction(&values, &(), &s, |_| "core failure").unwrap(),
            boolean(false)
        );
        assert_eq!(&*trace.borrow(), &["first", "unknown tail"]);
    }

    #[test]
    fn numeric_and_ebv_errors_have_fresh_identity_and_do_not_skip_raw_tail() {
        let s = session();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let original = ErrorValue::with_kind(SparqlErrorKind::OtherSparql, "source divide by zero");
        let values = [
            operand(
                SolutionValue::ReturnedError(original.clone()),
                false,
                "error expr",
                &trace,
            ),
            operand(boolean(false), false, "false tail", &trace),
        ];
        let SolutionValue::ReturnedError(a) =
            evaluate_numeric_conjunction(&values, &(), &s, |_| "core failure").unwrap()
        else {
            panic!("first EBV error must be returned by the enclosing Expr");
        };
        let SolutionValue::ReturnedError(b) =
            evaluate_numeric_conjunction(&values, &(), &s, |_| "core failure").unwrap()
        else {
            panic!("each evaluation must construct its own EBV error");
        };
        assert_eq!(a.kind(), SparqlErrorKind::Type);
        assert_ne!(a, original);
        assert_ne!(a, b);
        assert_eq!(
            &*trace.borrow(),
            &["error expr", "false tail", "error expr", "false tail"]
        );
        let reversed = [
            operand(boolean(false), false, "false first", &trace),
            operand(
                SolutionValue::ReturnedError(original),
                false,
                "error tail",
                &trace,
            ),
        ];
        assert_eq!(
            evaluate_numeric_conjunction(&reversed, &(), &s, |_| "core failure").unwrap(),
            boolean(false)
        );
    }

    #[test]
    fn numeric_and_variable_error_stops_before_tail_and_retains_identity() {
        let s = session();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let original = ErrorValue::with_kind(SparqlErrorKind::OtherSparql, "stored error");
        let values = [
            operand(
                SolutionValue::ReturnedError(original.clone()),
                true,
                "variable",
                &trace,
            ),
            operand(boolean(false), false, "unreached", &trace),
        ];
        let SolutionValue::ReturnedError(error) =
            evaluate_numeric_conjunction(&values, &(), &s, |_| "core failure").unwrap()
        else {
            panic!("variable operand must raise its stored SPARQL error");
        };
        assert_eq!(error, original);
        assert_eq!(&*trace.borrow(), &["variable"]);
    }

    #[test]
    fn legacy_and_still_short_circuits_before_later_raw_evaluation() {
        let calls = Rc::new(RefCell::new(0));
        let observed = Rc::clone(&calls);
        let first: ExpressionEvaluator<'_, (), ExpressionTerm> = Rc::new(|_| Some(false.into()));
        let second: ExpressionEvaluator<'_, (), ExpressionTerm> = Rc::new(move |_| {
            *observed.borrow_mut() += 1;
            None
        });
        assert!(build_legacy_conjunction(vec![first, second])(&()) == Some(false.into()));
        assert_eq!(*calls.borrow(), 0);
    }
}

fn build_binary_arithmetic<'a, C: ExpressionEvaluatorContext<'a>>(
    a: &Expression,
    b: &Expression,
    context: &mut C,
    raw_requested: bool,
    numeric_hooks: Option<&NumericExpressionHooks<'a, C::Term, C::Error>>,
    op: ArithmeticOp,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    let a_variable = if let Expression::Variable(v) = a {
        Some(v.clone())
    } else {
        None
    };
    let b_variable = if let Expression::Variable(v) = b {
        Some(v.clone())
    } else {
        None
    };
    let a_is_variable = a_variable.is_some();
    let b_is_variable = b_variable.is_some();
    let a = build_compiled_expression(a, context, raw_requested, numeric_hooks)?;
    let b = build_compiled_expression(b, context, raw_requested, numeric_hooks)?;
    let raw = if raw_requested {
        let a_numeric_result = a.raw_numeric_result;
        let b_numeric_result = b.raw_numeric_result;
        let a = a
            .raw
            .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
        let b = b
            .raw
            .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
        let unsupported: fn(&'static str) -> C::Error = C::unsupported_nullable_evaluation;
        let evaluator: RawEvaluator<'a, C::Tuple, ExpressionTerm, C::Error> =
            Rc::new(move |tuple| {
                nullable::capture_sparql((|| {
                    let a = a(tuple)?;
                    let a = if a_is_variable {
                        nullable::variable_operand(a)?
                    } else {
                        a
                    };
                    let b = b(tuple)?;
                    let b = if b_is_variable {
                        nullable::variable_operand(b)?
                    } else {
                        b
                    };
                    nullable::typed_binary(
                        op,
                        ArithmeticOperand::new(a, a_numeric_result),
                        ArithmeticOperand::new(b, b_numeric_result),
                        unsupported,
                    )
                })())
            });
        Some(evaluator)
    } else {
        None
    };
    #[cfg(feature = "rdflib760-nullable-internal")]
    let numeric = if let Some(hooks) = numeric_hooks {
        let first = NumericArithmeticOperand {
            evaluator: if let Some(variable) = &a_variable {
                build_numeric_variable_evaluator(variable, context, hooks, true)
            } else {
                a.numeric
                    .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?
            },
            is_variable: a_is_variable,
            source_literal: hooks.session.literal_semantics()
                == QueryLiteralSemantics::SourceLiteralV1,
        };
        let second = NumericArithmeticOperand {
            evaluator: if let Some(variable) = &b_variable {
                build_numeric_variable_evaluator(variable, context, hooks, true)
            } else {
                b.numeric
                    .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?
            },
            is_variable: b_is_variable,
            source_literal: hooks.session.literal_semantics()
                == QueryLiteralSemantics::SourceLiteralV1,
        };
        let session = Rc::clone(&hooks.session);
        let failure = hooks.failure;
        let op = numeric_binary_op(op);
        let kind = match op {
            NumericBinaryOp::Add | NumericBinaryOp::Subtract => NumericChainKind::Additive,
            NumericBinaryOp::Multiply | NumericBinaryOp::Divide => NumericChainKind::Multiplicative,
        };
        let evaluator: RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error> =
            Rc::new(move |tuple| {
                nullable::capture_sparql((|| {
                    let first = first.evaluate(tuple)?;
                    let second = second.evaluate(tuple)?;
                    session
                        .evaluate_chain_value(kind, first, vec![(op, second)])
                        .map_err(|error| numeric_evaluation_failure(error, failure))
                })())
            });
        Some(evaluator)
    } else {
        None
    };
    let legacy = a.legacy.zip(b.legacy).map(|(a, b)| {
        let evaluator: ExpressionEvaluator<'a, C::Tuple, ExpressionTerm> =
            Rc::new(move |tuple| op.evaluate(a(tuple)?, b(tuple)?).ok());
        evaluator
    });
    Ok(CompiledExpression {
        legacy,
        raw,
        #[cfg(feature = "rdflib760-nullable-internal")]
        numeric,
        raw_numeric_result: true,
    })
}

fn build_unary_arithmetic<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
    raw_requested: bool,
    numeric_hooks: Option<&NumericExpressionHooks<'a, C::Term, C::Error>>,
    op: UnaryArithmeticOp,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    let is_variable = matches!(expression, Expression::Variable(_));
    let compiled = build_compiled_expression(expression, context, raw_requested, numeric_hooks)?;
    let raw = if raw_requested {
        let numeric_result = compiled.raw_numeric_result;
        let raw = compiled
            .raw
            .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
        let unsupported: fn(&'static str) -> C::Error = C::unsupported_nullable_evaluation;
        let evaluator: RawEvaluator<'a, C::Tuple, ExpressionTerm, C::Error> =
            Rc::new(move |tuple| {
                nullable::capture_sparql((|| {
                    let value = raw(tuple)?;
                    let value = if is_variable {
                        nullable::variable_operand(value)?
                    } else {
                        value
                    };
                    nullable::typed_unary(
                        op,
                        ArithmeticOperand::new(value, numeric_result),
                        unsupported,
                    )
                })())
            });
        Some(evaluator)
    } else {
        None
    };
    #[cfg(feature = "rdflib760-nullable-internal")]
    let numeric = if let Some(hooks) = numeric_hooks {
        let operand = NumericArithmeticOperand {
            evaluator: if let Expression::Variable(variable) = expression {
                build_numeric_variable_evaluator(variable, context, hooks, true)
            } else {
                compiled
                    .numeric
                    .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?
            },
            is_variable,
            source_literal: hooks.session.literal_semantics()
                == QueryLiteralSemantics::SourceLiteralV1,
        };
        let session = Rc::clone(&hooks.session);
        let failure = hooks.failure;
        let op = match op {
            UnaryArithmeticOp::Plus => NumericUnaryOp::Plus,
            UnaryArithmeticOp::Minus => NumericUnaryOp::Minus,
        };
        let evaluator: RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error> =
            Rc::new(move |tuple| {
                nullable::capture_sparql((|| {
                    let value = operand.evaluate(tuple)?;
                    session
                        .evaluate_unary(op, value)
                        .map(SolutionValue::Rdf)
                        .map_err(|error| numeric_evaluation_failure(error, failure))
                })())
            });
        Some(evaluator)
    } else {
        None
    };
    let legacy = compiled.legacy.map(|legacy| {
        let evaluator: ExpressionEvaluator<'a, C::Tuple, ExpressionTerm> =
            Rc::new(move |tuple| op.evaluate(legacy(tuple)?).ok());
        evaluator
    });
    Ok(CompiledExpression {
        legacy,
        raw,
        #[cfg(feature = "rdflib760-nullable-internal")]
        numeric,
        raw_numeric_result: true,
    })
}

struct RawArithmeticOperand<'a, I, F> {
    evaluator: RawEvaluator<'a, I, ExpressionTerm, F>,
    is_variable: bool,
    numeric_result: bool,
}

impl<I, F> RawArithmeticOperand<'_, I, F> {
    fn evaluate(&self, tuple: &I) -> Result<ArithmeticOperand, EvalFailure<F>> {
        let value = (self.evaluator)(tuple)?;
        let value = if self.is_variable {
            nullable::variable_operand(value)?
        } else {
            value
        };
        Ok(ArithmeticOperand::new(value, self.numeric_result))
    }
}

fn build_raw_arithmetic_operand<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
) -> Result<RawArithmeticOperand<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    let compiled = build_compiled_expression(expression, context, true, None)?;
    Ok(RawArithmeticOperand {
        evaluator: compiled
            .raw
            .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?,
        is_variable: matches!(expression, Expression::Variable(_)),
        numeric_result: compiled.raw_numeric_result,
    })
}

fn build_chain_arithmetic<'a, 'e, C: ExpressionEvaluatorContext<'a>>(
    first: &Expression,
    tail: impl Iterator<Item = (ArithmeticOp, &'e Expression)>,
    context: &mut C,
    raw_requested: bool,
    numeric_hooks: Option<&NumericExpressionHooks<'a, C::Term, C::Error>>,
    kind: ArithmeticChainKind,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    if !raw_requested && numeric_hooks.is_none() {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    let tail = tail.collect::<Vec<_>>();
    if tail.is_empty() {
        // Empty precedence productions are transparent in the original
        // parser; a hand-built empty chain must not invent a catch boundary.
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    #[cfg(feature = "rdflib760-nullable-internal")]
    if let Some(hooks) = numeric_hooks {
        let first = build_numeric_arithmetic_operand(first, context, hooks)?;
        let tail = tail
            .into_iter()
            .map(|(op, expression)| {
                Ok((
                    numeric_binary_op(op),
                    build_numeric_arithmetic_operand(expression, context, hooks)?,
                ))
            })
            .collect::<Result<Vec<_>, ExpressionEvaluationError<C::Error>>>()?;
        let kind = match kind {
            ArithmeticChainKind::Additive => NumericChainKind::Additive,
            ArithmeticChainKind::Multiplicative => NumericChainKind::Multiplicative,
        };
        let session = Rc::clone(&hooks.session);
        let failure = hooks.failure;
        let evaluator: RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error> =
            Rc::new(move |tuple| {
                nullable::capture_sparql((|| {
                    let first = first.evaluate(tuple)?;
                    let tail = tail
                        .iter()
                        .map(|(op, operand)| Ok((*op, operand.evaluate(tuple)?)))
                        .collect::<Result<Vec<_>, EvalFailure<C::Error>>>()?;
                    session
                        .evaluate_chain_value(kind, first, tail)
                        .map_err(|error| numeric_evaluation_failure(error, failure))
                })())
            });
        return Ok(CompiledExpression {
            legacy: None,
            raw: None,
            numeric: Some(evaluator),
            raw_numeric_result: false,
        });
    }
    let first = build_raw_arithmetic_operand(first, context)?;
    let tail = tail
        .into_iter()
        .map(|(op, operand)| Ok((op, build_raw_arithmetic_operand(operand, context)?)))
        .collect::<Result<Vec<_>, ExpressionEvaluationError<C::Error>>>()?;
    let unsupported: fn(&'static str) -> C::Error = C::unsupported_nullable_evaluation;
    let raw: RawEvaluator<'a, C::Tuple, ExpressionTerm, C::Error> = Rc::new(move |tuple| {
        nullable::capture_sparql((|| {
            let first = first.evaluate(tuple)?;
            // RDFLib fetches e.expr and then the complete e.other list before
            // numeric conversion or any arithmetic. Fatal tail evaluation
            // therefore precedes even an earlier division by zero.
            let tail = tail
                .iter()
                .map(|(op, operand)| Ok((*op, operand.evaluate(tuple)?)))
                .collect::<Result<Vec<_>, EvalFailure<C::Error>>>()?;
            nullable::typed_chain(kind, first, tail, unsupported)
        })())
    });
    Ok(CompiledExpression {
        legacy: None,
        raw: Some(raw),
        #[cfg(feature = "rdflib760-nullable-internal")]
        numeric: None,
        raw_numeric_result: true,
    })
}

/// The source registry lookup happens before inspecting arguments. A known
/// default_cast reads e.expr three separate times; each read evaluates the
/// whole list in order. Do not fold these reads or memoize nested expressions.
#[cfg(feature = "rdflib760-nullable-internal")]
fn build_source_function<'a, C: ExpressionEvaluatorContext<'a>>(
    iri: &NamedNode,
    parameters: &[Expression],
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    use crate::numeric::source_function::{SourceFunctionKind, source_function_error};
    let kind = SourceFunctionKind::registered(iri);
    let operands = if kind.is_some() {
        parameters
            .iter()
            .map(|e| build_numeric_arithmetic_operand(e, context, hooks))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        // Unknown functions never inspect/compile unsupported operands.
        Vec::new()
    };
    let session = Rc::clone(&hooks.session);
    let failure = hooks.failure;
    let iri = iri.clone();
    let evaluator = Rc::new(move |tuple: &C::Tuple| {
        #[cfg(test)]
        crate::numeric::source_function::observation::call(&iri);
        let result = nullable::capture_sparql((|| {
            let Some(kind) = kind else {
                let error = session
                    .source_unknown_function(&iri)
                    .map_err(|e| EvalFailure::Fatal(failure(e)))?;
                return Err(numeric_evaluation_failure(error, failure));
            };
            let evaluate_args = || {
                operands
                    .iter()
                    .map(|e| e.evaluate(tuple))
                    .collect::<Result<Vec<_>, _>>()
            };
            if evaluate_args()?.is_empty() {
                return Err(numeric_evaluation_failure(
                    source_function_error("Nothing given to cast."),
                    failure,
                ));
            }
            if evaluate_args()?.len() > 1 {
                return Err(numeric_evaluation_failure(
                    source_function_error("Cannot cast more than one thing!"),
                    failure,
                ));
            }
            let args = evaluate_args()?;
            session
                .source_default_cast(kind, &args[0])
                .map(SolutionValue::Rdf)
                .map_err(|e| numeric_evaluation_failure(e, failure))
        })());
        #[cfg(test)]
        if let Ok(value) = &result {
            crate::numeric::source_function::observation::result(&iri, value);
        }
        result
    });
    Ok(CompiledExpression {
        legacy: None,
        raw: None,
        numeric: Some(evaluator),
        raw_numeric_result: false,
    })
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn numeric_binary_op(op: ArithmeticOp) -> NumericBinaryOp {
    match op {
        ArithmeticOp::Add => NumericBinaryOp::Add,
        ArithmeticOp::Subtract => NumericBinaryOp::Subtract,
        ArithmeticOp::Multiply => NumericBinaryOp::Multiply,
        ArithmeticOp::Divide => NumericBinaryOp::Divide,
    }
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn build_source_relational_order<'a, C: ExpressionEvaluatorContext<'a>>(
    left: &Expression,
    right: &Expression,
    operator: crate::numeric::SourceRelationalOrder,
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
    raw_requested: bool,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    if raw_requested
        || !source_execution_hooks(context, hooks)
        || !hooks
            .prepared_literals
            .as_ref()
            .is_some_and(|registry| registry.admits_graph_execution(&hooks.session))
    {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    let left = build_numeric_arithmetic_operand(left, context, hooks)?;
    let right = build_numeric_arithmetic_operand(right, context, hooks)?;
    let session = Rc::clone(&hooks.session);
    let failure = hooks.failure;
    Ok(CompiledExpression {
        legacy: None,
        raw: None,
        numeric: Some(Rc::new(move |tuple| {
            nullable::capture_sparql((|| {
                let left = left.evaluate(tuple)?;
                let right = right.evaluate(tuple)?;
                session
                    .source_relational_order(&left, &right, operator)
                    .map(SolutionValue::Rdf)
                    .map_err(|error| numeric_evaluation_failure(error, failure))
            })())
        })),
        raw_numeric_result: false,
    })
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn source_execution_hooks<'a, C: ExpressionEvaluatorContext<'a>>(
    context: &C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
) -> bool {
    hooks.session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1
        && hooks.session.context_semantics()
            == crate::numeric::QueryContextSemantics::SourceContextV1
        && hooks.prepared_literals.as_ref().is_some_and(|registry| {
            registry.admits_exists_execution(&hooks.session)
                && context.admits_source_execution(&hooks.session, registry)
        })
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn numeric_evaluation_failure<F>(
    error: NumericEvaluationError,
    failure: fn(RawNumericFault) -> F,
) -> EvalFailure<F> {
    match error.into_error_value() {
        Ok(error) => EvalFailure::Sparql(error),
        Err(error) => EvalFailure::Fatal(failure(error)),
    }
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn build_numeric_variable_evaluator<'a, C: ExpressionEvaluatorContext<'a>>(
    variable: &Variable,
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
    operand: bool,
) -> RawEvaluator<'a, C::Tuple, RawExpressionValue, C::Error> {
    let lookup = context.build_solution_variable_lookup(variable);
    let externalize = Rc::clone(&hooks.externalize);
    let source_literal =
        hooks.session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1;
    let name = variable.as_str().to_owned();
    Rc::new(move |tuple| {
        let value = if source_literal && operand {
            nullable::source_variable_operand(nullable::source_raw_variable(lookup(tuple))?)
        } else if source_literal {
            nullable::source_evaluated_variable(lookup(tuple), &name)
        } else {
            nullable::raw_variable(lookup(tuple))
        }?;
        match value {
            SolutionValue::Rdf(value) => externalize(value)
                .map(SolutionValue::Rdf)
                .map_err(EvalFailure::Fatal),
            SolutionValue::ExplicitNone => Ok(SolutionValue::ExplicitNone),
            SolutionValue::ReturnedError(error) => Ok(SolutionValue::ReturnedError(error)),
        }
    })
}

#[cfg(feature = "rdflib760-nullable-internal")]
struct NumericArithmeticOperand<'a, I, F> {
    evaluator: RawEvaluator<'a, I, RawExpressionValue, F>,
    is_variable: bool,
    source_literal: bool,
}
#[cfg(feature = "rdflib760-nullable-internal")]
impl<I, F> NumericArithmeticOperand<'_, I, F> {
    fn evaluate(&self, tuple: &I) -> Result<SolutionValue<RawExpressionValue>, EvalFailure<F>> {
        let value = (self.evaluator)(tuple)?;
        if self.is_variable {
            if self.source_literal {
                nullable::source_variable_operand(value)
            } else {
                nullable::variable_operand(value)
            }
        } else {
            Ok(value)
        }
    }
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn build_numeric_arithmetic_operand<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<NumericArithmeticOperand<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    let evaluator = if let Expression::Variable(variable) = expression {
        build_numeric_variable_evaluator(variable, context, hooks, true)
    } else {
        build_compiled_expression(expression, context, false, Some(hooks))?
            .numeric
            .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?
    };
    Ok(NumericArithmeticOperand {
        evaluator,
        is_variable: matches!(expression, Expression::Variable(_)),
        source_literal: hooks.session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1,
    })
}

/// Evaluates an expression and returns an internal term
///
/// Returns None if building such expression implies to convert into an [`ExpressionTerm`] then back.
pub fn try_build_internal_expression_evaluator<'a, C: ExpressionEvaluatorContext<'a>>(
    expression: &Expression,
    context: &mut C,
) -> Result<Option<ExpressionEvaluator<'a, C::Tuple, C::Term>>, ExpressionEvaluationError<C::Error>>
{
    Ok(Some(match expression {
        Expression::NamedNode(t) => {
            let t = context
                .internalize_named_node(t)
                .map_err(ExpressionEvaluationError::Context)?;
            Rc::new(move |_| Some(t.clone()))
        }
        Expression::Literal(t) => {
            let t = context
                .internalize_literal(t)
                .map_err(ExpressionEvaluationError::Context)?;
            Rc::new(move |_| Some(t.clone()))
        }
        Expression::Variable(v) => Rc::new(context.build_variable_lookup(v)),
        Expression::Coalesce(l) => {
            let Some(l) = l
                .iter()
                .map(|e| try_build_internal_expression_evaluator(e, context))
                .collect::<Result<Option<Vec<_>>, _>>()?
            else {
                return Ok(None);
            };
            Rc::new(move |tuple| {
                for e in &l {
                    if let Some(result) = e(tuple) {
                        return Some(result);
                    }
                }
                None
            })
        }
        Expression::If(a, b, c) => {
            let a = build_expression_evaluator(a, context)?;
            let Some(b) = try_build_internal_expression_evaluator(b, context)? else {
                return Ok(None);
            };
            let Some(c) = try_build_internal_expression_evaluator(c, context)? else {
                return Ok(None);
            };
            Rc::new(move |tuple| {
                if a(tuple)?.effective_boolean_value()? {
                    b(tuple)
                } else {
                    c(tuple)
                }
            })
        }
        _ => return Ok(None),
    }))
}

fn build_hash_expression_evaluator<'a, C: ExpressionEvaluatorContext<'a>, H: Digest>(
    parameters: &[Expression],
    context: &mut C,
) -> Result<ExpressionEvaluator<'a, C::Tuple, ExpressionTerm>, ExpressionEvaluationError<C::Error>>
{
    let arg = build_expression_evaluator(&parameters[0], context)?;
    Ok(Rc::new(move |tuple| {
        let ExpressionTerm::StringLiteral(input) = arg(tuple)? else {
            return None;
        };
        let hash = hex::encode(H::new().chain_update(input.as_str()).finalize());
        Some(ExpressionTerm::StringLiteral(hash))
    }))
}

#[cfg(feature = "sparql-12")]
type LanguageWithMaybeBaseDirection = (String, Option<BaseDirection>);
#[cfg(not(feature = "sparql-12"))]
type LanguageWithMaybeBaseDirection = String;

#[cfg(feature = "sparql-12")]
fn to_string_and_language(
    term: ExpressionTerm,
) -> Option<(String, Option<LanguageWithMaybeBaseDirection>)> {
    match term {
        ExpressionTerm::StringLiteral(value) => Some((value, None)),
        ExpressionTerm::LangStringLiteral { value, language } => {
            Some((value, Some((language, None))))
        }
        ExpressionTerm::DirLangStringLiteral {
            value,
            language,
            direction,
        } => Some((value, Some((language, Some(direction))))),
        _ => None,
    }
}

#[cfg(not(feature = "sparql-12"))]
fn to_string_and_language(
    term: ExpressionTerm,
) -> Option<(String, Option<LanguageWithMaybeBaseDirection>)> {
    match term {
        ExpressionTerm::StringLiteral(value) => Some((value, None)),
        ExpressionTerm::LangStringLiteral { value, language } => Some((value, Some(language))),
        _ => None,
    }
}

#[cfg(feature = "sparql-12")]
fn build_plain_literal(
    value: String,
    language: Option<LanguageWithMaybeBaseDirection>,
) -> ExpressionTerm {
    if let Some((language, direction)) = language {
        if let Some(direction) = direction {
            ExpressionTerm::DirLangStringLiteral {
                value,
                language,
                direction,
            }
        } else {
            ExpressionTerm::LangStringLiteral { value, language }
        }
    } else {
        ExpressionTerm::StringLiteral(value)
    }
}

#[cfg(not(feature = "sparql-12"))]
fn build_plain_literal(
    value: String,
    language: Option<LanguageWithMaybeBaseDirection>,
) -> ExpressionTerm {
    if let Some(language) = language {
        ExpressionTerm::LangStringLiteral { value, language }
    } else {
        ExpressionTerm::StringLiteral(value)
    }
}

fn to_argument_compatible_strings(
    arg1: ExpressionTerm,
    arg2: ExpressionTerm,
) -> Option<(String, String, Option<LanguageWithMaybeBaseDirection>)> {
    let (value1, language1) = to_string_and_language(arg1)?;
    let (value2, language2) = to_string_and_language(arg2)?;
    (language2.is_none() || language1 == language2).then_some((value1, value2, language1))
}

fn compile_static_pattern_if_exists(
    pattern: &Expression,
    options: Option<&Expression>,
) -> Option<Regex> {
    let static_pattern = if let Expression::Literal(pattern) = pattern {
        (pattern.datatype() == xsd::STRING).then(|| pattern.value())
    } else {
        None
    };
    let static_options = if let Some(options) = options {
        if let Expression::Literal(options) = options {
            (options.datatype() == xsd::STRING).then(|| Some(options.value()))
        } else {
            None
        }
    } else {
        Some(None)
    };
    if let (Some(static_pattern), Some(static_options)) = (static_pattern, static_options) {
        compile_pattern(static_pattern, static_options)
    } else {
        None
    }
}

fn compile_pattern(pattern: &str, flags: Option<&str>) -> Option<Regex> {
    let mut pattern = Cow::Borrowed(pattern);
    let flags = flags.unwrap_or_default();
    if flags.contains('q') {
        pattern = regex::escape(&pattern).into();
    }
    let mut regex_builder = RegexBuilder::new(&pattern);
    regex_builder.size_limit(REGEX_SIZE_LIMIT);
    for flag in flags.chars() {
        match flag {
            's' => {
                regex_builder.dot_matches_new_line(true);
            }
            'm' => {
                regex_builder.multi_line(true);
            }
            'i' => {
                regex_builder.case_insensitive(true);
            }
            'x' => {
                regex_builder.ignore_whitespace(true);
            }
            'q' => (),        // Already supported
            _ => return None, // invalid option
        }
    }
    regex_builder.build().ok()
}

/// Equality operator (=)
fn equals(a: &ExpressionTerm, b: &ExpressionTerm) -> Option<bool> {
    match a {
        ExpressionTerm::NamedNode(_)
        | ExpressionTerm::BlankNode(_)
        | ExpressionTerm::LangStringLiteral { .. } => Some(a == b),
        #[cfg(feature = "sparql-12")]
        ExpressionTerm::DirLangStringLiteral { .. } => Some(a == b),
        ExpressionTerm::StringLiteral(a) => match b {
            ExpressionTerm::StringLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        ExpressionTerm::OtherTypedLiteral { .. } => match b {
            ExpressionTerm::OtherTypedLiteral { .. } if a == b => Some(true),
            ExpressionTerm::NamedNode(_)
            | ExpressionTerm::BlankNode(_)
            | ExpressionTerm::LangStringLiteral { .. } => Some(false),
            #[cfg(feature = "sparql-12")]
            ExpressionTerm::DirLangStringLiteral { .. } => Some(false),
            #[cfg(feature = "sparql-12")]
            ExpressionTerm::Triple(_) => Some(false),
            _ => None,
        },
        ExpressionTerm::BooleanLiteral(a) => match b {
            ExpressionTerm::BooleanLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        ExpressionTerm::FloatLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => Some(a == b),
            ExpressionTerm::DoubleLiteral(b) => Some(Double::from(*a) == *b),
            ExpressionTerm::IntegerLiteral(b) => Some(*a == (*b).into()),
            ExpressionTerm::DecimalLiteral(b) => Some(*a == (*b).into()),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        ExpressionTerm::DoubleLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => Some(*a == (*b).into()),
            ExpressionTerm::DoubleLiteral(b) => Some(a == b),
            ExpressionTerm::IntegerLiteral(b) => Some(*a == (*b).into()),
            ExpressionTerm::DecimalLiteral(b) => Some(*a == (*b).into()),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        ExpressionTerm::IntegerLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => Some(Float::from(*a) == *b),
            ExpressionTerm::DoubleLiteral(b) => Some(Double::from(*a) == *b),
            ExpressionTerm::IntegerLiteral(b) => Some(a == b),
            ExpressionTerm::DecimalLiteral(b) => Some(Decimal::from(*a) == *b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        ExpressionTerm::DecimalLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => Some(Float::from(*a) == *b),
            ExpressionTerm::DoubleLiteral(b) => Some(Double::from(*a) == *b),
            ExpressionTerm::IntegerLiteral(b) => Some(*a == (*b).into()),
            ExpressionTerm::DecimalLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        ExpressionTerm::DateTimeLiteral(a) => match b {
            ExpressionTerm::DateTimeLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::TimeLiteral(a) => match b {
            ExpressionTerm::TimeLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DateLiteral(a) => match b {
            ExpressionTerm::DateLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GYearMonthLiteral(a) => match b {
            ExpressionTerm::GYearMonthLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GYearLiteral(a) => match b {
            ExpressionTerm::GYearLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GMonthDayLiteral(a) => match b {
            ExpressionTerm::GMonthDayLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GDayLiteral(a) => match b {
            ExpressionTerm::GDayLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GMonthLiteral(a) => match b {
            ExpressionTerm::GMonthLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DurationLiteral(a) => match b {
            ExpressionTerm::DurationLiteral(b) => Some(a == b),
            ExpressionTerm::YearMonthDurationLiteral(b) => Some(a == b),
            ExpressionTerm::DayTimeDurationLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::YearMonthDurationLiteral(a) => match b {
            ExpressionTerm::DurationLiteral(b) => Some(a == b),
            ExpressionTerm::YearMonthDurationLiteral(b) => Some(a == b),
            ExpressionTerm::DayTimeDurationLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DayTimeDurationLiteral(a) => match b {
            ExpressionTerm::DurationLiteral(b) => Some(a == b),
            ExpressionTerm::YearMonthDurationLiteral(b) => Some(a == b),
            ExpressionTerm::DayTimeDurationLiteral(b) => Some(a == b),
            ExpressionTerm::OtherTypedLiteral { .. } => None,
            _ => Some(false),
        },
        #[cfg(feature = "sparql-12")]
        ExpressionTerm::Triple(a) => {
            if let ExpressionTerm::Triple(b) = b {
                triple_equals(a, b)
            } else {
                Some(false)
            }
        }
    }
}

#[cfg(feature = "sparql-12")]
fn triple_equals(a: &ExpressionTriple, b: &ExpressionTriple) -> Option<bool> {
    Some(a.subject == b.subject && a.predicate == b.predicate && equals(&a.object, &b.object)?)
}

/// Comparison for <, >, <= and >= operators
fn partial_cmp(a: &ExpressionTerm, b: &ExpressionTerm) -> Option<Ordering> {
    if a == b {
        return Some(Ordering::Equal);
    }
    #[cfg(feature = "sparql-12")]
    if let ExpressionTerm::Triple(a) = a {
        return if let ExpressionTerm::Triple(b) = b {
            partial_cmp_triples(a, b)
        } else {
            None
        };
    }
    partial_cmp_literals(a, b)
}

pub fn partial_cmp_literals(a: &ExpressionTerm, b: &ExpressionTerm) -> Option<Ordering> {
    match a {
        ExpressionTerm::StringLiteral(a) => {
            if let ExpressionTerm::StringLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        ExpressionTerm::LangStringLiteral {
            value: va,
            language: la,
        } => {
            if let ExpressionTerm::LangStringLiteral {
                value: vb,
                language: lb,
            } = b
            {
                if la == lb { va.partial_cmp(vb) } else { None }
            } else {
                None
            }
        }
        #[cfg(feature = "sparql-12")]
        ExpressionTerm::DirLangStringLiteral {
            value: va,
            language: la,
            direction: da,
        } => {
            if let ExpressionTerm::DirLangStringLiteral {
                value: vb,
                language: lb,
                direction: db,
            } = b
            {
                if la == lb && da == db {
                    va.partial_cmp(vb)
                } else {
                    None
                }
            } else {
                None
            }
        }
        ExpressionTerm::FloatLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::DoubleLiteral(b) => Double::from(*a).partial_cmp(b),
            ExpressionTerm::IntegerLiteral(b) => a.partial_cmp(&Float::from(*b)),
            ExpressionTerm::DecimalLiteral(b) => a.partial_cmp(&(*b).into()),
            _ => None,
        },
        ExpressionTerm::DoubleLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => a.partial_cmp(&(*b).into()),
            ExpressionTerm::DoubleLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::IntegerLiteral(b) => a.partial_cmp(&Double::from(*b)),
            ExpressionTerm::DecimalLiteral(b) => a.partial_cmp(&(*b).into()),
            _ => None,
        },
        ExpressionTerm::IntegerLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => Float::from(*a).partial_cmp(b),
            ExpressionTerm::DoubleLiteral(b) => Double::from(*a).partial_cmp(b),
            ExpressionTerm::IntegerLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::DecimalLiteral(b) => Decimal::from(*a).partial_cmp(b),
            _ => None,
        },
        ExpressionTerm::DecimalLiteral(a) => match b {
            ExpressionTerm::FloatLiteral(b) => Float::from(*a).partial_cmp(b),
            ExpressionTerm::DoubleLiteral(b) => Double::from(*a).partial_cmp(b),
            ExpressionTerm::IntegerLiteral(b) => a.partial_cmp(&Decimal::from(*b)),
            ExpressionTerm::DecimalLiteral(b) => a.partial_cmp(b),
            _ => None,
        },
        ExpressionTerm::DateTimeLiteral(a) => {
            if let ExpressionTerm::DateTimeLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::TimeLiteral(a) => {
            if let ExpressionTerm::TimeLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DateLiteral(a) => {
            if let ExpressionTerm::DateLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GYearMonthLiteral(a) => {
            if let ExpressionTerm::GYearMonthLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GYearLiteral(a) => {
            if let ExpressionTerm::GYearLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GMonthDayLiteral(a) => {
            if let ExpressionTerm::GMonthDayLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GDayLiteral(a) => {
            if let ExpressionTerm::GDayLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "calendar-ext")]
        ExpressionTerm::GMonthLiteral(a) => {
            if let ExpressionTerm::GMonthLiteral(b) = b {
                a.partial_cmp(b)
            } else {
                None
            }
        }
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DurationLiteral(a) => match b {
            ExpressionTerm::DurationLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::YearMonthDurationLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::DayTimeDurationLiteral(b) => a.partial_cmp(b),
            _ => None,
        },
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::YearMonthDurationLiteral(a) => match b {
            ExpressionTerm::DurationLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::YearMonthDurationLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::DayTimeDurationLiteral(b) => a.partial_cmp(b),
            _ => None,
        },
        #[cfg(feature = "sep-0002")]
        ExpressionTerm::DayTimeDurationLiteral(a) => match b {
            ExpressionTerm::DurationLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::YearMonthDurationLiteral(b) => a.partial_cmp(b),
            ExpressionTerm::DayTimeDurationLiteral(b) => a.partial_cmp(b),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(feature = "sparql-12")]
fn partial_cmp_triples(a: &ExpressionTriple, b: &ExpressionTriple) -> Option<Ordering> {
    // We compare subjects
    match (&a.subject, &b.subject) {
        (NamedOrBlankNode::NamedNode(a), NamedOrBlankNode::NamedNode(b)) => {
            if a != b {
                return None;
            }
        }
        (NamedOrBlankNode::BlankNode(a), NamedOrBlankNode::BlankNode(b)) => {
            if a != b {
                return None;
            }
        }
        _ => return None,
    }
    if a.predicate != b.predicate {
        return None;
    }
    partial_cmp(&a.object, &b.object)
}

pub enum NumericBinaryOperands {
    Float(Float, Float),
    Double(Double, Double),
    Integer(Integer, Integer),
    Decimal(Decimal, Decimal),
    #[cfg(feature = "sep-0002")]
    Duration(Duration, Duration),
    #[cfg(feature = "sep-0002")]
    YearMonthDuration(YearMonthDuration, YearMonthDuration),
    #[cfg(feature = "sep-0002")]
    DayTimeDuration(DayTimeDuration, DayTimeDuration),
    #[cfg(feature = "sep-0002")]
    DateTime(DateTime, DateTime),
    #[cfg(feature = "sep-0002")]
    Time(Time, Time),
    #[cfg(feature = "sep-0002")]
    Date(Date, Date),
    #[cfg(feature = "sep-0002")]
    DateTimeDuration(DateTime, Duration),
    #[cfg(feature = "sep-0002")]
    DateTimeYearMonthDuration(DateTime, YearMonthDuration),
    #[cfg(feature = "sep-0002")]
    DateTimeDayTimeDuration(DateTime, DayTimeDuration),
    #[cfg(feature = "sep-0002")]
    DateDuration(Date, Duration),
    #[cfg(feature = "sep-0002")]
    DateYearMonthDuration(Date, YearMonthDuration),
    #[cfg(feature = "sep-0002")]
    DateDayTimeDuration(Date, DayTimeDuration),
    #[cfg(feature = "sep-0002")]
    TimeDuration(Time, Duration),
    #[cfg(feature = "sep-0002")]
    TimeDayTimeDuration(Time, DayTimeDuration),
}

impl NumericBinaryOperands {
    pub fn new(a: ExpressionTerm, b: ExpressionTerm) -> Option<Self> {
        match (a, b) {
            (ExpressionTerm::FloatLiteral(v1), ExpressionTerm::FloatLiteral(v2)) => {
                Some(Self::Float(v1, v2))
            }
            (ExpressionTerm::FloatLiteral(v1), ExpressionTerm::DoubleLiteral(v2)) => {
                Some(Self::Double(v1.into(), v2))
            }
            (ExpressionTerm::FloatLiteral(v1), ExpressionTerm::IntegerLiteral(v2)) => {
                Some(Self::Float(v1, v2.into()))
            }
            (ExpressionTerm::FloatLiteral(v1), ExpressionTerm::DecimalLiteral(v2)) => {
                Some(Self::Float(v1, v2.into()))
            }
            (ExpressionTerm::DoubleLiteral(v1), ExpressionTerm::FloatLiteral(v2)) => {
                Some(Self::Double(v1, v2.into()))
            }
            (ExpressionTerm::DoubleLiteral(v1), ExpressionTerm::DoubleLiteral(v2)) => {
                Some(Self::Double(v1, v2))
            }
            (ExpressionTerm::DoubleLiteral(v1), ExpressionTerm::IntegerLiteral(v2)) => {
                Some(Self::Double(v1, v2.into()))
            }
            (ExpressionTerm::DoubleLiteral(v1), ExpressionTerm::DecimalLiteral(v2)) => {
                Some(Self::Double(v1, v2.into()))
            }
            (ExpressionTerm::IntegerLiteral(v1), ExpressionTerm::FloatLiteral(v2)) => {
                Some(Self::Float(v1.into(), v2))
            }
            (ExpressionTerm::IntegerLiteral(v1), ExpressionTerm::DoubleLiteral(v2)) => {
                Some(Self::Double(v1.into(), v2))
            }
            (ExpressionTerm::IntegerLiteral(v1), ExpressionTerm::IntegerLiteral(v2)) => {
                Some(Self::Integer(v1, v2))
            }
            (ExpressionTerm::IntegerLiteral(v1), ExpressionTerm::DecimalLiteral(v2)) => {
                Some(Self::Decimal(v1.into(), v2))
            }
            (ExpressionTerm::DecimalLiteral(v1), ExpressionTerm::FloatLiteral(v2)) => {
                Some(Self::Float(v1.into(), v2))
            }
            (ExpressionTerm::DecimalLiteral(v1), ExpressionTerm::DoubleLiteral(v2)) => {
                Some(Self::Double(v1.into(), v2))
            }
            (ExpressionTerm::DecimalLiteral(v1), ExpressionTerm::IntegerLiteral(v2)) => {
                Some(Self::Decimal(v1, v2.into()))
            }
            (ExpressionTerm::DecimalLiteral(v1), ExpressionTerm::DecimalLiteral(v2)) => {
                Some(Self::Decimal(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DurationLiteral(v1), ExpressionTerm::DurationLiteral(v2)) => {
                Some(Self::Duration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DurationLiteral(v1), ExpressionTerm::YearMonthDurationLiteral(v2)) => {
                Some(Self::Duration(v1, v2.into()))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DurationLiteral(v1), ExpressionTerm::DayTimeDurationLiteral(v2)) => {
                Some(Self::Duration(v1, v2.into()))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::YearMonthDurationLiteral(v1), ExpressionTerm::DurationLiteral(v2)) => {
                Some(Self::Duration(v1.into(), v2))
            }
            #[cfg(feature = "sep-0002")]
            (
                ExpressionTerm::YearMonthDurationLiteral(v1),
                ExpressionTerm::YearMonthDurationLiteral(v2),
            ) => Some(Self::YearMonthDuration(v1, v2)),
            #[cfg(feature = "sep-0002")]
            (
                ExpressionTerm::YearMonthDurationLiteral(v1),
                ExpressionTerm::DayTimeDurationLiteral(v2),
            ) => Some(Self::Duration(v1.into(), v2.into())),
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DayTimeDurationLiteral(v1), ExpressionTerm::DurationLiteral(v2)) => {
                Some(Self::Duration(v1.into(), v2))
            }
            #[cfg(feature = "sep-0002")]
            (
                ExpressionTerm::DayTimeDurationLiteral(v1),
                ExpressionTerm::YearMonthDurationLiteral(v2),
            ) => Some(Self::Duration(v1.into(), v2.into())),
            #[cfg(feature = "sep-0002")]
            (
                ExpressionTerm::DayTimeDurationLiteral(v1),
                ExpressionTerm::DayTimeDurationLiteral(v2),
            ) => Some(Self::DayTimeDuration(v1, v2)),
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateTimeLiteral(v1), ExpressionTerm::DateTimeLiteral(v2)) => {
                Some(Self::DateTime(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateLiteral(v1), ExpressionTerm::DateLiteral(v2)) => {
                Some(Self::Date(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::TimeLiteral(v1), ExpressionTerm::TimeLiteral(v2)) => {
                Some(Self::Time(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateTimeLiteral(v1), ExpressionTerm::DurationLiteral(v2)) => {
                Some(Self::DateTimeDuration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateTimeLiteral(v1), ExpressionTerm::YearMonthDurationLiteral(v2)) => {
                Some(Self::DateTimeYearMonthDuration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateTimeLiteral(v1), ExpressionTerm::DayTimeDurationLiteral(v2)) => {
                Some(Self::DateTimeDayTimeDuration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateLiteral(v1), ExpressionTerm::DurationLiteral(v2)) => {
                Some(Self::DateDuration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateLiteral(v1), ExpressionTerm::YearMonthDurationLiteral(v2)) => {
                Some(Self::DateYearMonthDuration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::DateLiteral(v1), ExpressionTerm::DayTimeDurationLiteral(v2)) => {
                Some(Self::DateDayTimeDuration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::TimeLiteral(v1), ExpressionTerm::DurationLiteral(v2)) => {
                Some(Self::TimeDuration(v1, v2))
            }
            #[cfg(feature = "sep-0002")]
            (ExpressionTerm::TimeLiteral(v1), ExpressionTerm::DayTimeDurationLiteral(v2)) => {
                Some(Self::TimeDayTimeDuration(v1, v2))
            }
            _ => None,
        }
    }
}

struct ZipLongest<T1, T2, I1: Iterator<Item = T1>, I2: Iterator<Item = T2>> {
    a: I1,
    b: I2,
}

impl<T1, T2, I1: Iterator<Item = T1>, I2: Iterator<Item = T2>> ZipLongest<T1, T2, I1, I2> {
    fn new(a: I1, b: I2) -> Self {
        Self { a, b }
    }
}

impl<T1, T2, I1: Iterator<Item = T1>, I2: Iterator<Item = T2>> Iterator
    for ZipLongest<T1, T2, I1, I2>
{
    type Item = (Option<T1>, Option<T2>);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.a.next(), self.b.next()) {
            (None, None) => None,
            r => Some(r),
        }
    }
}

fn generate_uuid(buffer: &mut String) {
    let mut uuid = random::<u128>().to_le_bytes();
    uuid[6] = (uuid[6] & 0x0F) | 0x40;
    uuid[8] = (uuid[8] & 0x3F) | 0x80;

    write_hexa_bytes(&uuid[0..4], buffer);
    buffer.push('-');
    write_hexa_bytes(&uuid[4..6], buffer);
    buffer.push('-');
    write_hexa_bytes(&uuid[6..8], buffer);
    buffer.push('-');
    write_hexa_bytes(&uuid[8..10], buffer);
    buffer.push('-');
    write_hexa_bytes(&uuid[10..16], buffer);
}

fn write_hexa_bytes(bytes: &[u8], buffer: &mut String) {
    for b in bytes {
        let high = b / 16;
        buffer.push(char::from(if high < 10 {
            b'0' + high
        } else {
            b'a' + (high - 10)
        }));
        let low = b % 16;
        buffer.push(char::from(if low < 10 {
            b'0' + low
        } else {
            b'a' + (low - 10)
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid() {
        let mut buffer = String::default();
        generate_uuid(&mut buffer);
        assert!(
            Regex::new("^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
                .unwrap()
                .is_match(&buffer),
            "{buffer} is not a valid UUID"
        );
    }
}
