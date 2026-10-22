//! Explicit source-callable scope over the existing query session.
//! RDFLib operators.py derivation; BSD terms in docs/RDFLIB-SNAPSHOT-LICENSE.md.
use super::*;
use oxrdf::{NamedNode, Variable};
use std::cell::RefCell;

// Preserve the existing callback boundary names; their constructor owner is
// now shared with source arithmetic and subsequent qualified language owners.
pub(crate) use super::source_exception::{
    LanguageExceptionArgument as SourceFunctionArgument,
    SourceLanguageException as SourceFunctionException,
    SourceLanguageExceptionKind as SourceFunctionExceptionKind,
};

#[derive(Clone, Debug)]
pub(crate) enum SourceFunctionFailure {
    Sparql(ErrorValue),
    Exception(SourceFunctionException),
    Unreceived(RawNumericFault),
    // The adapter retains the actual host error in its call-local slot. A
    // callback cannot replace or swallow it by returning a different result.
    Host,
}
pub(crate) type SourceFunctionResult =
    Result<SolutionValue<RawExpressionValue>, SourceFunctionFailure>;

pub(crate) trait SourceFunctionContext {
    fn syntax(&self) -> &spargebra::SourceSyntaxSnapshot;
    fn iri(&self) -> RawExpressionValue;
    fn arguments(
        &self,
    ) -> Result<Option<Vec<SolutionValue<RawExpressionValue>>>, SourceFunctionFailure>;
    fn binding(&self, variable: &Variable) -> SourceFunctionResult;
    fn session(&self) -> &QueryNumericSession;
}
pub(crate) enum SourceFunctionInvocation<'a> {
    Raw(&'a dyn SourceFunctionContext),
    Arguments {
        values: &'a [SolutionValue<RawExpressionValue>],
        session: &'a QueryNumericSession,
    },
}
pub(crate) type SourceFunctionCallback =
    Rc<dyn for<'a> Fn(SourceFunctionInvocation<'a>) -> SourceFunctionResult>;

