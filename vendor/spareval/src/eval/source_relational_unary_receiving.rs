use super::*;
use crate::expression::{
    ExpressionEvaluationError, NumericExpressionHooks, build_numeric_expression_evaluator,
};
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    QueryContextSemantics, QueryGroupSemantics, QueryNumericCapabilities, SourceFact,
    SourceHashPolicy, SourceHashRuntime, SourceLiteralCacheKind, raw_numeric_fault,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use oxrdf::vocab::xsd;
use semantica_numeric::{DecimalContext, NumericFault, NumericLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spargebra::{NumericTokenKind, NumericTokenSign};
use std::convert::Infallible;

const FIXTURE: &str = include_str!("fixtures/source_relational_unary_protocols.json");

fn protocol(id: &str) -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "1561a5d9808156ddcf3c25762fe614750fb542f15f6fd22409f7c9784964c9af"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "36401bf9c90b69c7d441a299f39d3d21ebf0346dd3f1ae06fa9824f904082926"
    );
    assert_eq!(value["source_protocols"].as_array().unwrap().len(), 12);
    value["source_protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap()
        .clone()
}

fn session(limits: NumericLimits) -> Rc<QueryNumericSession> {
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            DecimalContext::default(),
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

fn integer(session: &QueryNumericSession, text: &str) -> RawExpressionValue {
    session
        .construct_source_numeric_token(NumericTokenKind::Integer, NumericTokenSign::Unsigned, text)
        .unwrap()
}

fn source_error() -> ErrorValue {
    ErrorValue::with_source(
        SparqlErrorKind::NotBound,
        "source missing",
        SourceException {
            class: SourceExceptionClass::NotBound,
            argument: SourceExceptionArgument::Text("Variable missing is not bound".into()),
        },
    )
}

fn assert_boolean(raw: &RawExpressionValue, expected: &Value) {
    let Term::Literal(literal) = raw.rdf().unwrap() else {
        panic!("not a boolean literal")
    };
    assert_eq!(literal.value(), expected["lexical"].as_str().unwrap());
    assert_eq!(
        literal.datatype().as_str(),
        expected["datatype"].as_str().unwrap()
    );
    assert!(matches!(raw.numeric(), NumericCacheState::NotNumeric));
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!("missing facts")
    };
    assert_eq!(metadata.ill_typed, SourceFact::Known(None));
    assert_eq!(metadata.language_original, SourceFact::Known(None));
    assert_eq!(
        metadata.cached_value_kind,
        SourceFact::Known(SourceLiteralCacheKind::Boolean(
            expected["cache"].as_bool().unwrap()
        ))
    );
    assert_eq!(expected["cache_type"], "builtins.bool");
    assert!(literal.language().is_none());
}

fn assert_source_error(error: &ErrorValue, expected: &Value, class: SourceExceptionClass) {
    let source = error.source_exception().unwrap();
    assert_eq!(source.class, class);
    let SourceExceptionArgument::Text(argument) = &source.argument else {
        panic!("missing source argument")
    };
    assert_eq!(json!([argument]), expected["args"]);
}

#[derive(Clone)]
struct Backend;
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
        empty()
    }
    fn internalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        Ok(value)
    }
    fn externalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        Ok(value)
    }
}

fn first_expression(pattern: &GraphPattern) -> &Expression {
    match pattern {
        GraphPattern::SourceContext { inner, .. } | GraphPattern::Project { inner, .. } => {
            first_expression(inner)
        }
        GraphPattern::Extend { expression, .. } => expression,
        other => panic!("missing original parsed expression: {other:?}"),
    }
}

