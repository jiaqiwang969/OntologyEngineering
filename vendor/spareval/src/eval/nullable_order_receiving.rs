//! Real source ORDER BY receiving over the sealed production parser/plan/start.
//! Frozen expectations are source/math-derived separately from the implementation.
//! Native sort schedules are received in source_sort_receiving; this module does
//! not sort, reinterpret ASTs, or recalculate LT inside its observation callbacks.
use super::numeric_query::{decode_source_presence_bindings, prepare_numeric_query};
use super::source_ordering::SourceOrderEvent;
use super::source_sort::{SourceSortError, reserve_sort_scratch, source_sort_indices};
use super::*;
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities, QueryNumericSession,
    RawExpressionValue, RawNumericFault, SourceDatatype, SourceFact, SourceLiteralCacheKind,
    SourceLiteralMetadata, SourceOrderKey, raw_numeric_fault,
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
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/source_order_cases.json");
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
        "2d6dde609a12acad72c06da84f568ebf95dc7165b394a9dcff9b3f1727f2a3b7"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "738d39778338087562cf4edbac0fb651a4d1af66cd59002ce7a47b3a4fcc2b2d"
    );
    let payload = json!({"context":root["context"],"source_math_cases":root["source_math_cases"],
        "active_contract_cases":root["active_contract_cases"],"author_controls":root["author_controls"]});
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "9db628edba79f453378dc49f8940c137f98971feac331fc09edf8678e511d2da"
    );
    root
}
fn case(id: &str) -> Value {
    let root = fixture();
    [
        "source_math_cases",
        "active_contract_cases",
        "author_controls",
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
        assert_eq!(metadata.ill_typed, SourceFact::Unknown);
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
        "ORDER must preserve original ill_typed metadata"
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
        if value["kind"] == "IRI" {
            assert!(
                matches!(raw.rdf().unwrap(), Term::NamedNode(node) if node.as_str() == text(value, "value"))
            );
        } else {
            assert_literal(raw, value);
        }
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
        f.write_str("ORDER receiving Dataset failure")
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
    fail_at_external: Cell<usize>,
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
            if self.trace.armed_external.get() == self.trace.fail_at_external.get() {
                return Err(ConversionFailure(Arc::clone(&self.identity)));
            }
        }
        Ok(term)
    }
}

fn assert_fault(
    error: &QueryEvaluationError,
    expected: &Value,
    dataset: &ReceivingDataset,
    injected_error: Option<&ErrorValue>,
) {
    let class = expected["error_class"]
        .as_str()
        .or_else(|| expected["then_error"].as_str())
        .or_else(|| expected["then_error_class"].as_str())
        .unwrap();
    match class {
        "TypeError" => assert!(matches!(
            raw_numeric_fault(error),
            Some(RawNumericFault::SourceComparisonTypeError)
        )),
        "decimal.FloatOperation" | "decimal.Inexact" => {
            super::source_decimal_trap_receiving::assert_original_decimal_class(
                error,
                if class == "decimal.FloatOperation" {
                    DecimalSignal::FloatOperation
                } else {
                    DecimalSignal::Inexact
                },
            );
        }
        "NotBoundError" => {
            let Some(nullable::NullableAggregateError::UncaughtSparql(actual)) =
                nullable::nullable_aggregate_error(error)
            else {
                panic!("bare bound error lost source identity: {error}")
            };
            assert!(actual == injected_error.expect("actual injected ErrorValue"));
        }
        "RawNumericFault::UnknownSourceLiteralIdentity" => assert!(matches!(
            raw_numeric_fault(error),
            Some(RawNumericFault::UnknownSourceLiteralIdentity)
        )),
        "RawNumericFault::UnknownNumericCache" => assert!(matches!(
            raw_numeric_fault(error),
            Some(RawNumericFault::UnknownNumericCache)
        )),
        "RawNumericFault::Core(ResourceLimit)" => {
            let Some(RawNumericFault::Core(NumericFault::ResourceLimit { operation })) =
                raw_numeric_fault(error)
            else {
                panic!("expected resource refusal: {error}")
            };
            assert_eq!(*operation, text(expected, "operation"));
        }
        "QueryEvaluationError::Cancelled" => {
            assert!(matches!(error, QueryEvaluationError::Cancelled));
        }
        "original Dataset error" => {
            let QueryEvaluationError::Dataset(actual) = error else {
                panic!("original backend error was reclassified: {error}")
            };
            let actual = actual.downcast_ref::<ConversionFailure>().unwrap();
            assert!(Arc::ptr_eq(&actual.0, &dataset.identity));
        }
        other => panic!("unreceived frozen fault {other}: {error}"),
    }
}

