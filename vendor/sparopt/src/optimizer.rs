use crate::algebra::{
    AggregateExpression, Expression, GraphPattern, JoinAlgorithm, LeftJoinAlgorithm,
    MinusAlgorithm, OrderExpression,
};
use crate::type_inference::{
    VariableType, VariableTypes, infer_expression_type, infer_graph_pattern_types,
};
use oxrdf::Variable;
use spargebra::algebra::PropertyPathExpression;
use spargebra::term::{GroundTermPattern, NamedNodePattern};
use std::cmp::{max, min};

pub struct Optimizer;

/// Full structural check, not the output-variable walker used for join keys.
fn contains_source_group(pattern: &GraphPattern) -> bool {
    match pattern {
        GraphPattern::SourceGroup { .. }
        | GraphPattern::SourceValues { .. }
        | GraphPattern::SourceOrderBy { .. }
        | GraphPattern::SourceContext { .. } => true,
        GraphPattern::QuadPattern { .. }
        | GraphPattern::Path { .. }
        | GraphPattern::Graph { .. }
        | GraphPattern::Values { .. } => false,
        GraphPattern::Join { left, right, .. } | GraphPattern::Minus { left, right, .. } => {
            contains_source_group(left) || contains_source_group(right)
        }
        #[cfg(feature = "sep-0006")]
        GraphPattern::Lateral { left, right } => {
            contains_source_group(left) || contains_source_group(right)
        }
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
            ..
        } => {
            contains_source_group(left)
                || contains_source_group(right)
                || expression_contains_source_group(expression)
        }
        GraphPattern::Union { inner } => inner.iter().any(contains_source_group),
        GraphPattern::Filter { inner, expression }
        | GraphPattern::Extend {
            inner, expression, ..
        } => contains_source_group(inner) || expression_contains_source_group(expression),
        GraphPattern::OrderBy { inner, expression } => {
            contains_source_group(inner)
                || expression.iter().any(|expression| {
                    let (OrderExpression::Asc(expression) | OrderExpression::Desc(expression)) =
                        expression;
                    expression_contains_source_group(expression)
                })
        }
        GraphPattern::Group {
            inner, aggregates, ..
        } => {
            contains_source_group(inner)
                || aggregates.iter().any(|(_, aggregate)| match aggregate {
                    AggregateExpression::CountSolutions { .. } => false,
                    AggregateExpression::FunctionCall { expr, .. } => {
                        expression_contains_source_group(expr)
                    }
                })
        }
        GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Service { inner, .. } => contains_source_group(inner),
    }
}

fn expression_contains_source_group(expression: &Expression) -> bool {
    match expression {
        Expression::SourceLiteral(_)
        | Expression::SourceExists(_)
        | Expression::SourceSyntax(_)
        | Expression::SourceFunction { .. } => true,
        Expression::NamedNode(_)
        | Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::Bound(_) => false,
        Expression::Exists(pattern) => contains_source_group(pattern),
        Expression::Or(expressions)
        | Expression::And(expressions)
        | Expression::Coalesce(expressions)
        | Expression::FunctionCall(_, expressions) => {
            expressions.iter().any(expression_contains_source_group)
        }
        Expression::Equal(a, b)
        | Expression::SourceNotEqual(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            expression_contains_source_group(a) || expression_contains_source_group(b)
        }
        Expression::UnaryPlus(inner) | Expression::UnaryMinus(inner) | Expression::Not(inner) => {
            expression_contains_source_group(inner)
        }
        Expression::If(a, b, c) => {
            expression_contains_source_group(a)
                || expression_contains_source_group(b)
                || expression_contains_source_group(c)
        }
        Expression::AdditiveChain { first, tail } => {
            expression_contains_source_group(first)
                || tail
                    .iter()
                    .any(|(_, expression)| expression_contains_source_group(expression))
        }
        Expression::MultiplicativeChain { first, tail } => {
            expression_contains_source_group(first)
                || tail
                    .iter()
                    .any(|(_, expression)| expression_contains_source_group(expression))
        }
    }
}

impl Optimizer {
    pub fn optimize_graph_pattern(pattern: GraphPattern) -> GraphPattern {
        // Even a SourceGroup nested in EXISTS makes optimizer rewrites unsafe:
        // evaluation order, empty explicit groups and aggregate occurrences
        // are observable. Literal occurrence identity requires the same complete
        // bypass, including SourceValues and leaves nested in EXISTS. The private
        // prepared evaluator also opts out.
        if contains_source_group(&pattern) {
            return pattern;
        }
        let pattern = Self::normalize_pattern(pattern, &VariableTypes::default());
        let pattern = Self::reorder_joins(pattern, &VariableTypes::default());
        Self::push_filters(pattern, Vec::new(), &VariableTypes::default())
    }

