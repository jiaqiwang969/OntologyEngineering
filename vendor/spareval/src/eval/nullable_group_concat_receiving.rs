//! Receiving for immutable source-derived GROUP_CONCAT examples and separately
//! identified active safety contracts. Expected values are mechanically copied
//! from frozen records; this adapter author reviewed, but did not author, them.
//! Queries use the sealed prepare/execute_source_presence entry. Explicit
//! changing evaluator sequences are confined to the frozen accumulator inputs.
use super::nullable::{
    NullableAccumulator, NullableAggregateError, NullableAggregateEvaluator,
    nullable_aggregate_error,
};
use super::numeric_query::prepare_numeric_query;
use super::*;
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    ParsedSourceLiteral, QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities,
    QueryNumericSession, RawExpressionValue, RawNumericFault, SourceDatatype, SourceFact,
    SourceLiteralCacheKind, SourceLiteralMetadata, SourceLiteralOrigin, raw_numeric_fault,
    source_identity_key,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use semantica_numeric::{
    DecimalContext, DecimalSignal, DecimalSignals, NumericFault, NumericLimits, NumericValue,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

const FIXTURE: &str = include_str!("fixtures/source_group_concat_cases.json");
const ORACLE_SHA: &str = "cd764129bcd981247cd7fc31d99f7d5cb6397a960a3e507358fd69e331fa0655";
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
        value => value.clone(),
    }
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "03e9be879107cbc3366dacb21770ad758c854143d540de0db30191f6ef5de300"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(root["source_record_sha256"], ORACLE_SHA);
    assert_eq!(
        root["reused_source_record_sha256"],
        "19ebfb25e122b559dffd1eccbcc384a48d007e7b8cc0f9c6c68c6dd75681c679"
    );
    let payload = Value::Object(
        [
            "config",
            "text_cases",
            "repr_cases",
            "protocol_cases",
            "query_cases",
            "active_safety_cases",
            "unclosed",
            "expected_comparison",
            "counts",
            "reused_cases",
        ]
        .into_iter()
        .map(|key| (key.to_owned(), root[key].clone()))
        .collect(),
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "ece051de1bb60bc3158e4b6a34b2e4698b52490527ebe0e00047d2c1da5bb9b6"
    );
    for (key, count) in [
        ("text_cases", 10),
        ("repr_cases", 5),
        ("protocol_cases", 6),
        ("query_cases", 19),
        ("active_safety_cases", 9),
        ("reused_cases", 7),
    ] {
        assert_eq!(root[key].as_array().unwrap().len(), count);
    }
    assert_eq!(root["config"]["numeric_capability"], "MixedBinary64V1");
    assert_eq!(root["config"]["group_semantics"], "SourceAggregateV1");
    assert_eq!(root["config"]["literal_semantics"], "SourceLiteralV1");
    root
}
fn case(section: &str, id: &str) -> Value {
    fixture()[section]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
        .clone()
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing string {key}: {value}"))
}
fn node(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}
fn try_session(run: &Value) -> Result<Rc<QueryNumericSession>, RawNumericFault> {
    let mut limits = NumericLimits::default();
    if let Some(fields) = run["limits"].as_object() {
        for (key, value) in fields {
            let value = usize::try_from(value.as_u64().unwrap()).unwrap();
            match key.as_str() {
                "max_input_characters" => limits.max_input_characters = value,
                "max_output_characters" => limits.max_output_characters = value,
                other => panic!("unreceived limit {other}"),
            }
        }
    }
    QueryNumericSession::new_with_literal_semantics(
        DecimalContext::default(),
        limits,
        None,
        true,
        QueryNumericCapabilities::MixedBinary64V1,
        QueryGroupSemantics::SourceAggregateV1,
        QueryLiteralSemantics::SourceLiteralV1,
    )
    .map(Rc::new)
}
fn session(run: &Value) -> Rc<QueryNumericSession> {
    try_session(run).unwrap()
}
fn assert_flags(session: &QueryNumericSession, run: &Value) {
    if run["id"] == "GC05" || run["expected"]["decimal_flags_required"].is_array() {
        assert!(session.flags().contains(DecimalSignal::DivisionByZero));
    } else {
        assert_eq!(session.flags(), DecimalSignals::EMPTY, "{}", run["id"]);
    }
}
fn literal(lexical: &str, datatype: Option<NamedNode>, language: Option<&str>) -> Literal {
    if let Some(language) = language {
        assert!(datatype.is_none());
        Literal::new_language_tagged_literal(lexical, language).unwrap()
    } else if let Some(datatype) = datatype {
        Literal::new_typed_literal(lexical, datatype)
    } else {
        Literal::new_simple_literal(lexical)
    }
}
fn error_input(class: &str, argument: &Value) -> ErrorValue {
    let (kind, class) = match class {
        "SPARQLError" => (SparqlErrorKind::OtherSparql, SourceExceptionClass::Sparql),
        "SPARQLTypeError" => (SparqlErrorKind::Type, SourceExceptionClass::SparqlType),
        "NotBoundError" => (SparqlErrorKind::NotBound, SourceExceptionClass::NotBound),
        other => panic!("unreceived source exception {other}"),
    };
    ErrorValue::with_source(
        kind,
        "receiving diagnostic intentionally differs from source text",
        SourceException {
            class,
            argument: argument
                .as_str()
                .map_or(SourceExceptionArgument::None, |s| {
                    SourceExceptionArgument::Text(s.into())
                }),
        },
    )
}
fn input(session: &QueryNumericSession, value: &Value) -> SolutionValue<RawExpressionValue> {
    match text(value, "kind") {
        "ExplicitNone" => SolutionValue::ExplicitNone,
        "ReturnedError" => SolutionValue::ReturnedError(error_input(
            text(value, "source_class"),
            &value["source_constructor_argument"],
        )),
        "named_node" => SolutionValue::Rdf(RawExpressionValue::from_import(
            node(text(value, "value")).into(),
        )),
        "blank_node" => SolutionValue::Rdf(RawExpressionValue::from_import(
            BlankNode::new(text(value, "value")).unwrap().into(),
        )),
        "literal" => {
            let dt = value["source_datatype"].as_str().map(node);
            let lang = value["language_original"].as_str().map(str::to_owned);
            let rdf = literal(text(value, "lexical"), dt.clone(), lang.as_deref());
            let raw = if value["metadata_origin"] == "UnknownNormalized" {
                RawExpressionValue::from_import(rdf.into())
            } else if let Some(bits) = value["numeric_cache"]["Binary64_bits"].as_str() {
                session
                    .known_source_cached(
                        rdf.into(),
                        NumericValue::Binary64(f64::from_bits(
                            u64::from_str_radix(bits, 16).unwrap(),
                        )),
                        SourceLiteralMetadata {
                            datatype: dt.map_or(SourceDatatype::Absent, SourceDatatype::Specified),
                            language_original: SourceFact::Known(lang),
                            ill_typed: SourceFact::Known(None),
                            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
                        },
                    )
                    .unwrap()
            } else {
                let origin = if dt.is_some() {
                    SourceLiteralOrigin::QueryTypedExistingLiteral
                } else if lang.is_some() {
                    SourceLiteralOrigin::QueryLanguageExistingLiteral
                } else {
                    SourceLiteralOrigin::QueryPlain
                };
                session
                    .construct_source_literal(&ParsedSourceLiteral {
                        occurrence: None,
                        rdf,
                        datatype: dt.map_or(SourceDatatype::Absent, SourceDatatype::Specified),
                        language_original: lang,
                        origin,
                    })
                    .unwrap()
            };
            if let Some(integer) = value["numeric_cache"]["Integer"].as_str() {
                let NumericCacheState::Known(cache) = raw.numeric() else {
                    panic!("input cache lost")
                };
                let NumericValue::Integer(actual) = cache.value() else {
                    panic!("input cache family")
                };
                assert_eq!(actual.to_string(), integer);
            }
            if let Some(bits) = value["numeric_cache"]["Binary64_bits"].as_str() {
                let NumericCacheState::Known(cache) = raw.numeric() else {
                    panic!("input cache lost")
                };
                let NumericValue::Binary64(actual) = cache.value() else {
                    panic!("input cache family")
                };
                assert_eq!(actual.to_bits(), u64::from_str_radix(bits, 16).unwrap());
            }
            SolutionValue::Rdf(raw)
        }
        other => panic!("unreceived input {other}"),
    }
}
fn run_text(id: &str, repr: bool) {
    let run = case(if repr { "repr_cases" } else { "text_cases" }, id);
    let session = session(&run);
    let value = input(&session, &run["input"]);
    if repr {
        assert_eq!(
            session.source_repr(&value).unwrap(),
            text(&run, "expected_repr")
        );
    } else {
        assert_eq!(
            session.source_text(&value).unwrap(),
            text(&run, "expected_text")
        );
    }
    assert_flags(&session, &run);
}
fn source_datatype(raw: &RawExpressionValue) -> Value {
    let LiteralSourceState::Literal(meta) = raw.literal_source() else {
        panic!("expected source Literal")
    };
    match &meta.datatype {
        SourceDatatype::Absent => Value::Null,
        SourceDatatype::Specified(dt) => json!(dt.as_str()),
        SourceDatatype::Unknown => panic!("output source datatype unknown"),
    }
}
fn assert_literal(raw: &RawExpressionValue, expected: &Value) {
    let Term::Literal(rdf) = raw.rdf().unwrap() else {
        panic!("expected literal output")
    };
    let LiteralSourceState::Literal(meta) = raw.literal_source() else {
        panic!("source metadata lost")
    };
    for (key, wanted) in expected.as_object().unwrap() {
        let actual = match key.as_str() {
            "kind" => json!("literal"),
            "lexical" => json!(rdf.value()),
            "source_datatype" | "datatype" => source_datatype(raw),
            "language_original" | "language" => match &meta.language_original {
                SourceFact::Known(value) => json!(value),
                SourceFact::Unknown => panic!("source language unknown"),
            },
            "ill_typed" => match &meta.ill_typed {
                SourceFact::Known(value) => json!(value),
                SourceFact::Unknown => panic!("source ill_typed unknown"),
            },
            "cache_kind" => match &meta.cached_value_kind {
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
                SourceFact::Known(SourceLiteralCacheKind::String) => json!("String"),
                SourceFact::Known(SourceLiteralCacheKind::None) => json!("None"),
                SourceFact::Known(SourceLiteralCacheKind::Numeric) => json!("Numeric"),
                SourceFact::Known(SourceLiteralCacheKind::ErrorObject) => {
                    panic!("GROUP_CONCAT receiving does not construct Literal(error)")
                }
                SourceFact::Known(SourceLiteralCacheKind::Boolean(_)) => {
                    panic!("GROUP_CONCAT receiving does not construct BooleanLiteral tokens")
                }
                SourceFact::Unknown => panic!("source cache kind unknown"),
            },
            other => panic!("unreceived output field {other}"),
        };
        assert_eq!(&actual, wanted, "literal field {key}");
    }
}
fn assert_row(row: SourcePresenceSolution, expected: &Value) {
    let actual = row
        .variables
        .iter()
        .zip(&row.values)
        .filter_map(|(variable, cell)| cell.as_ref().map(|cell| (variable.as_str(), cell)))
        .collect::<BTreeMap<_, _>>();
    let wanted = expected.as_object().unwrap();
    let mut wanted_names = wanted.keys().map(String::as_str).collect::<Vec<_>>();
    wanted_names.sort_unstable();
    assert_eq!(
        actual.keys().copied().collect::<Vec<_>>(),
        wanted_names,
        "bound variable set"
    );
    for (name, wanted) in wanted {
        let SolutionValue::Rdf(raw) = actual[name.as_str()] else {
            panic!("expected RDF output")
        };
        assert_literal(raw, wanted);
    }
}
fn rdf_input(value: &Value) -> Term {
    match text(value, "kind") {
        "literal" => {
            Literal::new_typed_literal(text(value, "lexical"), node(text(value, "datatype"))).into()
        }
        "blank_node" => BlankNode::new(text(value, "value")).unwrap().into(),
        other => panic!("unreceived imported input {other}"),
    }
}
fn initial(run: &Value) -> Vec<NumericInitialBinding> {
    run["initial"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|binding| NumericInitialBinding {
            variable: Variable::new(text(binding, "variable")).unwrap(),
            value: match text(binding, "origin") {
                "UnknownNormalized" => {
                    NumericInitialValue::UnknownNormalized(rdf_input(&binding["rdf"]))
                }
                "DirectString" => NumericInitialValue::SourceDirectString {
                    lexical: text(binding, "lexical").into(),
                    datatype: binding["datatype"].as_str().map(node),
                    language: binding["language"].as_str().map(str::to_owned),
                    normalize: true,
                },
                other => panic!("unreceived initial origin {other}"),
            },
        })
        .collect()
}
#[derive(Debug)]
struct FailureIdentity;
#[derive(Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GROUP_CONCAT receiving Dataset fault")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct Trace {
    internal: Cell<usize>,
    external: Cell<usize>,
    scans: Cell<usize>,
    armed: Cell<bool>,
    matching_external: Cell<usize>,
    matching_internal: Cell<usize>,
}
impl Trace {
    fn counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.internal.get(),
            self.external.get(),
            self.scans.get(),
            self.matching_external.get(),
            self.matching_internal.get(),
        )
    }
}
#[derive(Clone)]
struct ReceivingDataset {
    trace: Rc<Trace>,
    identity: Arc<FailureIdentity>,
    fault: Option<(Term, usize)>,
}
impl ReceivingDataset {
    fn new(run: &Value) -> Self {
        let fault = run.get("dataset_fault").map(|f| {
            assert_eq!(f["arm"], "after_prepare_before_execute");
            (
                Literal::new_typed_literal(
                    text(&f["matching_rdf"], "lexical"),
                    node(text(&f["matching_rdf"], "physical_datatype")),
                )
                .into(),
                f["fail_matching_externalize_call"].as_u64().unwrap() as usize,
            )
        });
        Self {
            trace: Rc::new(Trace::default()),
            identity: Arc::new(FailureIdentity),
            fault,
        }
    }
    fn arm(&self) {
        self.trace.armed.set(true);
    }
}
impl<'a> QueryableDataset<'a> for ReceivingDataset {
    type InternalTerm = Term;
    type Error = ConversionFailure;
    fn internal_quads_for_pattern(
        &self,
        _subject: Option<&Term>,
        _predicate: Option<&Term>,
        _object: Option<&Term>,
        _graph_name: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.trace.scans.set(self.trace.scans.get() + 1);
        empty()
    }
    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.trace.internal.set(self.trace.internal.get() + 1);
        if self.trace.armed.get()
            && self
                .fault
                .as_ref()
                .is_some_and(|(target, _)| target == &term)
        {
            self.trace
                .matching_internal
                .set(self.trace.matching_internal.get() + 1);
        }
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.trace.external.set(self.trace.external.get() + 1);
        if let Some((target, ordinal)) = &self.fault {
            if self.trace.armed.get() && target == &term {
                let count = self.trace.matching_external.get() + 1;
                self.trace.matching_external.set(count);
                if count == *ordinal {
                    return Err(ConversionFailure(Arc::clone(&self.identity)));
                }
            }
        }
        Ok(term)
    }
}
fn assert_dataset_error(error: &QueryEvaluationError, dataset: &ReceivingDataset) {
    let QueryEvaluationError::Dataset(error) = error else {
        panic!("expected real Dataset error: {error}")
    };
    let error = error
        .downcast_ref::<ConversionFailure>()
        .expect("original Dataset type");
    assert!(Arc::ptr_eq(&error.0, &dataset.identity));
}
fn assert_query_error(error: &QueryEvaluationError, run: &Value, dataset: &ReceivingDataset) {
    match text(run, "id") {
        "GA02" | "GA05" => assert!(
            matches!(
                raw_numeric_fault(error),
                Some(RawNumericFault::Core(NumericFault::ResourceLimit { .. }))
            ),
            "{error}"
        ),
        "GA07" => assert!(
            matches!(
                raw_numeric_fault(error),
                Some(RawNumericFault::UnknownSourceLiteralIdentity)
            ),
            "{error}"
        ),
        "GA09" => assert_dataset_error(error, dataset),
        "GC04" => {
            let Some(NullableAggregateError::UncaughtSparql(error)) =
                nullable_aggregate_error(error)
            else {
                panic!("expected uncaught source error: {error}")
            };
            assert_eq!(error.kind(), SparqlErrorKind::NotBound);
        }
        other => panic!("unexpected query failure for {other}: {error}"),
    }
}
fn receive_query(run: &Value) {
    let session = session(run);
    let dataset = ReceivingDataset::new(run);
    let prepared = prepare_numeric_query(
        text(run, "query"),
        dataset.clone(),
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    assert_eq!(session.flags(), DecimalSignals::EMPTY);
    if run.get("dataset_fault").is_some() {
        dataset.arm();
    }
    let mut rows = prepared
        .execute_source_presence()
        .expect("real source compiler");
    // A query iterator is lazy, including GROUP_CONCAT preparation and finish.
    assert_eq!(session.flags(), DecimalSignals::EMPTY);
    let expected = &run["expected"];
    let prefix = expected
        .get("rows")
        .or_else(|| expected.get("rows_before_error"))
        .map(|v| v.as_array().unwrap().clone())
        .unwrap_or_else(|| {
            run.get("expected_row")
                .map(|r| vec![r.clone()])
                .unwrap_or_default()
        });
    for wanted in &prefix {
        assert_row(
            rows.next().expect("frozen row").expect("successful prefix"),
            wanted,
        );
    }
    let fails = run.get("expected_error").is_some()
        || matches!(run["id"].as_str(), Some("GA02" | "GA05" | "GA07" | "GA09"));
    if fails {
        let error = rows
            .next()
            .expect("one frozen error")
            .err()
            .expect("error must not become row");
        assert_query_error(&error, run, &dataset);
    } else {
        assert!(rows.next().is_none(), "unexpected extra row: {}", run["id"]);
    }
    let counts = dataset.trace.counts();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(
        dataset.trace.counts(),
        counts,
        "fused iterator did further Dataset work"
    );
    assert_eq!(dataset.trace.scans.get(), 0, "VALUES query must not scan");
    if run["id"] == "GA09" {
        assert_eq!(dataset.trace.matching_external.get(), 2);
        // A completed one-item group would internalize its plain 'a' result.
        // This counter is armed after initial construction, not a fake finish hook.
        assert_eq!(dataset.trace.matching_internal.get(), 0);
    }
    assert_flags(&session, run);
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
fn accumulator<'b, 'a, T>(
    wrapper: &'b mut AccumulatorWrapper<'a, T>,
) -> &'b mut NullableAccumulator<'a, T> {
    match wrapper {
        AccumulatorWrapper::Nullable(acc)
        | AccumulatorWrapper::SourceDistinct {
            accumulator: acc, ..
        } => acc,
        _ => panic!("real source accumulator required"),
    }
}
fn receive_schedule(run: &Value) {
    let reused_errors = run["id"] == "GC05";
    let checkpoints = run["expected"].get("checkpoints").unwrap_or(&Value::Null);
    if checkpoints.is_null() && !reused_errors {
        return;
    }
    let session = session(run);
    let prepared = prepare_numeric_query(
        text(run, "query"),
        ReceivingDataset::new(run),
        Rc::clone(&session),
        initial(run),
    )
    .unwrap();
    let GraphPattern::SourceGroup {
        inner, aggregates, ..
    } = source_group(prepared.plan()).unwrap()
    else {
        unreachable!()
    };
    let mut variables = Vec::new();
    let (child, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(inner, &mut variables);
    let expression = &aggregates
        .iter()
        .find(|(_, a)| {
            matches!(
                a,
                AggregateExpression::FunctionCall {
                    name: AggregateFunction::GroupConcat { .. },
                    ..
                }
            )
        })
        .unwrap()
        .1;
    let builder = prepared
        .evaluator()
        .accumulator_builder(expression, &mut variables, &mut Vec::new())
        .unwrap();
    let mut acc = builder();
    let NullableAccumulator::GroupConcat { evaluator, .. } = accumulator(&mut acc) else {
        panic!("actual GC variant")
    };
    let NullableAggregateEvaluator::RawExpression(raw) = evaluator else {
        panic!("actual raw compiler")
    };
    let original = Rc::clone(raw);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let calls = Rc::new(Cell::new(0usize));
    let out = Rc::clone(&observed);
    let count = Rc::clone(&calls);
    *raw = Rc::new(move |row| {
        count.set(count.get() + 1);
        let result = original(row);
        if let Ok(value) = &result {
            out.borrow_mut().push(value.clone());
        }
        result
    });
    let x = variables.iter().position(|v| v.as_str() == "x");
    let input = prepared.initial().encode_for(&variables).unwrap();
    let mut input_rows = 0;
    for row in child.unwrap()(input).expect("legacy query starts successfully") {
        let row = row.unwrap();
        input_rows += 1;
        if checkpoints["inner_x"] == "ExplicitNone" {
            assert!(matches!(
                row.cell(x.expect("inner x slot")),
                Some(SolutionValue::ExplicitNone)
            ));
        }
        acc.accumulate(&row).unwrap();
    }
    if checkpoints["inner_x"] == "ExplicitNone" {
        assert_eq!(input_rows, 1);
    }
    if reused_errors {
        // GC05's preserved source note fixes a new gate error and a new update
        // error for every input row. Observe the actual compiler closures;
        // do not substitute an error object or rewrite the frozen record.
        assert_eq!(calls.get(), input_rows * 2);
        let observed = observed.borrow();
        assert_eq!(observed.len(), calls.get());
        let identities = observed
            .iter()
            .map(|v| match v {
                SolutionValue::ReturnedError(error) => error.clone(),
                _ => panic!("actual division expression must return an error object"),
            })
            .collect::<FxHashSet<_>>();
        assert_eq!(identities.len(), calls.get());
    }
    if let Some(wanted) = checkpoints["raw_evaluations"].as_u64() {
        assert_eq!(calls.get() as u64, wanted);
    }
    let NullableAccumulator::GroupConcat { values, .. } = accumulator(&mut acc) else {
        unreachable!()
    };
    if let Some(wanted) = checkpoints
        .get("retained_values")
        .or_else(|| checkpoints.get("outer_retained_values"))
    {
        assert_eq!(values.len() as u64, wanted.as_u64().unwrap());
    }
    if let Some(wanted) = checkpoints["distinct_error_objects"].as_u64() {
        let errors = values
            .iter()
            .map(|v| match v {
                SolutionValue::ReturnedError(e) => e.clone(),
                _ => panic!("retained error object"),
            })
            .collect::<FxHashSet<_>>();
        assert_eq!(errors.len() as u64, wanted);
    }
    if checkpoints["outer_expression_result_kind"] == "ReturnedNotBound" {
        assert_eq!(observed.borrow().len(), 1);
        assert!(
            matches!(&observed.borrow()[0],SolutionValue::ReturnedError(e) if e.kind()==SparqlErrorKind::NotBound)
        );
    }
    if let Some(wanted) = checkpoints["seen_size"].as_u64() {
        let AccumulatorWrapper::SourceDistinct { seen, .. } = &acc else {
            panic!("source DISTINCT wrapper")
        };
        assert_eq!(seen.len() as u64, wanted);
    }
    let AggregateValue::RawExpression(SolutionValue::Rdf(raw)) = acc.finish().unwrap().unwrap()
    else {
        panic!("actual raw GC finish")
    };
    assert_literal(
        &raw,
        if reused_errors {
            &run["expected_row"]["v"]
        } else {
            &run["expected"]["rows"][0]["v"]
        },
    );
    assert_flags(&session, run);
}
fn run_query(id: &str) {
    let run = case("query_cases", id);
    receive_query(&run);
    receive_schedule(&run);
}

enum Step {
    Value(SolutionValue<RawExpressionValue>),
    Raised(ErrorValue),
    Dataset(ReceivingDataset, Term),
    Unreached,
}
fn protocol_step(
    session: &QueryNumericSession,
    value: &Value,
    definitions: &Value,
    errors: &mut FxHashMap<String, ErrorValue>,
) -> Step {
    if let Some(label) = value.as_str() {
        if label == "UNREACHED" {
            return Step::Unreached;
        }
        return protocol_step(session, &definitions[label], definitions, errors);
    }
    if let Some(s) = value["plain_literal"].as_str() {
        return Step::Value(SolutionValue::Rdf(
            session
                .construct_source_direct_string(s, None, None, true)
                .unwrap(),
        ));
    }
    if value["ExplicitNone"] == true || value["kind"] == "ExplicitNone" {
        return Step::Value(SolutionValue::ExplicitNone);
    }
    for (key, class, raised) in [
        ("RaisedNotBound", "NotBoundError", true),
        ("ReturnedNotBound", "NotBoundError", false),
        ("RaisedSparqlTypeError", "SPARQLTypeError", true),
        ("ReturnedError", "SPARQLError", false),
    ] {
        if let Some(label) = value[key].as_str() {
            let def = &definitions[label];
            let error = errors
                .entry(label.into())
                .or_insert_with(|| {
                    error_input(
                        def["source_class"].as_str().unwrap_or(class),
                        &def["source_constructor_argument"],
                    )
                })
                .clone();
            return if raised {
                Step::Raised(error)
            } else {
                Step::Value(SolutionValue::ReturnedError(error))
            };
        }
    }
    if value["FatalDataset"].is_string() {
        let target: Term = Literal::new_simple_literal("a").into();
        let mut dataset = ReceivingDataset::new(&Value::Null);
        dataset.fault = Some((target.clone(), 1));
        dataset.arm();
        return Step::Dataset(dataset, target);
    }
    panic!("unreceived protocol input {value}")
}
fn run_protocol(id: &str) {
    let run = case("protocol_cases", id);
    let session = session(&run);
    let distinct = run["distinct"].as_bool().unwrap();
    let mut errors = FxHashMap::default();
    let (sequence, row_count) = if distinct {
        let pairs = run["gate_update_pairs"].as_array().unwrap();
        (
            pairs
                .iter()
                .flat_map(|p| p.as_array().unwrap().iter())
                .collect::<Vec<_>>(),
            pairs.len(),
        )
    } else {
        let values = run["update_sequence"].as_array().unwrap();
        (values.iter().collect(), values.len())
    };
    let steps = sequence
        .into_iter()
        .map(|v| protocol_step(&session, v, &run["definitions"], &mut errors))
        .collect::<VecDeque<_>>();
    let fatal_identity = steps.iter().find_map(|s| {
        if let Step::Dataset(d, _) = s {
            Some(Arc::clone(&d.identity))
        } else {
            None
        }
    });
    let steps = Rc::new(RefCell::new(steps));
    let calls = Rc::new(Cell::new(0usize));
    let queue = Rc::clone(&steps);
    let count = Rc::clone(&calls);
    let mut acc: NullableAccumulator<'_, Term> = NullableAccumulator::GroupConcat {
        evaluator: NullableAggregateEvaluator::RawExpression(Rc::new(move |_| {
            count.set(count.get() + 1);
            match queue
                .borrow_mut()
                .pop_front()
                .expect("only frozen evaluations")
            {
                Step::Value(v) => Ok(v),
                Step::Raised(e) => Err(EvalFailure::Sparql(e)),
                Step::Dataset(d, t) => match d.externalize_term(t) {
                    Err(e) => Err(EvalFailure::Fatal(QueryEvaluationError::Dataset(Box::new(
                        e,
                    )))),
                    Ok(_) => panic!("real Dataset fault not reached"),
                },
                Step::Unreached => panic!("DISTINCT update unexpectedly evaluated"),
            }
        })),
        session: Rc::clone(&session),
        values: Vec::new(),
        separator: Rc::from(" "),
    };
    let mut seen = FxHashSet::default();
    let tuple = InternalTuple::with_capacity(0);
    let mut failure = None;
    for _ in 0..row_count {
        let result = if distinct {
            acc.accumulate_inner(&tuple, Some(&mut seen))
        } else {
            acc.accumulate(&tuple)
        };
        if let Err(error) = result {
            failure = Some(error);
            break;
        }
    }
    let expected = &run["expected"];
    assert_eq!(
        calls.get() as u64,
        expected["evaluations"].as_u64().unwrap()
    );
    let NullableAccumulator::GroupConcat { values, .. } = &acc else {
        unreachable!()
    };
    if let Some(wanted) = expected["retained"].as_array() {
        assert_eq!(values.len(), wanted.len());
        for (actual, label) in values.iter().zip(wanted) {
            let label = label.as_str().unwrap();
            if label == "ExplicitNone" {
                assert!(matches!(actual, SolutionValue::ExplicitNone));
            } else if let Some(error) = errors.get(label) {
                assert!(matches!(actual,SolutionValue::ReturnedError(e) if e==error));
            } else {
                let Step::Value(wanted) = protocol_step(
                    &session,
                    &run["definitions"][label],
                    &run["definitions"],
                    &mut errors,
                ) else {
                    unreachable!()
                };
                assert!(actual == &wanted);
            }
        }
    }
    if let Some(wanted) = expected["seen"].as_array() {
        let keys = wanted
            .iter()
            .map(|label| {
                let Step::Value(value) = protocol_step(
                    &session,
                    &run["definitions"][label.as_str().unwrap()],
                    &run["definitions"],
                    &mut errors,
                ) else {
                    unreachable!()
                };
                value.try_map_rdf(source_identity_key).unwrap()
            })
            .collect::<FxHashSet<_>>();
        assert_eq!(seen, keys);
    }
    if let Some(wanted) = expected["seen_size"].as_u64() {
        assert_eq!(seen.len() as u64, wanted);
    }
    if let Some(wanted) = expected["retained_values"].as_u64() {
        assert_eq!(values.len() as u64, wanted);
    }
    if let Some(label) = expected["failure_identity"].as_str() {
        let error = failure.expect("uncaught original SPARQL failure");
        let Some(NullableAggregateError::UncaughtSparql(actual)) = nullable_aggregate_error(&error)
        else {
            panic!("original SPARQL error required")
        };
        assert!(actual == &errors[label]);
    } else if expected["fatal_identity"].is_string() {
        let QueryEvaluationError::Dataset(error) = failure.expect("actual Dataset failure") else {
            panic!("Dataset failure required")
        };
        let actual = error.downcast_ref::<ConversionFailure>().unwrap();
        assert!(Arc::ptr_eq(&actual.0, &fatal_identity.unwrap()));
    } else {
        assert!(failure.is_none());
        let Some(AggregateValue::RawExpression(SolutionValue::Rdf(raw))) = acc.finish().unwrap()
        else {
            panic!("raw finish")
        };
        assert_literal(
            &raw,
            &json!({"kind":"literal","lexical":expected["text"],"source_datatype":null,"language_original":null,"ill_typed":null,"cache_kind":"String"}),
        );
    }
    assert_flags(&session, &run);
}
fn run_active(id: &str) {
    let run = case("active_safety_cases", id);
    if id == "GA08" {
        let session = session(&run);
        let value = SolutionValue::ReturnedError(ErrorValue::with_kind(
            SparqlErrorKind::OtherSparql,
            text(&run["input"], "diagnostic"),
        ));
        assert!(matches!(
            session.source_text(&value),
            Err(RawNumericFault::UnknownSourceErrorText)
        ));
        assert_flags(&session, &run);
    } else {
        receive_query(&run);
    }
}
fn run_reused(id: &str) {
    let run = case("reused_cases", id);
    if id == "GC07" {
        let session = session(&run);
        let values = run["state_input"]["evaluated_values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| input(&session, v))
            .collect::<Vec<_>>();
        let steps = Rc::new(RefCell::new(VecDeque::from(values)));
        let queue = Rc::clone(&steps);
        let mut acc: NullableAccumulator<'_, Term> = NullableAccumulator::GroupConcat {
            evaluator: NullableAggregateEvaluator::RawExpression(Rc::new(move |_| {
                Ok(queue.borrow_mut().pop_front().unwrap())
            })),
            session: Rc::clone(&session),
            values: Vec::new(),
            separator: Rc::from(text(&run["state_input"], "separator")),
        };
        assert_eq!(run["state_input"]["distinct"], false);
        acc.accumulate(&InternalTuple::with_capacity(0)).unwrap();
        assert!(steps.borrow().is_empty());
        let Some(AggregateValue::RawExpression(SolutionValue::Rdf(raw))) = acc.finish().unwrap()
        else {
            panic!("raw finish")
        };
        assert_literal(&raw, &run["expected_finish"]);
        assert_flags(&session, &run);
    } else {
        receive_query(&run);
        if id == "GC05" {
            receive_schedule(&run);
        }
    }
}

#[test]
fn source_group_concat_fixture_identity() {
    let root = fixture();
    limits_supplement();
    assert!(!FIXTURE.contains("/Users/"));
    let hashes = [
        "78a80f069cf320bde515171cb031cbc2d2faa85e50e5cb5b79ebf9fcf80204f1",
        "4211a9b9200916608c981092e334134b2fbca9779148a741f47b26aa277df83f",
        "7cc3b8da72bc1565e3504c0f911fce8ddedf3d9740fe3dc8bd6f0e58591ea623",
        "17eb2a6bdbb9365cbcd4934cf0c82c900a676f111a4aa83bba68700dde6f295d",
        "5d250b7951ad02f1bf91f7659bed077bded791ccb4511078580bb7108cd39e70",
        "b5c380cf1450d32b7a454fc1d3bf1c05c1a825e969ae6f06d9ba87f03a8fecc5",
        "fd8cb8b51409c2cf6015c95705b9302ce7e2b3e8acba9ffc0643ef7860d1724a",
    ];
    for (case, wanted) in root["reused_cases"].as_array().unwrap().iter().zip(hashes) {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&canonical(case)).unwrap())
            ),
            wanted
        );
    }
}

