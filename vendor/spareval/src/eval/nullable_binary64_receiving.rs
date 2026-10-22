//! Receiving for the immutable mixed-binary64 source/math corpus.
//! The oracle and this adapter share an author; execution is a separate receipt.
//! Every final outcome uses the bound production prepare/execute entry. Optional
//! cache and raw-consumer observations replay that same parsed production plan
//! with fresh sessions; they neither supply answers nor replace its execution.
use super::nullable::{AggregateValue, NullableAccumulator, NullableAggregateEvaluator};
use super::numeric_query::prepare_numeric_query;
use super::*;
use crate::dataset::nullable_numeric::CachedInternalTerm;
use crate::numeric::{
    NumericCacheState, NumericInitialBinding, NumericInitialValue, QueryNumericCapabilities,
    QueryNumericSession, RawNumericFault, raw_numeric_fault,
};
use crate::tuple::{ErrorValue, SparqlErrorKind};
use oxrdf::Dataset;
use semantica_numeric::{
    Binary64Fault, DecimalContext, DecimalSignal, DecimalSignals, DecimalValue, NumericFault,
    NumericLimits, NumericValue, RoundingMode,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::Hasher;

const FIXTURE: &str = include_str!("../../tests/fixtures/query_binary64_cases.json");
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        other => other.clone(),
    }
}
fn fixture() -> Value {
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(value["source_record_id"], "QUERY-BINARY64-FROZEN-01");
    assert_eq!(
        value["source_record_sha256"],
        "69974deeacb163228a361b5b3d03de93ee2fe437c1d5a95361b7d7de62ed34dd"
    );
    let mut payload = serde_json::Map::new();
    for key in [
        "capability_contract",
        "explicit_environment",
        "query_cases",
        "compatibility_controls",
    ] {
        payload.insert(key.into(), value[key].clone());
    }
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&Value::Object(payload))).unwrap())
        ),
        "5835a77c9bccf887eca76aa767ca9ff13070fe625336f25af4c770ff2c047b1d"
    );
    assert_eq!(value["query_cases"].as_array().unwrap().len(), 21);
    assert_eq!(
        value["query_cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["runs"].as_array().unwrap().len())
            .sum::<usize>(),
        54
    );
    value
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing string {key}: {value}"))
}
fn rdf(value: &Value) -> Term {
    match text(value, "kind") {
        "literal" => Literal::new_typed_literal(
            text(value, "lexical"),
            NamedNode::new(text(value, "datatype")).unwrap(),
        )
        .into(),
        "named_node" => NamedNode::new(text(value, "iri")).unwrap().into(),
        kind => panic!("unsupported fixture RDF kind {kind}"),
    }
}
fn rdf_json(value: &Term) -> Value {
    match value {
        Term::Literal(lit) => {
            json!({"kind":"literal","lexical":lit.value(),"datatype":lit.datatype().as_str()})
        }
        Term::NamedNode(node) => json!({"kind":"named_node","iri":node.as_str()}),
        _ => panic!("unexpected output RDF family"),
    }
}
fn decimal(value: &Value) -> DecimalValue {
    match text(value, "kind") {
        "finite" => DecimalValue::Finite {
            negative: value["negative"].as_bool().unwrap(),
            coefficient: text(value, "coefficient").parse().unwrap(),
            exponent: text(value, "exponent").parse().unwrap(),
        },
        "infinity" => DecimalValue::Infinity {
            negative: value["negative"].as_bool().unwrap(),
        },
        "nan" => DecimalValue::NaN {
            negative: value["negative"].as_bool().unwrap(),
            signaling: value["signaling"].as_bool().unwrap(),
            payload: text(value, "payload").into(),
        },
        kind => panic!("unsupported Decimal {kind}"),
    }
}
fn numeric_input(value: &Value) -> NumericValue {
    match text(value, "family") {
        "Integer" => NumericValue::Integer(text(value, "value").parse().unwrap()),
        "Decimal" => NumericValue::Decimal(decimal(&value["value"])),
        "Binary64" => NumericValue::Binary64(f64::from_bits(
            u64::from_str_radix(text(value, "bits_hex"), 16).unwrap(),
        )),
        family => panic!("unsupported numeric input {family}"),
    }
}
fn numeric_json(value: &NumericValue) -> Value {
    match value {
        NumericValue::Integer(n) => json!({"family":"Integer","value":n.to_string()}),
        NumericValue::Binary64(n) => {
            json!({"family":"Binary64","bits_hex":format!("{:016x}",n.to_bits())})
        }
        NumericValue::Decimal(d) => {
            let repr = match d {
                DecimalValue::Finite {
                    negative,
                    coefficient,
                    exponent,
                } => {
                    json!({"kind":"finite","negative":negative,"coefficient":coefficient.to_string(),"exponent":exponent.to_string()})
                }
                DecimalValue::Infinity { negative } => {
                    json!({"kind":"infinity","negative":negative})
                }
                DecimalValue::NaN {
                    negative,
                    signaling,
                    payload,
                } => {
                    json!({"kind":"nan","negative":negative,"signaling":signaling,"payload":payload})
                }
            };
            json!({"family":"Decimal","value":repr})
        }
    }
}
fn cache_json(cache: &NumericCacheState) -> Value {
    match cache {
        NumericCacheState::Known(c) => json!({"state":"Known","value":numeric_json(c.value())}),
        NumericCacheState::UnknownNormalized => json!({"state":"Unknown"}),
        NumericCacheState::NotNumeric => json!({"state":"NotNumeric"}),
        NumericCacheState::CastFailed => json!({"state":"CastFailed"}),
        NumericCacheState::ErrorObject(_) => panic!("binary64 receiving does not construct Literal(error)"),
    }
}
fn assert_cache(actual: &NumericCacheState, expected: &Value) {
    if expected["state"] == "Known" && expected["value"]["classification"] == "NaN" {
        let NumericCacheState::Known(cache) = actual else {
            panic!("expected Known NaN")
        };
        let NumericValue::Binary64(value) = cache.value() else {
            panic!("expected binary64 NaN")
        };
        assert!(value.is_nan()); // Arithmetic NaN sign/payload deliberately unspecified.
    } else {
        assert_eq!(cache_json(actual), *expected);
    }
}
fn signal(name: &str) -> DecimalSignal {
    DecimalSignal::ALL
        .into_iter()
        .find(|s| format!("{s:?}") == name)
        .unwrap()
}
fn session(root: &Value, run: &Value, use_original_default: bool) -> Rc<QueryNumericSession> {
    let env = &root["explicit_environment"];
    let mut ctx = env["decimal_context"].clone();
    if let Some(fields) = run["context_override"].as_object() {
        for (key, value) in fields {
            ctx[key] = value.clone();
        }
    }
    assert_eq!(ctx["rounding"], "HALF_EVEN");
    assert_eq!(ctx["initial_flags"], json!([]));
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
    let limit = |key: &str| {
        run["numeric_limits_override"]
            .get(key)
            .unwrap_or(&env["numeric_limits"][key])
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap()
    };
    let limits = NumericLimits {
        max_input_characters: limit("max_input_characters"),
        max_coefficient_digits: limit("max_coefficient_digits"),
        max_output_characters: limit("max_output_characters"),
        max_materialized_power_of_ten: limit("max_materialized_power_of_ten"),
    };
    let integer_limit = Some(
        env["integer_text_limit"]
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap(),
    );
    let normalize = env["normalize_literals"].as_bool().unwrap();
    let capability = match text(run, "capability") {
        "integer-decimal-v1" => QueryNumericCapabilities::IntegerDecimalV1,
        "mixed-binary64-v1" => QueryNumericCapabilities::MixedBinary64V1,
        other => panic!("unknown frozen capability {other}"),
    };
    let result = if use_original_default {
        assert_eq!(capability, QueryNumericCapabilities::IntegerDecimalV1);
        QueryNumericSession::new(decimal, limits, integer_limit, normalize)
    } else {
        QueryNumericSession::new_with_capabilities(
            decimal,
            limits,
            integer_limit,
            normalize,
            capability,
        )
    }
    .unwrap();
    assert_eq!(result.capabilities(), capability);
    Rc::new(result)
}
fn assert_flags(session: &QueryNumericSession, expected: &Value) {
    let wanted: DecimalSignals = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|v| signal(v.as_str().unwrap()))
        .collect();
    assert_eq!(
        session.flags(),
        wanted,
        "query's exact accumulated Decimal signals"
    );
}
fn initial(run: &Value) -> Vec<NumericInitialBinding> {
    let mut result = Vec::new();
    if let Some(map) = run["initial_bindings"].as_object() {
        for (name, binding) in map {
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
                other => panic!("unknown initial origin {other}"),
            };
            result.push(NumericInitialBinding {
                variable: Variable::new(name).unwrap(),
                value,
            });
        }
    }
    if let Some(map) = run["dataset_fixture"]["initial_binding"].as_object() {
        for (name, value) in map {
            result.push(NumericInitialBinding {
                variable: Variable::new(name).unwrap(),
                value: NumericInitialValue::UnknownNormalized(rdf(value)),
            });
        }
    }
    result
}

