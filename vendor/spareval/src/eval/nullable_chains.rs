//! Real parser -> shared unoptimized planner -> typed consumer controls.
//! These finite internal tests do not enable a public nullable query profile.
use super::*;
use crate::QueryEvaluator;
use crate::tuple::SparqlErrorKind;
use oxrdf::Dataset;
use spargebra::{Query, SparqlParser};
use std::cell::Cell;

fn planned(query: &str) -> GraphPattern {
    let (query, _) = SparqlParser::new()
        .parse_query_rdflib760_nullable(query)
        .unwrap();
    let Query::Select { pattern, .. } = query else {
        panic!("SELECT required");
    };
    QueryEvaluator::new()
        .without_optimizations()
        .plan_graph_pattern(&pattern)
}

fn execute(query: &str) -> Vec<Result<PresenceSolution, QueryEvaluationError>> {
    let pattern = planned(query);
    let dataset = Dataset::new();
    let mut evaluator = QueryEvaluator::new()
        .without_optimizations()
        .simple_evaluator(&dataset, QueryDatasetSpecification::new(), &None)
        .unwrap();
    evaluator.nullable_expressions = true;
    let mut variables = Vec::new();
    let (plan, _) = evaluator.graph_pattern_evaluator(&pattern, &mut variables);
    let input = InternalTuple::with_capacity(variables.len());
    decode_presence_bindings(evaluator.dataset, plan.unwrap()(input).expect("legacy query starts successfully"), variables.into()).collect()
}

fn literal(query: &str) -> Literal {
    let mut rows = execute(query);
    assert_eq!(rows.len(), 1);
    let row = rows.pop().unwrap().unwrap();
    let index = row
        .variables
        .iter()
        .position(|v| v.as_str() == "result")
        .unwrap();
    match row.values[index].clone() {
        Some(SolutionValue::Rdf(Term::Literal(value))) => value,
        _ => panic!("A literal result is required"),
    }
}

// Extract the actual parsed and lowered projection expression to observe its
// returned error class before Extend applies its own row-retention policy.
fn raw(
    expression: &str,
) -> Result<SolutionValue<ExpressionTerm>, EvalFailure<QueryEvaluationError>> {
    raw_with_dataset(expression, &Dataset::new(), &[])
}

fn raw_with_dataset<'a, D: QueryableDataset<'a>>(
    expression: &str,
    dataset: D,
    bindings: &[(Variable, Term)],
) -> Result<SolutionValue<ExpressionTerm>, EvalFailure<QueryEvaluationError>> {
    let plan = planned(&format!("SELECT ({expression} AS ?result) WHERE {{}}"));
    let GraphPattern::Project { inner, .. } = plan else {
        panic!("projection required")
    };
    let GraphPattern::Extend { expression, .. } = *inner else {
        panic!("extension required")
    };
    let mut evaluator = QueryEvaluator::new()
        .without_optimizations()
        .simple_evaluator(dataset, QueryDatasetSpecification::new(), &None)
        .unwrap()
        .with_initial_binding_context(bindings)
        .unwrap();
    evaluator.nullable_expressions = true;
    let mut variables = Vec::new();
    let mut stats = Vec::new();
    let mut context = ExpressionContext {
        evaluator: &evaluator,
        encoded_variables: &mut variables,
        stat_children: &mut stats,
    };
    let compiled = build_raw_expression_evaluator(&expression, &mut context).unwrap();
    compiled(&InternalTuple::with_capacity(variables.len()))
}

#[derive(Debug)]
struct ChainFailureIdentity;

#[derive(Clone, Debug)]
struct ChainConversionFailure(Arc<ChainFailureIdentity>);

impl std::fmt::Display for ChainConversionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("chain tail externalization failed")
    }
}

impl std::error::Error for ChainConversionFailure {}

struct ChainFailingDataset {
    externalizations: Rc<Cell<usize>>,
    failure: ChainConversionFailure,
}

impl<'a> QueryableDataset<'a> for ChainFailingDataset {
    type InternalTerm = Term;
    type Error = ChainConversionFailure;

    fn internal_quads_for_pattern(
        &self,
        _subject: Option<&Term>,
        _predicate: Option<&Term>,
        _object: Option<&Term>,
        _graph_name: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        empty()
    }

    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        Ok(term)
    }

    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        assert_eq!(
            term,
            Term::NamedNode(NamedNode::new("urn:chain-fatal-tail").unwrap())
        );
        self.externalizations.set(self.externalizations.get() + 1);
        Err(self.failure.clone())
    }
}