#[derive(Clone)]
pub(crate) enum SourceCallable {
    DefaultCast,
    Native {
        qualified_name: Rc<str>,
        callback: SourceFunctionCallback,
    },
}
impl fmt::Debug for SourceCallable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SourceCallable")
            .field(&self.qualified_name())
            .finish()
    }
}
impl SourceCallable {
    pub(crate) fn qualified_name(&self) -> &str {
        match self {
            Self::DefaultCast => "rdflib.plugins.sparql.operators.default_cast",
            Self::Native { qualified_name, .. } => qualified_name,
        }
    }
    pub(crate) fn same_callable(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::DefaultCast, Self::DefaultCast) => true,
            (Self::Native { callback: a, .. }, Self::Native { callback: b, .. }) => {
                Rc::ptr_eq(a, b)
            }
            _ => false,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct SourceFunctionDefinition {
    pub(crate) callable: SourceCallable,
    pub(crate) raw: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceFunctionWarning {
    pub(crate) category: &'static str,
    pub(crate) message: String,
}

/// Explicitly share this handle where original calls share a registry. It
/// grants neither graph-source admission nor engineering adoption authority.
#[derive(Debug)]
pub(crate) struct SourceFunctionRegistry {
    // Replacement retains insertion position; deletion/reinsertion does not.
    entries: RefCell<Vec<(NamedNode, SourceFunctionDefinition)>>,
}
impl SourceFunctionRegistry {
    pub(crate) fn stock() -> Self {
        use oxrdf::vocab::xsd;
        Self {
            entries: RefCell::new(
                [
                    xsd::BOOLEAN,
                    xsd::INTEGER,
                    xsd::DECIMAL,
                    xsd::DOUBLE,
                    xsd::FLOAT,
                    xsd::DATE_TIME,
                    xsd::STRING,
                ]
                .into_iter()
                .map(|iri| {
                    (
                        iri.into_owned(),
                        SourceFunctionDefinition {
                            callable: SourceCallable::DefaultCast,
                            raw: true,
                        },
                    )
                })
                .collect(),
            ),
        }
    }
    pub(crate) fn lookup(&self, iri: &NamedNode) -> Option<SourceFunctionDefinition> {
        self.entries
            .borrow()
            .iter()
            .find(|(key, _)| key == iri)
            .map(|(_, value)| value.clone())
    }
    pub(crate) fn entries(&self) -> Vec<(NamedNode, SourceFunctionDefinition)> {
        self.entries.borrow().clone()
    }
    pub(crate) fn register(
        &self,
        iri: NamedNode,
        definition: SourceFunctionDefinition,
        override_existing: bool,
    ) -> Result<(), SourceFunctionException> {
        let mut entries = self.entries.borrow_mut();
        if let Some((_, value)) = entries.iter_mut().find(|(key, _)| key == &iri) {
            if !override_existing {
                return Err(SourceFunctionException::text(
                    SourceFunctionExceptionKind::ValueError,
                    format!("A function is already registered as {iri}"),
                ));
            }
            *value = definition;
        } else {
            entries.push((iri, definition));
        }
        Ok(())
    }
    pub(crate) fn decorate(
        &self,
        iri: NamedNode,
        definition: SourceFunctionDefinition,
        override_existing: bool,
    ) -> Result<SourceCallable, SourceFunctionException> {
        let callable = definition.callable.clone();
        self.register(iri, definition, override_existing)?;
        Ok(callable)
    }
    pub(crate) fn unregister(
        &self,
        iri: &NamedNode,
        _compatibility_callable: Option<&SourceCallable>,
    ) -> Option<SourceFunctionWarning> {
        let mut entries = self.entries.borrow_mut();
        if let Some(at) = entries.iter().position(|(key, _)| key == iri) {
            entries.remove(at);
            None
        } else {
            Some(SourceFunctionWarning {
                category: "UserWarning",
                message: format!("This function is not registered as {iri}"),
            })
        }
    }
}
impl QueryNumericSession {
    pub(crate) fn with_source_functions(
        mut self,
        registry: Rc<SourceFunctionRegistry>,
    ) -> Result<Self, RawNumericFault> {
        if self.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1
            || self.context_semantics() != QueryContextSemantics::SourceContextV1
        {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        self.source_functions = Some(registry);
        Ok(self)
    }
    pub(crate) fn source_functions(&self) -> Option<&Rc<SourceFunctionRegistry>> {
        self.source_functions.as_ref()
    }
}

/// Function's non-raw TypeError handler calls the real one-argument
/// SPARQLError constructor, including its failure for multiple arguments.
pub(crate) fn convert_nonraw_type_error(error: SourceFunctionException) -> SourceFunctionFailure {
    if !error.kind.is_type_error() {
        return SourceFunctionFailure::Exception(error);
    }
    let argument = match error.arguments.as_slice() {
        [] | [SourceFunctionArgument::None] => SourceExceptionArgument::None,
        [SourceFunctionArgument::Text(value)] => SourceExceptionArgument::Text(value.clone()),
        [SourceFunctionArgument::DecimalSignalClasses(signals)] => {
            SourceExceptionArgument::DecimalSignalClasses(signals.clone())
        }
        [_] => {
            return SourceFunctionFailure::Unreceived(
                RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source SPARQLError object argument",
                ),
            );
        }
        values => {
            return SourceFunctionFailure::Exception(SourceFunctionException::text(
                SourceFunctionExceptionKind::TypeError,
                format!(
                    "SPARQLError.__init__() takes from 1 to 2 positional arguments but {} were given",
                    values.len() + 1
                ),
            ));
        }
    };
    let source = SourceException {
        class: SourceExceptionClass::Sparql,
        argument,
    };
    let diagnostic = super::source_text::exception_text(&source).into_owned();
    SourceFunctionFailure::Sparql(ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        &diagnostic,
        source,
    ))
}
