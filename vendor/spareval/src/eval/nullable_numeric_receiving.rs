//! Receiving adapter for independently frozen query inputs. This module uses
//! the sole prepared parser/production planner/compiled evaluator path.
use super::nullable::NullableAggregateEvaluator;
use super::numeric_query::{NumericPrepareError, prepare_numeric_query};
use super::*;
use crate::dataset::nullable_numeric::CachedInternalTerm;
use crate::numeric::{
    NumericCacheState, NumericEvaluationError, NumericInitialBinding, NumericInitialValue,
    QueryNumericSession, RawExpressionValue, RawNumericFault, raw_numeric_fault,
};
use crate::tuple::{ErrorValue, SparqlErrorKind};
use oxrdf::{Dataset, GraphName, Quad};
use semantica_numeric::{
    DecimalContext, DecimalSignal, DecimalValue, NumericFault, NumericLimits, NumericValue,
    RoundingMode,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::Hasher;

const FIXTURE: &str = include_str!("../../tests/fixtures/query_numeric_cases.json");
const ORIGINAL_SHA: &str = "d42be1722ef47d25f72c1bb563eda15cd55c8c085e5151d09516800c3a1badbf";
const CURRENT_SHA: &str = "2a63d9a7043935df5c925b5ed5285d4447a24c9f0d09a532432fd8c2442cf5fd";

fn fixture() -> Value {
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(value["source_record_sha256"], CURRENT_SHA);
    assert_eq!(value["source_record_id"], "QUERY-NUMERIC-FROZEN-02");
    assert_eq!(
        value["revision"]["previous_source_record_sha256"],
        ORIGINAL_SHA
    );
    assert_eq!(value["case_count"], 28);
    // Feature unification may enable serde_json's preserve_order. Sort every
    // object explicitly so fixture identity never depends on that feature.
    let canonical = serde_json::to_vec(&canonical_json(&value["cases"])).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(canonical)),
        "42f57810e535d6695de2f66fe688fd817ae1c36ef1c97c830e5c0c0180f01f10"
    );
    let environment = serde_json::to_vec(&canonical_json(&value["explicit_environment"])).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(environment)),
        "b03d67ecc76353f0f8319a769bf938817cf7fc119808a4f835a1015f3d97390f"
    );
    value
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted = map.iter().collect::<BTreeMap<_, _>>();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonical_json(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}
fn text<'v>(value: &'v Value, key: &str) -> &'v str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing string {key}"))
}
fn rdf(value: &Value) -> Term {
    match text(value, "kind") {
        "literal" => Literal::new_typed_literal(
            text(value, "lexical"),
            NamedNode::new(text(value, "datatype")).unwrap(),
        )
        .into(),
        "named_node" => NamedNode::new(text(value, "iri")).unwrap().into(),
        kind => panic!("unsupported frozen input RDF kind {kind}"),
    }
}
fn rdf_json(term: &Term) -> Value {
    match term {
        Term::Literal(literal) => {
            json!({"kind":"literal", "lexical":literal.value(), "datatype":literal.datatype().as_str()})
        }
        Term::NamedNode(node) => json!({"kind":"named_node", "iri":node.as_str()}),
        _ => panic!("unexpected RDF output family"),
    }
}
fn numeric_input(value: &Value) -> NumericValue {
    match text(value, "kind") {
        "integer" => NumericValue::Integer(text(value, "value").parse().unwrap()),
        "finite" => NumericValue::Decimal(DecimalValue::Finite {
            negative: value["negative"].as_bool().unwrap(),
            coefficient: text(value, "coefficient").parse().unwrap(),
            exponent: text(value, "exponent").parse().unwrap(),
        }),
        kind => panic!("unsupported frozen cached INPUT family {kind}"),
    }
}
fn numeric_json(value: &NumericValue) -> Value {
    match value {
        NumericValue::Integer(value) => json!({"kind":"integer", "value":value.to_string()}),
        NumericValue::Decimal(DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        }) => {
            json!({"kind":"finite", "negative":negative, "coefficient":coefficient.to_string(), "exponent":exponent.to_string()})
        }
        NumericValue::Decimal(DecimalValue::Infinity { negative }) => {
            json!({"kind":"infinity", "negative":negative})
        }
        NumericValue::Decimal(DecimalValue::NaN {
            negative,
            signaling,
            payload,
        }) => json!({"kind":"nan", "negative":negative, "signaling":signaling, "payload":payload}),
        NumericValue::Binary64(value) => {
            json!({"kind":"binary64", "bits":value.to_bits().to_string()})
        }
    }
}
fn cache_json(cache: &NumericCacheState) -> Value {
    match cache {
        NumericCacheState::ErrorObject(_) => panic!("numeric receiving does not construct Literal(error)"),
        NumericCacheState::Known(cache) => {
            json!({"state":"Known", "value":numeric_json(cache.value())})
        }
        NumericCacheState::UnknownNormalized => json!({"state":"Unknown"}),
        NumericCacheState::CastFailed => json!({"state":"CastFailed"}),
        NumericCacheState::NotNumeric => json!({"state":"NotNumeric"}),
    }
}
fn signal(name: &str) -> DecimalSignal {
    DecimalSignal::ALL
        .into_iter()
        .find(|signal| format!("{signal:?}") == name)
        .unwrap()
}
fn session(root: &Value, run: &Value) -> Rc<QueryNumericSession> {
    let env = &root["explicit_environment"];
    let ctx = &env["decimal_context"];
    assert_eq!(ctx["rounding"], "HALF_EVEN");
    let decimal = DecimalContext {
        precision: ctx["precision"].as_u64().unwrap().try_into().unwrap(),
        rounding: RoundingMode::HalfEven,
        emin: ctx["Emin"].as_i64().unwrap().into(),
        emax: ctx["Emax"].as_i64().unwrap().into(),
        clamp: ctx["clamp"] == 1,
        traps: ctx["traps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| signal(s.as_str().unwrap()))
            .collect(),
    };
    let limit = |key: &str| -> usize {
        run.get("numeric_limits_override")
            .and_then(|v| v.get(key))
            .unwrap_or(&env["numeric_limits"][key])
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap()
    };
    Rc::new(
        QueryNumericSession::new(
            decimal,
            NumericLimits {
                max_input_characters: limit("max_input_characters"),
                max_coefficient_digits: limit("max_coefficient_digits"),
                max_output_characters: limit("max_output_characters"),
                max_materialized_power_of_ten: limit("max_materialized_power_of_ten"),
            },
            Some(
                env["integer_text_limit"]
                    .as_u64()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ),
            env["normalize_literals"].as_bool().unwrap(),
        )
        .unwrap(),
    )
}
fn initial(value: &Value) -> Vec<NumericInitialBinding> {
    value
        .as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .map(|(name, binding)| {
            let value = match text(binding, "origin") {
                "DirectString" => NumericInitialValue::DirectString {
                    lexical: text(binding, "input_lexical").into(),
                    datatype: NamedNode::new(text(binding, "datatype")).unwrap(),
                    normalize: binding["normalize"].as_bool().unwrap(),
                },
                "KnownCached" => NumericInitialValue::KnownCached {
                    rdf: rdf(&binding["rdf"]),
                    value: numeric_input(&binding["cache"]["value"]),
                },
                "UnknownNormalized" => NumericInitialValue::UnknownNormalized(rdf(&binding["rdf"])),
                origin => panic!("unknown frozen initial origin {origin}"),
            };
            NumericInitialBinding {
                variable: Variable::new(name).unwrap(),
                value,
            }
        })
        .collect()
}

#[derive(Debug)]
struct FailureIdentity;
#[derive(Clone, Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("query numeric receiving conversion failure")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct Trace {
    armed: Cell<bool>,
    failing_calls: Cell<usize>,
    internalizations: Cell<usize>,
    scans: Cell<usize>,
    object_patterns: RefCell<Vec<Option<Term>>>,
}
#[derive(Clone)]
struct ReceivingDataset {
    graph: Rc<Dataset>,
    trace: Rc<Trace>,
    identity: Arc<FailureIdentity>,
    lossy: bool,
    failing: bool,
}
impl ReceivingDataset {
    fn new(run: &Value, case_id: &str) -> Self {
        let mut graph = Dataset::new();
        if let Some(triples) = run["dataset_fixture"]["triples"].as_array() {
            for triple in triples {
                graph.insert(&Quad::new(
                    NamedNode::new(text(triple, "subject")).unwrap(),
                    NamedNode::new(text(triple, "predicate")).unwrap(),
                    rdf(&triple["object"]),
                    GraphName::DefaultGraph,
                ));
            }
        }
        Self {
            graph: Rc::new(graph),
            trace: Rc::new(Trace::default()),
            identity: Arc::new(FailureIdentity),
            lossy: case_id == "QN21",
            failing: case_id == "QN22",
        }
    }
}
impl<'a> QueryableDataset<'a> for ReceivingDataset {
    type InternalTerm = Term;
    type Error = ConversionFailure;
    fn internal_quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph_name: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.trace.scans.set(self.trace.scans.get() + 1);
        self.trace
            .object_patterns
            .borrow_mut()
            .push(object.cloned());
        let rows = self
            .graph
            .iter()
            .filter_map(|quad| {
                if matches!(graph_name, Some(Some(_))) {
                    return None;
                }
                let row = InternalQuad {
                    subject: Term::from(quad.subject.into_owned()),
                    predicate: Term::from(quad.predicate.into_owned()),
                    object: quad.object.into_owned(),
                    graph_name: None,
                };
                if subject.is_some_and(|s| s != &row.subject)
                    || predicate.is_some_and(|p| p != &row.predicate)
                    || object.is_some_and(|o| o != &row.object)
                {
                    return None;
                }
                Some(Ok(row))
            })
            .collect::<Vec<_>>();
        rows.into_iter()
    }
    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.trace
            .internalizations
            .set(self.trace.internalizations.get() + 1);
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        if self.failing
            && self.trace.armed.get()
            && matches!(&term, Term::NamedNode(node) if node.as_str() == "urn:query-numeric-fatal")
        {
            self.trace
                .failing_calls
                .set(self.trace.failing_calls.get() + 1);
            return Err(ConversionFailure(Arc::clone(&self.identity)));
        }
        if self.lossy && matches!(&term, Term::Literal(lit) if lit.value() == "1.00") {
            return Ok(Literal::new_typed_literal(
                "1.0",
                NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
            )
            .into());
        }
        Ok(term)
    }
}

