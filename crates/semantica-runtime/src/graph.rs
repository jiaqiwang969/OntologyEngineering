//! Local Oxigraph execution and explicit RDF term results.
use anyhow::{Context, Result, bail, ensure};
use oxigraph::{
    io::{RdfFormat, RdfParser},
    model::{BlankNode, Dataset, NamedOrBlankNode, Term, Variable},
    sparql::{QueryResults, SparqlEvaluator},
};
use serde_json::{Value, json};
use spareval::QueryEvaluator;
use spargebra::algebra::{AggregateExpression, Expression, GraphPattern, OrderExpression};
use std::collections::{BTreeMap, BTreeSet};

/// The parser, algebra formatter, evaluator and recursive AST destruction share
/// one explicit stack budget instead of inheriting a platform's caller stack.
pub const QUERY_STACK_BYTES: usize = 8 * 1024 * 1024;

fn with_query_stack<T: Send>(work: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("semantica-query".into())
            .stack_size(QUERY_STACK_BYTES)
            .spawn_scoped(scope, work)
            .context("semantic_query_thread_unavailable")?
            .join()
            .map_err(|_| anyhow::anyhow!("semantic_query_panicked"))?
    })
}

pub fn load(data: &[u8]) -> Result<Dataset> {
    load_format(data, RdfFormat::Turtle)
}
pub fn load_format(data: &[u8], format: RdfFormat) -> Result<Dataset> {
    load_format_with_quad_budget(data, format, 100_000)
}
pub(crate) fn load_format_with_quad_budget(
    data: &[u8],
    format: RdfFormat,
    quads: usize,
) -> Result<Dataset> {
    ensure!(data.len() <= crate::MAX_BYTES, "rdf_byte_budget");
    let mut dataset = Dataset::new();
    for quad in RdfParser::from_format(format)
        .without_named_graphs()
        .for_reader(data)
    {
        dataset.insert(&quad?);
        ensure!(dataset.len() <= quads, "rdf_quad_budget");
    }
    Ok(dataset)
}
/// Parse each document separately: Turtle prefixes and XML are document scoped,
/// and blank-node labels must not accidentally identify objects in another file.
pub fn merge_sources(sources: &[(&[u8], RdfFormat)]) -> Result<Vec<u8>> {
    ensure!(sources.len() <= 64, "rdf_source_budget");
    let mut dataset = Dataset::new();
    let mut input_bytes = 0usize;
    for (index, (bytes, format)) in sources.iter().enumerate() {
        input_bytes = input_bytes
            .checked_add(bytes.len())
            .context("rdf_byte_budget")?;
        ensure!(input_bytes <= crate::MAX_BYTES, "rdf_byte_budget");
        let mut names = BTreeMap::<String, BlankNode>::new();
        let mut blank = |node: BlankNode| -> Result<BlankNode> {
            let next = names.len();
            Ok(names
                .entry(node.as_str().into())
                .or_insert(BlankNode::new(format!("source_{index}_{next}"))?)
                .clone())
        };
        for q in RdfParser::from_format(*format)
            .without_named_graphs()
            .for_reader(*bytes)
        {
            let mut q = q?;
            if let NamedOrBlankNode::BlankNode(node) = q.subject {
                q.subject = blank(node)?.into();
            }
            if let Term::BlankNode(node) = q.object {
                q.object = blank(node)?.into();
            }
            dataset.insert(&q);
            ensure!(dataset.len() <= 100_000, "rdf_quad_budget");
        }
    }
    let mut lines: Vec<_> = dataset
        .iter()
        .map(|q| format!("{} {} {} .\n", q.subject, q.predicate, q.object))
        .collect();
    lines.sort();
    let out = lines.concat().into_bytes();
    ensure!(out.len() <= crate::MAX_BYTES, "rdf_serialized_byte_budget");
    Ok(out)
}
pub fn document_prefixes(data: &[u8]) -> Result<BTreeMap<String, String>> {
    ensure!(data.len() <= crate::MAX_BYTES, "rdf_byte_budget");
    let mut parser = RdfParser::from_format(RdfFormat::Turtle).for_reader(data);
    for item in parser.by_ref() {
        item?;
    }
    Ok(parser
        .prefixes()
        .map(|(p, n)| (p.into(), n.into()))
        .collect())
}
/// Use the pinned evaluator's RDF numeric/date/string comparison, rather than
/// coercing every value through f64. An unbound result is a comparison error.
pub fn compare_terms(left: &Term, right: &Term, operator: &str) -> Result<Option<bool>> {
    ensure!(
        ["<", "<=", ">", ">="].contains(&operator),
        "rdf_comparison_operator_invalid"
    );
    with_query_stack(|| {
        let empty = Dataset::new();
        let QueryResults::Solutions(mut rows) = SparqlEvaluator::new()
            .parse_query(&format!(
                "SELECT ?left ?right (?left {operator} ?right AS ?valid) WHERE {{}}"
            ))?
            .substitute_variable(Variable::new("left")?, left.clone())
            .substitute_variable(Variable::new("right")?, right.clone())
            .on_queryable_dataset(&empty)
            .execute()?
        else {
            bail!("rdf_comparison_result_invalid");
        };
        let row = rows.next().context("rdf_comparison_row_missing")??;
        Ok(match row.get("valid") {
            Some(Term::Literal(l))
                if l.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#boolean" =>
            {
                Some(["true", "1"].contains(&l.value()))
            }
            None => None,
            _ => bail!("rdf_comparison_result_invalid"),
        })
    })
}
pub fn term_value(t: &Term) -> Value {
    match t {
        Term::NamedNode(n) => json!({"kind":"iri","value":n.as_str()}),
        Term::BlankNode(n) => json!({"kind":"blank","value":n.as_str()}),
        Term::Literal(l) => {
            json!({"kind":"literal","value":l.value(),"datatype":l.datatype().as_str(),"language":l.language()})
        }
    }
}
pub fn local_query(query: &str) -> Result<()> {
    with_query_stack(|| local_query_inner(query))
}
fn local_query_inner(query: &str) -> Result<()> {
    ensure!(query.len() <= 1024 * 1024, "query_byte_budget");
    let parsed = spargebra::SparqlParser::new().parse_query(query)?;
    ensure_local_query(&parsed)
}
fn ensure_local_query(parsed: &spargebra::Query) -> Result<()> {
    // Inspect canonical parsed algebra rather than comments/string literals.
    let repr = format!("{parsed:?}");
    ensure!(
        !repr.contains("Service {") && !repr.contains("Service("),
        "remote_service_unsupported"
    );
    match parsed {
        spargebra::Query::Select { dataset, .. }
        | spargebra::Query::Ask { dataset, .. }
        | spargebra::Query::Construct { dataset, .. }
        | spargebra::Query::Describe { dataset, .. } => {
            ensure!(dataset.is_none(), "remote_dataset_unsupported")
        }
    }
    Ok(())
}
pub fn select(store: &Dataset, query: &str) -> Result<Vec<BTreeMap<String, Term>>> {
    Ok(select_with_columns(store, query)?.1)
}
pub type SelectResult = (Vec<String>, Vec<BTreeMap<String, Term>>);
pub fn select_with_columns(store: &Dataset, query: &str) -> Result<SelectResult> {
    with_query_stack(|| select_with_columns_inner(store, query))
}
fn select_with_columns_inner(store: &Dataset, query: &str) -> Result<SelectResult> {
    local_query_inner(query)?;
    let QueryResults::Solutions(rows) = SparqlEvaluator::new()
        .parse_query(query)?
        .on_queryable_dataset(store)
        .execute()?
    else {
        bail!("expected_select_query");
    };
    let columns = rows
        .variables()
        .iter()
        .map(|v| v.as_str().to_owned())
        .collect();
    let mut results = Vec::new();
    for row in rows {
        ensure!(results.len() < 10_000, "query_row_budget");
        let row = row?;
        results.push(
            row.iter()
                .map(|(k, v)| (k.as_str().into(), v.clone()))
                .collect(),
        );
    }
    Ok((columns, results))
}
pub fn select_bound(
    store: &Dataset,
    query: &str,
    focus: Term,
) -> Result<Vec<BTreeMap<String, Term>>> {
    with_query_stack(|| select_bound_inner(store, query, focus))
}
fn select_bound_inner(
    store: &Dataset,
    query: &str,
    focus: Term,
) -> Result<Vec<BTreeMap<String, Term>>> {
    local_query_inner(query)?;
    let QueryResults::Solutions(rows) = SparqlEvaluator::new()
        .parse_query(query)?
        .substitute_variable(oxigraph::model::Variable::new("this")?, focus)
        .on_queryable_dataset(store)
        .execute()?
    else {
        bail!("expected_shacl_select_query");
    };
    let mut results = Vec::new();
    for row in rows {
        ensure!(results.len() < 10_000, "query_row_budget");
        let row = row?;
        results.push(
            row.iter()
                .map(|(k, v)| (k.as_str().into(), v.clone()))
                .collect(),
        );
    }
    Ok(results)
}
pub fn query(store: &Dataset, text: &str) -> Result<Value> {
    with_query_stack(|| query_inner(store, text))
}
/// Keep the parsed query operation separate from its RDF graph result format.
/// Parsing, execution and AST destruction stay inside the same stack budget.
pub fn query_with_kind(store: &Dataset, text: &str) -> Result<(String, Value)> {
    with_query_stack(|| {
        query_with_options_and_kind_inner(
            store,
            text,
            &BTreeMap::new(),
            &BTreeMap::new(),
            false,
            QueryScopeLimits::default(),
        )
    })
}
fn query_inner(store: &Dataset, text: &str) -> Result<Value> {
    query_with_options_inner(store, text, &BTreeMap::new(), &BTreeMap::new())
}
/// Prefixes and typed bindings share the same parser, local-only guard, stack
/// budget and result encoding as ordinary queries.
pub fn query_with_options(
    store: &Dataset,
    text: &str,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, Term>,
) -> Result<Value> {
    with_query_stack(|| query_with_options_inner(store, text, namespaces, bindings))
}
fn query_with_options_inner(
    store: &Dataset,
    text: &str,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, Term>,
) -> Result<Value> {
    query_with_options_and_kind_inner(
        store,
        text,
        namespaces,
        bindings,
        false,
        QueryScopeLimits::default(),
    )
    .map(|(_, result)| result)
}
/// Explicit RDFLib 7.6 Describe modifier-key compatibility; neutral defaults
/// are separate. Uses the same parser, actual evaluator, guards and Dataset.
pub fn query_rdflib_with_options(
    store: &Dataset,
    text: &str,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, Term>,
) -> Result<Value> {
    query_rdflib_with_options_and_kind(store, text, namespaces, bindings).map(|(_, result)| result)
}
pub fn query_rdflib_with_options_and_kind(
    store: &Dataset,
    text: &str,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, Term>,
) -> Result<(String, Value)> {
    with_query_stack(|| {
        query_with_options_and_kind_inner(
            store,
            text,
            namespaces,
            bindings,
            true,
            QueryScopeLimits::default(),
        )
    })
}
#[derive(Clone, Copy)]
pub(crate) struct QueryScopeLimits {
    pub initial_shadows: bool,
    pub group_null_projection: bool,
    pub ast_depth: usize,
    pub ast_nodes: usize,
    pub rows: usize,
    pub triples: usize,
}
impl Default for QueryScopeLimits {
    fn default() -> Self {
        Self {
            initial_shadows: false,
            group_null_projection: false,
            ast_depth: 96,
            ast_nodes: 100_000,
            rows: 10_000,
            triples: 100_000,
        }
    }
}

