//! Internal, feature-gated bridge for an owner's verified source projection.
//! These inputs assert source facts; ordinary normalized RDF is not admission.
use super::numeric_query;
use crate::QueryEvaluationError;
use crate::dataset::source_memory::SourceMemoryGraph;
use crate::numeric::{
    NumericInitialBinding, NumericInitialValue, QueryContextSemantics, QueryGroupSemantics,
    QueryLiteralSemantics, QueryNumericCapabilities, QueryNumericSession, RawExpressionValue,
    SourceHashPolicy, SourceHashRuntime, source_registry::SourceFunctionRegistry,
};
use oxrdf::{BlankNode, NamedNode, Variable};
use semantica_numeric::{DecimalContext, NumericLimits};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Borrowed facts from an admitted source owner, never inferred from RDF1.1.
#[derive(Clone, Copy, Debug)]
pub enum SourceTermInput<'a> {
    Iri(&'a str),
    Blank(&'a str),
    /// Already constructed original Variable name, never a public RDFTermDTO.
    Variable(&'a str),
    Literal {
        value: &'a str,
        datatype: Option<&'a str>,
        language: Option<&'a str>,
    },
}
impl<'a> SourceTermInput<'a> {
    /// Original wrapper cache identity, before literal construction/normalization.
    pub fn cache_parts(self) -> (&'static str, &'a str, Option<&'a str>, Option<&'a str>) {
        match self {
            Self::Iri(v) => ("iri", v, None, None),
            Self::Blank(v) => ("blank_node", v, None, None),
            Self::Variable(v) => ("variable", v, None, None),
            Self::Literal {
                value,
                datatype,
                language,
            } => ("literal", value, datatype, language),
        }
    }
    pub(super) fn initial(self, normalize: bool) -> Result<NumericInitialValue, QueryEvaluationError> {
        Ok(match self {
            Self::Variable(v) => NumericInitialValue::SourceVariable(Variable::new_unchecked(v)),
            Self::Iri(v) => {
                NumericInitialValue::UnknownNormalized(NamedNode::new(v).map_err(error)?.into())
            }
            Self::Blank(v) => {
                NumericInitialValue::UnknownNormalized(BlankNode::new(v).map_err(error)?.into())
            }
            Self::Literal {
                value,
                datatype,
                language,
            } => {
                if datatype.is_some() && language.is_some() {
                    return Err(error(
                        "an RDF literal cannot have both datatype and language",
                    ));
                }
                NumericInitialValue::SourceDirectString {
                    lexical: value.into(),
                    datatype: datatype.map(NamedNode::new).transpose().map_err(error)?,
                    language: language.map(str::to_owned),
                    normalize,
                }
            }
        })
    }
}
fn error(e: impl std::fmt::Display) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(e.to_string().into())
}

pub(super) fn source_session() -> Result<Rc<QueryNumericSession>, QueryEvaluationError> {
    source_session_with_objects(None)
}
pub(super) fn source_session_with_objects(objects: Option<&crate::numeric::SourceDatatypeObjects>) -> Result<Rc<QueryNumericSession>, QueryEvaluationError> {
    let numeric = QueryNumericSession::new_with_context_semantics(
        DecimalContext::default(),
        NumericLimits::default(),
        None,
        true,
        QueryNumericCapabilities::MixedBinary64V1,
        QueryGroupSemantics::SourceAggregateV1,
        QueryLiteralSemantics::SourceLiteralV1,
        QueryContextSemantics::SourceContextV1,
        Some(Rc::new(
            SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).map_err(error)?,
        )),
    )
    .map_err(error)?
    .with_source_functions(Rc::new(SourceFunctionRegistry::stock()))
    .map_err(error)?
    .with_source_time_comparisons_v1()
    .map_err(error)?
    .with_source_calendar_duration_v1()
    .map_err(error)?;
    let numeric = if let Some(objects) = objects { numeric.with_source_datatype_objects(objects) } else { numeric };
    Ok(Rc::new(numeric))
}

/// Execute through the existing source graph/parser/evaluator/result owners.
/// This internal source profile fixes the previously received hash seed to 0;
/// it is not a public protocol or an assertion about arbitrary Python processes.
pub fn execute_source_default_graph_internal<'a>(
    query: &str,
    triples: impl IntoIterator<Item = [SourceTermInput<'a>; 3]>,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, SourceTermInput<'_>>,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<Value, QueryEvaluationError> {
    if cancelled
        .as_ref()
        .is_some_and(|c| c.load(Ordering::Relaxed))
    {
        return Err(QueryEvaluationError::Cancelled);
    }
    if query.len() > 1024 * 1024 || namespaces.len() > 128 || bindings.len() > 128 {
        return Err(error("source_query_parameter_budget"));
    }
    let numeric = source_session()?;
    let mut graph =
        SourceMemoryGraph::new(Rc::clone(&numeric), cancelled.clone()).map_err(error)?;
    let mut input_bytes = 0usize;
    for (ordinal, triple) in triples.into_iter().enumerate() {
        if ordinal >= 100_000 {
            return Err(error("source_graph_triple_budget"));
        }
        if cancelled
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return Err(QueryEvaluationError::Cancelled);
        }
        let mut terms = Vec::with_capacity(3);
        for value in triple {
            let (_, text, datatype, language) = value.cache_parts();
            input_bytes = input_bytes
                .checked_add(text.len())
                .and_then(|v| v.checked_add(datatype.map_or(0, str::len)))
                .and_then(|v| v.checked_add(language.map_or(0, str::len)))
                .filter(|v| *v <= 16 * 1024 * 1024)
                .ok_or_else(|| error("source_graph_byte_budget"))?;
            // The projection already reflects the actual load's normalization.
            let raw: RawExpressionValue = numeric
                .construct_source_graph_initial(value.initial(false)?)
                .map_err(error)?;
            terms.push(graph.admit_source(raw).map_err(error)?);
        }
        let mut it = terms.into_iter();
        graph
            .insert([it.next().unwrap(), it.next().unwrap(), it.next().unwrap()])
            .map_err(error)?;
    }
    let initial = bindings
        .iter()
        .map(|(name, value)| {
            Ok(NumericInitialBinding {
                variable: Variable::new(name).map_err(error)?,
                value: value.initial(true)?,
            })
        })
        .collect::<Result<Vec<_>, QueryEvaluationError>>()?;
    numeric_query::prepare_source_runtime_query_with_options(
        query, graph, numeric, initial, namespaces,
    )
    .map_err(error)?
    .execute_source_runtime_result()
}
