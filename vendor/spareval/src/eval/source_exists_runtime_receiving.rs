use super::*;
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, QueryContextSemantics, QueryGroupSemantics,
    QueryNumericCapabilities, SourceFact, SourceHashPolicy, SourceHashRuntime,
    SourceLiteralCacheKind, raw_numeric_fault,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use semantica_numeric::{DecimalContext, DecimalSignal, NumericFault, NumericLimits, NumericValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::convert::Infallible;

const FIXTURE: &str = include_str!("fixtures/source_exists_runtime_cases.json");

fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "f79612f5aa60ea8a5047b09e649bc814b7cb68ffce9597bcd4053ca7d0922d14"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "719f52bddffacd788335d10faedbc22f3787f26a97cba585a3ae19f3cdcf237c"
    );
    assert_eq!(value["cases"].as_array().unwrap().len(), 50);
    value
}

pub(super) fn case(id: &str) -> Value {
    fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap()
        .clone()
}

pub(super) fn session(case: &Value, limits: NumericLimits) -> Rc<QueryNumericSession> {
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
            limits,
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

#[derive(Clone, Default)]
pub(super) struct Backend {
    pub(super) scans: Rc<Cell<usize>>,
}

impl<'a> QueryableDataset<'a> for Backend {
    type InternalTerm = Term;
    type Error = Infallible;
    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.scans.set(self.scans.get() + 1);
        empty()
    }
    fn internalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        Ok(value)
    }
    fn externalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        Ok(value)
    }
}

fn cell_json(cell: &SolutionValue<RawExpressionValue>) -> Value {
    match cell {
        SolutionValue::ExplicitNone => Value::Null,
        SolutionValue::ReturnedError(error) => panic!("unexpected output error cell: {error:?}"),
        SolutionValue::Rdf(raw) => {
            let Term::Literal(literal) = raw.rdf().unwrap() else {
                panic!("nonliteral output");
            };
            let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                panic!("missing metadata");
            };
            let SourceFact::Known(language) = &metadata.language_original else {
                panic!("unknown language");
            };
            let SourceFact::Known(ill_typed) = metadata.ill_typed else {
                panic!("unknown ill_typed");
            };
            let cache_type = match raw.numeric() {
                NumericCacheState::Known(cache) => match cache.value() {
                    NumericValue::Integer(_) => "builtins.int",
                    NumericValue::Decimal(_) => "decimal.Decimal",
                    NumericValue::Binary64(_) => "builtins.float",
                },
                NumericCacheState::NotNumeric
                    if matches!(
                        metadata.cached_value_kind,
                        SourceFact::Known(SourceLiteralCacheKind::Boolean(_))
                    ) =>
                {
                    "builtins.bool"
                }
                _ => panic!("unreceived output cache"),
            };
            json!({"lexical":literal.value(),"datatype":literal.datatype().as_str(),"language":language,"ill_typed":ill_typed,"cache_type":cache_type})
        }
    }
}

fn row_json(row: &SourcePresenceSolution) -> Value {
    let cells: BTreeMap<_, _> = row
        .variables
        .iter()
        .zip(&row.values)
        .filter_map(|(variable, cell)| {
            cell.as_ref()
                .map(|cell| (variable.as_str(), cell_json(cell)))
        })
        .collect();
    json!(cells)
}