    /// Normalize the pattern, discarding any join ordering information
    fn normalize_pattern(pattern: GraphPattern, input_types: &VariableTypes) -> GraphPattern {
        match pattern {
            GraphPattern::QuadPattern {
                subject,
                predicate,
                object,
                graph_name,
            } => GraphPattern::QuadPattern {
                subject,
                predicate,
                object,
                graph_name,
            },
            GraphPattern::Path {
                subject,
                path,
                object,
                graph_name,
            } => GraphPattern::Path {
                subject,
                path,
                object,
                graph_name,
            },
            GraphPattern::Graph { graph_name } => GraphPattern::Graph { graph_name },
            GraphPattern::Join {
                left,
                right,
                algorithm,
            } => GraphPattern::join(
                Self::normalize_pattern(*left, input_types),
                Self::normalize_pattern(*right, input_types),
                algorithm,
            ),
            GraphPattern::LeftJoin {
                left,
                right,
                expression,
                algorithm,
            } => {
                let left = Self::normalize_pattern(*left, input_types);
                let right = Self::normalize_pattern(*right, input_types);
                let mut inner_types = infer_graph_pattern_types(&left, input_types.clone());
                inner_types.intersect_with(infer_graph_pattern_types(&right, input_types.clone()));
                GraphPattern::left_join(
                    left,
                    right,
                    Self::normalize_expression(expression, &inner_types),
                    algorithm,
                )
            }
            #[cfg(feature = "sep-0006")]
            GraphPattern::Lateral { left, right } => {
                let left = Self::normalize_pattern(*left, input_types);
                let left_types = infer_graph_pattern_types(&left, input_types.clone());
                let right = Self::normalize_pattern(*right, &left_types);
                GraphPattern::lateral(left, right)
            }
            GraphPattern::Filter { inner, expression } => {
                let inner = Self::normalize_pattern(*inner, input_types);
                let inner_types = infer_graph_pattern_types(&inner, input_types.clone());
                let expression = Self::normalize_expression(expression, &inner_types);
                let expression_type = infer_expression_type(&expression, &inner_types);
                if expression_type == VariableType::UNDEF {
                    GraphPattern::empty()
                } else {
                    GraphPattern::filter(inner, expression)
                }
            }
            GraphPattern::Union { inner } => GraphPattern::union_all(
                inner
                    .into_iter()
                    .map(|e| Self::normalize_pattern(e, input_types)),
            ),
            GraphPattern::Extend {
                inner,
                variable,
                expression,
            } => {
                let inner = Self::normalize_pattern(*inner, input_types);
                let inner_types = infer_graph_pattern_types(&inner, input_types.clone());
                let expression = Self::normalize_expression(expression, &inner_types);
                let expression_type = infer_expression_type(&expression, &inner_types);
                if expression_type == VariableType::UNDEF {
                    // TODO: valid?
                    inner
                } else {
                    GraphPattern::extend(inner, variable, expression)
                }
            }
            GraphPattern::Minus {
                left,
                right,
                algorithm,
            } => GraphPattern::minus(
                Self::normalize_pattern(*left, input_types),
                Self::normalize_pattern(*right, input_types),
                algorithm,
            ),
            GraphPattern::Values {
                variables,
                bindings,
            } => GraphPattern::values(variables, bindings),
            GraphPattern::OrderBy { inner, expression } => {
                let inner = Self::normalize_pattern(*inner, input_types);
                let inner_types = infer_graph_pattern_types(&inner, input_types.clone());
                GraphPattern::order_by(
                    inner,
                    expression
                        .into_iter()
                        .map(|e| match e {
                            OrderExpression::Asc(e) => {
                                OrderExpression::Asc(Self::normalize_expression(e, &inner_types))
                            }
                            OrderExpression::Desc(e) => {
                                OrderExpression::Desc(Self::normalize_expression(e, &inner_types))
                            }
                        })
                        .collect(),
                )
            }
            GraphPattern::Project { inner, variables } => {
                GraphPattern::project(Self::normalize_pattern(*inner, input_types), variables)
            }
            GraphPattern::Distinct { inner } => {
                GraphPattern::distinct(Self::normalize_pattern(*inner, input_types))
            }
            GraphPattern::Reduced { inner } => {
                GraphPattern::reduced(Self::normalize_pattern(*inner, input_types))
            }
            GraphPattern::Slice {
                inner,
                start,
                length,
            } => GraphPattern::slice(Self::normalize_pattern(*inner, input_types), start, length),
            GraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => {
                // TODO: min, max and sample don't care about DISTINCT
                GraphPattern::group(
                    Self::normalize_pattern(*inner, input_types),
                    variables,
                    aggregates,
                )
            }
            GraphPattern::SourceGroup { .. }
            | GraphPattern::SourceValues { .. }
            | GraphPattern::SourceOrderBy { .. }
            | GraphPattern::SourceContext { .. }
            | GraphPattern::Service { .. } => {
                // We leave this problem to the remote SPARQL endpoint
                pattern
            }
        }
    }

    fn normalize_expression(expression: Expression, types: &VariableTypes) -> Expression {
        match expression {
            // Preserve the entire source sequence, including every nested
            // boundary. This conservative arm does not qualify optimization
            // for nullable semantics; that private route bypasses the optimizer.
            expression @ (Expression::AdditiveChain { .. }
            | Expression::MultiplicativeChain { .. }) => expression,
            Expression::NamedNode(node) => node.into(),
            Expression::Literal(literal) => literal.into(),
            Expression::SourceLiteral(literal) => Expression::SourceLiteral(literal),
            Expression::SourceExists(value) => Expression::SourceExists(value),
            Expression::SourceSyntax(value) => Expression::SourceSyntax(value),
            value @ Expression::SourceFunction { .. } => value,
            Expression::Variable(variable) => variable.into(),
            Expression::Or(inner) => Expression::or_all(
                inner
                    .into_iter()
                    .map(|e| Self::normalize_expression(e, types)),
            ),
            Expression::And(inner) => Expression::and_all(
                inner
                    .into_iter()
                    .map(|e| Self::normalize_expression(e, types)),
            ),
            Expression::SourceNotEqual(left, right) => Expression::SourceNotEqual(
                Box::new(Self::normalize_expression(*left, types)),
                Box::new(Self::normalize_expression(*right, types)),
            ),
            Expression::Equal(left, right) => {
                let left = Self::normalize_expression(*left, types);
                let left_types = infer_expression_type(&left, types);
                let right = Self::normalize_expression(*right, types);
                let right_types = infer_expression_type(&right, types);
                #[allow(unused_mut, clippy::allow_attributes)]
                let mut must_use_equal = left_types.literal && right_types.literal;
                #[cfg(feature = "sparql-12")]
                {
                    must_use_equal = must_use_equal || left_types.triple && right_types.triple;
                }
                if must_use_equal {
                    Expression::equal(left, right)
                } else {
                    Expression::same_term(left, right)
                }
            }
            Expression::SameTerm(left, right) => Expression::same_term(
                Self::normalize_expression(*left, types),
                Self::normalize_expression(*right, types),
            ),
            Expression::Greater(left, right) => Expression::greater(
                Self::normalize_expression(*left, types),
                Self::normalize_expression(*right, types),
            ),
            Expression::GreaterOrEqual(left, right) => Expression::greater_or_equal(
                Self::normalize_expression(*left, types),
                Self::normalize_expression(*right, types),
            ),
            Expression::Less(left, right) => Expression::less(
                Self::normalize_expression(*left, types),
                Self::normalize_expression(*right, types),
            ),
            Expression::LessOrEqual(left, right) => Expression::less_or_equal(
                Self::normalize_expression(*left, types),
                Self::normalize_expression(*right, types),
            ),
            Expression::Add(left, right) => {
                Self::normalize_expression(*left, types) + Self::normalize_expression(*right, types)
            }
            Expression::Subtract(left, right) => {
                Self::normalize_expression(*left, types) - Self::normalize_expression(*right, types)
            }
            Expression::Multiply(left, right) => {
                Self::normalize_expression(*left, types) * Self::normalize_expression(*right, types)
            }
            Expression::Divide(left, right) => {
                Self::normalize_expression(*left, types) / Self::normalize_expression(*right, types)
            }
            Expression::UnaryPlus(inner) => {
                Expression::unary_plus(Self::normalize_expression(*inner, types))
            }
            Expression::UnaryMinus(inner) => -Self::normalize_expression(*inner, types),
            Expression::Not(inner) => !Self::normalize_expression(*inner, types),
            Expression::Exists(inner) => Expression::exists(Self::normalize_pattern(*inner, types)),
            Expression::Bound(variable) => {
                let t = types.get(&variable);
                if !t.undef {
                    true.into()
                } else if t == VariableType::UNDEF {
                    false.into()
                } else {
                    Expression::Bound(variable)
                }
            }
            Expression::If(cond, then, els) => Expression::if_cond(
                Self::normalize_expression(*cond, types),
                Self::normalize_expression(*then, types),
                Self::normalize_expression(*els, types),
            ),
            Expression::Coalesce(inners) => Expression::coalesce(
                inners
                    .into_iter()
                    .map(|e| Self::normalize_expression(e, types))
                    .collect(),
            ),
            Expression::FunctionCall(name, args) => Expression::call(
                name,
                args.into_iter()
                    .map(|e| Self::normalize_expression(e, types))
                    .collect(),
            ),
        }
    }

