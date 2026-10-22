//! Independent SourceContext receiving: sealed queries and real consumer protocols.
//! Expectations were frozen before the query runtime. The oracle and receiver
//! share an author; the production compiler/consumers have other owners.
//! No AST interpreter, replacement sealed plan, additional store or expected sort.
use super::numeric_query::prepare_numeric_query;
use super::source_context_join::{
    SourceChildSide, SourceContextEvent, SourceContextRuntime, source_join_evaluator,
    source_minus_evaluator,
};
use super::source_set::{SourceSetError, reserve_set_table};
use super::*;
use crate::dataset::nullable_numeric::{CachedInternalTerm, NumericDataset};
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    QueryContextSemantics, QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities,
    QueryNumericSession, RawExpressionValue, RawNumericFault, SourceContextFault, SourceDatatype,
    SourceFact, SourceHashPolicy, SourceHashRuntime, SourceLiteralCacheKind, raw_numeric_fault,
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
use spargebra::algebra::{SourceContextAnnotation, SourceJoinMode, SourceValuesForm};
use std::cell::RefCell;
use std::collections::BTreeMap;

type TermCell = SolutionValue<CachedInternalTerm<Term>>;
type Row = InternalTuple<CachedInternalTerm<Term>>;
type Event = SourceContextEvent<CachedInternalTerm<Term>>;
const FIXTURE: &str = include_str!("fixtures/source_context_query_cases.json");
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(v) => Value::Object(
            v.iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect(),
        ),
        Value::Array(v) => Value::Array(v.iter().map(canonical).collect()),
        v => v.clone(),
    }
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "49285fe3ec9042e7efc11ef2b8d67302ff2f79d8ae85ca9d084d7b42532a856a"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "ea2cd7e1619a8b9fa92734acb8ae3d85608b00be97777e05008e0b0610f5fd58"
    );
    root
}
fn case(id: &str) -> Value {
    let root = fixture();
    [
        "source_query_cases",
        "historical_correction_query_cases",
        "raw_consumer_protocol_cases",
        "active_safety_cases",
    ]
    .into_iter()
    .flat_map(|section| root[section].as_array().unwrap())
    .find(|c| c["id"] == id)
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
        .map(|x| signal(x.as_str().unwrap()))
        .collect()
}
fn session(c: &Value) -> Rc<QueryNumericSession> {
    let f = fixture();
    let e = &f["environment"];
    let d = &e["decimal_context"];
    let l = &e["numeric_limits"];
    let traps = c["environment_overrides"]
        .get("decimal_traps")
        .unwrap_or(&d["traps"]);
    let limit = |name: &str| l[name].as_u64().unwrap() as usize;
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            DecimalContext {
                precision: d["precision"].as_u64().unwrap() as usize,
                emin: d["Emin"].as_i64().unwrap().into(),
                emax: d["Emax"].as_i64().unwrap().into(),
                clamp: d["clamp"] == 1,
                traps: signals(traps),
                ..DecimalContext::default()
            },
            NumericLimits {
                max_input_characters: limit("max_input_characters"),
                max_coefficient_digits: limit("max_coefficient_digits"),
                max_output_characters: limit("max_output_characters"),
                max_materialized_power_of_ten: limit("max_materialized_power_of_ten"),
            },
            None,
            e["normalize_literals"].as_bool().unwrap(),
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
            QueryContextSemantics::SourceContextV1,
            Some(Rc::new(
                SourceHashRuntime::new(SourceHashPolicy::Seeded(
                    e["hash_runtime"]["seed"].as_u64().unwrap() as u32,
                ))
                .unwrap(),
            )),
        )
        .unwrap(),
    )
}
fn initial(c: &Value) -> Vec<NumericInitialBinding> {
    c["input"]["initial_bindings"]
        .as_object()
        .map(|items| {
            items
                .iter()
                .map(|(name, value)| NumericInitialBinding {
                    variable: Variable::new(name).unwrap(),
                    value: NumericInitialValue::SourceDirectString {
                        lexical: text(value, "lexical").into(),
                        datatype: value["datatype"]
                            .as_str()
                            .map(|s| NamedNode::new(s).unwrap()),
                        language: value["language"].as_str().map(str::to_owned),
                        normalize: value["normalize"].as_bool().unwrap_or(true),
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}
// Descriptor normalization changes field names only; it does not calculate any
// expected query value, comparison, grouping, set order or context operation.
fn descriptor(value: &Value) -> Value {
    let owned;
    let v = if let Some(name) = value.as_str() {
        owned = fixture()["values"][name].clone();
        assert!(!owned.is_null(), "{name}");
        &owned
    } else {
        value
    };
    if v.is_null() {
        return Value::Null;
    }
    if v.get("source_datatype").is_some() {
        let family = text(v, "cache_family");
        let cache = match family {
            "Integer" => json!({"family":family,"value":v["cache"]}),
            "String" => json!({"family":family,"value":v["cache"]}),
            "Decimal" => {
                let mut x = v["cache"].clone();
                x["family"] = json!("Decimal");
                x
            }
            _ => panic!("unreceived shorthand family {family}"),
        };
        return json!({"lexical":v["lexical"],"datatype":v["source_datatype"],"language":v["language_original"],"ill_typed":v["ill_typed"],"cache":cache});
    }
    let cache = match text(v, "cache_type") {
        "builtins.int" => {
            json!({"family":"Integer","value":v["cache"].as_str().map(str::to_owned).unwrap_or_else(||v["cache"].to_string())})
        }
        "builtins.str" => json!({"family":"String","value":v["cache"]}),
        "decimal.Decimal" => {
            json!({"family":"Decimal","negative":v["cache"]["sign"]==1,"coefficient":v["cache"]["digits"].as_array().unwrap().iter().map(Value::to_string).collect::<String>(),"exponent":v["cache"]["exponent"].to_string()})
        }
        name if name.ends_with("Error") => {
            json!({"family":"ErrorObject","class":v["cache"]["class"],"args":v["cache"]["args"]})
        }
        name => panic!("unreceived original cache {name}"),
    };
    json!({"lexical":v["lexical"],"datatype":v["datatype"],"language":v["language"],"ill_typed":v["ill_typed"],"cache":cache})
}
fn assert_literal(raw: &RawExpressionValue, expected: &Value) {
    let e = descriptor(expected);
    let Term::Literal(rdf) = raw.rdf().unwrap() else {
        panic!("expected literal")
    };
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!("source metadata lost")
    };
    assert_eq!(rdf.value(), text(&e, "lexical"));
    let datatype = match &metadata.datatype {
        SourceDatatype::Absent => Value::Null,
        SourceDatatype::Specified(v) => json!(v.as_str()),
        SourceDatatype::Unknown => panic!("known datatype lost"),
    };
    assert_eq!(datatype, e["datatype"]);
    assert_eq!(
        metadata.language_original,
        SourceFact::Known(e["language"].as_str().map(str::to_owned))
    );
    assert_eq!(
        metadata.ill_typed,
        SourceFact::Known(e["ill_typed"].as_bool())
    );
    let cache = &e["cache"];
    match text(cache, "family") {
        "String" => {
            assert_eq!(
                metadata.cached_value_kind,
                SourceFact::Known(SourceLiteralCacheKind::String)
            );
            assert!(matches!(raw.numeric(), NumericCacheState::NotNumeric));
            assert_eq!(rdf.value(), text(cache, "value"));
        }
        "Integer" | "Decimal" => {
            assert_eq!(
                metadata.cached_value_kind,
                SourceFact::Known(SourceLiteralCacheKind::Numeric)
            );
            let NumericCacheState::Known(n) = raw.numeric() else {
                panic!("numeric payload lost")
            };
            assert_eq!(n.datatype().as_ref(), rdf.datatype());
            match (n.value(), text(cache, "family")) {
                (NumericValue::Integer(v), "Integer") => {
                    assert_eq!(v.to_string(), text(cache, "value"))
                }
                (
                    NumericValue::Decimal(DecimalValue::Finite {
                        negative,
                        coefficient,
                        exponent,
                    }),
                    "Decimal",
                ) => {
                    assert_eq!(*negative, cache["negative"].as_bool().unwrap());
                    assert_eq!(coefficient.to_string(), text(cache, "coefficient"));
                    assert_eq!(exponent.to_string(), text(cache, "exponent"));
                }
                _ => panic!("numeric family/value differs: {:?}", n.value()),
            }
        }
        "ErrorObject" => {
            assert_eq!(
                metadata.cached_value_kind,
                SourceFact::Known(SourceLiteralCacheKind::ErrorObject)
            );
            let NumericCacheState::ErrorObject(error) = raw.numeric() else {
                panic!("actual error cache lost")
            };
            let source = error.source_exception().unwrap();
            let class = match source.class {
                SourceExceptionClass::Sparql => "SPARQLError",
                SourceExceptionClass::SparqlType => "SPARQLTypeError",
                SourceExceptionClass::NotBound => "NotBoundError",
                SourceExceptionClass::AlreadyBound => "AlreadyBound",
            };
            assert_eq!(class, text(cache, "class"));
            match &source.argument {
                SourceExceptionArgument::None => assert_eq!(cache["args"], json!([null])),
                SourceExceptionArgument::Text(s) => assert_eq!(cache["args"], json!([s])),
                SourceExceptionArgument::DecimalSignalClasses(_) => {
                    panic!("unexpected class-list in unchanged historical fixture")
                }
            }
        }
        other => panic!("unreceived expected cache {other}"),
    }
}
fn assert_output(row: &SourcePresenceSolution, expected: &Value) {
    let actual: BTreeMap<_, _> = row
        .variables
        .iter()
        .zip(&row.values)
        .filter_map(|(v, c)| c.as_ref().map(|c| (v.as_str(), c)))
        .collect();
    let wanted: BTreeMap<String, Value> = if let Some(items) = expected["local_items"].as_array() {
        items
            .iter()
            .map(|p| (text(&p[0], "value").to_owned(), p[1].clone()))
            .collect()
    } else {
        expected
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };
    assert_eq!(
        actual.keys().copied().collect::<Vec<_>>(),
        wanted.keys().map(String::as_str).collect::<Vec<_>>()
    );
    for (name, e) in wanted {
        match (actual[name.as_str()], e.is_null()) {
            (SolutionValue::ExplicitNone, true) => (),
            (SolutionValue::Rdf(raw), false) => assert_literal(raw, &e),
            _ => panic!("wrong cell domain {name}"),
        }
    }
}
#[derive(Debug)]
struct FailureIdentity;
#[derive(Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("context receiving real Dataset failure")
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
    fn arm(&self) {
        assert!(!self.armed.replace(true), "fault armed twice");
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
            trace: Rc::default(),
            identity: Arc::new(FailureIdentity),
        }
    }
}
impl<'a> QueryableDataset<'a> for ReceivingDataset {
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
            return Err(ConversionFailure(self.identity.clone()));
        }
        Ok(term)
    }
}
fn backend_fault(error: &QueryEvaluationError, dataset: &ReceivingDataset) {
    let QueryEvaluationError::Dataset(e) = error else {
        panic!("wrong Dataset error {error:?}")
    };
    assert!(Arc::ptr_eq(
        &e.downcast_ref::<ConversionFailure>()
            .expect("original backend type")
            .0,
        &dataset.identity
    ));
}
fn source_fault(error: &QueryEvaluationError, expected: &Value) {
    let class = expected["error"]["class"]
        .as_str()
        .or_else(|| expected["original_error"]["class"].as_str())
        .or_else(|| expected["original_raised"].as_str())
        .expect("frozen source error");
    match class {
        "AttributeError" => assert!(matches!(
            raw_numeric_fault(error),
            Some(RawNumericFault::SourceContext(
                SourceContextFault::EmptyValuesAttribute
            ))
        )),
        "Exception" => assert!(matches!(
            raw_numeric_fault(error),
            Some(RawNumericFault::SourceContext(
                SourceContextFault::NullGroupKey
            ))
        )),
        "decimal.FloatOperation" => {
            super::source_decimal_trap_receiving::assert_original_decimal_class(
                error,
                DecimalSignal::FloatOperation,
            )
        }
        "builtins.TypeError" => assert!(matches!(
            raw_numeric_fault(error),
            Some(RawNumericFault::SourceComparisonTypeError)
        )),
        _ => panic!("unreceived source error class {class}"),
    }
}
fn row_names(row: &Row, variables: &[Variable]) -> Vec<String> {
    row.source_local_order()
        .iter()
        .map(|&slot| variables[slot].as_str().to_owned())
        .collect()
}
fn row_cell<'a>(row: &'a Row, variables: &[Variable], name: &str) -> &'a TermCell {
    row.cell(variables.iter().position(|v| v.as_str() == name).unwrap())
        .unwrap()
}
fn raw_cell(cell: &TermCell) -> &CachedInternalTerm<Term> {
    let SolutionValue::Rdf(v) = cell else {
        panic!("expected actual RDF cell")
    };
    v
}
fn assert_internal_original(row: &Row, expected: &Value, variables: &[Variable]) {
    let items = expected.as_array().unwrap();
    assert_eq!(
        row_names(row, variables),
        items
            .iter()
            .map(|p| text(&p[0], "value").to_owned())
            .collect::<Vec<_>>()
    );
    for p in items {
        let cell = row_cell(row, variables, text(&p[0], "value"));
        match (cell, p[1].is_null()) {
            (SolutionValue::ExplicitNone, true) => (),
            (SolutionValue::Rdf(raw), false) => assert_literal(raw.source_raw().unwrap(), &p[1]),
            _ => panic!("original cell domain differs"),
        }
    }
}
fn assert_query_events(
    id: &str,
    c: &Value,
    events: &[Event],
    variables: &[Variable],
    initial: &Rc<Row>,
    expected_rows: &[Value],
) {
    let outputs: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::OutputRow { row, initial } = e {
                Some((row, initial))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        outputs.len(),
        expected_rows.len(),
        "{id}: real output rows before final column mapping"
    );
    for ((row, frame), expected) in outputs.iter().zip(expected_rows) {
        assert!(Rc::ptr_eq(frame, initial), "one immutable initial frame");
        if expected.get("local_items").is_some() {
            assert_internal_original(row, &expected["local_items"], variables);
            assert_internal_original(frame, &expected["initial_items"], variables);
        }
    }
    let right_starts = |probe: u8| {
        events.iter().filter(|e|matches!(e,Event::ChildStart{side:SourceChildSide::Right,probe:p,..} if *p==probe)).count()
    };
    let right_rows = |probe: u8| {
        events.iter().filter(|e|matches!(e,Event::ChildRow{side:SourceChildSide::Right,probe:p,..} if *p==probe)).count()
    };
    let wanted = &c["required_observations"];
    if let Some(n) = wanted["right_probe_starts"].as_u64() {
        assert_eq!(right_starts(1) + right_starts(2), n as usize);
    }
    if let Some(n) = wanted["second_probe_starts"].as_u64() {
        assert_eq!(right_starts(2), n as usize);
    }
    if let Some(n) = wanted["second_probe_rows_examined"].as_u64() {
        assert_eq!(right_rows(2), n as usize);
    }
    if let Some(n) = wanted["first_probe_yields"].as_u64() {
        assert_eq!(right_rows(1), n as usize);
    }
    if wanted["remembered_local"] == json!([]) {
        let remembered: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::OptionalRetry {
                    remembered_row,
                    initial: frame,
                    ..
                } = e
                {
                    assert!(Rc::ptr_eq(frame, initial));
                    Some(remembered_row)
                } else {
                    None
                }
            })
            .collect();
        assert!(!remembered.is_empty());
        for row in remembered {
            assert!(row.source_local_order().is_empty());
        }
    }
    if wanted["second_probe_tail_polled"] == false {
        let polls: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::ChildPoll {
                    side: SourceChildSide::Right,
                    probe: 2,
                    ordinal,
                    ..
                } = e
                {
                    Some(*ordinal)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(polls, vec![1]);
        let row = events
            .iter()
            .find_map(|e| {
                if let Event::ChildRow {
                    side: SourceChildSide::Right,
                    probe: 2,
                    row,
                    ..
                } = e
                {
                    Some(row)
                } else {
                    None
                }
            })
            .unwrap();
        assert_literal(
            raw_cell(row_cell(row, variables, "x"))
                .source_raw()
                .unwrap(),
            &wanted["second_probe_first_success_x"],
        );
    }
    if c["expected"]["left_polls"] == 0 {
        assert!(!events.iter().any(|e| matches!(
            e,
            Event::ChildPoll {
                side: SourceChildSide::Left,
                ..
            }
        )));
    }
    if c["expected"]["right_set_starts"] == 0 || wanted["right_set_starts"] == 0 {
        assert!(!events.iter().any(|e| matches!(
            e,
            Event::ChildStart {
                side: SourceChildSide::Right,
                probe: 0,
                ..
            }
        )));
    }
    if wanted["inner_join_starts"] == 0 {
        assert!(!events.iter().any(|e| matches!(e, Event::ChildStart { .. })));
    }
    match id {
        "CQ-J03" => {
            assert_eq!(right_starts(0), 1);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::RightSetComplete { .. }))
            );
            let left = events
                .iter()
                .find_map(|e| {
                    if let Event::ChildRow {
                        side: SourceChildSide::Left,
                        row,
                        ..
                    } = e
                    {
                        Some(row)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(
                raw_cell(row_cell(outputs[0].0, variables, "x"))
                    .same_transport_instance(raw_cell(row_cell(left, variables, "x")))
            );
        }
        "CQ-J04" => {
            let members = events
                .iter()
                .filter_map(|e| {
                    if let Event::RightSetComplete { retained_rows, .. } = e {
                        Some(retained_rows)
                    } else {
                        None
                    }
                })
                .next_back()
                .unwrap();
            assert_eq!(members.len(), 1);
            assert!(
                raw_cell(row_cell(outputs[0].0, variables, "x"))
                    .same_transport_instance(raw_cell(row_cell(&members[0], variables, "x")))
            );
        }
        "CQ-M04" | "CQ-M05" => {
            let right = events
                .iter()
                .find_map(|e| {
                    if let Event::RightSetComplete { retained_rows, .. } = e {
                        retained_rows.first()
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(
                row_names(right, variables),
                wanted["right_local_keys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            );
            if id == "CQ-M04" {
                let left = events
                    .iter()
                    .find_map(|e| {
                        if let Event::ChildRow {
                            side: SourceChildSide::Left,
                            row,
                            ..
                        } = e
                        {
                            Some(row)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                assert_eq!(row_names(left, variables), vec!["x", "y"]);
            } else {
                assert!(
                    !right
                        .source_local_order()
                        .contains(&variables.iter().position(|v| v.as_str() == "x").unwrap())
                );
                assert_literal(
                    raw_cell(row_cell(initial, variables, "x"))
                        .source_raw()
                        .unwrap(),
                    &wanted["right_lookup_x"],
                );
            }
        }
        "CQ2-C01" | "CQ2-C02" => {
            let name = if id == "CQ2-C01" { "a" } else { "b" };
            assert!(
                raw_cell(row_cell(outputs[0].0, variables, "x"))
                    .same_transport_instance(raw_cell(row_cell(initial, variables, name)))
            );
        }
        "CQ2-C07" => assert!(
            !raw_cell(row_cell(outputs[0].0, variables, "x"))
                .same_transport_instance(raw_cell(row_cell(initial, variables, "x")))
        ),
        "CQ2-R03" => {
            let x = raw_cell(row_cell(outputs[0].0, variables, "x"));
            let y = raw_cell(row_cell(outputs[0].0, variables, "y"));
            assert!(x.same_transport_instance(y));
            let (NumericCacheState::ErrorObject(x), NumericCacheState::ErrorObject(y)) =
                (x.numeric(), y.numeric())
            else {
                panic!("actual ErrorObject cache")
            };
            assert_eq!(x, y);
        }
        _ => (),
    }
}
fn receive_query(id: &str) {
    let c = case(id);
    let expected = &c["expected"];
    let session = session(&c);
    let dataset = ReceivingDataset::default();
    let prepared = prepare_numeric_query(
        text(&c["input"], "query"),
        dataset.clone(),
        session.clone(),
        initial(&c),
    )
    .unwrap_or_else(|e| panic!("{id}: prepare {e:?}"));
    let runtime = prepared
        .evaluator()
        .source_context
        .as_ref()
        .expect("actual source compiler runtime");
    let variables = runtime.variables.clone();
    let frame = runtime.initial.clone();
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    let prepared =
        prepared.with_source_context_observer(Rc::new(move |e| sink.borrow_mut().push(e)));
    let start = prepared.execute_source_presence();
    let rows = if expected["execute_start"] == "Err" {
        let error = match start {
            Err(e) => e,
            Ok(_) => panic!("{id}: required startup failure"),
        };
        source_fault(&error, expected);
        Vec::new()
    } else {
        let mut actual = start.unwrap_or_else(|e| panic!("{id}: unexpected startup {e:?}"));
        let mut rows = Vec::new();
        let prefix: Vec<Value> = if let Some(v) = expected["original_binding_rows"].as_array() {
            v.clone()
        } else if let Some(v) = expected["ordered_rows"].as_array() {
            v.clone()
        } else {
            expected["polls"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p.get("row").cloned())
                .collect()
        };
        for row in &prefix {
            let value = actual
                .next()
                .unwrap_or_else(|| panic!("{id}: missing row"))
                .unwrap_or_else(|e| panic!("{id}: early failure {e:?}"));
            assert_output(&value, row);
            rows.push(row.clone());
        }
        if expected.get("error").is_some() {
            let e = match actual.next().expect("first poll source failure") {
                Err(error) => error,
                Ok(_) => panic!("source failure required"),
            };
            source_fault(&e, expected);
        } else if let Some(polls) = expected["polls"].as_array() {
            let e = match actual.next().expect("prefix then source error") {
                Err(error) => error,
                Ok(_) => panic!("source failure required"),
            };
            source_fault(&e, polls.last().unwrap());
        }
        if expected.get("error").is_none() && expected.get("polls").is_none() {
            // This first exhaustion poll may execute an entire zero-output
            // generator or its remaining unsuccessful probes. It is real work,
            // not a forbidden callback after a previously observed None.
            assert!(actual.next().is_none(), "{id}: terminal exhaustion");
        }
        // If Err was just received, snapshot before the first post-error poll.
        // Otherwise the first terminal None has already been observed above.
        let before = (dataset.trace.counts(), events.borrow().len());
        assert!(actual.next().is_none(), "{id}: terminal/fatal fuse");
        assert!(actual.next().is_none());
        assert_eq!((dataset.trace.counts(), events.borrow().len()), before);
        rows
    };
    assert_query_events(id, &c, &events.borrow(), &variables, &frame, &rows);
    if let Some(flags) = expected.get("session_decimal_flags_exact") {
        assert_eq!(session.flags(), signals(flags), "{id}: exact sticky flags");
    }
    assert_eq!(
        dataset.trace.scans.get(),
        0,
        "scope does not grant graph access"
    );
}
fn protocol_error(name: &str) -> ErrorValue {
    let root = fixture();
    let e = &root["raw_protocol_values"][name];
    assert_eq!(e["source_class"], "SPARQLError");
    ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        "receiver diagnostic is not source args",
        SourceException {
            class: SourceExceptionClass::Sparql,
            argument: SourceExceptionArgument::Text(text(e, "source_argument").into()),
        },
    )
}
fn protocol_cells(
    c: &Value,
    dataset: &NumericDataset<'_, ReceivingDataset>,
    session: &QueryNumericSession,
) -> BTreeMap<String, TermCell> {
    let mut values = BTreeMap::new();
    let root = fixture();
    for (name, value) in root["values"].as_object().unwrap() {
        let raw = session
            .construct_source_direct_string(
                text(value, "lexical"),
                value["source_datatype"]
                    .as_str()
                    .map(|s| NamedNode::new(s).unwrap()),
                value["language_original"].as_str().map(str::to_owned),
                true,
            )
            .unwrap();
        values.insert(
            name.clone(),
            SolutionValue::Rdf(dataset.internalize_raw(raw).unwrap()),
        );
    }
    values.insert("None".into(), SolutionValue::ExplicitNone);
    values.insert(
        "E1".into(),
        SolutionValue::ReturnedError(protocol_error("E1")),
    );
    values.insert(
        "E2".into(),
        SolutionValue::ReturnedError(protocol_error("E2")),
    );
    if let Some(extra) = c["input"]["values"].as_object() {
        for (name, value) in extra {
            assert_eq!(value["source_literal"], "Unknown");
            let rdf = Literal::new_typed_literal(
                text(value, "rdf_lexical"),
                NamedNode::new(text(value, "rdf_datatype")).unwrap(),
            )
            .into();
            values.insert(
                name.clone(),
                SolutionValue::Rdf(
                    dataset
                        .internalize_raw(RawExpressionValue::from_import(rdf))
                        .unwrap(),
                ),
            );
        }
    }
    values
}
fn same_cell(actual: &TermCell, expected: &TermCell) {
    match (actual, expected) {
        (SolutionValue::Rdf(a), SolutionValue::Rdf(b)) => assert!(
            a.same_transport_instance(b),
            "actual source cell object retained"
        ),
        (SolutionValue::ExplicitNone, SolutionValue::ExplicitNone) => (),
        (SolutionValue::ReturnedError(a), SolutionValue::ReturnedError(b)) => {
            assert_eq!(a, b, "actual ErrorValue Arc retained")
        }
        _ => panic!("source cell domain mismatch"),
    }
}
fn protocol_row(pairs: &Value, variables: &[Variable], values: &BTreeMap<String, TermCell>) -> Row {
    let mut row = Row::with_capacity(variables.len());
    for pair in pairs.as_array().unwrap() {
        let slot = variables
            .iter()
            .position(|v| v.as_str() == pair[0].as_str().unwrap())
            .unwrap();
        row.set_cell(slot, values[pair[1].as_str().unwrap()].clone());
    }
    row
}
fn protocol_child(
    rows: Vec<Result<Row, Arc<FailureIdentity>>>,
    starts: Rc<Cell<usize>>,
    polls: Rc<Cell<usize>>,
) -> InternalTupleEvaluator<'static, CachedInternalTerm<Term>> {
    Rc::new(move |_current| {
        starts.set(starts.get() + 1);
        let mut rows = rows.clone().into_iter();
        let polls = polls.clone();
        Ok(Box::new(std::iter::from_fn(move || {
            polls.set(polls.get() + 1);
            rows.next().map(|r| {
                r.map_err(|identity| {
                    QueryEvaluationError::Dataset(Box::new(ConversionFailure(identity)))
                })
            })
        })))
    })
}
fn receive_protocol(id: &str) {
    let c = case(id);
    let input = &c["input"];
    let expected = &c["expected"];
    let session = session(&c);
    let backend = ReceivingDataset::default();
    let dataset = NumericDataset::new_with_literal_semantics(
        backend.clone(),
        QueryLiteralSemantics::SourceLiteralV1,
    );
    let values = protocol_cells(&c, &dataset, &session);
    let mut names = std::collections::BTreeSet::new();
    for side in ["left_rows", "right_rows"] {
        for row in input[side].as_array().unwrap() {
            if let Some(pairs) = row["local"].as_array() {
                for p in pairs {
                    names.insert(p[0].as_str().unwrap().to_owned());
                }
            }
        }
    }
    if let Some(pairs) = input["immutable_initial"].as_array() {
        for p in pairs {
            names.insert(p[0].as_str().unwrap().to_owned());
        }
    }
    let variables: Rc<[Variable]> = names
        .into_iter()
        .map(|v| Variable::new(v).unwrap())
        .collect::<Vec<_>>()
        .into();
    let empty = json!([]);
    let initial = Rc::new(protocol_row(
        input.get("immutable_initial").unwrap_or(&empty),
        &variables,
        &values,
    ));
    let mut objects = BTreeMap::new();
    let mut build = |side: &str| {
        input[side]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                if r.get("raise_original_marker").is_some() {
                    return Err(backend.identity.clone());
                }
                let row = protocol_row(&r["local"], &variables, &values);
                objects.insert(text(r, "row_object").to_owned(), row.clone());
                Ok(row)
            })
            .collect::<Vec<_>>()
    };
    let left_rows = build("left_rows");
    let right_rows = build("right_rows");
    let hash = session.source_hash().unwrap().clone();
    let rdf_hash = hash.clone();
    let events: Rc<RefCell<Vec<Event>>> = Rc::default();
    let sink = events.clone();
    let runtime = Rc::new(
        SourceContextRuntime::new(
            variables.clone(),
            initial,
            CancellationToken::new(),
            hash,
            Rc::new(|a, b| match (a, b) {
                (SolutionValue::Rdf(a), SolutionValue::Rdf(b)) => a
                    .source_context_equal(b)
                    .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e))),
                _ => Ok(a == b),
            }),
            Rc::new(move |v: &CachedInternalTerm<Term>| {
                v.source_raw()
                    .and_then(|v| rdf_hash.term_hash(v))
                    .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))
            }),
        )
        .with_observer(Rc::new(move |event| sink.borrow_mut().push(event))),
    );
    let left_starts = Rc::new(Cell::new(0));
    let left_polls = Rc::new(Cell::new(0));
    let right_starts = Rc::new(Cell::new(0));
    let right_polls = Rc::new(Cell::new(0));
    let left = protocol_child(left_rows, left_starts.clone(), left_polls.clone());
    let right = protocol_child(right_rows, right_starts.clone(), right_polls.clone());
    let evaluator = match text(input, "operation") {
        "Join" => source_join_evaluator(
            runtime,
            0,
            if input["mode"] == "lazy" {
                SourceJoinMode::Lazy
            } else {
                SourceJoinMode::MaterializedRightSet
            },
            left,
            right,
        ),
        "Minus" => source_minus_evaluator(runtime, 0, left, right),
        _ => panic!("unsupported protocol"),
    };
    let from = protocol_row(
        input.get("current_local").unwrap_or(&empty),
        &variables,
        &values,
    );
    let start = evaluator(from);
    if id == "CPQ10" {
        let error = match start {
            Err(e) => e,
            Ok(_) => panic!("right late error must abort start"),
        };
        backend_fault(&error, &backend);
        assert_eq!(left_starts.get(), 1);
        assert_eq!(left_polls.get(), 0);
        assert!(
            !events
                .borrow()
                .iter()
                .any(|e| matches!(e, Event::RightSetComplete { .. }))
        );
        return;
    }
    if id == "CA03" {
        let e = match start {
            Err(e) => e,
            Ok(_) => panic!("unknown RHS source hash must abort start"),
        };
        assert!(matches!(
            raw_numeric_fault(&e),
            Some(RawNumericFault::UnknownSourceLiteralIdentity)
        ));
        assert_eq!(left_polls.get(), 0);
        return;
    }
    let mut actual = start.unwrap_or_else(|e| panic!("{id}: start {e:?}"));
    if id == "CA05" {
        let e = actual
            .next()
            .unwrap()
            .expect_err("Unknown equality before disjointness");
        assert!(matches!(
            raw_numeric_fault(&e),
            Some(RawNumericFault::UnknownSourceLiteralIdentity)
        ));
        let counts = (left_polls.get(), right_polls.get());
        assert!(actual.next().is_none());
        assert_eq!(counts, (left_polls.get(), right_polls.get()));
        return;
    }
    let rows: Vec<_> = actual.by_ref().collect::<Result<_, _>>().unwrap();
    let counts = (left_polls.get(), right_polls.get());
    assert!(actual.next().is_none());
    assert_eq!(counts, (left_polls.get(), right_polls.get()));
    if id == "CA04" {
        assert_eq!(rows.len(), 1);
        assert!(rows[0].source_same_object(&objects["L"]));
        same_cell(row_cell(&rows[0], &variables, "x"), &values["U"]);
        return;
    }
    let wanted = expected["ordered_rows"].as_array().unwrap();
    assert_eq!(rows.len(), wanted.len());
    for (row, wanted) in rows.iter().zip(wanted) {
        assert_eq!(
            row_names(row, &variables),
            wanted
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p[0].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        for p in wanted.as_array().unwrap() {
            same_cell(
                row_cell(row, &variables, p[0].as_str().unwrap()),
                &values[p[1].as_str().unwrap()],
            );
        }
        if expected["new_row_object"] == true {
            assert!(
                objects
                    .values()
                    .all(|original| !row.source_same_object(original))
            );
        }
        if let Some(name) = expected["output_same_row_object"].as_str() {
            assert!(row.source_same_object(&objects[name]));
        }
    }
    let events = events.borrow();
    if let Some(n) = expected["right_set_members"].as_u64() {
        let members = events
            .iter()
            .find_map(|e| {
                if let Event::RightSetComplete { retained_rows, .. } = e {
                    Some(retained_rows)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(members.len(), n as usize);
        if let Some(name) = expected["right_representative_row"].as_str() {
            assert!(members[0].source_same_object(&objects[name]));
        }
    }
    if expected["right_complete_before_left_first_poll"] == true {
        let complete = events
            .iter()
            .position(|e| matches!(e, Event::RightSetComplete { .. }))
            .unwrap();
        let poll = events
            .iter()
            .position(|e| {
                matches!(
                    e,
                    Event::ChildPoll {
                        side: SourceChildSide::Left,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(complete < poll);
    }
    if let Some(n) = expected["right_starts"].as_u64() {
        assert_eq!(right_starts.get(), n as usize);
    }
    assert_eq!(backend.trace.scans.get(), 0);
}
fn receive_active_query(id: &str) {
    let c = case(id);
    let session = session(&c);
    let dataset = ReceivingDataset::default();
    let prepared = prepare_numeric_query(
        text(&c["input"], "query"),
        dataset.clone(),
        session,
        initial(&c),
    )
    .unwrap();
    if id == "CA10" {
        // Verify the actual sealed topology before choosing the first outer
        // child-start node. No test-supplied plan enters the execution path.
        let GraphPattern::Project { inner, .. } = prepared.plan() else {
            panic!("CA10 projection")
        };
        let GraphPattern::SourceContext {
            inner,
            annotation:
                SourceContextAnnotation::Join {
                    mode: SourceJoinMode::Lazy,
                },
        } = inner.as_ref()
        else {
            panic!("CA10 outer lazy Join")
        };
        let GraphPattern::Join { left, right, .. } = inner.as_ref() else {
            panic!("CA10 annotated Join")
        };
        for child in [left, right] {
            assert!(
                matches!(child.as_ref(), GraphPattern::SourceContext { inner, annotation: SourceContextAnnotation::Values { form: SourceValuesForm::Records } }
                if matches!(inner.as_ref(), GraphPattern::SourceValues { .. })),
                "CA10 direct VALUES children"
            );
        }
    }
    let outer_node = Cell::new(None);
    let token = prepared.evaluator().dataset.cancellation_token.clone();
    let events: Rc<RefCell<Vec<Event>>> = Rc::default();
    let sink = events.clone();
    let trace = dataset.trace.clone();
    let triggered = Rc::new(Cell::new(false));
    let trigger = triggered.clone();
    let mode = id.to_owned();
    let prepared = prepared.with_source_context_observer(Rc::new(move |event| {
        if mode == "CA10" && outer_node.get().is_none() {
            if let Event::ChildStart {
                node,
                side: SourceChildSide::Left,
                probe: 0,
                ..
            } = &event
            {
                outer_node.set(Some(*node));
            }
        }
        let hit = match mode.as_str() {
            "CA02" => matches!(
                &event,
                Event::ChildPoll {
                    side: SourceChildSide::Right,
                    probe: 1,
                    ordinal: 2,
                    ..
                }
            ),
            "CA06" => matches!(
                &event,
                Event::ChildStart {
                    side: SourceChildSide::Right,
                    probe: 2,
                    ..
                }
            ),
            "CA10" => matches!(
                &event,
                Event::ChildRow {
                    node,
                    side: SourceChildSide::Right,
                    probe: 0,
                    ..
                } if Some(*node) == outer_node.get()
            ),
            _ => false,
        };
        if hit {
            assert!(
                !trigger.replace(true),
                "trigger is an actual once-reached boundary"
            );
            if mode == "CA06" {
                token.cancel();
            } else {
                if mode == "CA10" {
                    assert!(
                        !sink
                            .borrow()
                            .iter()
                            .any(|e| matches!(e, Event::OutputRow { .. })),
                        "decoder cannot precede its actual input row"
                    );
                }
                trace.arm();
            }
        }
        sink.borrow_mut().push(event);
    }));
    if id == "CA01" {
        dataset.trace.arm();
    }
    let start = prepared.execute_source_presence();
    if id == "CA01" {
        let error = match start {
            Err(e) => e,
            Ok(_) => panic!("backend error must occur at execute start"),
        };
        backend_fault(&error, &dataset);
        assert_eq!(dataset.trace.armed_external.get(), 1);
        assert!(!events.borrow().iter().any(|e| matches!(
            e,
            Event::ChildPoll {
                side: SourceChildSide::Left,
                ..
            }
        )));
        return;
    }
    let mut rows = start.unwrap_or_else(|e| panic!("{id} unexpected start error {e:?}"));
    if id == "CA02" {
        let row = rows.next().unwrap().unwrap();
        let expected = &c["expected"]["prefix_literal_values"];
        let actual: BTreeMap<_, _> = row
            .variables
            .iter()
            .zip(&row.values)
            .filter_map(|(v, c)| c.as_ref().map(|c| (v.as_str(), c)))
            .collect();
        assert_eq!(
            actual.keys().copied().collect::<Vec<_>>(),
            vec!["v", "x", "z"]
        );
        for (name, value) in expected.as_object().unwrap() {
            let SolutionValue::Rdf(raw) = actual[name.as_str()] else {
                panic!("literal prefix")
            };
            let Term::Literal(rdf) = raw.rdf().unwrap() else {
                panic!("literal")
            };
            assert_eq!(rdf.value(), text(value, "lexical"));
            let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                panic!("source")
            };
            assert_eq!(
                metadata.ill_typed,
                SourceFact::Known(value["ill_typed"].as_bool())
            );
        }
        assert!(!triggered.get(), "second right poll has not been requested");
    }
    let error = match rows.next().expect("real active failure") {
        Err(error) => error,
        Ok(_) => panic!("active fatal required"),
    };
    if id == "CA06" {
        assert!(matches!(error, QueryEvaluationError::Cancelled));
        assert!(!events.borrow().iter().any(|e| matches!(
            e,
            Event::ChildStarted {
                side: SourceChildSide::Right,
                probe: 2,
                ..
            }
        )));
    } else {
        backend_fault(&error, &dataset);
        assert_eq!(dataset.trace.armed_external.get(), 1);
    }
    assert!(
        triggered.get(),
        "actual boundary armed/cancelled the existing resource"
    );
    let before = (dataset.trace.counts(), events.borrow().len());
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!((dataset.trace.counts(), events.borrow().len()), before);
    assert_eq!(dataset.trace.scans.get(), 0);
}
fn receive_guard(id: &str) {
    let c = case(id);
    let dataset = ReceivingDataset::default();
    let session = session(&c);
    if id == "CA08" {
        let prepared = prepare_numeric_query(
            "SELECT ?x WHERE {VALUES ?x {1}}",
            dataset.clone(),
            session,
            vec![],
        )
        .unwrap();
        let malformed = GraphPattern::SourceContext {
            annotation: SourceContextAnnotation::Join {
                mode: SourceJoinMode::Lazy,
            },
            inner: Box::new(GraphPattern::SourceValues {
                variables: vec![],
                bindings: vec![vec![]],
            }),
        };
        let mut variables = prepared
            .evaluator()
            .source_context
            .as_ref()
            .unwrap()
            .variables
            .to_vec();
        let result = prepared
            .evaluator()
            .graph_pattern_evaluator(&malformed, &mut variables)
            .0;
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("annotation mismatch compiled"),
        };
        assert!(matches!(
            raw_numeric_fault(&error),
            Some(RawNumericFault::InvalidSourceOrigin)
        ));
    } else {
        for (index, query) in c["input"]["queries"].as_array().unwrap().iter().enumerate() {
            let prepared = prepare_numeric_query(
                query.as_str().unwrap(),
                dataset.clone(),
                session.clone(),
                vec![],
            )
            .expect("actual supported syntax; compiler gate is not a syntax failure");
            let error = match prepared.execute_source_presence() {
                Err(error) => error,
                Ok(_) => panic!("deferred graph/operator unexpectedly executable"),
            };
            if index == 4 {
                // Existing expression compiler has its own unsupported EXISTS
                // diagnostic. The frozen claim is a real pre-row compile guard,
                // not a newly invented RawNumericFault variant or message.
                assert!(matches!(error, QueryEvaluationError::Unexpected(_)));
            } else {
                assert!(
                    matches!(
                        raw_numeric_fault(&error),
                        Some(RawNumericFault::UnsupportedSourceLiteralOperator(_))
                    ),
                    "{error:?}"
                );
            }
        }
        #[cfg(feature = "sep-0006")]
        {
            let prepared = prepare_numeric_query(
                "SELECT ?x WHERE {VALUES ?x {1}}",
                dataset.clone(),
                session,
                vec![],
            )
            .unwrap();
            let unit = GraphPattern::SourceValues {
                variables: vec![],
                bindings: vec![vec![]],
            };
            let lateral = GraphPattern::Lateral {
                left: Box::new(unit.clone()),
                right: Box::new(unit),
            };
            let mut variables = prepared
                .evaluator()
                .source_context
                .as_ref()
                .unwrap()
                .variables
                .to_vec();
            let error = match prepared
                .evaluator()
                .graph_pattern_evaluator(&lateral, &mut variables)
                .0
            {
                Err(e) => e,
                Ok(_) => panic!("unsupported Lateral compiled"),
            };
            assert!(matches!(
                raw_numeric_fault(&error),
                Some(RawNumericFault::UnsupportedSourceLiteralOperator(_))
            ));
        }
    }
    assert_eq!(dataset.trace.scans.get(), 0);
}
fn receive_active(id: &str) {
    match id {
        "CA01" | "CA02" | "CA06" | "CA10" => receive_active_query(id),
        "CA03" | "CA04" | "CA05" => receive_protocol(id),
        "CA07" => {
            let c = case(id);
            let mut scratch = c["input"]["scratch_initial"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect::<Vec<_>>();
            let before = scratch.clone();
            let result = reserve_set_table::<u8, ()>(&mut scratch, usize::MAX);
            assert!(matches!(result, Err(SourceSetError::Allocation)));
            assert_eq!(scratch, before);
        }
        "CA08" | "CA09" => receive_guard(id),
        _ => panic!("unknown active case"),
    }
}
#[test]
fn source_context_query_fixture_identity() {
    let root = fixture();
    let payload = json!({"environment":root["environment"],"values":root["values"],"raw_protocol_values":root["raw_protocol_values"],
        "source_query_cases":root["source_query_cases"],"historical_correction_query_cases":root["historical_correction_query_cases"],
        "raw_consumer_protocol_cases":root["raw_consumer_protocol_cases"],"active_safety_cases":root["active_safety_cases"]});
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        "6af638c4ac79c8f3ae098bf55540dc718762aa6c2919836ed7167a99e1843f16"
    );
    assert_eq!(root["source_query_cases"].as_array().unwrap().len(), 55);
    assert_eq!(
        root["historical_correction_query_cases"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
    assert_eq!(
        root["raw_consumer_protocol_cases"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert_eq!(root["active_safety_cases"].as_array().unwrap().len(), 10);
    assert!(!FIXTURE.contains("/Users/"));
    assert!(!FIXTURE.contains("/nix/store/"));
}

#[test]
fn source_context_query_cq_d01() {
    receive_query("CQ-D01");
}

#[test]
fn source_context_query_cq_d04() {
    receive_query("CQ-D04");
}

#[test]
fn source_context_query_cq_d05() {
    receive_query("CQ-D05");
}

#[test]
fn source_context_query_cq_d06() {
    receive_query("CQ-D06");
}

#[test]
fn source_context_query_cq_d07() {
    receive_query("CQ-D07");
}

#[test]
fn source_context_query_cq_d08() {
    receive_query("CQ-D08");
}

#[test]
fn source_context_query_cq_d09() {
    receive_query("CQ-D09");
}

#[test]
fn source_context_query_cq_d10() {
    receive_query("CQ-D10");
}

#[test]
fn source_context_query_cq_d11() {
    receive_query("CQ-D11");
}

#[test]
fn source_context_query_cq_d12() {
    receive_query("CQ-D12");
}

#[test]
fn source_context_query_cq_d13() {
    receive_query("CQ-D13");
}

#[test]
fn source_context_query_cq_d14() {
    receive_query("CQ-D14");
}

#[test]
fn source_context_query_cq_d15() {
    receive_query("CQ-D15");
}

#[test]
fn source_context_query_cq_v02() {
    receive_query("CQ-V02");
}

#[test]
fn source_context_query_cq_v03() {
    receive_query("CQ-V03");
}

#[test]
fn source_context_query_cq_v04() {
    receive_query("CQ-V04");
}

#[test]
fn source_context_query_cq_v05() {
    receive_query("CQ-V05");
}

#[test]
fn source_context_query_cq_v06() {
    receive_query("CQ-V06");
}

#[test]
fn source_context_query_cq_j03() {
    receive_query("CQ-J03");
}

#[test]
fn source_context_query_cq_j04() {
    receive_query("CQ-J04");
}

#[test]
fn source_context_query_cq_o01() {
    receive_query("CQ-O01");
}

#[test]
fn source_context_query_cq_o02() {
    receive_query("CQ-O02");
}

#[test]
fn source_context_query_cq_o03() {
    receive_query("CQ-O03");
}

#[test]
fn source_context_query_cq_o04() {
    receive_query("CQ-O04");
}

#[test]
fn source_context_query_cq_o05() {
    receive_query("CQ-O05");
}

#[test]
fn source_context_query_cq_o06() {
    receive_query("CQ-O06");
}

#[test]
fn source_context_query_cq_o07() {
    receive_query("CQ-O07");
}

#[test]
fn source_context_query_cq_o08() {
    receive_query("CQ-O08");
}

#[test]
fn source_context_query_cq_m01() {
    receive_query("CQ-M01");
}

#[test]
fn source_context_query_cq_m02() {
    receive_query("CQ-M02");
}

#[test]
fn source_context_query_cq_m03() {
    receive_query("CQ-M03");
}

#[test]
fn source_context_query_cq_m04() {
    receive_query("CQ-M04");
}

#[test]
fn source_context_query_cq_m05() {
    receive_query("CQ-M05");
}

#[test]
fn source_context_query_cq_j05() {
    receive_query("CQ-J05");
}

#[test]
fn source_context_query_cq_j06() {
    receive_query("CQ-J06");
}

#[test]
fn source_context_query_cq_j07() {
    receive_query("CQ-J07");
}

#[test]
fn source_context_query_cq_j08() {
    receive_query("CQ-J08");
}

#[test]
fn source_context_query_cq2_s01() {
    receive_query("CQ2-S01");
}

#[test]
fn source_context_query_cq2_s02() {
    receive_query("CQ2-S02");
}

#[test]
fn source_context_query_cq2_s03() {
    receive_query("CQ2-S03");
}

#[test]
fn source_context_query_cq2_s04() {
    receive_query("CQ2-S04");
}

#[test]
fn source_context_query_cq2_s05() {
    receive_query("CQ2-S05");
}

#[test]
fn source_context_query_cq2_s06() {
    receive_query("CQ2-S06");
}

#[test]
fn source_context_query_cq2_s07() {
    receive_query("CQ2-S07");
}

#[test]
fn source_context_query_cq2_c01() {
    receive_query("CQ2-C01");
}

#[test]
fn source_context_query_cq2_c02() {
    receive_query("CQ2-C02");
}

#[test]
fn source_context_query_cq2_c03() {
    receive_query("CQ2-C03");
}

#[test]
fn source_context_query_cq2_c04() {
    receive_query("CQ2-C04");
}

#[test]
fn source_context_query_cq2_c05() {
    receive_query("CQ2-C05");
}

#[test]
fn source_context_query_cq2_c06() {
    receive_query("CQ2-C06");
}

#[test]
fn source_context_query_cq2_c07() {
    receive_query("CQ2-C07");
}

#[test]
fn source_context_query_cq2_r01() {
    receive_query("CQ2-R01");
}

#[test]
fn source_context_query_cq2_r02() {
    receive_query("CQ2-R02");
}

#[test]
fn source_context_query_cq2_r03() {
    receive_query("CQ2-R03");
}

#[test]
fn source_context_query_cq2_r04() {
    receive_query("CQ2-R04");
}

#[test]
fn source_context_query_ch_h01() {
    receive_query("CH-H01");
}

#[test]
fn source_context_query_ch_h02() {
    receive_query("CH-H02");
}

#[test]
fn source_context_query_ch_h03() {
    receive_query("CH-H03");
}

#[test]
fn source_context_query_ch_h04() {
    receive_query("CH-H04");
}

#[test]
fn source_context_query_ch_h05() {
    receive_query("CH-H05");
}

#[test]
fn source_context_query_ch_h06() {
    receive_query("CH-H06");
}

#[test]
fn source_context_query_ch_h07() {
    receive_query("CH-H07");
}

#[test]
fn source_context_query_ch_h08() {
    receive_query("CH-H08");
}

#[test]
fn source_context_query_ch_h09() {
    receive_query("CH-H09");
}

#[test]
fn source_context_query_ch_h10() {
    receive_query("CH-H10");
}

#[test]
fn source_context_query_ch_h11() {
    receive_query("CH-H11");
}

#[test]
fn source_context_query_ch_h12() {
    receive_query("CH-H12");
}

#[test]
fn source_context_query_ch_h13() {
    receive_query("CH-H13");
}

#[test]
fn source_context_query_cpq01() {
    receive_protocol("CPQ01");
}

#[test]
fn source_context_query_cpq02() {
    receive_protocol("CPQ02");
}

#[test]
fn source_context_query_cpq03() {
    receive_protocol("CPQ03");
}

#[test]
fn source_context_query_cpq04() {
    receive_protocol("CPQ04");
}

#[test]
fn source_context_query_cpq05() {
    receive_protocol("CPQ05");
}

#[test]
fn source_context_query_cpq06() {
    receive_protocol("CPQ06");
}

#[test]
fn source_context_query_cpq07() {
    receive_protocol("CPQ07");
}

#[test]
fn source_context_query_cpq08() {
    receive_protocol("CPQ08");
}

#[test]
fn source_context_query_cpq09() {
    receive_protocol("CPQ09");
}

#[test]
fn source_context_query_cpq10() {
    receive_protocol("CPQ10");
}

#[test]
fn source_context_query_ca01() {
    receive_active("CA01");
}

#[test]
fn source_context_query_ca02() {
    receive_active("CA02");
}

#[test]
fn source_context_query_ca03() {
    receive_active("CA03");
}

#[test]
fn source_context_query_ca04() {
    receive_active("CA04");
}

#[test]
fn source_context_query_ca05() {
    receive_active("CA05");
}

#[test]
fn source_context_query_ca06() {
    receive_active("CA06");
}

#[test]
fn source_context_query_ca07() {
    receive_active("CA07");
}

#[test]
fn source_context_query_ca08() {
    receive_active("CA08");
}

#[test]
fn source_context_query_ca09() {
    receive_active("CA09");
}

#[test]
fn source_context_query_ca10() {
    receive_active("CA10");
}
