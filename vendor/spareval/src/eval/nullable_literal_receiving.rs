//! Independent receiving adapter for the immutable literal source corpus.
//! Expected data is a mechanical public projection of the source-only oracle.
//! The adapter author reviewed the oracle but did not author its expected data.
//! Every query outcome uses the sealed real prepare/execute_source_presence path.
use super::nullable::{
    NullableAccumulator, NullableAggregateError, NullableAggregateEvaluator,
    nullable_aggregate_error,
};
use super::numeric_query::prepare_numeric_query;
use super::*;
use crate::dataset::nullable_numeric::NumericDataset;
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    ParsedSourceLiteral, QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities,
    QueryNumericSession, RawExpressionValue, RawNumericFault, SourceDatatype, SourceFact,
    SourceLiteralCacheKind, SourceLiteralMetadata, SourceLiteralOrigin, raw_numeric_fault,
    source_identity_key,
};
use crate::tuple::SparqlErrorKind;
use semantica_numeric::{DecimalContext, DecimalSignals, NumericLimits, NumericValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

const FIXTURE: &str = include_str!("fixtures/literal_identity_cases.json");
const ORACLE_SHA: &str = "06c8b66119ae3a6138d08d3418bab7684096a789e65a170e8b6448bd31e94f98";
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        value => value.clone(),
    }
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "bf9ea8644ee1bab1e4facbbb0558c4cd02c832ee6e6545419ef74170c4406b10"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(root["source_record_id"], "LITERAL-IDENTITY-FROZEN-01");
    assert_eq!(root["source_record_sha256"], ORACLE_SHA);
    let payload: serde_json::Map<String, Value> = [
        "explicit_environment",
        "identity_contract",
        "core_cases",
        "query_families",
        "receiving_requirements",
        "excluded_not_complete",
    ]
    .into_iter()
    .map(|k| (k.to_owned(), root[k].clone()))
    .collect();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&Value::Object(payload))).unwrap())
        ),
        "0ea57f94c88ce9d11d3d14b4c6f74209e96398b087814fcfa001e767d50b895e"
    );
    assert_eq!(root["compatibility_controls"].as_array().unwrap().len(), 3);
    assert_eq!(root["core_cases"].as_array().unwrap().len(), 12);
    assert_eq!(root["query_families"].as_array().unwrap().len(), 17);
    assert_eq!(
        root["query_families"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["runs"].as_array().unwrap().len())
            .sum::<usize>(),
        31
    );
    root
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing string {key}: {v}"))
}
fn node(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}
fn source_session(root: &Value) -> Rc<QueryNumericSession> {
    let env = &root["explicit_environment"];
    assert_eq!(env["literal_semantics"], "SourceLiteralV1");
    assert_eq!(env["numeric_capabilities"], "MixedBinary64V1");
    assert_eq!(env["group_semantics"], "SourceAggregateV1");
    assert_eq!(env["decimal"]["precision"], 28);
    assert_eq!(env["decimal"]["rounding"], "HALF_EVEN");
    let decimal = DecimalContext::default();
    assert_eq!(
        env["decimal"]["traps"],
        json!(["InvalidOperation", "DivisionByZero", "Overflow"])
    );
    let session = QueryNumericSession::new_with_literal_semantics(
        decimal,
        NumericLimits::default(),
        None,
        env["normalize_literals"].as_bool().unwrap(),
        QueryNumericCapabilities::MixedBinary64V1,
        QueryGroupSemantics::SourceAggregateV1,
        QueryLiteralSemantics::SourceLiteralV1,
    )
    .unwrap();
    assert_eq!(
        session.literal_semantics(),
        QueryLiteralSemantics::SourceLiteralV1
    );
    Rc::new(session)
}
fn assert_no_flags(session: &QueryNumericSession) {
    assert_eq!(session.flags(), DecimalSignals::EMPTY);
}
fn datatype(value: &Value) -> SourceDatatype {
    match text(value, "state") {
        "absent" => SourceDatatype::Absent,
        "specified" => SourceDatatype::Specified(node(text(value, "iri"))),
        state => panic!("unexpected known datatype {state}"),
    }
}
fn optional_datatype(value: &Value) -> Option<NamedNode> {
    value.as_str().map(node)
}
fn rdf_literal(lexical: &str, datatype: Option<NamedNode>, language: Option<&str>) -> Literal {
    if let Some(language) = language {
        assert!(datatype.is_none());
        Literal::new_language_tagged_literal(lexical, language).unwrap()
    } else if let Some(datatype) = datatype {
        Literal::new_typed_literal(lexical, datatype)
    } else {
        Literal::new_simple_literal(lexical)
    }
}
fn descriptor_datatype(value: &Value) -> Option<NamedNode> {
    match datatype(&value["source_datatype"]) {
        SourceDatatype::Absent => None,
        SourceDatatype::Specified(dt) => Some(dt),
        SourceDatatype::Unknown => unreachable!(),
    }
}
fn parsed_literal(
    lexical: &str,
    datatype: Option<NamedNode>,
    language: Option<String>,
) -> ParsedSourceLiteral {
    let origin = if datatype.is_some() {
        SourceLiteralOrigin::QueryTypedExistingLiteral
    } else if language.is_some() {
        SourceLiteralOrigin::QueryLanguageExistingLiteral
    } else {
        SourceLiteralOrigin::QueryPlain
    };
    ParsedSourceLiteral {
        occurrence: None,
        rdf: rdf_literal(lexical, datatype.clone(), language.as_deref()),
        datatype: datatype.map_or(SourceDatatype::Absent, SourceDatatype::Specified),
        language_original: language,
        origin,
    }
}
fn construct_input(session: &QueryNumericSession, input: &Value) -> RawExpressionValue {
    match text(input, "origin") {
        "DirectString" => session
            .construct_source_direct_string(
                text(input, "lexical"),
                optional_datatype(&input["datatype"]),
                input["language"].as_str().map(str::to_owned),
                input["normalize"].as_bool().unwrap(),
            )
            .unwrap(),
        "QueryTypedExistingLiteral" => session
            .construct_source_literal(&parsed_literal(
                text(input, "lexical"),
                Some(node(text(input, "datatype"))),
                None,
            ))
            .unwrap(),
        other => panic!("unexpected constructor origin {other}"),
    }
}
fn descriptor_input(session: &QueryNumericSession, input: &Value) -> RawExpressionValue {
    // These are the frozen core INPUT objects, never a query expected answer.
    // Use actual factories; only C12's explicitly trusted cached float object
    // requires the source-known object constructor rather than a string parse.
    assert_eq!(input["kind"], "literal");
    let lexical = text(input, "lexical");
    let dt = descriptor_datatype(input);
    let language = input["original_language"].as_str().map(str::to_owned);
    let value = if input["cache_family"] == "Binary64" {
        assert_eq!(lexical, "1.0");
        assert_eq!(input["ill_typed"], Value::Null);
        session
            .known_source_cached(
                rdf_literal(lexical, dt, language.as_deref()).into(),
                NumericValue::Binary64(1.0),
                SourceLiteralMetadata {
                    datatype: datatype(&input["source_datatype"]),
                    language_original: SourceFact::Known(language),
                    ill_typed: SourceFact::Known(None),
                    cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
                },
            )
            .unwrap()
    } else if input["ill_typed"].is_boolean() {
        session
            .construct_source_direct_string(lexical, dt, language, true)
            .unwrap()
    } else {
        session
            .construct_source_literal(&parsed_literal(lexical, dt, language))
            .unwrap()
    };
    assert_eq!(
        describe(&value),
        *input,
        "core input factory did not form the frozen object"
    );
    value
}
fn describe(raw: &RawExpressionValue) -> Value {
    let Term::Literal(rdf) = raw.rdf().unwrap() else {
        panic!("this corpus expects Literal output")
    };
    let LiteralSourceState::Literal(meta) = raw.literal_source() else {
        panic!("Literal lost its source state")
    };
    if matches!(meta.datatype, SourceDatatype::Unknown) {
        assert!(matches!(meta.language_original, SourceFact::Unknown));
        assert!(matches!(meta.ill_typed, SourceFact::Unknown));
        assert!(matches!(meta.cached_value_kind, SourceFact::Unknown));
        return json!({"kind":"transported_rdf_with_unknown_source_metadata","rdf":{
            "kind":"literal","lexical":rdf.value(),"datatype":rdf.datatype().as_str(),"language":rdf.language()}});
    }
    let dt = match &meta.datatype {
        SourceDatatype::Absent => json!({"state":"absent"}),
        SourceDatatype::Specified(dt) => json!({"state":"specified","iri":dt.as_str()}),
        SourceDatatype::Unknown => unreachable!(),
    };
    let SourceFact::Known(language) = &meta.language_original else {
        panic!("known source language required")
    };
    let SourceFact::Known(ill) = &meta.ill_typed else {
        panic!("known ill_typed required")
    };
    let family = match &meta.cached_value_kind {
        SourceFact::Known(
            SourceLiteralCacheKind::DateTime(_)
            | SourceLiteralCacheKind::Date(_)
                    | SourceLiteralCacheKind::Time(_)
            | SourceLiteralCacheKind::DayTimeDuration(_)
            | SourceLiteralCacheKind::UnmappedDatatype,
        ) => {
            panic!("temporal cache outside this earlier fixture domain")
        }
        SourceFact::Known(SourceLiteralCacheKind::Duration(_)) => panic!("calendar duration is not admitted by this legacy receiving entry"),
                SourceFact::Known(SourceLiteralCacheKind::String) => "String",
        SourceFact::Known(SourceLiteralCacheKind::None) => "None",
        SourceFact::Known(SourceLiteralCacheKind::ErrorObject) => {
            panic!("literal receiving does not construct Literal(error)")
        }
        SourceFact::Known(SourceLiteralCacheKind::Boolean(_)) => {
            panic!("literal receiving does not construct BooleanLiteral tokens")
        }
        SourceFact::Known(SourceLiteralCacheKind::Numeric) => {
            let NumericCacheState::Known(cache) = raw.numeric() else {
                panic!("known numeric cache required")
            };
            match cache.value() {
                NumericValue::Integer(_) => "Integer",
                NumericValue::Decimal(_) => "Decimal",
                NumericValue::Binary64(_) => "Binary64",
            }
        }
        SourceFact::Unknown => panic!("source cache kind must not be inferred from RDF"),
    };
    json!({"kind":"literal","lexical":rdf.value(),"source_datatype":dt,"original_language":language,
        "ill_typed":ill,"cache_family":family})
}
fn row_json(row: SourcePresenceSolution) -> Value {
    let mut values = serde_json::Map::new();
    for (variable, cell) in row.variables.iter().zip(row.values) {
        match cell {
            None => {}
            Some(SolutionValue::Rdf(raw)) => {
                values.insert(variable.as_str().into(), describe(&raw));
            }
            Some(SolutionValue::ExplicitNone) => {
                values.insert(variable.as_str().into(), json!({"kind":"explicit_none"}));
            }
            Some(SolutionValue::ReturnedError(_)) => {
                panic!("unexpected returned error in frozen literal result")
            }
        }
    }
    Value::Object(values)
}
#[derive(Default)]
struct HashBytes(Vec<u8>);
impl Hasher for HashBytes {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes)
    }
}
fn hash_bytes(value: &impl Hash) -> Vec<u8> {
    let mut h = HashBytes::default();
    value.hash(&mut h);
    h.0
}
fn run_core(id: &str) {
    let root = fixture();
    let session = source_session(&root);
    let case = root["core_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    match text(case, "operation") {
        "source_eq_hash_contract" => {
            let input = case["inputs"].as_array().unwrap();
            let left = descriptor_input(&session, &input[0]);
            let right = descriptor_input(&session, &input[1]);
            let a = source_identity_key(&left).unwrap();
            let b = source_identity_key(&right).unwrap();
            let equal = case["expected"]["eq"].as_bool().unwrap();
            assert_eq!(a == b, equal, "{id}");
            if equal {
                assert_eq!(hash_bytes(&a), hash_bytes(&b), "{id}");
            }
            // Unequal hashes are not required: collisions are legal.
            assert_eq!(hash_bytes(&left), hash_bytes(left.rdf().unwrap()));
            assert_eq!(hash_bytes(&right), hash_bytes(right.rdf().unwrap()));
        }
        "constructor" => assert_eq!(
            describe(&construct_input(&session, &case["inputs"])),
            case["expected"],
            "{id}"
        ),
        "literal_existing_constructor" | "transport_clone" => {
            let input = descriptor_input(&session, &case["inputs"][0]);
            let value = if case["operation"] == "transport_clone" {
                input.clone()
            } else {
                session.copy_source_literal(&input).unwrap()
            };
            assert_eq!(describe(&value), case["expected"], "{id}");
            assert_eq!(
                describe(&input),
                case["inputs"][0],
                "constructor/clone mutated original"
            );
        }
        "source_identity_key" => {
            let rdf = &case["inputs"]["backend_rdf"];
            let raw = RawExpressionValue::from_import(
                Literal::new_typed_literal(text(rdf, "lexical"), node(text(rdf, "datatype")))
                    .into(),
            );
            assert!(matches!(
                source_identity_key(&raw),
                Err(RawNumericFault::UnknownSourceLiteralIdentity)
            ));
        }
        other => panic!("unknown frozen core operation {other}"),
    }
    assert_no_flags(&session);
}
fn initial(run: &Value) -> Vec<NumericInitialBinding> {
    run["initial_objects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|binding| {
            let c = &binding["constructor"];
            let value = match text(c, "origin") {
                "DirectString" => NumericInitialValue::SourceDirectString {
                    lexical: text(c, "lexical").into(),
                    datatype: optional_datatype(&c["datatype"]),
                    language: c["language"].as_str().map(str::to_owned),
                    normalize: c["normalize"].as_bool().unwrap(),
                },
                "UnknownNormalized" => {
                    let v = &c["backend_rdf"];
                    NumericInitialValue::UnknownNormalized(
                        Literal::new_typed_literal(text(v, "lexical"), node(text(v, "datatype")))
                            .into(),
                    )
                }
                other => panic!("unsupported initial origin {other}"),
            };
            NumericInitialBinding {
                variable: Variable::new(text(binding, "variable")).unwrap(),
                value,
            }
        })
        .collect()
}
fn assert_failure(error: &QueryEvaluationError, expected: &Value) {
    match text(expected, "classification") {
        "source_uncaught_sparql" => {
            assert_eq!(expected["source_exception"], "SPARQLTypeError");
            let Some(NullableAggregateError::UncaughtSparql(value)) =
                nullable_aggregate_error(error)
            else {
                panic!("original typed uncaught SPARQL error: {error}")
            };
            assert_eq!(value.kind(), SparqlErrorKind::Type);
        }
        "source_fatal" => {
            assert_eq!(expected["source_exception"], "TypeError");
            assert!(
                matches!(
                    raw_numeric_fault(error),
                    Some(RawNumericFault::AggregateTypePromotion)
                ),
                "{error}"
            );
        }
        "active_safety_not_original_python" => {
            assert_eq!(
                expected["semantic_class"],
                "unknown_source_literal_identity"
            );
            assert!(
                matches!(
                    raw_numeric_fault(error),
                    Some(RawNumericFault::UnknownSourceLiteralIdentity)
                ),
                "{error}"
            );
        }
        other => panic!("unknown expected failure {other}"),
    }
}
#[derive(Debug)]
struct FailureIdentity;
#[derive(Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("literal receiving Dataset failure")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct Trace {
    internal: Cell<usize>,
    external: Cell<usize>,
    scan: Cell<usize>,
    fail_external: Cell<bool>,
    lossy_internal: Cell<bool>,
}
impl Trace {
    fn counts(&self) -> (usize, usize, usize) {
        (self.internal.get(), self.external.get(), self.scan.get())
    }
}
#[derive(Clone)]
struct ReceivingDataset {
    trace: Rc<Trace>,
    identity: Arc<FailureIdentity>,
}
impl ReceivingDataset {
    fn new() -> Self {
        Self {
            trace: Rc::new(Trace::default()),
            identity: Arc::new(FailureIdentity),
        }
    }
}
impl<'a> QueryableDataset<'a> for ReceivingDataset {
    type InternalTerm = Term;
    type Error = ConversionFailure;
    fn internal_quads_for_pattern(
        &self,
        _s: Option<&Term>,
        _p: Option<&Term>,
        _o: Option<&Term>,
        _g: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.trace.scan.set(self.trace.scan.get() + 1);
        std::iter::empty()
    }
    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.trace.internal.set(self.trace.internal.get() + 1);
        if self.trace.lossy_internal.get() && matches!(term, Term::Literal(_)) {
            return Ok(Literal::new_simple_literal("changed by receiving backend").into());
        }
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.trace.external.set(self.trace.external.get() + 1);
        if self.trace.fail_external.get() {
            return Err(ConversionFailure(Arc::clone(&self.identity)));
        }
        Ok(term)
    }
}
fn assert_dataset_failure(error: &QueryEvaluationError, dataset: &ReceivingDataset) {
    let QueryEvaluationError::Dataset(error) = error else {
        panic!("actual Dataset error expected: {error}")
    };
    let error = error
        .downcast_ref::<ConversionFailure>()
        .expect("original Dataset error type");
    assert!(Arc::ptr_eq(&error.0, &dataset.identity));
}
fn receive_bound(root: &Value, run: &Value) {
    let session = source_session(root);
    let dataset = ReceivingDataset::new();
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset.clone(),
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    assert_no_flags(&session);
    let mut rows = prepared
        .execute_source_presence()
        .expect("actual source query compilation");
    let expected = &run["expected"];
    match text(expected, "terminal") {
        "end" => {
            let actual = rows
                .by_ref()
                .map(|r| row_json(r.expect("frozen successful row")))
                .collect::<Vec<_>>();
            assert_eq!(json!(actual), expected["rows"], "{}", text(run, "id"));
        }
        "one_error_then_fused" => {
            for wanted in expected["rows_before_error"].as_array().unwrap() {
                assert_eq!(
                    row_json(rows.next().expect("frozen prefix").unwrap()),
                    *wanted
                );
            }
            let error = rows
                .next()
                .expect("one source/active error")
                .err()
                .expect("failure cannot become a row");
            assert_failure(&error, &expected["error"]);
        }
        other => panic!("unexpected terminal {other}"),
    }
    let counts = dataset.trace.counts();
    assert!(rows.next().is_none(), "must fuse after EOF/error");
    assert!(rows.next().is_none());
    assert_eq!(
        dataset.trace.counts(),
        counts,
        "terminated iterator performed more Dataset work"
    );
    assert_eq!(
        dataset.trace.scan.get(),
        0,
        "fixture query must not invent a graph scan"
    );
    assert_no_flags(&session);
}
fn source_group(pattern: &GraphPattern) -> Option<&GraphPattern> {
    match pattern {
        GraphPattern::SourceGroup { .. } => Some(pattern),
        GraphPattern::Project { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Filter { inner, .. }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner } => source_group(inner),
        GraphPattern::Union { inner } => inner.iter().find_map(source_group),
        _ => None,
    }
}
fn raw_slot<'b, 'a, T>(
    acc: &'b mut NullableAccumulator<'a, T>,
) -> &'b mut NullableAggregateEvaluator<'a, T> {
    match acc {
        NullableAccumulator::Extremum { .. } => panic!("literal receiving does not build MIN/MAX"),
        NullableAccumulator::Count { evaluator, .. }
        | NullableAccumulator::Sum { evaluator, .. }
        | NullableAccumulator::Average { evaluator, .. }
        | NullableAccumulator::GroupConcat { evaluator, .. }
        | NullableAccumulator::Sample { evaluator, .. } => evaluator,
    }
}
fn receive_consumer_schedule(root: &Value, run: &Value) {
    let expected_counts = run.get("expected_evaluation_counts");
    let failing_index = run["expected"]["error"]["first_failing_input_index"].as_u64();
    if expected_counts.is_none() && failing_index.is_none() {
        return;
    }
    let session = source_session(root);
    let dataset = ReceivingDataset::new();
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset,
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    let GraphPattern::SourceGroup {
        inner, aggregates, ..
    } = source_group(prepared.plan()).expect("actual SourceGroup")
    else {
        unreachable!()
    };
    let mut variables = Vec::new();
    let (child, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(inner, &mut variables);
    let mut accumulators = Vec::new();
    let mut counts = Vec::new();
    let mut labels = Vec::new();
    for (_, expression) in aggregates {
        let builder = prepared
            .evaluator()
            .accumulator_builder(expression, &mut variables, &mut Vec::new())
            .unwrap();
        let mut accumulator = builder();
        let acc = match &mut accumulator {
            AccumulatorWrapper::Nullable(acc)
            | AccumulatorWrapper::SourceDistinct {
                accumulator: acc, ..
            } => acc,
            _ => panic!("real typed accumulator"),
        };
        let NullableAggregateEvaluator::RawExpression(raw) = raw_slot(acc) else {
            panic!("real numeric/source compiler")
        };
        let original = Rc::clone(raw);
        let count = Rc::new(Cell::new(0usize));
        let observer = Rc::clone(&count);
        *raw = Rc::new(move |row| {
            observer.set(observer.get() + 1);
            original(row)
        });
        // Only observe and forward the production evaluator. No values or
        // errors are injected and this replay never supplies final query rows.
        let AggregateExpression::FunctionCall { name, distinct, .. } = expression else {
            panic!("frozen observed function")
        };
        labels.push(match name {
            AggregateFunction::Count => {
                if *distinct {
                    "COUNT(DISTINCT ?x)"
                } else {
                    "COUNT(?x)"
                }
            }
            AggregateFunction::Sum => "SUM(?x)",
            _ => panic!("unexpected observed aggregate"),
        });
        counts.push(count);
        accumulators.push(accumulator);
    }
    let input = prepared.initial().encode_for(&variables).unwrap();
    let mut failed_at = None;
    'rows: for (index, row) in child.unwrap()(input)
        .expect("legacy query starts successfully")
        .enumerate()
    {
        let row = row.unwrap();
        for accumulator in &mut accumulators {
            if let Err(error) = accumulator.accumulate(&row) {
                assert_failure(&error, &run["expected"]["error"]);
                failed_at = Some(index as u64);
                break 'rows;
            }
        }
    }
    assert_eq!(failed_at, failing_index, "first source failure row");
    if let Some(wanted) = expected_counts {
        for (label, count) in labels.iter().zip(counts) {
            assert_eq!(
                count.get() as u64,
                wanted[*label].as_u64().unwrap(),
                "{label}"
            )
        }
    }
    assert_no_flags(&session);
}
fn run_family(id: &str) {
    let root = fixture();
    let family = root["query_families"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == id)
        .unwrap();
    for run in family["runs"].as_array().unwrap() {
        receive_bound(&root, run);
        receive_consumer_schedule(&root, run)
    }
}

#[test]
fn literal_identity_fixture_and_prior_receiving_bytes_are_immutable() {
    let root = fixture();
    let fixtures: [(&[u8], &str, &str); 3] = [
        (
            include_bytes!("../../tests/fixtures/query_numeric_cases.json"),
            "136d77465d33fb6adbdcfcadd6d27c4365bf71a4e8d2310f6ef45667b2b0bd76",
            "2a63d9a7043935df5c925b5ed5285d4447a24c9f0d09a532432fd8c2442cf5fd",
        ),
        (
            include_bytes!("../../tests/fixtures/query_binary64_cases.json"),
            "fda1fb295f6d59cc9d8a4d5dafb8f79dd7ff97c3f30705ecfd5adf8b9c71908b",
            "69974deeacb163228a361b5b3d03de93ee2fe437c1d5a95361b7d7de62ed34dd",
        ),
        (
            include_bytes!("fixtures/query_sum_avg_cases.json"),
            "f1c3215e01bff8fd555f5dd5ca4cbf4066786c3347647cbce877881840691031",
            "f86864839e50f1902e18ee89a9315377ead0577ab3065fdc6407d73f25246566",
        ),
    ];
    for ((raw, digest, oracle), control) in fixtures
        .into_iter()
        .zip(root["compatibility_controls"].as_array().unwrap())
    {
        assert_eq!(format!("{:x}", Sha256::digest(raw)), digest);
        let old: Value = serde_json::from_slice(raw).unwrap();
        assert_eq!(old["source_record_sha256"], oracle);
        assert_eq!(control["source_record_sha256"], oracle);
        assert_eq!(control["literal_semantics"], "RdfIdentityV1");
    }
}
#[test]
fn literal_identity_old_constructors_and_physical_hash_stream_remain_rdf() {
    let old = [
        QueryNumericSession::new(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
        )
        .unwrap(),
        QueryNumericSession::new_with_capabilities(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
        )
        .unwrap(),
        QueryNumericSession::new_with_group_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
        )
        .unwrap(),
    ];
    for session in old {
        assert_eq!(
            session.literal_semantics(),
            QueryLiteralSemantics::RdfIdentityV1
        );
        let rows=prepare_numeric_query("SELECT (COUNT(DISTINCT ?x) AS ?n) WHERE { VALUES ?x { \"a\" \"a\"^^<http://www.w3.org/2001/XMLSchema#string> } }",
            ReceivingDataset::new(),Rc::new(session),Vec::new()).unwrap().execute_presence().unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        assert_eq!(rows.len(), 1);
        let Some(SolutionValue::Rdf(Term::Literal(count))) = &rows[0].values[0] else {
            panic!("legacy integer result")
        };
        assert_eq!(count.value(), "1");
    }
    let session = source_session(&fixture());
    let dataset = NumericDataset::new(ReceivingDataset::new());
    let plain = session
        .construct_source_literal(&parsed_literal("a", None, None))
        .unwrap();
    let typed = session
        .construct_source_literal(&parsed_literal(
            "a",
            Some(node("http://www.w3.org/2001/XMLSchema#string")),
            None,
        ))
        .unwrap();
    assert!(plain == typed, "physical Eq remains RDF-only");
    assert!(source_identity_key(&plain).unwrap() != source_identity_key(&typed).unwrap());
    let a = dataset.internalize_raw(plain).unwrap();
    let b = dataset.internalize_raw(typed).unwrap();
    assert!(a == b);
    assert_eq!(hash_bytes(&a), hash_bytes(a.rdf()));
    assert_eq!(hash_bytes(&b), hash_bytes(b.rdf()));
    assert!(
        source_identity_key(&dataset.externalize_raw(a).unwrap()).unwrap()
            != source_identity_key(&dataset.externalize_raw(b).unwrap()).unwrap()
    );
}
#[test]
fn query_literal_identity_term_only_decoder_refuses_metadata_loss() {
    let prepared = prepare_numeric_query(
        "SELECT (\"a\" AS ?x) WHERE {}",
        ReceivingDataset::new(),
        source_session(&fixture()),
        Vec::new(),
    )
    .unwrap();
    let error = prepared
        .execute_presence()
        .err()
        .expect("source mode must not silently discard metadata");
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::SourceMetadataWouldBeLost)
    ));
}
#[test]
fn query_literal_identity_unreceived_operators_are_gated_before_scan() {
    // These are active interface boundaries, separate from the 12/31 frozen
    // source cases. Do not alter old modes to make these rejections pass.
    for query in [
        "SELECT ?x WHERE { ?s <urn:p> ?x }",
        "SELECT ?x WHERE { ?s (<urn:p>/<urn:q>) ?x }",
        "SELECT ?x WHERE { { VALUES ?x { \"a\" } } { VALUES ?y { \"b\" } } }",
        "SELECT ?x WHERE { VALUES ?x { \"a\" } MINUS { VALUES ?x { \"a\" } } }",
        "SELECT ?x WHERE { VALUES ?x { \"a\" } OPTIONAL { VALUES ?y { \"b\" } } }",
        "SELECT ?x WHERE { SERVICE <urn:service> { VALUES ?x { \"a\" } } }",
        // SourceOrderBy now has its own bounded receiving; legacy OrderBy stays gated.
    ] {
        let dataset = ReceivingDataset::new();
        let prepared = prepare_numeric_query(
            query,
            dataset.clone(),
            source_session(&fixture()),
            Vec::new(),
        )
        .unwrap();
        let error = prepared
            .execute_source_presence()
            .err()
            .unwrap_or_else(|| panic!("unreceived operator compiled: {query}"));
        assert!(
            matches!(
                raw_numeric_fault(&error),
                Some(RawNumericFault::UnsupportedSourceLiteralOperator(_))
            ),
            "{query}: {error}"
        );
        assert_eq!(dataset.trace.scan.get(), 0);
    }
}
#[test]
fn query_literal_identity_final_decoder_preserves_dataset_error_identity_and_fuses() {
    let dataset = ReceivingDataset::new();
    let session = source_session(&fixture());
    let prepared = prepare_numeric_query(
        "SELECT ?x WHERE { VALUES ?x { \"first\" \"second\" } }",
        dataset.clone(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    let first = rows.next().unwrap().unwrap();
    let Some(SolutionValue::Rdf(raw)) = &first.values[0] else {
        panic!("source literal first row")
    };
    let Term::Literal(literal) = raw.rdf().unwrap() else {
        panic!("literal")
    };
    assert_eq!(literal.value(), "first");
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!("source metadata")
    };
    assert!(matches!(metadata.datatype, SourceDatatype::Absent));
    dataset.trace.fail_external.set(true);
    let before = dataset.trace.counts();
    let error = rows
        .next()
        .unwrap()
        .err()
        .expect("actual second decoder must externalize");
    assert_dataset_failure(&error, &dataset);
    assert_eq!(dataset.trace.external.get(), before.1 + 1);
    assert_eq!(dataset.trace.internal.get(), before.0);
    let after = dataset.trace.counts();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(dataset.trace.counts(), after);
    assert_no_flags(&session);
}
#[test]
fn query_literal_identity_known_text_detects_lossy_dataset_roundtrip() {
    let dataset = ReceivingDataset::new();
    let session = source_session(&fixture());
    let prepared = prepare_numeric_query(
        "SELECT (\"a\" AS ?x) WHERE {}",
        dataset.clone(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    dataset.trace.lossy_internal.set(true);
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = rows
        .next()
        .unwrap()
        .err()
        .expect("known source text cannot survive lossy RDF conversion");
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::LossyNumericBackend)
    ));
    let after = dataset.trace.counts();
    assert!(rows.next().is_none());
    assert_eq!(dataset.trace.counts(), after);
    assert_no_flags(&session);
}
#[test]
fn query_literal_identity_cancelled_before_poll_stops_without_extra_conversion() {
    let dataset = ReceivingDataset::new();
    let session = source_session(&fixture());
    let prepared=prepare_numeric_query("SELECT (COUNT(DISTINCT ?x) AS ?n) WHERE { VALUES ?x { \"a\" \"a\"^^<http://www.w3.org/2001/XMLSchema#string> } }",dataset.clone(),Rc::clone(&session),Vec::new()).unwrap();
    let token = prepared.evaluator().dataset.cancellation_token.clone();
    let mut rows = prepared.execute_source_presence().unwrap();
    let before = dataset.trace.counts();
    token.cancel();
    assert!(matches!(
        rows.next(),
        Some(Err(QueryEvaluationError::Cancelled))
    ));
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(dataset.trace.counts(), before);
    assert_no_flags(&session);
}

