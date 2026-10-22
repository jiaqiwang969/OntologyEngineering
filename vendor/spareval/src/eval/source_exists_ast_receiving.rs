//! Independent AST receiving of fixed original traces, not EXISTS execution.
//! Expected fields are a mechanical original-trace projection. The walkers
//! below observe actual parser/planner objects and never evaluate a graph.
use super::*;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, PreparedSourceLiteralRegistry, QueryContextSemantics,
    QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities, SourceFact,
    SourceHashPolicy, SourceHashRuntime,
};
use semantica_numeric::{DecimalContext, DecimalSignal, NumericLimits, NumericValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spargebra::algebra::{AggregateFunction, SourceContextAnnotation, SourceJoinMode};
use spargebra::{
    ParsedSourceLiteral, SourceExistsExpression, SourceExistsPolarity, SourceSyntaxNode,
    SourceSyntaxSnapshot, SourceSyntaxValue, SparqlParser,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str = include_str!("fixtures/source_exists_ast_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "c3e13853653e780ee9f2db49bbc1c91caae65939b4609f863a19816e772db928"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "c4b53bda7860d30107b1d7e7cc38b6b40b7a151181b918564ba5bcc36705d8ad"
    );
    value
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), canonical(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn session(case: &Value) -> Rc<QueryNumericSession> {
    let mut decimal = DecimalContext::default();
    decimal.precision = case["input"]["decimal"]["precision"].as_u64().unwrap() as usize;
    if case["input"]["decimal"]["trap_float_operation"]
        .as_bool()
        .unwrap()
    {
        decimal.traps |= DecimalSignal::FloatOperation.into();
    }
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            decimal,
            NumericLimits::default(),
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
            QueryContextSemantics::SourceContextV1,
            Some(Rc::new(
                SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap(),
            )),
        )
        .unwrap(),
    )
}
fn flags(session: &QueryNumericSession) -> Value {
    json!(
        session
            .flags()
            .iter()
            .map(|signal| format!("{signal:?}"))
            .collect::<Vec<_>>()
    )
}

type Exists = SourceExistsExpression<GraphPattern>;
#[derive(Clone)]
struct Occurrence {
    target: String,
    not_depth: usize,
    value: Exists,
}

// Observe only outer executable expression links. Instance graph children
// are a separate domain and are inspected explicitly from their snapshot.
fn expression_occurrences(
    expression: &Expression,
    target: &str,
    depth: usize,
    found: &mut Vec<Occurrence>,
) {
    match expression {
        Expression::SourceExists(value) => found.push(Occurrence {
            target: target.to_owned(),
            not_depth: depth,
            value: value.clone(),
        }),
        Expression::Not(inner) => expression_occurrences(inner, target, depth + 1, found),
        Expression::Variable(_)
        | Expression::SourceLiteral(_)
        | Expression::NamedNode(_)
        | Expression::Literal(_)
        | Expression::Bound(_) => (),
        Expression::UnaryPlus(inner) | Expression::UnaryMinus(inner) => {
            expression_occurrences(inner, target, depth, found)
        }
        Expression::Or(args)
        | Expression::And(args)
        | Expression::Coalesce(args)
        | Expression::FunctionCall(_, args) => {
            for arg in args {
                expression_occurrences(arg, target, depth, found);
            }
        }
        Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b)
        | Expression::Equal(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b) => {
            expression_occurrences(a, target, depth, found);
            expression_occurrences(b, target, depth, found);
        }
        Expression::If(a, b, c) => {
            for arg in [a, b, c] {
                expression_occurrences(arg, target, depth, found);
            }
        }
        Expression::AdditiveChain { first, tail } => {
            expression_occurrences(first, target, depth, found);
            for (_, arg) in tail {
                expression_occurrences(arg, target, depth, found);
            }
        }
        Expression::MultiplicativeChain { first, tail } => {
            expression_occurrences(first, target, depth, found);
            for (_, arg) in tail {
                expression_occurrences(arg, target, depth, found);
            }
        }
        Expression::Exists(_) => panic!("source EXISTS was converted to legacy EXISTS"),
        #[allow(unreachable_patterns)]
        other => panic!("unfinalized or unsupported outer expression: {other:?}"),
    }
}
// This walker observes the parser result before planner lowering. It records
// the immutable source descriptor itself, not an expected interpretation.
fn parser_snapshots(
    pattern: &spargebra::algebra::GraphPattern,
    out: &mut BTreeMap<String, (usize, SourceExistsPolarity, SourceSyntaxSnapshot)>,
) {
    use spargebra::algebra::{AggregateExpression as A, Expression as E, GraphPattern as P};
    fn expr(
        value: &E,
        target: &str,
        depth: usize,
        out: &mut BTreeMap<String, (usize, SourceExistsPolarity, SourceSyntaxSnapshot)>,
    ) {
        match value {
            E::SourceExists(value) => {
                assert!(
                    value.syntax.root_node().is_some(),
                    "captured snapshot escaped parser"
                );
                assert!(
                    out.insert(
                        target.to_owned(),
                        (depth, value.polarity, value.syntax.clone())
                    )
                    .is_none()
                );
            }
            E::Not(value) => expr(value, target, depth + 1, out),
            E::Variable(_) | E::SourceLiteral(_) | E::NamedNode(_) | E::Literal(_) => (),
            other => panic!("unfrozen or unfinalized original parser expression: {other:?}"),
        }
    }
    match pattern {
        P::SourceContext { inner, .. }
        | P::Project { inner, .. }
        | P::Distinct { inner }
        | P::Reduced { inner }
        | P::Slice { inner, .. } => parser_snapshots(inner, out),
        P::Extend {
            inner,
            variable,
            expression,
        } => {
            expr(expression, variable.as_str(), 0, out);
            parser_snapshots(inner, out);
        }
        P::Filter {
            inner,
            expr: expression,
        } => {
            expr(expression, "<filter>", 0, out);
            parser_snapshots(inner, out);
        }
        P::Join { left, right } | P::Minus { left, right } => {
            parser_snapshots(left, out);
            parser_snapshots(right, out);
        }
        P::Group {
            inner, aggregates, ..
        }
        | P::SourceGroup {
            inner, aggregates, ..
        } => {
            for (variable, aggregate) in aggregates {
                if let A::FunctionCall { expr: value, .. } = aggregate {
                    expr(value, variable.as_str(), 0, out);
                }
            }
            parser_snapshots(inner, out);
        }
        P::Bgp { .. } | P::Values { .. } | P::SourceValues { .. } => (),
        other => panic!("unfrozen parser outer graph: {other:?}"),
    }
}
fn plan_occurrences(plan: &GraphPattern, found: &mut Vec<Occurrence>) {
    match plan {
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => {
            expression_occurrences(expression, variable.as_str(), 0, found);
            plan_occurrences(inner, found);
        }
        GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::SourceContext { inner, .. } => plan_occurrences(inner, found),
        GraphPattern::Filter { inner, expression } => {
            expression_occurrences(expression, "<filter>", 0, found);
            plan_occurrences(inner, found);
        }
        GraphPattern::Join { left, right, .. }
        | GraphPattern::Minus { left, right, .. }
        | GraphPattern::LeftJoin { left, right, .. } => {
            plan_occurrences(left, found);
            plan_occurrences(right, found);
        }
        GraphPattern::SourceGroup {
            inner, aggregates, ..
        }
        | GraphPattern::Group {
            inner, aggregates, ..
        } => {
            for (variable, aggregate) in aggregates {
                if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                    expression_occurrences(expr, variable.as_str(), 0, found);
                }
            }
            plan_occurrences(inner, found);
        }
        GraphPattern::SourceValues { .. } | GraphPattern::Values { .. } => (),
        other => panic!("unfrozen outer AST in observation: {other:?}"),
    }
}
fn node(snapshot: &SourceSyntaxSnapshot, id: usize) -> &SourceSyntaxNode {
    snapshot
        .nodes()
        .get(id)
        .expect("actual source node must exist")
}
fn field<'a>(node: &'a SourceSyntaxNode, key: &str) -> Option<&'a SourceSyntaxValue> {
    node.fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}
