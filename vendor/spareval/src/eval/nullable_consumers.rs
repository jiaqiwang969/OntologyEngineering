//! Frozen source-derived controls against the actual evaluator. Direct algebra
//! deliberately avoids claiming that the public parser preserves nullable ASTs.
use super::nullable::NullableAggregateEvaluator;
use super::*;
use crate::QueryEvaluator;
use crate::dataset::TermCow;
use crate::tuple::{ErrorValue, SparqlErrorKind};
use oxrdf::Dataset;

fn variable(name: &str) -> Variable {
    Variable::new(name).unwrap()
}
fn number(n: i64) -> Expression {
    Expression::Literal(Literal::from(n))
}
fn divide(a: i64, b: i64) -> Expression {
    Expression::Divide(Box::new(number(a)), Box::new(number(b)))
}
fn rows(n: usize) -> GraphPattern {
    GraphPattern::Values {
        variables: vec![],
        bindings: vec![vec![]; n],
    }
}
fn group(
    inner: GraphPattern,
    expr: Expression,
    function: AggregateFunction,
    distinct: bool,
) -> GraphPattern {
    GraphPattern::Group {
        inner: Box::new(inner),
        variables: vec![],
        aggregates: vec![(
            variable("result"),
            AggregateExpression::FunctionCall {
                name: function,
                expr,
                distinct,
            },
        )],
    }
}
fn evaluator(dataset: &Dataset) -> SimpleEvaluator<'_, &Dataset> {
    let mut evaluator = QueryEvaluator::new()
        .simple_evaluator(dataset, QueryDatasetSpecification::new(), &None)
        .unwrap();
    evaluator.nullable_expressions = true;
    evaluator
}
fn execute(
    pattern: GraphPattern,
    cell: Option<SolutionValue<TermCow<'_>>>,
    nullable: bool,
) -> Vec<Result<PresenceSolution, QueryEvaluationError>> {
    let dataset = Dataset::new();
    let mut evaluator = evaluator(&dataset);
    evaluator.nullable_expressions = nullable;
    let mut variables = vec![variable("x")];
    let (plan, _) = evaluator.graph_pattern_evaluator(&pattern, &mut variables);
    let mut row = InternalTuple::with_capacity(variables.len());
    if let Some(cell) = cell {
        row.set_cell(0, cell);
    }
    decode_presence_bindings(evaluator.dataset, plan.unwrap()(row).expect("legacy query starts successfully"), variables.into()).collect()
}
fn result_count(rows: Vec<Result<PresenceSolution, QueryEvaluationError>>) -> i64 {
    assert_eq!(rows.len(), 1);
    let row = rows.into_iter().next().unwrap().unwrap();
    let value = row
        .variables
        .iter()
        .position(|v| v.as_str() == "result")
        .unwrap();
    match &row.values[value] {
        Some(SolutionValue::Rdf(Term::Literal(literal))) => literal.value().parse().unwrap(),
        _ => panic!("integer count required"),
    }
}