    fn push_filters(
        pattern: GraphPattern,
        mut filters: Vec<Expression>,
        input_types: &VariableTypes,
    ) -> GraphPattern {
        match pattern {
            GraphPattern::QuadPattern { .. }
            | GraphPattern::Path { .. }
            | GraphPattern::Graph { .. }
            | GraphPattern::Values { .. } => {
                GraphPattern::filter(pattern, Expression::and_all(filters))
            }
            GraphPattern::Join {
                left,
                right,
                algorithm,
            } => {
                let left_types = infer_graph_pattern_types(&left, input_types.clone());
                let right_types = infer_graph_pattern_types(&right, input_types.clone());
                let mut left_filters = Vec::new();
                let mut right_filters = Vec::new();
                let mut final_filters = Vec::new();
                for filter in filters {
                    let push_left = are_all_expression_variables_bound(&filter, &left_types);
                    let push_right = are_all_expression_variables_bound(&filter, &right_types);
                    if push_left {
                        if push_right {
                            left_filters.push(filter.clone());
                            right_filters.push(filter);
                        } else {
                            left_filters.push(filter);
                        }
                    } else if push_right {
                        right_filters.push(filter);
                    } else {
                        final_filters.push(filter);
                    }
                }
                GraphPattern::filter(
                    GraphPattern::join(
                        Self::push_filters(*left, left_filters, input_types),
                        Self::push_filters(*right, right_filters, input_types),
                        algorithm,
                    ),
                    Expression::and_all(final_filters),
                )
            }
            #[cfg(feature = "sep-0006")]
            GraphPattern::Lateral { left, right } => {
                let left_types = infer_graph_pattern_types(&left, input_types.clone());
                let mut left_filters = Vec::new();
                let mut right_filters = Vec::new();
                for filter in filters {
                    let push_left = are_all_expression_variables_bound(&filter, &left_types);
                    if push_left {
                        left_filters.push(filter);
                    } else {
                        right_filters.push(filter);
                    }
                }
                let left = Self::push_filters(*left, left_filters, input_types);
                let right = Self::push_filters(*right, right_filters, &left_types);
                if let GraphPattern::Filter {
                    inner: inner_right,
                    expression,
                } = right
                {
                    // We prefer to have filter out of the lateral rather than inside the right part
                    GraphPattern::filter(GraphPattern::lateral(left, *inner_right), expression)
                } else {
                    GraphPattern::lateral(left, right)
                }
            }
            GraphPattern::LeftJoin {
                left,
                right,
                expression,
                algorithm,
            } => {
                let left_types = infer_graph_pattern_types(&left, input_types.clone());
                let right_types = infer_graph_pattern_types(&right, input_types.clone());
                let mut left_filters = Vec::new();
                let mut right_filters = Vec::new();
                let mut final_filters = Vec::new();
                for filter in filters {
                    let push_left = are_all_expression_variables_bound(&filter, &left_types);
                    if push_left {
                        left_filters.push(filter);
                    } else {
                        final_filters.push(filter);
                    }
                }
                let expression = if expression.effective_boolean_value().is_none()
                    && (are_all_expression_variables_bound(&expression, &right_types)
                        || are_no_expression_variables_bound(&expression, &left_types))
                {
                    right_filters.push(expression);
                    true.into()
                } else {
                    expression
                };
                GraphPattern::filter(
                    GraphPattern::left_join(
                        Self::push_filters(*left, left_filters, input_types),
                        Self::push_filters(*right, right_filters, input_types),
                        expression,
                        algorithm,
                    ),
                    Expression::and_all(final_filters),
                )
            }
            GraphPattern::Minus {
                left,
                right,
                algorithm,
            } => GraphPattern::minus(
                Self::push_filters(*left, filters, input_types),
                Self::push_filters(*right, Vec::new(), input_types),
                algorithm,
            ),
            GraphPattern::Extend {
                inner,
                expression,
                variable,
            } => {
                // TODO: handle the case where the filter overrides an expression variable (should not happen in SPARQL but allowed in the algebra)
                let mut inner_filters = Vec::new();
                let mut final_filters = Vec::new();
                for filter in filters {
                    let extend_variable_used =
                        filter.used_variables().into_iter().any(|v| *v == variable);
                    if extend_variable_used {
                        final_filters.push(filter);
                    } else {
                        inner_filters.push(filter);
                    }
                }
                GraphPattern::filter(
                    GraphPattern::extend(
                        Self::push_filters(*inner, inner_filters, input_types),
                        variable,
                        expression,
                    ),
                    Expression::and_all(final_filters),
                )
            }
            GraphPattern::Filter { inner, expression } => {
                if let Expression::And(expressions) = expression {
                    filters.extend(expressions)
                } else {
                    filters.push(expression)
                }
                Self::push_filters(*inner, filters, input_types)
            }
            GraphPattern::Union { inner } => GraphPattern::union_all(
                inner
                    .into_iter()
                    .map(|c| Self::push_filters(c, filters.clone(), input_types)),
            ),
            GraphPattern::Slice {
                inner,
                start,
                length,
            } => GraphPattern::filter(
                GraphPattern::slice(
                    Self::push_filters(*inner, Vec::new(), input_types),
                    start,
                    length,
                ),
                Expression::and_all(filters),
            ),
            GraphPattern::Distinct { inner } => {
                GraphPattern::distinct(Self::push_filters(*inner, filters, input_types))
            }
            GraphPattern::Reduced { inner } => {
                GraphPattern::reduced(Self::push_filters(*inner, filters, input_types))
            }
            GraphPattern::Project { inner, variables } => {
                GraphPattern::project(Self::push_filters(*inner, filters, input_types), variables)
            }
            GraphPattern::OrderBy { inner, expression } => {
                GraphPattern::order_by(Self::push_filters(*inner, filters, input_types), expression)
            }
            GraphPattern::Service { .. } => {
                // TODO: we can be smart and push some filters
                // But we need to check the behavior of SILENT that can transform no results into a singleton
                GraphPattern::filter(pattern, Expression::and_all(filters))
            }
            GraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => GraphPattern::filter(
                GraphPattern::group(
                    Self::push_filters(*inner, Vec::new(), input_types),
                    variables,
                    aggregates,
                ),
                Expression::and_all(filters),
            ),
            GraphPattern::SourceGroup { .. }
            | GraphPattern::SourceValues { .. }
            | GraphPattern::SourceOrderBy { .. }
            | GraphPattern::SourceContext { .. } => {
                if filters.is_empty() {
                    pattern
                } else {
                    GraphPattern::Filter {
                        inner: Box::new(pattern),
                        expression: Expression::And(filters),
                    }
                }
            }
        }
    }