fn node_field(node: &SourceSyntaxNode, key: &str) -> usize {
    let Some(SourceSyntaxValue::Node(id)) = field(node, key) else {
        panic!("missing actual node field {key} on {}", node.name)
    };
    *id
}
fn variable(value: &SourceSyntaxValue) -> &str {
    let SourceSyntaxValue::Variable(value) = value else {
        panic!("actual variable required")
    };
    value.as_str()
}
fn first(value: &SourceSyntaxValue) -> &SourceSyntaxValue {
    let SourceSyntaxValue::List(values) = value else {
        panic!("actual list required")
    };
    &values[0]
}
fn linked<'a>(
    snapshot: &'a SourceSyntaxSnapshot,
    value: &SourceSyntaxValue,
) -> &'a SourceSyntaxNode {
    let SourceSyntaxValue::Node(id) = value else {
        panic!("actual link required")
    };
    node(snapshot, *id)
}
fn shape(snapshot: &SourceSyntaxSnapshot, id: usize) -> Value {
    let n = node(snapshot, id);
    let scope = n.attributes.variables.as_ref().map(|vars| {
        let mut names: Vec<_> = vars.iter().map(|v| v.as_str()).collect();
        names.sort_unstable();
        names
    });
    // Fields are the source mapping; these two convenience attributes must
    // neither manufacture missing _vars/lazy nor erase their actual presence.
    assert_eq!(field(n, "_vars").is_some(), scope.is_some());
    assert_eq!(field(n, "lazy").is_some(), n.attributes.lazy.is_some());
    if let Some(value) = field(n, "lazy") {
        let SourceSyntaxValue::Boolean(value) = value else {
            panic!("source lazy must be bool");
        };
        assert_eq!(Some(*value), n.attributes.lazy);
    }
    if let Some(value) = field(n, "_vars") {
        let SourceSyntaxValue::VariableSet(values) = value else {
            panic!("source _vars must be a set");
        };
        let mut names: Vec<_> = values.iter().map(|v| v.as_str()).collect();
        names.sort_unstable();
        assert_eq!(Some(names), scope);
    }
    let var = match field(n, "var") {
        Some(SourceSyntaxValue::Variable(v)) => Some(v.as_str()),
        _ => None,
    };
    let empty_bgp = if n.name == "BGP" {
        let Some(SourceSyntaxValue::Triples(values)) = field(n, "triples") else {
            panic!("BGP triples field absent")
        };
        Some(values.is_empty())
    } else {
        None
    };
    let children: Vec<_> = ["p", "p1", "p2"]
        .into_iter()
        .filter_map(|key| {
            let Some(SourceSyntaxValue::Node(child)) = field(n, key) else {
                return None;
            };
            Some(json!([key, shape(snapshot, *child)]))
        })
        .collect();
    json!({"name":n.name,"scope":scope,"lazy":n.attributes.lazy,
        "no_isolated_scope":n.attributes.no_isolated_scope,"translated":n.attributes.translated,
        "var":var,"empty_bgp":empty_bgp,"children":children})
}
fn mapping_rewrite(snapshot: &SourceSyntaxSnapshot, graph: usize) -> Value {
    let sub = node(snapshot, graph);
    let projection = linked(snapshot, first(field(sub, "projection").unwrap()));
    let group = node(snapshot, node_field(sub, "where"));
    let values = linked(snapshot, first(field(group, "part").unwrap()));
    json!([
        variable(field(projection, "expr").unwrap()),
        variable(field(projection, "evar").unwrap()),
        variable(first(field(values, "var").unwrap()))
    ])
}
fn source_literals(snapshot: &SourceSyntaxSnapshot, root: usize) -> Vec<ParsedSourceLiteral> {
    fn value(
        snapshot: &SourceSyntaxSnapshot,
        current: &SourceSyntaxValue,
        seen: &mut BTreeSet<usize>,
        out: &mut Vec<ParsedSourceLiteral>,
    ) {
        match current {
            SourceSyntaxValue::Node(id) => visit(snapshot, *id, seen, out),
            SourceSyntaxValue::List(values) => {
                for item in values {
                    value(snapshot, item, seen, out);
                }
            }
            SourceSyntaxValue::Literal(literal) => out.push(literal.clone()),
            SourceSyntaxValue::Values { bindings, .. } => {
                for row in bindings {
                    for item in row.iter().flatten() {
                        if let spargebra::algebra::SourceValueTerm::Literal(literal) = item {
                            out.push(literal.clone());
                        }
                    }
                }
            }
            _ => (),
        }
    }
    fn visit(
        snapshot: &SourceSyntaxSnapshot,
        id: usize,
        seen: &mut BTreeSet<usize>,
        out: &mut Vec<ParsedSourceLiteral>,
    ) {
        if !seen.insert(id) {
            return;
        }
        let n = node(snapshot, id);
        for (_, item) in &n.fields {
            value(snapshot, item, seen, out);
        }
        if let Some(graph) = n.attributes.graph {
            visit(snapshot, graph, seen, out);
        }
    }
    let mut out = Vec::new();
    visit(snapshot, root, &mut BTreeSet::new(), &mut out);
    out
}
fn expression_literals(expression: &Expression, out: &mut Vec<ParsedSourceLiteral>) {
    match expression {
        Expression::SourceLiteral(value) => out.push(value.clone()),
        Expression::Variable(_) | Expression::NamedNode(_) | Expression::Bound(_) => (),
        Expression::Not(inner) | Expression::UnaryPlus(inner) | Expression::UnaryMinus(inner) => {
            expression_literals(inner, out)
        }
        Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b)
        | Expression::Equal(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b) => {
            expression_literals(a, out);
            expression_literals(b, out);
        }
        Expression::AdditiveChain { first, tail } => {
            expression_literals(first, out);
            for (_, item) in tail {
                expression_literals(item, out);
            }
        }
        Expression::MultiplicativeChain { first, tail } => {
            expression_literals(first, out);
            for (_, item) in tail {
                expression_literals(item, out);
            }
        }
        Expression::Or(args)
        | Expression::And(args)
        | Expression::Coalesce(args)
        | Expression::FunctionCall(_, args) => {
            for item in args {
                expression_literals(item, out);
            }
        }
        Expression::If(a, b, c) => {
            for item in [a, b, c] {
                expression_literals(item, out);
            }
        }
        other => panic!("unfrozen or lossy runtime leaf: {other:?}"),
    }
}
fn graph_literals(plan: &GraphPattern, out: &mut Vec<ParsedSourceLiteral>) {
    match plan {
        GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::SourceContext { inner, .. } => graph_literals(inner, out),
        GraphPattern::Extend {
            inner, expression, ..
        }
        | GraphPattern::Filter { inner, expression } => {
            expression_literals(expression, out);
            graph_literals(inner, out);
        }
        GraphPattern::Join { left, right, .. } => {
            graph_literals(left, out);
            graph_literals(right, out);
        }
        GraphPattern::SourceOrderBy { inner, expression } => {
            for order in expression {
                let (OrderExpression::Asc(expr) | OrderExpression::Desc(expr)) = order;
                expression_literals(expr, out);
            }
            graph_literals(inner, out);
        }
        GraphPattern::SourceValues { bindings, .. } => {
            for row in bindings {
                for cell in row.iter().flatten() {
                    if let spargebra::algebra::SourceValueTerm::Literal(literal) = cell {
                        out.push(literal.clone());
                    }
                }
            }
        }
        GraphPattern::Values {
            variables,
            bindings,
        } => {
            assert!(variables.is_empty() && bindings.len() == 1 && bindings[0].is_empty());
        }
        other => panic!("unfrozen runtime literal domain: {other:?}"),
    }
}
fn child_shape<'a>(expected: &'a Value, key: &str) -> &'a Value {
    &expected["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == key)
        .unwrap()[1]
}
// Compare the actual lowerable overlay as well as its syntax snapshot. A
// correct snapshot paired with a substituted empty executable graph fails.
fn assert_runtime_graph(actual: &GraphPattern, expected: &Value) {
    let mut actual = actual;
    while let GraphPattern::SourceContext { inner, annotation } = actual {
        match annotation {
            SourceContextAnnotation::ToMultiSet => {
                assert_eq!(expected["name"], "ToMultiSet");
                assert_runtime_graph(inner, child_shape(expected, "p"));
                return;
            }
            SourceContextAnnotation::Unanalyzed { no_isolated_scope } => {
                assert!(expected["scope"].is_null());
                assert!(expected["lazy"].is_null());
                assert_eq!(
                    *no_isolated_scope,
                    expected["no_isolated_scope"].as_bool().unwrap_or(false)
                );
                actual = inner;
            }
            // The existing SourceGroup jointly represents AggregateJoin and
            // Group. No runtime owner/finish behavior is inferred here.
            SourceContextAnnotation::Group { .. } if expected["name"] == "AggregateJoin" => {
                actual = inner
            }
            other => panic!("analyzed or wrong annotation on original overlay: {other:?}"),
        }
    }
    match actual {
        GraphPattern::Values {
            variables,
            bindings,
        } => {
            assert_eq!(expected["name"], "BGP");
            assert_eq!(expected["empty_bgp"], true);
            assert!(variables.is_empty() && bindings.len() == 1 && bindings[0].is_empty());
        }
        GraphPattern::SourceValues { .. } => assert_eq!(expected["name"], "values"),
        GraphPattern::Extend {
            inner, variable, ..
        } => {
            assert_eq!(expected["name"], "Extend");
            assert_eq!(variable.as_str(), expected["var"]);
            assert_runtime_graph(inner, child_shape(expected, "p"));
        }
        GraphPattern::Filter { inner, .. } => {
            assert_eq!(expected["name"], "Filter");
            assert_runtime_graph(inner, child_shape(expected, "p"));
        }
        GraphPattern::Join { left, right, .. } => {
            assert_eq!(expected["name"], "Join");
            assert_runtime_graph(left, child_shape(expected, "p1"));
            assert_runtime_graph(right, child_shape(expected, "p2"));
        }
        GraphPattern::Project { inner, .. } => {
            assert_eq!(expected["name"], "Project");
            assert_runtime_graph(inner, child_shape(expected, "p"));
        }
        GraphPattern::Distinct { inner } => {
            assert_eq!(expected["name"], "Distinct");
            assert_runtime_graph(inner, child_shape(expected, "p"));
        }
        GraphPattern::SourceOrderBy { inner, .. } => {
            assert_eq!(expected["name"], "OrderBy");
            assert_runtime_graph(inner, child_shape(expected, "p"));
        }
        GraphPattern::SourceGroup { inner, .. } => {
            assert_eq!(expected["name"], "AggregateJoin");
            let group = child_shape(expected, "p");
            assert_eq!(group["name"], "Group");
            assert_runtime_graph(inner, child_shape(group, "p"));
        }
        other => panic!("unexpected actual overlay graph: {other:?}"),
    }
}
fn literal_fact(value: &RawExpressionValue) -> Value {
    let Term::Literal(rdf) = value.rdf().unwrap() else {
        panic!("actual Literal required")
    };
    let LiteralSourceState::Literal(metadata) = value.literal_source() else {
        panic!("facts required")
    };
    let SourceFact::Known(language) = &metadata.language_original else {
        panic!("known language required")
    };
    let SourceFact::Known(ill) = &metadata.ill_typed else {
        panic!("known ill_typed required")
    };
    let NumericCacheState::Known(cache) = value.numeric() else {
        panic!("numeric cache required")
    };
    let (kind, cached) = match cache.value() {
        NumericValue::Integer(value) => (
            "builtins.int",
            serde_json::from_str::<Value>(&value.to_string()).unwrap(),
        ),
        NumericValue::Binary64(value) => (
            "builtins.float",
            json!({"family":"float",
            "bits_hex":format!("{:016x}", value.to_bits()),
            "repr":semantica_json::python_float_repr(*value).unwrap()}),
        ),
        other => panic!("unfrozen cache family in BX11/12: {other:?}"),
    };
    json!({"lexical":rdf.value(),"datatype":rdf.datatype().as_str(),"language":language,
        "ill_typed":ill,"cache_type":kind,"cache":cached})
}
fn strip_context(mut p: &GraphPattern) -> &GraphPattern {
    while let GraphPattern::SourceContext { inner, .. } = p {
        p = inner;
    }
    p
}
fn outer_spine(p: &GraphPattern) -> Value {
    let mut names = Vec::new();
    let mut current = p;
    loop {
        current = strip_context(current);
        current = match current {
            GraphPattern::Project { inner, .. } => {
                names.push("Project");
                inner
            }
            GraphPattern::Extend { inner, .. } => {
                names.push("Extend");
                inner
            }
            GraphPattern::SourceGroup { inner, .. } => {
                names.push("SourceGroup");
                inner
            }
            GraphPattern::Values {
                variables,
                bindings,
            } => {
                assert!(variables.is_empty() && bindings.len() == 1 && bindings[0].is_empty());
                names.push("EmptySingleton");
                break;
            }
            other => panic!("unexpected actual outer spine {other:?}"),
        };
    }
    json!(names)
}
fn outer_aggregates(p: &GraphPattern) -> Value {
    match strip_context(p) {
        GraphPattern::Project { inner, .. } | GraphPattern::Extend { inner, .. } => {
            outer_aggregates(inner)
        }
        GraphPattern::SourceGroup { aggregates, .. } => json!(
            aggregates
                .iter()
                .map(|(res, aggregate)| {
                    let AggregateExpression::FunctionCall {
                        name,
                        expr: Expression::Variable(var),
                        distinct,
                    } = aggregate
                    else {
                        panic!("unfrozen actual outer aggregate")
                    };
                    let name = match name {
                        AggregateFunction::Sum => "Aggregate_Sum",
                        AggregateFunction::Sample => "Aggregate_Sample",
                        _ => panic!("unexpected aggregate name"),
                    };
                    json!({"name":name,"vars":var.as_str(),"res":res.as_str(),"distinct":distinct})
                })
                .collect::<Vec<_>>()
        ),
        other => panic!("missing actual outer aggregate chain {other:?}"),
    }
}
fn outer_join_modes(p: &GraphPattern, modes: &mut Vec<bool>) {
    match p {
        GraphPattern::SourceContext { inner, annotation } => {
            if let SourceContextAnnotation::Join { mode } = annotation {
                modes.push(*mode == SourceJoinMode::Lazy);
            }
            outer_join_modes(inner, modes);
        }
        GraphPattern::Project { inner, .. } | GraphPattern::Extend { inner, .. } => {
            outer_join_modes(inner, modes)
        }
        GraphPattern::Join { left, right, .. } => {
            outer_join_modes(left, modes);
            outer_join_modes(right, modes);
        }
        GraphPattern::Values { .. } | GraphPattern::SourceValues { .. } => (),
        other => panic!("unexpected actual outer join domain {other:?}"),
    }
}