#[test]
fn raw_variable_and_operand_keep_none_error_and_initial_scope() {
    let dataset = Dataset::new();
    let evaluator = evaluator(&dataset)
        .with_initial_binding_context(&[(variable("x"), Literal::from(7).into())])
        .unwrap();
    let mut variables = vec![variable("x")];
    let mut stats = vec![];
    let mut context = ExpressionContext {
        evaluator: &evaluator,
        encoded_variables: &mut variables,
        stat_children: &mut stats,
    };
    let raw =
        build_raw_expression_evaluator(&Expression::Variable(variable("x")), &mut context).unwrap();
    let add = build_raw_expression_evaluator(
        &Expression::Add(
            Box::new(Expression::Variable(variable("x"))),
            Box::new(number(1)),
        ),
        &mut context,
    )
    .unwrap();
    let mut row = InternalTuple::with_capacity(1);
    assert!(
        matches!(raw(&row), Ok(SolutionValue::Rdf(ExpressionTerm::IntegerLiteral(n))) if i64::from(n) == 7)
    );
    row.set_cell(0, SolutionValue::ExplicitNone);
    assert!(matches!(raw(&row), Ok(SolutionValue::ExplicitNone)));
    assert!(
        matches!(add(&row), Ok(SolutionValue::ReturnedError(error)) if error.kind() == SparqlErrorKind::NotBound)
    );
    let stored = ErrorValue::with_kind(SparqlErrorKind::Type, "retained object");
    row.set_cell(0, SolutionValue::ReturnedError(stored.clone()));
    assert!(matches!(raw(&row), Ok(SolutionValue::ReturnedError(error)) if error == stored));
    assert!(matches!(add(&row), Ok(SolutionValue::ReturnedError(error)) if error == stored));
    let missing =
        build_raw_expression_evaluator(&Expression::Variable(variable("missing")), &mut context)
            .unwrap();
    assert!(
        matches!(missing(&row), Err(EvalFailure::Sparql(error)) if error.kind() == SparqlErrorKind::NotBound)
    );
}

#[test]
fn real_count_and_sample_preserve_returned_cells() {
    let x = Expression::Variable(variable("x"));
    let count = |expr, distinct| group(rows(3), expr, AggregateFunction::Count, distinct);
    assert_eq!(
        result_count(execute(count(x.clone(), false), None, true)),
        0
    );
    assert_eq!(
        result_count(execute(
            count(x.clone(), false),
            Some(SolutionValue::ExplicitNone),
            true
        )),
        3
    );
    let error = ErrorValue::new("same object");
    assert_eq!(
        result_count(execute(
            count(x.clone(), false),
            Some(SolutionValue::ReturnedError(error.clone())),
            true
        )),
        3
    );
    assert_eq!(
        result_count(execute(
            count(x.clone(), true),
            Some(SolutionValue::ReturnedError(error.clone())),
            true
        )),
        1
    );
    assert_eq!(
        result_count(execute(count(divide(1, 0), false), None, true)),
        3
    );
    assert_eq!(
        result_count(execute(count(divide(1, 0), true), None, true)),
        3
    );
    assert_eq!(
        result_count(execute(count(divide(1, 0), false), None, false)),
        0
    );
    let missing_add = Expression::Add(Box::new(x.clone()), Box::new(number(1)));
    assert_eq!(
        result_count(execute(count(missing_add, false), None, true)),
        3
    );
    for cell in [
        None,
        Some(SolutionValue::ExplicitNone),
        Some(SolutionValue::ReturnedError(error.clone())),
    ] {
        let output = execute(
            group(rows(3), x.clone(), AggregateFunction::Sample, false),
            cell.clone(),
            true,
        );
        let output = output.into_iter().next().unwrap().unwrap();
        let position = output
            .variables
            .iter()
            .position(|v| v.as_str() == "result")
            .unwrap();
        assert_eq!(
            output.values[position],
            Some(match cell {
                Some(SolutionValue::ReturnedError(error)) => SolutionValue::ReturnedError(error),
                _ => SolutionValue::ExplicitNone,
            })
        );
    }
}