#[test]
fn parsed_chain_raw_lookup_stops_at_first_raised_failure() {
    for (expression, expected_calls) in [
        ("?missing + 1 + ?fatal", 0),
        ("(?missing + 1) + ?fatal", 1),
        ("1 / 0 * ?fatal", 1),
    ] {
        let externalizations = Rc::new(Cell::new(0));
        let identity = Arc::new(ChainFailureIdentity);
        let result = raw_with_dataset(
            expression,
            ChainFailingDataset {
                externalizations: Rc::clone(&externalizations),
                failure: ChainConversionFailure(Arc::clone(&identity)),
            },
            &[(
                Variable::new("fatal").unwrap(),
                NamedNode::new("urn:chain-fatal-tail").unwrap().into(),
            )],
        );
        assert_eq!(externalizations.get(), expected_calls, "{expression}");
        if expected_calls == 0 {
            assert!(
                matches!(result, Ok(SolutionValue::ReturnedError(error)) if error.kind() == SparqlErrorKind::NotBound),
                "{expression}"
            );
        } else {
            let Err(EvalFailure::Fatal(QueryEvaluationError::Dataset(error))) = result else {
                panic!("{expression}: expected the original Dataset failure");
            };
            let source = error
                .downcast_ref::<ChainConversionFailure>()
                .expect("the original Dataset error type must survive");
            assert!(Arc::ptr_eq(&source.0, &identity), "{expression}");
            assert_eq!(source.to_string(), "chain tail externalization failed");
        }
        assert_eq!(externalizations.get(), expected_calls, "{expression}");
    }
}

#[test]
fn parsed_chains_execute_through_real_projection_in_source_order() {
    for (expression, value, datatype) in [
        ("2*3*4", "24", "decimal"),
        ("2+3*4", "14", "decimal"),
        ("-(2*3)", "-6", "decimal"),
        ("12/3/2", "2", "decimal"),
        ("10-3-2", "5", "integer"),
        ("10-(3-2)", "9", "integer"),
        ("(2*3)+1", "7", "decimal"),
        ("0*1", "0", "decimal"),
    ] {
        let actual = literal(&format!("SELECT ({expression} AS ?result) WHERE {{}}"));
        assert_eq!(actual.value(), value, "{expression}");
        assert_eq!(
            actual.datatype().as_str(),
            format!("http://www.w3.org/2001/XMLSchema#{datatype}")
        );
    }
}

#[test]
fn parsed_flat_and_nested_chains_keep_distinct_error_boundaries() {
    for (expression, expected) in [
        ("1/0*1", SparqlErrorKind::OtherSparql),
        ("(1/0)*1", SparqlErrorKind::Type),
        ("1/0 * ?missing", SparqlErrorKind::NotBound),
    ] {
        assert!(
            matches!(raw(expression), Ok(SolutionValue::ReturnedError(error)) if error.kind() == expected),
            "{expression}"
        );
    }
    // All raw tail expressions are evaluated before the first arithmetic op.
    assert!(matches!(raw("1/0 * (1/2)"), Err(EvalFailure::Fatal(_))));
}

#[test]
fn parsed_aggregate_parentheses_distinguish_missing_from_returned_error() {
    for (expression, expected) in [
        ("(?missing)", "0"),
        ("?missing+1", "3"),
        ("1/0*1", "3"),
        ("(1/0)*1", "3"),
    ] {
        let query =
            format!("SELECT (COUNT({expression}) AS ?result) WHERE {{ VALUES ?row {{ 1 2 3 }} }}");
        assert_eq!(literal(&query).value(), expected, "{expression}");
    }
    let rows = execute("SELECT (SAMPLE((?missing)) AS ?result) WHERE { VALUES ?row { 1 2 3 } }");
    assert_eq!(rows.len(), 1);
    let row = rows.into_iter().next().unwrap().unwrap();
    let index = row
        .variables
        .iter()
        .position(|v| v.as_str() == "result")
        .unwrap();
    assert!(matches!(
        row.values[index],
        Some(SolutionValue::ExplicitNone)
    ));
}

#[test]
fn parsed_unsupported_numeric_steps_abort_real_consumers() {
    for expression in ["1/2", "0/-1", "0*-1", "-1*0", "(0*-1)*-1", "-(0*-1)"] {
        assert!(
            matches!(raw(expression), Err(EvalFailure::Fatal(_))),
            "{expression}"
        );
        for query in [
            format!("SELECT ({expression} AS ?result) WHERE {{ VALUES ?row {{ 1 2 3 }} }}"),
            format!("SELECT (COUNT({expression}) AS ?result) WHERE {{ VALUES ?row {{ 1 2 3 }} }}"),
            format!("SELECT ?row WHERE {{ VALUES ?row {{ 1 2 3 }} FILTER({expression}) }}"),
        ] {
            let rows = execute(&query);
            assert_eq!(rows.len(), 1, "{query}");
            assert!(rows[0].is_err(), "{query}");
        }
    }
}

#[test]
fn parsed_decimal_variable_does_not_inherit_arithmetic_certification() {
    let rows = execute("SELECT (?v+1 AS ?result) WHERE { BIND(2*3 AS ?v) }");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_err());
    assert!(matches!(
        raw("-9223372036854775808"),
        Err(EvalFailure::Fatal(_))
    ));
}

#[test]
fn parsed_chain_reaching_legacy_compiler_is_rejected() {
    let plan = planned("SELECT (2*3*4 AS ?result) WHERE {}");
    let dataset = Dataset::new();
    let evaluator = QueryEvaluator::new()
        .without_optimizations()
        .simple_evaluator(&dataset, QueryDatasetSpecification::new(), &None)
        .unwrap();
    assert!(!evaluator.nullable_expressions);
    let (plan, _) = evaluator.graph_pattern_evaluator(&plan, &mut Vec::new());
    assert!(plan.is_err());
}