#[test]
fn literal_identity_core_c01() {
    run_core("LI-C01");
}

#[test]
fn literal_identity_core_c02() {
    run_core("LI-C02");
}

#[test]
fn literal_identity_core_c03() {
    run_core("LI-C03");
}

#[test]
fn literal_identity_core_c04() {
    run_core("LI-C04");
}

#[test]
fn literal_identity_core_c05() {
    run_core("LI-C05");
}

#[test]
fn literal_identity_core_c06() {
    run_core("LI-C06");
}

#[test]
fn literal_identity_core_c07() {
    run_core("LI-C07");
}

#[test]
fn literal_identity_core_c08() {
    run_core("LI-C08");
}

#[test]
fn literal_identity_core_c09() {
    run_core("LI-C09");
}

#[test]
fn literal_identity_core_c10() {
    run_core("LI-C10");
}

#[test]
fn literal_identity_core_c11() {
    run_core("LI-C11");
}

#[test]
fn literal_identity_core_c12() {
    run_core("LI-C12");
}

#[test]
fn query_literal_identity_q01() {
    run_family("LI-Q01");
}

#[test]
fn query_literal_identity_q02() {
    run_family("LI-Q02");
}

#[test]
fn query_literal_identity_q03() {
    run_family("LI-Q03");
}