#[test]
fn source_group_concat_text_gt01() {
    run_text("GT01", false);
}

#[test]
fn source_group_concat_text_gt02() {
    run_text("GT02", false);
}

#[test]
fn source_group_concat_text_gt03() {
    run_text("GT03", false);
}

#[test]
fn source_group_concat_text_gt04() {
    run_text("GT04", false);
}

#[test]
fn source_group_concat_text_gt05() {
    run_text("GT05", false);
}

#[test]
fn source_group_concat_text_gt06() {
    run_text("GT06", false);
}

#[test]
fn source_group_concat_text_gt07() {
    run_text("GT07", false);
}

#[test]
fn source_group_concat_text_gt08() {
    run_text("GT08", false);
}

#[test]
fn source_group_concat_text_gt09() {
    run_text("GT09", false);
}

#[test]
fn source_group_concat_text_gt10() {
    run_text("GT10", false);
}

#[test]
fn source_group_concat_repr_gr01() {
    run_text("GR01", true);
}

#[test]
fn source_group_concat_repr_gr02() {
    run_text("GR02", true);
}

#[test]
fn source_group_concat_repr_gr03() {
    run_text("GR03", true);
}

#[test]
fn source_group_concat_repr_gr04() {
    run_text("GR04", true);
}

#[test]
fn source_group_concat_repr_gr05() {
    run_text("GR05", true);
}