#[test]
fn distinct_count_uses_two_evaluations_and_second_identity() {
    let calls = Rc::new(Cell::new(0));
    let sequence = Rc::new(std::cell::RefCell::new(
        vec![
            Ok(SolutionValue::Rdf("A")),
            Ok(SolutionValue::Rdf("B")),
            Ok(SolutionValue::Rdf("B")), // Already seen: no second call.
            Ok(SolutionValue::Rdf("C")),
            Err(EvalFailure::Sparql(ErrorValue::with_kind(
                SparqlErrorKind::NotBound,
                "second evaluation unbound",
            ))),
        ]
        .into_iter(),
    ));
    let measured = Rc::clone(&calls);
    let mut counter = NullableAccumulator::Count {
        evaluator: NullableAggregateEvaluator::Internal(Rc::new(move |_| {
            measured.set(measured.get() + 1);
            sequence.borrow_mut().next().expect("no surplus evaluation")
        })),
        seen: Some(FxHashSet::default()),
        count: 0,
    };
    let row = InternalTuple::with_capacity(0);
    for _ in 0..3 {
        counter.accumulate(&row).unwrap();
    }
    assert_eq!(calls.get(), 5);
    let NullableAccumulator::Count {
        count,
        seen: Some(seen),
        ..
    } = counter
    else {
        panic!()
    };
    assert_eq!(count, 1);
    assert!(seen.contains(&AggregateValue::Internal(SolutionValue::Rdf("B"))));
    assert!(!seen.contains(&AggregateValue::Internal(SolutionValue::Rdf("A"))));
    assert!(!seen.contains(&AggregateValue::Internal(SolutionValue::Rdf("C"))));
}

#[test]
fn sample_none_stops_later_evaluation() {
    let calls = Rc::new(Cell::new(0));
    let measured = Rc::clone(&calls);
    let mut sample: NullableAccumulator<'_, &str> = NullableAccumulator::Sample {
        evaluator: NullableAggregateEvaluator::Internal(Rc::new(move |_| {
            let n = measured.get();
            measured.set(n + 1);
            match n {
                0 => Err(EvalFailure::Sparql(ErrorValue::with_kind(
                    SparqlErrorKind::NotBound,
                    "absent",
                ))),
                1 => Ok(SolutionValue::ExplicitNone),
                _ => Err(EvalFailure::Fatal(QueryEvaluationError::Cancelled)),
            }
        })),
        selected: None,
    };
    for _ in 0..4 {
        sample.accumulate(&InternalTuple::with_capacity(0)).unwrap();
    }
    assert_eq!(calls.get(), 2);
    assert!(matches!(
        sample.finish().unwrap(),
        Some(AggregateValue::Internal(SolutionValue::ExplicitNone))
    ));
    for function in [AggregateFunction::Count, AggregateFunction::Sample] {
        let dataset = Dataset::new();
        let evaluator = evaluator(&dataset);
        let mut variables = vec![];
        let mut stats = vec![];
        let expression = AggregateExpression::FunctionCall {
            name: function,
            distinct: false,
            expr: Expression::Variable(variable("x")),
        };
        let mut accumulator = evaluator
            .accumulator_builder(&expression, &mut variables, &mut stats)
            .unwrap()();
        evaluator.dataset.cancellation_token.cancel();
        assert!(matches!(
            accumulator.accumulate(&InternalTuple::with_capacity(1)),
            Err(QueryEvaluationError::Cancelled)
        ));
    }
}

#[test]
fn real_group_aborts_on_consumption_fatal() {
    // First group has a valid row, then another group hits unsupported numeric
    // rounding. No count from the first group may escape AggregateJoin.
    let x = variable("x");
    let input = GraphPattern::Values {
        variables: vec![x.clone()],
        bindings: vec![
            vec![Some(GroundTerm::Literal(Literal::from(1)))],
            vec![Some(GroundTerm::Literal(Literal::from(2)))],
        ],
    };
    let pattern = GraphPattern::Group {
        inner: Box::new(input),
        variables: vec![x.clone()],
        aggregates: vec![(
            variable("result"),
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Count,
                distinct: false,
                expr: Expression::Divide(Box::new(number(1)), Box::new(Expression::Variable(x))),
            },
        )],
    };
    let output = execute(pattern, None, true);
    assert_eq!(output.len(), 1);
    assert!(
        matches!(&output[0], Err(QueryEvaluationError::Unexpected(error)) if error.to_string().contains("integral quotient"))
    );
}

