//! Real MIN/MAX receiving for independently frozen source/math and active
//! contracts. This adapter and oracle share an author; production has separate
//! owners. Query cases use the sealed production parser/plan/execute entry.
//! Protocol inputs only exercise explicitly frozen accumulator sequences.
use super::nullable::{NullableAccumulator, NullableAggregateEvaluator};
use super::numeric_query::prepare_numeric_query;
use super::*;
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    ParsedSourceLiteral, QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities,
    QueryNumericSession, RawExpressionValue, RawNumericFault, SourceDatatype, SourceExtremumKind,
    SourceFact, SourceLiteralCacheKind, SourceLiteralMetadata, SourceLiteralOrigin,
    raw_numeric_fault,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use semantica_numeric::{
    DecimalContext, DecimalSignal, DecimalSignals, DecimalValue, NumericFault, NumericLimits,
    NumericValue,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

const FIXTURE: &str = include_str!("fixtures/source_extrema_cases.json");
const CORE_FIXTURE: &str = include_str!(
    "../../../../crates/semantica-numeric/tests/fixtures/source_comparison_cases.json"
);
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(key, value)| (key.clone(), canonical(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        value => value.clone(),
    }
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "bea9422ca751855d41ec8c7e1123b49cb45703f3ba0a3d5b172cd2dd120dede5"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "6df0965cedb36486e13c20876eab75315fb77e42c5af66a0b4278e3dbe5e55fa"
    );
    let payload = json!({
        "context":root["context"], "source_math_cases":root["source_math_cases"],
        "active_contract_cases":root["active_contract_cases"],
        "reused_source_cases":root["reused_source_cases"]
    });
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "59f6850d634564bedfc67741265b6f3f4eeaa7206e04d75ebeaccbc995c3a867"
    );
    root
}
fn case(id: &str) -> Value {
    let root = fixture();
    [
        "source_math_cases",
        "active_contract_cases",
        "reused_source_cases",
    ]
    .into_iter()
    .flat_map(|section| root[section].as_array().unwrap())
    .find(|case| case["id"] == id)
    .unwrap()
    .clone()
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing {key}: {value}"))
}
fn signal(name: &str) -> DecimalSignal {
    match name {
        "InvalidOperation" => DecimalSignal::InvalidOperation,
        "DivisionByZero" => DecimalSignal::DivisionByZero,
        "Overflow" => DecimalSignal::Overflow,
        "Inexact" => DecimalSignal::Inexact,
        "Rounded" => DecimalSignal::Rounded,
        "FloatOperation" => DecimalSignal::FloatOperation,
        other => panic!("unreceived signal {other}"),
    }
}
fn signals(value: &Value) -> DecimalSignals {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| signal(value.as_str().unwrap()))
        .collect()
}
fn session(input: &Value) -> Rc<QueryNumericSession> {
    let root = fixture();
    let base = &root["context"]["decimal_context"];
    let field = |key: &str| {
        input["context_override"]
            .get(key)
            .unwrap_or(&base[key])
            .clone()
    };
    let context = DecimalContext {
        precision: field("precision").as_u64().unwrap() as usize,
        emin: field("Emin").as_i64().unwrap().into(),
        emax: field("Emax").as_i64().unwrap().into(),
        clamp: field("clamp").as_u64().unwrap() != 0,
        traps: signals(&field("traps")),
        ..DecimalContext::default()
    };
    let limit = |key: &str| {
        input["limits_override"]
            .get(key)
            .unwrap_or(&root["context"]["numeric_limits"][key])
            .as_u64()
            .unwrap() as usize
    };
    Rc::new(
        QueryNumericSession::new_with_literal_semantics(
            context,
            NumericLimits {
                max_input_characters: limit("max_input_characters"),
                max_coefficient_digits: limit("max_coefficient_digits"),
                max_output_characters: limit("max_output_characters"),
                max_materialized_power_of_ten: limit("max_materialized_power_of_ten"),
            },
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
        )
        .unwrap(),
    )
}
fn number(value: &Value) -> NumericValue {
    match text(value, "family") {
        "Integer" => NumericValue::Integer(text(value, "value").parse().unwrap()),
        "Binary64" => NumericValue::Binary64(f64::from_bits(
            u64::from_str_radix(text(value, "bits").trim_start_matches("0x"), 16).unwrap(),
        )),
        "Decimal" => NumericValue::Decimal(match value["special"].as_str() {
            None => DecimalValue::Finite {
                negative: value["negative"].as_bool().unwrap(),
                coefficient: text(value, "coefficient").parse().unwrap(),
                exponent: text(value, "exponent").parse().unwrap(),
            },
            Some("NaN") => DecimalValue::NaN {
                negative: value["negative"].as_bool().unwrap(),
                signaling: value["signaling"].as_bool().unwrap(),
                payload: text(value, "payload").into(),
            },
            Some(other) => panic!("unreceived Decimal kind {other}"),
        }),
        other => panic!("unreceived number {other}"),
    }
}
fn node(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}
fn literal(value: &Value) -> Literal {
    let lexical = text(value, "lexical");
    if let Some(language) = value["language_original"].as_str() {
        Literal::new_language_tagged_literal(lexical, language).unwrap()
    } else if let Some(datatype) = value["datatype"].as_str() {
        Literal::new_typed_literal(lexical, node(datatype))
    } else {
        Literal::new_simple_literal(lexical)
    }
}
fn initial(input: &Value) -> Vec<NumericInitialBinding> {
    input["initial"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|binding| NumericInitialBinding {
            variable: Variable::new(text(binding, "variable")).unwrap(),
            value: match text(binding, "origin") {
                "UnknownNormalized" => {
                    NumericInitialValue::UnknownNormalized(if binding["kind"] == "BNode" {
                        BlankNode::new(text(binding, "value")).unwrap().into()
                    } else {
                        literal(&binding["rdf_literal"]).into()
                    })
                }
                "DirectString" => NumericInitialValue::SourceDirectString {
                    lexical: text(binding, "lexical").into(),
                    datatype: binding["datatype"].as_str().map(node),
                    language: binding["language_original"].as_str().map(str::to_owned),
                    normalize: binding["normalize"].as_bool().unwrap(),
                },
                "KnownSourceCached" => {
                    let value = &binding["literal"];
                    NumericInitialValue::SourceKnownCached {
                        rdf: literal(value).into(),
                        value: number(&value["cache"]),
                        metadata: SourceLiteralMetadata {
                            datatype: SourceDatatype::Specified(node(text(value, "datatype"))),
                            language_original: SourceFact::Known(None),
                            ill_typed: SourceFact::Known(value["ill_typed"].as_bool()),
                            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
                        },
                    }
                }
                other => panic!("unreceived origin {other}"),
            },
        })
        .collect()
}
fn source_error(class: &str, argument: Option<&str>) -> ErrorValue {
    let (kind, class) = match class {
        "SPARQLError" => (SparqlErrorKind::OtherSparql, SourceExceptionClass::Sparql),
        "SPARQLTypeError" => (SparqlErrorKind::Type, SourceExceptionClass::SparqlType),
        "NotBoundError" => (SparqlErrorKind::NotBound, SourceExceptionClass::NotBound),
        other => panic!("unreceived error class {other}"),
    };
    ErrorValue::with_source(
        kind,
        "adapter diagnostic deliberately differs from the source argument",
        SourceException {
            class,
            argument: argument.map_or(SourceExceptionArgument::None, |value| {
                SourceExceptionArgument::Text(value.into())
            }),
        },
    )
}
fn assert_error_object(actual: &ErrorValue, expected: &Value) {
    let source = actual.source_exception().expect("original exception facts");
    let class = match source.class {
        SourceExceptionClass::Sparql => "SPARQLError",
        SourceExceptionClass::SparqlType => "SPARQLTypeError",
        SourceExceptionClass::NotBound => "NotBoundError",
        SourceExceptionClass::AlreadyBound => "AlreadyBound",
    };
    assert_eq!(class, text(expected, "class"));
    match &source.argument {
        SourceExceptionArgument::None => assert_eq!(expected["argument"]["kind"], "None"),
        SourceExceptionArgument::DecimalSignalClasses(_) => {
            panic!("unexpected class-list in unchanged historical fixture")
        }
        SourceExceptionArgument::Text(argument) => {
            assert_eq!(expected["argument"]["kind"], "Text");
            assert_eq!(argument, text(&expected["argument"], "value"));
        }
    }
}
fn assert_literal(raw: &RawExpressionValue, expected: &Value) {
    let Term::Literal(rdf) = raw.rdf().unwrap() else {
        panic!("finish must produce a Literal")
    };
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!("source metadata lost")
    };
    if expected.get("rdf_lexical").is_some() {
        assert_eq!(rdf.value(), text(expected, "rdf_lexical"));
        assert_eq!(rdf.datatype().as_str(), text(expected, "rdf_datatype"));
        assert_eq!(metadata.datatype, SourceDatatype::Unknown);
        assert_eq!(metadata.language_original, SourceFact::Unknown);
        assert_eq!(metadata.ill_typed, SourceFact::Known(None));
        assert_eq!(metadata.cached_value_kind, SourceFact::Unknown);
        assert!(matches!(
            raw.numeric(),
            NumericCacheState::UnknownNormalized
        ));
        return;
    }
    assert_eq!(rdf.value(), text(expected, "lexical"));
    let datatype = match &metadata.datatype {
        SourceDatatype::Absent => Value::Null,
        SourceDatatype::Specified(value) => json!(value.as_str()),
        SourceDatatype::Unknown => panic!("known output datatype lost"),
    };
    assert_eq!(datatype, expected["datatype"]);
    let language = expected
        .get("language_original")
        .or_else(|| expected.get("language"))
        .unwrap_or(&Value::Null);
    assert_eq!(
        metadata.language_original,
        SourceFact::Known(language.as_str().map(str::to_owned))
    );
    assert_eq!(
        metadata.ill_typed,
        SourceFact::Known(expected["ill_typed"].as_bool()),
        "Literal(existing) resets the original ill_typed flag"
    );
    if let Some(cache) = expected.get("cache") {
        match text(cache, "family") {
            "String" => {
                assert_eq!(
                    metadata.cached_value_kind,
                    SourceFact::Known(SourceLiteralCacheKind::String)
                );
                assert!(matches!(raw.numeric(), NumericCacheState::NotNumeric));
                assert_eq!(rdf.value(), text(cache, "value"));
            }
            "None" => {
                assert_eq!(
                    metadata.cached_value_kind,
                    SourceFact::Known(SourceLiteralCacheKind::None)
                );
                assert!(matches!(raw.numeric(), NumericCacheState::CastFailed));
            }
            "ErrorObject" => {
                assert_eq!(
                    metadata.cached_value_kind,
                    SourceFact::Known(SourceLiteralCacheKind::ErrorObject)
                );
                let NumericCacheState::ErrorObject(error) = raw.numeric() else {
                    panic!("real ErrorValue payload is required, not a metadata tag")
                };
                assert_error_object(error, cache);
            }
            "Integer" | "Decimal" | "Binary64" => {
                assert_eq!(
                    metadata.cached_value_kind,
                    SourceFact::Known(SourceLiteralCacheKind::Numeric)
                );
                let NumericCacheState::Known(actual) = raw.numeric() else {
                    panic!("known numeric value cache lost")
                };
                assert_eq!(actual.datatype().as_ref(), rdf.datatype());
                let wanted = number(cache);
                match (actual.value(), wanted) {
                    (NumericValue::Binary64(actual), NumericValue::Binary64(wanted)) => {
                        assert_eq!(actual.to_bits(), wanted.to_bits());
                    }
                    (actual, wanted) => assert_eq!(actual, &wanted),
                }
            }
            other => panic!("unreceived cache family {other}"),
        }
    }
}
fn assert_row(row: &SourcePresenceSolution, expected: &Value) {
    let actual = row
        .variables
        .iter()
        .zip(&row.values)
        .filter_map(|(variable, cell)| cell.as_ref().map(|cell| (variable.as_str(), cell)))
        .collect::<BTreeMap<_, _>>();
    let wanted = expected.as_object().unwrap();
    let wanted_names = wanted
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        actual
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        wanted_names
    );
    let mut error_identities: BTreeMap<&str, &ErrorValue> = BTreeMap::new();
    for (name, value) in wanted {
        if value["kind"] == "ExplicitNone" {
            assert!(matches!(actual[name.as_str()], SolutionValue::ExplicitNone));
            continue;
        }
        let SolutionValue::Rdf(raw) = actual[name.as_str()] else {
            panic!("an error Literal must remain an RDF cell through alias projection")
        };
        assert_literal(raw, value);
        if let Some(label) = value["cache"]["same_identity_as"].as_str() {
            let NumericCacheState::ErrorObject(error) = raw.numeric() else {
                unreachable!()
            };
            if let Some(first) = error_identities.insert(label, error) {
                assert!(
                    first == error,
                    "cloning across aliases must preserve ErrorValue Arc identity"
                );
            }
        }
    }
}

