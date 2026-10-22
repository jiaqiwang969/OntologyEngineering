//! Source binding-context integration into the existing compiler. Expressions,
//! accumulators, dataset conversion and final decoding retain their real owners.
use super::source_context_join::*;
use super::*;
use spargebra::algebra::{
    SourceContextAnnotation, SourceGroupCondition, SourceGrouping, SourceValuesForm,
    SourceVariableScope,
};

fn invalid_origin() -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
}

/// The ordinary planner's used-variable walk intentionally stops at Project
/// and Group. A frozen source row instead uses one immutable slot domain across
/// every subquery. This walk includes expression-only and annotation variables.
pub(super) fn source_query_variables(
    pattern: &GraphPattern,
    initial: &[Variable],
) -> Vec<Variable> {
    fn add(variables: &mut Vec<Variable>, variable: &Variable) {
        if !variables.contains(variable) {
            variables.push(variable.clone());
        }
    }
    fn expression(variables: &mut Vec<Variable>, value: &Expression) {
        match value {
            Expression::NamedNode(_) | Expression::Literal(_) | Expression::SourceLiteral(_) => (),
            Expression::Variable(variable) | Expression::Bound(variable) => {
                add(variables, variable)
            }
            Expression::SourceExists(value) => {
                // Slot allocation sees the full runtime overlay, including
                // variables hidden by a subquery Project. Source scope facts
                // still come from the original mapping analysis separately.
                value
                    .syntax
                    .lookup_used_variables(&mut |variable| add(variables, variable));
                if let Some(graph) = &value.graph {
                    walk(graph, variables);
                }
            }
            Expression::SourceSyntax(syntax) => {
                // A temporary capture is never a compiled source expression;
                // the compiler rejects it. This visitor grants no provenance.
                syntax.lookup_used_variables(&mut |variable| add(variables, variable));
            }
            Expression::Exists(graph) => walk(graph, variables),
            Expression::Or(children)
            | Expression::And(children)
            | Expression::Coalesce(children)
            | Expression::FunctionCall(_, children)
            | Expression::SourceFunction {
                arguments: children,
                ..
            } => {
                for child in children {
                    expression(variables, child);
                }
            }
            Expression::Equal(left, right)
            | Expression::SourceNotEqual(left, right)
            | Expression::SameTerm(left, right)
            | Expression::Greater(left, right)
            | Expression::GreaterOrEqual(left, right)
            | Expression::Less(left, right)
            | Expression::LessOrEqual(left, right)
            | Expression::Add(left, right)
            | Expression::Subtract(left, right)
            | Expression::Multiply(left, right)
            | Expression::Divide(left, right) => {
                expression(variables, left);
                expression(variables, right);
            }
            Expression::UnaryPlus(inner)
            | Expression::UnaryMinus(inner)
            | Expression::Not(inner) => {
                expression(variables, inner);
            }
            Expression::If(condition, yes, no) => {
                expression(variables, condition);
                expression(variables, yes);
                expression(variables, no);
            }
            Expression::AdditiveChain { first, tail } => {
                expression(variables, first);
                for (_, child) in tail {
                    expression(variables, child);
                }
            }
            Expression::MultiplicativeChain { first, tail } => {
                expression(variables, first);
                for (_, child) in tail {
                    expression(variables, child);
                }
            }
        }
    }
    fn walk(pattern: &GraphPattern, variables: &mut Vec<Variable>) {
        match pattern {
            GraphPattern::SourceContext { inner, annotation } => {
                if let SourceContextAnnotation::Bgp { patterns } = annotation {
                    // The physical planner synthesizes Variables for blank
                    // nodes. Source keys retain their actual BNode type and
                    // use separately registered slots, never those aliases.
                    for pattern in patterns {
                        for term in [&pattern.subject, &pattern.object] {
                            if let spargebra::term::TermPattern::Variable(v) = term {
                                add(variables, v);
                            }
                        }
                        if let spargebra::term::NamedNodePattern::Variable(v) = &pattern.predicate {
                            add(variables, v);
                        }
                    }
                } else if let SourceContextAnnotation::PathBgp { patterns } = annotation {
                    for pattern in patterns {
                        for term in [&pattern.subject, &pattern.object] {
                            if let spargebra::term::TermPattern::Variable(v) = term {
                                add(variables, v);
                            }
                        }
                        if let spargebra::SourceTriplePredicate::Rdf(
                            spargebra::term::NamedNodePattern::Variable(v),
                        ) = &pattern.predicate
                        {
                            add(variables, v);
                        }
                    }
                } else {
                    walk(inner, variables);
                }
                if let SourceContextAnnotation::Graph {
                    name: spargebra::term::NamedNodePattern::Variable(v),
                } = annotation
                {
                    add(variables, v);
                }
                let scopes: Vec<&SourceVariableScope> = match annotation {
                    SourceContextAnnotation::LeftJoin { left_scope, .. } => vec![left_scope],
                    SourceContextAnnotation::Filter { scope }
                    | SourceContextAnnotation::Extend { scope } => vec![scope],
                    SourceContextAnnotation::Group { conditions } => conditions
                        .iter()
                        .filter_map(|condition| condition.assignment_scope.as_ref())
                        .collect(),
                    _ => Vec::new(),
                };
                for scope in scopes {
                    for variable in &scope.variables {
                        add(variables, variable);
                    }
                }
            }
            GraphPattern::Project {
                inner,
                variables: projected,
            } => {
                for variable in projected {
                    add(variables, variable);
                }
                walk(inner, variables);
            }
            GraphPattern::SourceGroup {
                inner,
                grouping,
                aggregates,
            } => {
                walk(inner, variables);
                if let SourceGrouping::Explicit(conditions) = grouping {
                    for condition in conditions {
                        match condition {
                            SourceGroupCondition::Raw(value) => expression(variables, value),
                            SourceGroupCondition::As {
                                expression: value,
                                variable,
                            } => {
                                expression(variables, value);
                                add(variables, variable);
                            }
                        }
                    }
                }
                for (variable, aggregate) in aggregates {
                    add(variables, variable);
                    if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                        expression(variables, expr);
                    }
                }
            }
            GraphPattern::Group {
                inner,
                variables: keys,
                aggregates,
            } => {
                walk(inner, variables);
                for variable in keys {
                    add(variables, variable);
                }
                for (variable, aggregate) in aggregates {
                    add(variables, variable);
                    if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                        expression(variables, expr);
                    }
                }
            }
            GraphPattern::SourceOrderBy {
                inner,
                expression: expressions,
            }
            | GraphPattern::OrderBy {
                inner,
                expression: expressions,
            } => {
                walk(inner, variables);
                for value in expressions {
                    match value {
                        OrderExpression::Asc(value) | OrderExpression::Desc(value) => {
                            expression(variables, value)
                        }
                    }
                }
            }
            GraphPattern::Join { left, right, .. } | GraphPattern::Minus { left, right, .. } => {
                walk(left, variables);
                walk(right, variables);
            }
            GraphPattern::LeftJoin {
                left,
                right,
                expression: value,
                ..
            } => {
                walk(left, variables);
                walk(right, variables);
                expression(variables, value);
            }
            #[cfg(feature = "sep-0006")]
            GraphPattern::Lateral { left, right } => {
                walk(left, variables);
                walk(right, variables);
            }
            GraphPattern::Filter {
                inner,
                expression: value,
            } => {
                walk(inner, variables);
                expression(variables, value);
            }
            GraphPattern::Extend {
                inner,
                variable,
                expression: value,
            } => {
                walk(inner, variables);
                add(variables, variable);
                expression(variables, value);
            }
            GraphPattern::Union { inner } => {
                for child in inner {
                    walk(child, variables);
                }
            }
            GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::Service { inner, .. } => {
                walk(inner, variables);
                pattern.lookup_used_variables(&mut |v| add(variables, v));
            }
            _ => pattern.lookup_used_variables(&mut |v| add(variables, v)),
        }
    }
    let mut variables = Vec::new();
    walk(pattern, &mut variables);
    for variable in initial {
        add(&mut variables, variable);
    }
    variables
}