    fn reorder_joins(pattern: GraphPattern, input_types: &VariableTypes) -> GraphPattern {
        match pattern {
            GraphPattern::QuadPattern { .. }
            | GraphPattern::Path { .. }
            | GraphPattern::Values { .. }
            | GraphPattern::Graph { .. } => pattern,
            GraphPattern::Join { left, right, .. } => {
                // We flatten the join operation
                let mut to_reorder = Vec::new();
                let mut todo = vec![*right, *left];
                while let Some(e) = todo.pop() {
                    if let GraphPattern::Join { left, right, .. } = e {
                        todo.push(*right);
                        todo.push(*left);
                    } else {
                        to_reorder.push(Self::reorder_joins(e, input_types));
                    }
                }

                // We do first type inference
                let to_reorder_types = to_reorder
                    .iter()
                    .map(|p| infer_graph_pattern_types(p, input_types.clone()))
                    .collect::<Vec<_>>();

                // We do greedy join reordering
                let mut output_cartesian_product_joins = Vec::new();
                let mut not_yet_reordered_ids = vec![true; to_reorder.len()];
                // We look for the next connected component to reorder and pick the smallest element
                while let Some(next_entry_id) = not_yet_reordered_ids
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v)
                    .map(|(i, _)| i)
                    .min_by_key(|i| estimate_graph_pattern_size(&to_reorder[*i], input_types))
                {
                    not_yet_reordered_ids[next_entry_id] = false; // It's now done
                    let mut output = to_reorder[next_entry_id].clone();
                    let mut output_types = to_reorder_types[next_entry_id].clone();
                    // We look for an other child to join with that does not blow up the join cost
                    while let Some(next_id) = not_yet_reordered_ids
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| **v)
                        .map(|(i, _)| i)
                        .filter(|i| {
                            has_common_variables(&output_types, &to_reorder_types[*i], input_types)
                        })
                        .min_by_key(|i| {
                            // Estimation of the join cost
                            if cfg!(feature = "sep-0006")
                                && is_fit_for_for_loop_join(
                                    &to_reorder[*i],
                                    input_types,
                                    &output_types,
                                )
                            {
                                estimate_lateral_cost(
                                    &output,
                                    &output_types,
                                    &to_reorder[*i],
                                    input_types,
                                )
                            } else {
                                estimate_join_cost(
                                    &output,
                                    &to_reorder[*i],
                                    &JoinAlgorithm::HashBuildLeftProbeRight {
                                        keys: join_key_variables(
                                            &output_types,
                                            &to_reorder_types[*i],
                                            input_types,
                                        ),
                                    },
                                    input_types,
                                )
                            }
                        })
                    {
                        not_yet_reordered_ids[next_id] = false; // It's now done
                        let next = to_reorder[next_id].clone();
                        #[cfg(feature = "sep-0006")]
                        {
                            output = if is_fit_for_for_loop_join(&next, input_types, &output_types)
                            {
                                GraphPattern::lateral(output, next)
                            } else {
                                GraphPattern::join(
                                    output,
                                    next,
                                    JoinAlgorithm::HashBuildLeftProbeRight {
                                        keys: join_key_variables(
                                            &output_types,
                                            &to_reorder_types[next_id],
                                            input_types,
                                        ),
                                    },
                                )
                            };
                        }
                        #[cfg(not(feature = "sep-0006"))]
                        {
                            output = GraphPattern::join(
                                output,
                                next,
                                JoinAlgorithm::HashBuildLeftProbeRight {
                                    keys: join_key_variables(
                                        &output_types,
                                        &to_reorder_types[next_id],
                                        input_types,
                                    ),
                                },
                            );
                        }
                        output_types.intersect_with(to_reorder_types[next_id].clone());
                    }
                    output_cartesian_product_joins.push(output);
                }
                output_cartesian_product_joins
                    .into_iter()
                    .reduce(|left, right| {
                        let keys = join_key_variables(
                            &infer_graph_pattern_types(&left, input_types.clone()),
                            &infer_graph_pattern_types(&right, input_types.clone()),
                            input_types,
                        );
                        if estimate_graph_pattern_size(&left, input_types)
                            <= estimate_graph_pattern_size(&right, input_types)
                        {
                            GraphPattern::join(
                                left,
                                right,
                                JoinAlgorithm::HashBuildLeftProbeRight { keys },
                            )
                        } else {
                            GraphPattern::join(
                                right,
                                left,
                                JoinAlgorithm::HashBuildLeftProbeRight { keys },
                            )
                        }
                    })
                    .unwrap()
            }
            #[cfg(feature = "sep-0006")]
            GraphPattern::Lateral { left, right } => {
                let left_types = infer_graph_pattern_types(&left, input_types.clone());
                GraphPattern::lateral(
                    Self::reorder_joins(*left, input_types),
                    Self::reorder_joins(*right, &left_types),
                )
            }
            GraphPattern::LeftJoin {
                left,
                right,
                expression,
                ..
            } => {
                let left = Self::reorder_joins(*left, input_types);
                let left_types = infer_graph_pattern_types(&left, input_types.clone());
                let right = Self::reorder_joins(*right, input_types);
                let right_types = infer_graph_pattern_types(&right, input_types.clone());
                #[cfg(feature = "sep-0006")]
                {
                    if is_fit_for_for_loop_join(&right, input_types, &left_types)
                        && has_common_variables(&left_types, &right_types, input_types)
                    {
                        return GraphPattern::lateral(
                            left,
                            GraphPattern::left_join(
                                GraphPattern::empty_singleton(),
                                right,
                                expression,
                                LeftJoinAlgorithm::HashBuildRightProbeLeft { keys: Vec::new() },
                            ),
                        );
                    }
                }
                GraphPattern::left_join(
                    left,
                    right,
                    expression,
                    LeftJoinAlgorithm::HashBuildRightProbeLeft {
                        keys: join_key_variables(&left_types, &right_types, input_types),
                    },
                )
            }
            GraphPattern::Minus { left, right, .. } => {
                let left = Self::reorder_joins(*left, input_types);
                let left_types = infer_graph_pattern_types(&left, input_types.clone());
                let right = Self::reorder_joins(*right, input_types);
                let right_types = infer_graph_pattern_types(&right, input_types.clone());
                GraphPattern::minus(
                    left,
                    right,
                    MinusAlgorithm::HashBuildRightProbeLeft {
                        keys: join_key_variables(&left_types, &right_types, input_types),
                    },
                )
            }
            GraphPattern::Extend {
                inner,
                expression,
                variable,
            } => GraphPattern::extend(
                Self::reorder_joins(*inner, input_types),
                variable,
                expression,
            ),
            GraphPattern::Filter { inner, expression } => {
                GraphPattern::filter(Self::reorder_joins(*inner, input_types), expression)
            }
            GraphPattern::Union { inner } => GraphPattern::union_all(
                inner
                    .into_iter()
                    .map(|c| Self::reorder_joins(c, input_types)),
            ),
            GraphPattern::Slice {
                inner,
                start,
                length,
            } => GraphPattern::slice(Self::reorder_joins(*inner, input_types), start, length),
            GraphPattern::Distinct { inner } => {
                GraphPattern::distinct(Self::reorder_joins(*inner, input_types))
            }
            GraphPattern::Reduced { inner } => {
                GraphPattern::reduced(Self::reorder_joins(*inner, input_types))
            }
            GraphPattern::Project { inner, variables } => {
                GraphPattern::project(Self::reorder_joins(*inner, input_types), variables)
            }
            GraphPattern::OrderBy { inner, expression } => {
                GraphPattern::order_by(Self::reorder_joins(*inner, input_types), expression)
            }
            GraphPattern::Service { .. } => {
                // We don't do join reordering inside of SERVICE calls, we don't know about cardinalities
                pattern
            }
            GraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => GraphPattern::group(
                Self::reorder_joins(*inner, input_types),
                variables,
                aggregates,
            ),
            GraphPattern::SourceGroup { .. }
            | GraphPattern::SourceValues { .. }
            | GraphPattern::SourceOrderBy { .. }
            | GraphPattern::SourceContext { .. } => pattern,
        }
    }
}

