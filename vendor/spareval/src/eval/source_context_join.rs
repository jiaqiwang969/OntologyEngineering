//! Source binding consumers over the existing compiled child functions.
//! RDFLib evaluate.py/evalutils.py control flow; no AST or expression interpreter.
use super::source_set::{SourceSet, SourceSetError};
use super::*;
use crate::numeric::SourceHashRuntime;
use crate::source_hash::hash_source_none;
use semantica_numeric::NumericFault;
use spargebra::algebra::SourceJoinMode;

pub(super) type SourceCellEqual<'a, T> =
    Rc<dyn Fn(&SolutionValue<T>, &SolutionValue<T>) -> Result<bool, QueryEvaluationError> + 'a>;
pub(super) type SourceRdfHash<'a, T> = Rc<dyn Fn(&T) -> Result<i64, QueryEvaluationError> + 'a>;
pub(super) type SourcePredicate<'a, T> =
    Rc<dyn Fn(&InternalTuple<T>) -> Result<bool, QueryEvaluationError> + 'a>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourceChildSide {
    Left,
    Right,
    Child,
}

#[cfg(test)]
#[derive(Clone)]
pub(super) enum SourceContextEvent<T> {
    QueryContextStart {
        current: InternalTuple<T>,
    },
    ValuesSolution {
        row: InternalTuple<T>,
    },
    ChildStart {
        node: usize,
        side: SourceChildSide,
        probe: u8,
        current: InternalTuple<T>,
        initial: Rc<InternalTuple<T>>,
    },
    ChildStarted {
        node: usize,
        side: SourceChildSide,
        probe: u8,
    },
    ChildPoll {
        node: usize,
        side: SourceChildSide,
        probe: u8,
        ordinal: usize,
    },
    ChildRow {
        node: usize,
        side: SourceChildSide,
        probe: u8,
        ordinal: usize,
        row: InternalTuple<T>,
    },
    ChildEnd {
        node: usize,
        side: SourceChildSide,
        probe: u8,
        ordinal: usize,
    },
    ChildFailed {
        node: usize,
        side: SourceChildSide,
        probe: u8,
        ordinal: usize,
    },
    ChildDropped {
        node: usize,
        side: SourceChildSide,
        probe: u8,
        polls: usize,
    },
    RightSetComplete {
        node: usize,
        retained_rows: Vec<InternalTuple<T>>,
    },
    OptionalRetry {
        node: usize,
        left_row: InternalTuple<T>,
        remembered_row: InternalTuple<T>,
        thawed_current: InternalTuple<T>,
        initial: Rc<InternalTuple<T>>,
    },
    DistinctContains {
        node: usize,
        row: InternalTuple<T>,
        present: bool,
    },
    DistinctAdd {
        node: usize,
        row: InternalTuple<T>,
    },
    OutputRow {
        row: InternalTuple<T>,
        initial: Rc<InternalTuple<T>>,
    },
}
#[cfg(test)]
pub(super) type SourceContextObserver<'a, T> = Rc<dyn Fn(SourceContextEvent<T>) + 'a>;

pub(super) struct SourceContextRuntime<'a, T> {
    pub(super) variables: Rc<[Variable]>,
    pub(super) initial: Rc<InternalTuple<T>>,
    pub(super) token: CancellationToken,
    graph_blank_nodes: Rc<[(usize, oxrdf::BlankNode)]>,
    hash: Rc<SourceHashRuntime>,
    cell_equal: SourceCellEqual<'a, T>,
    rdf_hash: SourceRdfHash<'a, T>,
    next_node: Rc<Cell<usize>>,
    #[cfg(test)]
    observer: Option<SourceContextObserver<'a, T>>,
}

impl<T> Clone for SourceContextRuntime<'_, T> {
    fn clone(&self) -> Self {
        Self {
            variables: Rc::clone(&self.variables),
            initial: Rc::clone(&self.initial),
            token: self.token.clone(),
            graph_blank_nodes: Rc::clone(&self.graph_blank_nodes),
            hash: Rc::clone(&self.hash),
            cell_equal: Rc::clone(&self.cell_equal),
            rdf_hash: Rc::clone(&self.rdf_hash),
            next_node: Rc::clone(&self.next_node),
            #[cfg(test)]
            observer: self.observer.clone(),
        }
    }
}