fn compiled_value(
    query: &str,
    session: Rc<QueryNumericSession>,
    local: Option<SolutionValue<RawExpressionValue>>,
    initial: Vec<NumericInitialBinding>,
) -> Result<SolutionValue<RawExpressionValue>, EvalFailure<QueryEvaluationError>> {
    // This is expression-component receiving with the actual parser/codec and
    // prepared registry. Whole-query receiving below executes the frozen BXs.
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        query,
        Backend,
        Rc::clone(&session),
        initial,
    )
    .unwrap();
    let evaluator = prepared.evaluator();
    let codec = evaluator.dataset.numeric.as_ref().unwrap();
    let runtime = evaluator.source_context.as_ref().unwrap();
    let mut variables = runtime.variables.to_vec();
    let mut stats = Vec::new();
    let mut context = ExpressionContext {
        evaluator,
        encoded_variables: &mut variables,
        stat_children: &mut stats,
    };
    let expression = build_numeric_expression_evaluator(
        first_expression(prepared.plan()),
        &mut context,
        NumericExpressionHooks {
            session,
            prepared_literals: codec.prepared_literals.clone(),
            externalize: Rc::clone(&codec.externalize),
            failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
        },
    )
    .unwrap();
    let mut row = InternalTuple::with_capacity(variables.len());
    if let Some(local) = local {
        let slot = variables
            .iter()
            .position(|variable| variable.as_str() == "x")
            .unwrap();
        row.set_cell(
            slot,
            local
                .try_map_rdf(|raw| (codec.internalize)(raw.clone()))
                .unwrap(),
        );
    }
    expression(&row)
}

fn returned_error(
    value: Result<SolutionValue<RawExpressionValue>, EvalFailure<QueryEvaluationError>>,
) -> ErrorValue {
    let Ok(SolutionValue::ReturnedError(error)) = value else {
        panic!("missing caught source error: {value:?}")
    };
    error
}

fn receive_relation(id: &str) {
    let source = protocol(id);
    let session = session(NumericLimits::default());
    let left = integer(
        &session,
        source["input"]["left_local_cell"]["lexical"]
            .as_str()
            .unwrap(),
    );
    let right = integer(&session, "0");
    let left_copy = left.clone();
    let right_copy = right.clone();
    let first = session
        .source_relational_greater(
            &SolutionValue::Rdf(left.clone()),
            &SolutionValue::Rdf(right.clone()),
        )
        .unwrap();
    let second = session
        .source_relational_greater(
            &SolutionValue::Rdf(left.clone()),
            &SolutionValue::Rdf(right.clone()),
        )
        .unwrap();
    assert_boolean(&first, &source["expected"]["value"]);
    assert_boolean(&second, &source["expected"]["value"]);
    assert!(!first.same_source_object(&second));
    assert!(left.same_source_object(&left_copy) && right.same_source_object(&right_copy));
    assert!(session.flags().iter().next().is_none());
}

#[test]
fn source_relational_unary_rg01_cached_zero() {
    receive_relation("RG01");
}
#[test]
fn source_relational_unary_rg02_cached_one() {
    receive_relation("RG02");
}

#[test]
fn source_relational_unary_rg03_none_masks_initial_and_rhs_stays_unread() {
    let _ = protocol("RG03");
    for none in [false, true] {
        let session = session(NumericLimits::default());
        let raw = integer(&session, "1");
        let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
            panic!()
        };
        let NumericCacheState::Known(cache) = raw.numeric() else {
            panic!()
        };
        let initial = if none {
            vec![NumericInitialBinding {
                variable: Variable::new("x").unwrap(),
                value: NumericInitialValue::SourceKnownCached {
                    rdf: raw.rdf().unwrap().clone(),
                    value: cache.value().clone(),
                    metadata: metadata.clone(),
                },
            }]
        } else {
            Vec::new()
        };
        let error = returned_error(compiled_value(
            "SELECT (?x > 0 AS ?b) WHERE {}",
            Rc::clone(&session),
            none.then_some(SolutionValue::ExplicitNone),
            initial,
        ));
        assert_eq!(
            error.source_exception().unwrap(),
            &SourceException {
                class: SourceExceptionClass::NotBound,
                argument: SourceExceptionArgument::None
            }
        );
        assert!(session.flags().iter().next().is_none());
        // Independent active RHS: division is reached only after the left
        // variable succeeds. Decimal flags prove it was not evaluated here.
        let error = returned_error(compiled_value(
            "SELECT (?x > (1/0) AS ?b) WHERE {}",
            Rc::clone(&session),
            None,
            Vec::new(),
        ));
        assert_eq!(
            error.source_exception().unwrap().class,
            SourceExceptionClass::NotBound
        );
        assert!(session.flags().iter().next().is_none());
    }
}

