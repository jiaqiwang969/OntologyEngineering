//! One source-literal registry per sealed parser preparation. The actual
//! constructor result is shared by translation facts and compiler leaves;
//! RDF equality never identifies a parser occurrence or grants provenance.
use super::*;
use spargebra::{
    SourceLiteralHooks, SourceLiteralInput, SourceLiteralOccurrenceId, SourceLiteralScope,
};
use std::cell::RefCell;

struct PreparedSourceLiteral {
    // This description is unstamped. Only the parser stamps an occurrence id.
    description: ParsedSourceLiteral,
    value: RawExpressionValue,
}

pub(crate) struct PreparedSourceLiteralRegistry {
    session: Rc<QueryNumericSession>,
    scope: SourceLiteralScope,
    entries: RefCell<Vec<PreparedSourceLiteral>>,
    exists_execution: bool,
    graph_execution: bool,
    path_execution: bool,
    graph_blank_nodes: RefCell<Vec<oxrdf::BlankNode>>,
}

impl PreparedSourceLiteralRegistry {
    pub(crate) fn session(&self) -> &Rc<QueryNumericSession> {
        &self.session
    }
    pub(crate) fn new(session: Rc<QueryNumericSession>) -> Rc<Self> {
        Self::with_execution(session, false)
    }

    pub(crate) fn new_for_exists_execution(session: Rc<QueryNumericSession>) -> Rc<Self> {
        Self::with_execution(session, true)
    }

    pub(crate) fn new_for_graph_execution(session: Rc<QueryNumericSession>) -> Rc<Self> {
        let mut registry = Self::with_execution(session, true);
        Rc::get_mut(&mut registry)
            .expect("fresh private registry")
            .graph_execution = true;
        registry
    }
    pub(crate) fn admits_graph_execution(&self, session: &Rc<QueryNumericSession>) -> bool {
        self.graph_execution && self.admits_exists_execution(session)
    }
    pub(crate) fn new_for_path_execution(session: Rc<QueryNumericSession>) -> Rc<Self> {
        let mut registry = Self::new_for_graph_execution(session);
        Rc::get_mut(&mut registry)
            .expect("fresh native path registry")
            .path_execution = true;
        registry
    }
    pub(crate) fn admits_path_execution(&self, session: &Rc<QueryNumericSession>) -> bool {
        self.path_execution && self.admits_graph_execution(session)
    }

    fn with_execution(session: Rc<QueryNumericSession>, exists_execution: bool) -> Rc<Self> {
        Rc::new(Self {
            session,
            scope: SourceLiteralScope::new(),
            entries: RefCell::new(Vec::new()),
            exists_execution,
            graph_execution: false,
            path_execution: false,
            graph_blank_nodes: RefCell::new(Vec::new()),
        })
    }

    pub(crate) fn admits_exists_execution(&self, session: &Rc<QueryNumericSession>) -> bool {
        self.exists_execution && Rc::ptr_eq(&self.session, session)
    }

    pub(super) fn record_graph_blank_node(
        &self,
        node: &oxrdf::BlankNode,
    ) -> Result<(), RawNumericFault> {
        let mut nodes = self.graph_blank_nodes.borrow_mut();
        if !nodes.contains(node) {
            nodes.try_reserve(1).map_err(|_| {
                RawNumericFault::Core(NumericFault::ResourceLimit {
                    operation: "source_graph_blank_node_domain",
                })
            })?;
            nodes.push(node.clone());
        }
        Ok(())
    }
    pub(crate) fn graph_blank_nodes(&self) -> Vec<oxrdf::BlankNode> {
        self.graph_blank_nodes.borrow().clone()
    }