#[derive(Debug)]
struct FailureIdentity;
#[derive(Clone, Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("binary64 receiving Dataset conversion failure")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct Trace {
    armed: Cell<bool>,
    failing: Cell<usize>,
    externalizations: Cell<usize>,
    internalizations: Cell<usize>,
    scans: Cell<usize>,
}
impl Trace {
    fn counts(&self) -> (usize, usize, usize, usize) {
        (
            self.failing.get(),
            self.externalizations.get(),
            self.internalizations.get(),
            self.scans.get(),
        )
    }
}
#[derive(Clone)]
struct ReceivingDataset {
    graph: Rc<Dataset>,
    trace: Rc<Trace>,
    identity: Arc<FailureIdentity>,
    failing: bool,
}
impl ReceivingDataset {
    fn new(run: &Value) -> Self {
        Self {
            graph: Rc::new(Dataset::new()),
            trace: Rc::new(Trace::default()),
            identity: Arc::new(FailureIdentity),
            failing: run.get("dataset_fixture").is_some(),
        }
    }
    fn arm(&self) {
        self.trace.failing.set(0);
        self.trace.armed.set(true);
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
        let rows = self
            .graph
            .iter()
            .filter_map(|q| {
                if matches!(graph_name, Some(Some(_))) {
                    return None;
                }
                let row = InternalQuad {
                    subject: Term::from(q.subject.into_owned()),
                    predicate: Term::from(q.predicate.into_owned()),
                    object: q.object.into_owned(),
                    graph_name: None,
                };
                if subject.is_some_and(|v| v != &row.subject)
                    || predicate.is_some_and(|v| v != &row.predicate)
                    || object.is_some_and(|v| v != &row.object)
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
        self.trace
            .externalizations
            .set(self.trace.externalizations.get() + 1);
        if self.failing
            && self.trace.armed.get()
            && matches!(&term,Term::NamedNode(n) if n.as_str()=="urn:query-numeric-fatal")
        {
            self.trace.failing.set(self.trace.failing.get() + 1);
            Err(ConversionFailure(Arc::clone(&self.identity)))
        } else {
            Ok(term)
        }
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
    let mut h = HashBytes::default();
    value.hash(&mut h);
    h.0
}
fn row_json(row: PresenceSolution) -> Value {
    let mut result = serde_json::Map::new();
    for (variable, value) in row.variables.iter().zip(row.values) {
        match value {
            Some(SolutionValue::Rdf(term)) => {
                result.insert(variable.as_str().into(), rdf_json(&term));
            }
            None => {}
            _ => panic!("unexpected non-RDF final SELECT binding"),
        }
    }
    Value::Object(result)
}
fn assert_rows(actual: &[Value], expected: &Value) {
    let mut remaining = expected.as_array().unwrap().clone();
    for row in actual {
        let pos = remaining
            .iter()
            .position(|r| r == row)
            .unwrap_or_else(|| panic!("unexpected row {row}; remaining {remaining:?}"));
        remaining.remove(pos);
    }
    assert!(remaining.is_empty(), "missing rows {remaining:?}");
}
fn assert_failure(error: &QueryEvaluationError, expected: &Value, dataset: &ReceivingDataset) {
    let category = text(expected, "semantic_category");
    if category == "Dataset" {
        let QueryEvaluationError::Dataset(error) = error else {
            panic!("expected original Dataset failure, got {error}")
        };
        let error = error
            .downcast_ref::<ConversionFailure>()
            .expect("original backend error type");
        assert!(Arc::ptr_eq(&error.0, &dataset.identity));
        return;
    }
    let fault = raw_numeric_fault(error).expect("original typed RawNumericFault");
    match (category, fault) {
        ("MixedNumericTypeError", RawNumericFault::MixedNumericTypeError)
        | ("UnknownNumericCache", RawNumericFault::UnknownNumericCache)
        | ("UnsupportedNumericFamily", RawNumericFault::UnsupportedNumericFamily)
        | ("Binary64.IntegerOverflow", RawNumericFault::Binary64(Binary64Fault::IntegerOverflow))
        | (
            "Binary64.SignalingNaNConversion",
            RawNumericFault::Binary64(Binary64Fault::SignalingNaNConversion),
        ) => {}
        (
            "Core.DecimalTrap",
            RawNumericFault::Core(NumericFault::DecimalTrap { primary, signals }),
        ) => {
            assert_eq!(format!("{primary:?}"), text(expected, "primary_signal"));
            for name in expected["required_signals"].as_array().unwrap() {
                assert!(signals.contains(signal(name.as_str().unwrap())));
            }
        }
        (
            "Core.ResourceLimit",
            RawNumericFault::Core(NumericFault::ResourceLimit { operation }),
        ) => {
            assert_eq!(*operation, text(expected, "operation"));
        }
        _ => panic!("expected {category}, got {fault:?}"),
    }
}

// All 54 runs, plus both old controls, reach this actual bound entry.
fn receive_bound(root: &Value, run: &Value, old_default: bool) {
    let session = session(root, run, old_default);
    let dataset = ReceivingDataset::new(run);
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset.clone(),
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    dataset.arm(); // BEFORE execute_presence: Group may eagerly consume its child.
    let mut rows = prepared
        .execute_presence()
        .expect("compile/bind must succeed before the frozen runtime outcome");
    if run["expect"]["kind"] == "fatal" {
        let Err(error) = rows.next().expect("first poll must report fatal") else {
            panic!("fatal must not become a successful row")
        };
        assert_failure(&error, &run["expect"], &dataset);
        if let Some(n) = run["expect"]["failing_externalize_calls"].as_u64() {
            assert_eq!(dataset.trace.failing.get(), n as usize);
        }
        let counts = dataset.trace.counts();
        assert!(rows.next().is_none(), "fatal must fuse");
        assert_eq!(
            dataset.trace.counts(),
            counts,
            "fused poll performed more Dataset work"
        );
        if let Some(n) = run["expect"]["calls_after_second_poll"].as_u64() {
            assert_eq!(dataset.trace.failing.get(), n as usize);
        }
    } else {
        assert_eq!(run["expect"]["kind"], "solutions");
        let actual = rows
            .by_ref()
            .map(|r| row_json(r.unwrap()))
            .collect::<Vec<_>>();
        assert_rows(&actual, &run["expect"]["rows"]);
        assert!(rows.next().is_none());
        if let Some(n) = run["failing_externalize_calls"].as_u64() {
            assert_eq!(dataset.trace.failing.get(), n as usize);
        }
    }
    assert_flags(&session, &run["decimal_signals_after_query_exact"]);
}

// Separate observation replay. No AST is constructed or changed here.
fn receive_cache_replay(root: &Value, case: &Value, run: &Value) {
    let checkpoints = run["checkpoints"].as_array();
    let is_decoder = run["dataset_fixture"]["required_failure_boundary"].is_string();
    if checkpoints.is_none() && case.get("carrier_assertions").is_none() && !is_decoder {
        return;
    }
    let session = session(root, run, false);
    let dataset = ReceivingDataset::new(run);
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset.clone(),
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    if case.get("carrier_assertions").is_some() {
        let names = vec![
            Variable::new("left").unwrap(),
            Variable::new("right").unwrap(),
        ];
        let calls = dataset.trace.internalizations.get();
        let input = prepared.initial().encode_for(&names).unwrap();
        assert_eq!(
            dataset.trace.internalizations.get(),
            calls,
            "encode clones prepared values"
        );
        let (Some(SolutionValue::Rdf(left)), Some(SolutionValue::Rdf(right))) =
            (input.cell(0), input.cell(1))
        else {
            panic!("both actual initial cells")
        };
        assert_eq!(left.rdf(), right.rdf());
        assert!(left == right);
        assert_eq!(hash_bytes(left), hash_bytes(right));
        assert_ne!(cache_json(left.numeric()), cache_json(right.numeric()));
        for value in [left, right] {
            assert_eq!(
                cache_json(value.numeric()),
                cache_json(value.clone().numeric())
            );
            assert_eq!(hash_bytes(value), hash_bytes(value.rdf()));
        }
    }
    let mut variables = Vec::new();
    let (plan, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(prepared.plan(), &mut variables);
    let input = prepared.initial().encode_for(&variables).unwrap();
    dataset.arm(); // Before plan(input), including eager Group.
    let mut rows = plan.unwrap()(input).expect("legacy query starts successfully");
    let mut observed = Vec::<BTreeMap<String, CachedInternalTerm<Term>>>::new();
    while let Some(row) = rows.next() {
        let tuple = row.expect("cache checkpoints require successful internal tuples");
        let mut map = BTreeMap::new();
        for (index, name) in variables.iter().enumerate() {
            if let Some(SolutionValue::Rdf(cell)) = tuple.cell(index) {
                assert_eq!(hash_bytes(cell), hash_bytes(cell.rdf()));
                assert_eq!(
                    cache_json(cell.numeric()),
                    cache_json(cell.clone().numeric())
                );
                if let NumericCacheState::Known(cache) = cell.numeric() {
                    let Term::Literal(literal) = cell.rdf() else {
                        panic!("Known cache requires real RDF literal")
                    };
                    assert_eq!(cache.datatype().as_str(), literal.datatype().as_str());
                }
                map.insert(name.as_str().into(), cell.clone());
            }
        }
        observed.push(map);
    }
    if let Some(checkpoints) = checkpoints {
        for checkpoint in checkpoints {
            let selected = observed
                .iter()
                .filter(|row| {
                    checkpoint["row_selector"]
                        .as_object()
                        .is_none_or(|selector| {
                            selector.iter().all(|(name, value)| {
                                row.get(name)
                                    .is_some_and(|cell| rdf_json(cell.rdf()) == *value)
                            })
                        })
                })
                .collect::<Vec<_>>();
            assert!(!selected.is_empty(), "checkpoint did not observe a row");
            for row in selected {
                assert_cache(
                    row.get(text(checkpoint, "binding"))
                        .expect("checkpoint binding")
                        .numeric(),
                    &checkpoint["cache"],
                );
            }
        }
    }
    if is_decoder {
        assert_eq!(
            observed.len(),
            1,
            "BIND succeeds before final decoder fails"
        );
        assert!(matches!(
            observed[0]["v"].numeric(),
            NumericCacheState::Known(_)
        ));
        assert!(matches!(observed[0]["fatal"].rdf(), Term::NamedNode(_)));
        assert_eq!(
            dataset.trace.failing.get(),
            0,
            "failure must be deferred until final decoder"
        );
    }
    assert_flags(&session, &run["decimal_signals_after_query_exact"]);
}

// Extract references from the actual production-lowered plan. Never reconstruct
// an expression from fixture text or synthesize rows to satisfy a call count.
enum Consumer<'a> {
    Aggregate {
        inner: &'a GraphPattern,
        expr: &'a Expression,
        sample: bool,
        distinct: bool,
    },
    Extend {
        inner: &'a GraphPattern,
        expr: &'a Expression,
    },
}
fn group_consumer(pattern: &GraphPattern) -> Option<Consumer<'_>> {
    match pattern {
        GraphPattern::Project { inner, .. } | GraphPattern::Extend { inner, .. } => {
            group_consumer(inner)
        }
        GraphPattern::Group {
            inner, aggregates, ..
        } => {
            assert_eq!(
                aggregates.len(),
                1,
                "instrumented frozen aggregate has one consumer"
            );
            let (
                _,
                AggregateExpression::FunctionCall {
                    name,
                    expr,
                    distinct,
                },
            ) = &aggregates[0]
            else {
                panic!("function aggregate")
            };
            let sample = match name {
                AggregateFunction::Count => false,
                AggregateFunction::Sample => true,
                _ => panic!("unexpected observed aggregate"),
            };
            Some(Consumer::Aggregate {
                inner,
                expr,
                sample,
                distinct: *distinct,
            })
        }
        _ => None,
    }
}
fn raw_consumer(pattern: &GraphPattern) -> Consumer<'_> {
    if let Some(group) = group_consumer(pattern) {
        return group;
    }
    match pattern {
        GraphPattern::Project { inner, .. } => raw_consumer(inner),
        GraphPattern::Extend {
            inner, expression, ..
        } => Consumer::Extend {
            inner,
            expr: expression,
        },
        _ => panic!("unexpected frozen raw observation plan"),
    }
}
fn assert_error_value(error: &ErrorValue, expected: &Value) {
    let name = expected
        .get("error_kind")
        .or_else(|| expected.get("kind"))
        .unwrap()
        .as_str()
        .unwrap();
    let wanted = match name {
        "OtherSparql" => SparqlErrorKind::OtherSparql,
        "NotBound" => SparqlErrorKind::NotBound,
        _ => panic!("unknown error kind {name}"),
    };
    assert_eq!(error.kind(), wanted);
    if let Some(diagnostic) = expected["diagnostic"].as_str() {
        assert_eq!(error.diagnostic(), diagnostic);
    }
}
fn receive_raw_replay(root: &Value, run: &Value) {
    if run.get("raw_expression_evaluations").is_none() && run.get("raw_expression_result").is_none()
    {
        return;
    }
    let session = session(root, run, false);
    let dataset = ReceivingDataset::new(run);
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset.clone(),
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    let consumer = raw_consumer(prepared.plan());
    let (inner, expr) = match &consumer {
        Consumer::Aggregate { inner, expr, .. } | Consumer::Extend { inner, expr } => {
            (*inner, *expr)
        }
    };
    let mut variables = Vec::new();
    let (plan, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(inner, &mut variables);
    let raw = prepared
        .evaluator()
        .nullable_aggregate_evaluator(expr, &mut variables, &mut Vec::new())
        .unwrap();
    let NullableAggregateEvaluator::RawExpression(raw) = raw else {
        panic!("must use actual numeric expression compiler")
    };
    let calls = Rc::new(Cell::new(0_usize));
    let errors = Rc::new(RefCell::new(Vec::<ErrorValue>::new()));
    let seen_calls = Rc::clone(&calls);
    let seen_errors = Rc::clone(&errors);
    let raw = Rc::new(move |row: &InternalTuple<CachedInternalTerm<Term>>| {
        seen_calls.set(seen_calls.get() + 1);
        let result = raw(row);
        if let Ok(SolutionValue::ReturnedError(error)) = &result {
            seen_errors.borrow_mut().push(error.clone());
        }
        result
    });
    let mut accumulator = match consumer {
        Consumer::Aggregate { sample: true, .. } => Some(NullableAccumulator::Sample {
            evaluator: NullableAggregateEvaluator::RawExpression(raw.clone()),
            selected: None,
        }),
        Consumer::Aggregate { distinct, .. } => Some(NullableAccumulator::Count {
            evaluator: NullableAggregateEvaluator::RawExpression(raw.clone()),
            seen: distinct.then(FxHashSet::default),
            count: 0,
        }),
        Consumer::Extend { .. } => None,
    };
    let input = prepared.initial().encode_for(&variables).unwrap();
    dataset.arm();
    let mut failed = false;
    for row in plan.unwrap()(input).expect("legacy query starts successfully") {
        let row = row.unwrap();
        if let Some(acc) = &mut accumulator {
            if let Err(error) = acc.accumulate(&row) {
                assert_failure(&error, &run["expect"], &dataset);
                failed = true;
                break;
            }
        } else {
            assert!(
                matches!(raw(&row), Ok(SolutionValue::ReturnedError(_))),
                "actual raw expression returns error object"
            );
        }
    }
    assert_eq!(failed, run["expect"]["kind"] == "fatal");
    if let Some(expected) = run["raw_expression_evaluations"].as_u64() {
        assert_eq!(
            calls.get(),
            expected as usize,
            "actual consumer evaluation count"
        );
    }
    let errors = errors.borrow();
    if let Some(expected) = run.get("raw_expression_result") {
        assert!(!errors.is_empty());
        for error in errors.iter() {
            assert_error_value(error, expected);
        }
    }
    if let Some(expected) = run.get("returned_error_identity_assertions") {
        assert_eq!(
            errors.len(),
            expected["object_count"].as_u64().unwrap() as usize
        );
        for (index, error) in errors.iter().enumerate() {
            assert_error_value(error, expected);
            for earlier in &errors[..index] {
                assert!(
                    error != earlier,
                    "separate evaluations reused error identity"
                );
            }
        }
    }
    if let Some(expected) = run.get("aggregate_internal_result") {
        let AggregateValue::RawExpression(SolutionValue::ReturnedError(selected)) =
            accumulator.unwrap().finish().unwrap().unwrap()
        else {
            panic!("SAMPLE must retain the selected returned error before alias Extend")
        };
        assert_error_value(&selected, expected);
        assert_eq!(errors.len(), 1);
        assert!(selected == errors[0]);
    }
    assert_flags(&session, &run["decimal_signals_after_query_exact"]);
    if let Some(n) = run["failing_externalize_calls"].as_u64() {
        assert_eq!(dataset.trace.failing.get(), n as usize);
    }
}
fn run_family(id: &str) {
    let root = fixture();
    let case = root["query_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap();
    for (index, run) in case["runs"].as_array().unwrap().iter().enumerate() {
        // A panic label identifies the immutable input without altering it.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            receive_bound(&root, run, false);
            receive_cache_replay(&root, case, run);
            receive_raw_replay(&root, run);
        }));
        if let Err(error) = result {
            eprintln!("frozen {id} run {index}: {}", text(run, "query"));
            std::panic::resume_unwind(error);
        }
    }
}
#[test]
fn query_binary64_fixture_identity_and_original_control_bytes() {
    let root = fixture();
    let old: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/query_numeric_cases.json"
    ))
    .unwrap();
    assert_eq!(
        old["source_record_sha256"],
        "2a63d9a7043935df5c925b5ed5285d4447a24c9f0d09a532432fd8c2442cf5fd"
    );
    for (index, control) in root["compatibility_controls"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            control["unchanged_frozen_run"],
            old["cases"][23]["runs"][index]
        );
    }
}
#[test]
fn query_binary64_original_qn24_rejections_remain_active() {
    let root = fixture();
    for control in root["compatibility_controls"].as_array().unwrap() {
        let mut run = control["unchanged_frozen_run"].clone();
        run["capability"] = control["capability"].clone();
        run["decimal_signals_after_query_exact"] =
            control["additional_decimal_signals_after_query_exact"].clone();
        receive_bound(&root, &run, true);
        receive_bound(&root, &run, false);
    }
}

