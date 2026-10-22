//! Frozen source protocols and unchanged original BX16, through the real
//! parser/planner/codec. Component controls are separate from whole-query
//! observations; production and this adapter share an author.
use super::*;
use crate::dataset::nullable_numeric::CachedInternalTerm;
use crate::expression::{NumericExpressionHooks, build_numeric_expression_evaluator};
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    QueryContextSemantics, QueryGroupSemantics, QueryNumericCapabilities, SourceFact,
    SourceHashPolicy, SourceHashRuntime, SourceLiteralCacheKind, raw_numeric_fault,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use semantica_numeric::{DecimalContext, NumericFault, NumericLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;

type Row = InternalTuple<CachedInternalTerm<Term>>;
type Event = source_context_join::SourceContextEvent<CachedInternalTerm<Term>>;
const FIXTURE: &str = include_str!("fixtures/source_initial_cells_protocols.json");

fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "12f5797fef32d71c57741888d8feb05cb1ad21f99563058c5b9c2b2a46bafddc"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "dd4555a1ed8bd6402bcd082c5c7cefc416770ac12880ece6a81411e51896902a"
    );
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(value["protocols"].as_array().unwrap().len(), 9);
    value
}
fn protocol(id: &str) -> Value {
    fixture()["protocols"]
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
fn binding(name: &str, value: NumericInitialValue) -> NumericInitialBinding {
    NumericInitialBinding {
        variable: Variable::new(name).unwrap(),
        value,
    }
}
fn source_error() -> ErrorValue {
    let case = fixture();
    let original = &case["original_case"]["input"]["initial_bindings"][1];
    assert_eq!(original["variable"], "err");
    assert_eq!(original["value"]["class"], "NotBoundError");
    assert_eq!(original["value"]["argument"], Value::Null);
    ErrorValue::with_source(
        SparqlErrorKind::NotBound,
        "None",
        SourceException {
            class: SourceExceptionClass::NotBound,
            argument: SourceExceptionArgument::None,
        },
    )
}
fn original_initial(error: &ErrorValue) -> Vec<NumericInitialBinding> {
    assert_eq!(
        fixture()["original_case"]["input"]["initial_bindings"][0],
        json!({"variable":"x","value":{"kind":"none"}})
    );
    vec![
        binding("x", NumericInitialValue::SourceNone),
        binding("err", NumericInitialValue::SourceError(error.clone())),
    ]
}
fn iri(name: &str) -> NumericInitialValue {
    NumericInitialValue::UnknownNormalized(NamedNode::new(name).unwrap().into())
}
fn slot(variables: &[Variable], name: &str) -> usize {
    variables
        .iter()
        .position(|variable| variable.as_str() == name)
        .unwrap()
}
fn assert_same_error(cell: Option<SolutionValue<CachedInternalTerm<Term>>>, error: &ErrorValue) {
    let Some(SolutionValue::ReturnedError(actual)) = cell else {
        panic!("missing error cell")
    };
    assert_eq!(
        &actual, error,
        "must be the same Arc, not equal diagnostics"
    );
}
fn assert_boolean(raw: &RawExpressionValue, expected: bool) {
    let Term::Literal(literal) = raw.rdf().unwrap() else {
        panic!("nonliteral Boolean")
    };
    assert_eq!(literal.value(), if expected { "true" } else { "false" });
    assert_eq!(
        literal.datatype().as_str(),
        "http://www.w3.org/2001/XMLSchema#boolean"
    );
    assert!(literal.language().is_none());
    assert!(matches!(raw.numeric(), NumericCacheState::NotNumeric));
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!("lost metadata")
    };
    assert_eq!(metadata.ill_typed, SourceFact::Known(None));
    assert_eq!(metadata.language_original, SourceFact::Known(None));
    assert_eq!(
        metadata.cached_value_kind,
        SourceFact::Known(SourceLiteralCacheKind::Boolean(expected))
    );
}