fn expected_rows(case: &Value) -> Value {
    json!(
        case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(|cell| (cell[0].as_str().unwrap(), cell[1].clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .collect::<Vec<_>>()
    )
}

pub(super) fn receive_success(id: &str) {
    let case = case(id);
    assert_eq!(case["original_status"], "observed_complete");
    assert_eq!(case["input"]["initial_bindings"], json!([]));
    let session = session(&case, NumericLimits::default());
    let backend = Backend::default();
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let rows = prepared
        .execute_source_presence()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    if let Some(row) = rows.first() {
        assert_eq!(
            json!(
                row.variables
                    .iter()
                    .map(Variable::as_str)
                    .collect::<Vec<_>>()
            ),
            case["columns"],
            "{id} columns"
        );
    }
    assert_eq!(
        json!(rows.iter().map(row_json).collect::<Vec<_>>()),
        expected_rows(&case),
        "{id}"
    );
    assert_eq!(
        json!(
            session
                .flags()
                .iter()
                .map(|signal| format!("{signal:?}"))
                .collect::<Vec<_>>()
        ),
        case["flags"]
    );
    assert_eq!(backend.scans.get(), 0);
}

fn receive_error(id: &str) {
    let case = case(id);
    assert_eq!(case["original_status"], "original_error");
    assert_eq!(case["error_stage"], "first_poll");
    let session = session(&case, NumericLimits::default());
    let backend = Backend::default();
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = match rows.next().unwrap() {
        Err(error) => error,
        Ok(_) => panic!("expected first-poll error: {id}"),
    };
    match case["error"]["class"].as_str().unwrap() {
        "AttributeError" => {
            assert!(matches!(
                raw_numeric_fault(&error),
                Some(RawNumericFault::SourceContext(
                    crate::numeric::SourceContextFault::EmptyValuesAttribute
                ))
            ));
            assert_eq!(
                case["error"]["args"],
                json!(["'list' object has no attribute 'name'"])
            );
        }
        "FloatOperation" => super::source_decimal_trap_receiving::assert_original_decimal_class(
            &error,
            DecimalSignal::FloatOperation,
        ),
        "NotBoundError" => {
            let Some(nullable::NullableAggregateError::UncaughtSparql(value)) =
                nullable::nullable_aggregate_error(&error)
            else {
                panic!("source NotBoundError not preserved: {error:?}");
            };
            let source = value.source_exception().unwrap();
            assert_eq!(source.class, SourceExceptionClass::NotBound);
            let SourceExceptionArgument::Text(text) = &source.argument else {
                panic!("missing argument");
            };
            assert_eq!(json!([text]), case["error"]["args"]);
        }
        other => panic!("unreceived error class {other}"),
    }
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(
        json!(
            session
                .flags()
                .iter()
                .map(|signal| format!("{signal:?}"))
                .collect::<Vec<_>>()
        ),
        case["flags"]
    );
    assert_eq!(backend.scans.get(), 0);
}

#[test]
fn source_exists_runtime_bx01_polarities() {
    receive_success("BX01");
}

#[test]
fn source_exists_runtime_bx02_removed_false_filter() {
    receive_success("BX02");
}

#[test]
fn source_exists_runtime_bx07_captured_sparql() {
    receive_success("BX07");
}

#[test]
fn source_exists_runtime_bx08_not_exists_captured_sparql() {
    receive_success("BX08");
}

#[test]
fn source_exists_runtime_bx10_captured_sum_error() {
    receive_success("BX10");
}

#[test]
fn source_exists_runtime_bx13_nested_polarities() {
    receive_success("BX13");
}

#[test]
fn source_exists_runtime_bx14_aggregate_boolean_consumers() {
    receive_success("BX14");
}

#[test]
fn source_exists_runtime_bx15_actual_filter_entry() {
    receive_success("BX15");
}

#[test]
fn source_exists_runtime_ex23_deferred_limit_zero() {
    receive_success("EX23");
}

#[test]
fn source_exists_runtime_ex25_distinct_limit_zero() {
    receive_success("EX25");
}

#[test]
fn source_exists_runtime_ex33_parent_analysis_and_distinct() {
    receive_success("EX33");
}

#[test]
fn source_exists_runtime_ex34_hidden_variable_registry() {
    receive_success("EX34");
}

#[test]
fn source_exists_runtime_bx03_empty_values_attribute() {
    receive_error("BX03");
}

#[test]
fn source_exists_runtime_bx04_empty_tuple_values_attribute() {
    receive_error("BX04");
}

#[test]
fn source_exists_runtime_bx11_materialized_right_float_operation() {
    receive_error("BX11");
}

#[test]
fn source_exists_runtime_bx12_eager_order_float_operation() {
    receive_error("BX12");
}

#[test]
fn source_exists_runtime_ex16_parent_aggregate_not_bound() {
    receive_error("EX16");
}

#[test]
fn source_exists_runtime_ex17_parent_aggregate_not_bound() {
    receive_error("EX17");
}

#[test]
fn source_exists_runtime_actual_compiler_polls_and_drops_first_child() {
    use super::source_context_join::{SourceChildSide, SourceContextEvent};
    let case = case("BX01");
    let session = session(&case, NumericLimits::default());
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&events);
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        Backend::default(),
        session,
        Vec::new(),
    )
    .unwrap()
    .with_source_context_observer(Rc::new(move |event| seen.borrow_mut().push(event)));
    let rows = prepared
        .execute_source_presence()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    let events = events.borrow();
    let starts: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SourceContextEvent::ChildStart {
                node,
                side: SourceChildSide::Child,
                ..
            } => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 2);
    for node in starts {
        assert_eq!(events.iter().filter(|event| matches!(event, SourceContextEvent::ChildPoll {node:actual, side:SourceChildSide::Child, ordinal:1, ..} if *actual==node)).count(), 1);
        assert_eq!(events.iter().filter(|event| matches!(event, SourceContextEvent::ChildDropped {node:actual, side:SourceChildSide::Child, polls:1, ..} if *actual==node)).count(), 1);
    }
}

#[test]
fn source_exists_runtime_actual_start_and_poll_cancellation_fuse() {
    use super::source_context_join::{SourceChildSide, SourceContextEvent};
    for at_poll in [false, true] {
        let case = case("BX01");
        let prepared = numeric_query::prepare_source_exists_numeric_query(
            case["input"]["query"].as_str().unwrap(),
            Backend::default(),
            session(&case, NumericLimits::default()),
            Vec::new(),
        )
        .unwrap();
        let token = prepared.evaluator().dataset.cancellation_token.clone();
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&events);
        let prepared = prepared.with_source_context_observer(Rc::new(move |event| {
            if (at_poll
                && matches!(
                    &event,
                    SourceContextEvent::ChildPoll {
                        side: SourceChildSide::Child,
                        ..
                    }
                ))
                || (!at_poll
                    && matches!(
                        &event,
                        SourceContextEvent::ChildStart {
                            side: SourceChildSide::Child,
                            ..
                        }
                    ))
            {
                token.cancel();
            }
            seen.borrow_mut().push(event);
        }));
        let mut rows = prepared.execute_source_presence().unwrap();
        assert!(matches!(
            rows.next(),
            Some(Err(QueryEvaluationError::Cancelled))
        ));
        let frozen = events.borrow().len();
        assert!(rows.next().is_none());
        assert!(rows.next().is_none());
        assert_eq!(events.borrow().len(), frozen);
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| matches!(
                    event,
                    SourceContextEvent::ChildStart {
                        side: SourceChildSide::Child,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| matches!(
                    event,
                    SourceContextEvent::ChildPoll {
                        side: SourceChildSide::Child,
                        ..
                    }
                ))
                .count(),
            usize::from(at_poll)
        );
    }
}