#[test]
fn source_relational_unary_rg04_variable_error_keeps_arc() {
    let source = protocol("RG04");
    let error = source_error();
    let output = returned_error(compiled_value(
        "SELECT (?x > 0 AS ?b) WHERE {}",
        session(NumericLimits::default()),
        Some(SolutionValue::ReturnedError(error.clone())),
        Vec::new(),
    ));
    assert_eq!(output, error);
    assert_source_error(
        &output,
        &source["expected"]["value"],
        SourceExceptionClass::NotBound,
    );
}

#[test]
fn source_relational_unary_rg05_expr_error_gets_new_guard_error() {
    let source = protocol("RG05");
    let session = session(NumericLimits::default());
    let prior = source_error();
    let error = session
        .source_relational_greater(
            &SolutionValue::ReturnedError(prior.clone()),
            &SolutionValue::Rdf(integer(&session, "0")),
        )
        .unwrap_err()
        .into_error_value()
        .unwrap();
    assert_ne!(error, prior);
    assert_source_error(
        &error,
        &source["expected"]["value"],
        SourceExceptionClass::Sparql,
    );
    assert!(session.flags().iter().next().is_none());
}

#[test]
fn source_relational_unary_rg06_uri_is_not_order_rank() {
    let source = protocol("RG06");
    let session = session(NumericLimits::default());
    let uri = RawExpressionValue::from_import(NamedNode::new("urn:test:value").unwrap().into());
    let error = session
        .source_relational_greater(
            &SolutionValue::Rdf(uri),
            &SolutionValue::Rdf(integer(&session, "0")),
        )
        .unwrap_err()
        .into_error_value()
        .unwrap();
    assert_source_error(
        &error,
        &source["expected"]["value"],
        SourceExceptionClass::Sparql,
    );
}

#[test]
fn source_relational_unary_un01_boolean_cache_and_new_result() {
    let source = protocol("UN01");
    let session = session(NumericLimits::default());
    for run in source["input"]["runs"].as_array().unwrap() {
        let value = session
            .construct_source_boolean_result(run["value"]["cache"].as_bool().unwrap())
            .unwrap();
        let ebv = session
            .source_effective_boolean_value(&SolutionValue::Rdf(value.clone()))
            .unwrap();
        assert_eq!(!ebv, run["expected_bool"].as_bool().unwrap());
        let result = session.construct_source_boolean_result(!ebv).unwrap();
        assert!(!result.same_source_object(&value));
        let mut expected = run["value"].clone();
        expected["cache"] = json!(!ebv);
        expected["lexical"] = json!(if ebv { "false" } else { "true" });
        assert_boolean(&result, &expected);
    }
}

#[test]
fn source_relational_unary_un02_integer_ebv() {
    let source = protocol("UN02");
    let session = session(NumericLimits::default());
    for run in source["input"]["runs"].as_array().unwrap() {
        let raw = integer(&session, run["value"]["lexical"].as_str().unwrap());
        assert_eq!(
            !session
                .source_effective_boolean_value(&SolutionValue::Rdf(raw))
                .unwrap(),
            run["expected_bool"].as_bool().unwrap()
        );
    }
}

#[test]
fn source_relational_unary_un03_ebv_error_is_distinct() {
    let source = protocol("UN03");
    let session = session(NumericLimits::default());
    let prior = source_error();
    let error = session
        .source_effective_boolean_value(&SolutionValue::ReturnedError(prior.clone()))
        .unwrap_err()
        .into_error_value()
        .unwrap();
    assert_ne!(error, prior);
    assert_source_error(
        &error,
        &source["expected"]["value"],
        SourceExceptionClass::SparqlType,
    );
}

