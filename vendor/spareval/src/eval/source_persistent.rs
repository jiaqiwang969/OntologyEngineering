//! Same source store across runtime calls. Only immutable values and the exact
//! table/index state cross a thread boundary; parsing/evaluation remain local.
use super::{
    numeric_query,
    source_bridge::{SourceTermInput, source_session_with_objects},
};
use crate::{
    QueryEvaluationError,
    dataset::{
        QueryableDataset,
        source_memory::{SourceMemoryGraph, SourceMemoryState},
    },
    numeric::{
        LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
        RawExpressionValue, RawNumericFault, SourceDatatypeObjects, SourceFact,
        SourceLiteralMetadata, SourceTermKey,
    },
};
use oxrdf::{BlankNode, Literal, NamedNode, Term, Variable};
use serde_json::Value;
use spargebra::SourceDatatype;
use std::{
    collections::{BTreeMap, HashSet},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod lifecycle;
mod storage;
mod turtle;
pub use turtle::{SourceTurtleTerm, SourceTurtleView};
mod update;
pub use update::{SourceUpdateLoader,SourceUpdateError};
pub use lifecycle::SourceNodeObservation;

pub type SourceQuadInput<'a> = ([SourceTermInput<'a>; 3], Option<SourceTermInput<'a>>);
const DEFAULT: &str = "urn:x-rdflib:default";
fn error(e: impl std::fmt::Display) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(e.to_string().into())
}
fn graph_value(
    input: SourceTermInput<'_>,
    numeric: &crate::numeric::QueryNumericSession,
) -> Result<RawExpressionValue, QueryEvaluationError> {
    let initial = match input {
        SourceTermInput::Variable(v) => NumericInitialValue::SourceVariable(Variable::new_unchecked(v)),
        SourceTermInput::Iri(v) => {
            NumericInitialValue::UnknownNormalized(NamedNode::new_unchecked(v).into())
        }
        SourceTermInput::Blank(v) => {
            NumericInitialValue::UnknownNormalized(BlankNode::new_unchecked(v).into())
        }
        SourceTermInput::Literal {
            value,
            datatype,
            language,
        } => NumericInitialValue::SourceDirectString {
            lexical: value.into(),
            datatype: datatype.map(NamedNode::new_unchecked),
            language: language.map(str::to_owned),
            normalize: false,
        },
    };
    match numeric.construct_source_graph_initial(initial) {
        Ok(value) => Ok(value),
        // Original loading already receives this term's spelling and type;
        // its converter cache is not received. Preserve that exact unknown,
        // and preserve the existing query refusal until the converter lands.
        Err(RawNumericFault::UnsupportedSourceLiteralOperator(
            "source string datatype constructor",
        )) => {
            let SourceTermInput::Literal {
                value,
                datatype: Some(dtype),
                language: None,
            } = input
            else {
                return Err(error("source_key_observation_invalid"));
            };
            let datatype = NamedNode::new_unchecked(dtype);
            RawExpressionValue::from_source_parts(
                Literal::new_typed_literal(value, datatype.clone()).into(),
                NumericCacheState::UnknownNormalized,
                LiteralSourceState::Literal(SourceLiteralMetadata {
                    datatype: SourceDatatype::Specified(datatype),
                    language_original: SourceFact::Known(None),
                    ill_typed: SourceFact::Unknown,
                    cached_value_kind: SourceFact::Unknown,
                }),
            )
            .map_err(error)
        }
        Err(e) => Err(error(e)),
    }
}

