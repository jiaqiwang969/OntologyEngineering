//! Explicit profile4 projection compatibility, on the genuine parsed algebra.
//! A key's presence is justified by the one actual top-level Group that every
//! emitted solution must pass through. This is not a general Python None domain.
use super::*;

const UNSUPPORTED: &str = "query_group_null_projection_scope_unsupported";

pub(super) fn projected_keys(
    query: &spargebra::Query,
    limits: QueryScopeLimits,
) -> Result<Vec<Variable>> {
    let pattern = match query {
        spargebra::Query::Select { pattern, .. }
        | spargebra::Query::Ask { pattern, .. }
        | spargebra::Query::Construct { pattern, .. }
        | spargebra::Query::Describe { pattern, .. } => pattern,
    };
    let mut groups = 0;
    let mut nodes = 0;
    inspect_pattern(pattern, 0, &mut nodes, &mut groups, limits)?;
    if groups == 0 {
        // Ordinary missing SELECT bindings retain their ordinary absent keys.
        return Ok(Vec::new());
    }
    ensure!(
        groups == 1 && matches!(query, spargebra::Query::Select { .. }),
        UNSUPPORTED
    );
    let keys = top_group_keys(pattern, false).ok_or_else(|| anyhow::anyhow!(UNSUPPORTED))?;
    Ok(keys)
}

fn top_group_keys(pattern: &GraphPattern, projected: bool) -> Option<Vec<Variable>> {
    match pattern {
        GraphPattern::Group { variables, .. } => Some(variables.clone()),
        GraphPattern::Project { inner, variables } => {
            if projected {
                return None;
            }
            let mut keys = top_group_keys(inner, true)?;
            keys.retain(|key| variables.contains(key));
            Some(keys)
        }
        GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::OrderBy { inner, .. } => top_group_keys(inner, projected),
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => {
            let keys = top_group_keys(inner, projected)?;
            // Do not silently claim Python None propagation through computed
            // expressions. The normal COUNT/SUM result alias uses its generated
            // aggregate variable, which is not a grouping key.
            if keys.contains(variable) || mentions_keys(expression, &keys) {
                return None;
            }
            Some(keys)
        }
        GraphPattern::Filter { inner, expr } => {
            let keys = top_group_keys(inner, projected)?;
            (!mentions_keys(expr, &keys)).then_some(keys)
        }
        // A group below a join/union/optional/service/graph cannot prove that
        // every output row executed it or retained its original result domain.
        _ => None,
    }
}