fn numeric_key_integer(key: &SourceOrderKey) -> i64 {
    let SourceOrderKey::Rdf(raw) = key else {
        panic!("expected an actual numeric key")
    };
    let NumericCacheState::Known(cache) = raw.numeric() else {
        panic!("expected the actual known key cache")
    };
    let NumericValue::Integer(value) = cache.value() else {
        panic!("expected integer key")
    };
    value.to_string().parse().unwrap()
}

fn sample_x_count(pattern: &GraphPattern) -> usize {
    match pattern {
        GraphPattern::SourceGroup { aggregates, .. } => aggregates
            .iter()
            .filter(|(_, aggregate)| {
                matches!(aggregate,
                    AggregateExpression::FunctionCall {
                        name: AggregateFunction::Sample,
                        expr: Expression::Variable(variable), ..
                    } if variable.as_str() == "x")
            })
            .count(),
        GraphPattern::Project { inner, .. }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Filter { inner, .. }
        | GraphPattern::SourceOrderBy { inner, .. } => sample_x_count(inner),
        GraphPattern::Union { inner } => inner.iter().map(sample_x_count).sum(),
        _ => 0,
    }
}

fn assert_events(id: &str, expected: &Value, events: &[SourceOrderEvent]) {
    let starts: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SourceOrderEvent::KeyStart {
                condition_index,
                original_row_index,
            } => Some((*condition_index, *original_row_index)),
            _ => None,
        })
        .collect();
    let keys: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SourceOrderEvent::Key {
                condition_index,
                original_row_index,
                key,
            } => Some((*condition_index, *original_row_index, key)),
            _ => None,
        })
        .collect();
    let comparisons: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SourceOrderEvent::Compare {
                condition_index,
                left_original,
                right_original,
            } => Some((*condition_index, *left_original, *right_original)),
            _ => None,
        })
        .collect();
    if let Some(count) = expected
        .get("total_key_evaluations")
        .or_else(|| expected.get("key_evaluations"))
        .or_else(|| expected.get("order_key_evaluations"))
    {
        assert_eq!(
            starts.len(),
            count.as_u64().unwrap() as usize,
            "{id}: actual key starts"
        );
    }
    if let Some(count) = expected
        .get("comparator_calls")
        .or_else(|| expected.get("comparison_calls"))
    {
        assert_eq!(
            comparisons.len(),
            count.as_u64().unwrap() as usize,
            "{id}: actual comparisons"
        );
    }
    if let Some(pairs) = expected.get("comparison_pairs") {
        let wanted: Vec<_> = pairs
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p[0].as_u64().unwrap() as usize,
                    p[1].as_u64().unwrap() as usize,
                )
            })
            .collect();
        assert_eq!(
            comparisons
                .iter()
                .map(|(_, a, b)| (*a, *b))
                .collect::<Vec<_>>(),
            wanted
        );
        if pairs
            .as_array()
            .unwrap()
            .iter()
            .any(|pair| pair.get(2).is_some())
        {
            let wanted: Vec<_> = pairs
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        pair[0].as_u64().unwrap() as usize,
                        pair[1].as_u64().unwrap() as usize,
                        pair[2].as_bool().expect("frozen success pair Boolean"),
                    )
                })
                .collect();
            let returned: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    SourceOrderEvent::Compared {
                        left_original,
                        right_original,
                        less,
                        ..
                    } => Some((*left_original, *right_original, *less)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                returned, wanted,
                "{id}: original comparator result, without a repeated LT"
            );
        }
    }
    if let Some(tags) = expected.get("key_tags") {
        let labels: &[usize] = if id == "OQ15" {
            &[2, 1, 4, 3]
        } else {
            &[1, 2, 3, 4]
        };
        let actual: Vec<_> = starts.iter().map(|(_, row)| labels[*row]).collect();
        assert_eq!(json!(actual), *tags, "{id}: original key order");
        if id == "OQ15" {
            let actual_values: Vec<_> = keys
                .iter()
                .map(|(_, _, key)| numeric_key_integer(key))
                .collect();
            assert_eq!(
                json!(actual_values),
                *tags,
                "actual bare tag keys must preserve complete left-then-right UNION collection"
            );
        }
    }
    if let Some(passes) = expected.get("key_passes") {
        let mut all_wanted = Vec::new();
        for pass in passes.as_array().unwrap() {
            let condition = pass["condition_index"].as_u64().unwrap() as usize;
            let labels = pass
                .get("key_tags")
                .or_else(|| pass.get("group_ids"))
                .or_else(|| pass.get("first_seen_group_ids"));
            if let Some(labels) = labels {
                all_wanted.extend(
                    labels
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| (condition, value.as_u64().unwrap() as usize - 1)),
                );
            }
            if let Some(values) = pass.get("key_values") {
                let actual: Vec<_> = keys
                    .iter()
                    .filter(|(c, _, _)| *c == condition)
                    .map(|(_, _, key)| numeric_key_integer(key))
                    .collect();
                assert_eq!(json!(actual), *values);
            }
        }
        assert_eq!(
            starts, all_wanted,
            "{id}: reverse passes must use prior permutation"
        );
    }
    if let Some(values) = expected.get("key_values_by_first_seen_group") {
        let actual: Vec<_> = keys
            .iter()
            .map(|(_, _, key)| numeric_key_integer(key))
            .collect();
        assert_eq!(json!(actual), *values);
    }
    if let Some(class) = expected.get("key_class").and_then(Value::as_str) {
        for (_, _, key) in &keys {
            match class {
                "NoneKey" => assert!(matches!(key, SourceOrderKey::None)),
                "UnboundVariable(missing)" | "UnboundVariable(x)" => {
                    let SourceOrderKey::UnboundVariable(variable) = key else {
                        panic!("actual variable placeholder was lost")
                    };
                    assert_eq!(
                        variable.as_str(),
                        if class.ends_with("(x)") {
                            "x"
                        } else {
                            "missing"
                        }
                    );
                }
                "Rdf alias s" => assert!(matches!(key, SourceOrderKey::Rdf(_))),
                _ => panic!("unreceived frozen key class {class}"),
            }
        }
    }
    if let Some(classes) = expected.get("key_classes") {
        let actual: Vec<_> = keys
            .iter()
            .map(|(_, _, key)| match key {
                SourceOrderKey::None => "NoneKey",
                SourceOrderKey::Rdf(_) => "Rdf",
                SourceOrderKey::UnboundVariable(_) => "UnboundVariable",
            })
            .collect();
        assert_eq!(json!(actual), *classes);
    }
    // Every comparison follows successful construction of all keys in its pass.
    // A new pass starts only after the previous pass's comparisons have ended.
    for (position, event) in events.iter().enumerate() {
        if let SourceOrderEvent::Compare {
            condition_index, ..
        } = event
        {
            assert!(!events[position+1..].iter().any(|event| matches!(event,
                SourceOrderEvent::KeyStart { condition_index: later, .. } if later == condition_index)));
        }
    }
}