impl<'a, T> SourceContextRuntime<'a, T> {
    pub(super) fn new(
        variables: Rc<[Variable]>,
        initial: Rc<InternalTuple<T>>,
        token: CancellationToken,
        hash: Rc<SourceHashRuntime>,
        cell_equal: SourceCellEqual<'a, T>,
        rdf_hash: SourceRdfHash<'a, T>,
    ) -> Self {
        Self {
            variables,
            initial,
            token,
            graph_blank_nodes: Rc::from([]),
            hash,
            cell_equal,
            rdf_hash,
            next_node: Rc::new(Cell::new(0)),
            #[cfg(test)]
            observer: None,
        }
    }

    pub(super) fn with_graph_blank_nodes(mut self, nodes: Vec<(usize, oxrdf::BlankNode)>) -> Self {
        self.graph_blank_nodes = nodes.into();
        self
    }
    pub(super) fn graph_blank_node_slot(&self, node: &oxrdf::BlankNode) -> Option<usize> {
        self.graph_blank_nodes
            .iter()
            .find_map(|(slot, key)| (key == node).then_some(*slot))
    }

    /// Compilation only. Each allocated node owns a live compiled closure, so
    /// address-space exhaustion necessarily precedes this counter overflow.
    pub(super) fn allocate_node(&self) -> usize {
        let node = self.next_node.get();
        self.next_node.set(
            node.checked_add(1)
                .expect("compiled source node count overflow"),
        );
        node
    }

    pub(super) fn equal_cells(
        &self,
        a: &SolutionValue<T>,
        b: &SolutionValue<T>,
    ) -> Result<bool, QueryEvaluationError> {
        self.token.ensure_alive()?;
        (self.cell_equal)(a, b)
    }

    pub(super) fn hash_row(&self, row: &InternalTuple<T>) -> Result<i64, QueryEvaluationError> {
        self.token.ensure_alive()?;
        row.source_hash_by(
            |slot| {
                self.token.ensure_alive()?;
                if let Some((_, node)) = self.graph_blank_nodes.iter().find(|(at, _)| *at == slot) {
                    return Ok(self.hash.variable_hash(node.as_str()));
                }
                let variable = self.variables.get(slot).ok_or_else(|| {
                    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
                })?;
                Ok(self.hash.variable_hash(variable.as_str()))
            },
            |cell| {
                self.token.ensure_alive()?;
                match cell {
                    SolutionValue::Rdf(value) => (self.rdf_hash)(value),
                    SolutionValue::ExplicitNone => Ok(hash_source_none()),
                    SolutionValue::ReturnedError(error) => Ok(error.source_native_identity_hash()),
                }
            },
        )
    }

    pub(super) fn equal_rows(
        &self,
        a: &InternalTuple<T>,
        b: &InternalTuple<T>,
    ) -> Result<bool, QueryEvaluationError> {
        self.token.ensure_alive()?;
        if a.source_same_object(b) {
            return Ok(true);
        }
        a.source_mapping_equal_by(b, |left, right| self.equal_cells(left, right))
    }

    pub(super) fn compatible(
        &self,
        a: &InternalTuple<T>,
        b: &InternalTuple<T>,
    ) -> Result<bool, QueryEvaluationError> {
        self.token.ensure_alive()?;
        a.source_compatible_by(b, &self.initial, |left, right| {
            self.equal_cells(left, right)
        })
    }

    pub(super) fn is_disjoint(&self, a: &InternalTuple<T>, b: &InternalTuple<T>) -> bool {
        a.domain_disjoint(b)
    }

    #[cfg(test)]
    pub(super) fn with_observer(mut self, observer: SourceContextObserver<'a, T>) -> Self {
        self.observer = Some(observer);
        self
    }

    #[cfg(test)]
    fn observe(&self, event: SourceContextEvent<T>) {
        if let Some(observer) = &self.observer {
            observer(event);
        }
    }
}

