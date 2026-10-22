//! Runtime source registry dispatch; expression evaluation remains with its owner.
use super::*;
use crate::numeric::source_function::source_function_error;
use crate::numeric::source_registry::*;
use std::cell::RefCell;

struct CallView<'a> {
    syntax: &'a spargebra::SourceSyntaxSnapshot,
    iri: &'a RawExpressionValue,
    session: &'a QueryNumericSession,
    read_arguments:
        &'a dyn Fn()
            -> Result<Option<Vec<SolutionValue<RawExpressionValue>>>, SourceFunctionFailure>,
    read_binding: &'a dyn Fn(&Variable) -> SourceFunctionResult,
}
impl SourceFunctionContext for CallView<'_> {
    fn syntax(&self) -> &spargebra::SourceSyntaxSnapshot {
        self.syntax
    }
    fn iri(&self) -> RawExpressionValue {
        self.iri.clone()
    }
    fn arguments(
        &self,
    ) -> Result<Option<Vec<SolutionValue<RawExpressionValue>>>, SourceFunctionFailure> {
        (self.read_arguments)()
    }
    fn binding(&self, variable: &Variable) -> SourceFunctionResult {
        (self.read_binding)(variable)
    }
    fn session(&self) -> &QueryNumericSession {
        self.session
    }
}

fn observed_failure<'a, C: ExpressionEvaluatorContext<'a>>(
    error: EvalFailure<C::Error>,
    host: &RefCell<Option<C::Error>>,
) -> SourceFunctionFailure {
    match error {
        EvalFailure::Sparql(error) => SourceFunctionFailure::Sparql(error),
        EvalFailure::Fatal(error) => {
            if let Some(source) =
                C::source_native_fault(&error).and_then(RawNumericFault::source_language_exception)
            {
                return SourceFunctionFailure::Exception(source.clone());
            }
            if host.borrow().is_none() {
                *host.borrow_mut() = Some(error);
            }
            SourceFunctionFailure::Host
        }
    }
}
fn numeric_failure(error: crate::numeric::NumericEvaluationError) -> SourceFunctionFailure {
    match error.into_error_value() {
        Ok(error) => SourceFunctionFailure::Sparql(error),
        Err(
            RawNumericFault::SourceFunctionException(error)
            | RawNumericFault::SourceLanguageException(error),
        ) => SourceFunctionFailure::Exception(error),
        Err(error) => SourceFunctionFailure::Unreceived(error),
    }
}