fn receive_query(id: &str, input: &Value, query: &str, expected: &Value) {
    let session = session(input);
    let dataset = ReceivingDataset::default();
    let events = Rc::new(RefCell::new(Vec::<SourceOrderEvent>::new()));
    let observed = Rc::clone(&events);
    let prepared =
        prepare_numeric_query(query, dataset.clone(), Rc::clone(&session), initial(input))
            .unwrap()
            .with_source_order_observer(Rc::new(move |event| observed.borrow_mut().push(event)));
    assert_eq!(
        session.flags(),
        DecimalSignals::EMPTY,
        "{id}: prepare must not sort"
    );
    assert!(events.borrow().is_empty());
    if let Some(count) = expected.get("order_implicit_samples") {
        assert_eq!(
            sample_x_count(prepared.plan()),
            count.as_u64().unwrap() as usize
        );
    }
    let cancellation = prepared.evaluator().dataset.cancellation_token.clone();
    if id == "OA04" {
        dataset.trace.fail_at_external.set(1);
        dataset.trace.armed.set(true);
    }
    if id == "OA07" {
        cancellation.cancel();
    }
    let injected_error =
        (id == "OQ05").then(|| source_error("NotBoundError", Some("bound-marker")));
    let started: Result<
        Box<dyn Iterator<Item = Result<SourcePresenceSolution, QueryEvaluationError>>>,
        QueryEvaluationError,
    > = if matches!(id, "OQ04" | "OQ05") {
        let mut variables = Vec::new();
        let (compiled, _) = prepared
            .evaluator()
            .graph_pattern_evaluator(prepared.plan(), &mut variables);
        let x = variables
            .iter()
            .position(|variable| variable.as_str() == "x")
            .expect("root Project must explicitly preserve the injected x slot");
        let mut from = prepared.initial().encode_for(&variables).unwrap();
        from.set_cell(
            x,
            injected_error
                .as_ref()
                .map_or(SolutionValue::ExplicitNone, |error| {
                    SolutionValue::ReturnedError(error.clone())
                }),
        );
        let codec = prepared
            .evaluator()
            .dataset
            .numeric
            .as_ref()
            .unwrap()
            .clone();
        compiled.and_then(|compiled| compiled(from)).map(|rows| {
            Box::new(decode_source_presence_bindings(
                codec,
                cancellation.clone(),
                rows,
                variables.into(),
            ))
                as Box<dyn Iterator<Item = Result<SourcePresenceSolution, QueryEvaluationError>>>
        })
    } else {
        prepared.execute_source_presence().map(|rows| {
            Box::new(rows)
                as Box<dyn Iterator<Item = Result<SourcePresenceSolution, QueryEvaluationError>>>
        })
    };
    let after_start_flags = expected.get("flags_after_execute").or_else(|| {
        expected
            .get("phases")
            .and_then(|p| p.get("execute_start"))
            .and_then(|p| p.get("flags_exact"))
    });
    if let Some(flags) = after_start_flags {
        assert_eq!(session.flags(), signals(flags), "{id}: start flags");
    }
    if expected["execute"] == "Err" {
        let error = started
            .err()
            .expect("frozen eager failure must not become a lazy row error");
        assert_fault(&error, expected, &dataset, injected_error.as_ref());
        assert_eq!(session.flags(), signals(&expected["flags_exact"]));
        assert_events(id, expected, &events.borrow());
        assert_eq!(dataset.trace.scans.get(), 0);
        if id == "OA04" {
            assert_eq!(dataset.trace.armed_external.get(), 1);
        }
        return;
    }
    let eager_completed = expected["order_work_stage"] == "execute_start"
        || expected["phases"]["first_poll"]["order_work"] == "completed at execute_start";
    if eager_completed {
        assert_events(id, expected, &events.borrow());
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|e| matches!(e, SourceOrderEvent::KeyStart { .. }))
                .count(),
            events
                .borrow()
                .iter()
                .filter(|e| matches!(e, SourceOrderEvent::Key { .. }))
                .count(),
            "{id}: every successful eager key must already exist before polling"
        );
    }
    if id == "OQ14-control" {
        assert_eq!(session.flags(), DecimalSignals::EMPTY);
        assert!(events.borrow().is_empty());
    }
    let events_after_start = events.borrow().len();
    let mut rows =
        started.unwrap_or_else(|error| panic!("{id}: unexpected startup failure {error}"));
    if id == "OA05" {
        dataset.trace.fail_at_external.set(2);
        dataset.trace.armed.set(true);
    }
    if id == "OA08" {
        cancellation.cancel();
    }
    if expected["order_work_stage"] == "first_poll"
        || expected["order_work_stage"] == "not_demanded"
        || id == "OA08"
    {
        assert!(
            events.borrow().is_empty(),
            "{id}: generator child started early"
        );
    }
    let empty_rows = Vec::new();
    let wanted = expected
        .get("rows")
        .or_else(|| expected.get("prefix_rows"))
        .map(|rows| rows.as_array().unwrap())
        .unwrap_or(&empty_rows);
    let mut received = Vec::new();
    for wanted in wanted {
        let row = rows.next().expect("required source row").unwrap();
        assert_row(&row, wanted);
        received.push(row);
    }
    if expected.get("then_error").is_some()
        || expected.get("then_error_class").is_some()
        || expected["stage"] == "first_poll"
    {
        let error = rows
            .next()
            .expect("one original poll failure")
            .err()
            .expect("error, not a row");
        assert_fault(&error, expected, &dataset, injected_error.as_ref());
    } else {
        assert!(rows.next().is_none(), "{id}: unexpected extra row/failure");
    }
    let counts = dataset.trace.counts();
    let event_count = events.borrow().len();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(
        dataset.trace.counts(),
        counts,
        "{id}: backend work after exhaustion/failure"
    );
    assert_eq!(
        events.borrow().len(),
        event_count,
        "{id}: ordering after exhaustion/failure"
    );
    assert_eq!(
        dataset.trace.scans.get(),
        0,
        "source graph remains outside this lane"
    );
    if id == "OA05" {
        assert_eq!(dataset.trace.armed_external.get(), 2);
    }
    let flags = expected
        .get("flags_exact")
        .cloned()
        .unwrap_or_else(|| json!([]));
    assert_eq!(session.flags(), signals(&flags), "{id}: final flags");
    assert_events(id, expected, &events.borrow());
    if eager_completed {
        assert_eq!(
            events.borrow().len(),
            events_after_start,
            "{id}: eager ordering must not restart while decoding"
        );
    }
    if id == "OQ14-control" {
        assert!(events.borrow().is_empty());
    }
    if id == "OQ09" {
        let observed = events.borrow();
        let key_error = observed
            .iter()
            .find_map(|event| match event {
                SourceOrderEvent::Key {
                    key: SourceOrderKey::Rdf(raw),
                    ..
                } => match raw.numeric() {
                    NumericCacheState::ErrorObject(error) => Some(error),
                    _ => None,
                },
                _ => None,
            })
            .expect("real inner MIN ErrorObject reached ORDER key callback");
        let Some(SolutionValue::Rdf(first)) = &received[0].values[0] else {
            panic!("original error Literal")
        };
        let NumericCacheState::ErrorObject(output_error) = first.numeric() else {
            panic!("ErrorObject became String")
        };
        assert!(
            key_error == output_error,
            "ORDER + decoder must preserve the actual inner object Arc"
        );
    }
}