#[derive(Debug)]
struct FailureIdentity;
#[derive(Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("initial-cell Dataset fault")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct BackendTrace {
    internal: Cell<usize>,
    external: Cell<usize>,
    scans: Cell<usize>,
    armed: Cell<bool>,
    failed: Cell<usize>,
}
impl BackendTrace {
    fn counts(&self) -> (usize, usize, usize) {
        (self.internal.get(), self.external.get(), self.scans.get())
    }
}
#[derive(Clone)]
struct Backend {
    trace: Rc<BackendTrace>,
    identity: Arc<FailureIdentity>,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            trace: Rc::default(),
            identity: Arc::new(FailureIdentity),
        }
    }
}
impl<'a> QueryableDataset<'a> for Backend {
    type InternalTerm = Term;
    type Error = ConversionFailure;
    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.trace.scans.set(self.trace.scans.get() + 1);
        empty()
    }
    fn internalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        self.trace.internal.set(self.trace.internal.get() + 1);
        Ok(value)
    }
    fn externalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        self.trace.external.set(self.trace.external.get() + 1);
        if self.trace.armed.get() {
            self.trace.failed.set(self.trace.failed.get() + 1);
            return Err(ConversionFailure(Arc::clone(&self.identity)));
        }
        Ok(value)
    }
}
fn prepare(
    query: &str,
    backend: Backend,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
) -> numeric_query::PreparedNumericQuery<'static, Backend> {
    numeric_query::prepare_source_exists_numeric_query(query, backend, session, initial).unwrap()
}
fn first_expression(pattern: &GraphPattern) -> &Expression {
    match pattern {
        GraphPattern::SourceContext { inner, .. } | GraphPattern::Project { inner, .. } => {
            first_expression(inner)
        }
        GraphPattern::Extend { expression, .. } => expression,
        other => panic!("missing parsed expression: {other:?}"),
    }
}
fn compiled_value(
    query: &str,
    session: Rc<QueryNumericSession>,
    backend: Backend,
    initial: Vec<NumericInitialBinding>,
    local_none: bool,
) -> Result<SolutionValue<RawExpressionValue>, EvalFailure<QueryEvaluationError>> {
    let prepared = prepare(query, backend, Rc::clone(&session), initial);
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
    if local_none {
        row.set_cell(slot(&variables, "x"), SolutionValue::ExplicitNone);
    }
    assert!(
        !row.source_has_context_owner(),
        "component uses the actual runtime initial cells without fabricating an owner"
    );
    expression(&row)
}
fn returned_error(
    value: Result<SolutionValue<RawExpressionValue>, EvalFailure<QueryEvaluationError>>,
) -> ErrorValue {
    let Ok(SolutionValue::ReturnedError(error)) = value else {
        panic!("missing caught error: {value:?}")
    };
    error
}
fn invalid<T>(value: Result<T, numeric_query::NumericPrepareError>) {
    assert!(matches!(
        value,
        Err(numeric_query::NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin
        ))
    ));
}