#[derive(Default)]
struct HashBytes(Vec<u8>);
impl Hasher for HashBytes {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}
fn hash_bytes(value: &impl Hash) -> Vec<u8> {
    let mut hash = HashBytes::default();
    value.hash(&mut hash);
    hash.0
}
fn row_json(row: PresenceSolution) -> Value {
    let mut map = serde_json::Map::new();
    for (variable, value) in row.variables.iter().zip(row.values) {
        match value {
            Some(SolutionValue::Rdf(term)) => {
                map.insert(variable.as_str().into(), rdf_json(&term));
            }
            None => {}
            _ => panic!("unexpected non-RDF output for this frozen case"),
        }
    }
    Value::Object(map)
}
fn assert_rows(actual: &[Value], expected: &Value) {
    let mut remaining = expected.as_array().unwrap().clone();
    for row in actual {
        let pos = remaining
            .iter()
            .position(|wanted| wanted == row)
            .unwrap_or_else(|| panic!("unexpected result row {row}"));
        remaining.remove(pos);
    }
    assert!(remaining.is_empty(), "missing rows {remaining:?}");
}
fn assert_failure(error: &QueryEvaluationError, expected: &Value, dataset: &ReceivingDataset) {
    let category = text(expected, "semantic_category");
    if category == "Dataset" {
        let QueryEvaluationError::Dataset(error) = error else {
            panic!("expected Dataset failure")
        };
        let error = error
            .downcast_ref::<ConversionFailure>()
            .expect("original Dataset error type");
        assert!(Arc::ptr_eq(&error.0, &dataset.identity));
        return;
    }
    let fault = raw_numeric_fault(error).expect("typed original RawNumericFault must survive");
    match (category, fault) {
        ("UnknownNumericCache", RawNumericFault::UnknownNumericCache)
        | ("LossyNumericBackend", RawNumericFault::LossyNumericBackend)
        | ("UnsupportedNumericFamily", RawNumericFault::UnsupportedNumericFamily) => {}
        ("ResourceLimit", RawNumericFault::Core(NumericFault::ResourceLimit { .. })) => {}
        ("DecimalTrap", RawNumericFault::Core(NumericFault::DecimalTrap { primary, signals })) => {
            assert_eq!(format!("{primary:?}"), text(expected, "primary_signal"));
            for required in expected["required_signals"].as_array().unwrap() {
                assert!(signals.contains(signal(required.as_str().unwrap())));
            }
        }
        _ => panic!("wrong typed fatal category: wanted {category}, got {fault:?}"),
    }
}
fn legacy(run: &Value) {
    let query = spargebra::SparqlParser::new()
        .parse_query(text(run, "query"))
        .unwrap();
    if run["expect"]["kind"] == "parser_structure" {
        let spargebra::Query::Select { pattern, .. } = query else {
            panic!("SELECT")
        };
        let spargebra::algebra::GraphPattern::Project { inner, .. } = pattern else {
            panic!("Project")
        };
        let spargebra::algebra::GraphPattern::Extend { expression, .. } = *inner else {
            panic!("Extend")
        };
        let spargebra::algebra::Expression::Add(left, _) = expression else {
            panic!("binary Add")
        };
        assert!(matches!(*left, spargebra::algebra::Expression::Add(_, _)));
        return;
    }
    let graph = Dataset::new();
    let evaluator = crate::QueryEvaluator::new();
    let crate::QueryResults::Solutions(rows) = evaluator.prepare(&query).execute(&graph).unwrap()
    else {
        panic!("solutions")
    };
    let actual = rows
        .map(|row| {
            let row = row.unwrap();
            let map = row
                .iter()
                .map(|(variable, term)| (variable.as_str().to_owned(), rdf_json(term)))
                .collect::<serde_json::Map<_, _>>();
            Value::Object(map)
        })
        .collect::<Vec<_>>();
    assert_rows(&actual, &run["expect"]["rows"]);
}