#[derive(Debug)]
struct FailureIdentity;
#[derive(Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("extrema receiving Dataset failure")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct Trace {
    internal: Cell<usize>,
    external: Cell<usize>,
    scans: Cell<usize>,
    armed: Cell<bool>,
    armed_external: Cell<usize>,
}
impl Trace {
    fn counts(&self) -> (usize, usize, usize, usize) {
        (
            self.internal.get(),
            self.external.get(),
            self.scans.get(),
            self.armed_external.get(),
        )
    }
}
#[derive(Clone)]
struct ReceivingDataset {
    trace: Rc<Trace>,
    identity: Arc<FailureIdentity>,
}
impl Default for ReceivingDataset {
    fn default() -> Self {
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
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        self.trace.external.set(self.trace.external.get() + 1);
        if self.trace.armed.get() {
            self.trace
                .armed_external
                .set(self.trace.armed_external.get() + 1);
            return Err(ConversionFailure(Arc::clone(&self.identity)));
        }
        Ok(term)
    }
}
fn assert_fault(
    error: &QueryEvaluationError,
    expected: &Value,
    id: &str,
    dataset: &ReceivingDataset,
) {
    if id == "EA05" {
        let QueryEvaluationError::Dataset(error) = error else {
            panic!("original Dataset error: {error}")
        };
        let actual = error.downcast_ref::<ConversionFailure>().unwrap();
        assert!(Arc::ptr_eq(&actual.0, &dataset.identity));
        return;
    }
    if id == "EA06" {
        assert!(matches!(error, QueryEvaluationError::Cancelled));
        return;
    }
    if expected["error_class"] == "TypeError" || expected["expected_error"].is_string() {
        assert!(
            matches!(
                raw_numeric_fault(error),
                Some(RawNumericFault::SourceComparisonTypeError)
            ),
            "{error}"
        );
    } else if expected["error_class"] == "decimal.FloatOperation" {
        assert!(
            matches!(
                raw_numeric_fault(error),
                Some(RawNumericFault::Core(NumericFault::DecimalTrap {
                    primary: DecimalSignal::FloatOperation,
                    ..
                }))
            ),
            "{error}"
        );
    } else if id == "EA03" {
        assert!(
            matches!(
                raw_numeric_fault(error),
                Some(RawNumericFault::UnknownSourceLiteralIdentity)
            ),
            "{error}"
        );
    } else if matches!(id, "EA01" | "EA08") {
        let Some(RawNumericFault::Core(NumericFault::ResourceLimit { operation })) =
            raw_numeric_fault(error)
        else {
            panic!("output-only budget: {error}")
        };
        assert_eq!(*operation, "output_characters");
    } else {
        panic!("unreceived failure {id}: {error}");
    }
}
fn expected_flags(id: &str, expected: &Value, outer: &Value) -> DecimalSignals {
    if let Some(flags) = expected
        .get("flags_exact")
        .or_else(|| outer.get("flags_exact"))
    {
        signals(flags)
    } else if id == "MM02" {
        DecimalSignal::DivisionByZero.into()
    } else {
        DecimalSignals::EMPTY
    }
}
fn receive_query(id: &str, input: &Value, query: &str, expected: &Value, outer: &Value) {
    let session = session(input);
    let dataset = ReceivingDataset::default();
    let prepared =
        prepare_numeric_query(query, dataset.clone(), Rc::clone(&session), initial(input)).unwrap();
    assert_eq!(
        session.flags(),
        DecimalSignals::EMPTY,
        "construction must not execute aggregate operands"
    );
    let cancellation = prepared.evaluator().dataset.cancellation_token.clone();
    if id == "EA05" {
        dataset.trace.armed.set(true);
    }
    let mut rows = prepared
        .execute_source_presence()
        .expect("actual source parser/lowering/compiler");
    assert_eq!(
        session.flags(),
        DecimalSignals::EMPTY,
        "Group first demand must be lazy"
    );
    if id == "EA06" {
        cancellation.cancel();
    }
    let default_rows = Vec::new();
    let prefix = expected
        .get("rows")
        .or_else(|| expected.get("prefix_rows"))
        .map(|v| v.as_array().unwrap())
        .unwrap_or(&default_rows);
    for wanted in prefix {
        let row = rows
            .next()
            .expect("frozen output row")
            .expect("successful prefix");
        assert_row(&row, wanted);
        if id == "MM02" {
            let Some(SolutionValue::Rdf(raw)) = &row.values[0] else {
                panic!("Literal(error)")
            };
            assert!(matches!(raw.numeric(), NumericCacheState::ErrorObject(_)));
        }
    }
    let fails = expected.get("error_class").is_some()
        || expected.get("fatal_kind").is_some()
        || expected.get("then").is_some()
        || matches!(id, "EA05" | "EA06");
    if fails {
        let error = rows
            .next()
            .expect("one fatal result")
            .err()
            .expect("fatal must not become an empty row");
        assert_fault(&error, expected, id, &dataset);
    } else {
        assert!(rows.next().is_none(), "unexpected extra result for {id}");
    }
    let counts = dataset.trace.counts();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(
        dataset.trace.counts(),
        counts,
        "fused stream did extra Dataset work"
    );
    assert_eq!(dataset.trace.scans.get(), 0);
    if id == "EA05" {
        assert_eq!(dataset.trace.armed_external.get(), 1);
    }
    assert_eq!(session.flags(), expected_flags(id, expected, outer), "{id}");
}