#[test]
fn source_group_concat_protocol_gp01() {
    run_protocol("GP01");
}

#[test]
fn source_group_concat_protocol_gp02() {
    run_protocol("GP02");
}

#[test]
fn source_group_concat_protocol_gp03() {
    run_protocol("GP03");
}

#[test]
fn source_group_concat_protocol_gp04() {
    run_protocol("GP04");
}

#[test]
fn source_group_concat_protocol_gp05() {
    run_protocol("GP05");
}

#[test]
fn source_group_concat_protocol_gp06() {
    run_protocol("GP06");
}

#[test]
fn query_source_group_concat_gq01() {
    run_query("GQ01");
}

#[test]
fn query_source_group_concat_gq02() {
    run_query("GQ02");
}

#[test]
fn query_source_group_concat_gq03() {
    run_query("GQ03");
}

#[test]
fn query_source_group_concat_gq04() {
    run_query("GQ04");
}

#[test]
fn query_source_group_concat_gq05() {
    run_query("GQ05");
}

#[test]
fn query_source_group_concat_gq06() {
    run_query("GQ06");
}

#[test]
fn query_source_group_concat_gq07() {
    run_query("GQ07");
}

#[test]
fn query_source_group_concat_gq08() {
    run_query("GQ08");
}

#[test]
fn query_source_group_concat_gq09() {
    run_query("GQ09");
}