#[test]
fn query_binary64_constructor_signal_survives_resource_failure() {
    // A separately frozen supplement: the original 42/54 corpus is unchanged.
    // It combines a source-reached FloatOperation with an active resource
    // rejection. No-conversion under a trap is checked by error precedence,
    // not by inventing a core conversion counter.
    let supplement: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/query_binary64_signal_resource.json"
    ))
    .unwrap();
    assert_eq!(
        supplement["source_record_sha256"],
        "98ee2a3aed96c57f8aa44f7e67c6b1ca48d049ebfc993d84e257e3e66c864da4"
    );
    let payload = json!({"environment":supplement["environment"],"cases":supplement["cases"]});
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "577e1a7fef32db8e1c396fb2d19a54e0117798fd88885e18b1ac6ec8d57bf7fe"
    );
    let env = &supplement["environment"];
    let ctx = &env["decimal_context"];
    assert_eq!(ctx["rounding"], "HalfEven");
    assert_eq!(ctx["flags_initial"], json!([]));
    assert_eq!(ctx["ignored_flags"], json!([]));
    assert_eq!(env["initial_bindings"], json!([]));
    let cases = supplement["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let mut failures = Vec::new();
    for case in cases {
        let root = json!({"explicit_environment": {
            "decimal_context": {
                "precision":ctx["precision"],"rounding":"HALF_EVEN",
                "Emin":text(ctx,"emin").parse::<i64>().unwrap(),
                "Emax":text(ctx,"emax").parse::<i64>().unwrap(),
                "clamp":u8::from(ctx["clamp"].as_bool().unwrap()),
                "traps":case["traps"],"initial_flags":ctx["flags_initial"]
            },
            "numeric_limits":env["numeric_limits"],
            "integer_text_limit":env["integer_text_limit"],
            "normalize_literals":env["normalize_literals"]
        }});
        let mut expected = case["expected"].clone();
        expected["semantic_category"] = expected["reason"].clone();
        let run = json!({"query":env["query"],"capability":case["capability"],
            "expect":expected,"decimal_signals_after_query_exact":case["expected"]["session_flags_exact"]});
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            receive_bound(&root, &run, false)
        }))
        .is_err()
        {
            failures.push(text(case, "id").to_owned());
        }
    }
    assert!(failures.is_empty(), "supplement cases failed: {failures:?}");
}