pub(super) fn source_query_projection(
    pattern: &GraphPattern,
) -> Result<Vec<Variable>, QueryEvaluationError> {
    match pattern {
        GraphPattern::Project { variables, .. } => Ok(variables.clone()),
        GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::SourceOrderBy { inner, .. }
        | GraphPattern::OrderBy { inner, .. } => source_query_projection(inner),
        _ => Err(invalid_origin()),
    }
}

impl<'a, D: QueryableDataset<'a>> SimpleEvaluator<'a, D> {
    pub(super) fn source_context_runtime(
        &self,
    ) -> Result<Rc<SourceContextRuntime<'a, D::InternalTerm>>, QueryEvaluationError> {
        self.source_context.clone().ok_or_else(invalid_origin)
    }

    fn source_scope_slots(
        &self,
        scope: &SourceVariableScope,
    ) -> Result<Rc<[usize]>, QueryEvaluationError> {
        let runtime = self.source_context_runtime()?;
        scope
            .variables
            .iter()
            .map(|variable| {
                runtime
                    .variables
                    .iter()
                    .position(|candidate| candidate == variable)
                    .ok_or_else(invalid_origin)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Into::into)
    }

    pub(super) fn source_context_annotation_evaluator(
        &self,
        inner: &GraphPattern,
        annotation: &SourceContextAnnotation,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        let runtime = self.source_context_runtime()?;
        let node = runtime.allocate_node();
        match (annotation, inner) {
            (SourceContextAnnotation::Bgp { patterns }, inner) => {
                self.source_bgp_evaluator(inner, patterns, variables)
            }
            (SourceContextAnnotation::PathBgp { patterns }, inner) => {
                self.source_path_bgp_evaluator(inner, patterns, variables)
            }
            (SourceContextAnnotation::Graph { name }, inner) => {
                if self.source_scoped_evaluator.is_some() {
                    return self.source_named_graph_evaluator(inner,name,variables);
                }
                let codec = self
                    .dataset
                    .source_literal_codec()
                    .ok_or_else(invalid_origin)?;
                // The only sealed source Dataset currently admitted here is
                // the native immutable SingleGraph owner. This is the source
                // evalGraph/QueryContext.dataset failure, before child polls;
                // named graph execution remains separately unqualified.
                if codec.source_bgp_scan.is_none()
                    || !codec
                        .prepared_literals
                        .as_ref()
                        .is_some_and(|registry| registry.admits_graph_execution(&codec.session))
                {
                    return Err(invalid_origin());
                }
                Ok(Rc::new(|_| {
                    Ok(Box::new(std::iter::once_with(|| {
                        Err(QueryEvaluationError::Unexpected(Box::new(source_compvalue::SourceCompValueError {
                        module: "builtins",
                        class: "Exception",
                        argument: "You performed a query operation requiring a dataset (i.e. ConjunctiveGraph), but operating currently on a single graph.".to_owned(),
                    })))
                    })))
                }))
            }
            (SourceContextAnnotation::Unanalyzed { no_isolated_scope }, inner) => {
                if !self.dataset.numeric.as_ref().is_some_and(|codec| {
                    codec
                        .prepared_literals
                        .as_ref()
                        .is_some_and(|registry| registry.admits_exists_execution(&codec.session))
                }) {
                    return Err(invalid_origin());
                }
                match inner {
                    GraphPattern::Join { left, right, .. } => {
                        let (left, left_stats) = self.graph_pattern_evaluator(left, variables);
                        stats.push(left_stats);
                        let left = left?;
                        let (right, right_stats) = self.graph_pattern_evaluator(right, variables);
                        stats.push(right_stats);
                        Ok(source_join_evaluator(
                            runtime,
                            node,
                            spargebra::algebra::SourceJoinMode::MaterializedRightSet,
                            left,
                            right?,
                        ))
                    }
                    GraphPattern::LeftJoin {
                        left, right, expression, ..
                    } => {
                        let (left, left_stats) = self.graph_pattern_evaluator(left, variables);
                        stats.push(left_stats);
                        let left = left?;
                        let (right, right_stats) = self.graph_pattern_evaluator(right, variables);
                        stats.push(right_stats);
                        let right = right?;
                        // The sealed source frontend represents TrueFilter as
                        // None; the physical planner lowers only that absence
                        // to native true. User literals retain SourceLiteral.
                        let condition: SourcePredicate<'a, D::InternalTerm> =
                            if expression == &Expression::from(true) {
                                Rc::new(|_| Ok(true))
                            } else {
                                self.nullable_boolean_evaluator(expression, variables, stats)?
                            };
                        // UPDATE's translateGroupGraphPattern does not add
                        // p1._vars. Preserve None: the original optional owner
                        // performs no invented isolated-scope retry here.
                        Ok(source_optional_evaluator(
                            runtime, node, left, right, None, condition,
                        ))
                    }
                    GraphPattern::Union { .. } => {
                        // Reuse the existing source-aware, ordered materialized
                        // UNION compiler. It retains every row and propagates
                        // either branch's error before returning results.
                        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
                        stats.push(child_stats);
                        child
                    }
                    GraphPattern::Minus { left, right, .. } => {
                        let (left, left_stats) = self.graph_pattern_evaluator(left, variables);
                        stats.push(left_stats);
                        let left = left?;
                        let (right, right_stats) = self.graph_pattern_evaluator(right, variables);
                        stats.push(right_stats);
                        Ok(source_minus_evaluator(runtime, node, left, right?))
                    }
                    GraphPattern::Filter { inner, expression } => self.source_filter_evaluator(
                        inner,
                        expression,
                        None,
                        *no_isolated_scope,
                        variables,
                        stats,
                    ),
                    GraphPattern::Extend {
                        inner,
                        variable,
                        expression,
                    } => {
                        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
                        stats.push(child_stats);
                        self.source_extend_scoped_evaluator(
                            child?,
                            Some(variable),
                            expression,
                            None,
                            variables,
                            stats,
                        )
                    }
                    GraphPattern::SourceValues {
                        variables: columns,
                        bindings,
                    } => {
                        if columns.is_empty() || bindings.is_empty() {
                            return Ok(Rc::new(|_| {
                                Err(QueryEvaluationError::Unexpected(Box::new(
                                    RawNumericFault::SourceContext(
                                        crate::numeric::SourceContextFault::EmptyValuesAttribute,
                                    ),
                                )))
                            }));
                        }
                        self.source_context_values_evaluator(columns, bindings, variables)
                    }
                    GraphPattern::SourceContext { .. }
                    | GraphPattern::Project { .. }
                    | GraphPattern::Distinct { .. }
                    | GraphPattern::Slice { .. }
                    | GraphPattern::SourceOrderBy { .. } => {
                        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
                        stats.push(child_stats);
                        child
                    }
                    GraphPattern::Values {
                        variables: columns,
                        bindings,
                    } if columns.is_empty() && bindings.len() == 1 && bindings[0].is_empty() => {
                        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
                        stats.push(child_stats);
                        child
                    }
                    _ => Err(invalid_origin()),
                }
            }
            (SourceContextAnnotation::ToMultiSet, inner) => {
                // evaluate.py::evalMultiset delegates to evalPart with the
                // same context (or evalValues for its actual values child).
                // Keep the syntax boundary without reconstructing any rows,
                // source literals, owner bindings or iterator errors.
                let (inner, inner_stats) = self.graph_pattern_evaluator(inner, variables);
                stats.push(inner_stats);
                inner
            }
            (SourceContextAnnotation::Join { mode }, GraphPattern::Join { left, right, .. }) => {
                let (left, left_stats) = self.graph_pattern_evaluator(left, variables);
                stats.push(left_stats);
                let left = left?;
                let (right, right_stats) = self.graph_pattern_evaluator(right, variables);
                stats.push(right_stats);
                Ok(source_join_evaluator(runtime, node, *mode, left, right?))
            }
            (SourceContextAnnotation::Minus, GraphPattern::Minus { left, right, .. }) => {
                let (left, left_stats) = self.graph_pattern_evaluator(left, variables);
                stats.push(left_stats);
                let left = left?;
                let (right, right_stats) = self.graph_pattern_evaluator(right, variables);
                stats.push(right_stats);
                Ok(source_minus_evaluator(runtime, node, left, right?))
            }
            (
                SourceContextAnnotation::LeftJoin {
                    left_scope,
                    unconditional,
                },
                GraphPattern::LeftJoin {
                    left,
                    right,
                    expression,
                    ..
                },
            ) => {
                let (left, left_stats) = self.graph_pattern_evaluator(left, variables);
                stats.push(left_stats);
                let left = left?;
                let (right, right_stats) = self.graph_pattern_evaluator(right, variables);
                stats.push(right_stats);
                let right = right?;
                let condition: SourcePredicate<'a, D::InternalTerm> = if *unconditional {
                    Rc::new(|_| Ok(true))
                } else {
                    self.nullable_boolean_evaluator(expression, variables, stats)?
                };
                let scope = self.source_scope_slots(left_scope)?;
                Ok(source_optional_evaluator(
                    runtime,
                    node,
                    left,
                    right,
                    Some(scope),
                    condition,
                ))
            }
            (
                SourceContextAnnotation::Values { form },
                GraphPattern::SourceValues {
                    variables: columns,
                    bindings,
                },
            ) => {
                if *form == SourceValuesForm::EmptyTranslatedList {
                    return Ok(Rc::new(|_| {
                        Err(QueryEvaluationError::Unexpected(Box::new(
                            RawNumericFault::SourceContext(
                                crate::numeric::SourceContextFault::EmptyValuesAttribute,
                            ),
                        )))
                    }));
                }
                self.source_context_values_evaluator(columns, bindings, variables)
            }
            (
                SourceContextAnnotation::Filter { scope },
                GraphPattern::Filter { inner, expression },
            ) => self.source_filter_evaluator(
                inner,
                expression,
                Some(scope),
                false,
                variables,
                stats,
            ),
            (
                SourceContextAnnotation::Extend { scope },
                GraphPattern::Extend {
                    inner,
                    variable,
                    expression,
                },
            ) => {
                let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
                stats.push(child_stats);
                self.source_extend_evaluator(
                    child?,
                    Some(variable),
                    expression,
                    scope,
                    variables,
                    stats,
                )
            }
            (
                SourceContextAnnotation::Group { conditions },
                GraphPattern::SourceGroup {
                    inner,
                    grouping,
                    aggregates,
                },
            ) => self.source_context_group_evaluator(
                inner, grouping, aggregates, conditions, variables, stats,
            ),
            _ => Err(invalid_origin()),
        }
    }

    fn source_context_values_evaluator(
        &self,
        columns: &[Variable],
        bindings: &[Vec<Option<spargebra::SourceValueTerm>>],
        variables: &mut Vec<Variable>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        let runtime = self.source_context_runtime()?;
        let codec = self
            .dataset
            .source_literal_codec()
            .cloned()
            .ok_or_else(invalid_origin)?;
        let slots: Vec<_> = columns
            .iter()
            .map(|variable| encode_variable(variables, variable))
            .collect();
        // Literal occurrences are constructed once, retaining the same cached
        // object through repeated OPTIONAL probes of this actual VALUES node.
        let records: Rc<[InternalTuple<D::InternalTerm>]> = bindings
            .iter()
            .map(|binding| {
                if binding.len() != slots.len() {
                    return Err(invalid_origin());
                }
                let mut row = InternalTuple::with_capacity(runtime.variables.len());
                for (&slot, value) in slots.iter().zip(binding) {
                    if let Some(value) = value {
                        let raw = match value {
                            spargebra::SourceValueTerm::NamedNode(node) => {
                                RawExpressionValue::from_import(node.clone().into())
                            }
                            spargebra::SourceValueTerm::Literal(literal) => {
                                codec.source_literal(literal).map_err(|error| {
                                    QueryEvaluationError::Unexpected(Box::new(error))
                                })?
                            }
                        };
                        runtime.token.ensure_alive()?;
                        row.set(slot, (codec.internalize)(raw)?);
                    }
                }
                Ok(row)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into();
        Ok(Rc::new(move |from| {
            let records = Rc::clone(&records);
            let runtime = Rc::clone(&runtime);
            let mut index = 0;
            Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(
                move || {
                    while let Some(record) = records.get(index) {
                        index += 1;
                        if let Err(error) = runtime.token.ensure_alive() {
                            return Some(Err(error));
                        }
                        let mut current = from.source_push(&runtime.initial);
                        let mut compatible = true;
                        for &slot in record.source_local_order() {
                            let Some(value) = record.cell(slot) else {
                                continue;
                            };
                            match current.source_assign_by(slot, value.clone(), |left, right| {
                                runtime.equal_cells(left, right)
                            }) {
                                Ok(true) => (),
                                Ok(false) => {
                                    compatible = false;
                                    break;
                                }
                                Err(error) => return Some(Err(error)),
                            }
                        }
                        if compatible {
                            let row = current.source_solution(None);
                            runtime.observe_values_solution(&row);
                            return Some(Ok(row));
                        }
                    }
                    None
                },
            ))))
        }))
    }

    pub(super) fn source_context_project_evaluator(
        &self,
        inner: &GraphPattern,
        columns: &[Variable],
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
        stats.push(child_stats);
        let child = child?;
        let slots: Rc<[usize]> = columns
            .iter()
            .map(|variable| encode_variable(variables, variable))
            .collect::<Vec<_>>()
            .into();
        Ok(Rc::new(move |from| {
            let slots = Rc::clone(&slots);
            // Source evalProject starts its child immediately; projection alone
            // is lazy. It never reinstates initial values into the local domain.
            Ok(Box::new(
                child(from)?.map(move |row| row.map(|row| row.source_project(&slots))),
            ))
        }))
    }

    fn source_filter_evaluator(
        &self,
        inner: &GraphPattern,
        expression: &Expression,
        scope: Option<&SourceVariableScope>,
        no_isolated_scope: bool,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        let runtime = self.source_context_runtime()?;
        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
        stats.push(child_stats);
        let child = child?;
        let scope: Rc<[usize]> = scope
            .map(|scope| self.source_scope_slots(scope))
            .transpose()?
            .unwrap_or_else(|| Rc::from([]));
        let expression = self.nullable_boolean_evaluator(expression, variables, stats)?;
        Ok(Rc::new(move |from| {
            let before = from.clone();
            let runtime = Rc::clone(&runtime);
            let scope = Rc::clone(&scope);
            let expression = Rc::clone(&expression);
            Ok(Box::new(nullable::halt_after_error(
                deferred_start(Rc::clone(&child), from).filter_map(move |row| {
                    let row = match row {
                        Ok(row) => row,
                        Err(error) => return Some(Err(error)),
                    };
                    let forgotten;
                    let input = if no_isolated_scope {
                        &row
                    } else {
                        forgotten = row.source_forget(&before, &runtime.initial, &scope);
                        &forgotten
                    };
                    match expression(input) {
                        Ok(true) => Some(Ok(row)),
                        Ok(false) => None,
                        Err(error) => Some(Err(error)),
                    }
                }),
            )))
        }))
    }

    pub(super) fn source_extend_evaluator(
        &self,
        child: InternalTupleEvaluator<'a, D::InternalTerm>,
        variable: Option<&Variable>,
        expression: &Expression,
        scope: &SourceVariableScope,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        self.source_extend_scoped_evaluator(
            child,
            variable,
            expression,
            Some(scope),
            variables,
            stats,
        )
    }

    fn source_extend_scoped_evaluator(
        &self,
        child: InternalTupleEvaluator<'a, D::InternalTerm>,
        variable: Option<&Variable>,
        expression: &Expression,
        scope: Option<&SourceVariableScope>,
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        let runtime = self.source_context_runtime()?;
        let scope: Rc<[usize]> = scope
            .map(|scope| self.source_scope_slots(scope))
            .transpose()?
            .unwrap_or_else(|| Rc::from([]));
        let target = variable.map(|variable| encode_variable(variables, variable));
        let expression = self.nullable_aggregate_evaluator(expression, variables, stats)?;
        let dataset = self.dataset.clone();
        Ok(Rc::new(move |from| {
            let before = from.clone();
            let runtime = Rc::clone(&runtime);
            let scope = Rc::clone(&scope);
            let expression = expression.clone();
            let dataset = dataset.clone();
            Ok(Box::new(nullable::halt_after_error(
                deferred_start(Rc::clone(&child), from).map(move |row| {
                    let row = row?;
                    let forgotten = row.source_forget(&before, &runtime.initial, &scope);
                    match expression.evaluate(&forgotten) {
                        Ok(value) if !value.is_returned_error() => {
                            let cell = dataset.internalize_aggregate_value(value, true)?;
                            let mut assignment =
                                InternalTuple::with_capacity(runtime.variables.len());
                            if let Some(target) = target {
                                assignment.set_cell(target, cell);
                            }
                            // The None target occurs only in the bounded GROUP
                            // syntax path, whose later None key necessarily fails.
                            Ok(row.source_merge(&assignment))
                        }
                        Ok(_) | Err(EvalFailure::Sparql(_)) => Ok(row),
                        Err(EvalFailure::Fatal(error)) => Err(error),
                    }
                }),
            )))
        }))
    }
}