#[test]
fn query_source_group_concat_gq10() {
    run_query("GQ10");
}

#[test]
fn query_source_group_concat_gq11() {
    run_query("GQ11");
}

#[test]
fn query_source_group_concat_gq12() {
    run_query("GQ12");
}

#[test]
fn query_source_group_concat_gq13() {
    run_query("GQ13");
}

#[test]
fn query_source_group_concat_gq14() {
    run_query("GQ14");
}

#[test]
fn query_source_group_concat_gq15() {
    run_query("GQ15");
}

#[test]
fn query_source_group_concat_gq16() {
    run_query("GQ16");
}

#[test]
fn query_source_group_concat_gq17() {
    run_query("GQ17");
}

#[test]
fn query_source_group_concat_gq18() {
    run_query("GQ18");
}

#[test]
fn query_source_group_concat_gq19() {
    run_query("GQ19");
}

#[test]
fn source_group_concat_active_ga01() {
    run_active("GA01");
}

#[test]
fn source_group_concat_active_ga02() {
    run_active("GA02");
}

#[test]
fn source_group_concat_active_ga03_invalid_session_precondition() {
    receive_invalid_original_precondition("GA03");
}

#[test]
fn source_group_concat_active_ga04_invalid_session_precondition() {
    receive_invalid_original_precondition("GA04");
}