fn execute_run(root: &Value, case: &Value, run: &Value, inputs: &Value) {
    let case_id = text(case, "id");
    if run["execution_lane"] == "legacy_default" {
        legacy(run);
        return;
    }
    let session = session(root, run);
    let dataset = ReceivingDataset::new(run, case_id);
    let mut bindings = initial(inputs);
    if case_id == "QN22" {
        bindings.push(NumericInitialBinding {
            variable: Variable::new("fatal").unwrap(),
            value: NumericInitialValue::UnknownNormalized(
                NamedNode::new("urn:query-numeric-fatal").unwrap().into(),
            ),
        });
    }
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset.clone(),
        Rc::clone(&session),
        bindings,
    );
    if run["expect"]["kind"] == "source_parse_failure" {
        assert!(matches!(
            prepared,
            Err(NumericPrepareError::LiteralConstruction(
                RawNumericFault::ParserLiteralType
            ))
        ));
        assert_eq!(dataset.trace.scans.get(), 0);
        assert_eq!(dataset.trace.internalizations.get(), 0);
        return;
    }
    let prepared = prepared.unwrap_or_else(|error| {
        panic!("{case_id}: preparation failed before required execution: {error:?}")
    });
    let mut variables = Vec::new();
    let (plan, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(prepared.plan(), &mut variables);
    let plan = plan.unwrap_or_else(|error| panic!("{case_id}: compile failed: {error}"));
    let before_encode = dataset.trace.internalizations.get();
    let input = prepared.initial().encode_for(&variables).unwrap();
    assert_eq!(
        dataset.trace.internalizations.get(),
        before_encode,
        "initial tuple must clone, not internalize again"
    );
    let held = Rc::new(RefCell::new(Vec::<
        BTreeMap<String, CachedInternalTerm<Term>>,
    >::new()));
    let observed = Rc::clone(&held);
    let observed_variables = variables.clone();
    // Group may consume its complete child when the plan is activated, before
    // the first next(). Arm after preparation/compilation but before activation.
    dataset.trace.armed.set(true);
    let rows = plan(input).expect("legacy query starts successfully").map(move |row| {
        if let Ok(tuple) = &row {
            let mut values = BTreeMap::new();
            for (index, variable) in observed_variables.iter().enumerate() {
                if let Some(SolutionValue::Rdf(value)) = tuple.cell(index) {
                    assert_eq!(
                        hash_bytes(value),
                        hash_bytes(value.rdf()),
                        "numeric wrapper changed old RDF Hash bytes"
                    );
                    assert_eq!(
                        cache_json(value.numeric()),
                        cache_json(value.clone().numeric()),
                        "clone changed cache"
                    );
                    if let NumericCacheState::Known(cache) = value.numeric() {
                        let Term::Literal(literal) = value.rdf() else {
                            panic!("Known numeric must be a real literal")
                        };
                        assert_eq!(cache.datatype().as_str(), literal.datatype().as_str());
                    }
                    values.insert(variable.as_str().into(), value.clone());
                }
            }
            observed.borrow_mut().push(values);
        }
        row
    });
    let mut decoded = decode_presence_bindings(
        prepared.evaluator().dataset.clone(),
        Box::new(rows),
        variables.into(),
    );
    if run["expect"]["kind"] == "fatal" {
        let first = decoded
            .next()
            .expect("fatal must occur in the real iterator");
        let Err(error) = first else {
            panic!("fatal became a successful row")
        };
        assert_failure(&error, &run["expect"], &dataset);
        let calls = dataset.trace.failing_calls.get();
        let scans = dataset.trace.scans.get();
        let internals = dataset.trace.internalizations.get();
        assert!(decoded.next().is_none(), "fatal iterator did not fuse");
        assert_eq!(dataset.trace.failing_calls.get(), calls);
        assert_eq!(dataset.trace.scans.get(), scans);
        assert_eq!(dataset.trace.internalizations.get(), internals);
        assert!(
            held.borrow().is_empty(),
            "fatal group published a partial row"
        );
        if case_id == "QN22" {
            assert_eq!(calls, 1);
        }
        return;
    }
    let actual = decoded
        .map(|row| row_json(row.unwrap()))
        .collect::<Vec<_>>();
    assert_rows(&actual, &run["expect"]["rows"]);
    let held = held.borrow();
    if let Some(checkpoints) = run["checkpoints"].as_array() {
        assert!(!held.is_empty(), "checkpoint was not reached");
        for check in checkpoints {
            for row in held.iter() {
                let value = row
                    .get(text(check, "binding"))
                    .expect("projected cache checkpoint");
                let mut expected = check["cache"].clone();
                // Unknown's frozen reason denotes missing origin, not a Rust enum
                // spelling. The actual input factory and fatal tests prove that
                // origin; retain the distinct Unknown/CastFailed state here.
                expected.as_object_mut().unwrap().remove("reason");
                assert_eq!(cache_json(value.numeric()), expected);
            }
        }
    }
    if let Some(expected) = run["decimal_signals_after_query_exact"].as_array() {
        let mut actual = session
            .flags()
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>();
        actual.sort();
        let mut expected = expected
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(actual, expected);
    }
    if case_id == "QN03" && run.get("initial_bindings").is_some() && actual.len() == 2 {
        let a = &held[0]["x"];
        let b = &held[1]["x"];
        assert!(a == b, "equal RDF must ignore different numeric caches");
        assert_eq!(hash_bytes(a), hash_bytes(b));
        let mut caches = vec![cache_json(a.numeric()), cache_json(b.numeric())];
        for expected in case["carrier_assertions"]["caches_after_clone"]
            .as_array()
            .unwrap()
        {
            let i = caches
                .iter()
                .position(|value| value == expected)
                .expect("per-instance cache preserved");
            caches.remove(i);
        }
        assert!(caches.is_empty());
    }
    if case_id == "QN27" {
        assert!(
            dataset.trace.scans.get() > 0,
            "query bypassed the actual Dataset"
        );
        if text(run, "query").contains("\"1E+2\"") {
            assert!(
                dataset
                    .trace
                    .object_patterns
                    .borrow()
                    .iter()
                    .flatten()
                    .any(|term| matches!(term, Term::Literal(lit) if lit.value() == "1E+2"))
            );
        }
    }
}