fn is_fit_for_for_loop_join(
    pattern: &GraphPattern,
    global_input_types: &VariableTypes,
    entry_types: &VariableTypes,
) -> bool {
    // TODO: think more about it
    match pattern {
        GraphPattern::Values { .. }
        | GraphPattern::QuadPattern { .. }
        | GraphPattern::Path { .. }
        | GraphPattern::Graph { .. } => true,
        #[cfg(feature = "sep-0006")]
        GraphPattern::Lateral { left, right } => {
            is_fit_for_for_loop_join(left, global_input_types, entry_types)
                && is_fit_for_for_loop_join(right, global_input_types, entry_types)
        }
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
            ..
        } => {
            if !is_fit_for_for_loop_join(left, global_input_types, entry_types) {
                return false;
            }

            // It is not ok to transform into for loop join if right binds a variable also bound by the entry part of the for loop join
            let mut left_types = infer_graph_pattern_types(left, global_input_types.clone());
            let right_types = infer_graph_pattern_types(right, global_input_types.clone());
            if right_types.iter().any(|(variable, t)| {
                *t != VariableType::UNDEF
                    && left_types.get(variable).undef
                    && entry_types.get(variable) != VariableType::UNDEF
            }) {
                return false;
            }

            // We don't forget the final expression
            left_types.intersect_with(right_types);
            is_expression_fit_for_for_loop_join(expression, &left_types, entry_types)
        }
        GraphPattern::Union { inner } => inner
            .iter()
            .all(|i| is_fit_for_for_loop_join(i, global_input_types, entry_types)),
        GraphPattern::Filter { inner, expression } => {
            is_fit_for_for_loop_join(inner, global_input_types, entry_types)
                && is_expression_fit_for_for_loop_join(
                    expression,
                    &infer_graph_pattern_types(inner, global_input_types.clone()),
                    entry_types,
                )
        }
        GraphPattern::Extend {
            inner,
            expression,
            variable,
        } => {
            is_fit_for_for_loop_join(inner, global_input_types, entry_types)
                && entry_types.get(variable) == VariableType::UNDEF
                && is_expression_fit_for_for_loop_join(
                    expression,
                    &infer_graph_pattern_types(inner, global_input_types.clone()),
                    entry_types,
                )
        }
        GraphPattern::Join { .. }
        | GraphPattern::Minus { .. }
        | GraphPattern::Service { .. }
        | GraphPattern::OrderBy { .. }
        | GraphPattern::Distinct { .. }
        | GraphPattern::Reduced { .. }
        | GraphPattern::Slice { .. }
        | GraphPattern::Project { .. }
        | GraphPattern::Group { .. }
        | GraphPattern::SourceGroup { .. }
        | GraphPattern::SourceValues { .. }
        | GraphPattern::SourceOrderBy { .. }
        | GraphPattern::SourceContext { .. } => false,
    }
}

fn are_all_expression_variables_bound(
    expression: &Expression,
    variable_types: &VariableTypes,
) -> bool {
    expression
        .used_variables()
        .into_iter()
        .all(|v| !variable_types.get(v).undef)
}

fn are_no_expression_variables_bound(
    expression: &Expression,
    variable_types: &VariableTypes,
) -> bool {
    expression
        .used_variables()
        .into_iter()
        .all(|v| variable_types.get(v) == VariableType::UNDEF)
}