fn run_case(case: &Value) {
    let s = session(case);
    let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
    let mut hooks = registry.hooks();
    let construct = Rc::clone(&hooks.construct);
    let originals: Rc<RefCell<BTreeMap<usize, RawExpressionValue>>> = Rc::default();
    let observed = Rc::clone(&originals);
    let owner = Rc::clone(&registry);
    hooks.construct = Rc::new(move |id, input| {
        let result = construct(id.clone(), input)?;
        let mut lookup = result.clone();
        lookup.occurrence = Some(id.clone());
        let raw = owner
            .resolve(&lookup)
            .map_err(|e| -> spargebra::SourceLiteralBridgeError { Box::new(e) })?;
        assert!(observed.borrow_mut().insert(id.index(), raw).is_none());
        Ok(result)
    });
    let (query, _) = SparqlParser::new()
        .parse_query_rdflib760_source_exists_frontend(
            case["input"]["query"].as_str().unwrap(),
            hooks,
            true,
        )
        .unwrap();
    let spargebra::Query::Select { pattern, .. } = query else {
        panic!("SELECT required")
    };
    let mut parsed_snapshots = BTreeMap::new();
    parser_snapshots(&pattern, &mut parsed_snapshots);
    finished_parser_graph(&pattern, &registry);
    let lowered = GraphPattern::from(&pattern);
    finished_plan_graph(&lowered, &registry);
    let reverse: spargebra::algebra::GraphPattern = (&lowered).into();
    finished_parser_graph(&reverse, &registry);
    let lowered_again = GraphPattern::from(&reverse);
    finished_plan_graph(&lowered_again, &registry);
    let mut previous: Option<Vec<Exists>> = None;
    for plan in [&lowered, &lowered_again] {
        let mut found = Vec::new();
        plan_occurrences(plan, &mut found);
        found.sort_by(|a, b| a.target.cmp(&b.target));
        let expected = case["expected_ast"]["expressions"].as_array().unwrap();
        assert_eq!(found.len(), expected.len(), "{}", case["id"]);
        assert_eq!(found.len(), parsed_snapshots.len());
        for (actual, expected) in found.iter().zip(expected) {
            assert_eq!(actual.target, expected["target"]);
            assert_eq!(actual.not_depth as u64, expected["generic_not_depth"]);
            let snapshot = &actual.value.syntax;
            let (parsed_not_depth, parsed_polarity, parsed_snapshot) =
                parsed_snapshots.get(&actual.target).unwrap();
            assert_eq!(actual.not_depth, *parsed_not_depth);
            assert_eq!(actual.value.polarity, *parsed_polarity);
            assert_eq!(snapshot, parsed_snapshot);
            let root = snapshot.root_node().unwrap();
            let polarity = match actual.value.polarity {
                SourceExistsPolarity::Exists => "Builtin_EXISTS",
                SourceExistsPolarity::NotExists => "Builtin_NOTEXISTS",
            };
            assert_eq!(polarity, expected["polarity"]);
            assert_eq!(root.name, polarity);
            let raw_graph = node_field(root, "graph");
            assert_eq!(
                node(snapshot, raw_graph).name,
                expected["mapping_graph_root"]
            );
            assert_eq!(
                actual.value.graph.is_some(),
                root.attributes.graph.is_some()
            );
            let runtime = root
                .attributes
                .graph
                .map(|id| shape(snapshot, id))
                .unwrap_or(Value::Null);
            assert_eq!(
                runtime, expected["runtime"],
                "{} target {}",
                case["id"], actual.target
            );
            if let Some(graph) = &actual.value.graph {
                assert_runtime_graph(graph, &expected["runtime"]);
            }
            assert_eq!(actual.value.no_isolated_scope,
                root.attributes.graph.is_some_and(|id| node(snapshot,id).attributes.no_isolated_scope==Some(true)));
            if let Some(wanted) = expected.get("rewritten_mapping_variables") {
                assert_eq!(&mapping_rewrite(snapshot, raw_graph), wanted);
            }
            if let Some(wanted) = expected.get("runtime_literal_facts") {
                let literals = source_literals(snapshot, root.attributes.graph.unwrap());
                let mut unique = BTreeMap::new();
                for literal in literals {
                    let id = literal.occurrence.as_ref().unwrap().index();
                    let value = registry.resolve(&literal).unwrap();
                    assert!(value.same_source_object(originals.borrow().get(&id).unwrap()));
                    unique.insert(id, literal_fact(&value));
                }
                assert_eq!(
                    unique.len() as u64,
                    expected["runtime_unique_literal_count"]
                );
                let syntax_ids: BTreeSet<_> = unique.keys().copied().collect();
                let mut runtime_leaves = Vec::new();
                graph_literals(actual.value.graph.as_ref().unwrap(), &mut runtime_leaves);
                let mut runtime_ids = BTreeSet::new();
                for literal in runtime_leaves {
                    let id = literal.occurrence.as_ref().unwrap().index();
                    runtime_ids.insert(id);
                    let raw = registry.resolve(&literal).unwrap();
                    assert!(raw.same_source_object(originals.borrow().get(&id).unwrap()));
                    assert_eq!(literal_fact(&raw), unique[&id]);
                }
                assert_eq!(runtime_ids, syntax_ids);
                let mut actual: Vec<_> = unique.into_values().collect();
                actual.sort_by_key(|value| serde_json::to_vec(&canonical(value)).unwrap());
                assert_eq!(&json!(actual), wanted);
            }
        }
        if let Some(previous) = &previous {
            assert_eq!(
                &found.iter().map(|x| x.value.clone()).collect::<Vec<_>>(),
                previous
            );
        }
        previous = Some(found.iter().map(|x| x.value.clone()).collect());
        if let Some(wanted) = case["expected_ast"].get("outer_spine") {
            assert_eq!(&outer_spine(plan), wanted);
        }
        if let Some(wanted) = case["expected_ast"].get("outer_aggregates") {
            assert_eq!(&outer_aggregates(plan), wanted);
        }
        if let Some(wanted) = case["expected_ast"].get("outer_join_modes") {
            let mut modes = Vec::new();
            outer_join_modes(plan, &mut modes);
            assert_eq!(&json!(modes), wanted);
        }
    }
    assert_eq!(flags(&s), case["expected_flags_after_parse"]);
}
fn run_invariant(id: &str) {
    let fixture = fixture();
    let mut count = 0;
    for case in fixture["payload"]["cases"].as_array().unwrap() {
        if case["invariants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == id)
        {
            run_case(case);
            count += 1;
        }
    }
    assert!(count > 0);
}
#[test]
fn source_exists_ast_fixture_identity() {
    let fixture = fixture();
    assert_eq!(fixture["payload"]["cases"].as_array().unwrap().len(), 12);
    assert_eq!(
        fixture["payload"]["completion_cases"]
            .as_array()
            .unwrap()
            .len(),
        50
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&fixture["payload"])).unwrap())
        ),
        fixture["payload_sha256"]
    );
}
#[test]
fn source_exists_ast_01_translation_domain_and_polarity() {
    run_invariant("EA-AST01");
}
#[test]
fn source_exists_ast_02_filter_attributes_and_false_literal() {
    run_invariant("EA-AST02");
}
#[test]
fn source_exists_ast_03_unsimplified_overlay_join() {
    run_invariant("EA-AST03");
}
#[test]
fn source_exists_ast_04_parent_analysis_and_hidden_variables() {
    run_invariant("EA-AST04");
}
#[test]
fn source_exists_ast_05_outer_aggregate_extraction_order() {
    run_invariant("EA-AST05");
}
#[test]
fn source_exists_ast_06_original_leaf_identity_and_transport() {
    run_invariant("EA-AST06");
}