fn extremum_group(pattern: &GraphPattern) -> Option<&GraphPattern> {
    match pattern {
        GraphPattern::SourceGroup {
            inner, aggregates, ..
        } => {
            if aggregates.iter().any(|(_, agg)| {
                matches!(
                    agg,
                    AggregateExpression::FunctionCall {
                        name: AggregateFunction::Min | AggregateFunction::Max,
                        ..
                    }
                )
            }) {
                Some(pattern)
            } else {
                extremum_group(inner)
            }
        }
        GraphPattern::Project { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Filter { inner, .. }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner } => extremum_group(inner),
        GraphPattern::Union { inner } => inner.iter().find_map(extremum_group),
        _ => None,
    }
}
/// Observe real compiled operand closures, without replacing their result or
/// evaluating an AST in the test. Full query output is separately received.
fn receive_operand_schedule(id: &str, input: &Value, query: &str, expected: &Value) {
    if !matches!(id, "EX09-01" | "EX09-02" | "MM02" | "EX08-01") {
        return;
    }
    let session = session(input);
    let prepared = prepare_numeric_query(
        query,
        ReceivingDataset::default(),
        Rc::clone(&session),
        initial(input),
    )
    .unwrap();
    let GraphPattern::SourceGroup {
        inner, aggregates, ..
    } = extremum_group(prepared.plan()).unwrap()
    else {
        unreachable!()
    };
    for (_, expression) in aggregates.iter().filter(|(_, agg)| {
        matches!(
            agg,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Min | AggregateFunction::Max,
                ..
            }
        )
    }) {
        let mut variables = Vec::new();
        let (child, _) = prepared
            .evaluator()
            .graph_pattern_evaluator(inner, &mut variables);
        let builder = prepared
            .evaluator()
            .accumulator_builder(expression, &mut variables, &mut Vec::new())
            .unwrap();
        let mut acc = builder();
        let AccumulatorWrapper::Nullable(NullableAccumulator::Extremum { evaluator, .. }) =
            &mut acc
        else {
            panic!("MIN/MAX must not use SourceDistinct wrapper")
        };
        let NullableAggregateEvaluator::RawExpression(raw) = evaluator else {
            panic!("same production raw compiler")
        };
        let original = Rc::clone(raw);
        let calls = Rc::new(Cell::new(0usize));
        let observed = Rc::new(RefCell::new(Vec::new()));
        let count = Rc::clone(&calls);
        let out = Rc::clone(&observed);
        *raw = Rc::new(move |row| {
            count.set(count.get() + 1);
            let result = original(row);
            if let Ok(SolutionValue::ReturnedError(error)) = &result {
                out.borrow_mut().push(error.clone());
            }
            result
        });
        let from = prepared.initial().encode_for(&variables).unwrap();
        let mut failure = None;
        for row in child.unwrap()(from).expect("legacy query starts successfully") {
            if let Err(error) = acc.accumulate(&row.unwrap()) {
                failure = Some(error);
                break;
            }
        }
        let count_expected = if id == "EX09-01" || id == "EX09-02" {
            2
        } else {
            1
        };
        assert_eq!(
            calls.get(),
            count_expected,
            "MIN/MAX has no DISTINCT pre-evaluation"
        );
        if expected.get("error_class").is_some() {
            assert_fault(
                &failure.unwrap(),
                expected,
                id,
                &ReceivingDataset::default(),
            );
            continue;
        }
        assert!(failure.is_none());
        let finished = acc.finish().unwrap();
        if let Some(selected_error) = observed.borrow().first() {
            let Some(AggregateValue::RawExpression(SolutionValue::Rdf(raw))) = finished else {
                panic!("Literal(error) must be a raw RDF output")
            };
            let NumericCacheState::ErrorObject(actual) = raw.numeric() else {
                panic!("actual error cache")
            };
            assert!(
                actual == selected_error,
                "finish must preserve selected ErrorValue Arc"
            );
            // Exercise the actual same evaluator's aggregate codec in both
            // directions, including the real Dataset round trip.
            let internal = prepared
                .evaluator()
                .dataset
                .internalize_aggregate_value(
                    AggregateValue::RawExpression(SolutionValue::Rdf(raw)),
                    true,
                )
                .unwrap();
            let SolutionValue::Rdf(internal) = internal else {
                unreachable!()
            };
            let codec = prepared.evaluator().dataset.numeric.as_ref().unwrap();
            let external = (codec.externalize)(internal).unwrap();
            let NumericCacheState::ErrorObject(actual) = external.numeric() else {
                panic!("codec lost ErrorObject")
            };
            assert!(actual == selected_error);
        }
    }
}
fn run_queries(id: &str) {
    let case = case(id);
    let input = case
        .get("input")
        .cloned()
        .unwrap_or_else(|| json!({"queries":case["queries"]}));
    let expected = case
        .get("expected")
        .cloned()
        .unwrap_or_else(|| json!({"rows":[case["expected_row"]]}));
    let default_queries = vec![input["query"].clone()];
    let queries = input["queries"].as_array().unwrap_or(&default_queries);
    for (index, query) in queries.iter().enumerate() {
        let wanted = expected["runs"]
            .as_array()
            .map(|runs| &runs[index])
            .or_else(|| expected.get("each_query"))
            .unwrap_or(&expected);
        receive_query(id, &input, query.as_str().unwrap(), wanted, &expected);
        receive_operand_schedule(id, &input, query.as_str().unwrap(), wanted);
    }
}