#[test]
fn query_binary64_bq01() {
    run_family("BQ01");
}

#[test]
fn query_binary64_bq02() {
    run_family("BQ02");
}

#[test]
fn query_binary64_bq03() {
    run_family("BQ03");
}

#[test]
fn query_binary64_bq04() {
    run_family("BQ04");
}

#[test]
fn query_binary64_bq05() {
    run_family("BQ05");
}

#[test]
fn query_binary64_bq06() {
    run_family("BQ06");
}

#[test]
fn query_binary64_bq07() {
    run_family("BQ07");
}

#[test]
fn query_binary64_bq08() {
    run_family("BQ08");
}

#[test]
fn query_binary64_bq09() {
    run_family("BQ09");
}

#[test]
fn query_binary64_bq10() {
    run_family("BQ10");
}

#[test]
fn query_binary64_bq11() {
    run_family("BQ11");
}

#[test]
fn query_binary64_bq12() {
    run_family("BQ12");
}

#[test]
fn query_binary64_bq13() {
    run_family("BQ13");
}

#[test]
fn query_binary64_bq14() {
    run_family("BQ14");
}

#[test]
fn query_binary64_bq15() {
    run_family("BQ15");
}

#[test]
fn query_binary64_bq16() {
    run_family("BQ16");
}

#[test]
fn query_binary64_bq17() {
    run_family("BQ17");
}

#[test]
fn query_binary64_bq18() {
    run_family("BQ18");
}

#[test]
fn query_binary64_bq19() {
    run_family("BQ19");
}

#[test]
fn query_binary64_bq20() {
    run_family("BQ20");
}

#[test]
fn query_binary64_bq21() {
    run_family("BQ21");
}