impl<T: Clone> SourceContextRuntime<'_, T> {
    pub(super) fn observe_query_context_start(&self, _current: &InternalTuple<T>) {
        #[cfg(test)]
        self.observe(SourceContextEvent::QueryContextStart {
            current: _current.clone(),
        });
    }

    pub(super) fn observe_values_solution(&self, _row: &InternalTuple<T>) {
        #[cfg(test)]
        self.observe(SourceContextEvent::ValuesSolution { row: _row.clone() });
    }

    pub(super) fn observe_output_row(&self, _row: &InternalTuple<T>) {
        #[cfg(test)]
        self.observe(SourceContextEvent::OutputRow {
            row: _row.clone(),
            initial: Rc::clone(&self.initial),
        });
    }
}

fn resource(operation: &'static str) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::Core(
        NumericFault::ResourceLimit { operation },
    )))
}
fn set_error(error: SourceSetError<QueryEvaluationError>) -> QueryEvaluationError {
    match error {
        SourceSetError::Callback(error) => error,
        SourceSetError::Allocation => resource("source_context_set"),
        SourceSetError::IndexOverflow => resource("source_context_index"),
    }
}

/// One actual child iterator. Observations neither replay nor prefetch it.
struct ChildRows<'a, T> {
    runtime: Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    side: SourceChildSide,
    probe: u8,
    ordinal: usize,
    rows: InternalTuplesIterator<'a, T>,
    finished: bool,
}

fn start_child<'a, T: Clone + 'a>(
    runtime: &Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    side: SourceChildSide,
    probe: u8,
    plan: &InternalTupleEvaluator<'a, T>,
    current: InternalTuple<T>,
) -> Result<ChildRows<'a, T>, QueryEvaluationError> {
    runtime.token.ensure_alive()?;
    #[cfg(test)]
    runtime.observe(SourceContextEvent::ChildStart {
        node,
        side,
        probe,
        current: current.clone(),
        initial: Rc::clone(&runtime.initial),
    });
    // A test observation may cancel at the real callback boundary.
    runtime.token.ensure_alive()?;
    let rows = plan(current)?;
    #[cfg(test)]
    runtime.observe(SourceContextEvent::ChildStarted { node, side, probe });
    Ok(ChildRows {
        runtime: Rc::clone(runtime),
        node,
        side,
        probe,
        ordinal: 0,
        rows,
        finished: false,
    })
}

impl<T: Clone> Iterator for ChildRows<'_, T> {
    type Item = Result<InternalTuple<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let result = (|| {
            self.runtime.token.ensure_alive()?;
            self.ordinal = self
                .ordinal
                .checked_add(1)
                .ok_or_else(|| resource("source_context_poll"))?;
            #[cfg(test)]
            self.runtime.observe(SourceContextEvent::ChildPoll {
                node: self.node,
                side: self.side,
                probe: self.probe,
                ordinal: self.ordinal,
            });
            self.runtime.token.ensure_alive()?;
            let result = self.rows.next().transpose();
            #[cfg(test)]
            match &result {
                Ok(Some(row)) => self.runtime.observe(SourceContextEvent::ChildRow {
                    node: self.node,
                    side: self.side,
                    probe: self.probe,
                    ordinal: self.ordinal,
                    row: row.clone(),
                }),
                Ok(None) => self.runtime.observe(SourceContextEvent::ChildEnd {
                    node: self.node,
                    side: self.side,
                    probe: self.probe,
                    ordinal: self.ordinal,
                }),
                Err(_) => self.runtime.observe(SourceContextEvent::ChildFailed {
                    node: self.node,
                    side: self.side,
                    probe: self.probe,
                    ordinal: self.ordinal,
                }),
            }
            result
        })();
        if !matches!(&result, Ok(Some(_))) {
            self.finished = true;
        }
        result.transpose()
    }
}

impl<T> Drop for ChildRows<'_, T> {
    fn drop(&mut self) {
        #[cfg(test)]
        self.runtime.observe(SourceContextEvent::ChildDropped {
            node: self.node,
            side: self.side,
            probe: self.probe,
            polls: self.ordinal,
        });
    }
}

pub(super) fn source_exists_first_row<'a, T: Clone + 'a>(
    runtime: &Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    plan: &InternalTupleEvaluator<'a, T>,
    current: InternalTuple<T>,
) -> Result<bool, QueryEvaluationError> {
    let mut child = start_child(runtime, node, SourceChildSide::Child, 0, plan, current)?;
    let result = child.next().transpose().map(|row| row.is_some());
    drop(child);
    result
}