/// Observe the actual complete compiled plan's codec calls. Both callbacks
/// delegate unchanged to the prepared Dataset codec, including all failures.
/// The first ErrorObject entering it is the inner MIN's constructed Literal;
/// later callbacks and the final SAMPLE cell must retain that same Arc.
fn receive_sample_error_identity() {
    let case = case("EX10-01");
    let input = &case["input"];
    let session = session(input);
    let prepared = prepare_numeric_query(
        text(input, "query"),
        ReceivingDataset::default(),
        Rc::clone(&session),
        initial(input),
    )
    .unwrap();
    let mut evaluator = prepared.evaluator().clone();
    let original = Rc::clone(evaluator.dataset.numeric.as_ref().unwrap());
    let entered = Rc::new(RefCell::new(Vec::new()));
    let exited = Rc::new(RefCell::new(Vec::new()));
    let into_original = Rc::clone(&original);
    let out_original = Rc::clone(&original);
    let observed_entered = Rc::clone(&entered);
    let observed_exited = Rc::clone(&exited);
    let codec = Rc::new(RawTermCodec {
        session: Rc::clone(&original.session),
        prepared_literals: original.prepared_literals.clone(),
        internalize: Rc::new(move |raw: RawExpressionValue| {
            let selected = match raw.numeric() {
                NumericCacheState::ErrorObject(error) => Some(error.clone()),
                _ => None,
            };
            let result = (into_original.internalize)(raw);
            if result.is_ok() {
                if let Some(error) = selected {
                    observed_entered.borrow_mut().push(error);
                }
            }
            result
        }),
        externalize: Rc::new(move |internal| {
            let result = (out_original.externalize)(internal);
            if let Ok(raw) = &result {
                if let NumericCacheState::ErrorObject(error) = raw.numeric() {
                    observed_exited.borrow_mut().push(error.clone());
                }
            }
            result
        }),
        source_identity: Rc::clone(&original.source_identity),
        same_transport: original.same_transport,
        source_bgp_scan: original.source_bgp_scan.clone(),
    });
    evaluator.dataset.numeric = Some(Rc::clone(&codec));
    let mut variables = Vec::new();
    let (compiled, _) = evaluator.graph_pattern_evaluator(prepared.plan(), &mut variables);
    let from = prepared.initial().encode_for(&variables).unwrap();
    let mut rows = compiled.unwrap()(from).expect("legacy query starts successfully");
    let tuple = rows.next().unwrap().unwrap();
    let mut values = Vec::new();
    for index in 0..variables.len() {
        values.push(match tuple.cell(index) {
            Some(SolutionValue::Rdf(value)) => Some(SolutionValue::Rdf(
                (codec.externalize)(value.clone()).unwrap(),
            )),
            Some(SolutionValue::ExplicitNone) => Some(SolutionValue::ExplicitNone),
            Some(SolutionValue::ReturnedError(error)) => {
                Some(SolutionValue::ReturnedError(error.clone()))
            }
            None => None,
        });
    }
    let row = SourcePresenceSolution {
        variables: variables.into(),
        values,
    };
    assert_row(&row, &case["expected"]["rows"][0]);
    let sample_index = row
        .variables
        .iter()
        .position(|variable| variable.as_str() == "sample")
        .unwrap();
    let Some(SolutionValue::Rdf(sample)) = &row.values[sample_index] else {
        panic!("actual SAMPLE RDF cell")
    };
    let NumericCacheState::ErrorObject(sample_error) = sample.numeric() else {
        panic!("actual SAMPLE error cache")
    };
    let entered = entered.borrow();
    assert!(
        entered.len() >= 2,
        "inner MIN and outer SAMPLE must use the actual codec"
    );
    assert!(
        sample_error == &entered[0],
        "SAMPLE retained the inner selected error Arc"
    );
    assert!(entered.iter().all(|error| error == &entered[0]));
    let exited = exited.borrow();
    assert!(!exited.is_empty());
    assert!(exited.iter().all(|error| error == &entered[0]));
    assert!(rows.next().is_none());
    assert_eq!(session.flags(), signals(&case["expected"]["flags_exact"]));
}