fn first_count_expression(pattern: &GraphPattern) -> &Expression {
    match pattern {
        GraphPattern::Project { inner, .. } | GraphPattern::Extend { inner, .. } => {
            first_count_expression(inner)
        }
        GraphPattern::Group { aggregates, .. } => {
            let (
                _,
                AggregateExpression::FunctionCall {
                    name: AggregateFunction::Count,
                    expr,
                    distinct: true,
                },
            ) = &aggregates[0]
            else {
                panic!("real parsed DISTINCT COUNT")
            };
            expr
        }
        _ => panic!("unexpected parsed COUNT plan"),
    }
}
fn check_count_identity(root: &Value, case: &Value) {
    let run = &case["runs"][0];
    let session = session(root, run);
    let prepared = prepare_numeric_query(
        text(run, "query"),
        ReceivingDataset::new(run, "QN15"),
        session,
        Vec::new(),
    )
    .unwrap();
    let mut variables = Vec::new();
    let mut stats = Vec::new();
    let evaluator = prepared
        .evaluator()
        .nullable_aggregate_evaluator(
            first_count_expression(prepared.plan()),
            &mut variables,
            &mut stats,
        )
        .unwrap();
    let NullableAggregateEvaluator::RawExpression(raw) = evaluator else {
        panic!("prepared numeric COUNT must use the production numeric compiler")
    };
    let seen = Rc::new(RefCell::new(Vec::<ErrorValue>::new()));
    let observed = Rc::clone(&seen);
    let raw = Rc::new(move |row: &InternalTuple<_>| {
        let result = raw(row);
        if let Ok(SolutionValue::ReturnedError(error)) = &result {
            observed.borrow_mut().push(error.clone());
        }
        result
    });
    let mut count = NullableAccumulator::Count {
        evaluator: NullableAggregateEvaluator::RawExpression(raw),
        seen: Some(FxHashSet::default()),
        count: 0,
    };
    let input = prepared.initial().encode_for(&variables).unwrap();
    count.accumulate(&input).unwrap();
    count.accumulate(&input).unwrap();
    let NullableAccumulator::Count { count, .. } = count else {
        unreachable!()
    };
    assert_eq!(count, 2);
    let errors = seen.borrow();
    assert_eq!(errors.len(), 4);
    for (index, error) in errors.iter().enumerate() {
        assert_eq!(error.kind(), SparqlErrorKind::OtherSparql);
        assert_eq!(error.diagnostic(), "divide by 0");
        for other in &errors[..index] {
            assert!(error != other, "fresh evaluation reused an error object");
        }
    }
}
fn check_case(id: &str) {
    let root = fixture();
    let case = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    for run in case["runs"].as_array().unwrap() {
        if let Some(variants) = run["initial_binding_variants"].as_array() {
            assert!(!variants.is_empty());
            for inputs in variants {
                execute_run(&root, case, run, inputs);
            }
        } else {
            execute_run(&root, case, run, &run["initial_bindings"]);
        }
    }
    if id == "QN15" {
        check_count_identity(&root, case);
    }
    if id == "QN20" {
        let session = session(&root, &case["runs"][0]);
        let value =
            RawExpressionValue::from_import(rdf(&case["runs"][0]["initial_bindings"]["x"]["rdf"]));
        assert!(matches!(
            session.typed_ebv(&value),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnknownNumericCache
            ))
        ));
    }
}