fn insert_row<T>(
    runtime: &SourceContextRuntime<'_, T>,
    set: &mut SourceSet<InternalTuple<T>>,
    row: InternalTuple<T>,
) -> Result<(), QueryEvaluationError> {
    let hash = runtime.hash_row(&row)?;
    set.insert_hashed(
        row,
        hash,
        &mut |a, b| a.source_same_object(b),
        &mut |a, b| runtime.equal_rows(a, b),
        &mut || runtime.token.ensure_alive(),
    )
    .map_err(set_error)
}

fn start_materialized<'a, T: Clone + 'a>(
    runtime: &Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    left: &InternalTupleEvaluator<'a, T>,
    right: &InternalTupleEvaluator<'a, T>,
    from: InternalTuple<T>,
) -> Result<(ChildRows<'a, T>, SourceSet<InternalTuple<T>>), QueryEvaluationError> {
    // evaluate.py starts left before constructing set(right), but does not
    // poll left until the complete right set has been successfully made.
    let left_rows = start_child(runtime, node, SourceChildSide::Left, 0, left, from.clone())?;
    let right_rows = start_child(runtime, node, SourceChildSide::Right, 0, right, from)?;
    let mut set = SourceSet::new();
    for row in right_rows {
        insert_row(runtime, &mut set, row?)?;
    }
    #[cfg(test)]
    if runtime.observer.is_some() {
        let mut retained_rows = Vec::new();
        retained_rows
            .try_reserve(set.len())
            .map_err(|_| resource("source_context_observer"))?;
        retained_rows.extend(set.iter().cloned());
        runtime.observe(SourceContextEvent::RightSetComplete {
            node,
            retained_rows,
        });
    }
    runtime.token.ensure_alive()?;
    Ok((left_rows, set))
}

pub(super) fn source_join_evaluator<'a, T: Clone + 'a>(
    runtime: Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    mode: SourceJoinMode,
    left: InternalTupleEvaluator<'a, T>,
    right: InternalTupleEvaluator<'a, T>,
) -> InternalTupleEvaluator<'a, T> {
    if mode == SourceJoinMode::Lazy {
        return lazy_join(runtime, node, left, right);
    }
    Rc::new(move |from| {
        let (mut left_rows, set) = start_materialized(&runtime, node, &left, &right, from)?;
        let runtime = Rc::clone(&runtime);
        let mut current: Option<InternalTuple<T>> = None;
        let mut cursor = 0;
        Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(
            move || {
                (|| {
                    runtime.token.ensure_alive()?;
                    loop {
                        if current.is_none() {
                            current = left_rows.next().transpose()?;
                            if current.is_none() {
                                return Ok(None);
                            }
                            cursor = 0;
                        }
                        let a = current.as_ref().expect("current left row");
                        while let Some(b) = set
                            .next_occupied_from(&mut cursor, &mut || runtime.token.ensure_alive())
                            .map_err(set_error)?
                        {
                            if runtime.compatible(a, b)? {
                                return Ok(Some(a.source_merge(b)));
                            }
                        }
                        current = None;
                    }
                })()
                .transpose()
            },
        ))))
    })
}

fn lazy_join<'a, T: Clone + 'a>(
    runtime: Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    left: InternalTupleEvaluator<'a, T>,
    right: InternalTupleEvaluator<'a, T>,
) -> InternalTupleEvaluator<'a, T> {
    Rc::new(move |from| {
        let runtime = Rc::clone(&runtime);
        let left = Rc::clone(&left);
        let right = Rc::clone(&right);
        let mut left_rows: Option<ChildRows<'a, T>> = None;
        let mut right_rows: Option<ChildRows<'a, T>> = None;
        let mut current: Option<InternalTuple<T>> = None;
        Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(
            move || {
                (|| {
                    runtime.token.ensure_alive()?;
                    if left_rows.is_none() {
                        left_rows = Some(start_child(
                            &runtime,
                            node,
                            SourceChildSide::Left,
                            0,
                            &left,
                            from.clone(),
                        )?);
                    }
                    loop {
                        if let Some(rows) = &mut right_rows {
                            if let Some(b) = rows.next().transpose()? {
                                return Ok(Some(b.source_merge(
                                    current.as_ref().expect("right has a left row"),
                                )));
                            }
                            right_rows = None;
                        }
                        let Some(a) = left_rows
                            .as_mut()
                            .expect("left started")
                            .next()
                            .transpose()?
                        else {
                            return Ok(None);
                        };
                        let thawed = from.source_thaw(&a, &runtime.initial);
                        current = Some(a);
                        right_rows = Some(start_child(
                            &runtime,
                            node,
                            SourceChildSide::Right,
                            0,
                            &right,
                            thawed,
                        )?);
                    }
                })()
                .transpose()
            },
        ))))
    })
}