#[test]
fn source_relational_unary_un04_variable_error_bypasses_ebv() {
    let source = protocol("UN04");
    let error = source_error();
    let output = returned_error(compiled_value(
        "SELECT (! ?x AS ?b) WHERE {}",
        session(NumericLimits::default()),
        Some(SolutionValue::ReturnedError(error.clone())),
        Vec::new(),
    ));
    assert_eq!(output, error);
    assert_source_error(
        &output,
        &source["expected"]["value"],
        SourceExceptionClass::NotBound,
    );
}

#[test]
fn source_relational_unary_eb01_empty_error_literal_uses_lexical_ebv() {
    let source = protocol("EB01");
    let session = session(NumericLimits::default());
    let error = ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        "",
        SourceException {
            class: SourceExceptionClass::Sparql,
            argument: SourceExceptionArgument::Text(String::new()),
        },
    );
    let raw = session
        .construct_source_extremum_result(&SolutionValue::ReturnedError(error))
        .unwrap()
        .unwrap();
    assert_eq!(
        session.source_literal_python_truth(&raw).unwrap(),
        source["expected"]["literal_python_truth"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        session
            .source_effective_boolean_value(&SolutionValue::Rdf(raw))
            .unwrap(),
        source["expected"]["source_ebv"].as_bool().unwrap()
    );
}

#[test]
fn source_relational_unary_eb02_bad_numeric_cast_type_boundary() {
    let source = protocol("EB02");
    let session = session(NumericLimits::default());
    let raw = session
        .construct_source_direct_string("bad", Some(xsd::INTEGER.into_owned()), None, true)
        .unwrap();
    assert!(session.source_literal_python_truth(&raw).unwrap());
    let error = session
        .source_effective_boolean_value(&SolutionValue::Rdf(raw))
        .unwrap_err()
        .into_error_value()
        .unwrap();
    assert_eq!(
        error.source_exception().unwrap().class,
        SourceExceptionClass::SparqlType
    );
    assert_eq!(
        source["expected"]["source_ebv_error_args"]["status"],
        "unknown_exact_bytes_not_admitted_by_this_freeze"
    );
    // Class/control-flow only: this frozen protocol does not qualify args.
}

#[test]
fn source_relational_unary_bx05_original_top_filter() {
    super::source_exists_runtime_receiving::receive_success("BX05");
}
#[test]
fn source_relational_unary_bx06_original_deep_filter_forget() {
    super::source_exists_runtime_receiving::receive_success("BX06");
}
#[test]
fn source_relational_unary_bx09_original_not_exists_error() {
    super::source_exists_runtime_receiving::receive_success("BX09");
}

#[test]
fn source_relational_unary_output_budget_fuses_real_queries() {
    for query in [
        "SELECT (0 > 1 AS ?b) WHERE {}",
        "SELECT (!true AS ?b) WHERE {}",
    ] {
        let limits = NumericLimits {
            max_output_characters: 4,
            ..NumericLimits::default()
        };
        let prepared = numeric_query::prepare_source_exists_numeric_query(
            query,
            Backend,
            session(limits),
            Vec::new(),
        )
        .unwrap();
        let mut rows = prepared.execute_source_presence().unwrap();
        let Err(error) = rows.next().unwrap() else {
            panic!("output budget bypassed")
        };
        assert!(matches!(
            raw_numeric_fault(&error),
            Some(RawNumericFault::Core(NumericFault::ResourceLimit {
                operation: "output_characters"
            }))
        ));
        assert!(rows.next().is_none() && rows.next().is_none());
    }
}

#[test]
fn source_relational_unary_unknown_facts_stay_fatal() {
    let raw = RawExpressionValue::from_import(Literal::new_typed_literal("1", xsd::INTEGER).into());
    for query in [
        "SELECT (?x > 0 AS ?b) WHERE {}",
        "SELECT (! ?x AS ?b) WHERE {}",
    ] {
        let Err(EvalFailure::Fatal(error)) = compiled_value(
            query,
            session(NumericLimits::default()),
            Some(SolutionValue::Rdf(raw.clone())),
            Vec::new(),
        ) else {
            panic!("unknown facts were caught")
        };
        assert!(matches!(
            raw_numeric_fault(&error),
            Some(RawNumericFault::UnknownSourceLiteralIdentity)
        ));
    }
}