#[test]
fn query_numeric_literal_origin_bind_project() {
    check_case("QN01");
}

#[test]
fn query_numeric_direct_string_zero_exponents_sample() {
    check_case("QN02");
}

#[test]
fn query_numeric_same_rdf_distinct_caches_and_distinct() {
    check_case("QN03");
}

#[test]
fn query_numeric_initial_provenance_bind_sample_filter() {
    check_case("QN04");
}

#[test]
fn query_numeric_single_group_key_cache() {
    check_case("QN05");
}

#[test]
fn query_numeric_big_integer_bind_sample() {
    check_case("QN06");
}

#[test]
fn query_numeric_integer_exact_vs_decimal28_multiply() {
    check_case("QN07");
}

#[test]
fn query_numeric_fractional_chain_step_rounding() {
    check_case("QN08");
}

#[test]
fn query_numeric_terminating_division_sample_scale() {
    check_case("QN09");
}

#[test]
fn query_numeric_values_bind_decimal_scale() {
    check_case("QN10");
}

#[test]
fn query_numeric_negative_zero_operations_and_ebv() {
    check_case("QN11");
}

#[test]
fn query_numeric_exact_literal_then_arithmetic_rounding() {
    check_case("QN12");
}

#[test]
fn query_numeric_all_count_finishes_known_integer() {
    check_case("QN13");
}

#[test]
fn query_numeric_aggregate_literal_fast_path_cache() {
    check_case("QN14");
}

#[test]
fn query_numeric_count_distinct_double_evaluation_error_identity() {
    check_case("QN15");
}

#[test]
fn query_numeric_numeric_ebv_special_values() {
    check_case("QN16");
}