fn is_expression_fit_for_for_loop_join(
    expression: &Expression,
    input_types: &VariableTypes,
    entry_types: &VariableTypes,
) -> bool {
    match expression {
        // The ordinary RDF-only type lattice cannot establish the nullable
        // evaluation/capture behavior required for a loop-join rewrite.
        Expression::AdditiveChain { .. }
        | Expression::MultiplicativeChain { .. }
        | Expression::SourceLiteral(_)
        | Expression::SourceExists(_)
        | Expression::SourceSyntax(_)
        | Expression::SourceFunction { .. } => false,
        Expression::NamedNode(_) | Expression::Literal(_) => true,
        Expression::Variable(v) | Expression::Bound(v) => {
            !input_types.get(v).undef || entry_types.get(v) == VariableType::UNDEF
        }
        Expression::Or(inner)
        | Expression::And(inner)
        | Expression::Coalesce(inner)
        | Expression::FunctionCall(_, inner) => inner
            .iter()
            .all(|e| is_expression_fit_for_for_loop_join(e, input_types, entry_types)),
        Expression::Equal(a, b)
        | Expression::SourceNotEqual(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            is_expression_fit_for_for_loop_join(a, input_types, entry_types)
                && is_expression_fit_for_for_loop_join(b, input_types, entry_types)
        }
        Expression::UnaryPlus(e) | Expression::UnaryMinus(e) | Expression::Not(e) => {
            is_expression_fit_for_for_loop_join(e, input_types, entry_types)
        }
        Expression::If(a, b, c) => {
            is_expression_fit_for_for_loop_join(a, input_types, entry_types)
                && is_expression_fit_for_for_loop_join(b, input_types, entry_types)
                && is_expression_fit_for_for_loop_join(c, input_types, entry_types)
        }
        Expression::Exists(inner) => is_fit_for_for_loop_join(inner, input_types, entry_types),
    }
}

fn has_common_variables(
    left: &VariableTypes,
    right: &VariableTypes,
    input_types: &VariableTypes,
) -> bool {
    // TODO: we should be smart and count as shared variables FILTER(?a = ?b)
    left.iter().any(|(variable, left_type)| {
        !left_type.undef && !right.get(variable).undef && input_types.get(variable).undef
    })
}

fn join_key_variables(
    left: &VariableTypes,
    right: &VariableTypes,
    input_types: &VariableTypes,
) -> Vec<Variable> {
    left.iter()
        .filter(|(variable, left_type)| {
            !left_type.undef && !right.get(variable).undef && input_types.get(variable).undef
        })
        .map(|(variable, _)| variable.clone())
        .collect()
}

fn estimate_graph_pattern_size(pattern: &GraphPattern, input_types: &VariableTypes) -> usize {
    match pattern {
        GraphPattern::Values { bindings, .. } => bindings.len(),
        GraphPattern::QuadPattern {
            subject,
            predicate,
            object,
            ..
        } => estimate_triple_pattern_size(
            is_term_pattern_bound(subject, input_types),
            is_named_node_pattern_bound(predicate, input_types),
            is_term_pattern_bound(object, input_types),
        ),
        GraphPattern::Path {
            subject,
            path,
            object,
            ..
        } => estimate_path_size(
            is_term_pattern_bound(subject, input_types),
            path,
            is_term_pattern_bound(object, input_types),
        ),
        GraphPattern::Graph { graph_name } => {
            if is_named_node_pattern_bound(graph_name, input_types) {
                100
            } else {
                1
            }
        }
        GraphPattern::Join {
            left,
            right,
            algorithm,
        } => estimate_join_cost(left, right, algorithm, input_types),
        GraphPattern::LeftJoin {
            left,
            right,
            algorithm,
            ..
        } => match algorithm {
            LeftJoinAlgorithm::HashBuildRightProbeLeft { keys } => {
                let left_size = estimate_graph_pattern_size(left, input_types);
                max(
                    left_size,
                    left_size
                        .saturating_mul(estimate_graph_pattern_size(
                            right,
                            &infer_graph_pattern_types(right, input_types.clone()),
                        ))
                        .saturating_div(1_000_usize.saturating_pow(keys.len().try_into().unwrap())),
                )
            }
        },
        #[cfg(feature = "sep-0006")]
        GraphPattern::Lateral { left, right } => estimate_lateral_cost(
            left,
            &infer_graph_pattern_types(left, input_types.clone()),
            right,
            input_types,
        ),
        GraphPattern::Union { inner } => inner
            .iter()
            .map(|inner| estimate_graph_pattern_size(inner, input_types))
            .fold(0, usize::saturating_add),
        GraphPattern::Minus { left, .. } => estimate_graph_pattern_size(left, input_types),
        GraphPattern::Filter { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner, .. }
        | GraphPattern::Reduced { inner, .. }
        | GraphPattern::Group { inner, .. }
        | GraphPattern::Service { inner, .. } => estimate_graph_pattern_size(inner, input_types),
        GraphPattern::Slice {
            inner,
            start,
            length,
        } => {
            let inner = estimate_graph_pattern_size(inner, input_types).saturating_sub(*start);
            if let Some(length) = length {
                min(inner, *length)
            } else {
                inner
            }
        }
        GraphPattern::SourceGroup { .. }
        | GraphPattern::SourceValues { .. }
        | GraphPattern::SourceOrderBy { .. }
        | GraphPattern::SourceContext { .. } => usize::MAX,
    }
}

fn estimate_join_cost(
    left: &GraphPattern,
    right: &GraphPattern,
    algorithm: &JoinAlgorithm,
    input_types: &VariableTypes,
) -> usize {
    match algorithm {
        JoinAlgorithm::HashBuildLeftProbeRight { keys } => {
            estimate_graph_pattern_size(left, input_types)
                .saturating_mul(estimate_graph_pattern_size(right, input_types))
                .saturating_div(1_000_usize.saturating_pow(keys.len().try_into().unwrap()))
        }
    }
}
fn estimate_lateral_cost(
    left: &GraphPattern,
    left_types: &VariableTypes,
    right: &GraphPattern,
    input_types: &VariableTypes,
) -> usize {
    estimate_graph_pattern_size(left, input_types)
        .saturating_mul(estimate_graph_pattern_size(right, left_types))
}

fn estimate_triple_pattern_size(
    subject_bound: bool,
    predicate_bound: bool,
    object_bound: bool,
) -> usize {
    match (subject_bound, predicate_bound, object_bound) {
        (true, true, true) => 1,
        (true, true, false) => 10,
        (true, false, true) => 2,
        (false, true, true) => 10_000,
        (true, false, false) => 100,
        (false, false, false) => 1_000_000_000,
        (false, true, false) => 1_000_000,
        (false, false, true) => 100_000,
    }
}

