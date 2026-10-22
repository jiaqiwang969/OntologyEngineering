//! Exercise real existing evaluator boundaries with explicit internal cells.
//! These are source-derived carrier controls, not public nullable-query parity.
use super::*;
use crate::QueryEvaluator;
use crate::dataset::TermCow;
use crate::tuple::ErrorValue;
use oxrdf::Dataset;

fn run(query: &str, cell: Option<SolutionValue<TermCow<'_>>>) -> Vec<PresenceSolution> {
    let dataset = Dataset::new();
    let evaluator = QueryEvaluator::new()
        .simple_evaluator(&dataset, QueryDatasetSpecification::new(), &None)
        .unwrap();
    let parsed = spargebra::SparqlParser::new().parse_query(query).unwrap();
    let spargebra::Query::Select { pattern, .. } = parsed else {
        panic!("SELECT control required")
    };
    let pattern = GraphPattern::from(&pattern);
    let mut variables = vec![Variable::new("x").unwrap()];
    let (plan, _) = evaluator.graph_pattern_evaluator(&pattern, &mut variables);
    let mut from = InternalTuple::with_capacity(variables.len());
    if let Some(cell) = cell {
        from.set_cell(0, cell);
    }
    decode_presence_bindings(
        evaluator.dataset.clone(),
        plan.unwrap()(from).expect("legacy query starts successfully"),
        variables.into(),
    )
    .collect::<Result<_, _>>()
    .unwrap()
}

#[test]
fn nullable_cells_cross_real_project_join_union_and_distinct() {
    let query = "SELECT ?x ?y ?z WHERE { VALUES ?y { <urn:a> } VALUES ?z { <urn:b> } }";
    let rows = run(query, Some(SolutionValue::ExplicitNone));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].values[0], Some(SolutionValue::ExplicitNone));
    assert_eq!(
        rows[0].values[1],
        Some(SolutionValue::Rdf(NamedNode::new("urn:a").unwrap().into()))
    );
    assert_eq!(
        rows[0].values[2],
        Some(SolutionValue::Rdf(NamedNode::new("urn:b").unwrap().into()))
    );
    let absent = run(query, None);
    assert_eq!(absent.len(), 1);
    assert_eq!(absent[0].values[0], None);
    let union = "SELECT ?x ?y WHERE { { VALUES ?y { <urn:a> } } UNION { VALUES ?y { <urn:b> } } }";
    let rows = run(union, Some(SolutionValue::ExplicitNone));
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.values[0] == Some(SolutionValue::ExplicitNone))
    );
    let distinct = "SELECT DISTINCT ?x WHERE { { } UNION { } }";
    let rows = run(distinct, Some(SolutionValue::ExplicitNone));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].values, vec![Some(SolutionValue::ExplicitNone)]);
}

#[test]
fn nullable_cells_use_real_minus_domain_and_hash_set_without_invented_terms() {
    let mut none = InternalTuple::<&str>::with_capacity(1);
    none.set_cell(0, SolutionValue::ExplicitNone);
    let absent = InternalTuple::with_capacity(1);
    let mut term = InternalTuple::with_capacity(1);
    term.set(0, "urn:t");
    assert!(are_compatible_and_not_disjointed(&none, &none));
    assert!(!are_compatible_and_not_disjointed(&none, &absent));
    assert!(!are_compatible_and_not_disjointed(&none, &term));
    let mut set = InternalTupleSet::new(vec![0]);
    set.insert(none.clone());
    set.insert(absent.clone());
    set.insert(term);
    assert_eq!(set.len(), 3);
    assert_eq!(set.get(&none), &[none.clone()]);
    let input: Vec<Result<_, QueryEvaluationError>> =
        vec![Ok(none.clone()), Ok(absent.clone()), Ok(none)];
    let deduplicated: Vec<_> = hash_deduplicate(input.into_iter())
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(deduplicated.len(), 2);
    assert!(deduplicated.contains(&absent));
    let rows = run(
        "SELECT ?x WHERE { {} MINUS { VALUES ?x { <urn:t> } } }",
        Some(SolutionValue::ExplicitNone),
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].values, vec![Some(SolutionValue::ExplicitNone)]);
}