#[test]
fn source_group_concat_active_ga05() {
    run_active("GA05");
}

#[test]
fn source_group_concat_active_ga06() {
    run_active("GA06");
}

#[test]
fn source_group_concat_active_ga07() {
    run_active("GA07");
}

#[test]
fn source_group_concat_active_ga08() {
    run_active("GA08");
}

#[test]
fn source_group_concat_active_ga09() {
    run_active("GA09");
}

#[test]
fn source_group_concat_reused_gc01() {
    run_reused("GC01");
}

#[test]
fn source_group_concat_reused_gc02() {
    run_reused("GC02");
}

#[test]
fn source_group_concat_reused_gc03() {
    run_reused("GC03");
}

#[test]
fn source_group_concat_reused_gc04() {
    run_reused("GC04");
}

#[test]
fn source_group_concat_reused_gc05() {
    run_reused("GC05");
}

#[test]
fn source_group_concat_reused_gc06() {
    run_reused("GC06");
}

#[test]
fn source_group_concat_reused_gc07() {
    run_reused("GC07");
}

// Author controls outside the immutable 56-case source/active corpus.
#[test]
fn source_group_concat_requires_both_explicit_source_axes() {
    let query = text(&case("query_cases", "GQ01"), "query").to_owned();
    for (group, literal) in [
        (
            QueryGroupSemantics::LegacyPrivateV1,
            QueryLiteralSemantics::RdfIdentityV1,
        ),
        (
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::RdfIdentityV1,
        ),
        (
            QueryGroupSemantics::LegacyPrivateV1,
            QueryLiteralSemantics::SourceLiteralV1,
        ),
    ] {
        let session = Rc::new(
            QueryNumericSession::new_with_literal_semantics(
                DecimalContext::default(),
                NumericLimits::default(),
                None,
                true,
                QueryNumericCapabilities::MixedBinary64V1,
                group,
                literal,
            )
            .unwrap(),
        );
        let prepared = prepare_numeric_query(
            &query,
            ReceivingDataset::new(&Value::Null),
            session,
            Vec::new(),
        )
        .unwrap();
        // Inspect the real compiler result so decoder choice cannot mask a
        // GROUP_CONCAT capability failure with a result-metadata refusal.
        let (compiled, _) = prepared
            .evaluator()
            .graph_pattern_evaluator(prepared.plan(), &mut Vec::new());
        let error = compiled
            .err()
            .expect("either disabled axis must reject source GC");
        assert!(matches!(error, QueryEvaluationError::Unexpected(_)));
        if literal == QueryLiteralSemantics::SourceLiteralV1 {
            assert!(matches!(
                raw_numeric_fault(&error),
                Some(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "legacy group with source literal identity"
                ))
            ));
        } else {
            assert_eq!(error.to_string(), "unsupported nullable aggregate");
        }
    }
}