pub(super) fn source_minus_evaluator<'a, T: Clone + 'a>(
    runtime: Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    left: InternalTupleEvaluator<'a, T>,
    right: InternalTupleEvaluator<'a, T>,
) -> InternalTupleEvaluator<'a, T> {
    Rc::new(move |from| {
        let (mut left_rows, set) = start_materialized(&runtime, node, &left, &right, from)?;
        let runtime = Rc::clone(&runtime);
        Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(
            move || {
                (|| {
                    runtime.token.ensure_alive()?;
                    'left: loop {
                        let Some(a) = left_rows.next().transpose()? else {
                            return Ok(None);
                        };
                        let mut cursor = 0;
                        while let Some(b) = set
                            .next_occupied_from(&mut cursor, &mut || runtime.token.ensure_alive())
                            .map_err(set_error)?
                        {
                            // Original all(not compatible OR disjoint), in this order.
                            if runtime.compatible(&a, b)? && !runtime.is_disjoint(&a, b) {
                                continue 'left;
                            }
                        }
                        return Ok(Some(a));
                    }
                })()
                .transpose()
            },
        ))))
    })
}

pub(super) fn source_optional_evaluator<'a, T: Clone + 'a>(
    runtime: Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    left: InternalTupleEvaluator<'a, T>,
    right: InternalTupleEvaluator<'a, T>,
    left_scope: Option<Rc<[usize]>>,
    condition: SourcePredicate<'a, T>,
) -> InternalTupleEvaluator<'a, T> {
    Rc::new(move |from| {
        let runtime = Rc::clone(&runtime);
        let left = Rc::clone(&left);
        let right = Rc::clone(&right);
        let left_scope = left_scope.clone();
        let condition = Rc::clone(&condition);
        let mut left_rows: Option<ChildRows<'a, T>> = None;
        let mut right_rows: Option<ChildRows<'a, T>> = None;
        let mut current: Option<InternalTuple<T>> = None;
        let mut successful = false;
        let mut retry = false;
        Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(
            move || {
                (|| {
                    runtime.token.ensure_alive()?;
                    if left_rows.is_none() {
                        left_rows = Some(start_child(
                            &runtime,
                            node,
                            SourceChildSide::Left,
                            0,
                            &left,
                            from.clone(),
                        )?);
                    }
                    loop {
                        if current.is_none() {
                            let Some(a) = left_rows
                                .as_mut()
                                .expect("left started")
                                .next()
                                .transpose()?
                            else {
                                return Ok(None);
                            };
                            let thawed = from.source_thaw(&a, &runtime.initial);
                            current = Some(a);
                            successful = false;
                            retry = false;
                            right_rows = Some(start_child(
                                &runtime,
                                node,
                                SourceChildSide::Right,
                                1,
                                &right,
                                thawed,
                            )?);
                        }
                        if let Some(b) = right_rows
                            .as_mut()
                            .expect("probe started")
                            .next()
                            .transpose()?
                        {
                            runtime.token.ensure_alive()?;
                            let accepted = if retry {
                                condition(&b)?
                            } else {
                                condition(&b.source_forget(&from, &runtime.initial, &[]))?
                            };
                            if accepted {
                                if retry {
                                    // any() stops at its first truthy row: do not
                                    // poll or finish the rest of this second probe.
                                    right_rows = None;
                                    current = None;
                                    continue;
                                }
                                successful = true;
                                return Ok(Some(
                                    b.source_merge(current.as_ref().expect("probe left row")),
                                ));
                            }
                            continue;
                        }
                        right_rows = None;
                        if retry {
                            return Ok(current.take());
                        }
                        if successful {
                            current = None;
                            continue;
                        }
                        let Some(scope) = &left_scope else {
                            return Ok(current.take());
                        };
                        let a = current.as_ref().expect("probe left row");
                        let remembered = a.source_project(scope);
                        let thawed = from.source_thaw(&remembered, &runtime.initial);
                        #[cfg(test)]
                        runtime.observe(SourceContextEvent::OptionalRetry {
                            node,
                            left_row: a.clone(),
                            remembered_row: remembered,
                            thawed_current: thawed.clone(),
                            initial: Rc::clone(&runtime.initial),
                        });
                        runtime.token.ensure_alive()?;
                        retry = true;
                        right_rows = Some(start_child(
                            &runtime,
                            node,
                            SourceChildSide::Right,
                            2,
                            &right,
                            thawed,
                        )?);
                    }
                })()
                .transpose()
            },
        ))))
    })
}

