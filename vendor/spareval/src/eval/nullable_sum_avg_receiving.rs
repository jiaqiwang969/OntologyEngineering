//! Receiving for the immutable SUM/AVG source/math corpus.
//! The oracle and this adapter share an author; execution is a separate receipt.
//! Every final outcome uses the bound production prepare/execute entry. Optional
//! cache and raw-consumer observations replay that same parsed production plan
//! with fresh sessions; they neither supply answers nor replace its execution.
use super::nullable::{
    AggregateValue, NullableAccumulator, NullableAggregateError, NullableAggregateEvaluator,
    nullable_aggregate_error,
};
use super::numeric_query::prepare_numeric_query;
use super::*;
use crate::dataset::nullable_numeric::CachedInternalTerm;
use crate::numeric::{
    NumericCacheState, NumericInitialBinding, NumericInitialValue, QueryGroupSemantics,
    QueryNumericCapabilities, QueryNumericSession, RawNumericFault, raw_numeric_fault,
};
use crate::tuple::{ErrorValue, SparqlErrorKind};
use oxrdf::Dataset;
use semantica_numeric::{
    DecimalContext, DecimalSignal, DecimalSignals, DecimalValue, NumericFault, NumericLimits,
    NumericValue, RoundingMode,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::Hasher;

const FIXTURE: &str = include_str!("fixtures/query_sum_avg_cases.json");
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
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(root["source_record_id"], "QUERY-SUM-AVG-FROZEN-03");
    assert_eq!(
        root["source_record_sha256"],
        "f86864839e50f1902e18ee89a9315377ead0577ab3065fdc6407d73f25246566"
    );
    let payload = json!({"environment":root["environment"],"cases":root["cases"],"compatibility_controls":root["compatibility_controls"]});
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "274ebab2456dbd1fffb0543369ebb1f26b3159068b2d5d697353019aa5ef3181"
    );
    assert_eq!(root["cases"].as_array().unwrap().len(), 12);
    assert_eq!(
        root["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["runs"].as_array().unwrap().len())
            .sum::<usize>(),
        57
    );
    root
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
        NumericCacheState::ErrorObject(_) => panic!("SUM/AVG receiving does not construct Literal(error)"),
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
    let env = &root["environment"];
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
    let capability = match text(run, "numeric_capability") {
        "integer-decimal-v1" => QueryNumericCapabilities::IntegerDecimalV1,
        "mixed-binary64-v1" => QueryNumericCapabilities::MixedBinary64V1,
        other => panic!("unknown frozen capability {other}"),
    };
    let result = if use_original_default {
        assert_eq!(capability, QueryNumericCapabilities::IntegerDecimalV1);
        QueryNumericSession::new(decimal, limits, integer_limit, normalize)
    } else if run["group_semantics"] == "source-group/1" {
        QueryNumericSession::new_with_group_semantics(
            decimal,
            limits,
            integer_limit,
            normalize,
            capability,
            QueryGroupSemantics::SourceAggregateV1,
        )
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
    assert_eq!(
        result.group_semantics(),
        if run["group_semantics"] == "source-group/1" {
            QueryGroupSemantics::SourceAggregateV1
        } else {
            QueryGroupSemantics::LegacyPrivateV1
        }
    );
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
        f.write_str("SUM/AVG receiving Dataset conversion failure")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct Trace {
    armed: Cell<bool>,
    failing: Cell<usize>,
    target_calls: Cell<usize>,
    externalizations: Cell<usize>,
    internalizations: Cell<usize>,
    scans: Cell<usize>,
}
impl Trace {
    fn counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.failing.get(),
            self.target_calls.get(),
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
    fault: Option<(String, Term, usize)>,
}
impl ReceivingDataset {
    fn new(run: &Value) -> Self {
        let fault = run.get("dataset_fixture").map(|spec| {
            (
                text(spec, "operation").to_owned(),
                rdf(&spec["target_rdf"]),
                spec["fail_target_call"].as_u64().unwrap() as usize,
            )
        });
        Self {
            graph: Rc::new(Dataset::new()),
            trace: Rc::new(Trace::default()),
            identity: Arc::new(FailureIdentity),
            fault,
        }
    }
    fn arm(&self) {
        self.trace.failing.set(0);
        self.trace.target_calls.set(0);
        self.trace.armed.set(true);
    }
    fn conversion(&self, operation: &str, term: Term) -> Result<Term, ConversionFailure> {
        if let Some((wanted, target, ordinal)) = &self.fault {
            if self.trace.armed.get() && wanted == operation && target == &term {
                let count = self.trace.target_calls.get() + 1;
                self.trace.target_calls.set(count);
                if count == *ordinal {
                    self.trace.failing.set(self.trace.failing.get() + 1);
                    return Err(ConversionFailure(Arc::clone(&self.identity)));
                }
            }
        }
        Ok(term)
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
        self.conversion("internalize_term", term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.trace
            .externalizations
            .set(self.trace.externalizations.get() + 1);
        self.conversion("externalize_term", term)
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
    let mut out = serde_json::Map::new();
    for (variable, value) in row.variables.iter().zip(row.values) {
        match value {
            None => {}
            Some(SolutionValue::Rdf(term)) => {
                out.insert(variable.as_str().into(), rdf_json(&term));
            }
            Some(SolutionValue::ExplicitNone) => {
                out.insert(variable.as_str().into(), json!({"kind":"explicit_none"}));
            }
            Some(SolutionValue::ReturnedError(_)) => {
                panic!("returned error must have passed through actual alias Extend")
            }
        }
    }
    Value::Object(out)
}
fn assert_error_value(error: &ErrorValue, kind: &str) {
    let wanted = match kind {
        "NotBound" => SparqlErrorKind::NotBound,
        "Type" => SparqlErrorKind::Type,
        "OtherSparql" => SparqlErrorKind::OtherSparql,
        other => panic!("unexpected error kind {other}"),
    };
    assert_eq!(error.kind(), wanted);
}
fn assert_failure(error: &QueryEvaluationError, run: &Value, dataset: &ReceivingDataset) {
    let category = text(&run["expect"], "semantic_category");
    if category == "Dataset" {
        let QueryEvaluationError::Dataset(error) = error else {
            panic!("expected Dataset failure, got {error}")
        };
        let error = error
            .downcast_ref::<ConversionFailure>()
            .expect("original Dataset error type");
        assert!(Arc::ptr_eq(&error.0, &dataset.identity));
        return;
    }
    if category == "UncaughtSparql" {
        let Some(NullableAggregateError::UncaughtSparql(error)) = nullable_aggregate_error(error)
        else {
            panic!("uncaught SPARQL must preserve typed error")
        };
        assert_error_value(error, text(run, "source_error_kind"));
        return;
    }
    let fault = raw_numeric_fault(error).expect("original typed numeric fault");
    match (category, fault) {
        ("AggregateDatatypeAccess", RawNumericFault::AggregateDatatypeAccess)
        | ("AggregateTypePromotion", RawNumericFault::AggregateTypePromotion)
        | ("UnknownNumericCache", RawNumericFault::UnknownNumericCache)
        | ("MixedNumericTypeError", RawNumericFault::MixedNumericTypeError)
        | ("UnsupportedNumericFamily", RawNumericFault::UnsupportedNumericFamily) => {}
        (
            "Core.DecimalTrap",
            RawNumericFault::Core(NumericFault::DecimalTrap { primary, signals }),
        ) => {
            assert_eq!(format!("{primary:?}"), text(run, "primary_signal"));
            for s in run["required_signals"].as_array().unwrap() {
                assert!(signals.contains(signal(s.as_str().unwrap())));
            }
        }
        (
            "Core.ResourceLimit",
            RawNumericFault::Core(NumericFault::ResourceLimit { operation }),
        ) => assert_eq!(*operation, text(run, "operation")),
        _ => panic!("wanted {category}, got {fault:?}"),
    }
}

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
    if let Some(expected) = run.get("decimal_signals_after_prepare_exact") {
        assert_flags(&session, expected);
    }
    dataset.arm(); // Before execute_presence; the supplement also checks demand timing.
    let mut rows = prepared
        .execute_presence()
        .expect("actual query compilation");
    if let Some(expected) = run.get("decimal_signals_after_execute_before_first_poll_exact") {
        assert_flags(&session, expected);
    }
    let wanted = &run["expect"];
    if wanted["kind"] == "fatal_after_prefix" {
        for row in wanted["prefix_rows"].as_array().unwrap() {
            assert_eq!(
                row_json(
                    rows.next()
                        .expect("frozen prefix row")
                        .expect("prefix success")
                ),
                *row
            );
        }
        let Err(error) = rows.next().expect("fatal after exact prefix") else {
            panic!("fatal cannot become a row")
        };
        assert_failure(&error, run, &dataset);
        if let Some(n) = run["failing_dataset_calls"].as_u64() {
            assert_eq!(dataset.trace.failing.get(), n as usize);
        }
        if let Some(n) = run["target_dataset_calls"].as_u64() {
            assert_eq!(dataset.trace.target_calls.get(), n as usize);
        }
        let counts = dataset.trace.counts();
        assert!(rows.next().is_none(), "fatal must stop all following work");
        assert_eq!(dataset.trace.counts(), counts);
    } else {
        assert_eq!(wanted["kind"], "solutions");
        let actual = rows
            .by_ref()
            .map(|r| row_json(r.unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            *wanted["rows"].as_array().unwrap(),
            "first-seen/source row order"
        );
        assert!(rows.next().is_none());
    }
    assert_flags(&session, &run["decimal_signals_after_query_exact"]);
}

// References from the actual lowered tree, never rebuilt fixture expressions.
fn source_group(pattern: &GraphPattern) -> Option<&GraphPattern> {
    match pattern {
        GraphPattern::SourceGroup { .. } => Some(pattern),
        GraphPattern::Project { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Filter { inner, .. }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Service { inner, .. } => source_group(inner),
        GraphPattern::Join { left, right, .. }
        | GraphPattern::LeftJoin { left, right, .. }
        | GraphPattern::Minus { left, right, .. } => {
            source_group(left).or_else(|| source_group(right))
        }
        GraphPattern::Union { inner } => inner.iter().find_map(source_group),
        _ => None,
    }
}
fn stats_for<'a>(stats: &'a EvalNodeWithStats, label: &str) -> Option<&'a EvalNodeWithStats> {
    if stats.label == label {
        return Some(stats);
    }
    stats
        .children
        .iter()
        .find_map(|child| stats_for(child, label))
}
fn receive_cache_replay(root: &Value, run: &Value) {
    let checkpoints = run["checkpoints"].as_array();
    let decoder = run["expect"]["failure_stage"] == "final_presence_decoder";
    let observe_group = run.get("group_observation").is_some();
    if checkpoints.is_none()
        && !decoder
        && !observe_group
        && run.get("carrier_assertions").is_none()
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
    let group =
        source_group(prepared.plan()).expect("source revision must reach production SourceGroup");
    if run.get("carrier_assertions").is_some() {
        let names = vec![
            Variable::new("left").unwrap(),
            Variable::new("right").unwrap(),
        ];
        let calls = dataset.trace.internalizations.get();
        let input = prepared.initial().encode_for(&names).unwrap();
        assert_eq!(
            calls,
            dataset.trace.internalizations.get(),
            "prepared initial encoding only clones"
        );
        let (Some(SolutionValue::Rdf(left)), Some(SolutionValue::Rdf(right))) =
            (input.cell(0), input.cell(1))
        else {
            panic!("actual initial cells")
        };
        assert_eq!(left.rdf(), right.rdf());
        assert!(left == right);
        assert_eq!(hash_bytes(left), hash_bytes(right));
        assert_ne!(cache_json(left.numeric()), cache_json(right.numeric()));
        assert_eq!(hash_bytes(left), hash_bytes(left.rdf()));
    }
    // Only enable the existing execution counters; keep the prepared plan,
    // dataset, initial context and compiler intact.
    let mut evaluator = prepared.evaluator().clone();
    evaluator.run_stats = true;
    let mut variables = Vec::new();
    let (plan, stats) = evaluator.graph_pattern_evaluator(prepared.plan(), &mut variables);
    let group_stats =
        stats_for(&stats, &eval_node_label(group)).expect("actual Group statistics node");
    let input = prepared.initial().encode_for(&variables).unwrap();
    dataset.arm();
    let mut rows = plan.unwrap()(input).expect("legacy query starts successfully");
    let mut observed = Vec::<BTreeMap<String, CachedInternalTerm<Term>>>::new();
    let mut had_error = false;
    while let Some(row) = rows.next() {
        let tuple = match row {
            Ok(tuple) => tuple,
            Err(error) => {
                assert!(
                    !decoder,
                    "decoder-only failure must not happen in internal evaluation"
                );
                assert_failure(&error, run, &dataset);
                had_error = true;
                let counts = dataset.trace.counts();
                assert!(rows.next().is_none());
                assert_eq!(counts, dataset.trace.counts());
                break;
            }
        };
        if observed.is_empty() {
            if let Some(n) =
                run["group_observation"]["input_rows_consumed_before_first_output"].as_u64()
            {
                assert_eq!(
                    group_stats
                        .children
                        .first()
                        .expect("real Group input stats")
                        .exec_count
                        .get(),
                    n as usize
                );
            }
        }
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
                        panic!("Known cache must have real RDF literal")
                    };
                    assert_eq!(cache.datatype().as_str(), literal.datatype().as_str());
                }
                map.insert(name.as_str().to_owned(), cell.clone());
            }
        }
        observed.push(map);
    }
    assert_eq!(
        had_error,
        run["expect"]["kind"] == "fatal_after_prefix" && !decoder
    );
    if let Some(checkpoints) = checkpoints {
        for checkpoint in checkpoints {
            let index = checkpoint["row_index"].as_u64().unwrap() as usize;
            assert_cache(
                observed[index]
                    .get(text(checkpoint, "binding"))
                    .expect("frozen cache binding")
                    .numeric(),
                &checkpoint["cache"],
            );
        }
    }
    if let Some(n) = run["group_observation"]["finished_groups"].as_u64() {
        assert_eq!(
            group_stats.exec_count.get(),
            n as usize,
            "LIMIT must not poll/finalize another group"
        );
    }
    if decoder {
        assert_eq!(observed.len(), 1);
        assert_cache(
            observed[0]["s"].numeric(),
            &json!({"state":"Known","value":{"family":"Integer","value":"4"}}),
        );
        assert_eq!(
            dataset.trace.target_calls.get(),
            run["target_dataset_calls"].as_u64().unwrap() as usize - 1
        );
        assert_eq!(
            dataset.trace.failing.get(),
            0,
            "actual alias succeeds before final decoder"
        );
    }
    assert_flags(&session, &run["decimal_signals_after_query_exact"]);
}