#[test]
fn nullable_cells_decoder_preserves_domain_and_refuses_lossy_legacy_export() {
    let none = run("SELECT ?x WHERE {}", Some(SolutionValue::ExplicitNone));
    assert!(matches!(
        none.into_iter().next().unwrap().into_legacy(),
        Err(QueryEvaluationError::UnrepresentableSolutionCell {
            kind: "explicit_none",
            ..
        })
    ));
    let absent = run("SELECT ?x WHERE {}", None);
    let legacy = absent.into_iter().next().unwrap().into_legacy().unwrap();
    assert!(legacy.get("x").is_none());
    let error = ErrorValue::new("source-derived internal error control");
    let rows = run(
        "SELECT ?x WHERE {}",
        Some(SolutionValue::ReturnedError(error.clone())),
    );
    assert_eq!(
        rows[0].values,
        vec![Some(SolutionValue::ReturnedError(error))]
    );
    assert!(matches!(
        rows.into_iter().next().unwrap().into_legacy(),
        Err(QueryEvaluationError::UnrepresentableSolutionCell {
            kind: "returned_error",
            ..
        })
    ));
    let dataset = Dataset::new();
    let eval = QueryEvaluator::new()
        .simple_evaluator(&dataset, QueryDatasetSpecification::new(), &None)
        .unwrap();
    let variables: Arc<[Variable]> = vec![Variable::new("x").unwrap()].into();
    let mut rows = decode_presence_bindings(
        eval.dataset.clone(),
        Box::new(once(Err(QueryEvaluationError::Cancelled))),
        Arc::clone(&variables),
    );
    assert!(matches!(
        rows.next().unwrap(),
        Err(QueryEvaluationError::Cancelled)
    ));
    let mut bad = InternalTuple::with_capacity(3);
    bad.set_cell(2, SolutionValue::ExplicitNone);
    let mut rows = decode_presence_bindings(eval.dataset, Box::new(once(Ok(bad))), variables);
    assert!(matches!(
        rows.next().unwrap(),
        Err(QueryEvaluationError::InvalidSolutionDomain(2))
    ));
}

#[test]
fn nullable_cells_do_not_turn_present_none_into_a_bgp_wildcard_assignment() {
    let dataset = Dataset::new();
    let eval = QueryEvaluator::new()
        .simple_evaluator(&dataset, QueryDatasetSpecification::new(), &None)
        .unwrap();
    let term = eval
        .dataset
        .internalize_term(NamedNode::new("urn:t").unwrap().into())
        .unwrap();
    let selector = TupleSelector::Variable(0);
    let mut none = InternalTuple::with_capacity(1);
    none.set_cell(0, SolutionValue::ExplicitNone);
    assert!(
        !put_pattern_value::<&Dataset>(
            &selector,
            term.clone(),
            &mut none,
            #[cfg(feature = "sparql-12")]
            &eval.dataset
        )
        .unwrap()
    );
    assert!(matches!(none.cell(0), Some(SolutionValue::ExplicitNone)));
    let mut absent = InternalTuple::with_capacity(1);
    assert!(
        put_pattern_value::<&Dataset>(
            &selector,
            term.clone(),
            &mut absent,
            #[cfg(feature = "sparql-12")]
            &eval.dataset
        )
        .unwrap()
    );
    assert!(absent.get(0) == Some(&term));
}

#[test]
fn nullable_variable_consumers_never_replace_a_present_none_with_initial_rdf() {
    let dataset = Dataset::new();
    let x = Variable::new("x").unwrap();
    let initial = NamedNode::new("urn:initial").unwrap();
    let evaluator = QueryEvaluator::new()
        .simple_evaluator(&dataset, QueryDatasetSpecification::new(), &None)
        .unwrap()
        .with_initial_binding_context(&[(x.clone(), initial.clone().into())])
        .unwrap();
    let mut variables = vec![x.clone()];
    let mut stats = Vec::new();
    let lookup = evaluator
        .expression_evaluator(&Expression::Variable(x.clone()), &mut variables, &mut stats)
        .unwrap();
    let bound = evaluator
        .effective_boolean_value_expression_evaluator(
            &Expression::Bound(x),
            &mut variables,
            &mut stats,
        )
        .unwrap();
    let mut row = InternalTuple::with_capacity(1);
    assert!(lookup(&row) == Some(ExpressionTerm::from(Term::from(initial))));
    assert_eq!(bound(&row), Some(true));
    row.set_cell(0, SolutionValue::ExplicitNone);
    assert!(lookup(&row).is_none());
    assert_eq!(bound(&row), Some(false));
    let local = NamedNode::new("urn:local").unwrap();
    row.set(
        0,
        evaluator
            .dataset
            .internalize_term(local.clone().into())
            .unwrap(),
    );
    assert!(lookup(&row) == Some(ExpressionTerm::from(Term::from(local))));
    assert_eq!(bound(&row), Some(true));
    // This is the old RDF-only expression boundary. A complete returned-error
    // channel is separate remaining work, and must not be inferred from None.
    row.set_cell(
        0,
        SolutionValue::ReturnedError(ErrorValue::new("stored error")),
    );
    assert!(lookup(&row).is_none());
    assert_eq!(bound(&row), Some(false));
}