fn one(session: &QueryNumericSession) -> RawExpressionValue {
    session
        .construct_source_literal(&ParsedSourceLiteral {
            occurrence: None,
            rdf: Literal::new_typed_literal("1", node("http://www.w3.org/2001/XMLSchema#integer")),
            datatype: SourceDatatype::Specified(node("http://www.w3.org/2001/XMLSchema#integer")),
            language_original: None,
            origin: SourceLiteralOrigin::QueryTypedExistingLiteral,
        })
        .unwrap()
}
enum Step {
    Value(SolutionValue<RawExpressionValue>),
    Raised(ErrorValue),
}
fn step(label: &str, session: &QueryNumericSession) -> Step {
    match label {
        "ExplicitNone" => Step::Value(SolutionValue::ExplicitNone),
        "Rdf(integer1)" | "RdfInteger1" => Step::Value(SolutionValue::Rdf(one(session))),
        "RaisedNotBound" => Step::Raised(source_error("NotBoundError", None)),
        "RaisedSPARQLTypeError" => {
            Step::Raised(source_error("SPARQLTypeError", Some("source type failure")))
        }
        "ReturnedNotBound(None)" => Step::Value(SolutionValue::ReturnedError(source_error(
            "NotBoundError",
            None,
        ))),
        other => panic!("unreceived protocol step {other}"),
    }
}
fn protocol_accumulator(
    kind: SourceExtremumKind,
    session: Rc<QueryNumericSession>,
    steps: VecDeque<Step>,
    calls: Rc<Cell<usize>>,
) -> NullableAccumulator<'static, Term> {
    let queue = RefCell::new(steps);
    NullableAccumulator::Extremum {
        evaluator: NullableAggregateEvaluator::RawExpression(Rc::new(move |_| {
            calls.set(calls.get() + 1);
            match queue
                .borrow_mut()
                .pop_front()
                .expect("only frozen protocol calls")
            {
                Step::Value(value) => Ok(value),
                Step::Raised(error) => Err(EvalFailure::Sparql(error)),
            }
        })),
        session,
        selected: None,
        kind,
    }
}
fn receive_protocol(id: &str) {
    let case = case(id);
    let sequences: Vec<(&Value, &Value)> = if id == "MM03" {
        case["state_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|run| (&run["sequence"], run))
            .collect()
    } else {
        case["input"]["sequences"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["expected"]["per_sequence"].as_array().unwrap())
            .collect()
    };
    for kind in [SourceExtremumKind::Minimum, SourceExtremumKind::Maximum] {
        for (sequence, expected) in &sequences {
            let session = session(&Value::Null);
            let steps = sequence
                .as_array()
                .unwrap()
                .iter()
                .map(|v| step(v.as_str().unwrap(), &session))
                .collect();
            let calls = Rc::new(Cell::new(0));
            let mut accumulator =
                protocol_accumulator(kind, Rc::clone(&session), steps, Rc::clone(&calls));
            let mut failure = None;
            for _ in sequence.as_array().unwrap() {
                if let Err(error) = accumulator.accumulate(&InternalTuple::with_capacity(0)) {
                    failure = Some(error);
                    break;
                }
            }
            assert_eq!(calls.get(), 2);
            if expected.get("expected_error").is_some() || expected.get("error_class").is_some() {
                assert_fault(
                    &failure.unwrap(),
                    expected,
                    id,
                    &ReceivingDataset::default(),
                );
            } else {
                assert!(failure.is_none());
                let Some(AggregateValue::RawExpression(SolutionValue::Rdf(raw))) =
                    accumulator.finish().unwrap()
                else {
                    panic!("selected value")
                };
                if let Some(wanted) = expected.get("selected") {
                    assert_literal(&raw, wanted);
                } else {
                    let NumericCacheState::Known(cache) = raw.numeric() else {
                        panic!("integer1")
                    };
                    assert_eq!(cache.value(), &NumericValue::Integer(1.into()));
                }
            }
            assert_eq!(session.flags(), DecimalSignals::EMPTY);
        }
    }
}
fn run_active(id: &str) {
    let case = case(id);
    let input = &case["input"];
    let expected = &case["expected"];
    match id {
        "EA01" => {
            receive_query(
                id,
                input,
                text(input, "query"),
                &expected["without_limit"],
                expected,
            );
            let query = format!(
                "{} LIMIT {}",
                text(input, "query"),
                input["also_run_with_limit"].as_u64().unwrap()
            );
            receive_query(id, input, &query, &expected["with_limit1"], expected);
        }
        "EA09" => {
            let error = source_error("SPARQLError", Some("divide by 0"));
            for (numeric, kind) in [
                (
                    NumericCacheState::NotNumeric,
                    SourceLiteralCacheKind::ErrorObject,
                ),
                (
                    NumericCacheState::ErrorObject(error.clone()),
                    SourceLiteralCacheKind::String,
                ),
            ] {
                let actual = RawExpressionValue::from_source_parts(
                    Literal::new_simple_literal(text(&input["rdf_literal"], "lexical")).into(),
                    numeric,
                    LiteralSourceState::Literal(SourceLiteralMetadata {
                        datatype: SourceDatatype::Absent,
                        language_original: SourceFact::Known(None),
                        ill_typed: SourceFact::Known(None),
                        cached_value_kind: SourceFact::Known(kind),
                    }),
                );
                assert!(matches!(actual, Err(RawNumericFault::InvalidSourceOrigin)));
            }
        }
        "EA10" => {
            for kind in [SourceExtremumKind::Minimum, SourceExtremumKind::Maximum] {
                let session = session(input);
                let calls = Rc::new(Cell::new(0));
                let error = ErrorValue::with_kind(
                    SparqlErrorKind::OtherSparql,
                    text(&input["steps"][0], "diagnostic"),
                );
                let mut accumulator = protocol_accumulator(
                    kind,
                    session,
                    VecDeque::from([Step::Value(SolutionValue::ReturnedError(error.clone()))]),
                    Rc::clone(&calls),
                );
                accumulator
                    .accumulate(&InternalTuple::with_capacity(0))
                    .unwrap();
                let NullableAccumulator::Extremum { selected, .. } = &accumulator else {
                    unreachable!()
                };
                assert!(
                    matches!(selected, Some(SolutionValue::ReturnedError(selected)) if selected == &error)
                );
                let failure = accumulator
                    .finish()
                    .err()
                    .expect("unknown original error text");
                assert!(matches!(
                    raw_numeric_fault(&failure),
                    Some(RawNumericFault::UnknownSourceErrorText)
                ));
                assert_eq!(calls.get(), 1);
            }
        }
        _ => run_queries(id),
    }
}

#[test]
fn source_extrema_fixture_identity() {
    let root = fixture();
    assert_eq!(root["source_math_cases"].as_array().unwrap().len(), 26);
    assert_eq!(root["active_contract_cases"].as_array().unwrap().len(), 10);
    assert_eq!(root["reused_source_cases"].as_array().unwrap().len(), 4);
    assert_eq!(
        root["reused_record_sha256"],
        "19ebfb25e122b559dffd1eccbcc384a48d007e7b8cc0f9c6c68c6dd75681c679"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(CORE_FIXTURE.as_bytes())),
        "45fa9dae344cb445d92ea72342d82db97afd3edb3323fb657506c37b88e7c400"
    );
}