pub(crate) fn query_shadow_with_options_and_kind(
    store: &Dataset,
    text: &str,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, Term>,
    limits: QueryScopeLimits,
) -> Result<(String, Value)> {
    with_query_stack(|| {
        query_with_options_and_kind_inner(store, text, namespaces, bindings, true, limits)
    })
}

pub(crate) fn check_query_parameter_budget(
    text: &str,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, Term>,
) -> Result<()> {
    ensure!(text.len() <= 1024 * 1024, "query_byte_budget");
    ensure!(
        namespaces.len() <= 128 && bindings.len() <= 128,
        "query_parameter_count_budget"
    );
    let parameter_bytes: usize = namespaces
        .iter()
        .map(|(k, v)| k.len() + v.len())
        .sum::<usize>()
        + bindings
            .iter()
            .map(|(k, v)| k.len() + v.to_string().len())
            .sum::<usize>();
    ensure!(
        parameter_bytes <= crate::MAX_BYTES,
        "query_parameter_byte_budget"
    );
    Ok(())
}

fn query_with_options_and_kind_inner(
    store: &Dataset,
    text: &str,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, Term>,
    rdflib_describe: bool,
    limits: QueryScopeLimits,
) -> Result<(String, Value)> {
    check_query_parameter_budget(text, namespaces, bindings)?;
    let mut parser = spargebra::SparqlParser::new();
    for (prefix, iri) in namespaces {
        parser = parser.with_prefix(prefix, iri)?;
    }
    let (mut parsed, describe_targets) = if rdflib_describe {
        parser.parse_query_rdflib760(text)?
    } else {
        (parser.parse_query(text)?, None)
    };
    ensure_local_query(&parsed)?;
    let kind = match &parsed {
        spargebra::Query::Select { .. } => "select",
        spargebra::Query::Ask { .. } => "ask",
        spargebra::Query::Construct { .. } => "construct",
        spargebra::Query::Describe { .. } => "describe",
    };
    let mut initial = BTreeMap::new();
    for (name, value) in bindings {
        let variable = Variable::new(name.strip_prefix('?').unwrap_or(name))?;
        if let Some(previous) = initial.insert(variable, value.clone()) {
            ensure!(previous == *value, "query_initial_binding_alias_conflict");
        }
    }
    let null_group_keys = if limits.group_null_projection {
        group_null::projected_keys(&parsed, limits)?
    } else {
        Vec::new()
    };
    let (projection, shadow_targets) =
        retain_initial_binding_scope(&mut parsed, initial.keys().cloned().collect(), limits)?;
    // The optimizer assumes ordinary projection scope. Fixed application
    // bindings deliberately cross those boundaries; preserve their AST slots.
    let evaluator = if initial.is_empty() && !rdflib_describe {
        QueryEvaluator::new()
    } else {
        QueryEvaluator::new().without_optimizations()
    };
    let evaluator = if let Some(targets) = describe_targets {
        evaluator.with_describe_target_variables(targets)
    } else {
        evaluator
    };
    let evaluator = evaluator.with_initial_shadow_targets(shadow_targets);
    let mut prepared = evaluator.prepare(&parsed);
    for (variable, value) in initial {
        prepared = prepared.substitute_variable(variable, value);
    }
    let result: Result<Value> = match prepared.execute(store)? {
        QueryResults::Boolean(value) => Ok(json!({"kind":"ask","value":value})),
        QueryResults::Solutions(rows) => {
            let columns: Vec<_> = projection
                .unwrap_or_else(|| rows.variables().to_vec())
                .iter()
                .map(|v| v.as_str().to_owned())
                .collect();
            let selected: BTreeSet<_> = columns.iter().map(String::as_str).collect();
            let mut out = Vec::new();
            for row in rows {
                ensure!(out.len() < limits.rows, "query_row_budget");
                let mut values = row?
                    .iter()
                    .filter(|(k, _)| selected.contains(k.as_str()))
                    .map(|(k, v)| (k.as_str().to_owned(), term_value(v)))
                    .collect::<BTreeMap<_, _>>();
                // This actual row passed through the proved top-level Group.
                // Null is result metadata, never an RDF term or a new row.
                for key in &null_group_keys {
                    if selected.contains(key.as_str()) {
                        values.entry(key.as_str().to_owned()).or_insert(Value::Null);
                    }
                }
                out.push(values);
            }
            Ok(json!({"kind":"select","columns":columns,"rows":out,"row_count":out.len()}))
        }
        QueryResults::Graph(rows) => {
            let mut out = Vec::new();
            for row in rows {
                ensure!(out.len() < limits.triples, "query_triple_budget");
                out.push(row?.to_string());
            }
            out.sort();
            Ok(json!({"kind":"graph","triples":out}))
        }
    };
    Ok((kind.into(), result?))
}