pub(super) fn source_distinct_evaluator<'a, T: Clone + 'a>(
    runtime: Rc<SourceContextRuntime<'a, T>>,
    node: usize,
    child: InternalTupleEvaluator<'a, T>,
) -> InternalTupleEvaluator<'a, T> {
    Rc::new(move |from| {
        let runtime = Rc::clone(&runtime);
        let child = Rc::clone(&child);
        let mut rows: Option<ChildRows<'a, T>> = None;
        let mut pending: Option<InternalTuple<T>> = None;
        let mut done = SourceSet::new();
        Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(
            move || {
                (|| {
                    runtime.token.ensure_alive()?;
                    if let Some(row) = pending.take() {
                        #[cfg(test)]
                        let observed = row.clone();
                        // Source generator performs done.add(x) only after yield
                        // resumes. Dropping under LIMIT does not execute this add.
                        insert_row(&runtime, &mut done, row)?;
                        #[cfg(test)]
                        runtime.observe(SourceContextEvent::DistinctAdd {
                            node,
                            row: observed,
                        });
                        runtime.token.ensure_alive()?;
                    }
                    if rows.is_none() {
                        rows = Some(start_child(
                            &runtime,
                            node,
                            SourceChildSide::Child,
                            0,
                            &child,
                            from.clone(),
                        )?);
                    }
                    loop {
                        let Some(row) = rows
                            .as_mut()
                            .expect("distinct child started")
                            .next()
                            .transpose()?
                        else {
                            return Ok(None);
                        };
                        let hash = runtime.hash_row(&row)?;
                        let present = done
                            .contains_hashed(
                                &row,
                                hash,
                                &mut |a, b| a.source_same_object(b),
                                &mut |a, b| runtime.equal_rows(a, b),
                                &mut || runtime.token.ensure_alive(),
                            )
                            .map_err(set_error)?;
                        #[cfg(test)]
                        runtime.observe(SourceContextEvent::DistinctContains {
                            node,
                            row: row.clone(),
                            present,
                        });
                        runtime.token.ensure_alive()?;
                        if !present {
                            pending = Some(row.clone());
                            return Ok(Some(row));
                        }
                    }
                })()
                .transpose()
            },
        ))))
    })
}

/// update.py evalDeleteWhere uses a materialized list, not evalJoin's set.
/// Keep order/duplicates and materialize each right side before pulling left.
pub(super) fn source_join_ordered_list_evaluator<'a,T:Clone+'a>(runtime:Rc<SourceContextRuntime<'a,T>>,left:InternalTupleEvaluator<'a,T>,right:InternalTupleEvaluator<'a,T>)->InternalTupleEvaluator<'a,T> {
    Rc::new(move |from| {
        let mut left=left(from.clone())?;
        let mut right_rows=Vec::new();
        for row in right(from)? {
            runtime.token.ensure_alive()?;
            if right_rows.len()>=100_000 {return Err(resource("source_update_solution_budget"));}
            right_rows.push(row?);
        }
        let mut current:Option<InternalTuple<T>>=None;let mut at=0;
        let runtime=runtime.clone();
        Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(move || {
            (|| {
                runtime.token.ensure_alive()?;
                loop {
                    if current.is_none(){current=left.next().transpose()?;at=0;}
                    let Some(row)=&current else {return Ok(None)};
                    while let Some(other)=right_rows.get(at) {
                        at+=1;
                        if runtime.compatible(row,other)? {return Ok(Some(row.source_merge(other)));}
                    }
                    current=None;
                }
            })().transpose()
        }))))
    })
}

#[cfg(test)]
mod optional_scope_receiving;