#[test]
fn source_extrema_query_ex01_01() {
    run_queries("EX01-01");
}

#[test]
fn source_extrema_query_ex02_01() {
    run_queries("EX02-01");
}

#[test]
fn source_extrema_query_ex02_02() {
    run_queries("EX02-02");
}

#[test]
fn source_extrema_query_ex03_01() {
    run_queries("EX03-01");
}

#[test]
fn source_extrema_query_ex03_02() {
    run_queries("EX03-02");
}

#[test]
fn source_extrema_query_ex04_06() {
    run_queries("EX04-06");
}

#[test]
fn source_extrema_query_ex05_01() {
    run_queries("EX05-01");
}

#[test]
fn source_extrema_query_ex05_02() {
    run_queries("EX05-02");
}

#[test]
fn source_extrema_query_ex06_04() {
    run_queries("EX06-04");
}

#[test]
fn source_extrema_query_ex06_05() {
    run_queries("EX06-05");
}

#[test]
fn source_extrema_query_ex06_06() {
    run_queries("EX06-06");
}

#[test]
fn source_extrema_query_ex06_07() {
    run_queries("EX06-07");
}

#[test]
fn source_extrema_query_ex07_01() {
    run_queries("EX07-01");
}

#[test]
fn source_extrema_query_ex07_02() {
    run_queries("EX07-02");
}