const LIMITS_SUPPLEMENT: &str = include_str!("fixtures/source_group_concat_limits_supplement.json");
fn limits_supplement() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(LIMITS_SUPPLEMENT.as_bytes())),
        "4ba29c0ce4fcb13e6e14da22524fb3ada3b97d69ffc54e9aeaab2bb97602eae5"
    );
    let root: Value = serde_json::from_str(LIMITS_SUPPLEMENT).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "f78e004f278779fcddf8661f8873998ebaf8310919ff2f3128c6075a114a5d3a"
    );
    assert_eq!(root["original_source_record_sha256"], ORACLE_SHA);
    let payload = json!({"environment":root["environment"],
        "invalid_original_cases":root["invalid_original_cases"],"query_cases":root["query_cases"]});
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "3d683dd13a8a57efa062e35fb4b30aa41d8dfbb1db10bdb98df65e0dad5ce5a0"
    );
    assert_eq!(root["invalid_original_cases"].as_array().unwrap().len(), 2);
    assert_eq!(root["query_cases"].as_array().unwrap().len(), 2);
    assert!(!LIMITS_SUPPLEMENT.contains("/Users/"));
    root
}
fn receive_invalid_original_precondition(id: &str) {
    let run = case("active_safety_cases", id);
    let supplement = limits_supplement();
    let recorded = supplement["invalid_original_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["id"] == id)
        .unwrap();
    assert_eq!(recorded["original_frozen_case"], run);
    assert_eq!(recorded["not_a_pass_of_original_gc_expectation"], true);
    let error = try_session(&run)
        .err()
        .expect("original context cannot be constructed");
    let RawNumericFault::Core(NumericFault::ResourceLimit { operation }) = error else {
        panic!("the fixed original precondition must fail during context admission")
    };
    assert_eq!(
        operation,
        text(&recorded["expected_constructor_failure"], "operation")
    );
    // Deliberately no prepare or query execution: the preserved original
    // expectations were never reachable under these two resource settings.
}
fn receive_limits_supplement(id: &str) {
    let root = limits_supplement();
    let run = root["query_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["id"] == id)
        .unwrap();
    receive_query(run);
}
#[test]
fn query_source_group_concat_gcs01_unused_separator_valid_context() {
    receive_limits_supplement("GCS01");
}
#[test]
fn query_source_group_concat_gcs02_output_budget_distinct_from_input() {
    receive_limits_supplement("GCS02");
}

#[test]
fn source_group_concat_default_public_distinct_remains_rdf_identity() {
    let run = case("query_cases", "GQ06");
    let query = spargebra::SparqlParser::new()
        .parse_query(text(&run, "query"))
        .unwrap();
    let dataset = oxrdf::Dataset::new();
    let evaluator = crate::QueryEvaluator::new();
    let crate::QueryResults::Solutions(mut rows) =
        evaluator.prepare(&query).execute(&dataset).unwrap()
    else {
        panic!("SELECT solutions")
    };
    let row = rows.next().unwrap().unwrap();
    // In the original public RDF lane simple literal and explicit xsd:string
    // denote one RDF term, while the separately enabled source lane has two.
    assert_eq!(
        row.get("v"),
        Some(&Term::Literal(Literal::new_simple_literal("x")))
    );
    assert_eq!(row.iter().count(), 1);
    assert!(rows.next().is_none());
}