#[test]
fn real_extend_and_filter_keep_none_errors_and_stop_after_fatal() {
    let x = Expression::Variable(variable("x"));
    let extend = |expression| GraphPattern::Extend {
        inner: Box::new(rows(3)),
        variable: variable("y"),
        expression,
    };
    let output = execute(extend(x.clone()), Some(SolutionValue::ExplicitNone), true);
    assert_eq!(output.len(), 3);
    for row in output {
        assert_eq!(row.unwrap().values[1], Some(SolutionValue::ExplicitNone));
    }
    for expression in [x.clone(), divide(1, 0)] {
        let output = execute(extend(expression), None, true);
        assert_eq!(output.len(), 3);
        for row in output {
            assert_eq!(row.unwrap().values[1], None);
        }
    }
    for pattern in [
        extend(divide(1, 2)),
        GraphPattern::Filter {
            inner: Box::new(rows(3)),
            expression: divide(1, 2),
        },
    ] {
        let output = execute(pattern, None, true);
        assert_eq!(output.len(), 1); // Full collection proves the iterator stops.
        assert!(output[0].is_err());
    }
    let filtered = |expression| GraphPattern::Filter {
        inner: Box::new(rows(3)),
        expression,
    };
    assert!(execute(filtered(x), Some(SolutionValue::ExplicitNone), true).is_empty());
    assert!(execute(filtered(divide(1, 0)), None, true).is_empty());
    assert_eq!(execute(filtered(number(1)), None, true).len(), 3);
}

#[test]
fn nullable_lane_rejects_unsupported_without_changing_legacy() {
    let dataset = Dataset::new();
    let default = QueryEvaluator::new()
        .simple_evaluator(&dataset, QueryDatasetSpecification::new(), &None)
        .unwrap();
    assert!(!default.nullable_expressions);
    let evaluator = evaluator(&dataset);
    let pattern = GraphPattern::Filter {
        inner: Box::new(rows(1)),
        expression: Expression::Bound(variable("x")),
    };
    assert!(
        evaluator
            .graph_pattern_evaluator(&pattern, &mut vec![])
            .0
            .is_err()
    );
    assert!(
        default
            .graph_pattern_evaluator(&pattern, &mut vec![])
            .0
            .is_ok()
    );
    let pattern = GraphPattern::OrderBy {
        inner: Box::new(rows(1)),
        expression: vec![],
    };
    assert!(
        evaluator
            .graph_pattern_evaluator(&pattern, &mut vec![])
            .0
            .is_err()
    );
    assert!(
        default
            .graph_pattern_evaluator(&pattern, &mut vec![])
            .0
            .is_ok()
    );
}

#[test]
fn group_keys_retain_legacy_hash_bytes() {
    #[derive(Default)]
    struct Bytes(Vec<u8>);
    impl Hasher for Bytes {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }
    fn bytes(value: &impl Hash) -> Vec<u8> {
        let mut stream = Bytes::default();
        value.hash(&mut stream);
        stream.0
    }
    fn hash(value: &impl Hash) -> u64 {
        let mut hash = FxHasher::default();
        value.hash(&mut hash);
        hash.finish()
    }
    for old in [
        vec![None, Some("urn:x")],
        vec![Some("urn:x"), Some("urn:y")],
        vec![],
    ] {
        let new = old
            .iter()
            .map(|cell| GroupKeyCell(cell.map(SolutionValue::Rdf)))
            .collect::<Vec<_>>();
        assert_eq!(hash(&old), hash(&new));
        assert_eq!(bytes(&old), bytes(&new));
    }
    let calls = Rc::new(Cell::new(0));
    let measured = Rc::clone(&calls);
    let input = std::iter::from_fn(move || {
        measured.set(measured.get() + 1);
        Some(Err::<(), _>("fault"))
    });
    let mut halted = nullable::halt_after_error(input);
    assert_eq!(halted.next(), Some(Err("fault")));
    assert_eq!(halted.next(), None);
    assert_eq!(halted.next(), None);
    assert_eq!(calls.get(), 1);
}