#[test]
fn source_extrema_query_ex07_03() {
    run_queries("EX07-03");
}

#[test]
fn source_extrema_query_ex08_01() {
    run_queries("EX08-01");
}

#[test]
fn source_extrema_protocol_ex08_02() {
    receive_protocol("EX08-02");
}

#[test]
fn source_extrema_query_ex09_01() {
    run_queries("EX09-01");
}

#[test]
fn source_extrema_query_ex09_02() {
    run_queries("EX09-02");
}

#[test]
fn source_extrema_query_ex10_01() {
    run_queries("EX10-01");
    receive_sample_error_identity();
}

#[test]
fn source_extrema_query_ex11_01() {
    run_queries("EX11-01");
}

#[test]
fn source_extrema_query_ex11_02() {
    run_queries("EX11-02");
}

#[test]
fn source_extrema_query_ex12_01() {
    run_queries("EX12-01");
}

#[test]
fn source_extrema_query_ex12_02() {
    run_queries("EX12-02");
}

#[test]
fn source_extrema_query_ex10_02() {
    run_queries("EX10-02");
}

#[test]
fn source_extrema_query_ex10_03() {
    run_queries("EX10-03");
}

#[test]
fn source_extrema_query_mm01() {
    run_queries("MM01");
}

#[test]
fn source_extrema_query_mm02() {
    run_queries("MM02");
}