#[test]
fn query_literal_identity_q04() {
    run_family("LI-Q04");
}

#[test]
fn query_literal_identity_q05() {
    run_family("LI-Q05");
}

#[test]
fn query_literal_identity_q06() {
    run_family("LI-Q06");
}

#[test]
fn query_literal_identity_q07() {
    run_family("LI-Q07");
}

#[test]
fn query_literal_identity_q08() {
    run_family("LI-Q08");
}

#[test]
fn query_literal_identity_q09() {
    run_family("LI-Q09");
}

#[test]
fn query_literal_identity_q10() {
    run_family("LI-Q10");
}

#[test]
fn query_literal_identity_q11() {
    run_family("LI-Q11");
}

#[test]
fn query_literal_identity_q12() {
    run_family("LI-Q12");
}

#[test]
fn query_literal_identity_q13() {
    run_family("LI-Q13");
}

#[test]
fn query_literal_identity_q14() {
    run_family("LI-Q14");
}

#[test]
fn query_literal_identity_q15() {
    run_family("LI-Q15");
}

#[test]
fn query_literal_identity_q16() {
    run_family("LI-Q16");
}

#[test]
fn query_literal_identity_q17() {
    run_family("LI-Q17");
}

// Language supplement 02 is separate from the immutable main 12/31 corpus.
// These expectations were source-derived after static mismatch exposure; they
// are not a blind oracle and no candidate value supplies an expected answer.
const LANGUAGE_FIXTURE: &str = include_str!("fixtures/literal_language_cases.json");
fn language_fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(LANGUAGE_FIXTURE.as_bytes())),
        "56413e8e012a89e9ee80663191c90b8a950d67a796c929d5d37d43eecb7cf476"
    );
    let root: Value = serde_json::from_str(LANGUAGE_FIXTURE).unwrap();
    assert_eq!(
        root["source_record_id"],
        "LITERAL-LANGUAGE-SUPPLEMENT-FROZEN-02"
    );
    assert_eq!(
        root["source_record_sha256"],
        "7096daa56ce66da5fe842ecc9f0a970f74021b37ac430744f6e1d7824bd66523"
    );
    assert_eq!(root["parent_record_sha256"], ORACLE_SHA);
    let payload: serde_json::Map<String, Value> = [
        "explicit_environment",
        "rules",
        "core_cases",
        "query_cases",
        "implementation_error_contract",
        "unknown_or_excluded",
    ]
    .into_iter()
    .map(|key| (key.to_owned(), root[key].clone()))
    .collect();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&Value::Object(payload))).unwrap())
        ),
        "70725421a0d44d3d2f32dc6cf446df8ec96c76bbe328c99e74253d23b798fb63"
    );
    assert_eq!(root["core_cases"].as_array().unwrap().len(), 17);
    assert_eq!(root["query_cases"].as_array().unwrap().len(), 17);
    root
}
fn language_session(root: &Value) -> Rc<QueryNumericSession> {
    let parent = fixture();
    for key in [
        "literal_semantics",
        "numeric_capabilities",
        "group_semantics",
        "normalize_literals",
    ] {
        assert_eq!(
            root["explicit_environment"][key],
            parent["explicit_environment"][key]
        );
    }
    source_session(&parent)
}
fn language_core_case(id: &str) {
    let root = language_fixture();
    let case = root["core_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap();
    let session = language_session(&root);
    if case["operation"] == "source_identity_comparison" {
        for pair in case["pairs"].as_array().unwrap() {
            let left = session
                .construct_source_direct_string(
                    "a",
                    None,
                    Some(text(pair, "left_language").to_owned()),
                    true,
                )
                .unwrap();
            let right = session
                .construct_source_direct_string(
                    "a",
                    None,
                    Some(text(pair, "right_language").to_owned()),
                    true,
                )
                .unwrap();
            assert_eq!(describe(&left)["original_language"], pair["left_language"]);
            assert_eq!(
                describe(&right)["original_language"],
                pair["right_language"]
            );
            let left_key = source_identity_key(&left).unwrap();
            let right_key = source_identity_key(&right).unwrap();
            assert_eq!(left_key == right_key, pair["equal"].as_bool().unwrap());
            if pair["hash_equal"] == true {
                assert_eq!(hash_bytes(&left_key), hash_bytes(&right_key));
            } else {
                assert!(
                    pair["hash_equal"].is_null(),
                    "unequal identity does not require unequal hashes"
                );
            }
        }
    } else {
        let input = &case["input"];
        let expected = &case["expected"];
        assert!(input["datatype"].is_null());
        let result = session.construct_source_direct_string(
            text(input, "lexical"),
            None,
            Some(text(input, "language").to_owned()),
            input["normalize"].as_bool().unwrap(),
        );
        match text(expected, "status") {
            "ok" => {
                let raw = result.expect("source constructor accepts this language");
                assert_eq!(describe(&raw), expected["constructed"], "{id}");
                let copied = session.copy_source_literal(&raw).unwrap();
                assert_eq!(describe(&copied), expected["copied"], "{id}");
                let raw_key = source_identity_key(&raw).unwrap();
                let copy_key = source_identity_key(&copied).unwrap();
                assert_eq!(
                    raw_key == copy_key,
                    expected["source_identity_equal_to_copy"].as_bool().unwrap()
                );
                assert!(expected["hash_equal_to_copy"].as_bool().unwrap());
                assert_eq!(hash_bytes(&raw_key), hash_bytes(&copy_key));
                // Exercise the real codec as well as the factory. In particular
                // the accepted direct final LF must not disappear in transport.
                let dataset = NumericDataset::new(ReceivingDataset::new());
                let term = dataset.internalize_raw(raw).unwrap();
                assert_eq!(hash_bytes(&term), hash_bytes(term.rdf()));
                let received = dataset.externalize_raw(term).unwrap();
                assert_eq!(
                    describe(&received),
                    expected["constructed"],
                    "{id} Dataset roundtrip"
                );
                assert_eq!(source_identity_key(&received).unwrap(), raw_key);
            }
            "source_exception" => {
                assert_eq!(expected["source_exception"], "ValueError");
                assert_eq!(expected["stage"], "language_validation_before_value_cast");
                assert!(
                    matches!(
                        result,
                        Err(RawNumericFault::SourceLiteralLanguageValueError)
                    ),
                    "{id}: {result:?}"
                );
            }
            other => panic!("unhandled language expectation {other}"),
        }
        assert_eq!(expected["flags"], json!([]));
    }
    assert_no_flags(&session);
}
fn language_describe(raw: &RawExpressionValue) -> Value {
    let mut result = describe(raw);
    if let NumericCacheState::Known(cache) = raw.numeric() {
        match cache.value() {
            NumericValue::Integer(value) => {
                // Supplement Q07 asks for the cached value in addition to RDF.
                // Read the actual Integer; never re-parse the RDF lexical text.
                result
                    .as_object_mut()
                    .unwrap()
                    .insert("cache_value".into(), json!(value.to_string()));
            }
            _ => panic!("language supplement has no noninteger numeric result"),
        }
    }
    result
}
fn language_row_json(row: SourcePresenceSolution) -> Value {
    let mut values = serde_json::Map::new();
    for (variable, cell) in row.variables.iter().zip(row.values) {
        match cell {
            None => (),
            Some(SolutionValue::Rdf(raw)) => {
                values.insert(variable.as_str().into(), language_describe(&raw));
            }
            _ => panic!("language supplement expected a Literal binding"),
        }
    }
    Value::Object(values)
}
fn language_query_case(id: &str) {
    let root = language_fixture();
    let case = root["query_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap();
    let session = language_session(&root);
    let dataset = ReceivingDataset::new();
    assert_eq!(case["initial"], json!([]));
    let prepared = prepare_numeric_query(
        text(case, "query"),
        dataset.clone(),
        Rc::clone(&session),
        Vec::new(),
    );
    let expected = &case["expected"];
    match text(expected, "status") {
        "ok" => {
            let mut rows = prepared
                .expect("real private source parser")
                .execute_source_presence()
                .expect("real bound evaluator compilation");
            let actual = rows
                .by_ref()
                .map(|row| language_row_json(row.expect("source-derived successful row")))
                .collect::<Vec<_>>();
            assert_eq!(json!(actual), expected["rows"], "{id}");
            let after = dataset.trace.counts();
            assert!(rows.next().is_none());
            assert!(rows.next().is_none());
            assert_eq!(dataset.trace.counts(), after);
        }
        "syntax_error" => {
            assert_eq!(expected["source_exception"], "ParseException");
            assert_eq!(expected["stage"], "parse_query");
            match prepared {
                Err(super::numeric_query::NumericPrepareError::Syntax(_)) => (),
                Err(other) => panic!("{id}: wrong preparation error {other}"),
                Ok(_) => panic!("{id}: invalid source query syntax was accepted"),
            }
            assert_eq!(
                dataset.trace.counts(),
                (0, 0, 0),
                "syntax failure cannot run an executor"
            );
        }
        other => panic!("unknown language query status {other}"),
    }
    assert_eq!(dataset.trace.scan.get(), 0);
    assert_eq!(expected["flags"], json!([]));
    assert_no_flags(&session);
}
#[test]
fn literal_language_fixture_preserves_parent_and_supplement_bytes() {
    let _ = fixture();
    let _ = language_fixture();
}
#[test]
fn literal_language_default_parser_keeps_its_existing_validation() {
    let root = language_fixture();
    for id in ["LL-Q01", "LL-Q02"] {
        let case = root["query_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == id)
            .unwrap();
        let query = text(case, "query");
        assert!(
            spargebra::SparqlParser::new().parse_query(query).is_err(),
            "default parser {id}"
        );
        // Enabling numeric/source-group capabilities alone must not relax
        // literal syntax: every pre-existing constructor stays RdfIdentityV1.
        let session = Rc::new(
            QueryNumericSession::new_with_group_semantics(
                DecimalContext::default(),
                NumericLimits::default(),
                None,
                true,
                QueryNumericCapabilities::MixedBinary64V1,
                QueryGroupSemantics::SourceAggregateV1,
            )
            .unwrap(),
        );
        assert_eq!(
            session.literal_semantics(),
            QueryLiteralSemantics::RdfIdentityV1
        );
        match prepare_numeric_query(
            query,
            ReceivingDataset::new(),
            Rc::clone(&session),
            Vec::new(),
        ) {
            Err(super::numeric_query::NumericPrepareError::Syntax(_)) => (),
            Err(other) => panic!("legacy {id}: wrong error {other}"),
            Ok(_) => panic!("legacy {id}: source syntax leaked into old mode"),
        }
        assert_no_flags(&session);
    }
    assert!(
        spargebra::SparqlParser::new()
            .parse_query("SELECT ?x WHERE { VALUES ?x { \"a\"@EN } }")
            .is_ok()
    );
}

#[test]
fn literal_language_core_lc01() {
    language_core_case("LL-C01");
}

#[test]
fn literal_language_core_lc02() {
    language_core_case("LL-C02");
}

#[test]
fn literal_language_core_lc03() {
    language_core_case("LL-C03");
}

#[test]
fn literal_language_core_lc04() {
    language_core_case("LL-C04");
}

#[test]
fn literal_language_core_lc05() {
    language_core_case("LL-C05");
}

#[test]
fn literal_language_core_lc06() {
    language_core_case("LL-C06");
}

#[test]
fn literal_language_core_lc07() {
    language_core_case("LL-C07");
}

#[test]
fn literal_language_core_lc08() {
    language_core_case("LL-C08");
}

#[test]
fn literal_language_core_lc09() {
    language_core_case("LL-C09");
}

#[test]
fn literal_language_core_lc10() {
    language_core_case("LL-C10");
}

#[test]
fn literal_language_core_lc11() {
    language_core_case("LL-C11");
}

#[test]
fn literal_language_core_lc12() {
    language_core_case("LL-C12");
}

#[test]
fn literal_language_core_lc13() {
    language_core_case("LL-C13");
}

#[test]
fn literal_language_core_lc14() {
    language_core_case("LL-C14");
}

#[test]
fn literal_language_core_lc15() {
    language_core_case("LL-C15");
}

#[test]
fn literal_language_core_lc16() {
    language_core_case("LL-C16");
}

#[test]
fn literal_language_core_lc17() {
    language_core_case("LL-C17");
}

#[test]
fn query_literal_language_lq01() {
    language_query_case("LL-Q01");
}

#[test]
fn query_literal_language_lq02() {
    language_query_case("LL-Q02");
}

#[test]
fn query_literal_language_lq03() {
    language_query_case("LL-Q03");
}

#[test]
fn query_literal_language_lq04() {
    language_query_case("LL-Q04");
}

#[test]
fn query_literal_language_lq05() {
    language_query_case("LL-Q05");
}

#[test]
fn query_literal_language_lq06() {
    language_query_case("LL-Q06");
}

#[test]
fn query_literal_language_lq07() {
    language_query_case("LL-Q07");
}

#[test]
fn query_literal_language_lq08() {
    language_query_case("LL-Q08");
}

#[test]
fn query_literal_language_lq09() {
    language_query_case("LL-Q09");
}

#[test]
fn query_literal_language_lq10() {
    language_query_case("LL-Q10");
}

#[test]
fn query_literal_language_lq11() {
    language_query_case("LL-Q11");
}

#[test]
fn query_literal_language_lq12() {
    language_query_case("LL-Q12");
}

#[test]
fn query_literal_language_lq13() {
    language_query_case("LL-Q13");
}

#[test]
fn query_literal_language_lq14() {
    language_query_case("LL-Q14");
}

#[test]
fn query_literal_language_lq15() {
    language_query_case("LL-Q15");
}

#[test]
fn query_literal_language_lq16() {
    language_query_case("LL-Q16");
}

#[test]
fn query_literal_language_lq17() {
    language_query_case("LL-Q17");
}

// Additional active backend-integrity contract. These are not new original
// Python observations and do not change either source/language corpus.
const NONLITERAL_FIXTURE: &str = include_str!("fixtures/literal_nonliteral_cases.json");
fn nonliteral_fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(NONLITERAL_FIXTURE.as_bytes())),
        "b0dae4fc3bbb5f5a24d63236ed5ec4ff5e986f848415c7b447fe4de5eeb668bd"
    );
    let value: Value = serde_json::from_str(NONLITERAL_FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "5b67833998ab6483eb4217eabcd6d58d25ae1504cf85e02540908e1fc5c253b4"
    );
    let payload: serde_json::Map<String, Value> = ["classification", "api", "cases", "invariants"]
        .into_iter()
        .map(|k| (k.to_owned(), value[k].clone()))
        .collect();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&Value::Object(payload))).unwrap())
        ),
        "428fdcd577b2de6f20edb0f88519fd85de62e7e31f1048b13f34e55bc5cfdb76"
    );
    value
}
fn nonliteral_term(value: &Value) -> Term {
    match text(value, "kind") {
        "NamedNode" => NamedNode::new(text(value, "value")).unwrap().into(),
        "BlankNode" => BlankNode::new(text(value, "value")).unwrap().into(),
        other => panic!("nonliteral fixture kind: {other}"),
    }
}
#[derive(Clone)]
struct NonLiteralBackend {
    base: ReceivingDataset,
    replacement: Term,
    replace_on_external: Rc<Cell<Option<usize>>>,
}
impl NonLiteralBackend {
    fn new(replacement: Term, ordinal: Option<usize>) -> Self {
        Self {
            base: ReceivingDataset::new(),
            replacement,
            replace_on_external: Rc::new(Cell::new(ordinal)),
        }
    }
}
impl<'a> QueryableDataset<'a> for NonLiteralBackend {
    type InternalTerm = Term;
    type Error = ConversionFailure;
    fn internal_quads_for_pattern(
        &self,
        _s: Option<&Term>,
        _p: Option<&Term>,
        _o: Option<&Term>,
        _g: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.base.trace.scan.set(self.base.trace.scan.get() + 1);
        std::iter::empty()
    }
    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.base.internalize_term(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        let term = self.base.externalize_term(term)?;
        if self.replace_on_external.get() == Some(self.base.trace.external.get()) {
            Ok(self.replacement.clone())
        } else {
            Ok(term)
        }
    }
}
fn assert_nonliteral_loss(error: &QueryEvaluationError) {
    assert!(
        matches!(
            raw_numeric_fault(error),
            Some(RawNumericFault::LossyNumericBackend)
        ),
        "original typed fault required: {error}"
    );
}
fn run_nonliteral_case(id: &str) {
    let fixture = nonliteral_fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let input = nonliteral_term(&case["input"]);
    let backend = NonLiteralBackend::new(
        nonliteral_term(&case["backend_replacement"]),
        case["backend_change_on_externalize_call"]
            .as_u64()
            .map(|x| x as usize),
    );
    let dataset = if case["constructor"] == "new" {
        NumericDataset::new(backend.clone())
    } else {
        NumericDataset::new_with_literal_semantics(
            backend.clone(),
            QueryLiteralSemantics::SourceLiteralV1,
        )
    };
    let expected = &case["expected"];
    let internal = dataset.internalize_raw(RawExpressionValue::from_import(input.clone()));
    if expected["stage"] == "internalize_roundtrip" {
        assert_nonliteral_loss(
            &internal
                .err()
                .expect("known nonliteral readback must reject changed backend"),
        );
    } else {
        let internal = internal.unwrap();
        if let Some(count) = expected["externalize_calls_after_internalize"].as_u64() {
            assert_eq!(backend.base.trace.external.get(), count as usize);
        }
        if case["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "source_identity_key")
        {
            let key = internal.source_identity_key().unwrap();
            assert_eq!(
                key,
                source_identity_key(&RawExpressionValue::from_import(input)).unwrap()
            );
            assert_eq!(
                backend.base.trace.external.get(),
                expected["externalize_calls_after_key"].as_u64().unwrap() as usize,
                "key lookup adds no Dataset conversion"
            );
        }
        let result = dataset.externalize_raw(internal);
        if expected["outcome"] == "error" {
            assert_nonliteral_loss(&result.err().expect("changed output must be rejected"));
        } else {
            let raw = result.unwrap();
            assert_eq!(raw.rdf().unwrap(), &nonliteral_term(&expected["rdf"]));
            assert!(matches!(
                raw.literal_source(),
                LiteralSourceState::NotLiteral
            ));
            assert!(matches!(raw.numeric(), NumericCacheState::NotNumeric));
        }
    }
    assert_eq!(
        backend.base.trace.internal.get(),
        expected["internalize_calls"].as_u64().unwrap() as usize
    );
    assert_eq!(
        backend.base.trace.external.get(),
        expected["externalize_calls"].as_u64().unwrap() as usize
    );
    assert_eq!(backend.base.trace.scan.get(), 0);
}
#[test]
fn literal_nonliteral_backend_n1() {
    run_nonliteral_case("NLB-N1");
}
#[test]
fn literal_nonliteral_backend_n2() {
    run_nonliteral_case("NLB-N2");
}
#[test]
fn literal_nonliteral_backend_n3() {
    run_nonliteral_case("NLB-N3");
}
#[test]
fn literal_nonliteral_backend_n4() {
    run_nonliteral_case("NLB-N4");
}
#[test]
fn literal_nonliteral_backend_b1() {
    run_nonliteral_case("NLB-B1");
}
#[test]
fn literal_nonliteral_backend_b2() {
    run_nonliteral_case("NLB-B2");
}
#[test]
fn literal_nonliteral_backend_b3() {
    run_nonliteral_case("NLB-B3");
}
#[test]
fn literal_nonliteral_backend_b4() {
    run_nonliteral_case("NLB-B4");
}

#[test]
fn query_literal_nonliteral_initial_uses_source_backend_policy_and_fuses() {
    // Actual prepared initial NamedNode/BlankNode transport, then DISTINCT and
    // final source decoder. This is an author integration control for the
    // supplemental integrity policy; not an extra original-library oracle row.
    for (input, replacement) in [
        (
            Term::from(NamedNode::new("urn:source:a").unwrap()),
            Term::from(NamedNode::new("urn:source:b").unwrap()),
        ),
        (
            Term::from(BlankNode::new("source-a").unwrap()),
            Term::from(BlankNode::new("source-b").unwrap()),
        ),
    ] {
        let backend = NonLiteralBackend::new(replacement, None);
        let session = source_session(&fixture());
        let prepared = prepare_numeric_query(
            "SELECT DISTINCT ?x WHERE {}",
            backend.clone(),
            session,
            vec![NumericInitialBinding {
                variable: Variable::new("x").unwrap(),
                value: NumericInitialValue::UnknownNormalized(input),
            }],
        )
        .unwrap();
        assert_eq!(
            backend.base.trace.counts(),
            (1, 1, 0),
            "prepared source nonliteral must perform real readback"
        );
        backend.replace_on_external.set(Some(2));
        let mut rows = prepared.execute_source_presence().unwrap();
        assert_nonliteral_loss(&rows.next().unwrap().err().expect("final decoder loss"));
        let stopped = backend.base.trace.counts();
        assert_eq!(stopped, (1, 2, 0));
        assert!(rows.next().is_none());
        assert!(rows.next().is_none());
        assert_eq!(backend.base.trace.counts(), stopped);
    }
}
#[test]
fn query_literal_nonliteral_values_uses_source_backend_policy() {
    // Compilation must check SourceValues terms through the same real codec.
    // The gate is the ordinary compile-time VALUES internalization boundary.
    let backend = NonLiteralBackend::new(NamedNode::new("urn:source:b").unwrap().into(), Some(1));
    let prepared = prepare_numeric_query(
        "SELECT DISTINCT ?x WHERE { VALUES ?x { <urn:source:a> } }",
        backend.clone(),
        source_session(&fixture()),
        Vec::new(),
    )
    .unwrap();
    let error = prepared
        .execute_source_presence()
        .err()
        .expect("SourceValues readback cannot return a tuple stream after mismatch");
    assert_nonliteral_loss(&error);
    assert_eq!(backend.base.trace.counts(), (1, 1, 0));
}

// Source-derived supplementary controls, frozen before candidate execution:
// LITERAL-VALUES-REPRESENTATIVE-SUPPLEMENT-01, SHA256
// 5d1da98bc5825595638e8d0e6e13815431c33e33548fcfc0435df52ef6e8d965.
// evaluate.py:255-267 assigns each VALUES object through QueryContext;
// sparql.py:413-417 still assigns that object when its identity is equal.
// A longer initial context must not choose which equal object survives.
fn receive_values_representative(
    query: &str,
    mut bindings: Vec<NumericInitialBinding>,
    hidden: bool,
) -> SourcePresenceSolution {
    if hidden {
        bindings.push(NumericInitialBinding {
            variable: Variable::new("hidden").unwrap(),
            value: NumericInitialValue::SourceDirectString {
                lexical: "context".into(),
                datatype: None,
                language: None,
                normalize: true,
            },
        });
    }
    let session = source_session(&fixture());
    let dataset = ReceivingDataset::new();
    let prepared =
        prepare_numeric_query(query, dataset.clone(), Rc::clone(&session), bindings).unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    let row = rows.next().expect("one source VALUES row").unwrap();
    assert!(rows.next().is_none());
    assert_eq!(dataset.trace.scan.get(), 0);
    assert_no_flags(&session);
    row
}

#[test]
fn query_literal_source_values_overrides_equal_initial_language_object() {
    for hidden in [false, true] {
        let row = receive_values_representative(
            "SELECT ?x WHERE { VALUES ?x { \"v\"@en } }",
            vec![NumericInitialBinding {
                variable: Variable::new("x").unwrap(),
                value: NumericInitialValue::SourceDirectString {
                    lexical: "v".into(),
                    datatype: None,
                    language: Some("EN".into()),
                    normalize: true,
                },
            }],
            hidden,
        );
        assert_eq!(
            row_json(row),
            json!({"x": {"kind":"literal", "lexical":"v",
                "source_datatype":{"state":"absent"}, "original_language":"en",
                "ill_typed":null, "cache_family":"String"}}),
            "VALUES supplies the original-language representative; hidden={hidden}"
        );
    }
}

#[test]
fn query_literal_source_values_overrides_equal_initial_numeric_cache() {
    for hidden in [false, true] {
        let row = receive_values_representative(
            "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> SELECT ?x (100 / ?x AS ?v) WHERE { VALUES ?x { \"100\"^^xsd:decimal } }",
            vec![NumericInitialBinding {
                variable: Variable::new("x").unwrap(),
                value: NumericInitialValue::SourceDirectString {
                    lexical: "1E+2".into(),
                    datatype: Some(node("http://www.w3.org/2001/XMLSchema#decimal")),
                    language: None,
                    normalize: true,
                },
            }],
            hidden,
        );
        for (variable, lexical, coefficient) in [("x", "100", "100"), ("v", "1", "1")] {
            let index = row
                .variables
                .iter()
                .position(|value| value.as_str() == variable)
                .unwrap();
            let Some(SolutionValue::Rdf(raw)) = &row.values[index] else {
                panic!("selected numeric source cell required")
            };
            let Term::Literal(literal) = raw.rdf().unwrap() else {
                panic!("numeric RDF literal required")
            };
            assert_eq!(literal.value(), lexical, "{variable}; hidden={hidden}");
            assert_eq!(
                literal.datatype().as_str(),
                "http://www.w3.org/2001/XMLSchema#decimal"
            );
            let NumericCacheState::Known(cache) = raw.numeric() else {
                panic!("selected numeric cache required")
            };
            let NumericValue::Decimal(semantica_numeric::DecimalValue::Finite {
                negative,
                coefficient: actual_coefficient,
                exponent,
            }) = cache.value()
            else {
                panic!("finite Decimal cache required")
            };
            assert!(!negative);
            assert_eq!(
                actual_coefficient.to_string(),
                coefficient,
                "{variable}; hidden={hidden}"
            );
            assert_eq!(exponent.to_string(), "0", "{variable}; hidden={hidden}");
            let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                panic!("source literal metadata required")
            };
            assert_eq!(metadata.ill_typed, SourceFact::Known(None));
        }
    }
}