#[test]
fn query_numeric_expression_local_trap_conversion() {
    check_case("QN17");
}

#[test]
fn query_numeric_unknown_transport_count_sample() {
    check_case("QN18");
}

#[test]
fn query_numeric_unknown_arithmetic_is_fatal() {
    check_case("QN19");
}

#[test]
fn query_numeric_unknown_ebv_is_fatal() {
    check_case("QN20");
}

#[test]
fn query_numeric_lossy_backend_identity_is_fatal() {
    check_case("QN21");
}

#[test]
fn query_numeric_dataset_failure_precedes_math_and_fuses_group() {
    check_case("QN22");
}

#[test]
fn query_numeric_fixed_output_resource_limit() {
    check_case("QN23");
}

#[test]
fn query_numeric_binary64_explicit_unsupported_boundary() {
    check_case("QN24");
}

#[test]
fn query_numeric_legacy_default_isolation() {
    check_case("QN25");
}

#[test]
fn query_numeric_parser_constructor_origin_boundaries() {
    check_case("QN26");
}

#[test]
fn query_numeric_bgp_exact_identity_and_unknown_scan() {
    check_case("QN27");
}

#[test]
fn query_numeric_zero_distinct_uses_rdf_identity() {
    check_case("QN28");
}

// These additional receiving controls are active safety contracts. They do
// not amend the independently frozen 28 families / 57 concrete inputs.
fn check_minus_fatal(shared_key: bool) {
    let root = fixture();
    let session = session(&root, &json!({}));
    let dataset = ReceivingDataset::new(&json!({}), "minus-safety");
    let query = if shared_key {
        "SELECT ?x WHERE { VALUES ?x { 1 } MINUS { VALUES ?x { 1 } BIND(?u + 1 AS ?z) } }"
    } else {
        "SELECT ?x WHERE { VALUES ?x { 1 } MINUS { BIND(?u + 1 AS ?z) } }"
    };
    let prepared = prepare_numeric_query(
        query,
        dataset.clone(),
        session,
        vec![NumericInitialBinding {
            variable: Variable::new("u").unwrap(),
            value: NumericInitialValue::UnknownNormalized(
                Literal::new_typed_literal("100", oxrdf::vocab::xsd::DECIMAL).into(),
            ),
        }],
    )
    .unwrap();
    let mut pattern = prepared.plan().clone();
    let GraphPattern::Project { inner, .. } = &mut pattern else {
        panic!("actual SELECT projection")
    };
    let GraphPattern::Minus { algorithm, .. } = inner.as_mut() else {
        panic!("actual parsed MINUS")
    };
    let MinusAlgorithm::HashBuildRightProbeLeft { keys } = algorithm;
    assert!(
        keys.is_empty(),
        "unoptimized lowering must retain its default"
    );
    if shared_key {
        // The production unoptimized lowering always chooses empty keys. Set
        // only its physical key selection to exercise the real hash branch;
        // every expression and term remains the actual prepared parser output.
        keys.push(Variable::new("x").unwrap());
    }
    let mut variables = Vec::new();
    let (plan, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(&pattern, &mut variables);
    let input = prepared.initial().encode_for(&variables).unwrap();
    let mut rows = plan.unwrap()(input).expect("legacy query starts successfully");
    let error = rows
        .next()
        .expect("MINUS must propagate the right-side fatal")
        .err()
        .expect("MINUS must not produce a successful row after a right-side fatal");
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::UnknownNumericCache)
    ));
    let internalizations = dataset.trace.internalizations.get();
    let scans = dataset.trace.scans.get();
    assert!(rows.next().is_none(), "MINUS fatal must fuse the iterator");
    assert_eq!(dataset.trace.internalizations.get(), internalizations);
    assert_eq!(dataset.trace.scans.get(), scans);
}

#[test]
fn query_numeric_minus_without_shared_keys_propagates_first_fatal() {
    check_minus_fatal(false);
}

#[test]
fn query_numeric_minus_with_shared_hash_key_propagates_first_fatal() {
    check_minus_fatal(true);
}

