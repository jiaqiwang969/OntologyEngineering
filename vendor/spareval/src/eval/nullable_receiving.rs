//! Independent receiving controls for the real join and Dataset boundaries.
//! Expected outcomes were frozen from source; these are not public /5 parity.
use super::*;
use crate::tuple::ErrorValue;

type Row = InternalTuple<&'static str>;

fn keyed_row(cell: SolutionValue<&'static str>, payload: usize, value: &'static str) -> Row {
    let mut row = Row::with_capacity(3);
    row.set_cell(0, cell);
    row.set(payload, value);
    row
}

#[test]
fn real_hash_join_preserves_none_and_shared_error_identity_with_nonempty_keys() {
    let error = ErrorValue::new("same diagnostic");
    let independent = ErrorValue::new("same diagnostic");
    let mut built = InternalTupleSet::new(vec![0]);
    built.insert(keyed_row(SolutionValue::ExplicitNone, 1, "build-none"));
    built.insert(keyed_row(
        SolutionValue::ReturnedError(error.clone()),
        1,
        "build-shared-error",
    ));
    built.insert(keyed_row(
        SolutionValue::ReturnedError(independent),
        1,
        "build-independent-error",
    ));
    let probes: InternalTuplesIterator<'_, &'static str> = Box::new(
        vec![
            Ok(keyed_row(SolutionValue::ExplicitNone, 2, "probe-none")),
            Ok(keyed_row(
                SolutionValue::ReturnedError(error.clone()),
                2,
                "probe-shared-error",
            )),
            Ok(keyed_row(SolutionValue::Rdf("urn:key"), 2, "probe-missing")),
        ]
        .into_iter(),
    );
    let rows = HashJoinIterator {
        probe_iter: probes.peekable(),
        built,
        buffered_results: Vec::new(),
    }
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cell(0), Some(&SolutionValue::ExplicitNone));
    assert_eq!(rows[0].get(1), Some(&"build-none"));
    assert_eq!(rows[0].get(2), Some(&"probe-none"));
    assert_eq!(rows[1].cell(0), Some(&SolutionValue::ReturnedError(error)));
    assert_eq!(rows[1].get(1), Some(&"build-shared-error"));
    assert_eq!(rows[1].get(2), Some(&"probe-shared-error"));
}

#[test]
fn real_hash_left_join_preserves_unmatched_cells_and_matches_shared_error_identity() {
    let error = ErrorValue::new("same diagnostic");
    let independent = ErrorValue::new("same diagnostic");
    let mut right = InternalTupleSet::new(vec![0]);
    right.insert(keyed_row(SolutionValue::ExplicitNone, 1, "right-none"));
    right.insert(keyed_row(
        SolutionValue::ReturnedError(error.clone()),
        1,
        "right-shared-error",
    ));
    let left_rows = vec![
        keyed_row(SolutionValue::ExplicitNone, 2, "left-none"),
        keyed_row(
            SolutionValue::ReturnedError(error.clone()),
            2,
            "left-shared-error",
        ),
        keyed_row(
            SolutionValue::ReturnedError(independent),
            2,
            "left-independent-error",
        ),
        keyed_row(SolutionValue::Rdf("urn:key"), 2, "left-rdf"),
    ];
    let untouched = left_rows.clone();
    let expression_calls = Rc::new(Cell::new(0));
    let calls = Rc::clone(&expression_calls);
    let rows = HashLeftJoinIterator {
        left_iter: Box::new(left_rows.into_iter().map(Ok)),
        right,
        buffered_results: Vec::new(),
        expression: Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Some(true)
        }),
    }
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(expression_calls.get(), 2);
    assert_eq!(rows[0].cell(0), Some(&SolutionValue::ExplicitNone));
    assert_eq!(rows[0].get(1), Some(&"right-none"));
    assert_eq!(rows[0].get(2), Some(&"left-none"));
    assert_eq!(rows[1].cell(0), Some(&SolutionValue::ReturnedError(error)));
    assert_eq!(rows[1].get(1), Some(&"right-shared-error"));
    assert_eq!(rows[1].get(2), Some(&"left-shared-error"));
    assert_eq!(rows[2], untouched[2]);
    assert_eq!(rows[3], untouched[3]);
    assert!(rows[2].cell(1).is_none());
    assert!(rows[3].cell(1).is_none());
}

