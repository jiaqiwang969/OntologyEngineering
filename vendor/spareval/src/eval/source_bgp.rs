//! Compiled source BGP leaves over the existing tuple/context/raw codec.
//! Selectors are compiled once; execution uses the actual sealed Dataset's
//! indexes and iterators. No runtime syntax interpretation or copied graph.
use super::source_context_join::SourceContextRuntime;
use super::source_sort::{SourceSortError, source_sort_indices};
use super::*;
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern};

pub(super) type SourceBgpScan<'a, T> = Rc<
    dyn Fn(
            [Option<SolutionValue<T>>; 3],
        ) -> Result<
            Box<dyn Iterator<Item = Result<[T; 3], QueryEvaluationError>> + 'a>,
            QueryEvaluationError,
        > + 'a,
>;

#[derive(Clone)]
pub(super) enum SourceBgpTerm<T> {
    Binding(usize),
    Constant(T),
    NativePath,
}
impl<T: Clone> SourceBgpTerm<T> {
    fn lookup(&self, bindings: &InternalTuple<T>) -> Option<SolutionValue<T>> {
        match self {
            Self::Binding(slot) => bindings.cell(*slot).cloned(),
            Self::Constant(value) => Some(SolutionValue::Rdf(value.clone())),
            Self::NativePath => None,
        }
    }
}
fn is_none<T>(value: &Option<SolutionValue<T>>) -> bool {
    matches!(value, None | Some(SolutionValue::ExplicitNone))
}
fn invalid() -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
}
fn resource(operation: &'static str) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::Core(
        semantica_numeric::NumericFault::ResourceLimit { operation },
    )))
}
fn already_bound() -> QueryEvaluationError {
    nullable::uncaught(EvalFailure::Sparql(ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        "None",
        SourceException {
            class: SourceExceptionClass::AlreadyBound,
            argument: SourceExceptionArgument::None,
        },
    )))
}

/// evalPart performs a stable key-only sort before returning evalBGP's lazy
/// iterator. Missing/explicit None are unknown; actual exception cells are
/// present, with no RDF conversion or SPARQL exception capture at this step.
pub(super) fn source_bgp_runtime_order<T: Clone>(
    patterns: &[[SourceBgpTerm<T>; 3]],
    bindings: &InternalTuple<T>,
    token: &CancellationToken,
) -> Result<Vec<usize>, QueryEvaluationError> {
    token.ensure_alive()?;
    let mut counts = Vec::new();
    counts
        .try_reserve_exact(patterns.len())
        .map_err(|_| resource("source_bgp_runtime_counts"))?;
    for row in patterns {
        token.ensure_alive()?;
        counts.push(
            row.iter()
                .filter(|term| {
                    !matches!(term, SourceBgpTerm::NativePath) && is_none(&term.lookup(bindings))
                })
                .count(),
        );
    }
    let mut order = Vec::new();
    order
        .try_reserve_exact(patterns.len())
        .map_err(|_| resource("source_bgp_runtime_indices"))?;
    order.extend(0..patterns.len());
    source_sort_indices(
        &mut order,
        false,
        |a, b| Ok(counts[a] < counts[b]),
        || token.ensure_alive(),
    )
    .map_err(|error| match error {
        SourceSortError::Callback(error) => error,
        SourceSortError::Allocation => resource("source_bgp_runtime_sort_allocation"),
        SourceSortError::IndexOverflow => resource("source_bgp_runtime_sort_index"),
    })?;
    Ok(order)
}