#[derive(Default)]
struct RawTrace {
    calls: Cell<usize>,
    returned: RefCell<Vec<ErrorValue>>,
    raised: RefCell<Vec<ErrorValue>>,
    per_row: RefCell<Vec<usize>>,
}
fn evaluator_slot<'b, 'a, T>(
    accumulator: &'b mut NullableAccumulator<'a, T>,
) -> &'b mut NullableAggregateEvaluator<'a, T> {
    match accumulator {
        NullableAccumulator::Extremum { .. } => panic!("SUM/AVG receiving does not build MIN/MAX"),
        NullableAccumulator::Count { evaluator, .. }
        | NullableAccumulator::Sample { evaluator, .. }
        | NullableAccumulator::Sum { evaluator, .. }
        | NullableAccumulator::GroupConcat { evaluator, .. }
        | NullableAccumulator::Average { evaluator, .. } => evaluator,
    }
}

// This replay observes actual raw callbacks on production accumulators built
// by the same compiler from the real SourceGroup. It does not supply final
// query answers: every frozen run first goes through bound execute_presence.
fn receive_consumer_replay(root: &Value, run: &Value) {
    let Some(expected_traces) = run["consumer_trace"].as_array() else {
        return;
    };
    let session = session(root, run, false);
    let dataset = ReceivingDataset::new(run);
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset.clone(),
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    let Some(GraphPattern::SourceGroup {
        inner,
        aggregates,
        grouping,
    }) = source_group(prepared.plan())
    else {
        panic!("real source group")
    };
    assert!(
        matches!(grouping, spargebra::algebra::SourceGrouping::Implicit),
        "observed raw cases have actual implicit group"
    );
    if let Some(n) = run["group_observation"]["aggregate_count"].as_u64() {
        assert_eq!(aggregates.len(), n as usize);
    }
    let mut variables = Vec::new();
    let (child, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(inner, &mut variables);
    let mut accumulators = Vec::new();
    let mut traces = Vec::new();
    for (_, expression) in aggregates {
        let builder = prepared
            .evaluator()
            .accumulator_builder(expression, &mut variables, &mut Vec::new())
            .unwrap();
        let mut accumulator = builder();
        let AccumulatorWrapper::Nullable(acc) = &mut accumulator else {
            panic!("actual nullable accumulator")
        };
        let NullableAggregateEvaluator::RawExpression(raw) = evaluator_slot(acc) else {
            panic!("actual numeric compiler callback")
        };
        let original = Rc::clone(raw);
        let trace = Rc::new(RawTrace::default());
        let observer = Rc::clone(&trace);
        *raw = Rc::new(move |row| {
            observer.calls.set(observer.calls.get() + 1);
            let result = original(row);
            match &result {
                Ok(SolutionValue::ReturnedError(error)) => {
                    observer.returned.borrow_mut().push(error.clone())
                }
                Err(EvalFailure::Sparql(error)) => observer.raised.borrow_mut().push(error.clone()),
                _ => {}
            }
            result
        });
        traces.push(trace);
        accumulators.push(accumulator);
    }
    let input = prepared.initial().encode_for(&variables).unwrap();
    dataset.arm();
    let mut failed = false;
    'rows: for row in child.unwrap()(input).expect("legacy query starts successfully") {
        let row = row.expect("real Group child row");
        for (index, accumulator) in accumulators.iter_mut().enumerate() {
            let before = traces[index].calls.get();
            let result = accumulator.accumulate(&row);
            traces[index]
                .per_row
                .borrow_mut()
                .push(traces[index].calls.get() - before);
            if let Err(error) = result {
                assert_failure(&error, run, &dataset);
                if let Some(NullableAggregateError::UncaughtSparql(uncaught)) =
                    nullable_aggregate_error(&error)
                {
                    let raised = traces[index].raised.borrow();
                    // A gate-raised NotBound must emerge as the SAME object;
                    // helper-created numeric Type errors have no raw identity.
                    if !raised.is_empty() {
                        assert!(
                            uncaught == raised.last().unwrap(),
                            "uncaught wrapper changed raw error identity"
                        );
                    }
                }
                failed = true;
                break 'rows;
            }
        }
    }
    assert_eq!(failed, run["expect"]["kind"] == "fatal_after_prefix");
    for (index, wanted) in expected_traces.iter().enumerate() {
        let trace = &traces[index];
        let AggregateExpression::FunctionCall { name, .. } = &aggregates[index].1 else {
            panic!("observed function aggregate")
        };
        let name = match name {
            AggregateFunction::Count => "COUNT",
            AggregateFunction::Sample => "SAMPLE",
            AggregateFunction::Sum => "SUM",
            AggregateFunction::Avg => "AVG",
            other => panic!("unexpected observed aggregate {other:?}"),
        };
        assert_eq!(name, text(wanted, "kind"));
        if let Some(n) = wanted["raw_evaluations"].as_u64() {
            assert_eq!(trace.calls.get(), n as usize);
        }
        if let Some(gate) = wanted["gate_evaluations"].as_u64() {
            let update = wanted["update_evaluations"].as_u64().unwrap();
            assert_eq!(trace.calls.get(), (gate + update) as usize);
            assert_eq!(trace.per_row.borrow().len(), gate as usize);
            if gate == 2 && update == 1 {
                assert_eq!(
                    *trace.per_row.borrow(),
                    vec![2, 1],
                    "DISTINCT second equal RDF row gets gate only"
                );
            }
            if gate == 1 && update == 0 {
                assert_eq!(
                    *trace.per_row.borrow(),
                    vec![1],
                    "fatal gate prevents update"
                );
            }
        }
        if let Some(kind) = wanted["returned_error_kind"].as_str() {
            assert!(!trace.returned.borrow().is_empty());
            for error in trace.returned.borrow().iter() {
                assert_error_value(error, kind);
            }
        }
        if wanted["selected_exact_error_object"] == true {
            let AccumulatorWrapper::Nullable(NullableAccumulator::Sample {
                selected:
                    Some(AggregateValue::RawExpression(SolutionValue::ReturnedError(selected))),
                ..
            }) = &accumulators[index]
            else {
                panic!("SAMPLE must retain raw selected error before real alias Extend")
            };
            let errors = trace.returned.borrow();
            assert_eq!(errors.len(), 1);
            assert!(selected == &errors[0]);
        }
    }
    if let Some(identity) = run.get("returned_error_identity_assertions") {
        let errors = traces
            .iter()
            .flat_map(|t| t.returned.borrow().clone())
            .collect::<Vec<_>>();
        assert_eq!(
            errors.len(),
            identity["object_count"].as_u64().unwrap() as usize
        );
        for (index, error) in errors.iter().enumerate() {
            assert_error_value(error, text(identity, "kind"));
            assert_eq!(error.diagnostic(), text(identity, "diagnostic"));
            for previous in &errors[..index] {
                assert!(
                    error != previous,
                    "fresh source evaluation reused error identity"
                );
            }
        }
    }
    if !failed {
        for accumulator in accumulators {
            let _ = accumulator.finish().expect("actual fallible finish");
        }
    }
    assert_flags(&session, &run["decimal_signals_after_query_exact"]);
}