fn run_source(id: &str) {
    let c = case(id);
    let input = &c["input"];
    let query = text(input, "query");
    let expected = &c["expected"];
    receive_query(id, input, query, expected);
    if expected["also_descending_same_rows"] == true {
        let query = query
            .strip_suffix("ORDER BY ?x")
            .expect("frozen plain direction query")
            .to_owned()
            + "ORDER BY DESC(?x)";
        receive_query(id, input, &query, expected);
    }
    if let Some(query) = input["control_without_order"].as_str() {
        receive_query("OQ14-control", input, query, &c["control_expected"]);
    }
}

fn find_source_order(pattern: &GraphPattern) -> Option<(&GraphPattern, &[OrderExpression])> {
    match pattern {
        GraphPattern::SourceOrderBy { inner, expression } => Some((inner, expression)),
        GraphPattern::Project { inner, .. }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Filter { inner, .. } => find_source_order(inner),
        _ => None,
    }
}

fn run_author(id: &str) {
    match id {
        "OC01" => {
            let c = case(id);
            let query = spargebra::SparqlParser::new()
                .parse_query(text(&c["input"], "query"))
                .unwrap();
            let dataset = oxrdf::Dataset::new();
            let evaluator = crate::QueryEvaluator::new();
            let crate::QueryResults::Solutions(mut rows) =
                evaluator.prepare(&query).execute(&dataset).unwrap()
            else {
                panic!("default public SELECT")
            };
            for lexical in c["expected"]["legacy_rows_lexical"].as_array().unwrap() {
                let row = rows.next().unwrap().unwrap();
                let Some(Term::Literal(actual)) = row.get("x") else {
                    panic!("public RDF literal")
                };
                assert_eq!(actual.value(), lexical.as_str().unwrap());
            }
            assert!(rows.next().is_none());
        }
        "OC02" => {
            let query = "SELECT ?x WHERE { VALUES ?x { 2 1 } } ORDER BY ?x";
            let legacy = spargebra::SparqlParser::new().parse_query(query).unwrap();
            let spargebra::Query::Select { pattern, .. } = legacy else {
                panic!("SELECT")
            };
            let plan = crate::QueryEvaluator::new()
                .without_optimizations()
                .plan_graph_pattern(&pattern);
            let prepared = prepare_numeric_query(
                query,
                ReceivingDataset::default(),
                session(&Value::Null),
                Vec::new(),
            )
            .unwrap();
            let (compiled, _) = prepared
                .evaluator()
                .graph_pattern_evaluator(&plan, &mut Vec::new());
            let error = compiled
                .err()
                .expect("ordinary legacy OrderBy remains forbidden in source mode");
            assert!(matches!(
                raw_numeric_fault(&error),
                Some(RawNumericFault::UnsupportedSourceLiteralOperator(_))
            ));
        }
        "OC03" => {
            let prepared = prepare_numeric_query(
                "SELECT ?x WHERE { VALUES ?x { 2 1 } } ORDER BY ?x",
                ReceivingDataset::default(),
                session(&Value::Null),
                Vec::new(),
            )
            .unwrap();
            assert!(find_source_order(prepared.plan()).is_some());
            let public = crate::QueryEvaluator::new();
            let evaluator = public
                .simple_evaluator(
                    ReceivingDataset::default(),
                    QueryDatasetSpecification::new(),
                    &None,
                )
                .unwrap();
            let (compiled, _) = evaluator.graph_pattern_evaluator(prepared.plan(), &mut Vec::new());
            assert!(
                compiled.is_err(),
                "new source-only node may not run through default compiler"
            );
        }
        "OC04" => {
            let c = case(id);
            let query = text(&c["input"], "query");
            let prepared = prepare_numeric_query(
                query,
                ReceivingDataset::default(),
                session(&Value::Null),
                Vec::new(),
            )
            .unwrap();
            let (inner, expressions) =
                find_source_order(prepared.plan()).expect("actual private parser/planner marker");
            assert_eq!(expressions.len(), 2);
            assert_eq!(
                expressions[0], expressions[1],
                "duplicate key AST occurrences survive"
            );
            assert!(
                !matches!(inner, GraphPattern::Extend { .. }),
                "ORDER expressions were prematurely lowered through Extend"
            );
            let ordinary = spargebra::SparqlParser::new().parse_query(query).unwrap();
            let spargebra::Query::Select { pattern, .. } = ordinary else {
                panic!("SELECT")
            };
            let ordinary = crate::QueryEvaluator::new()
                .without_optimizations()
                .plan_graph_pattern(&pattern);
            assert!(find_source_order(&ordinary).is_none());
            // Exact prior enum Hash byte streams are exercised by the actual
            // spargebra/sparopt source_order_append_preserves_prior_graph_hash_bytes
            // controls, not inferred from this AST-marker assertion.
        }
        _ => panic!("unknown frozen author control {id}"),
    }
}