// Active completion-state policy, distinct from original runtime results:
// inspect every expression edge, including actual EXISTS overlay children.
fn finished_literal(literal: &ParsedSourceLiteral, registry: &PreparedSourceLiteralRegistry) {
    assert!(literal.occurrence.is_some(), "source leaf lost occurrence");
    registry.resolve(literal).unwrap();
}
fn finished_snapshot(snapshot: &SourceSyntaxSnapshot, registry: &PreparedSourceLiteralRegistry) {
    fn value(v: &SourceSyntaxValue, count: usize, registry: &PreparedSourceLiteralRegistry) {
        match v {
            SourceSyntaxValue::Node(id) => assert!(*id < count, "unresolved source node"),
            SourceSyntaxValue::List(values) => {
                for v in values {
                    value(v, count, registry);
                }
            }
            SourceSyntaxValue::Literal(literal) => finished_literal(literal, registry),
            SourceSyntaxValue::Values { bindings, .. } => {
                for row in bindings {
                    for cell in row.iter().flatten() {
                        if let spargebra::algebra::SourceValueTerm::Literal(literal) = cell {
                            finished_literal(literal, registry);
                        }
                    }
                }
            }
            _ => (),
        }
    }
    assert!(
        snapshot.root_node().is_some(),
        "captured source snapshot escaped"
    );
    for node in snapshot.nodes() {
        for (_, v) in &node.fields {
            value(v, snapshot.nodes().len(), registry);
        }
        if let Some(id) = node.attributes.graph {
            assert!(id < snapshot.nodes().len());
        }
    }
}
fn finished_parser_expression(
    e: &spargebra::algebra::Expression,
    registry: &PreparedSourceLiteralRegistry,
) {
    use spargebra::algebra::Expression as E;
    match e {
        E::SourceSyntax(_) => panic!("temporary SourceSyntax escaped finalized frontend"),
        E::SourceLiteral(literal) => finished_literal(literal, registry),
        E::NamedNode(_) | E::Literal(_) | E::Variable(_) | E::Bound(_) => (),
        E::SourceExists(value) => {
            finished_snapshot(&value.syntax, registry);
            if let Some(graph) = &value.graph {
                finished_parser_graph(graph, registry);
            }
        }
        E::Exists(graph) => finished_parser_graph(graph, registry),
        E::SourceFunction {
            arguments, syntax, ..
        } => {
            finished_snapshot(syntax, registry);
            for argument in arguments {
                finished_parser_expression(argument, registry);
            }
        }
        E::Not(inner) | E::UnaryPlus(inner) | E::UnaryMinus(inner) => {
            finished_parser_expression(inner, registry)
        }
        E::Equal(a, b)
        | E::SourceNotEqual(a, b)
        | E::SameTerm(a, b)
        | E::Greater(a, b)
        | E::GreaterOrEqual(a, b)
        | E::Less(a, b)
        | E::LessOrEqual(a, b)
        | E::Add(a, b)
        | E::Subtract(a, b)
        | E::Multiply(a, b)
        | E::Divide(a, b)
        | E::Or(a, b)
        | E::And(a, b) => {
            finished_parser_expression(a, registry);
            finished_parser_expression(b, registry);
        }
        E::In(a, tail) => {
            finished_parser_expression(a, registry);
            for e in tail {
                finished_parser_expression(e, registry);
            }
        }
        E::Coalesce(args) | E::FunctionCall(_, args) => {
            for e in args {
                finished_parser_expression(e, registry);
            }
        }
        E::If(a, b, c) => {
            for e in [a, b, c] {
                finished_parser_expression(e, registry);
            }
        }
        E::AdditiveChain { first, tail } => {
            finished_parser_expression(first, registry);
            for (_, e) in tail {
                finished_parser_expression(e, registry);
            }
        }
        E::MultiplicativeChain { first, tail } => {
            finished_parser_expression(first, registry);
            for (_, e) in tail {
                finished_parser_expression(e, registry);
            }
        }
    }
}
fn finished_parser_graph(
    p: &spargebra::algebra::GraphPattern,
    registry: &PreparedSourceLiteralRegistry,
) {
    use spargebra::algebra::{AggregateExpression as A, GraphPattern as P, OrderExpression as O};
    use spargebra::algebra::{SourceGroupCondition, SourceGrouping};
    match p {
        P::Bgp { .. } | P::Path { .. } | P::Values { .. } => (),
        P::SourceValues { bindings, .. } => {
            for row in bindings {
                for cell in row.iter().flatten() {
                    if let spargebra::algebra::SourceValueTerm::Literal(literal) = cell {
                        finished_literal(literal, registry);
                    }
                }
            }
        }
        P::SourceContext { inner, .. }
        | P::Project { inner, .. }
        | P::Distinct { inner, .. }
        | P::Reduced { inner, .. }
        | P::Slice { inner, .. }
        | P::Service { inner, .. }
        | P::Graph { inner, .. } => finished_parser_graph(inner, registry),
        P::Filter {
            inner,
            expr: expression,
        }
        | P::Extend {
            inner, expression, ..
        } => {
            finished_parser_expression(expression, registry);
            finished_parser_graph(inner, registry);
        }
        P::Join { left, right, .. } | P::Minus { left, right, .. } => {
            finished_parser_graph(left, registry);
            finished_parser_graph(right, registry);
        }
        P::LeftJoin {
            left,
            right,
            expression,
            ..
        } => {
            if let Some(e) = expression {
                finished_parser_expression(e, registry);
            }
            finished_parser_graph(left, registry);
            finished_parser_graph(right, registry);
        }
        P::Union { left, right } => {
            finished_parser_graph(left, registry);
            finished_parser_graph(right, registry);
        }
        P::OrderBy { inner, expression } | P::SourceOrderBy { inner, expression } => {
            for order in expression {
                let (O::Asc(e) | O::Desc(e)) = order;
                finished_parser_expression(e, registry);
            }
            finished_parser_graph(inner, registry);
        }
        P::Group {
            inner, aggregates, ..
        } => {
            for (_, a) in aggregates {
                if let A::FunctionCall { expr, .. } = a {
                    finished_parser_expression(expr, registry);
                }
            }
            finished_parser_graph(inner, registry);
        }
        P::SourceGroup {
            inner,
            grouping,
            aggregates,
        } => {
            if let SourceGrouping::Explicit(conditions) = grouping {
                for condition in conditions {
                    let (SourceGroupCondition::Raw(e)
                    | SourceGroupCondition::As { expression: e, .. }) = condition;
                    finished_parser_expression(e, registry);
                }
            }
            for (_, a) in aggregates {
                if let A::FunctionCall { expr, .. } = a {
                    finished_parser_expression(expr, registry);
                }
            }
            finished_parser_graph(inner, registry);
        }
        #[cfg(feature = "sep-0006")]
        P::Lateral { left, right } => {
            finished_parser_graph(left, registry);
            finished_parser_graph(right, registry);
        }
    }
}
fn finished_plan_expression(
    e: &sparopt::algebra::Expression,
    registry: &PreparedSourceLiteralRegistry,
) {
    use sparopt::algebra::Expression as E;
    match e {
        E::SourceSyntax(_) => panic!("temporary SourceSyntax escaped finalized frontend"),
        E::SourceLiteral(literal) => finished_literal(literal, registry),
        E::NamedNode(_) | E::Literal(_) | E::Variable(_) | E::Bound(_) => (),
        E::SourceExists(value) => {
            finished_snapshot(&value.syntax, registry);
            if let Some(graph) = &value.graph {
                finished_plan_graph(graph, registry);
            }
        }
        E::Exists(graph) => finished_plan_graph(graph, registry),
        E::SourceFunction {
            arguments, syntax, ..
        } => {
            finished_snapshot(syntax, registry);
            for argument in arguments {
                finished_plan_expression(argument, registry);
            }
        }
        E::Not(inner) | E::UnaryPlus(inner) | E::UnaryMinus(inner) => {
            finished_plan_expression(inner, registry)
        }
        E::Equal(a, b)
        | E::SourceNotEqual(a, b)
        | E::SameTerm(a, b)
        | E::Greater(a, b)
        | E::GreaterOrEqual(a, b)
        | E::Less(a, b)
        | E::LessOrEqual(a, b)
        | E::Add(a, b)
        | E::Subtract(a, b)
        | E::Multiply(a, b)
        | E::Divide(a, b) => {
            finished_plan_expression(a, registry);
            finished_plan_expression(b, registry);
        }
        E::Coalesce(args) | E::FunctionCall(_, args) | E::Or(args) | E::And(args) => {
            for e in args {
                finished_plan_expression(e, registry);
            }
        }
        E::If(a, b, c) => {
            for e in [a, b, c] {
                finished_plan_expression(e, registry);
            }
        }
        E::AdditiveChain { first, tail } => {
            finished_plan_expression(first, registry);
            for (_, e) in tail {
                finished_plan_expression(e, registry);
            }
        }
        E::MultiplicativeChain { first, tail } => {
            finished_plan_expression(first, registry);
            for (_, e) in tail {
                finished_plan_expression(e, registry);
            }
        }
    }
}
fn finished_plan_graph(
    p: &sparopt::algebra::GraphPattern,
    registry: &PreparedSourceLiteralRegistry,
) {
    use spargebra::algebra::{SourceGroupCondition, SourceGrouping};
    use sparopt::algebra::{AggregateExpression as A, GraphPattern as P, OrderExpression as O};
    match p {
        P::QuadPattern { .. } | P::Path { .. } | P::Graph { .. } | P::Values { .. } => (),
        P::SourceValues { bindings, .. } => {
            for row in bindings {
                for cell in row.iter().flatten() {
                    if let spargebra::algebra::SourceValueTerm::Literal(literal) = cell {
                        finished_literal(literal, registry);
                    }
                }
            }
        }
        P::SourceContext { inner, .. }
        | P::Project { inner, .. }
        | P::Distinct { inner, .. }
        | P::Reduced { inner, .. }
        | P::Slice { inner, .. }
        | P::Service { inner, .. } => finished_plan_graph(inner, registry),
        P::Filter { inner, expression }
        | P::Extend {
            inner, expression, ..
        } => {
            finished_plan_expression(expression, registry);
            finished_plan_graph(inner, registry);
        }
        P::Join { left, right, .. } | P::Minus { left, right, .. } => {
            finished_plan_graph(left, registry);
            finished_plan_graph(right, registry);
        }
        P::LeftJoin {
            left,
            right,
            expression,
            ..
        } => {
            finished_plan_expression(expression, registry);
            finished_plan_graph(left, registry);
            finished_plan_graph(right, registry);
        }
        P::Union { inner } => {
            for p in inner {
                finished_plan_graph(p, registry);
            }
        }
        P::OrderBy { inner, expression } | P::SourceOrderBy { inner, expression } => {
            for order in expression {
                let (O::Asc(e) | O::Desc(e)) = order;
                finished_plan_expression(e, registry);
            }
            finished_plan_graph(inner, registry);
        }
        P::Group {
            inner, aggregates, ..
        } => {
            for (_, a) in aggregates {
                if let A::FunctionCall { expr, .. } = a {
                    finished_plan_expression(expr, registry);
                }
            }
            finished_plan_graph(inner, registry);
        }
        P::SourceGroup {
            inner,
            grouping,
            aggregates,
        } => {
            if let SourceGrouping::Explicit(conditions) = grouping {
                for condition in conditions {
                    let (SourceGroupCondition::Raw(e)
                    | SourceGroupCondition::As { expression: e, .. }) = condition;
                    finished_plan_expression(e, registry);
                }
            }
            for (_, a) in aggregates {
                if let A::FunctionCall { expr, .. } = a {
                    finished_plan_expression(expr, registry);
                }
            }
            finished_plan_graph(inner, registry);
        }
        #[cfg(feature = "sep-0006")]
        P::Lateral { left, right } => {
            finished_plan_graph(left, registry);
            finished_plan_graph(right, registry);
        }
    }
}