fn check_join_fatal_before_empty_probe(shared_key: bool) {
    let root = fixture();
    let session = session(&root, &json!({}));
    let dataset = ReceivingDataset::new(&json!({}), "join-safety");
    let query = if shared_key {
        "SELECT ?x WHERE { { VALUES ?x { 1 } BIND(?u + 1 AS ?z) } { <urn:probe-s> <urn:probe-p> ?x FILTER(false) } }"
    } else {
        "SELECT ?x WHERE { { BIND(?u + 1 AS ?x) } { <urn:probe-s> <urn:probe-p> ?probe FILTER(false) } }"
    };
    let prepared = prepare_numeric_query(
        query,
        dataset.clone(),
        session,
        vec![NumericInitialBinding {
            variable: Variable::new("u").unwrap(),
            value: NumericInitialValue::UnknownNormalized(
                Literal::new_typed_literal("100", oxrdf::vocab::xsd::DECIMAL).into(),
            ),
        }],
    )
    .unwrap();
    let mut pattern = prepared.plan().clone();
    let GraphPattern::Project { inner, .. } = &mut pattern else {
        panic!("actual SELECT projection")
    };
    let GraphPattern::Join { algorithm, .. } = inner.as_mut() else {
        panic!("actual parsed Join")
    };
    let JoinAlgorithm::HashBuildLeftProbeRight { keys } = algorithm;
    assert!(
        keys.is_empty(),
        "unoptimized lowering must retain its default"
    );
    if shared_key {
        // Select only the real hash algorithm's existing common variable.
        // Prepared expressions and RDF literals are otherwise untouched.
        keys.push(Variable::new("x").unwrap());
    }
    let mut variables = Vec::new();
    let (plan, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(&pattern, &mut variables);
    let input = prepared.initial().encode_for(&variables).unwrap();
    assert_eq!(dataset.trace.scans.get(), 0);
    let mut rows = plan.unwrap()(input).expect("legacy query starts successfully");
    let error = rows
        .next()
        .expect("empty probe must not erase a build-side fatal")
        .err()
        .expect("Join must propagate its build-side fatal first");
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::UnknownNumericCache)
    ));
    assert_eq!(
        dataset.trace.scans.get(),
        0,
        "the probe Dataset must not be started after a build-side fatal"
    );
    let internalizations = dataset.trace.internalizations.get();
    assert!(rows.next().is_none(), "Join fatal must fuse the iterator");
    assert_eq!(dataset.trace.scans.get(), 0);
    assert_eq!(dataset.trace.internalizations.get(), internalizations);
}

#[test]
fn query_numeric_cartesian_join_fatal_prevents_empty_probe_scan() {
    check_join_fatal_before_empty_probe(false);
}

#[test]
fn query_numeric_hash_join_fatal_prevents_empty_probe_scan() {
    check_join_fatal_before_empty_probe(true);
}