struct SourceBgpFrame<'a, T> {
    context: InternalTuple<T>,
    depth: usize,
    wildcard: [bool; 3],
    rows: Option<
        Box<dyn Iterator<Item = Result<[Option<SolutionValue<T>>; 3], QueryEvaluationError>> + 'a>,
    >,
}
struct SourceBgpIterator<'a, T> {
    patterns: Rc<[[SourceBgpTerm<T>; 3]]>,
    paths: Rc<[Option<Rc<super::source_path::CompiledSourcePath<'a, T>>>]>,
    order: Vec<usize>,
    stack: Vec<SourceBgpFrame<'a, T>>,
    runtime: Rc<SourceContextRuntime<'a, T>>,
    scan: SourceBgpScan<'a, T>,
    terminal: bool,
}
impl<'a, T: Clone + Eq + 'a> SourceBgpIterator<'a, T> {
    fn poll(&mut self) -> Result<Option<InternalTuple<T>>, QueryEvaluationError> {
        loop {
            self.runtime.token.ensure_alive()?;
            let Some(frame) = self.stack.last_mut() else {
                return Ok(None);
            };
            if frame.depth == self.order.len() {
                let row = frame.context.source_solution(None);
                self.stack.pop();
                return Ok(Some(row));
            }
            let index = self.order[frame.depth];
            let pattern = &self.patterns[index];
            if frame.rows.is_none() {
                let bindings = frame
                    .context
                    .source_context_bindings()
                    .ok_or_else(invalid)?;
                let cells = pattern.each_ref().map(|term| term.lookup(&bindings));
                frame.wildcard = std::array::from_fn(|at| {
                    !matches!(pattern[at], SourceBgpTerm::NativePath) && is_none(&cells[at])
                });
                frame.rows = Some(if let Some(path) = &self.paths[index] {
                    Box::new(
                        path.start([cells[0].clone(), cells[2].clone()])
                            .map(|pair| pair.map(|[s, o]| [Some(s), None, Some(o)])),
                    )
                } else {
                    Box::new(
                        (self.scan)(cells)?
                            .map(|row| row.map(|row| row.map(|v| Some(SolutionValue::Rdf(v))))),
                    )
                });
            }
            let Some(triple) = frame.rows.as_mut().ok_or_else(invalid)?.next() else {
                self.stack.pop();
                continue;
            };
            let triple = triple?;
            self.runtime.token.ensure_alive()?;
            let mut child = if frame.wildcard.iter().any(|v| *v) {
                frame.context.source_push(&self.runtime.initial)
            } else {
                frame.context.clone()
            };
            let mut skip = false;
            for (at, value) in triple.into_iter().enumerate() {
                if !frame.wildcard[at] {
                    continue;
                }
                let SourceBgpTerm::Binding(slot) = &pattern[at] else {
                    return Err(invalid());
                };
                let value = value.ok_or_else(invalid)?;
                if !child
                    .source_assign_by(*slot, value, |old, new| self.runtime.equal_cells(old, new))?
                {
                    // Subject assignment is OUTSIDE both AlreadyBound catches.
                    // Predicate/object assignments each skip this match.
                    if at == 0 {
                        return Err(already_bound());
                    }
                    skip = true;
                    break;
                }
            }
            if skip {
                continue;
            }
            let depth = frame
                .depth
                .checked_add(1)
                .ok_or_else(|| resource("source_bgp_depth"))?;
            self.stack.push(SourceBgpFrame {
                context: child,
                depth,
                wildcard: [false; 3],
                rows: None,
            });
        }
    }
}
impl<'a, T: Clone + Eq + 'a> Iterator for SourceBgpIterator<'a, T> {
    type Item = Result<InternalTuple<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.terminal {
            return None;
        }
        match self.poll() {
            Ok(Some(row)) => Some(Ok(row)),
            Ok(None) => {
                self.terminal = true;
                None
            }
            Err(error) => {
                self.terminal = true;
                self.stack.clear();
                Some(Err(error))
            }
        }
    }
}