#[test]
fn source_extrema_protocol_mm03() {
    receive_protocol("MM03");
}

#[test]
fn source_extrema_query_mm04() {
    run_queries("MM04");
}

#[test]
fn source_extrema_active_ea01() {
    run_active("EA01");
}

#[test]
fn source_extrema_active_ea02() {
    run_active("EA02");
}

#[test]
fn source_extrema_active_ea03() {
    run_active("EA03");
}

#[test]
fn source_extrema_active_ea05() {
    run_active("EA05");
}

#[test]
fn source_extrema_active_ea06() {
    run_active("EA06");
}

#[test]
fn source_extrema_active_ea07() {
    run_active("EA07");
}

#[test]
fn source_extrema_active_ea08() {
    run_active("EA08");
}

#[test]
fn source_extrema_active_ea09() {
    run_active("EA09");
}

#[test]
fn source_extrema_active_ea10() {
    run_active("EA10");
}

#[test]
fn source_extrema_active_ea11() {
    run_active("EA11");
}

// Author controls outside the immutable source/math and active fixtures.
#[test]
fn source_extrema_requires_both_explicit_source_axes() {
    for function in ["MIN", "MAX"] {
        let query = format!("SELECT ({function}(?x) AS ?v) WHERE {{ VALUES ?x {{ 2 1 }} }}");
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
            let prepared =
                prepare_numeric_query(&query, ReceivingDataset::default(), session, Vec::new())
                    .unwrap();
            let (compiled, _) = prepared
                .evaluator()
                .graph_pattern_evaluator(prepared.plan(), &mut Vec::new());
            let error = compiled
                .err()
                .expect("both source axes are required for private extrema");
            if literal == QueryLiteralSemantics::SourceLiteralV1 {
                assert!(matches!(
                    raw_numeric_fault(&error),
                    Some(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "legacy group with source literal identity"
                    ))
                ));
            } else {
                assert!(matches!(error, QueryEvaluationError::Unexpected(_)));
                assert_eq!(error.to_string(), "unsupported nullable aggregate");
            }
        }
    }
}

#[test]
fn source_extrema_default_public_min_max_remains_rdf() {
    let query = spargebra::SparqlParser::new()
        .parse_query("SELECT (MIN(?x) AS ?lo) (MAX(?x) AS ?hi) WHERE { VALUES ?x { 2 1 } }")
        .unwrap();
    let dataset = oxrdf::Dataset::new();
    let evaluator = crate::QueryEvaluator::new();
    let crate::QueryResults::Solutions(mut rows) =
        evaluator.prepare(&query).execute(&dataset).unwrap()
    else {
        panic!("public SELECT solutions")
    };
    let row = rows.next().unwrap().unwrap();
    assert_eq!(row.get("lo"), Some(&Term::Literal(Literal::from(1))));
    assert_eq!(row.get("hi"), Some(&Term::Literal(Literal::from(2))));
    assert_eq!(row.iter().count(), 2);
    assert!(rows.next().is_none());
}

#[test]
fn source_extrema_does_not_enable_legacy_order_node() {
    let session = session(&Value::Null);
    let prepared = prepare_numeric_query(
        "SELECT (MIN(?x) AS ?lo) (MAX(?x) AS ?hi) WHERE { VALUES ?x { 2 1 } }",
        ReceivingDataset::default(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    // New sealed SourceOrderBy does not authorize the legacy node.
    let legacy_order = GraphPattern::OrderBy {
        inner: Box::new(prepared.plan().clone()),
        expression: Vec::new(),
    };
    let (compiled, _) = prepared
        .evaluator()
        .graph_pattern_evaluator(&legacy_order, &mut Vec::new());
    let error = compiled
        .err()
        .expect("legacy OrderBy remains unreceived in source mode");
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::UnsupportedSourceLiteralOperator(
            "source value ordering"
        ))
    ));
    assert_eq!(session.flags(), DecimalSignals::EMPTY);
}
