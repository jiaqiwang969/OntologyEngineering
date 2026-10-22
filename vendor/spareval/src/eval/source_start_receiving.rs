//! Author controls for the actual start/iterator protocol, not source-query
//! oracle cases. Injected child functions exercise the production adapters;
//! EXISTS uses the real ExpressionContext and a failing Dataset.
use super::*;
use crate::expression::{
    ExpressionEvaluationError, NumericExpressionHooks, build_numeric_expression_evaluator,
};
use crate::numeric::{QueryGroupSemantics, QueryNumericCapabilities};
use semantica_numeric::{DecimalContext, NumericLimits};
use std::cell::RefCell;

#[derive(Debug)]
struct StartMarker(Arc<()>);

impl fmt::Display for StartMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("start protocol marker")
    }
}
impl std::error::Error for StartMarker {}

fn original_error() -> (QueryEvaluationError, *const StartMarker, Arc<()>) {
    let marker = Arc::new(());
    let error = Box::new(StartMarker(Arc::clone(&marker)));
    let address = &*error as *const StartMarker;
    (QueryEvaluationError::Dataset(error), address, marker)
}

fn assert_original(error: QueryEvaluationError, address: *const StartMarker, marker: &Arc<()>) {
    let QueryEvaluationError::Dataset(error) = error else {
        panic!("changed the original error variant")
    };
    let actual = error.downcast_ref::<StartMarker>().unwrap();
    assert_eq!(actual as *const StartMarker, address);
    assert!(Arc::ptr_eq(&actual.0, marker));
}

#[test]
fn legacy_start_error_preserves_original_object_as_one_item() {
    let (error, address, marker) = original_error();
    let mut rows = legacy_start_rows::<()>(Err(error));
    assert_original(rows.next().unwrap().unwrap_err(), address, &marker);
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
}

#[test]
fn deferred_start_calls_once_on_first_poll_and_fuses_after_errors() {
    let (error, address, marker) = original_error();
    let pending = Rc::new(RefCell::new(Some(error)));
    let starts = Rc::new(Cell::new(0));
    let calls = Rc::clone(&starts);
    let plan: InternalTupleEvaluator<'_, ()> = Rc::new(move |_| {
        calls.set(calls.get() + 1);
        Err(pending.borrow_mut().take().expect("one start"))
    });
    let mut rows = deferred_start(plan, InternalTuple::with_capacity(0));
    assert_eq!(starts.get(), 0);
    assert_original(rows.next().unwrap().unwrap_err(), address, &marker);
    assert_eq!(starts.get(), 1);
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(starts.get(), 1);

    let (error, address, marker) = original_error();
    let pending = Rc::new(RefCell::new(Some(error)));
    let polls = Rc::new(Cell::new(0));
    let observed = Rc::clone(&polls);
    let plan: InternalTupleEvaluator<'_, ()> = Rc::new(move |_| {
        let error = pending.borrow_mut().take().expect("one start");
        let observed = Rc::clone(&observed);
        let mut items = vec![Ok(InternalTuple::with_capacity(0)), Err(error)].into_iter();
        Ok(Box::new(std::iter::from_fn(move || {
            observed.set(observed.get() + 1);
            assert!(observed.get() <= 2, "polled child after fatal row");
            items.next()
        })))
    });
    let mut rows = deferred_start(plan, InternalTuple::with_capacity(0));
    assert_eq!(polls.get(), 0);
    assert!(rows.next().unwrap().is_ok());
    assert_original(rows.next().unwrap().unwrap_err(), address, &marker);
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(polls.get(), 2);
}

#[test]
fn legacy_union_starts_each_plan_once_and_keeps_start_error_item() {
    let (error, address, marker) = original_error();
    let error = RefCell::new(Some(error));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let first_calls = Rc::clone(&calls);
    let first: InternalTupleEvaluator<'_, usize> = Rc::new(move |_| {
        first_calls.borrow_mut().push(0);
        Err(error.borrow_mut().take().expect("one first-plan start"))
    });
    let second_calls = Rc::clone(&calls);
    let second: InternalTupleEvaluator<'_, usize> = Rc::new(move |mut row| {
        second_calls.borrow_mut().push(1);
        row.set(0, 7);
        Ok(Box::new(once(Ok(row))))
    });
    let third_calls = Rc::clone(&calls);
    let third: InternalTupleEvaluator<'_, usize> = Rc::new(move |_| {
        third_calls.borrow_mut().push(2);
        Ok(Box::new(empty()))
    });
    let mut rows = UnionIterator {
        plans: vec![first, second, third],
        input: InternalTuple::with_capacity(1),
        current_iterator: Box::new(empty()),
        current_plan: 0,
    };
    assert!(calls.borrow().is_empty());
    assert_original(rows.next().unwrap().unwrap_err(), address, &marker);
    assert_eq!(&*calls.borrow(), &[0]);
    assert_eq!(rows.next().unwrap().unwrap().get(0), Some(&7));
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(&*calls.borrow(), &[0, 1, 2]);
}