fn run_family(id: &str) {
    let root = fixture();
    let family = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap();
    for run in family["runs"].as_array().unwrap() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            receive_bound(&root, run, false);
            receive_cache_replay(&root, run);
            receive_consumer_replay(&root, run);
        }));
        if let Err(error) = result {
            eprintln!("frozen {}: {}", text(run, "id"), text(run, "query"));
            std::panic::resume_unwind(error);
        }
    }
}

#[test]
fn query_sum_avg_fixture_identity_and_legacy_controls() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ErrorValue>();
    assert_send_sync::<NullableAggregateError>();
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
        let mut run = control["unchanged_frozen_run"].clone();
        run["numeric_capability"] = control["capability"].clone();
        run["group_semantics"] = json!("legacy-private-v1");
        run["decimal_signals_after_query_exact"] =
            control["additional_decimal_signals_after_query_exact"].clone();
        // Equivalent assertion shape for the unchanged old expectation;
        // never change the original stored input or numerical/error answer.
        assert_eq!(run["expect"]["kind"], "fatal");
        run["expect"]["kind"] = json!("fatal_after_prefix");
        run["expect"]["prefix_rows"] = json!([]);
        receive_bound(&root, &run, true);
        receive_bound(&root, &run, false);
    }
}

#[test]
fn query_sum_avg_source_group_waits_for_first_poll() {
    // Separate immutable source supplement; the 16/57 main cases are unchanged.
    let supplement: Value =
        serde_json::from_str(include_str!("fixtures/query_sum_avg_demand_cases.json")).unwrap();
    assert_eq!(
        supplement["source_record_id"],
        "QUERY-SUM-AVG-DEMAND-SUPPLEMENT-01"
    );
    assert_eq!(
        supplement["source_record_sha256"],
        "a66e658c20a014f949a45ca301f2d460871d636f28aca21e87f8464502b0cfce"
    );
    let payload = json!({"environment":supplement["environment"],"cases":supplement["cases"]});
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "af1fd60cee6d1e67604a71fd4336fdddc2ea230964a35872e1f15f2e558554a9"
    );
    let cases = supplement["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let mut failures = Vec::new();
    for run in cases {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            receive_bound(&supplement, run, false);
            if let Some(checkpoint) = run.get("checkpoint") {
                let mut observation = run.clone();
                let mut checkpoint = checkpoint.clone();
                checkpoint["row_index"] = json!(0);
                observation["checkpoints"] = json!([checkpoint]);
                receive_cache_replay(&supplement, &observation);
            }
        }));
        if result.is_err() {
            failures.push(text(run, "id").to_owned());
        }
    }
    assert!(
        failures.is_empty(),
        "source-demand cases failed: {failures:?}"
    );
}

#[test]
fn query_sum_avg_sa01() {
    run_family("SA01");
}
#[test]
fn query_sum_avg_sa02() {
    run_family("SA02");
}
#[test]
fn query_sum_avg_sa03() {
    run_family("SA03");
}
#[test]
fn query_sum_avg_sa04() {
    run_family("SA04");
}
#[test]
fn query_sum_avg_sa05() {
    run_family("SA05");
}
#[test]
fn query_sum_avg_sa06() {
    run_family("SA06");
}
#[test]
fn query_sum_avg_sa07() {
    run_family("SA07");
}
#[test]
fn query_sum_avg_sa08() {
    run_family("SA08");
}
#[test]
fn query_sum_avg_sa09() {
    run_family("SA09");
}
#[test]
fn query_sum_avg_sa10() {
    run_family("SA10");
}
#[test]
fn query_sum_avg_sa11() {
    run_family("SA11");
}
#[test]
fn query_sum_avg_sa12() {
    run_family("SA12");
}