/// Keep the caller's SELECT projection and an outer encoding slot for each
/// typed application binding. The same pinned evaluator keeps these values in
/// its query context across subselects, without enlarging their result domains.
/// DESCRIBE keeps the original target pattern. The same evaluator encodes
/// application slots separately from the target output domain.
fn retain_initial_binding_scope(
    query: &mut spargebra::Query,
    initial: Vec<Variable>,
    limits: QueryScopeLimits,
) -> Result<(Option<Vec<Variable>>, Vec<Variable>)> {
    if initial.is_empty() && !limits.initial_shadows {
        return Ok((None, Vec::new()));
    }
    let select = matches!(query, spargebra::Query::Select { .. });
    let describe = matches!(query, spargebra::Query::Describe { .. });
    let pattern = match query {
        spargebra::Query::Select { pattern, .. }
        | spargebra::Query::Ask { pattern, .. }
        | spargebra::Query::Construct { pattern, .. }
        | spargebra::Query::Describe { pattern, .. } => pattern,
    };
    let mut original = Vec::new();
    pattern.on_in_scope_variable(|variable| {
        if !original.contains(variable) {
            original.push(variable.clone());
        }
    });
    let mut nodes = 0;
    let mut shadow_targets = Vec::new();
    retain_pattern_bindings(
        pattern,
        &initial,
        0,
        &mut nodes,
        limits,
        &mut shadow_targets,
    )?;
    if initial.is_empty() {
        return Ok((None, shadow_targets));
    }
    if describe {
        // Preserve the parser's genuine DESCRIBE target projection. Adding an
        // outer initial-parameter Project would turn hidden context into targets.
        // evaluate_describe handles its encoding slots without changing this AST.
        return Ok((None, shadow_targets));
    }
    // ASK/CONSTRUCT can have no outer Project, and an unused application binding
    // must still be encoded. The extra outer Project has the original visible
    // variables and fixed slots; it does not alter the inner query modifiers.
    let mut variables = original.clone();
    for variable in initial {
        if !variables.contains(&variable) {
            variables.push(variable);
        }
    }
    let inner = std::mem::replace(pattern, GraphPattern::Bgp { patterns: vec![] });
    *pattern = GraphPattern::Project {
        inner: Box::new(inner),
        variables,
    };
    Ok((select.then_some(original), shadow_targets))
}