/// This state is only created by the admitted Rust source owner. Its fields
/// are private; no JSON, RDF deserializer or supplied identifier grants access.
#[derive(Clone, Default)]
pub struct SourceDatasetState {
    memory: Option<SourceMemoryState>,
    datatype_objects: SourceDatatypeObjects,
}
impl SourceDatasetState {
    fn graph(
        &self,
        cancelled: Option<Arc<AtomicBool>>,
    ) -> Result<SourceMemoryGraph, QueryEvaluationError> {
        let numeric = source_session_with_objects(Some(&self.datatype_objects))?;
        match &self.memory {
            Some(state) => {
                SourceMemoryGraph::from_state(state.clone(), numeric, cancelled).map_err(error)
            }
            None => SourceMemoryGraph::new_dataset(
                numeric,
                cancelled,
                NamedNode::new_unchecked(DEFAULT).into(),
            )
            .map_err(error),
        }
    }
    fn commit(
        &mut self,
        graph: SourceMemoryGraph,
        max_quads: usize,
        max_bytes: usize,
    ) -> Result<(), QueryEvaluationError> {
        let mut quads = 0usize;
        for name in graph.context_names().map_err(error)? {
            quads = quads
                .checked_add(graph.len_context(Some(&name)).map_err(error)?)
                .ok_or_else(|| error("source_dataset_quad_budget"))?;
        }
        if quads > max_quads {
            return Err(error("source_dataset_quad_budget"));
        }
        let (slots, bytes) = graph.retained_work().map_err(error)?;
        if bytes > max_bytes || slots > max_quads.saturating_mul(64).saturating_add(64) {
            return Err(error("source_persistent_retained_state_budget"));
        }
        self.memory = Some(graph.into_state());
        Ok(())
    }
    /// The original wrapper parses into a temporary Graph/Dataset, enumerates
    /// its actual source set, then appends those original objects to live state.
    /// ConstructDTO instead appends its ordered triples directly.
    pub fn load_parsed<'a>(
        &mut self,
        rows: impl IntoIterator<Item = SourceQuadInput<'a>>,
        staged_graph: bool,
        max_quads: usize,
        max_bytes: usize,
    ) -> Result<(), QueryEvaluationError> {
        let mut graph = self.graph(None)?;
        let numeric = Rc::clone(graph.session());
        let mut temporary = SourceMemoryGraph::new_dataset(
            Rc::clone(&numeric),
            None,
            NamedNode::new_unchecked(DEFAULT).into(),
        )
        .map_err(error)?;
        let target = if staged_graph {
            &mut temporary
        } else {
            &mut graph
        };
        for (triple, name) in rows {
            let mut values = Vec::with_capacity(3);
            for input in triple {
                values.push(
                    target
                        .admit_recorded_source(graph_value(input, &numeric)?)
                        .map_err(error)?,
                );
            }
            let name = name.unwrap_or(SourceTermInput::Iri(DEFAULT));
            let name = target
                .admit_recorded_source(graph_value(name, &numeric)?)
                .map_err(error)?;
            target
                .insert_context(&name, values.try_into().expect("three source inputs"))
                .map_err(error)?;
        }
        if staged_graph {
            for triple in temporary
                .scan_context(None, [None, None, None])
                .map_err(error)?
            {
                for name in temporary.triple_contexts(&triple).map_err(error)? {
                    let name = graph
                        .admit_recorded_source(temporary.externalize_source(name).map_err(error)?)
                        .map_err(error)?;
                    let mut values = Vec::with_capacity(3);
                    for term in &triple {
                        values.push(
                            graph
                                .admit_recorded_source(
                                    temporary.externalize_source(term.clone()).map_err(error)?,
                                )
                                .map_err(error)?,
                        );
                    }
                    graph
                        .insert_context(&name, values.try_into().expect("three source terms"))
                        .map_err(error)?;
                }
            }
        }
        self.commit(graph, max_quads, max_bytes)
    }
    /// Wrapper clear iterates actual registered contexts (plus default), rather
    /// than discarding the store or performing the UPDATE CLEAR-union action.
    pub fn clear(
        &mut self,
        name: Option<&str>,
        max_quads: usize,
        max_bytes: usize,
    ) -> Result<(), QueryEvaluationError> {
        let mut graph = self.graph(None)?;
        if let Some(name) = name {
            let name = graph
                .internalize_term(NamedNode::new(name).map_err(error)?.into())
                .map_err(error)?;
            graph.register_context(&name).map_err(error)?;
            graph
                .remove_context(Some(&name), [None, None, None])
                .map_err(error)?;
        } else {
            let mut names = graph.context_names().map_err(error)?;
            let default = graph
                .internalize_term(NamedNode::new_unchecked(DEFAULT).into())
                .map_err(error)?;
            if !names.contains(&default) {
                names.push(default);
            }
            for name in names {
                graph
                    .remove_context(Some(&name), [None, None, None])
                    .map_err(error)?;
            }
        }
        self.commit(graph, max_quads, max_bytes)
    }
    /// Existing bounded source DELETE DATA contract. Full UPDATE uses its own
    /// prepared operations and partial-mutation/cache phases, still required.
    pub fn remove_quads<'a>(
        &mut self,
        rows: impl IntoIterator<Item = SourceQuadInput<'a>>,
        max_quads: usize,
        max_bytes: usize,
    ) -> Result<(), QueryEvaluationError> {
        let mut graph = self.graph(None)?;
        let numeric = Rc::clone(graph.session());
        for (triple, name) in rows {
            let mut values = Vec::new();
            for v in triple {
                values.push(Some(
                    graph
                        .admit_recorded_source(graph_value(v, &numeric)?)
                        .map_err(error)?,
                ));
            }
            let name = name.unwrap_or(SourceTermInput::Iri(DEFAULT));
            let name = graph
                .admit_recorded_source(graph_value(name, &numeric)?)
                .map_err(error)?;
            graph
                .remove_context(Some(&name), values.try_into().expect("three source inputs"))
                .map_err(error)?;
        }
        self.commit(graph, max_quads, max_bytes)
    }
    /// Check current source membership before cache use. This only compares
    /// keys against retained objects; it never constructs object provenance.
    pub fn matches_projection<'a>(
        &self,
        rows: impl IntoIterator<Item = SourceQuadInput<'a>>,
    ) -> Result<bool, QueryEvaluationError> {
        fn key(v: SourceTermInput<'_>) -> SourceTermKey {
            match v {
                SourceTermInput::Variable(v) => SourceTermKey::Variable(Variable::new_unchecked(v)),
                SourceTermInput::Iri(v) => SourceTermKey::NamedNode(NamedNode::new_unchecked(v)),
                SourceTermInput::Blank(v) => SourceTermKey::BlankNode(BlankNode::new_unchecked(v)),
                SourceTermInput::Literal {
                    value,
                    datatype,
                    language,
                } => SourceTermKey::Literal {
                    lexical: value.into(),
                    datatype: datatype.map(NamedNode::new_unchecked),
                    language_lower: language.map(str::to_ascii_lowercase),
                },
            }
        }
        let expected: HashSet<_> = rows
            .into_iter()
            .map(|(t, g)| (t.map(key), key(g.unwrap_or(SourceTermInput::Iri(DEFAULT)))))
            .collect();
        let graph = self.graph(None)?;
        let mut actual = HashSet::new();
        for triple in graph
            .scan_context(None, [None, None, None])
            .map_err(error)?
        {
            let terms = triple.each_ref().map(|v| {
                crate::numeric::source_identity_key(
                    &graph.externalize_source(v.clone()).expect("same owner"),
                )
                .expect("admitted source key")
            });
            for name in graph.triple_contexts(&triple).map_err(error)? {
                actual.insert((
                    terms.clone(),
                    crate::numeric::source_identity_key(
                        &graph.externalize_source(name).map_err(error)?,
                    )
                    .map_err(error)?,
                ));
            }
        }
        Ok(actual == expected)
    }
    pub fn query(
        &self,
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, SourceTermInput<'_>>,
        cancelled: Option<Arc<AtomicBool>>,
    ) -> Result<Value, QueryEvaluationError> {
        self.query_observed(query, namespaces, bindings, cancelled, &mut false)
    }
    /// Expose actual constructor/backend entry, without publishing namespace
    /// effects before the original wrapper would enter Dataset.query.
    pub fn query_observed(
        &self,
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, SourceTermInput<'_>>,
        cancelled: Option<Arc<AtomicBool>>,
        backend_reached: &mut bool,
    ) -> Result<Value, QueryEvaluationError> {
        *backend_reached = false;
        if cancelled
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return Err(QueryEvaluationError::Cancelled);
        }
        if query.len() > 1024 * 1024 || namespaces.len() > 128 || bindings.len() > 128 {
            return Err(error("source_query_parameter_budget"));
        }
        let graph = self.graph(cancelled)?;
        let numeric = Rc::clone(graph.session());
        let initial = update::construct_initial_bindings(&numeric, bindings)
            .map_err(|e| e.error)?;
        *backend_reached = true;
        for triple in graph
            .scan_context(None, [None, None, None])
            .map_err(error)?
        {
            for term in triple {
                let raw = graph.externalize_source(term).map_err(error)?;
                match raw.rdf() {
                    // A source ConstructResultDTO can contain a URIRef whose
                    // text is not valid RDF syntax. RDFLib still queries that
                    // stored identity; URIRef.n3 owns the Turtle refusal.
                    // Native parser validation remains at its input boundary.
                    Ok(Term::NamedNode(_)) => (),
                    Ok(Term::BlankNode(v)) => {
                        BlankNode::new(v.as_str()).map_err(error)?;
                    }
                    _ => (),
                }
                if let LiteralSourceState::Literal(m) = raw.literal_source() {
                    if matches!(m.cached_value_kind, SourceFact::Unknown) {
                        return Err(error(RawNumericFault::UnsupportedSourceLiteralOperator(
                            "source string datatype constructor",
                        )));
                    }
                }
            }
        }
        numeric_query::prepare_source_runtime_query_with_constructed_options(
            query, graph, numeric, initial.initial, initial.values, namespaces,
        )
        .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))?
        .execute_source_runtime_result()
    }
}
#[cfg(test)]
mod receiving {
    use super::*;
    fn row(value: &str) -> SourceQuadInput<'_> {
        (
            [
                SourceTermInput::Iri("urn:s"),
                SourceTermInput::Iri("urn:p"),
                SourceTermInput::Literal {
                    value,
                    datatype: None,
                    language: None,
                },
            ],
            None,
        )
    }
    #[test]
    fn source_persistent_retained_budget_refusal_keeps_previous_state() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<SourceDatasetState>();
        send_sync::<RawExpressionValue>();
        let mut state = SourceDatasetState::default();
        state.load_parsed([row("old")], true, 100, 4096).unwrap();
        let before = state
            .query(
                "SELECT ?o WHERE {<urn:s> <urn:p> ?o}",
                &BTreeMap::new(),
                &BTreeMap::new(),
                None,
            )
            .unwrap();
        let error = state.load_parsed([row("new")], true, 100, 1).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("source_persistent_retained_state_budget")
        );
        assert!(state.matches_projection([row("old")]).unwrap());
        assert_eq!(
            state
                .query(
                    "SELECT ?o WHERE {<urn:s> <urn:p> ?o}",
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    None
                )
                .unwrap(),
            before
        );
    }
    #[test]
    fn source_persistent_unreceived_converter_is_stored_without_fact_upgrade() {
        let mut state = SourceDatasetState::default();
        let row = (
            [
                SourceTermInput::Iri("urn:s"),
                SourceTermInput::Iri("urn:p"),
                SourceTermInput::Literal {
                    value: "<b/>",
                    datatype: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral"),
                    language: None,
                },
            ],
            None,
        );
        state.load_parsed([row], true, 100, 4096).unwrap();
        assert!(state.matches_projection([row]).unwrap());
        let graph = state.graph(None).unwrap();
        let triples = graph.scan_context(None, [None, None, None]).unwrap();
        let raw = graph.externalize_source(triples[0][2].clone()).unwrap();
        let LiteralSourceState::Literal(meta) = raw.literal_source() else {
            panic!("literal expected")
        };
        assert_eq!(meta.cached_value_kind, SourceFact::Unknown);
        assert_eq!(meta.ill_typed, SourceFact::Unknown);
        assert!(graph.admit_source(raw).is_err());
        assert!(
            state
                .query("ASK {}", &BTreeMap::new(), &BTreeMap::new(), None)
                .unwrap_err()
                .to_string()
                .contains("source string datatype constructor")
        );
        state.clear(None, 100, 4096).unwrap();
        assert_eq!(
            state
                .query("ASK {}", &BTreeMap::new(), &BTreeMap::new(), None)
                .unwrap()["boolean"],
            true
        );
    }
}
