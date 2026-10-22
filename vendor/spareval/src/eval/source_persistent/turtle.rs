//! Bounded Turtle read view of the existing source Memory owner.
//! Reuses source object caches, pattern indexes and CPython sort schedule.
use super::lifecycle::view;
use super::*;
use crate::eval::source_sort::{SourceSortError, source_sort_indices};
use crate::numeric::SourceLiteralCacheKind;
use std::cell::Cell;

/// Actual retained value, including original literal metadata/cache.
#[derive(Clone)]
pub struct SourceTurtleTerm(RawExpressionValue);
impl SourceTurtleTerm {
    pub fn term(&self) -> Result<SourceTermInput<'_>, QueryEvaluationError> {
        view(&self.0)
    }
    /// Only called for the original _PLAIN_LITERAL_TYPES branch. Unknown
    /// converter facts must not be upgraded by reparsing a normalized RDF term.
    pub fn has_cached_value(&self) -> Result<bool, QueryEvaluationError> {
        let LiteralSourceState::Literal(m) = self.0.literal_source() else {
            return Err(error("source_turtle_expected_literal"));
        };
        match &m.cached_value_kind {
            SourceFact::Known(
                SourceLiteralCacheKind::None | SourceLiteralCacheKind::UnmappedDatatype,
            ) => Ok(false),
            SourceFact::Known(_) => Ok(true),
            SourceFact::Unknown => Err(error("source_turtle_literal_cache_unreceived")),
        }
    }
}

/// Read-only view, not a second authoritative dataset or deserializable state.
pub struct SourceTurtleView {
    graph: SourceMemoryGraph,
    scope: crate::dataset::source_memory::SourceGraphTerm,
    work: Cell<usize>,
    deadline: Option<std::time::Instant>,
}
impl SourceDatasetState {
    pub fn turtle_view(
        &mut self,
        graph_name: Option<&str>,
        max_quads: usize,
        max_bytes: usize,
        max_work: usize,
    ) -> Result<SourceTurtleView, QueryEvaluationError> {
        self.turtle_view_until(graph_name, max_quads, max_bytes, max_work, None)
    }
    pub fn turtle_view_until(
        &mut self,
        graph_name: Option<&str>,
        max_quads: usize,
        max_bytes: usize,
        max_work: usize,
        deadline: Option<std::time::Instant>,
    ) -> Result<SourceTurtleView, QueryEvaluationError> {
        let check = || {
            if deadline.is_some_and(|at| std::time::Instant::now() >= at) {
                Err(error("source_turtle_deadline"))
            } else {
                Ok(())
            }
        };
        check()?;
        // Keep the existing selected-context registration/commit boundary.
        self.visit_serialization_rows(false, graph_name, max_quads, max_bytes, |_| check())?;
        check()?;
        let graph = self.graph(None)?;
        let scope = if let Some(name) = graph_name {
            graph
                .admit_recorded_source(graph_value(SourceTermInput::Iri(name), graph.session())?)
                .map_err(error)?
        } else {
            graph.default_context().map_err(error)?
        };
        Ok(SourceTurtleView {
            graph,
            scope,
            work: Cell::new(max_work),
            deadline,
        })
    }
}
impl SourceTurtleView {
    pub fn charge(&self, amount: usize) -> Result<(), QueryEvaluationError> {
        if self
            .deadline
            .is_some_and(|at| std::time::Instant::now() >= at)
        {
            return Err(error("source_turtle_deadline"));
        }
        let left = self
            .work
            .get()
            .checked_sub(amount)
            .ok_or_else(|| error("source_turtle_work_budget"))?;
        self.work.set(left);
        Ok(())
    }
    pub fn iri(&self, value: &str) -> Result<SourceTurtleTerm, QueryEvaluationError> {
        Ok(SourceTurtleTerm(graph_value(
            SourceTermInput::Iri(value),
            self.graph.session(),
        )?))
    }
    pub fn scan(
        &self,
        pattern: [Option<&SourceTurtleTerm>; 3],
    ) -> Result<Vec<[SourceTurtleTerm; 3]>, QueryEvaluationError> {
        self.charge(1)?;
        let mut keys = Vec::with_capacity(3);
        for value in pattern {
            keys.push(
                value
                    .map(|v| self.graph.admit_recorded_source(v.0.clone()))
                    .transpose()
                    .map_err(error)?,
            );
        }
        let rows = self
            .graph
            .scan_context(
                Some(&self.scope),
                [keys[0].clone(), keys[1].clone(), keys[2].clone()],
            )
            .map_err(error)?;
        self.charge(rows.len())?;
        rows.into_iter()
            .map(|row| {
                self.charge(0)?;
                let [a, b, c] = row.map(|v| {
                    self.graph
                        .externalize_source(v)
                        .map(SourceTurtleTerm)
                        .map_err(error)
                });
                Ok([a?, b?, c?])
            })
            .collect()
    }
    pub fn truth(&self, value: &SourceTurtleTerm) -> Result<bool, QueryEvaluationError> {
        match value.term()? {
            SourceTermInput::Literal { .. } => self
                .graph
                .session()
                .source_literal_python_truth(&value.0)
                .map_err(error),
            t => Ok(!t.cache_parts().1.is_empty()),
        }
    }
    pub fn less(
        &self,
        left: &SourceTurtleTerm,
        right: &SourceTurtleTerm,
    ) -> Result<bool, QueryEvaluationError> {
        self.charge(0)?;
        self.graph
            .session()
            .source_turtle_node_less(&left.0, &right.0)
            .map_err(error)
    }
    /// Source list.sort, or tuple (is_blank, references, subject). The original
    /// graph-node rank differs from SPARQL ORDER and derived Rust term Ord.
    pub fn sort(
        &self,
        values: &mut Vec<SourceTurtleTerm>,
        prefixes: Option<&[(bool, usize)]>,
    ) -> Result<(), QueryEvaluationError> {
        if prefixes.is_some_and(|p| p.len() != values.len()) {
            return Err(error("source_turtle_sort_prefix_length"));
        }
        self.charge(values.len())?;
        let mut order: Vec<_> = (0..values.len()).collect();
        source_sort_indices(
            &mut order,
            false,
            |a, b| {
                if let Some(p) = prefixes {
                    if p[a] != p[b] {
                        return Ok(p[a] < p[b]);
                    }
                }
                self.graph
                    .session()
                    .source_turtle_node_less(&values[a].0, &values[b].0)
                    .map_err(error)
            },
            || self.charge(1),
        )
        .map_err(|e| match e {
            SourceSortError::Callback(e) => e,
            SourceSortError::Allocation => error("source_turtle_sort_allocation"),
            SourceSortError::IndexOverflow => error("source_turtle_sort_index"),
        })?;
        let mut result = Vec::new();
        result.try_reserve_exact(values.len()).map_err(error)?;
        result.extend(order.into_iter().map(|i| values[i].clone()));
        *values = result;
        Ok(())
    }
}