#[test]
fn source_initial_cells_bx16_original_complete_query() {
    let f = fixture();
    let case = &f["original_case"];
    let expected = &case["observed_outcome"];
    assert_eq!(expected["status"], "observed_complete");
    let numeric = session(NumericLimits::default());
    let error = source_error();
    let backend = Backend::default();
    let prepared = prepare(
        case["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&numeric),
        original_initial(&error),
    );
    assert_eq!(backend.trace.counts(), (0, 0, 0));
    let runtime = prepared.evaluator().source_context.as_ref().unwrap();
    let variables = Rc::clone(&runtime.variables);
    let initial = Rc::clone(&runtime.initial);
    assert_eq!(
        initial
            .source_local_order()
            .iter()
            .map(|&s| variables[s].as_str())
            .collect::<Vec<_>>(),
        ["x", "err"]
    );
    assert!(matches!(
        initial.cell(slot(&variables, "x")),
        Some(SolutionValue::ExplicitNone)
    ));
    assert_same_error(initial.cell(slot(&variables, "err")).cloned(), &error);
    let events: Rc<RefCell<Vec<Event>>> = Rc::default();
    let sink = Rc::clone(&events);
    let prepared =
        prepared.with_source_context_observer(Rc::new(move |event| sink.borrow_mut().push(event)));
    let mut rows = prepared.execute_source_presence().unwrap();
    let row = rows.next().unwrap().unwrap();
    assert_eq!(
        row.variables
            .iter()
            .map(Variable::as_str)
            .collect::<Vec<_>>(),
        expected["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["value"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(expected["row_count"], 1);
    assert!(matches!(row.values[0], Some(SolutionValue::ExplicitNone)));
    assert_eq!(expected["rows"][0]["local_items"][0][1], Value::Null);
    for i in [1, 2] {
        let Some(SolutionValue::Rdf(raw)) = &row.values[i] else {
            panic!("missing Boolean")
        };
        let value = &expected["rows"][0]["local_items"][i][1];
        assert_eq!(value["cache_type"], "builtins.bool");
        assert_boolean(raw, value["cache"].as_bool().unwrap());
        let Term::Literal(literal) = raw.rdf().unwrap() else {
            unreachable!()
        };
        assert_eq!(literal.value(), value["lexical"].as_str().unwrap());
        assert_eq!(
            literal.datatype().as_str(),
            value["datatype"].as_str().unwrap()
        );
    }
    let (Some(SolutionValue::Rdf(left)), Some(SolutionValue::Rdf(right))) =
        (&row.values[1], &row.values[2])
    else {
        unreachable!()
    };
    assert!(!left.same_source_object(right));
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(expected["decimal_flags_after_query"], json!([]));
    assert!(numeric.flags().iter().next().is_none());
    assert_eq!(backend.trace.scans.get(), 0);
    let events = events.borrow();
    let outputs = events
        .iter()
        .filter_map(|event| match event {
            Event::OutputRow {
                row,
                initial: actual,
            } => Some((row, actual)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(outputs.len(), 1);
    let (output, actual_initial) = outputs[0];
    assert!(Rc::ptr_eq(actual_initial, &initial));
    assert!(
        !output.contains_cell(slot(&variables, "err")),
        "projection removes local err"
    );
    assert_same_error(
        output.source_context_lookup(slot(&variables, "err")),
        &error,
    );
    assert_eq!(
        output
            .source_local_order()
            .iter()
            .map(|&s| variables[s].as_str())
            .collect::<Vec<_>>(),
        ["x", "b", "present"]
    );
}

#[test]
fn source_initial_cells_ic01_order_and_single_rdf_conversion() {
    drop(protocol("IC01"));
    let error = source_error();
    let backend = Backend::default();
    let mut initial = original_initial(&error);
    initial.push(binding("uri", iri("urn:initial")));
    let prepared = prepare(
        "SELECT ?x WHERE {}",
        backend.clone(),
        session(NumericLimits::default()),
        initial,
    );
    // Existing SourceLiteralV1 internalize_raw reads back a nonliteral once
    // to verify the backend. None/error contribute neither conversion.
    assert_eq!(backend.trace.counts(), (1, 1, 0));
    let runtime = prepared.evaluator().source_context.as_ref().unwrap();
    let encoded = prepared.initial().encode_for(&runtime.variables).unwrap();
    assert_eq!(
        encoded
            .source_local_order()
            .iter()
            .map(|&s| runtime.variables[s].as_str())
            .collect::<Vec<_>>(),
        ["x", "err", "uri"]
    );
    assert_same_error(
        encoded.cell(slot(&runtime.variables, "err")).cloned(),
        &error,
    );
    assert!(matches!(
        encoded.cell(slot(&runtime.variables, "x")),
        Some(SolutionValue::ExplicitNone)
    ));
    let Some(SolutionValue::Rdf(value)) = encoded.cell(slot(&runtime.variables, "uri")) else {
        panic!()
    };
    assert_eq!(
        value.rdf(),
        &Term::NamedNode(NamedNode::new("urn:initial").unwrap())
    );
    assert_eq!(
        backend.trace.counts(),
        (1, 1, 0),
        "encoding must not construct/internalize or read back again"
    );
}

#[test]
fn source_initial_cells_ic02_local_none_masks_initial_rdf() {
    drop(protocol("IC02"));
    let backend = Backend::default();
    let result = compiled_value(
        "SELECT (?x AS ?out) WHERE {}",
        session(NumericLimits::default()),
        backend.clone(),
        vec![binding("x", iri("urn:initial"))],
        true,
    )
    .unwrap();
    assert!(matches!(result, SolutionValue::ExplicitNone));
    assert_eq!(backend.trace.counts(), (1, 1, 0));
    let relation_backend = Backend::default();
    let error = returned_error(compiled_value(
        "SELECT (?x > ?rhs AS ?out) WHERE {}",
        session(NumericLimits::default()),
        relation_backend.clone(),
        vec![
            binding("x", iri("urn:initial")),
            binding(
                "rhs",
                NumericInitialValue::UnknownNormalized(Literal::from(1).into()),
            ),
        ],
        true,
    ));
    assert_eq!(
        error.source_exception(),
        Some(&SourceException {
            class: SourceExceptionClass::NotBound,
            argument: SourceExceptionArgument::None
        })
    );
    assert_eq!(
        relation_backend.trace.counts(),
        (2, 1, 0),
        "only initial nonliteral readback; masked first operand prevents later operand externalization"
    );
}

#[test]
fn source_initial_cells_ic03_project_forget_thaw_keep_owner() {
    drop(protocol("IC03"));
    let error = source_error();
    let backend = Backend::default();
    let prepared = prepare(
        "SELECT ?x WHERE {}",
        backend.clone(),
        session(NumericLimits::default()),
        original_initial(&error),
    );
    let runtime = prepared.evaluator().source_context.as_ref().unwrap();
    let x = slot(&runtime.variables, "x");
    let err = slot(&runtime.variables, "err");
    let current = runtime
        .initial
        .source_context_from_bindings(Rc::clone(&runtime.initial));
    let frozen = current.source_solution(None);
    let projected = frozen.source_project(&[]);
    let forgotten = frozen.source_forget(&current, &runtime.initial, &[]);
    assert!(projected.source_local_order().is_empty());
    assert!(projected.source_context_owner_same(&frozen));
    for row in [&projected, &forgotten] {
        assert!(matches!(
            row.source_context_lookup(x),
            Some(SolutionValue::ExplicitNone)
        ));
        assert_same_error(row.source_context_lookup(err), &error);
    }
    let thawed = projected.source_thaw_owned().unwrap();
    assert!(!thawed.source_context_owner_same(&frozen));
    assert!(thawed.source_context_initial_shared(&frozen));
    assert_eq!(
        thawed.source_local_order(),
        runtime.initial.source_local_order()
    );
    assert_same_error(thawed.cell(err).cloned(), &error);
    assert_eq!(backend.trace.counts(), (0, 0, 0));
    // Added whole-query control: projected error output remains a cell with
    // its original identity. It is not a new independently observed BX case.
    let prepared = prepare(
        "SELECT ?x ?err WHERE {}",
        backend.clone(),
        session(NumericLimits::default()),
        original_initial(&error),
    );
    let mut rows = prepared.execute_source_presence().unwrap();
    let row = rows.next().unwrap().unwrap();
    assert!(matches!(row.values[0], Some(SolutionValue::ExplicitNone)));
    let Some(SolutionValue::ReturnedError(actual)) = &row.values[1] else {
        panic!("lost projected error cell")
    };
    assert_eq!(actual, &error);
    assert!(rows.next().is_none());
    assert_eq!(backend.trace.counts(), (0, 0, 0));
}

fn receive_bound(initial: Vec<NumericInitialBinding>, expected: bool) {
    let numeric = session(NumericLimits::default());
    let result = compiled_value(
        "SELECT (BOUND(?x) AS ?b) WHERE {}",
        Rc::clone(&numeric),
        Backend::default(),
        initial,
        false,
    )
    .unwrap();
    let SolutionValue::Rdf(raw) = result else {
        panic!("BOUND must return Boolean")
    };
    assert_boolean(&raw, expected);
    assert!(numeric.flags().iter().next().is_none());
}
#[test]
fn source_initial_cells_ic04_bound_rdf_true() {
    let p = protocol("IC04");
    receive_bound(
        vec![binding("x", iri("urn:initial"))],
        p["expected"]["rdf"].as_bool().unwrap(),
    );
}
#[test]
fn source_initial_cells_ic04_bound_none_false() {
    let p = protocol("IC04");
    receive_bound(
        vec![binding("x", NumericInitialValue::SourceNone)],
        p["expected"]["none"].as_bool().unwrap(),
    );
}
#[test]
fn source_initial_cells_ic04_bound_absent_false() {
    let p = protocol("IC04");
    receive_bound(Vec::new(), p["expected"]["absent"].as_bool().unwrap());
}
#[test]
fn source_initial_cells_ic04_bound_error_preserves_arc() {
    drop(protocol("IC04"));
    let error = source_error();
    let numeric = session(NumericLimits {
        max_output_characters: 4,
        ..NumericLimits::default()
    });
    let actual = returned_error(compiled_value(
        "SELECT (BOUND(?x) AS ?b) WHERE {}",
        Rc::clone(&numeric),
        Backend::default(),
        vec![binding(
            "x",
            NumericInitialValue::SourceError(error.clone()),
        )],
        false,
    ));
    assert_eq!(
        actual, error,
        "error is raised before Builtin_BOUND, not Boolean false or an output-budget error"
    );
    assert!(numeric.flags().iter().next().is_none());
}

#[test]
fn source_initial_cells_ic05_unary_error_keeps_original_arc() {
    drop(protocol("IC05"));
    let error = source_error();
    let actual = returned_error(compiled_value(
        "SELECT (!?err AS ?out) WHERE {}",
        session(NumericLimits::default()),
        Backend::default(),
        original_initial(&error),
        false,
    ));
    assert_eq!(actual, error);
}
#[test]
fn source_initial_cells_ic05_relation_error_stops_rhs() {
    drop(protocol("IC05"));
    let error = source_error();
    let backend = Backend::default();
    let mut initial = original_initial(&error);
    initial.push(binding(
        "rhs",
        NumericInitialValue::UnknownNormalized(Literal::from(1).into()),
    ));
    let actual = returned_error(compiled_value(
        "SELECT (?err > ?rhs AS ?out) WHERE {}",
        session(NumericLimits::default()),
        backend.clone(),
        initial,
        false,
    ));
    assert_eq!(actual, error);
    assert_eq!(backend.trace.counts(), (1, 0, 0));
}

#[test]
fn source_initial_cells_ic06_duplicate_variable_across_cell_kinds() {
    drop(protocol("IC06"));
    let inputs = [
        NumericInitialValue::SourceNone,
        NumericInitialValue::SourceError(source_error()),
        iri("urn:initial"),
    ];
    for left in &inputs {
        for right in &inputs {
            invalid(numeric_query::prepare_source_exists_numeric_query(
                "SELECT ?x WHERE {}",
                Backend::default(),
                session(NumericLimits::default()),
                vec![binding("x", left.clone()), binding("x", right.clone())],
            ));
        }
    }
}

#[test]
fn source_initial_cells_ic07_old_preparation_denies_cells() {
    drop(protocol("IC07"));
    for value in [
        NumericInitialValue::SourceNone,
        NumericInitialValue::SourceError(source_error()),
    ] {
        invalid(numeric_query::prepare_numeric_query(
            "SELECT ?x WHERE {}",
            Backend::default(),
            session(NumericLimits::default()),
            vec![binding("x", value)],
        ));
    }
}
#[test]
fn source_initial_cells_ic07_frontend_does_not_grant_execution() {
    drop(protocol("IC07"));
    for value in [
        NumericInitialValue::SourceNone,
        NumericInitialValue::SourceError(source_error()),
    ] {
        invalid(numeric_query::prepare_source_frontend_numeric_query(
            "SELECT ?x WHERE {}",
            Backend::default(),
            session(NumericLimits::default()),
            vec![binding("x", value)],
        ));
    }
}
#[test]
fn source_initial_cells_ic07_rdf_constructor_refuses_cells() {
    drop(protocol("IC07"));
    let numeric = session(NumericLimits::default());
    for value in [
        NumericInitialValue::SourceNone,
        NumericInitialValue::SourceError(source_error()),
    ] {
        assert!(matches!(
            numeric.construct_initial(value),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
    }
}
#[test]
fn source_initial_cells_ic07_diagnostic_and_mismatched_error_refused() {
    drop(protocol("IC07"));
    for error in [
        ErrorValue::with_kind(SparqlErrorKind::NotBound, "None"),
        ErrorValue::with_source(
            SparqlErrorKind::Type,
            "None",
            SourceException {
                class: SourceExceptionClass::NotBound,
                argument: SourceExceptionArgument::None,
            },
        ),
    ] {
        invalid(numeric_query::prepare_source_exists_numeric_query(
            "SELECT ?x WHERE {}",
            Backend::default(),
            session(NumericLimits::default()),
            vec![binding("x", NumericInitialValue::SourceError(error))],
        ));
    }
    // Active constructor-domain control, separate from original BX16's
    // NotBound(None). Only the existing three typed source classes and two
    // represented argument kinds are admitted; no arbitrary Python class.
    for (class, kind) in [
        (SourceExceptionClass::NotBound, SparqlErrorKind::NotBound),
        (SourceExceptionClass::SparqlType, SparqlErrorKind::Type),
        (SourceExceptionClass::Sparql, SparqlErrorKind::OtherSparql),
    ] {
        for argument in [
            SourceExceptionArgument::None,
            SourceExceptionArgument::Text("source argument".into()),
        ] {
            let source = SourceException { class, argument };
            let error = ErrorValue::with_source(
                kind,
                "diagnostic is not the source argument",
                source.clone(),
            );
            let actual = returned_error(compiled_value(
                "SELECT (BOUND(?x) AS ?b) WHERE {}",
                session(NumericLimits {
                    max_output_characters: 1,
                    ..NumericLimits::default()
                }),
                Backend::default(),
                vec![binding(
                    "x",
                    NumericInitialValue::SourceError(error.clone()),
                )],
                false,
            ));
            assert_eq!(actual, error);
            assert_eq!(actual.source_exception(), Some(&source));
        }
    }
}
#[test]
fn source_initial_cells_ic07_foreign_session_cannot_borrow_bound() {
    drop(protocol("IC07"));
    let prepared = prepare(
        "SELECT (BOUND(?x) AS ?b) WHERE {}",
        Backend::default(),
        session(NumericLimits::default()),
        vec![binding("x", NumericInitialValue::SourceNone)],
    );
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
    assert!(matches!(
        build_numeric_expression_evaluator(
            first_expression(prepared.plan()),
            &mut context,
            NumericExpressionHooks {
                session: session(NumericLimits::default()),
                prepared_literals: codec.prepared_literals.clone(),
                externalize: Rc::clone(&codec.externalize),
                failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
            }
        ),
        Err(crate::expression::ExpressionEvaluationError::UnsupportedNullableExpression)
    ));
}

#[test]
fn source_initial_cells_ic08_reached_dataset_fault_identity_and_fuse() {
    drop(protocol("IC08"));
    let error = source_error();
    let backend = Backend::default();
    let prepared = prepare(
        "SELECT ?x (BOUND(?x) AS ?b) WHERE {}",
        backend.clone(),
        session(NumericLimits::default()),
        original_initial(&error),
    );
    assert_eq!(backend.trace.counts(), (0, 0, 0));
    backend.trace.armed.set(true);
    let mut rows = prepared.execute_source_presence().unwrap();
    let Some(Err(QueryEvaluationError::Dataset(error))) = rows.next() else {
        panic!("lost Dataset fault")
    };
    assert!(Arc::ptr_eq(
        &error.downcast_ref::<ConversionFailure>().unwrap().0,
        &backend.identity
    ));
    assert_eq!(backend.trace.failed.get(), 1);
    let counts = backend.trace.counts();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(backend.trace.counts(), counts);
}
#[test]
fn source_initial_cells_ic08_actual_cancellation_before_poll() {
    drop(protocol("IC08"));
    let error = source_error();
    let backend = Backend::default();
    let prepared = prepare(
        "SELECT ?x (BOUND(?x) AS ?b) WHERE {}",
        backend.clone(),
        session(NumericLimits::default()),
        original_initial(&error),
    );
    let token = prepared.evaluator().dataset.cancellation_token.clone();
    let mut rows = prepared.execute_source_presence().unwrap();
    token.cancel();
    assert!(matches!(
        rows.next(),
        Some(Err(QueryEvaluationError::Cancelled))
    ));
    let counts = backend.trace.counts();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(counts, (0, 0, 0));
    assert_eq!(backend.trace.counts(), counts);
}
#[test]
fn source_initial_cells_ic09_boolean_budget_is_fatal_and_fused() {
    drop(protocol("IC09"));
    let backend = Backend::default();
    let numeric = session(NumericLimits {
        max_output_characters: 4,
        ..NumericLimits::default()
    });
    let prepared = prepare(
        "SELECT (BOUND(?x) AS ?b) WHERE {}",
        backend.clone(),
        Rc::clone(&numeric),
        vec![binding("x", NumericInitialValue::SourceNone)],
    );
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = rows.next().unwrap().err().unwrap();
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::Core(NumericFault::ResourceLimit {
            operation: "output_characters"
        }))
    ));
    let counts = backend.trace.counts();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(backend.trace.counts(), counts);
    assert!(numeric.flags().iter().next().is_none());
}

#[test]
fn source_initial_cells_ic03_actual_subquery_preserves_none_and_unused_error() {
    drop(protocol("IC03"));
    let error = source_error();
    let prepared = prepare(
        "SELECT ?b WHERE { { SELECT (BOUND(?x) AS ?b) WHERE {} } }",
        Backend::default(),
        session(NumericLimits::default()),
        original_initial(&error),
    );
    let runtime = prepared.evaluator().source_context.as_ref().unwrap();
    let variables = Rc::clone(&runtime.variables);
    let err = slot(&variables, "err");
    let observed = Rc::new(Cell::new(0));
    let counter = Rc::clone(&observed);
    let original_error = error.clone();
    let prepared = prepared.with_source_context_observer(Rc::new(move |event| {
        if let Event::OutputRow { row, .. } = event {
            assert_same_error(row.source_context_lookup(err), &original_error);
            counter.set(counter.get() + 1);
        }
    }));
    let mut rows = prepared.execute_source_presence().unwrap();
    let row = rows.next().unwrap().unwrap();
    assert_eq!(
        row.variables
            .iter()
            .map(Variable::as_str)
            .collect::<Vec<_>>(),
        ["b"]
    );
    let Some(SolutionValue::Rdf(raw)) = &row.values[0] else {
        panic!()
    };
    assert_boolean(raw, false);
    assert!(rows.next().is_none());
    assert_eq!(observed.get(), 1);
}