    pub(crate) fn hooks(self: &Rc<Self>) -> SourceLiteralHooks {
        let construct_owner = Rc::clone(self);
        let truth_owner = Rc::clone(self);
        let variable_owner = Rc::clone(self);
        let order_owner = Rc::clone(self);
        let native_order_owner = Rc::clone(self);
        let projection_owner = Rc::clone(self);
        let variable_sets: Option<spargebra::SourceVariableSets> =
            self.exists_execution.then(|| {
                let construct: spargebra::SourceVariableSets = Rc::new(move |steps| {
                    let hash = variable_owner.session.source_hash().ok_or_else(
                        || -> spargebra::SourceLiteralBridgeError {
                            Box::new(RawNumericFault::InvalidSourceOrigin)
                        },
                    )?;
                    crate::eval::compile_source_variable_sets(steps, hash)
                        .map_err(|error| -> spargebra::SourceLiteralBridgeError { Box::new(error) })
                });
                construct
            });
        SourceLiteralHooks {
            scope: self.scope.clone(),
            variable_sets,
            triples_order: self.graph_execution.then(|| {
                let order: spargebra::SourceTriplesOrder = Rc::new(move |triples| {
                    order_owner
                        .order_graph_triples(triples)
                        .map_err(|error| -> spargebra::SourceLiteralBridgeError { Box::new(error) })
                });
                order
            }),
            graph_path_admission: self.graph_execution.then(|| {
                let native = self.path_execution;
                let check: spargebra::SourcePathAdmission = Rc::new(move |path| {
                    if native
                        || matches!(
                            path,
                            spargebra::algebra::PropertyPathExpression::NamedNode(_)
                        )
                    {
                        Ok(())
                    } else {
                        Err(Box::new(RawNumericFault::UnsupportedSourceLiteralOperator(
                            "source graph path operator",
                        )))
                    }
                });
                check
            }),
            visible_projection: self.graph_execution.then(|| {
                // Path operands remain opaque to source variable visitors.
                let project: spargebra::SourceVisibleProjection =
                    Rc::new(move |occurrences, children| {
                        let hash = projection_owner.session.source_hash().ok_or_else(
                            || -> spargebra::SourceLiteralBridgeError {
                                Box::new(RawNumericFault::InvalidSourceOrigin)
                            },
                        )?;
                        crate::eval::source_visible_projection(occurrences, children, hash).map_err(
                            |error| -> spargebra::SourceLiteralBridgeError { Box::new(error) },
                        )
                    });
                project
            }),
            native_triples_order: self.path_execution.then(|| {
                let order: spargebra::SourceNativeTriplesOrder = Rc::new(move |triples| {
                    native_order_owner
                        .order_native_graph_triples(triples)
                        .map_err(|e| -> spargebra::SourceLiteralBridgeError { Box::new(e) })
                });
                order
            }),
            construct: Rc::new(move |id, input| {
                construct_owner.construct(id, input).map_err(
                    |error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) },
                )
            }),
            python_truth: Rc::new(move |id| {
                truth_owner.python_truth(id).map_err(
                    |error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) },
                )
            }),
        }
    }

    fn construct(
        &self,
        id: SourceLiteralOccurrenceId,
        input: SourceLiteralInput,
    ) -> Result<ParsedSourceLiteral, RawNumericFault> {
        // A repeated PEG occurrence is handled by the parser's span cache;
        // this callback cannot overwrite an already constructed source object.
        if !id.belongs_to(&self.scope) || id.index() != self.entries.borrow().len() {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let (description, value) = match input {
            SourceLiteralInput::Parsed(description) => {
                if description.occurrence.is_some() {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                }
                let value = if self.graph_execution {
                    self.session.construct_source_graph_literal(&description)?
                } else {
                    self.session.construct_source_literal(&description)?
                };
                (description, value)
            }
            SourceLiteralInput::NumericToken {
                kind,
                sign,
                magnitude,
            } => {
                let value = if self.graph_execution {
                    self.session
                        .construct_source_graph_numeric_token(kind, sign, &magnitude)?
                } else {
                    self.session
                        .construct_source_numeric_token(kind, sign, &magnitude)?
                };
                let Term::Literal(rdf) = value.rdf()? else {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                };
                let description = ParsedSourceLiteral {
                    rdf: rdf.clone(),
                    datatype: SourceDatatype::Specified(rdf.datatype().into_owned()),
                    language_original: None,
                    origin: SourceLiteralOrigin::QueryNumericToken {
                        kind,
                        sign,
                        magnitude,
                    },
                    occurrence: None,
                };
                (description, value)
            }
        };
        if value.rdf()? != &Term::Literal(description.rdf.clone()) {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let mut entries = self.entries.borrow_mut();
        if entries.len() != id.index() {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        entries.push(PreparedSourceLiteral {
            description: description.clone(),
            value,
        });
        Ok(description)
    }

    fn python_truth(&self, id: &SourceLiteralOccurrenceId) -> Result<bool, RawNumericFault> {
        if !id.belongs_to(&self.scope) {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let value = self
            .entries
            .borrow()
            .get(id.index())
            .ok_or(RawNumericFault::InvalidSourceOrigin)?
            .value
            .clone();
        self.session.source_literal_python_truth(&value)
    }

    /// Resolve only an occurrence from this parser preparation and its exact
    /// source description. Unknown ids and edited descriptions never fall back
    /// to string reconstruction, even if their RDF term compares equal.
    pub(crate) fn resolve(
        &self,
        literal: &ParsedSourceLiteral,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        let id = literal
            .occurrence
            .as_ref()
            .ok_or(RawNumericFault::InvalidSourceOrigin)?;
        if !id.belongs_to(&self.scope) {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let mut description = literal.clone();
        description.occurrence = None;
        let entries = self.entries.borrow();
        let entry = entries
            .get(id.index())
            .ok_or(RawNumericFault::InvalidSourceOrigin)?;
        if entry.description != description {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        Ok(entry.value.clone())
    }
}

/// Shared by the existing expression compiler and all source VALUES paths.
pub(crate) fn resolve_source_literal(
    session: &QueryNumericSession,
    prepared: Option<&PreparedSourceLiteralRegistry>,
    literal: &ParsedSourceLiteral,
) -> Result<RawExpressionValue, RawNumericFault> {
    if literal.occurrence.is_some() {
        prepared
            .ok_or(RawNumericFault::InvalidSourceOrigin)?
            .resolve(literal)
    } else {
        session.construct_source_literal(literal)
    }
}