#[cfg(feature = "sep-0006")]
#[test]
fn legacy_for_loop_left_join_retains_unmatched_rows_after_start_error() {
    let (error, address, marker) = original_error();
    let error = RefCell::new(Some(error));
    let calls = Rc::new(Cell::new(0));
    let observed = Rc::clone(&calls);
    let right: InternalTupleEvaluator<'_, usize> = Rc::new(move |row| {
        observed.set(observed.get() + 1);
        if row.get(0) == Some(&2) {
            Err(error.borrow_mut().take().expect("one second-row start"))
        } else {
            Ok(Box::new(empty()))
        }
    });
    let mut rows = ForLoopLeftJoinIterator {
        right_evaluator: right,
        left_iter: Box::new([1, 2].into_iter().map(|value| {
            let mut row = InternalTuple::with_capacity(1);
            row.set(0, value);
            Ok(row)
        })),
        current_right: Box::new(empty()),
        left_tuple_to_yield: None,
    };
    assert_eq!(rows.next().unwrap().unwrap().get(0), Some(&1));
    assert_original(rows.next().unwrap().unwrap_err(), address, &marker);
    assert_eq!(rows.next().unwrap().unwrap().get(0), Some(&2));
    assert!(rows.next().is_none());
    assert_eq!(calls.get(), 2);
}

struct ExistsDataset {
    scans: Rc<Cell<usize>>,
    internalizations: Rc<Cell<usize>>,
}
impl<'a> QueryableDataset<'a> for ExistsDataset {
    type InternalTerm = Term;
    type Error = StartMarker;

    fn internal_quads_for_pattern(
        &self,
        _subject: Option<&Term>,
        _predicate: Option<&Term>,
        _object: Option<&Term>,
        _graph_name: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.scans.set(self.scans.get() + 1);
        once(Err(StartMarker(Arc::new(()))))
    }

    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.internalizations.set(self.internalizations.get() + 1);
        Ok(term)
    }

    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        Ok(term)
    }
}

fn exists_expression() -> Expression {
    Expression::Exists(Box::new(GraphPattern::QuadPattern {
        subject: GroundTermPattern::Variable(Variable::new_unchecked("s")),
        predicate: NamedNodePattern::NamedNode(NamedNode::new_unchecked("urn:predicate")),
        object: GroundTermPattern::Variable(Variable::new_unchecked("o")),
        graph_name: None,
    }))
}

#[test]
fn actual_legacy_exists_retains_first_error_is_some_behavior() {
    let scans = Rc::new(Cell::new(0));
    let internalizations = Rc::new(Cell::new(0));
    let evaluator = crate::QueryEvaluator::new()
        .simple_evaluator(
            ExistsDataset {
                scans: Rc::clone(&scans),
                internalizations: Rc::clone(&internalizations),
            },
            QueryDatasetSpecification::new(),
            &None,
        )
        .unwrap();
    let mut variables = Vec::new();
    let mut stats = Vec::new();
    let mut context = ExpressionContext {
        evaluator: &evaluator,
        encoded_variables: &mut variables,
        stat_children: &mut stats,
    };
    let expression = build_expression_evaluator(&exists_expression(), &mut context).unwrap();
    assert!(internalizations.get() > 0, "actual graph was compiled");
    assert_eq!(scans.get(), 0);
    let value = expression(&InternalTuple::with_capacity(variables.len())).unwrap();
    assert_eq!(value.effective_boolean_value(), Some(true));
    assert_eq!(scans.get(), 1);
    assert_eq!(stats.len(), 1);
}

#[test]
fn actual_numeric_and_source_exists_reject_before_graph_compilation() {
    for literal_semantics in [
        QueryLiteralSemantics::RdfIdentityV1,
        QueryLiteralSemantics::SourceLiteralV1,
    ] {
        let scans = Rc::new(Cell::new(0));
        let internalizations = Rc::new(Cell::new(0));
        let evaluator = crate::QueryEvaluator::new()
            .simple_evaluator(
                ExistsDataset {
                    scans: Rc::clone(&scans),
                    internalizations: Rc::clone(&internalizations),
                },
                QueryDatasetSpecification::new(),
                &None,
            )
            .unwrap();
        let session = Rc::new(
            QueryNumericSession::new_with_literal_semantics(
                DecimalContext::default(),
                NumericLimits::default(),
                None,
                true,
                QueryNumericCapabilities::MixedBinary64V1,
                QueryGroupSemantics::SourceAggregateV1,
                literal_semantics,
            )
            .unwrap(),
        );
        let mut variables = Vec::new();
        let mut stats = Vec::new();
        let mut context = ExpressionContext {
            evaluator: &evaluator,
            encoded_variables: &mut variables,
            stat_children: &mut stats,
        };
        let result = build_numeric_expression_evaluator(
            &exists_expression(),
            &mut context,
            NumericExpressionHooks {
                session,
                prepared_literals: None,
                externalize: Rc::new(|_| panic!("must not externalize an EXISTS result")),
                failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
            },
        );
        assert!(matches!(
            result,
            Err(ExpressionEvaluationError::UnsupportedNullableExpression)
        ));
        assert!(variables.is_empty(), "graph variable encoding was entered");
        assert!(stats.is_empty(), "graph compilation was entered");
        assert_eq!(internalizations.get(), 0);
        assert_eq!(scans.get(), 0);
    }
}