fn estimate_path_size(start_bound: bool, path: &PropertyPathExpression, end_bound: bool) -> usize {
    match path {
        PropertyPathExpression::NamedNode(_) => {
            estimate_triple_pattern_size(start_bound, true, end_bound)
        }
        PropertyPathExpression::Reverse(p) => estimate_path_size(end_bound, p, start_bound),
        PropertyPathExpression::Sequence(a, b) => {
            // We do a for loop join in the best direction
            min(
                estimate_path_size(start_bound, a, false)
                    .saturating_mul(estimate_path_size(true, b, end_bound)),
                estimate_path_size(start_bound, a, true)
                    .saturating_mul(estimate_path_size(false, b, end_bound)),
            )
        }
        PropertyPathExpression::Alternative(a, b) => estimate_path_size(start_bound, a, end_bound)
            .saturating_add(estimate_path_size(start_bound, b, end_bound)),
        PropertyPathExpression::ZeroOrMore(p) => {
            if start_bound && end_bound {
                1
            } else if start_bound || end_bound {
                estimate_path_size(start_bound, p, end_bound).saturating_mul(1000)
            } else {
                1_000_000_000
            }
        }
        PropertyPathExpression::OneOrMore(p) => {
            if start_bound && end_bound {
                1
            } else {
                estimate_path_size(start_bound, p, end_bound).saturating_mul(1000)
            }
        }
        PropertyPathExpression::ZeroOrOne(p) => {
            if start_bound && end_bound {
                1
            } else if start_bound || end_bound {
                estimate_path_size(start_bound, p, end_bound)
            } else {
                1_000_000_000
            }
        }
        PropertyPathExpression::NegatedPropertySet(_) => {
            estimate_triple_pattern_size(start_bound, false, end_bound)
        }
    }
}

fn is_term_pattern_bound(pattern: &GroundTermPattern, input_types: &VariableTypes) -> bool {
    match pattern {
        GroundTermPattern::NamedNode(_)
        | GroundTermPattern::Literal(_)
        | GroundTermPattern::SourceLiteral(_) => true,
        GroundTermPattern::Variable(v) => !input_types.get(v).undef,
        #[cfg(feature = "sparql-12")]
        GroundTermPattern::Triple(t) => {
            is_term_pattern_bound(&t.subject, input_types)
                && is_named_node_pattern_bound(&t.predicate, input_types)
                && is_term_pattern_bound(&t.object, input_types)
        }
    }
}

fn is_named_node_pattern_bound(pattern: &NamedNodePattern, input_types: &VariableTypes) -> bool {
    match pattern {
        NamedNodePattern::NamedNode(_) => true,
        NamedNodePattern::Variable(v) => !input_types.get(v).undef,
    }
}

#[cfg(test)]
mod nullable_chain_tests {
    use super::*;
    use crate::algebra::{AdditiveOperator, Literal, MultiplicativeOperator};

    #[test]
    fn nullable_chains_keep_conservative_optimizer_and_type_boundaries() {
        let types = VariableTypes::default();
        let variable = Variable::new("unbound").unwrap();
        let additive = Expression::AdditiveChain {
            first: Box::new(Expression::Bound(variable.clone())),
            tail: vec![(AdditiveOperator::Add, Literal::from(1).into())],
        };
        let multiplicative = Expression::MultiplicativeChain {
            first: Box::new(additive.clone()),
            tail: vec![
                (MultiplicativeOperator::Divide, Literal::from(0).into()),
                (
                    MultiplicativeOperator::Multiply,
                    Expression::Bound(variable),
                ),
            ],
        };
        for expression in [additive, multiplicative] {
            // Bound would be normalized inside an ordinary expression. The
            // chain arm must preserve nested inputs rather than recurse/fold.
            assert_eq!(
                Optimizer::normalize_expression(expression.clone(), &types),
                expression
            );
            assert_eq!(
                infer_expression_type(&expression, &types),
                // Assert the complete conservative lattice through its public
                // fields; ANY remains private to the type-inference owner.
                VariableType {
                    undef: true,
                    named_node: true,
                    blank_node: true,
                    literal: true,
                    #[cfg(feature = "sparql-12")]
                    triple: true,
                }
            );
            assert!(!is_expression_fit_for_for_loop_join(
                &expression,
                &types,
                &types
            ));
        }
    }
}

#[cfg(test)]
mod source_group_tests {
    use super::*;
    use crate::algebra::SourceGrouping;

    fn source_group() -> GraphPattern {
        GraphPattern::SourceGroup {
            inner: Box::new(GraphPattern::empty()),
            grouping: SourceGrouping::Implicit,
            aggregates: vec![(
                Variable::new_unchecked("total"),
                AggregateExpression::CountSolutions { distinct: false },
            )],
        }
    }

    #[test]
    fn source_group_optimizer_preserves_entire_plan_including_exists() {
        let group = source_group();
        for pattern in [
            GraphPattern::Join {
                left: Box::new(GraphPattern::empty()),
                right: Box::new(group.clone()),
                algorithm: JoinAlgorithm::default(),
            },
            GraphPattern::Filter {
                inner: Box::new(GraphPattern::empty_singleton()),
                expression: Expression::Exists(Box::new(group.clone())),
            },
        ] {
            assert!(contains_source_group(&pattern));
            assert_eq!(Optimizer::optimize_graph_pattern(pattern.clone()), pattern);
        }
        let legacy = GraphPattern::Join {
            left: Box::new(GraphPattern::empty()),
            right: Box::new(GraphPattern::empty_singleton()),
            algorithm: JoinAlgorithm::default(),
        };
        assert!(!contains_source_group(&legacy));
        assert_eq!(
            Optimizer::optimize_graph_pattern(legacy),
            GraphPattern::empty()
        );
    }

    #[test]
    fn source_group_type_scope_contains_only_optional_aggregate_outputs() {
        let group = source_group();
        let types = VariableTypes::default();
        let result = infer_graph_pattern_types(&group, types.clone());
        assert_eq!(
            result.get(&Variable::new_unchecked("total")),
            VariableType {
                undef: true,
                named_node: true,
                blank_node: true,
                literal: true,
                #[cfg(feature = "sparql-12")]
                triple: true,
            }
        );
        assert_eq!(
            result.get(&Variable::new_unchecked("key")),
            VariableType::UNDEF
        );
        assert!(!is_fit_for_for_loop_join(&group, &types, &types));
    }
}

#[cfg(test)]
mod source_literal_optimizer_tests {
    use super::*;
    use crate::algebra::{Literal, ParsedSourceLiteral, SourceValueTerm};
    use spargebra::{SourceDatatype, SourceLiteralOrigin};

    fn literal() -> ParsedSourceLiteral {
        ParsedSourceLiteral {
            occurrence: None,
            rdf: Literal::new_simple_literal("a"),
            datatype: SourceDatatype::Absent,
            language_original: None,
            origin: SourceLiteralOrigin::QueryPlain,
        }
    }

