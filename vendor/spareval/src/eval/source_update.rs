//! Whole-request source preparation. Execution must use this exact registry
//! and source session; a caller-provided AST cannot acquire this provenance.
mod execution;
pub(super) use execution::PreparedUpdateBindings;
use super::numeric_query::NumericPrepareError;
use crate::QueryEvaluationError;
use crate::numeric::{PreparedSourceLiteralRegistry, QueryNumericSession, RawNumericFault};
use spargebra::{NumericQueryParseError, SourceUpdate, SparqlParser};
use std::{collections::BTreeMap, rc::Rc};

pub(super) struct PreparedSourceUpdate {
    pub(super) request: SourceUpdate,
    pub(super) registry: Rc<PreparedSourceLiteralRegistry>,
    // Explicit per-request input only; legacy parser-only injection stays separate.
    pub(super) update_blank_constructor: Option<spargebra::SourceBlankNodeConstructor>,
}

pub(super) fn prepare(
    text: &str,
    namespaces: &BTreeMap<String, String>,
    session: Rc<QueryNumericSession>,
) -> Result<PreparedSourceUpdate, NumericPrepareError> {
    prepare_with_constructor(text, namespaces, session, None)
}

pub(super) fn prepare_with_constructor(
    text: &str,
    namespaces: &BTreeMap<String, String>,
    session: Rc<QueryNumericSession>,
    constructor: Option<spargebra::SourceBlankNodeConstructor>,
) -> Result<PreparedSourceUpdate, NumericPrepareError> {
    prepare_with_constructors(text, namespaces, session, constructor, None)
}

/// Explicit private UPDATE construction input. No shared/global RNG state is
/// replaced, and no callback is retained by the persistent dataset.
pub(super) fn prepare_with_update_constructor(
    text: &str,
    namespaces: &BTreeMap<String, String>,
    session: Rc<QueryNumericSession>,
    constructor: Option<spargebra::SourceBlankNodeConstructor>,
) -> Result<PreparedSourceUpdate, NumericPrepareError> {
    prepare_with_constructors(text, namespaces, session, constructor.clone(), constructor)
}

fn prepare_with_constructors(
    text: &str,
    namespaces: &BTreeMap<String, String>,
    session: Rc<QueryNumericSession>,
    constructor: Option<spargebra::SourceBlankNodeConstructor>,
    update_blank_constructor: Option<spargebra::SourceBlankNodeConstructor>,
) -> Result<PreparedSourceUpdate, NumericPrepareError> {
    if session.context_semantics() != crate::numeric::QueryContextSemantics::SourceContextV1
        || session.literal_semantics() != crate::numeric::QueryLiteralSemantics::SourceLiteralV1
    {
        return Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin,
        ));
    }
    let registry = PreparedSourceLiteralRegistry::new_for_graph_execution(session);
    let mut parser = SparqlParser::new();
    if let Some(constructor) = constructor {
        parser = parser.with_source_blank_node_constructor(constructor);
    }
    for (prefix, iri) in namespaces {
        parser = parser.with_prefix(prefix, iri).map_err(|e| {
            NumericPrepareError::Query(QueryEvaluationError::Unexpected(Box::new(e)))
        })?;
    }
    let request = parser
        .parse_update_rdflib760_source(text, registry.hooks())
        .map_err(|error| {
            if let Some(argument) = error.source_update_namespace_message() {
                return NumericPrepareError::Query(QueryEvaluationError::Unexpected(Box::new(
                    crate::eval::source_compvalue::SourceCompValueError {
                        module: "builtins", class: "Exception", argument,
                    },
                )));
            }
            match error {
            NumericQueryParseError::Syntax(e) => NumericPrepareError::Syntax(e),
            NumericQueryParseError::SourceSyntax(e) => NumericPrepareError::SourceSyntax(e),
            NumericQueryParseError::LiteralConstruction(e) => match e.downcast::<RawNumericFault>()
            {
                Ok(e) => NumericPrepareError::LiteralConstruction(*e),
                Err(e) => NumericPrepareError::Query(QueryEvaluationError::Unexpected(e)),
            },
        }})?;
    Ok(PreparedSourceUpdate { request, registry, update_blank_constructor })
}