#[derive(Debug)]
struct ConversionFailure(&'static str);

impl fmt::Display for ConversionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ConversionFailure {}

struct FailingDataset {
    externalizations: Rc<Cell<usize>>,
    internalizations: Rc<Cell<usize>>,
}

impl<'a> QueryableDataset<'a> for FailingDataset {
    type InternalTerm = Term;
    type Error = ConversionFailure;

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

    fn externalize_term(&self, _term: Term) -> Result<Term, Self::Error> {
        self.externalizations.set(self.externalizations.get() + 1);
        Err(ConversionFailure("externalize_term receiving failure"))
    }

    fn internalize_expression_term(&self, _term: ExpressionTerm) -> Result<Term, Self::Error> {
        self.internalizations.set(self.internalizations.get() + 1);
        Err(ConversionFailure(
            "internalize_expression_term receiving failure",
        ))
    }
}

fn assert_conversion_failure(error: QueryEvaluationError, diagnostic: &'static str) {
    let QueryEvaluationError::Dataset(error) = error else {
        panic!("Expected the original Dataset error, received {error:?}");
    };
    let original = error
        .downcast_ref::<ConversionFailure>()
        .expect("Dataset error must retain its actual source error type");
    assert_eq!(original.0, diagnostic);
}

#[test]
fn presence_decoder_propagates_externalization_failure_without_erasing_non_rdf_cells() {
    let externalizations = Rc::new(Cell::new(0));
    let internalizations = Rc::new(Cell::new(0));
    let dataset = EvalDataset::new(
        FailingDataset {
            externalizations: Rc::clone(&externalizations),
            internalizations: Rc::clone(&internalizations),
        },
        QueryDatasetSpecification::new(),
        CancellationToken::default(),
    )
    .unwrap();
    let mut none = InternalTuple::with_capacity(1);
    none.set_cell(0, SolutionValue::ExplicitNone);
    let error = ErrorValue::new("returned error is not a dataset failure");
    let mut returned_error = InternalTuple::with_capacity(1);
    returned_error.set_cell(0, SolutionValue::ReturnedError(error.clone()));
    let mut rdf = InternalTuple::with_capacity(1);
    rdf.set(0, NamedNode::new("urn:term").unwrap().into());
    let mut rows = decode_presence_bindings(
        dataset,
        Box::new(
            vec![
                Ok(none),
                Ok(returned_error),
                Ok(rdf),
                Ok(InternalTuple::with_capacity(1)),
            ]
            .into_iter(),
        ),
        vec![Variable::new("x").unwrap()].into(),
    );
    assert_eq!(
        rows.next().unwrap().unwrap().values,
        vec![Some(SolutionValue::ExplicitNone)]
    );
    assert_eq!(externalizations.get(), 0);
    assert_eq!(
        rows.next().unwrap().unwrap().values,
        vec![Some(SolutionValue::ReturnedError(error))]
    );
    assert_eq!(externalizations.get(), 0);
    let Err(failure) = rows.next().unwrap() else {
        panic!("RDF externalization failure must not become an unbound output cell");
    };
    assert_conversion_failure(failure, "externalize_term receiving failure");
    assert_eq!(externalizations.get(), 1);
    assert_eq!(rows.next().unwrap().unwrap().values, vec![None]);
    assert!(rows.next().is_none());
    assert_eq!(externalizations.get(), 1);
    assert_eq!(internalizations.get(), 0);
}

#[test]
fn actual_typed_expression_context_retains_dataset_conversion_failures() {
    let externalizations = Rc::new(Cell::new(0));
    let internalizations = Rc::new(Cell::new(0));
    let evaluator = crate::QueryEvaluator::new()
        .simple_evaluator(
            FailingDataset {
                externalizations: Rc::clone(&externalizations),
                internalizations: Rc::clone(&internalizations),
            },
            QueryDatasetSpecification::new(),
            &None,
        )
        .unwrap();
    let x = Variable::new("x").unwrap();
    let mut variables = vec![x.clone()];
    let mut stats = Vec::new();
    let mut context = ExpressionContext {
        evaluator: &evaluator,
        encoded_variables: &mut variables,
        stat_children: &mut stats,
    };
    let expression =
        build_raw_expression_evaluator(&Expression::Variable(x), &mut context).unwrap();
    let mut row = InternalTuple::with_capacity(1);
    row.set(0, NamedNode::new("urn:term").unwrap().into());
    let Err(EvalFailure::Fatal(failure)) = expression(&row) else {
        panic!("Raw RDF variable conversion must preserve the Dataset failure");
    };
    assert_conversion_failure(failure, "externalize_term receiving failure");
    assert_eq!(externalizations.get(), 1);
    row.set_cell(0, SolutionValue::ExplicitNone);
    assert!(matches!(expression(&row), Ok(SolutionValue::ExplicitNone)));
    let error = ErrorValue::new("returned error remains a value");
    row.set_cell(0, SolutionValue::ReturnedError(error.clone()));
    assert!(expression(&row).unwrap() == SolutionValue::ReturnedError(error));
    assert_eq!(externalizations.get(), 1);

    let internalize = context.build_internalize_expression_term_result();
    let failure = internalize(ExpressionTerm::from(Term::from(Literal::from(1)))).unwrap_err();
    assert_conversion_failure(failure, "internalize_expression_term receiving failure");
    assert_eq!(internalizations.get(), 1);
}

fn receiving_variable(name: &str) -> Variable {
    Variable::new(name).unwrap()
}

fn three_receiving_rows() -> GraphPattern {
    GraphPattern::Values {
        variables: Vec::new(),
        bindings: vec![Vec::new(); 3],
    }
}

fn receiving_sum(left: Expression, right: i64) -> Expression {
    Expression::Add(
        Box::new(left),
        Box::new(Expression::Literal(Literal::from(right))),
    )
}

fn receiving_group(name: AggregateFunction, expr: Expression, distinct: bool) -> GraphPattern {
    GraphPattern::Group {
        inner: Box::new(three_receiving_rows()),
        variables: Vec::new(),
        aggregates: vec![(
            receiving_variable("result"),
            AggregateExpression::FunctionCall {
                name,
                distinct,
                expr,
            },
        )],
    }
}

fn assert_real_typed_dataset_failure_stops(
    pattern: GraphPattern,
    input: Option<SolutionValue<Term>>,
    diagnostic: &'static str,
    expected_externalizations: usize,
    expected_internalizations: usize,
) {
    let externalizations = Rc::new(Cell::new(0));
    let internalizations = Rc::new(Cell::new(0));
    let mut evaluator = crate::QueryEvaluator::new()
        .simple_evaluator(
            FailingDataset {
                externalizations: Rc::clone(&externalizations),
                internalizations: Rc::clone(&internalizations),
            },
            QueryDatasetSpecification::new(),
            &None,
        )
        .unwrap();
    evaluator.nullable_expressions = true;
    let mut variables = vec![receiving_variable("x")];
    let (plan, _) = evaluator.graph_pattern_evaluator(&pattern, &mut variables);
    let mut row = InternalTuple::with_capacity(variables.len());
    if let Some(cell) = input {
        row.set_cell(0, cell);
    }
    let mut output = plan.unwrap()(row).expect("legacy query starts successfully");
    let Err(failure) = output
        .next()
        .expect("The real operator must emit its failure")
    else {
        panic!("Dataset failure must not be swallowed or converted to a successful row");
    };
    assert_conversion_failure(failure, diagnostic);
    assert_eq!(externalizations.get(), expected_externalizations);
    assert_eq!(internalizations.get(), expected_internalizations);
    // The original generator terminates after an uncaught exception. A second
    // next must not evaluate the remaining two rows or emit a partial group.
    assert!(output.next().is_none());
    assert!(output.next().is_none());
    assert_eq!(externalizations.get(), expected_externalizations);
    assert_eq!(internalizations.get(), expected_internalizations);
}

#[test]
fn real_typed_filter_variable_propagates_dataset_failure_and_stops() {
    // This explicitly tests the active Rust safety contract. Original Python's
    // bare-variable _ebv branch has a broad except and is not parity evidence.
    assert_real_typed_dataset_failure_stops(
        GraphPattern::Filter {
            inner: Box::new(three_receiving_rows()),
            expression: Expression::Variable(receiving_variable("x")),
        },
        Some(SolutionValue::Rdf(Literal::from(true).into())),
        "externalize_term receiving failure",
        1,
        0,
    );
}

#[test]
fn real_typed_count_arithmetic_propagates_dataset_failure_and_stops() {
    for distinct in [false, true] {
        assert_real_typed_dataset_failure_stops(
            receiving_group(
                AggregateFunction::Count,
                receiving_sum(Expression::Variable(receiving_variable("x")), 1),
                distinct,
            ),
            Some(SolutionValue::Rdf(Literal::from(7).into())),
            "externalize_term receiving failure",
            1,
            0,
        );
    }
}

#[test]
fn real_typed_sample_arithmetic_propagates_dataset_failure_and_stops() {
    assert_real_typed_dataset_failure_stops(
        receiving_group(
            AggregateFunction::Sample,
            receiving_sum(Expression::Variable(receiving_variable("x")), 1),
            false,
        ),
        Some(SolutionValue::Rdf(Literal::from(7).into())),
        "externalize_term receiving failure",
        1,
        0,
    );
}

#[test]
fn real_typed_extend_arithmetic_propagates_internalization_failure_and_stops() {
    assert_real_typed_dataset_failure_stops(
        GraphPattern::Extend {
            inner: Box::new(three_receiving_rows()),
            variable: receiving_variable("result"),
            expression: receiving_sum(Expression::Literal(Literal::from(1)), 1),
        },
        None,
        "internalize_expression_term receiving failure",
        0,
        1,
    );
}

#[test]
fn real_typed_sample_variable_preserves_internal_cells_until_result_decoding() {
    let error = ErrorValue::new("sampled error retains its identity");
    for selected in [
        SolutionValue::Rdf(Term::from(Literal::from(7))),
        SolutionValue::ExplicitNone,
        SolutionValue::ReturnedError(error),
    ] {
        let externalizations = Rc::new(Cell::new(0));
        let internalizations = Rc::new(Cell::new(0));
        let mut evaluator = crate::QueryEvaluator::new()
            .simple_evaluator(
                FailingDataset {
                    externalizations: Rc::clone(&externalizations),
                    internalizations: Rc::clone(&internalizations),
                },
                QueryDatasetSpecification::new(),
                &None,
            )
            .unwrap();
        evaluator.nullable_expressions = true;
        let pattern = receiving_group(
            AggregateFunction::Sample,
            Expression::Variable(receiving_variable("x")),
            false,
        );
        let mut variables = vec![receiving_variable("x")];
        let (plan, _) = evaluator.graph_pattern_evaluator(&pattern, &mut variables);
        let result_slot = variables
            .iter()
            .position(|variable| variable.as_str() == "result")
            .unwrap();
        let mut row = InternalTuple::with_capacity(variables.len());
        row.set_cell(0, selected.clone());
        let mut output = plan.unwrap()(row).expect("legacy query starts successfully");
        let result = output.next().unwrap().unwrap();
        assert_eq!(result.cell(result_slot), Some(&selected));
        assert!(output.next().is_none());
        assert_eq!(externalizations.get(), 0);
        assert_eq!(internalizations.get(), 0);

        // RDF is externalized only by the actual result decoder; None/error
        // stays in the presence row without asking the Dataset to coerce it.
        let is_rdf = matches!(&selected, SolutionValue::Rdf(_));
        let mut decoded = decode_presence_bindings(
            evaluator.dataset,
            Box::new(once(Ok(result))),
            variables.into(),
        );
        if is_rdf {
            let Err(failure) = decoded.next().unwrap() else {
                panic!("RDF result must reach the failing externalization boundary");
            };
            assert_conversion_failure(failure, "externalize_term receiving failure");
            assert_eq!(externalizations.get(), 1);
        } else {
            assert_eq!(
                decoded.next().unwrap().unwrap().values[result_slot],
                Some(selected),
            );
            assert_eq!(externalizations.get(), 0);
        }
        assert!(decoded.next().is_none());
        assert_eq!(internalizations.get(), 0);
    }
}