fn mentions_keys(expression: &Expression, keys: &[Variable]) -> bool {
    match expression {
        Expression::Variable(v) | Expression::Bound(v) => keys.contains(v),
        Expression::NamedNode(_) | Expression::Literal(_) | Expression::SourceLiteral(_) => false,
        // EXISTS may depend on a correlated key. This narrow projection profile
        // does not claim to model a nullable key's correlated Python context.
        Expression::Exists(_)
        | Expression::SourceExists(_)
        | Expression::SourceSyntax(_)
        | Expression::SourceNotEqual(_, _)
        | Expression::SourceFunction { .. } => !keys.is_empty(),
        Expression::UnaryPlus(e) | Expression::UnaryMinus(e) | Expression::Not(e) => {
            mentions_keys(e, keys)
        }
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::Equal(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => mentions_keys(a, keys) || mentions_keys(b, keys),
        Expression::If(a, b, c) => {
            mentions_keys(a, keys) || mentions_keys(b, keys) || mentions_keys(c, keys)
        }
        Expression::In(a, rest) => {
            mentions_keys(a, keys) || rest.iter().any(|e| mentions_keys(e, keys))
        }
        Expression::Coalesce(rest) | Expression::FunctionCall(_, rest) => {
            rest.iter().any(|e| mentions_keys(e, keys))
        }
        Expression::AdditiveChain { first, tail } => {
            mentions_keys(first, keys) || tail.iter().any(|(_, e)| mentions_keys(e, keys))
        }
        Expression::MultiplicativeChain { first, tail } => {
            mentions_keys(first, keys) || tail.iter().any(|(_, e)| mentions_keys(e, keys))
        }
    }
}

fn inspect_pattern(
    pattern: &GraphPattern,
    depth: usize,
    nodes: &mut usize,
    groups: &mut usize,
    limits: QueryScopeLimits,
) -> Result<()> {
    binding_ast_budget(depth, nodes, limits)?;
    match pattern {
        GraphPattern::SourceContext { .. } => {
            anyhow::bail!("source_context_private_preparation_required")
        }
        GraphPattern::SourceOrderBy { .. } => {
            anyhow::bail!("source_order_private_preparation_required")
        }
        GraphPattern::SourceValues { .. } => {
            anyhow::bail!("source_literal_private_preparation_required")
        }
        GraphPattern::SourceGroup { .. } => {
            anyhow::bail!("source_group_private_preparation_required")
        }
        GraphPattern::Bgp { .. } | GraphPattern::Path { .. } | GraphPattern::Values { .. } => {}
        GraphPattern::Join { left, right }
        | GraphPattern::Lateral { left, right }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right } => {
            inspect_pattern(left, depth + 1, nodes, groups, limits)?;
            inspect_pattern(right, depth + 1, nodes, groups, limits)?;
        }
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => {
            inspect_pattern(left, depth + 1, nodes, groups, limits)?;
            inspect_pattern(right, depth + 1, nodes, groups, limits)?;
            if let Some(e) = expression {
                inspect_expression(e, depth + 1, nodes, groups, limits)?;
            }
        }
        GraphPattern::Filter { expr, inner }
        | GraphPattern::Extend {
            expression: expr,
            inner,
            ..
        } => {
            inspect_pattern(inner, depth + 1, nodes, groups, limits)?;
            inspect_expression(expr, depth + 1, nodes, groups, limits)?;
        }
        GraphPattern::Project { inner, .. }
        | GraphPattern::Graph { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Service { inner, .. } => {
            inspect_pattern(inner, depth + 1, nodes, groups, limits)?
        }
        GraphPattern::OrderBy { inner, expression } => {
            inspect_pattern(inner, depth + 1, nodes, groups, limits)?;
            for order in expression {
                let (OrderExpression::Asc(e) | OrderExpression::Desc(e)) = order;
                inspect_expression(e, depth + 1, nodes, groups, limits)?;
            }
        }
        GraphPattern::Group {
            inner, aggregates, ..
        } => {
            *groups += 1;
            inspect_pattern(inner, depth + 1, nodes, groups, limits)?;
            for (_, aggregate) in aggregates {
                if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                    inspect_expression(expr, depth + 1, nodes, groups, limits)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn inspect_expression(
    e: &Expression,
    depth: usize,
    nodes: &mut usize,
    groups: &mut usize,
    limits: QueryScopeLimits,
) -> Result<()> {
    binding_ast_budget(depth, nodes, limits)?;
    match e {
        Expression::SourceNotEqual(_, _) => {
            anyhow::bail!("source_relation_private_preparation_required")
        }
        Expression::SourceFunction { .. } => {
            anyhow::bail!("source_function_private_preparation_required")
        }
        Expression::SourceSyntax(_) => {
            anyhow::bail!("source_syntax_private_preparation_required")
        }
        Expression::SourceExists(_) => {
            anyhow::bail!("source_exists_private_preparation_required")
        }
        Expression::SourceLiteral(_) => {
            anyhow::bail!("source_literal_private_preparation_required")
        }
        Expression::NamedNode(_)
        | Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::Bound(_) => {}
        Expression::Exists(p) => inspect_pattern(p, depth + 1, nodes, groups, limits)?,
        Expression::UnaryPlus(e) | Expression::UnaryMinus(e) | Expression::Not(e) => {
            inspect_expression(e, depth + 1, nodes, groups, limits)?
        }
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::Equal(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            inspect_expression(a, depth + 1, nodes, groups, limits)?;
            inspect_expression(b, depth + 1, nodes, groups, limits)?;
        }
        Expression::If(a, b, c) => {
            inspect_expression(a, depth + 1, nodes, groups, limits)?;
            inspect_expression(b, depth + 1, nodes, groups, limits)?;
            inspect_expression(c, depth + 1, nodes, groups, limits)?;
        }
        Expression::In(a, rest) => {
            inspect_expression(a, depth + 1, nodes, groups, limits)?;
            for e in rest {
                inspect_expression(e, depth + 1, nodes, groups, limits)?;
            }
        }
        Expression::Coalesce(rest) | Expression::FunctionCall(_, rest) => {
            for e in rest {
                inspect_expression(e, depth + 1, nodes, groups, limits)?;
            }
        }
        Expression::AdditiveChain { first, tail } => {
            inspect_expression(first, depth + 1, nodes, groups, limits)?;
            for (_, expression) in tail {
                inspect_expression(expression, depth + 1, nodes, groups, limits)?;
            }
        }
        Expression::MultiplicativeChain { first, tail } => {
            inspect_expression(first, depth + 1, nodes, groups, limits)?;
            for (_, expression) in tail {
                inspect_expression(expression, depth + 1, nodes, groups, limits)?;
            }
        }
    }
    Ok(())
}