impl PreparedSourceUpdate {
    /// Received INSERT DATA / DELETE DATA phase over the exact sealed parser
    /// registry and existing Memory owner. This is not the full wrapper UPDATE
    /// API: WHERE/graph management/loading/cache/revision stay separate duties.
    pub(super) fn execute_data(&self, graph: &mut crate::dataset::source_memory::SourceMemoryGraph) -> Result<(), QueryEvaluationError> {
        use crate::dataset::source_memory::{SourceGraphTerm, SourceMemoryGraph};
        use crate::numeric::{NumericInitialValue, RawExpressionValue};
        use spargebra::term::{TermPattern, NamedNodePattern, GraphNamePattern};
        use spargebra::SourceUpdateOperationKind;
        fn fault(e:impl std::error::Error+Send+Sync+'static)->QueryEvaluationError { QueryEvaluationError::Unexpected(Box::new(e)) }
        if !Rc::ptr_eq(graph.session(), self.registry.session()) {
            return Err(fault(RawNumericFault::InvalidSourceOrigin));
        }
        // Evaluate values on demand, in original operation/triple order. Do
        // not preflight later operations and erase an earlier actual mutation.
        let session=graph.session().clone();
        let apply=|graph:&mut SourceMemoryGraph,scope:&SourceGraphTerm,triples:&[spargebra::term::TriplePattern],delete:bool|->Result<(),QueryEvaluationError>{
            for triple in triples {
                let term=|v:&TermPattern|->Result<SourceGraphTerm,QueryEvaluationError>{
                    let raw=match v {
                        TermPattern::NamedNode(v)=>RawExpressionValue::from_import(v.clone().into()).with_new_source_object(),
                        TermPattern::BlankNode(v)=>RawExpressionValue::from_import(v.clone().into()).with_new_source_object(),
                        TermPattern::Variable(v)=>session.construct_initial(NumericInitialValue::SourceVariable(v.clone())).map_err(fault)?,
                        TermPattern::SourceLiteral(v)=>self.registry.resolve(v).map_err(fault)?,
                        _=>return Err(fault(RawNumericFault::InvalidSourceOrigin)),
                    };
                    graph.admit_source(raw).map_err(fault)
                };
                let predicate=match &triple.predicate { NamedNodePattern::NamedNode(v)=>TermPattern::NamedNode(v.clone()), NamedNodePattern::Variable(v)=>TermPattern::Variable(v.clone()) };
                let row=[term(&triple.subject)?,term(&predicate)?,term(&triple.object)?];
                if delete { graph.remove_context(Some(scope),row.map(Some)).map_err(fault)?; }
                else { graph.insert_context(scope,row).map_err(fault)?; }
            }
            Ok(())
        };
        for operation in &self.request.operations {
            let (data,delete)=match &operation.kind {
                SourceUpdateOperationKind::InsertData(v)=>(v,false),
                SourceUpdateOperationKind::DeleteData(v)=>(v,true),
                _=>return Err(fault(RawNumericFault::UnsupportedSourceLiteralOperator("unreceived source UPDATE operation execution"))),
            };
            let default=graph.default_context().map_err(fault)?;
            apply(graph,&default,&data.triples,delete)?;
            for (name,triples) in &data.quads {
                let scope=match name {
                    GraphNamePattern::DefaultGraph=>default.clone(),
                    GraphNamePattern::NamedNode(v)=>graph.admit_source(RawExpressionValue::from_import(v.clone().into()).with_new_source_object()).map_err(fault)?,
                    // Actual Graph(identifier) constructor converts a Variable
                    // graph name to URIRef(str); fact variables stay Variables.
                    GraphNamePattern::Variable(v)=>graph.admit_source(RawExpressionValue::from_import(oxrdf::NamedNode::new_unchecked(v.as_str()).into()).with_new_source_object()).map_err(fault)?,
                };
                apply(graph,&scope,triples,delete)?;
            }
        }
        Ok(())
    }
}