pub(super) fn first_source_expression(pattern: &GraphPattern) -> &Expression {
    match pattern {
        GraphPattern::SourceContext { inner, .. } | GraphPattern::Project { inner, .. } => {
            first_source_expression(inner)
        }
        GraphPattern::Extend {
            expression: expression @ Expression::SourceExists(_),
            ..
        } => expression,
        other => panic!("missing actual source expression: {other:?}"),
    }
}

#[test]
fn source_exists_runtime_same_preparation_required_and_missing_owner_is_fatal() {
    use crate::expression::{
        ExpressionEvaluationError, NumericExpressionHooks, build_numeric_expression_evaluator,
    };
    let case = case("BX01");
    let session = session(&case, NumericLimits::default());
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        Backend::default(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let evaluator = prepared.evaluator();
    let codec = evaluator.dataset.numeric.as_ref().unwrap();
    let registry = codec.prepared_literals.as_ref().unwrap();
    let actual_expression = first_source_expression(prepared.plan());
    let runtime = evaluator.source_context.as_ref().unwrap();
    for foreign in [true, false] {
        let mut variables = runtime.variables.to_vec();
        let mut stats = Vec::new();
        let mut context = ExpressionContext {
            evaluator,
            encoded_variables: &mut variables,
            stat_children: &mut stats,
        };
        let registry = if foreign {
            crate::numeric::PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(
                &session,
            ))
        } else {
            Rc::clone(registry)
        };
        let result = build_numeric_expression_evaluator(
            actual_expression,
            &mut context,
            NumericExpressionHooks {
                session: Rc::clone(&session),
                prepared_literals: Some(registry),
                externalize: Rc::clone(&codec.externalize),
                failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
            },
        );
        assert_eq!(variables.as_slice(), runtime.variables.as_ref());
        if foreign {
            assert!(matches!(
                result,
                Err(ExpressionEvaluationError::UnsupportedNullableExpression)
            ));
            assert!(stats.is_empty());
        } else {
            let expression = result.unwrap();
            let plain = InternalTuple::with_capacity(variables.len());
            let Err(EvalFailure::Fatal(error)) = expression(&plain) else {
                panic!("missing owner acquired execution context");
            };
            assert!(matches!(
                raw_numeric_fault(&error),
                Some(RawNumericFault::InvalidSourceOrigin)
            ));
        }
    }
}

#[test]
fn source_exists_runtime_source_sparql_arc_is_preserved_without_global_catch() {
    let value = ErrorValue::with_source(
        SparqlErrorKind::NotBound,
        "source missing",
        SourceException {
            class: SourceExceptionClass::NotBound,
            argument: SourceExceptionArgument::Text("Variable missing is not bound".into()),
        },
    );
    let error = nullable::uncaught(EvalFailure::Sparql(value.clone()));
    let EvalFailure::Sparql(actual) = nullable::source_exists_failure(error) else {
        panic!("typed source exception lost");
    };
    assert_eq!(actual, value);
    let plain = ErrorValue::with_kind(SparqlErrorKind::NotBound, "no source constructor identity");
    let error = nullable::uncaught(EvalFailure::Sparql(plain.clone()));
    let EvalFailure::Fatal(actual) = nullable::source_exists_failure(error) else {
        panic!("unsourced exception falsely received");
    };
    let Some(nullable::NullableAggregateError::UncaughtSparql(actual)) =
        nullable::nullable_aggregate_error(&actual)
    else {
        panic!("fatal wrapper lost");
    };
    assert_eq!(actual, &plain);
    assert!(matches!(
        nullable::source_exists_failure(QueryEvaluationError::Cancelled),
        EvalFailure::Fatal(QueryEvaluationError::Cancelled)
    ));
}

#[test]
fn source_exists_runtime_boolean_output_budget_fuses_actual_query() {
    let case = case("BX01");
    let limits = NumericLimits {
        max_input_characters: 6,
        max_output_characters: 4,
        ..NumericLimits::default()
    };
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        Backend::default(),
        session(&case, limits),
        Vec::new(),
    )
    .unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = match rows.next().unwrap() {
        Err(error) => error,
        Ok(_) => panic!("false result exceeded output budget"),
    };
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::Core(NumericFault::ResourceLimit {
            operation: "output_characters"
        }))
    ));
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
}