    #[test]
    fn source_literal_optimizer_bypasses_entire_plan_and_nested_exists() {
        let table = GraphPattern::SourceValues {
            variables: vec![Variable::new_unchecked("x")],
            bindings: vec![],
        };
        for pattern in [
            GraphPattern::Filter {
                inner: Box::new(GraphPattern::empty()),
                expression: Expression::SourceLiteral(literal()),
            },
            GraphPattern::Join {
                left: Box::new(GraphPattern::empty()),
                right: Box::new(table.clone()),
                algorithm: JoinAlgorithm::default(),
            },
            GraphPattern::Filter {
                inner: Box::new(GraphPattern::empty_singleton()),
                expression: Expression::Exists(Box::new(table)),
            },
            GraphPattern::Filter {
                inner: Box::new(GraphPattern::empty_singleton()),
                expression: Expression::Exists(Box::new(GraphPattern::Extend {
                    inner: Box::new(GraphPattern::empty()),
                    variable: Variable::new_unchecked("copy"),
                    expression: Expression::SourceLiteral(literal()),
                })),
            },
        ] {
            assert!(contains_source_group(&pattern));
            assert_eq!(Optimizer::optimize_graph_pattern(pattern.clone()), pattern);
        }
        let legacy = GraphPattern::Filter {
            inner: Box::new(GraphPattern::empty()),
            expression: Expression::Literal(Literal::new_simple_literal("a")),
        };
        assert!(!contains_source_group(&legacy));
        assert_eq!(
            Optimizer::optimize_graph_pattern(legacy),
            GraphPattern::empty()
        );
    }

    #[test]
    fn source_literal_type_inference_makes_no_totality_or_join_rewrite_guarantee() {
        let input = VariableTypes::default();
        let expr = Expression::SourceLiteral(literal());
        let unconstrained = VariableType {
            undef: true,
            named_node: true,
            blank_node: true,
            literal: true,
            #[cfg(feature = "sparql-12")]
            triple: true,
        };
        assert_eq!(infer_expression_type(&expr, &input), unconstrained);
        assert!(!is_expression_fit_for_for_loop_join(&expr, &input, &input));
        let variable = Variable::new_unchecked("x");
        let table = GraphPattern::SourceValues {
            variables: vec![variable.clone()],
            bindings: vec![vec![Some(SourceValueTerm::Literal(literal()))]],
        };
        assert_eq!(
            infer_graph_pattern_types(&table, input.clone()).get(&variable),
            unconstrained
        );
        assert!(!is_fit_for_for_loop_join(&table, &input, &input));
    }
}

#[cfg(test)]
mod source_order_tests {
    use super::*;

    fn order() -> GraphPattern {
        GraphPattern::SourceOrderBy {
            inner: Box::new(GraphPattern::Values {
                variables: vec![Variable::new_unchecked("x")],
                bindings: vec![],
            }),
            expression: vec![OrderExpression::Asc(Expression::Variable(
                Variable::new_unchecked("missing"),
            ))],
        }
    }

    #[test]
    fn source_order_optimizer_bypasses_entire_plan_including_exists() {
        for pattern in [
            order(),
            GraphPattern::Join {
                left: Box::new(GraphPattern::empty()),
                right: Box::new(order()),
                algorithm: JoinAlgorithm::default(),
            },
            GraphPattern::Filter {
                inner: Box::new(GraphPattern::empty_singleton()),
                expression: Expression::Exists(Box::new(order())),
            },
        ] {
            assert!(contains_source_group(&pattern));
            assert_eq!(Optimizer::optimize_graph_pattern(pattern.clone()), pattern);
        }
        let legacy = GraphPattern::OrderBy {
            inner: Box::new(GraphPattern::empty()),
            expression: vec![OrderExpression::Asc(Expression::Variable(
                Variable::new_unchecked("missing"),
            ))],
        };
        assert!(!contains_source_group(&legacy));
        assert_eq!(
            Optimizer::optimize_graph_pattern(legacy),
            GraphPattern::empty()
        );
        let filter = Expression::Variable(Variable::new_unchecked("predicate"));
        let pushed =
            Optimizer::push_filters(order(), vec![filter.clone()], &VariableTypes::default());
        assert!(
            matches!(pushed, GraphPattern::Filter { inner, expression: Expression::And(expressions) } if *inner == order() && expressions == vec![filter])
        );
    }

    #[test]
    fn source_order_type_inference_keeps_child_outputs_only() {
        let pattern = order();
        let GraphPattern::SourceOrderBy { inner, .. } = &pattern else {
            panic!()
        };
        let input = VariableTypes::default();
        for name in ["x", "missing"] {
            let variable = Variable::new_unchecked(name);
            assert_eq!(
                infer_graph_pattern_types(&pattern, input.clone()).get(&variable),
                infer_graph_pattern_types(inner, input.clone()).get(&variable)
            );
        }
        assert_eq!(
            infer_graph_pattern_types(&pattern, input.clone())
                .get(&Variable::new_unchecked("missing")),
            VariableType::UNDEF
        );
        assert!(!is_fit_for_for_loop_join(&pattern, &input, &input));
        assert_eq!(estimate_graph_pattern_size(&pattern, &input), usize::MAX);
    }
}

#[cfg(test)]
mod source_context_optimizer_tests {
    use super::*;
    use spargebra::algebra::{SourceContextAnnotation, SourceVariableScope};

    #[test]
    fn source_context_optimizer_bypasses_whole_plan_and_scope_does_not_create_outputs() {
        let context = GraphPattern::SourceContext {
            inner: Box::new(GraphPattern::Filter {
                inner: Box::new(GraphPattern::Values {
                    variables: vec![Variable::new_unchecked("visible")],
                    bindings: vec![vec![None]],
                }),
                expression: true.into(),
            }),
            annotation: SourceContextAnnotation::Filter {
                scope: SourceVariableScope {
                    variables: vec![Variable::new_unchecked("scope_only")],
                },
            },
        };
        for plan in [
            context.clone(),
            GraphPattern::Join {
                left: Box::new(GraphPattern::empty()),
                right: Box::new(context.clone()),
                algorithm: JoinAlgorithm::default(),
            },
            GraphPattern::Filter {
                inner: Box::new(GraphPattern::empty_singleton()),
                expression: Expression::Exists(Box::new(context.clone())),
            },
        ] {
            assert_eq!(Optimizer::optimize_graph_pattern(plan.clone()), plan);
        }
        let input = VariableTypes::default();
        assert_eq!(
            infer_graph_pattern_types(&context, input.clone())
                .get(&Variable::new_unchecked("scope_only")),
            VariableType::UNDEF
        );
        assert!(!is_fit_for_for_loop_join(&context, &input, &input));
        assert_eq!(estimate_graph_pattern_size(&context, &input), usize::MAX);
    }
}