#[test]
fn source_exists_ast_50_original_frontends_complete_without_capture_escape() {
    let f = fixture();
    let cases = f["payload"]["completion_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 50);
    let mut ids = BTreeSet::new();
    for case in cases {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        assert_eq!(
            case["original_completed_stages"],
            json!(["parse_query", "translate_query"])
        );
        let s = session(case);
        let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
        let (query, _) = SparqlParser::new()
            .parse_query_rdflib760_source_exists_frontend(
                case["input"]["query"].as_str().unwrap(),
                registry.hooks(),
                true,
            )
            .unwrap_or_else(|error| panic!("{} original frontend completed: {error}", case["id"]));
        let spargebra::Query::Select { pattern, .. } = query else {
            panic!("SELECT required");
        };
        // Only parser/translation is tested. Original initial bindings, graph,
        // resulting rows, errors, cancellation and owner are not executed here.
        finished_parser_graph(&pattern, &registry);
        assert_eq!(flags(&s), case["source_flags_by_stage"]["parse_query"]);
        assert_eq!(flags(&s), case["source_flags_by_stage"]["translate_query"]);
        let lowered = GraphPattern::from(&pattern);
        finished_plan_graph(&lowered, &registry);
        let reverse: spargebra::algebra::GraphPattern = (&lowered).into();
        finished_parser_graph(&reverse, &registry);
        let lower_again = GraphPattern::from(&reverse);
        finished_plan_graph(&lower_again, &registry);
        assert_eq!(flags(&s), case["source_flags_by_stage"]["translate_query"]);
    }
}

const SEPARATOR_FIXTURE: &str = include_str!("fixtures/source_exists_separator_cases.json");
fn separator_fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(SEPARATOR_FIXTURE.as_bytes())),
        "fd0cc8d30b3a5044c3f00c85fe94961f1fc172345250664ba66106530057375b"
    );
    let f: Value = serde_json::from_str(SEPARATOR_FIXTURE).unwrap();
    assert_eq!(
        f["source_record_sha256"],
        "5a3da57279a5b667d8dd0b8e2c6bf86281f3f3281f4db04928be3b0568b6b83c"
    );
    f
}
fn separator_string_fact(raw: &RawExpressionValue, truth: bool) -> Value {
    use crate::numeric::{SourceDatatype, SourceLiteralCacheKind};
    let Term::Literal(rdf) = raw.rdf().unwrap() else {
        panic!("actual Literal required");
    };
    let LiteralSourceState::Literal(meta) = raw.literal_source() else {
        panic!("actual source facts required");
    };
    assert_eq!(meta.datatype, SourceDatatype::Absent);
    assert_eq!(
        meta.cached_value_kind,
        SourceFact::Known(SourceLiteralCacheKind::String)
    );
    assert!(matches!(raw.numeric(), NumericCacheState::NotNumeric));
    let SourceFact::Known(language) = &meta.language_original else {
        panic!("known language required");
    };
    let SourceFact::Known(ill_typed) = &meta.ill_typed else {
        panic!("known ill_typed required");
    };
    json!({"lexical":rdf.value(),"source_datatype":"absent","language":language,
        "ill_typed":ill_typed,"cache_family":"String","python_truth":truth})
}
// Follow links from the actual EXISTS instance graph, never search unrelated
// raw syntax/arena allocations for an expected-looking separator.
fn separator_overlay_fields(
    snapshot: &SourceSyntaxSnapshot,
    root: usize,
) -> (Vec<Option<ParsedSourceLiteral>>, Vec<ParsedSourceLiteral>) {
    fn value(
        snapshot: &SourceSyntaxSnapshot,
        v: &SourceSyntaxValue,
        seen: &mut BTreeSet<usize>,
        separators: &mut Vec<Option<ParsedSourceLiteral>>,
        data: &mut Vec<ParsedSourceLiteral>,
    ) {
        match v {
            SourceSyntaxValue::Node(id) => visit(snapshot, *id, seen, separators, data),
            SourceSyntaxValue::List(values) => {
                for v in values {
                    value(snapshot, v, seen, separators, data);
                }
            }
            SourceSyntaxValue::Values { bindings, .. } => {
                for row in bindings {
                    for cell in row.iter().flatten() {
                        if let spargebra::algebra::SourceValueTerm::Literal(literal) = cell {
                            data.push(literal.clone());
                        }
                    }
                }
            }
            _ => (),
        }
    }
    fn visit(
        snapshot: &SourceSyntaxSnapshot,
        id: usize,
        seen: &mut BTreeSet<usize>,
        separators: &mut Vec<Option<ParsedSourceLiteral>>,
        data: &mut Vec<ParsedSourceLiteral>,
    ) {
        if !seen.insert(id) {
            return;
        }
        let n = node(snapshot, id);
        if n.name == "Aggregate_GroupConcat" {
            separators.push(match field(n, "separator") {
                None => None,
                Some(SourceSyntaxValue::Literal(literal)) => Some(literal.clone()),
                other => panic!("separator must retain its original literal field: {other:?}"),
            });
        }
        for (_, v) in &n.fields {
            value(snapshot, v, seen, separators, data);
        }
        if let Some(graph) = n.attributes.graph {
            visit(snapshot, graph, seen, separators, data);
        }
    }
    let (mut separators, mut data) = (Vec::new(), Vec::new());
    visit(
        snapshot,
        root,
        &mut BTreeSet::new(),
        &mut separators,
        &mut data,
    );
    (separators, data)
}
fn lowered_separators(p: &GraphPattern, out: &mut Vec<Value>, data: &mut Vec<ParsedSourceLiteral>) {
    match p {
        GraphPattern::SourceContext { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Slice { inner, .. } => lowered_separators(inner, out, data),
        GraphPattern::SourceGroup {
            inner, aggregates, ..
        }
        | GraphPattern::Group {
            inner, aggregates, ..
        } => {
            for (_, a) in aggregates {
                if let AggregateExpression::FunctionCall {
                    name: AggregateFunction::GroupConcat { separator },
                    ..
                } = a
                {
                    out.push(json!(separator));
                }
            }
            lowered_separators(inner, out, data);
        }
        GraphPattern::Join { left, right, .. } => {
            lowered_separators(left, out, data);
            lowered_separators(right, out, data);
        }
        GraphPattern::Values { .. } => (),
        GraphPattern::SourceValues { bindings, .. } => {
            for row in bindings {
                for cell in row.iter().flatten() {
                    if let spargebra::algebra::SourceValueTerm::Literal(literal) = cell {
                        data.push(literal.clone());
                    }
                }
            }
        }
        other => panic!("unexpected actual separator overlay: {other:?}"),
    }
}
fn receive_separator(id: &str) {
    let f = separator_fixture();
    let case = f["payload"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let expected = &case["expected"];
    assert_eq!(expected["runtime_exists_execution"], "not_admitted");
    let s = session(&json!({"input":{"decimal":{"precision":28,"trap_float_operation":false}}}));
    let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
    let mut hooks = registry.hooks();
    let construct = Rc::clone(&hooks.construct);
    let truth = Rc::clone(&hooks.python_truth);
    let originals: Rc<RefCell<BTreeMap<usize, RawExpressionValue>>> = Rc::default();
    let observed = Rc::clone(&originals);
    let owner = Rc::clone(&registry);
    hooks.construct = Rc::new(move |id, input| {
        let description = construct(id.clone(), input)?;
        let mut lookup = description.clone();
        lookup.occurrence = Some(id.clone());
        let raw = owner
            .resolve(&lookup)
            .map_err(|e| -> spargebra::SourceLiteralBridgeError { Box::new(e) })?;
        assert!(observed.borrow_mut().insert(id.index(), raw).is_none());
        Ok(description)
    });
    let (query, _) = SparqlParser::new()
        .parse_query_rdflib760_source_exists_frontend(
            case["input"]["query"].as_str().unwrap(),
            hooks,
            true,
        )
        .unwrap();
    let spargebra::Query::Select { pattern, .. } = query else {
        panic!("SELECT required");
    };
    assert_eq!(
        originals.borrow().len() as u64,
        expected["actual_string_occurrence_count"]
    );
    finished_parser_graph(&pattern, &registry);
    let mut parser_originals = BTreeMap::new();
    parser_snapshots(&pattern, &mut parser_originals);
    assert_eq!(parser_originals.len(), 1);
    let lowered = GraphPattern::from(&pattern);
    finished_plan_graph(&lowered, &registry);
    let reverse: spargebra::algebra::GraphPattern = (&lowered).into();
    finished_parser_graph(&reverse, &registry);
    let again = GraphPattern::from(&reverse);
    finished_plan_graph(&again, &registry);
    let mut retained: Option<(Option<RawExpressionValue>, RawExpressionValue)> = None;
    for plan in [&lowered, &again] {
        let mut occurrences = Vec::new();
        plan_occurrences(plan, &mut occurrences);
        assert_eq!(occurrences.len(), 1);
        let exists = &occurrences[0].value;
        assert_eq!(occurrences[0].target, "b");
        assert_eq!(exists.polarity, SourceExistsPolarity::Exists);
        assert_eq!(&exists.syntax, &parser_originals["b"].2);
        let root = exists.syntax.root_node().unwrap();
        let runtime = root
            .attributes
            .graph
            .expect("BIND EXISTS actual translated attribute required");
        let (separators, data) = separator_overlay_fields(&exists.syntax, runtime);
        assert_eq!(separators.len(), 1);
        assert_eq!(data.len(), 1);
        let data_id = data[0].occurrence.as_ref().unwrap();
        let data_raw = registry.resolve(&data[0]).unwrap();
        assert!(data_raw.same_source_object(originals.borrow().get(&data_id.index()).unwrap()));
        assert_eq!(
            separator_string_fact(&data_raw, truth(data_id).unwrap()),
            expected["data_literal"]
        );
        let separator_raw = if let Some(separator) = &separators[0] {
            assert_eq!(
                expected["aggregate_separator_field"],
                "source_literal_occurrence"
            );
            let separator_id = separator.occurrence.as_ref().unwrap();
            let raw = registry.resolve(separator).unwrap();
            assert!(raw.same_source_object(originals.borrow().get(&separator_id.index()).unwrap()));
            assert_eq!(
                separator_string_fact(&raw, truth(separator_id).unwrap()),
                expected["separator"]
            );
            assert_ne!(separator_id, data_id);
            assert!(!raw.same_source_object(&data_raw));
            assert_eq!(expected["separator_and_data_distinct_occurrences"], true);
            assert_eq!(expected["separator_and_data_distinct_source_objects"], true);
            assert_eq!(
                expected["separator_field_resolves_same_constructed_object"],
                true
            );
            Some(raw)
        } else {
            assert_eq!(expected["aggregate_separator_field"], "absent");
            assert_eq!(expected["no_synthetic_default_space_occurrence"], true);
            None
        };
        let graph = exists
            .graph
            .as_ref()
            .expect("actual executable overlay retained");
        let mut actual = Vec::new();
        let mut lowered_data = Vec::new();
        lowered_separators(graph, &mut actual, &mut lowered_data);
        assert_eq!(lowered_data.len(), 1);
        assert_eq!(lowered_data[0].occurrence, data[0].occurrence);
        assert!(
            registry
                .resolve(&lowered_data[0])
                .unwrap()
                .same_source_object(&data_raw)
        );
        let wanted = expected
            .get("separator")
            .map(|s| s["lexical"].clone())
            .unwrap_or(Value::Null);
        assert_eq!(actual, vec![wanted]);
        if let Some((old_separator, old_data)) = &retained {
            assert!(old_data.same_source_object(&data_raw));
            match (old_separator, &separator_raw) {
                (Some(a), Some(b)) => assert!(a.same_source_object(b)),
                (None, None) => (),
                _ => panic!("separator presence changed"),
            }
        }
        retained = Some((separator_raw, data_raw));
    }
    assert_eq!(
        originals.borrow().len() as u64,
        expected["actual_string_occurrence_count"]
    );
    assert_eq!(flags(&s), expected["numeric_flags"]);
}
#[test]
fn source_exists_separator_fixture_identity() {
    let f = separator_fixture();
    assert_eq!(f["payload"]["cases"].as_array().unwrap().len(), 3);
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&f["payload"])).unwrap())
        ),
        f["payload_sha256"]
    );
}
#[test]
fn source_exists_separator_gcb01_same_text_distinct_occurrences() {
    receive_separator("GCB01");
}
#[test]
fn source_exists_separator_gcb02_empty_string_truth() {
    receive_separator("GCB02");
}
#[test]
fn source_exists_separator_gcb03_absent_is_not_default_occurrence() {
    receive_separator("GCB03");
}