#[test]
fn source_order_query_fixture_identity() {
    let root = fixture();
    assert_eq!(root["source_math_cases"].as_array().unwrap().len(), 38);
    assert_eq!(root["active_contract_cases"].as_array().unwrap().len(), 11);
    assert_eq!(root["author_controls"].as_array().unwrap().len(), 4);
    assert_eq!(root["excluded_deferred_ids"], json!(["OA12", "OA13"]));
    for case in root["active_contract_cases"].as_array().unwrap() {
        assert_eq!(
            case["evidence_class"],
            "active_safety_contract_not_original_python_parity"
        );
    }
}

fn run_active(id: &str) {
    let c = case(id);
    match id {
        "OA09" => {
            let input = &c["input"];
            let values: Vec<i64> = input["integer_keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect();
            let mut indices: Vec<usize> = input["indices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect();
            let calls = Cell::new(0);
            let cancelled = Cell::new(false);
            let marker = Rc::new(());
            let result = source_sort_indices(
                &mut indices,
                false,
                |a, b| {
                    calls.set(calls.get() + 1);
                    if calls.get() == 3 {
                        cancelled.set(true);
                    }
                    Ok(values[a] < values[b])
                },
                || {
                    if cancelled.get() {
                        Err(marker.clone())
                    } else {
                        Ok(())
                    }
                },
            );
            match result {
                Err(SourceSortError::Callback(actual)) => assert!(Rc::ptr_eq(&actual, &marker)),
                other => panic!("actual cancel callback did not stop sort: {other:?}"),
            }
            assert_eq!(calls.get(), 3);
        }
        "OA10" => {
            let mut scratch: Vec<usize> = c["input"]["scratch_initial"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect();
            let result = reserve_sort_scratch::<()>(&mut scratch, usize::MAX);
            assert!(matches!(result, Err(SourceSortError::Allocation)));
            assert_eq!(json!(scratch), c["expected"]["scratch_unchanged"]);
        }
        _ => receive_query(id, &c["input"], text(&c["input"], "query"), &c["expected"]),
    }
}

#[test]
fn source_order_query_oq01_a() {
    run_source("OQ01-a");
}

#[test]
fn source_order_query_oq01_b() {
    run_source("OQ01-b");
}

#[test]
fn source_order_query_oq01_c() {
    run_source("OQ01-c");
}

#[test]
fn source_order_query_oq02_a() {
    run_source("OQ02-a");
}

#[test]
fn source_order_query_oq02_b() {
    run_source("OQ02-b");
}

#[test]
fn source_order_query_oq02_c() {
    run_source("OQ02-c");
}

#[test]
fn source_order_query_oq03() {
    run_source("OQ03");
}

#[test]
fn source_order_query_oq04() {
    run_source("OQ04");
}

#[test]
fn source_order_query_oq05() {
    run_source("OQ05");
}

#[test]
fn source_order_query_oq06() {
    run_source("OQ06");
}

#[test]
fn source_order_query_oq07() {
    run_source("OQ07");
}

#[test]
fn source_order_query_oq08() {
    run_source("OQ08");
}

#[test]
fn source_order_query_oq09() {
    run_source("OQ09");
}

#[test]
fn source_order_query_oq10() {
    run_source("OQ10");
}

#[test]
fn source_order_query_oq11() {
    run_source("OQ11");
}

#[test]
fn source_order_query_oq12() {
    run_source("OQ12");
}

#[test]
fn source_order_query_oq13() {
    run_source("OQ13");
}

#[test]
fn source_order_query_oq14() {
    run_source("OQ14");
}

#[test]
fn source_order_query_oq15() {
    run_source("OQ15");
}

#[test]
fn source_order_query_oq16_a() {
    run_source("OQ16-a");
}

#[test]
fn source_order_query_oq16_b() {
    run_source("OQ16-b");
}

#[test]
fn source_order_query_oq16_c() {
    run_source("OQ16-c");
}

#[test]
fn source_order_query_oq16_d() {
    run_source("OQ16-d");
}

#[test]
fn source_order_query_oq16_e() {
    run_source("OQ16-e");
}

#[test]
fn source_order_query_oq17_a() {
    run_source("OQ17-a");
}

#[test]
fn source_order_query_oq17_b() {
    run_source("OQ17-b");
}

#[test]
fn source_order_query_oq18_a() {
    run_source("OQ18-a");
}

#[test]
fn source_order_query_oq18_b() {
    run_source("OQ18-b");
}

#[test]
fn source_order_query_oq19() {
    run_source("OQ19");
}

#[test]
fn source_order_query_oq20() {
    run_source("OQ20");
}

#[test]
fn source_order_query_oq21() {
    run_source("OQ21");
}

#[test]
fn source_order_query_oq22() {
    run_source("OQ22");
}

#[test]
fn source_order_query_oq23_a() {
    run_source("OQ23-a");
}

#[test]
fn source_order_query_oq23_b() {
    run_source("OQ23-b");
}

#[test]
fn source_order_query_oq23_c() {
    run_source("OQ23-c");
}

#[test]
fn source_order_query_oq24() {
    run_source("OQ24");
}

#[test]
fn source_order_query_oq25() {
    run_source("OQ25");
}

#[test]
fn source_order_query_oq23_d() {
    run_source("OQ23-d");
}

#[test]
fn source_order_active_oa01() {
    run_active("OA01");
}

#[test]
fn source_order_active_oa02() {
    run_active("OA02");
}

#[test]
fn source_order_active_oa03() {
    run_active("OA03");
}

#[test]
fn source_order_active_oa04() {
    run_active("OA04");
}

#[test]
fn source_order_active_oa05() {
    run_active("OA05");
}

#[test]
fn source_order_active_oa06() {
    run_active("OA06");
}

#[test]
fn source_order_active_oa07() {
    run_active("OA07");
}

#[test]
fn source_order_active_oa08() {
    run_active("OA08");
}

#[test]
fn source_order_active_oa09() {
    run_active("OA09");
}

#[test]
fn source_order_active_oa10() {
    run_active("OA10");
}

#[test]
fn source_order_active_oa11() {
    run_active("OA11");
}

#[test]
fn source_order_author_oc01() {
    run_author("OC01");
}

#[test]
fn source_order_author_oc02() {
    run_author("OC02");
}

#[test]
fn source_order_author_oc03() {
    run_author("OC03");
}

#[test]
fn source_order_author_oc04() {
    run_author("OC04");
}