#[test]
fn query_numeric_generic_import_does_not_inherit_prepared_literal_cache() {
    let root = fixture();
    let session = session(&root, &json!({}));
    let dataset = ReceivingDataset::new(&json!({}), "origin-safety");
    let prepared = prepare_numeric_query(
        "SELECT ?x WHERE { VALUES ?x { \"1E+2\"^^<http://www.w3.org/2001/XMLSchema#decimal> } }",
        dataset,
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let mut variables = Vec::new();
    let (plan, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(prepared.plan(), &mut variables);
    let input = prepared.initial().encode_for(&variables).unwrap();
    let mut rows = plan.unwrap()(input).expect("legacy query starts successfully");
    let row = rows.next().unwrap().unwrap();
    assert!(rows.next().is_none());
    let parsed = row.get(0).unwrap();
    assert!(matches!(parsed.numeric(), NumericCacheState::Known(_)));
    assert_eq!(
        cache_json(parsed.numeric()),
        json!({"state":"Known", "value":{"kind":"finite", "negative":false, "coefficient":"1", "exponent":"2"}})
    );
    // Identical RDF entering the generic external-binding adapter must remain
    // Unknown. Merely possessing a prepared evaluator cannot supply provenance.
    let imported = prepared
        .evaluator()
        .dataset
        .internalize_term(parsed.rdf().clone())
        .unwrap();
    assert!(imported == *parsed);
    assert_eq!(hash_bytes(&imported), hash_bytes(parsed));
    assert!(matches!(
        imported.numeric(),
        NumericCacheState::UnknownNormalized
    ));
    let hooks = prepared.evaluator().dataset.numeric_hooks().unwrap();
    let raw = (hooks.externalize)(imported).unwrap();
    assert!(matches!(
        session.typed_ebv(&raw),
        Err(NumericEvaluationError::Fatal(
            RawNumericFault::UnknownNumericCache
        ))
    ));
}

#[test]
fn query_numeric_bound_entry_executes_its_prepared_plan() {
    let root = fixture();
    let case = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "QN08")
        .unwrap();
    let run = &case["runs"][1];
    let prepared = prepare_numeric_query(
        text(run, "query"),
        ReceivingDataset::new(run, "bound-entry"),
        session(&root, run),
        Vec::new(),
    )
    .unwrap();
    // Production execution consumes its own sealed plan, context and decoder;
    // none of the inspection-only getters supply an alternate AST here.
    let actual = prepared
        .execute_presence()
        .unwrap()
        .map(|row| row_json(row.unwrap()))
        .collect::<Vec<_>>();
    assert_rows(&actual, &run["expect"]["rows"]);
}

#[test]
fn query_numeric_bound_entry_decoder_fatal_retains_identity_and_stops() {
    let root = fixture();
    let dataset = ReceivingDataset::new(&json!({}), "QN22");
    let prepared = prepare_numeric_query(
        "SELECT ?fatal WHERE { VALUES ?row { 1 2 } }",
        dataset.clone(),
        session(&root, &json!({})),
        vec![NumericInitialBinding {
            variable: Variable::new("fatal").unwrap(),
            value: NumericInitialValue::UnknownNormalized(
                NamedNode::new("urn:query-numeric-fatal").unwrap().into(),
            ),
        }],
    )
    .unwrap();
    // No expression consumes ?fatal: only the actual presence decoder reads
    // this projected RDF term. Two source rows expose accidental continuation.
    dataset.trace.armed.set(true);
    let mut rows = prepared.execute_presence().unwrap();
    let error = rows
        .next()
        .expect("decoder must report its Dataset failure")
        .err()
        .expect("decoder must not replace an error with a successful row");
    assert_failure(&error, &json!({"semantic_category":"Dataset"}), &dataset);
    assert_eq!(dataset.trace.failing_calls.get(), 1);
    let internalizations = dataset.trace.internalizations.get();
    let scans = dataset.trace.scans.get();
    assert!(
        rows.next().is_none(),
        "decoder fatal must fuse the bound entry"
    );
    assert_eq!(dataset.trace.failing_calls.get(), 1);
    assert_eq!(dataset.trace.internalizations.get(), internalizations);
    assert_eq!(dataset.trace.scans.get(), scans);
}

#[test]
fn query_numeric_real_prepare_rejects_unqualified_and_shapes_at_compile() {
    let root = fixture();
    for query in [
        "SELECT ((true && true) && true AS ?v) WHERE {}",
        "SELECT (true && true && true AS ?v) WHERE {}",
    ] {
        let dataset = ReceivingDataset::new(&json!({}), "and-compile-boundary");
        let prepared = prepare_numeric_query(
            query,
            dataset.clone(),
            session(&root, &json!({})),
            Vec::new(),
        )
        .expect("the actual parser accepts these syntactic shapes");
        let error = prepared
            .execute_presence()
            .err()
            .expect("unqualified nested or n-ary And must fail actual compilation");
        assert!(matches!(error, QueryEvaluationError::Unexpected(_)));
        assert_eq!(error.to_string(), "unsupported nullable expression");
        assert_eq!(dataset.trace.scans.get(), 0);
        assert_eq!(dataset.trace.failing_calls.get(), 0);
    }
}

#[test]
fn query_numeric_offset_preserves_fatal_and_counts_only_rows() {
    let root = fixture();
    // CPython3.13.11 islice_next consumes the offset before checking stop,
    // including start=stop=1. LIMIT0 therefore cannot hide the first fatal.
    for query in [
        "SELECT (?x + 1 AS ?v) WHERE {} OFFSET 1",
        "SELECT (?x + 1 AS ?v) WHERE {} OFFSET 1 LIMIT 0",
    ] {
        let dataset = ReceivingDataset::new(&json!({}), "offset-safety");
        let prepared = prepare_numeric_query(
            query,
            dataset.clone(),
            session(&root, &json!({})),
            vec![NumericInitialBinding {
                variable: Variable::new("x").unwrap(),
                value: NumericInitialValue::UnknownNormalized(
                    Literal::new_typed_literal("100", oxrdf::vocab::xsd::DECIMAL).into(),
                ),
            }],
        )
        .unwrap();
        let mut rows = prepared.execute_presence().unwrap();
        let error = rows
            .next()
            .expect("OFFSET must not consume a fatal as a skipped row")
            .err()
            .expect("numeric infrastructure failure must remain fatal under OFFSET");
        assert!(matches!(
            raw_numeric_fault(&error),
            Some(RawNumericFault::UnknownNumericCache)
        ));
        let internalizations = dataset.trace.internalizations.get();
        let scans = dataset.trace.scans.get();
        assert!(rows.next().is_none());
        assert_eq!(dataset.trace.internalizations.get(), internalizations);
        assert_eq!(dataset.trace.scans.get(), scans);
    }

    let prepared = prepare_numeric_query(
        "SELECT ?x WHERE { VALUES ?x { 1 2 } } OFFSET 1",
        ReceivingDataset::new(&json!({}), "offset-ordinary-control"),
        session(&root, &json!({})),
        Vec::new(),
    )
    .unwrap();
    let actual = prepared
        .execute_presence()
        .unwrap()
        .map(|row| row_json(row.unwrap()))
        .collect::<Vec<_>>();
    assert_rows(
        &actual,
        &json!([{"x":{"kind":"literal", "lexical":"2", "datatype":"http://www.w3.org/2001/XMLSchema#integer"}}]),
    );
}

#[test]
fn query_numeric_recursive_paths_require_separate_receiving() {
    let root = fixture();
    for query in [
        "SELECT ?o WHERE { <urn:s> <urn:p>+ ?o }",
        "SELECT ?o WHERE { <urn:s> <urn:p>* ?o }",
        "SELECT ?o WHERE { <urn:s> (<urn:p>/<urn:q>*) ?o }",
    ] {
        let dataset = ReceivingDataset::new(&json!({}), "recursive-path-boundary");
        let prepared = prepare_numeric_query(
            query,
            dataset.clone(),
            session(&root, &json!({})),
            Vec::new(),
        )
        .expect("these recursive path shapes parse successfully");
        let error = prepared
            .execute_presence()
            .err()
            .expect("unreceived recursive path traversal must fail compilation");
        assert!(matches!(error, QueryEvaluationError::Unexpected(_)));
        assert_eq!(dataset.trace.scans.get(), 0);
    }
    // Existing QN27 independently exercises ordinary BGP matching and keeps
    // that accepted boundary separate from unreceived recursive traversal.
}