const MAPPING_FIXTURE: &str = include_str!("fixtures/source_exists_mapping_cases.json");
fn mapping_keys(node: &SourceSyntaxNode) -> Value {
    json!(
        node.fields
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>()
    )
}
#[test]
fn source_exists_mapping_absent_fields_and_source_insertion_order() {
    assert_eq!(
        format!("{:x}", Sha256::digest(MAPPING_FIXTURE.as_bytes())),
        "bd0e9c53849614d753a4c07ef7541b7c142a8461f466651c4e65b5a53db4bcf3"
    );
    let f: Value = serde_json::from_str(MAPPING_FIXTURE).unwrap();
    assert_eq!(
        f["source_record_sha256"],
        "83789c4c296bd8d8475d4496acda75cf5ef27fe4df27c4258c3286219a1f06db"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&f["payload"])).unwrap())
        ),
        f["payload_sha256"]
    );
    let cases = f["payload"]["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let s = session(case);
        let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
        let (query, _) = SparqlParser::new()
            .parse_query_rdflib760_source_exists_frontend(
                case["input"]["query"].as_str().unwrap(),
                registry.hooks(),
                true,
            )
            .unwrap();
        let spargebra::Query::Select { pattern, .. } = query else {
            panic!("SELECT required");
        };
        finished_parser_graph(&pattern, &registry);
        let mut parsed = BTreeMap::new();
        parser_snapshots(&pattern, &mut parsed);
        assert_eq!(parsed.len(), 1);
        let lowered = GraphPattern::from(&pattern);
        finished_plan_graph(&lowered, &registry);
        let reverse: spargebra::algebra::GraphPattern = (&lowered).into();
        finished_parser_graph(&reverse, &registry);
        let again = GraphPattern::from(&reverse);
        finished_plan_graph(&again, &registry);
        let expected = &case["expected"];
        for plan in [&lowered, &again] {
            let mut occurrences = Vec::new();
            plan_occurrences(plan, &mut occurrences);
            assert_eq!(occurrences.len(), 1);
            let actual = &occurrences[0];
            let snapshot = &actual.value.syntax;
            assert_eq!(snapshot, &parsed.get(&actual.target).unwrap().2);
            assert_eq!(
                actual.value.graph.is_some(),
                expected["runtime_overlay_present"].as_bool().unwrap()
            );
            let root = snapshot.root_node().unwrap();
            assert_eq!(root.name, "Builtin_EXISTS");
            assert!(root.attributes.graph.is_none());
            assert_eq!(mapping_keys(root), expected["exists_mapping_keys"]);
            let graph = node(snapshot, node_field(root, "graph"));
            assert_eq!(graph.name, expected["raw_graph_operator"]);
            assert_eq!(mapping_keys(graph), expected["raw_graph_mapping_keys"]);
            assert_eq!(
                field(graph, "part").is_some(),
                expected["part_field_present"].as_bool().unwrap()
            );
            if expected["part_field_present"] == true {
                let Some(SourceSyntaxValue::List(parts)) = field(graph, "part") else {
                    panic!("actual part list required");
                };
                assert_eq!(parts.len(), 1);
                let inline = linked(snapshot, &parts[0]);
                assert_eq!(inline.name, expected["inline_operator"]);
                assert_eq!(mapping_keys(inline), expected["inline_mapping_keys"]);
                for absent in expected["inline_absent_fields"].as_array().unwrap() {
                    assert!(field(inline, absent.as_str().unwrap()).is_none());
                }
                if let Some(wanted) = expected.get("inline_variables") {
                    let Some(SourceSyntaxValue::List(variables)) = field(inline, "var") else {
                        panic!("actual var list required");
                    };
                    assert_eq!(
                        &json!(variables.iter().map(variable).collect::<Vec<_>>()),
                        wanted
                    );
                }
                if let Some(wanted) = expected.get("inline_rows") {
                    let Some(SourceSyntaxValue::List(rows)) = field(inline, "value") else {
                        panic!("actual value list required");
                    };
                    assert_eq!(wanted, &json!([[]]));
                    assert_eq!(rows.len(), 1);
                    let SourceSyntaxValue::List(row) = &rows[0] else {
                        panic!("actual empty row retained");
                    };
                    assert!(row.is_empty());
                }
            }
        }
    }
}