fn physical_bgp_in_scope(pattern: &GraphPattern, scope:Option<&NamedNodePattern>) -> bool {
    match pattern {
        GraphPattern::QuadPattern { graph_name, .. } => graph_name.as_ref()==scope,
        GraphPattern::Join { left, right, .. } => {
            physical_bgp_in_scope(left,scope) && physical_bgp_in_scope(right,scope)
        }
        GraphPattern::Values {
            variables,
            bindings,
        } => variables.is_empty() && bindings.len() == 1 && bindings[0].is_empty(),
        GraphPattern::Graph {graph_name} => Some(graph_name)==scope,
        _ => false,
    }
}
impl<'a, D: QueryableDataset<'a>> SimpleEvaluator<'a, D> {
    pub(super) fn source_bgp_evaluator(
        &self,
        physical: &GraphPattern,
        patterns: &[TriplePattern],
        variables: &mut Vec<Variable>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        self.source_bgp_evaluator_ordered(physical,patterns,variables,true)
    }
    pub(super) fn source_bgp_evaluator_ordered(
        &self, physical:&GraphPattern, patterns:&[TriplePattern], variables:&mut Vec<Variable>, sort_for_eval_part:bool,
    ) -> Result<InternalTupleEvaluator<'a,D::InternalTerm>,QueryEvaluationError> {
        let runtime = self.source_context_runtime()?;
        let codec = self
            .dataset
            .source_literal_codec()
            .cloned()
            .ok_or_else(invalid)?;
        if !physical_bgp_in_scope(physical,self.source_graph_scope.as_ref())
            || !codec
                .prepared_literals
                .as_ref()
                .is_some_and(|registry| registry.admits_graph_execution(&codec.session))
        {
            return Err(invalid());
        }
        let scan = codec.source_bgp_scan.clone().ok_or_else(invalid)?;
        let slot = |variable: &Variable| -> Result<usize, QueryEvaluationError> {
            runtime
                .variables
                .iter()
                .position(|v| v == variable)
                .ok_or_else(invalid)
        };
        let convert =
            |term: &TermPattern| -> Result<SourceBgpTerm<D::InternalTerm>, QueryEvaluationError> {
                Ok(match term {
                    TermPattern::Variable(v) => SourceBgpTerm::Binding(slot(v)?),
                    TermPattern::BlankNode(v) => SourceBgpTerm::Binding(
                        runtime.graph_blank_node_slot(v).ok_or_else(invalid)?,
                    ),
                    TermPattern::NamedNode(v) => SourceBgpTerm::Constant((codec.internalize)(
                        RawExpressionValue::from_import(v.clone().into()),
                    )?),
                    TermPattern::SourceLiteral(v) => SourceBgpTerm::Constant((codec.internalize)(
                        codec
                            .source_literal(v)
                            .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))?,
                    )?),
                    TermPattern::Literal(_) => return Err(invalid()),
                    #[cfg(feature = "sparql-12")]
                    TermPattern::Triple(_) => return Err(invalid()),
                })
            };
        let mut compiled = Vec::new();
        compiled
            .try_reserve_exact(patterns.len())
            .map_err(|_| resource("source_bgp_compiled_patterns"))?;
        for pattern in patterns {
            runtime.token.ensure_alive()?;
            let predicate = match &pattern.predicate {
                NamedNodePattern::Variable(v) => TermPattern::Variable(v.clone()),
                NamedNodePattern::NamedNode(v) => TermPattern::NamedNode(v.clone()),
            };
            compiled.push([
                convert(&pattern.subject)?,
                convert(&predicate)?,
                convert(&pattern.object)?,
            ]);
        }
        if runtime.variables.as_ref() != variables.as_slice() {
            return Err(invalid());
        }
        let patterns: Rc<[_]> = compiled.into();
        let paths: Rc<[_]> = (0..patterns.len()).map(|_| None).collect::<Vec<_>>().into();
        Ok(Rc::new(move |from| {
            runtime.token.ensure_alive()?;
            let bindings = from.source_context_bindings().ok_or_else(invalid)?;
            let order = if sort_for_eval_part {source_bgp_runtime_order(&patterns, &bindings, &runtime.token)?} else {(0..patterns.len()).collect()};
            let mut stack = Vec::new();
            stack
                .try_reserve_exact(
                    order
                        .len()
                        .checked_add(1)
                        .ok_or_else(|| resource("source_bgp_frames"))?,
                )
                .map_err(|_| resource("source_bgp_frames"))?;
            stack.push(SourceBgpFrame {
                context: from,
                depth: 0,
                wildcard: [false; 3],
                rows: None,
            });
            Ok(Box::new(SourceBgpIterator {
                patterns: Rc::clone(&patterns),
                paths: Rc::clone(&paths),
                order,
                stack,
                runtime: Rc::clone(&runtime),
                scan: Rc::clone(&scan),
                terminal: false,
            }))
        }))
    }

    pub(super) fn source_path_bgp_evaluator(
        &self,
        physical: &GraphPattern,
        patterns: &[spargebra::SourceTriplePattern],
        variables: &mut Vec<Variable>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        use spargebra::SourceTriplePredicate as P;
        fn physical_default(pattern: &GraphPattern,scope:Option<&NamedNodePattern>) -> bool {
            match pattern {
                GraphPattern::Path { graph_name, .. } => graph_name.as_ref()==scope,
                GraphPattern::Join { left, right, .. } => {
                    physical_default(left,scope) && physical_default(right,scope)
                }
                _ => physical_bgp_in_scope(pattern,scope),
            }
        }
        let runtime = self.source_context_runtime()?;
        let codec = self
            .dataset
            .source_literal_codec()
            .cloned()
            .ok_or_else(invalid)?;
        let registry = codec.prepared_literals.clone().ok_or_else(invalid)?;
        if !physical_default(physical,self.source_graph_scope.as_ref()) || !registry.admits_path_execution(&codec.session) {
            return Err(invalid());
        }
        let scan = codec.source_bgp_scan.clone().ok_or_else(invalid)?;
        let slot = |v: &Variable| {
            runtime
                .variables
                .iter()
                .position(|a| a == v)
                .ok_or_else(invalid)
        };
        let convert =
            |v: &TermPattern| -> Result<SourceBgpTerm<D::InternalTerm>, QueryEvaluationError> {
                Ok(match v {
                    TermPattern::Variable(v) => SourceBgpTerm::Binding(slot(v)?),
                    TermPattern::BlankNode(v) => SourceBgpTerm::Binding(
                        runtime.graph_blank_node_slot(v).ok_or_else(invalid)?,
                    ),
                    TermPattern::NamedNode(v) => SourceBgpTerm::Constant((codec.internalize)(
                        RawExpressionValue::from_import(v.clone().into()),
                    )?),
                    TermPattern::SourceLiteral(v) => SourceBgpTerm::Constant((codec.internalize)(
                        codec
                            .source_literal(v)
                            .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))?,
                    )?),
                    _ => return Err(invalid()),
                })
            };
        let path_runtime = Rc::new(super::source_path::SourcePathRuntime {
            scan: scan.clone(),
            read: codec.externalize.clone(),
            write: codec.internalize.clone(),
            session: codec.session.clone(),
            registry,
            token: runtime.token.clone(),
        });
        let mut compiled = Vec::new();
        let mut paths = Vec::new();
        compiled
            .try_reserve_exact(patterns.len())
            .map_err(|_| resource("source_path_bgp_patterns"))?;
        paths
            .try_reserve_exact(patterns.len())
            .map_err(|_| resource("source_path_bgp_paths"))?;
        for p in patterns {
            runtime.token.ensure_alive()?;
            let (predicate, path) = match &p.predicate {
                P::Rdf(NamedNodePattern::Variable(v)) => (SourceBgpTerm::Binding(slot(v)?), None),
                P::Rdf(NamedNodePattern::NamedNode(v)) => (
                    SourceBgpTerm::Constant((codec.internalize)(RawExpressionValue::from_import(
                        v.clone().into(),
                    ))?),
                    None,
                ),
                P::Path { definition, .. } => (
                    SourceBgpTerm::NativePath,
                    Some(Rc::new(super::source_path::compile_source_path(
                        path_runtime.clone(),
                        definition,
                    )?)),
                ),
            };
            compiled.push([convert(&p.subject)?, predicate, convert(&p.object)?]);
            paths.push(path);
        }
        if runtime.variables.as_ref() != variables.as_slice() {
            return Err(invalid());
        }
        let patterns: Rc<[_]> = compiled.into();
        let paths: Rc<[_]> = paths.into();
        Ok(Rc::new(move |from| {
            runtime.token.ensure_alive()?;
            let bindings = from.source_context_bindings().ok_or_else(invalid)?;
            let order = source_bgp_runtime_order(&patterns, &bindings, &runtime.token)?;
            let mut stack = Vec::new();
            stack
                .try_reserve_exact(
                    order
                        .len()
                        .checked_add(1)
                        .ok_or_else(|| resource("source_path_bgp_frames"))?,
                )
                .map_err(|_| resource("source_path_bgp_frames"))?;
            stack.push(SourceBgpFrame {
                context: from,
                depth: 0,
                wildcard: [false; 3],
                rows: None,
            });
            Ok(Box::new(SourceBgpIterator {
                patterns: patterns.clone(),
                paths: paths.clone(),
                order,
                stack,
                runtime: runtime.clone(),
                scan: scan.clone(),
                terminal: false,
            }))
        }))
    }
}