/// operators.Builtin_STR preserves a returned SPARQL error, and otherwise
/// constructs a plain Literal from source str. It does not swallow host or
/// language exceptions and does not invoke the custom-function registry.
pub(super) fn build_str<'a, C: ExpressionEvaluatorContext<'a>>(
    parameters: &[Expression],
    syntax: &spargebra::SourceSyntaxSnapshot,
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    let node = syntax
        .root_node()
        .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
    if node.name != "Builtin_STR"
        || parameters.len() != 1
        || !node.fields.iter().any(|(key, _)| key == "arg")
    {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    let operand = build_numeric_arithmetic_operand(&parameters[0], context, hooks)?;
    let session = Rc::clone(&hooks.session);
    let failure = hooks.failure;
    let evaluator = Rc::new(move |tuple: &C::Tuple| {
        nullable::capture_sparql((|| {
            let value = operand.evaluate(tuple)?;
            if let SolutionValue::ReturnedError(error) = value {
                return Err(EvalFailure::Sparql(error));
            }
            let text = session
                .source_text(&value)
                .map_err(|e| EvalFailure::Fatal(failure(e)))?;
            let result = session
                .construct_source_plain_result(text.into_owned())
                .map_err(|e| EvalFailure::Fatal(failure(e)))?;
            Ok(SolutionValue::Rdf(result))
        })())
    });
    Ok(CompiledExpression {
        legacy: None,
        raw: None,
        numeric: Some(evaluator),
        raw_numeric_result: false,
    })
}

pub(super) fn build_datatype<'a, C: ExpressionEvaluatorContext<'a>>(
    parameters: &[Expression],
    syntax: &spargebra::SourceSyntaxSnapshot,
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    let node = syntax.root_node().ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
    if node.name != "Builtin_DATATYPE" || parameters.len() != 1
        || !node.fields.iter().any(|(key, _)| key == "arg")
    {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    let operand = build_numeric_arithmetic_operand(&parameters[0], context, hooks)?;
    let session = Rc::clone(&hooks.session);
    let failure = hooks.failure;
    Ok(CompiledExpression {
        legacy: None,
        raw: None,
        numeric: Some(Rc::new(move |tuple| {
            nullable::capture_sparql((|| {
                let value = operand.evaluate(tuple)?;
                session.source_datatype(&value)
                    .map(SolutionValue::Rdf)
                    .map_err(|error| numeric_evaluation_failure(error, failure))
            })())
        })),
        raw_numeric_result: false,
    })
}

pub(super) fn build<'a, C: ExpressionEvaluatorContext<'a>>(
    iri: &NamedNode,
    parameters: &[Expression],
    syntax: &spargebra::SourceSyntaxSnapshot,
    context: &mut C,
    hooks: &NumericExpressionHooks<'a, C::Term, C::Error>,
) -> Result<CompiledExpression<'a, C::Tuple, C::Error>, ExpressionEvaluationError<C::Error>> {
    use spargebra::SourceSyntaxValue as V;
    let node = syntax
        .root_node()
        .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
    if node.name != "Function"
        || !node
            .fields
            .iter()
            .any(|(key, value)| key == "iri" && matches!(value,V::NamedNode(value) if value==iri))
    {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    let has_arguments = match node.fields.iter().find(|(key, _)| key == "expr") {
        Some((_, V::List(values))) if values.len() == parameters.len() => true,
        None if parameters.is_empty() => false,
        _ => return Err(ExpressionEvaluationError::UnsupportedNullableExpression),
    };
    let registry = Rc::clone(
        hooks
            .session
            .source_functions()
            .ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?,
    );
    // Compilation may discover an unreceived operand, but raw or unknown
    // callbacks need not evaluate it. Only actual argument access reports it.
    let operands = parameters
        .iter()
        .map(|value| build_numeric_arithmetic_operand(value, context, hooks))
        .collect::<Vec<_>>();
    let bindings = context.build_source_function_bindings();
    let externalize = Rc::clone(&hooks.externalize);
    let session = Rc::clone(&hooks.session);
    let failure = hooks.failure;
    let syntax = syntax.clone();
    let iri = iri.clone();
    let source_iri = RawExpressionValue::from_import(iri.clone().into()).with_new_source_object();
    let evaluator = Rc::new(move |tuple: &C::Tuple| {
        let host = RefCell::new(None);
        #[cfg(test)]
        observation::call(&iri, &syntax);
        let arguments = || {
            if !has_arguments {
                return Ok(None);
            }
            let mut values = Vec::new();
            for operand in &operands {
                let operand = operand.as_ref().map_err(|_| {
                    observed_failure::<C>(
                        EvalFailure::Fatal(failure(
                            RawNumericFault::UnsupportedSourceLiteralOperator(
                                "source callback argument compilation",
                            ),
                        )),
                        &host,
                    )
                })?;
                values.push(
                    operand
                        .evaluate(tuple)
                        .map_err(|error| observed_failure::<C>(error, &host))?,
                );
            }
            Ok(Some(values))
        };
        let binding = |variable: &Variable| {
            let bindings = bindings.as_ref().ok_or_else(|| {
                observed_failure::<C>(
                    EvalFailure::Fatal(failure(RawNumericFault::InvalidSourceOrigin)),
                    &host,
                )
            })?;
            let value = bindings(tuple, variable)
                .map_err(|error| observed_failure::<C>(EvalFailure::Fatal(error), &host))?
                .ok_or_else(|| {
                    SourceFunctionFailure::Exception(SourceFunctionException {
                        kind: SourceFunctionExceptionKind::KeyError,
                        arguments: vec![SourceFunctionArgument::Variable(variable.clone())],
                    })
                })?;
            match value {
                SolutionValue::Rdf(value) => externalize(value)
                    .map(SolutionValue::Rdf)
                    .map_err(|error| observed_failure::<C>(EvalFailure::Fatal(error), &host)),
                SolutionValue::ExplicitNone => Ok(SolutionValue::ExplicitNone),
                SolutionValue::ReturnedError(error) => Ok(SolutionValue::ReturnedError(error)),
            }
        };
        let view = CallView {
            syntax: &syntax,
            iri: &source_iri,
            session: &session,
            read_arguments: &arguments,
            read_binding: &binding,
        };
        // The cloned entry is detached from the RefCell borrow before callbacks.
        let result = if let Some(entry) = registry.lookup(&iri) {
            if entry.raw {
                invoke_raw(&entry.callable, &view)
            } else {
                let result = (|| {
                    let values = arguments()?.ok_or_else(|| {
                        SourceFunctionFailure::Exception(SourceFunctionException::text(
                            SourceFunctionExceptionKind::TypeError,
                            format!(
                                "{}() argument after * must be an iterable, not NoneType",
                                entry.callable.qualified_name()
                            ),
                        ))
                    })?;
                    invoke_arguments(&entry.callable, &values, &session)
                })();
                match result {
                    Err(SourceFunctionFailure::Exception(error)) if error.kind.is_type_error() => {
                        #[cfg(test)]
                        observation::raised(&iri, &SourceFunctionFailure::Exception(error.clone()));
                        Err(convert_nonraw_type_error(error))
                    }
                    result => result,
                }
            }
        } else {
            Err(match session.source_unknown_function(&iri) {
                Ok(error) => numeric_failure(error),
                Err(error) => SourceFunctionFailure::Unreceived(error),
            })
        };
        if let Some(error) = host.into_inner() {
            return Err(EvalFailure::Fatal(error));
        }
        #[cfg(test)]
        match &result {
            Ok(value) => observation::returned(&iri, value),
            Err(error) => observation::raised(&iri, error),
        }
        nullable::capture_sparql(match result {
            Ok(value) => Ok(value),
            Err(SourceFunctionFailure::Sparql(error)) => Err(EvalFailure::Sparql(error)),
            Err(SourceFunctionFailure::Exception(error)) => Err(EvalFailure::Fatal(failure(
                RawNumericFault::SourceFunctionException(error),
            ))),
            Err(SourceFunctionFailure::Unreceived(error)) => {
                Err(EvalFailure::Fatal(failure(error)))
            }
            Err(SourceFunctionFailure::Host) => Err(EvalFailure::Fatal(failure(
                RawNumericFault::InvalidSourceOrigin,
            ))),
        })
    });
    Ok(CompiledExpression {
        legacy: None,
        raw: None,
        numeric: Some(evaluator),
        raw_numeric_result: false,
    })
}

fn invoke_raw(callable: &SourceCallable, view: &dyn SourceFunctionContext) -> SourceFunctionResult {
    match callable {
        SourceCallable::Native { callback, .. } => callback(SourceFunctionInvocation::Raw(view)),
        SourceCallable::DefaultCast => {
            if view.arguments()?.is_none_or(|args| args.is_empty()) {
                return Err(numeric_failure(source_function_error(
                    "Nothing given to cast.",
                )));
            }
            if view.arguments()?.is_some_and(|args| args.len() > 1) {
                return Err(numeric_failure(source_function_error(
                    "Cannot cast more than one thing!",
                )));
            }
            let args = view.arguments()?.ok_or(SourceFunctionFailure::Unreceived(
                RawNumericFault::InvalidSourceOrigin,
            ))?;
            let iri = view.iri();
            let Ok(Term::NamedNode(iri)) = iri.rdf() else {
                return Err(SourceFunctionFailure::Unreceived(
                    RawNumericFault::InvalidSourceOrigin,
                ));
            };
            view.session()
                .source_default_cast_for_iri(iri, &args[0])
                .map(|value| value.map_or(SolutionValue::ExplicitNone, SolutionValue::Rdf))
                .map_err(numeric_failure)
        }
    }
}
fn invoke_arguments(
    callable: &SourceCallable,
    values: &[SolutionValue<RawExpressionValue>],
    session: &QueryNumericSession,
) -> SourceFunctionResult {
    match callable {
        SourceCallable::Native { callback, .. } => {
            callback(SourceFunctionInvocation::Arguments { values, session })
        }
        SourceCallable::DefaultCast => Err(SourceFunctionFailure::Exception(
            SourceFunctionException::text(
                SourceFunctionExceptionKind::TypeError,
                match values.len() {
                    0 => "default_cast() missing 2 required positional arguments: 'e' and 'ctx'"
                        .to_owned(),
                    1 => "default_cast() missing 1 required positional argument: 'ctx'".to_owned(),
                    // Two arguments enter default_cast's object protocol. That
                    // protocol is not replaced with a guessed exception here.
                    2 => {
                        return Err(SourceFunctionFailure::Unreceived(
                            RawNumericFault::UnsupportedSourceLiteralOperator(
                                "nonraw default_cast object protocol",
                            ),
                        ));
                    }
                    count => format!(
                        "default_cast() takes 2 positional arguments but {count} were given"
                    ),
                },
            ),
        )),
    }
}

#[cfg(test)]
pub(crate) mod observation {
    use super::*;
    #[derive(Clone, Debug)]
    pub(crate) enum Event {
        Call(NamedNode, spargebra::SourceSyntaxSnapshot),
        Return(NamedNode, SolutionValue<RawExpressionValue>),
        Raised(NamedNode, SourceFunctionFailure),
    }
    thread_local! { static EVENTS:RefCell<Option<Vec<Event>>>=const{RefCell::new(None)}; }
    pub(crate) struct Guard;
    impl Guard {
        pub(crate) fn start() -> Self {
            EVENTS.with_borrow_mut(|events| {
                assert!(events.is_none());
                *events = Some(Vec::new());
            });
            Self
        }
        pub(crate) fn take(&self) -> Vec<Event> {
            EVENTS.with_borrow_mut(|events| std::mem::take(events.as_mut().unwrap()))
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            EVENTS.with_borrow_mut(|events| *events = None);
        }
    }
    pub(super) fn call(iri: &NamedNode, syntax: &spargebra::SourceSyntaxSnapshot) {
        EVENTS.with_borrow_mut(|events| {
            if let Some(events) = events {
                events.push(Event::Call(iri.clone(), syntax.clone()));
            }
        });
    }
    pub(super) fn returned(iri: &NamedNode, value: &SolutionValue<RawExpressionValue>) {
        EVENTS.with_borrow_mut(|events| {
            if let Some(events) = events {
                events.push(Event::Return(iri.clone(), value.clone()));
            }
        });
    }
    pub(super) fn raised(iri: &NamedNode, value: &SourceFunctionFailure) {
        EVENTS.with_borrow_mut(|events| {
            if let Some(events) = events {
                events.push(Event::Raised(iri.clone(), value.clone()));
            }
        });
    }
}

/// Original operators.Builtin_isLITERAL; the source operand owner handles
/// binding/error access and the source Boolean constructor owns the result.
pub(super) fn build_is_literal<'a,C:ExpressionEvaluatorContext<'a>>(
    parameters:&[Expression],syntax:&spargebra::SourceSyntaxSnapshot,context:&mut C,
    hooks:&NumericExpressionHooks<'a,C::Term,C::Error>,
)->Result<CompiledExpression<'a,C::Tuple,C::Error>,ExpressionEvaluationError<C::Error>> {
    let node=syntax.root_node().ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
    if !hooks.session.admits_source_calendar_duration() || node.name!="Builtin_isLITERAL" || parameters.len()!=1 || !node.fields.iter().any(|(key,_)|key=="arg") {
        return Err(ExpressionEvaluationError::UnsupportedNullableExpression);
    }
    let operand=build_numeric_arithmetic_operand(&parameters[0],context,hooks)?;
    let session=Rc::clone(&hooks.session);let failure=hooks.failure;
    Ok(CompiledExpression {legacy:None,raw:None,raw_numeric_result:false,numeric:Some(Rc::new(move |tuple| {
        nullable::capture_sparql((|| {
            let value=operand.evaluate(tuple)?;
            if let SolutionValue::ReturnedError(error)=value {return Err(EvalFailure::Sparql(error))}
            let is_literal=matches!(&value,SolutionValue::Rdf(raw) if raw.literal().is_some());
            session.construct_source_boolean_result(is_literal).map(SolutionValue::Rdf).map_err(|e|EvalFailure::Fatal(failure(e)))
        })())
    }))})
}

pub(super) fn build_node_unary<'a,C:ExpressionEvaluatorContext<'a>>(
    operation:crate::numeric::source_function::SourceNodeUnary, name:&str,
    parameters:&[Expression], syntax:&spargebra::SourceSyntaxSnapshot, context:&mut C,
    hooks:&NumericExpressionHooks<'a,C::Term,C::Error>,
)->Result<CompiledExpression<'a,C::Tuple,C::Error>,ExpressionEvaluationError<C::Error>> {
    let node=syntax.root_node().ok_or(ExpressionEvaluationError::UnsupportedNullableExpression)?;
    if (node.name!=name && !(matches!(operation,crate::numeric::source_function::SourceNodeUnary::IsIri) && node.name=="Builtin_isURI")) || parameters.len()!=1 || !node.fields.iter().any(|(key,_)|key=="arg") { return Err(ExpressionEvaluationError::UnsupportedNullableExpression); }
    let operand=build_numeric_arithmetic_operand(&parameters[0],context,hooks)?;
    let session=Rc::clone(&hooks.session);let failure=hooks.failure;
    Ok(CompiledExpression {legacy:None,raw:None,raw_numeric_result:false,numeric:Some(Rc::new(move |tuple| {
        nullable::capture_sparql((|| {
            let value=operand.evaluate(tuple)?;
            if let SolutionValue::ReturnedError(error)=value {return Err(EvalFailure::Sparql(error))}
            session.source_node_unary(operation,&value).map(SolutionValue::Rdf).map_err(|e|numeric_evaluation_failure(e,failure))
        })())
    }))})
}