fn binding_ast_budget(depth: usize, nodes: &mut usize, limits: QueryScopeLimits) -> Result<()> {
    ensure!(
        depth <= limits.ast_depth,
        "query_initial_binding_ast_depth_budget"
    );
    ensure!(
        *nodes < limits.ast_nodes,
        "query_initial_binding_ast_node_budget"
    );
    *nodes += 1;
    Ok(())
}
fn retain_pattern_bindings(
    pattern: &mut GraphPattern,
    initial: &[Variable],
    depth: usize,
    nodes: &mut usize,
    limits: QueryScopeLimits,
    shadow_targets: &mut Vec<Variable>,
) -> Result<()> {
    binding_ast_budget(depth, nodes, limits)?;
    match pattern {
        GraphPattern::Bgp { .. } | GraphPattern::Path { .. } | GraphPattern::Values { .. } => {}
        GraphPattern::Join { left, right }
        | GraphPattern::Lateral { left, right }
        | GraphPattern::Union { left, right } => {
            retain_pattern_bindings(left, initial, depth + 1, nodes, limits, shadow_targets)?;
            retain_pattern_bindings(right, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => {
            retain_pattern_bindings(left, initial, depth + 1, nodes, limits, shadow_targets)?;
            retain_pattern_bindings(right, initial, depth + 1, nodes, limits, shadow_targets)?;
            if let Some(expression) = expression {
                retain_expression_bindings(
                    expression,
                    initial,
                    depth + 1,
                    nodes,
                    limits,
                    shadow_targets,
                )?;
            }
        }
        GraphPattern::Filter { expr, inner } => {
            retain_pattern_bindings(inner, initial, depth + 1, nodes, limits, shadow_targets)?;
            retain_expression_bindings(expr, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
        GraphPattern::Project { inner, .. } => {
            retain_pattern_bindings(inner, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
        GraphPattern::Graph { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. } => {
            retain_pattern_bindings(inner, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
        GraphPattern::OrderBy { inner, expression } => {
            retain_pattern_bindings(inner, initial, depth + 1, nodes, limits, shadow_targets)?;
            for order in expression {
                let (OrderExpression::Asc(expr) | OrderExpression::Desc(expr)) = order;
                retain_expression_bindings(
                    expr,
                    initial,
                    depth + 1,
                    nodes,
                    limits,
                    shadow_targets,
                )?;
            }
        }
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => {
            if initial.contains(variable) {
                ensure!(
                    limits.initial_shadows,
                    "query_initial_binding_reassignment_unsupported"
                );
                // External application context may be shadowed, but an actual
                // query declaration must never be overwritten by this policy.
                let mut declared = false;
                inner.on_in_scope_variable(|existing| declared |= existing == variable);
                ensure!(!declared, "query_initial_shadow_query_target_redeclared");
                if !shadow_targets.contains(variable) {
                    shadow_targets.push(variable.clone());
                }
            }
            retain_pattern_bindings(inner, initial, depth + 1, nodes, limits, shadow_targets)?;
            retain_expression_bindings(
                expression,
                initial,
                depth + 1,
                nodes,
                limits,
                shadow_targets,
            )?;
        }
        GraphPattern::Group {
            inner, aggregates, ..
        } => {
            retain_pattern_bindings(inner, initial, depth + 1, nodes, limits, shadow_targets)?;
            for (variable, aggregate) in aggregates {
                ensure!(
                    !initial.contains(variable),
                    "query_initial_binding_aggregate_target_unsupported"
                );
                if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                    retain_expression_bindings(
                        expr,
                        initial,
                        depth + 1,
                        nodes,
                        limits,
                        shadow_targets,
                    )?;
                }
            }
        }
        GraphPattern::SourceGroup { .. } => bail!("source_group_private_preparation_required"),
        GraphPattern::SourceValues { .. } => bail!("source_literal_private_preparation_required"),
        GraphPattern::SourceOrderBy { .. } => bail!("source_order_private_preparation_required"),
        GraphPattern::SourceContext { .. } => bail!("source_context_private_preparation_required"),
        GraphPattern::Service { .. } => bail!("remote_service_unsupported"),
        GraphPattern::Minus { left, right } => {
            retain_pattern_bindings(left, initial, depth + 1, nodes, limits, shadow_targets)?;
            retain_pattern_bindings(right, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
    }
    Ok(())
}
fn retain_expression_bindings(
    expression: &mut Expression,
    initial: &[Variable],
    depth: usize,
    nodes: &mut usize,
    limits: QueryScopeLimits,
    shadow_targets: &mut Vec<Variable>,
) -> Result<()> {
    binding_ast_budget(depth, nodes, limits)?;
    match expression {
        Expression::SourceLiteral(_) => bail!("source_literal_private_preparation_required"),
        Expression::SourceExists(_) => bail!("source_exists_private_preparation_required"),
        Expression::SourceSyntax(_) => bail!("source_syntax_private_preparation_required"),
        Expression::SourceFunction { .. } => bail!("source_function_private_preparation_required"),
        Expression::SourceNotEqual(_, _) => bail!("source_relation_private_preparation_required"),
        Expression::NamedNode(_)
        | Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::Bound(_) => {}
        Expression::Exists(pattern) => {
            retain_pattern_bindings(pattern, initial, depth + 1, nodes, limits, shadow_targets)?
        }
        Expression::UnaryPlus(inner) | Expression::UnaryMinus(inner) | Expression::Not(inner) => {
            retain_expression_bindings(inner, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
        Expression::Or(left, right)
        | Expression::And(left, right)
        | Expression::Equal(left, right)
        | Expression::SameTerm(left, right)
        | Expression::Greater(left, right)
        | Expression::GreaterOrEqual(left, right)
        | Expression::Less(left, right)
        | Expression::LessOrEqual(left, right)
        | Expression::Add(left, right)
        | Expression::Subtract(left, right)
        | Expression::Multiply(left, right)
        | Expression::Divide(left, right) => {
            retain_expression_bindings(left, initial, depth + 1, nodes, limits, shadow_targets)?;
            retain_expression_bindings(right, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
        Expression::If(condition, left, right) => {
            retain_expression_bindings(
                condition,
                initial,
                depth + 1,
                nodes,
                limits,
                shadow_targets,
            )?;
            retain_expression_bindings(left, initial, depth + 1, nodes, limits, shadow_targets)?;
            retain_expression_bindings(right, initial, depth + 1, nodes, limits, shadow_targets)?;
        }
        Expression::In(left, right) => {
            retain_expression_bindings(left, initial, depth + 1, nodes, limits, shadow_targets)?;
            for expr in right {
                retain_expression_bindings(
                    expr,
                    initial,
                    depth + 1,
                    nodes,
                    limits,
                    shadow_targets,
                )?;
            }
        }
        Expression::Coalesce(expressions) | Expression::FunctionCall(_, expressions) => {
            for expr in expressions {
                retain_expression_bindings(
                    expr,
                    initial,
                    depth + 1,
                    nodes,
                    limits,
                    shadow_targets,
                )?;
            }
        }
        Expression::AdditiveChain { first, tail } => {
            retain_expression_bindings(first, initial, depth + 1, nodes, limits, shadow_targets)?;
            for (_, expression) in tail {
                retain_expression_bindings(
                    expression,
                    initial,
                    depth + 1,
                    nodes,
                    limits,
                    shadow_targets,
                )?;
            }
        }
        Expression::MultiplicativeChain { first, tail } => {
            retain_expression_bindings(first, initial, depth + 1, nodes, limits, shadow_targets)?;
            for (_, expression) in tail {
                retain_expression_bindings(
                    expression,
                    initial,
                    depth + 1,
                    nodes,
                    limits,
                    shadow_targets,
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[test]
    fn source_exists_frontend_gate_public_visitors_refuse_actual_private_expression() {
        use super::*;
        use sha2::{Digest, Sha256};
        use std::rc::Rc;
        fn expression(pattern: &GraphPattern) -> &Expression {
            match pattern {
                GraphPattern::SourceContext { inner, .. } | GraphPattern::Project { inner, .. } => {
                    expression(inner)
                }
                GraphPattern::Extend {
                    variable,
                    expression,
                    ..
                } if variable.as_str() == "probe" => expression,
                other => panic!("probe not in actual source query: {other:?}"),
            }
        }
        let input = include_str!(
            "../../../vendor/spareval/src/eval/fixtures/source_exists_frontend_gate_controls.json"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(input.as_bytes())),
            "56aed5a34a0e3f3daebfb360b08f6ebefc9453596abbee83976adf3581da461c"
        );
        let fixture: Value = serde_json::from_str(input).unwrap();
        for case in fixture["queries"].as_array().unwrap() {
            let (query, _) = spargebra::SparqlParser::new()
                .parse_query_rdflib760_source_exists_frontend(
                    case["query"].as_str().unwrap(),
                    spargebra::SourceLiteralHooks {
                        native_triples_order: None,
                        triples_order: None,
                        graph_path_admission: None,
                        visible_projection: None,
                        variable_sets: None,
                        scope: spargebra::SourceLiteralScope::new(),
                        construct: Rc::new(|_, _| panic!("gate query has no literals")),
                        python_truth: Rc::new(|_| panic!("gate query has no literal filters")),
                    },
                    true,
                )
                .unwrap();
            let spargebra::Query::Select { pattern, .. } = query else {
                panic!("query kind changed")
            };
            let mut expression = expression(&pattern).clone();
            assert!(matches!(expression, Expression::SourceExists(_)));
            let error = retain_expression_bindings(
                &mut expression,
                &[],
                0,
                &mut 0,
                QueryScopeLimits::default(),
                &mut Vec::new(),
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "source_exists_private_preparation_required"
            );
            let error = group_null::inspect_expression(
                &expression,
                0,
                &mut 0,
                &mut 0,
                QueryScopeLimits::default(),
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "source_exists_private_preparation_required"
            );
        }
    }

    #[cfg(feature = "rdflib760-nullable-internal")]
    #[test]
    fn source_function_registry_public_visitors_refuse_actual_private_expression() {
        use super::*;
        use std::rc::Rc;
        fn expression(pattern: &GraphPattern) -> &Expression {
            match pattern {
                GraphPattern::SourceContext { inner, .. } | GraphPattern::Project { inner, .. } => {
                    expression(inner)
                }
                GraphPattern::Extend {
                    variable,
                    expression,
                    ..
                } if variable.as_str() == "probe" => expression,
                other => panic!("probe not in actual source query: {other:?}"),
            }
        }
        for query_text in [
            "SELECT (<urn:f>() AS ?probe) WHERE {}",
            "SELECT (<urn:f>(?x) AS ?probe) WHERE {}",
        ] {
            let (query, _) = spargebra::SparqlParser::new()
                .parse_query_rdflib760_source_exists_frontend(
                    query_text,
                    spargebra::SourceLiteralHooks {
                        native_triples_order: None,
                        triples_order: None,
                        graph_path_admission: None,
                        visible_projection: None,
                        variable_sets: None,
                        scope: spargebra::SourceLiteralScope::new(),
                        construct: Rc::new(|_, _| panic!("gate query has no literals")),
                        python_truth: Rc::new(|_| panic!("gate query has no literal filters")),
                    },
                    true,
                )
                .unwrap();
            let spargebra::Query::Select { pattern, .. } = query else {
                panic!("query kind changed")
            };
            let mut expression = expression(&pattern).clone();
            assert!(matches!(expression, Expression::SourceFunction { .. }));
            let error = retain_expression_bindings(
                &mut expression,
                &[],
                0,
                &mut 0,
                QueryScopeLimits::default(),
                &mut Vec::new(),
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "source_function_private_preparation_required"
            );
            let error = group_null::inspect_expression(
                &expression,
                0,
                &mut 0,
                &mut 0,
                QueryScopeLimits::default(),
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "source_function_private_preparation_required"
            );
        }
    }

    #[test]
    fn genuine_group_target_collision_remains_refused_in_all_profiles() {
        use super::*;
        // Obtain an actual generated aggregate target from the genuine parser;
        // never construct a replacement AST or pretend an external CLI knows it.
        fn target(pattern: &GraphPattern) -> Variable {
            match pattern {
                GraphPattern::Project { inner, .. } | GraphPattern::Extend { inner, .. } => {
                    target(inner)
                }
                GraphPattern::Group { aggregates, .. } => aggregates[0].0.clone(),
                _ => panic!("unexpected original COUNT lowering"),
            }
        }
        for initial_shadows in [false, true] {
            let mut query = spargebra::SparqlParser::new()
                .parse_query("SELECT (COUNT(?s) AS ?count) WHERE { ?s <urn:tag> ?tag }")
                .unwrap();
            let variable = match &query {
                spargebra::Query::Select { pattern, .. } => target(pattern),
                _ => unreachable!(),
            };
            let error = retain_initial_binding_scope(
                &mut query,
                vec![variable],
                QueryScopeLimits {
                    initial_shadows,
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "query_initial_binding_aggregate_target_unsupported"
            );
        }
    }

    #[test]
    fn ordinary_query_panic_remains_an_execution_error() {
        let result: anyhow::Result<()> = super::with_query_stack(|| panic!("synthetic failure"));
        assert_eq!(result.unwrap_err().to_string(), "semantic_query_panicked");
    }
}

mod group_null;