// B-PROJECTION-INSTANCE-EXPR-01: original BX07 mapping and translated
// property read are separate. This is AST receiving, not EXISTS execution.
const PROJECTION_INSTANCE_FIXTURE: &str =
    include_str!("fixtures/source_exists_projection_instance_cases.json");

fn projection_scope_members(actual: &SourceSyntaxNode) -> Value {
    let Some(SourceSyntaxValue::VariableSet(variables)) = field(actual, "_vars") else {
        panic!("source mapping scope is absent on {}", actual.name);
    };
    let mut variables = variables.iter().map(|v| v.as_str()).collect::<Vec<_>>();
    variables.sort();
    let mut convenience = actual
        .attributes
        .variables
        .as_ref()
        .expect("mapping scope convenience projection must exist")
        .iter()
        .map(|v| v.as_str())
        .collect::<Vec<_>>();
    convenience.sort();
    assert_eq!(variables, convenience);
    json!(variables)
}

#[test]
fn source_exists_projection_instance_expr_preserves_mapping_aggregate() {
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(PROJECTION_INSTANCE_FIXTURE.as_bytes())
        ),
        "f02232a7b41196c09e3a8c05e329d2b0aae5e63e09391363bb3f0b4f8db8e92f"
    );
    let f: Value = serde_json::from_str(PROJECTION_INSTANCE_FIXTURE).unwrap();
    assert_eq!(
        f["source_record_sha256"],
        "d6bf5043872a226d91f9f18027609fb4b8178b7de20a859ef2fb907417728fc1"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&f["payload"])).unwrap())
        ),
        f["payload_sha256"]
    );
    assert_eq!(f["payload"]["cases"].as_array().unwrap().len(), 1);
    let case = &f["payload"]["cases"][0];
    let expected = &case["expected"];
    let s = session(case);
    let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
    let (query, _) = SparqlParser::new()
        .parse_query_rdflib760_source_exists_frontend(
            case["input"]["query"].as_str().unwrap(),
            registry.hooks(),
            true,
        )
        .unwrap();
    let spargebra::Query::Select { pattern, .. } = query else {
        panic!("SELECT required");
    };
    finished_parser_graph(&pattern, &registry);
    let mut original = BTreeMap::new();
    parser_snapshots(&pattern, &mut original);
    assert_eq!(original.len(), 1);
    let lowered = GraphPattern::from(&pattern);
    finished_plan_graph(&lowered, &registry);
    let reverse: spargebra::algebra::GraphPattern = (&lowered).into();
    finished_parser_graph(&reverse, &registry);
    let again = GraphPattern::from(&reverse);
    finished_plan_graph(&again, &registry);
    for plan in [&lowered, &again] {
        let mut occurrences = Vec::new();
        plan_occurrences(plan, &mut occurrences);
        assert_eq!(occurrences.len(), 1);
        let actual = &occurrences[0];
        assert_eq!(actual.target, "b");
        let snapshot = &actual.value.syntax;
        assert_eq!(snapshot, &original.get("b").unwrap().2);
        let root = snapshot.root_node().unwrap();
        assert_eq!(root.name, expected["exists_operator"]);
        let graph = node(snapshot, node_field(root, "graph"));
        assert_eq!(graph.name, expected["mapping_graph_operator"]);
        let projection = linked(snapshot, first(field(graph, "projection").unwrap()));
        assert_eq!(projection.name, expected["mapping_projection_operator"]);
        // The old implementation fails here: it replaced this mapping root
        // with a Variable instead of assigning the Python instance property.
        let aggregate_id = node_field(projection, "expr");
        let aggregate = node(snapshot, aggregate_id);
        assert_eq!(aggregate.name, expected["mapping_aggregate_operator"]);
        let Some(SourceSyntaxValue::Text(distinct)) = field(aggregate, "distinct") else {
            panic!("actual DISTINCT text required");
        };
        assert_eq!(distinct, expected["aggregate_distinct"].as_str().unwrap());
        assert_eq!(
            variable(field(aggregate, "vars").unwrap()),
            expected["aggregate_vars"]
        );
        assert_eq!(
            variable(field(aggregate, "res").unwrap()),
            expected["aggregate_res"]
        );
        for (label, n) in [
            ("exists", root),
            ("graph", graph),
            ("projection", projection),
            ("aggregate", aggregate),
        ] {
            assert_eq!(
                projection_scope_members(n),
                expected["mapping_scope_members"][label]
            );
        }
        let overlay = node(
            snapshot,
            root.attributes.graph.expect("actual translated graph"),
        );
        let projected = node(snapshot, node_field(overlay, "p"));
        let extended = node(snapshot, node_field(projected, "p"));
        let joined = node(snapshot, node_field(extended, "p"));
        let spine = [overlay, projected, extended, joined];
        assert_eq!(
            json!(spine.iter().map(|n| n.name.as_str()).collect::<Vec<_>>()),
            expected["translated_overlay_spine"]
        );
        assert_eq!(
            json!(
                spine
                    .iter()
                    .map(|n| field(n, "_vars").is_some())
                    .collect::<Vec<_>>()
            ),
            expected["overlay_spine_scope_fields_present"]
        );
        assert!(spine.iter().all(|n| n.attributes.variables.is_none()));
        assert_eq!(
            variable(field(extended, "expr").unwrap()),
            expected["translated_extend_expr"]
        );
        assert_eq!(
            variable(field(extended, "var").unwrap()),
            expected["translated_extend_var"]
        );
        // The observer did not serialize this property: its expected value
        // is source-derived, separately from the observed translated Extend.
        let instance_expr = projection
            .attributes
            .expr
            .as_ref()
            .expect("source projection has an actual instance expr override");
        assert_eq!(
            variable(instance_expr),
            expected["instance_projection_expr"]["value"]
        );
        assert_eq!(instance_expr, field(extended, "expr").unwrap());
        let Some(SourceSyntaxValue::List(aggregates)) = field(joined, "A") else {
            panic!("actual AggregateJoin list required");
        };
        assert_eq!(aggregates.len(), 1);
        let SourceSyntaxValue::Node(selected_id) = &aggregates[0] else {
            panic!("actual aggregate node link required");
        };
        assert!(
            expected["same_mapping_aggregate_and_overlay_A_node"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(*selected_id, aggregate_id);

        // Also inspect the lowered real graph: a correct syntax snapshot with
        // an unrelated executable Extend must not pass the transport check.
        let GraphPattern::Project { inner, .. } =
            strip_context(actual.value.graph.as_deref().unwrap())
        else {
            panic!("actual translated Project required");
        };
        let GraphPattern::Extend {
            variable: output,
            expression: Expression::Variable(value),
            inner,
        } = strip_context(inner)
        else {
            panic!("actual translated Extend variable required");
        };
        assert_eq!(output.as_str(), expected["translated_extend_var"]);
        assert_eq!(value.as_str(), expected["translated_extend_expr"]);
        let GraphPattern::SourceGroup { aggregates, .. } = strip_context(inner) else {
            panic!("actual translated SourceGroup required");
        };
        assert_eq!(aggregates.len(), 1);
        let (
            res,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Sum,
                expr: Expression::Variable(value),
                distinct: true,
            },
        ) = &aggregates[0]
        else {
            panic!("actual DISTINCT SUM source aggregate required");
        };
        assert_eq!(res.as_str(), expected["aggregate_res"]);
        assert_eq!(value.as_str(), expected["aggregate_vars"]);
    }
    assert_eq!(flags(&s), expected["flags_after_frontend"]);
}