#[test]
fn source_relational_unary_foreign_preparation_and_old_gates_deny() {
    for query in [
        "SELECT (?x > ?rhs AS ?b) WHERE {}",
        "SELECT (! ?x AS ?b) WHERE {}",
    ] {
        let session = session(NumericLimits::default());
        let prepared = numeric_query::prepare_source_exists_numeric_query(
            query,
            Backend,
            Rc::clone(&session),
            Vec::new(),
        )
        .unwrap();
        let evaluator = prepared.evaluator();
        let codec = evaluator.dataset.numeric.as_ref().unwrap();
        for registry in [
            crate::numeric::PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(
                &session,
            )),
            crate::numeric::PreparedSourceLiteralRegistry::new(Rc::clone(&session)),
        ] {
            let mut variables = evaluator
                .source_context
                .as_ref()
                .unwrap()
                .variables
                .to_vec();
            let mut stats = Vec::new();
            let mut context = ExpressionContext {
                evaluator,
                encoded_variables: &mut variables,
                stat_children: &mut stats,
            };
            let result = build_numeric_expression_evaluator(
                first_expression(prepared.plan()),
                &mut context,
                NumericExpressionHooks {
                    session: Rc::clone(&session),
                    prepared_literals: Some(registry),
                    externalize: Rc::clone(&codec.externalize),
                    failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
                },
            );
            assert!(matches!(
                result,
                Err(ExpressionEvaluationError::UnsupportedNullableExpression)
            ));
            assert!(stats.is_empty());
        }
        let old = numeric_query::prepare_source_frontend_numeric_query(
            query,
            Backend,
            Rc::clone(&session),
            Vec::new(),
        )
        .unwrap();
        assert!(old.execute_source_presence().is_err());
    }
}

#[test]
fn source_relational_unary_first_fatal_argument_stops_right() {
    let session = session(NumericLimits::default());
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        "SELECT (?x > ?rhs AS ?b) WHERE {}",
        Backend,
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let evaluator = prepared.evaluator();
    let codec = evaluator.dataset.numeric.as_ref().unwrap();
    let mut variables = evaluator
        .source_context
        .as_ref()
        .unwrap()
        .variables
        .to_vec();
    let mut stats = Vec::new();
    let mut context = ExpressionContext {
        evaluator,
        encoded_variables: &mut variables,
        stat_children: &mut stats,
    };
    let reads = Rc::new(Cell::new(0));
    let counter = Rc::clone(&reads);
    let expression = build_numeric_expression_evaluator(
        first_expression(prepared.plan()),
        &mut context,
        NumericExpressionHooks {
            session: Rc::clone(&session),
            prepared_literals: codec.prepared_literals.clone(),
            externalize: Rc::new(move |_| {
                counter.set(counter.get() + 1);
                Err(QueryEvaluationError::Cancelled)
            }),
            failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
        },
    )
    .unwrap();
    let mut row = InternalTuple::with_capacity(variables.len());
    for (name, value) in [("x", "0"), ("rhs", "1")] {
        let slot = variables
            .iter()
            .position(|variable| variable.as_str() == name)
            .unwrap();
        row.set(slot, (codec.internalize)(integer(&session, value)).unwrap());
    }
    assert!(matches!(
        expression(&row),
        Err(EvalFailure::Fatal(QueryEvaluationError::Cancelled))
    ));
    assert_eq!(reads.get(), 1);
    assert!(session.flags().iter().next().is_none());
}

#[test]
fn source_relational_unary_boolean_cache_clone_keeps_identity() {
    let session = session(NumericLimits::default());
    let raw = session.construct_source_boolean_result(true).unwrap();
    let cloned = raw.clone();
    assert!(cloned.same_source_object(&raw));
    assert!(
        session
            .source_effective_boolean_value(&SolutionValue::Rdf(cloned))
            .unwrap()
    );
    assert!(
        session
            .source_effective_boolean_value(&SolutionValue::Rdf(raw))
            .unwrap()
    );
    assert!(session.flags().iter().next().is_none());
}
