//! Root-authored comparisons to fixed original observations. The production
//! parser/planner/compiler, source Dataset, tuple consumers and decoder run
//! together. No candidate result supplies an oracle; no independent reviewer,
//! named Dataset, mutation/time/path lifecycle or public qualification claimed.
use super::*;
use crate::dataset::nullable_numeric::NumericDataset;
use crate::dataset::source_memory::{SourceGraphError, SourceMemoryGraph};
use crate::numeric::source_bgp_order::{SourceTripleOrderCell, source_reorder_triples};
use crate::numeric::{
    LiteralSourceState, NumericCacheState, NumericInitialBinding, NumericInitialValue,
    PreparedSourceLiteralRegistry, QueryContextSemantics, QueryGroupSemantics,
    QueryNumericCapabilities, SourceDatatype, SourceFact, SourceHashPolicy, SourceHashRuntime,
    SourceLiteralCacheKind,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use oxrdf::{BlankNode, NamedNode};
use semantica_numeric::{DecimalContext, NumericLimits, NumericValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/source_bgp_query_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "c6e9bed1fec3e0328095f52386f365444eb3d0f29202ebafae1874762ef23988"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(
        value["source_records"]["order"],
        "59643189443a2bb79f7b715de23df615badb3e952384f8ef51ea77794a813e45"
    );
    assert_eq!(
        value["source_records"]["runtime_order"],
        "97f60f53712c3286a2bfaa7d8c785197319f5bdeffb9dde2abecb960f29c579c"
    );
    assert_eq!(
        value["source_records"]["graph"],
        "0554a023b7d7ee4765b42e33692bf51fd63bfe280636af98da1a2bf4eebf0382"
    );
    assert_eq!(
        value["source_records"]["supplement"],
        "5d6b28ad616da50ffd2e6df2b22e53300816c3fcd80ab4d9d267aaf1cfe9470b"
    );
    assert_eq!(
        value["source_records"]["dataset"],
        "dc02204249d4ebbea592e66b5df30dc9b0ddf8a52bad47df9fab16230b21bba2"
    );
    assert_eq!(value["graph_queries"].as_array().unwrap().len(), 34);
    assert_eq!(
        value["pending_original_queries"].as_array().unwrap().len(),
        2
    );
    value
}
pub(super) fn session() -> Rc<QueryNumericSession> {
    session_with_decimal(DecimalContext::default())
}
pub(super) fn session_with_decimal(decimal: DecimalContext) -> Rc<QueryNumericSession> {
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            decimal,
            NumericLimits::default(),
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
fn iri(value: &str) -> RawExpressionValue {
    RawExpressionValue::from_import(NamedNode::new(value).unwrap().into()).ensure_source_object()
}
fn not_bound() -> ErrorValue {
    ErrorValue::with_source(
        SparqlErrorKind::NotBound,
        "None",
        SourceException {
            class: SourceExceptionClass::NotBound,
            argument: SourceExceptionArgument::None,
        },
    )
}
fn initial(case: &Value) -> Vec<NumericInitialBinding> {
    case["input"]["initial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| NumericInitialBinding {
            variable: Variable::new(v["variable"].as_str().unwrap()).unwrap(),
            value: match v["kind"].as_str().unwrap() {
                "none" => NumericInitialValue::SourceNone,
                "error" => NumericInitialValue::SourceError(not_bound()),
                "literal" => NumericInitialValue::SourceDirectString {
                    lexical: v["lexical"].as_str().unwrap().into(),
                    datatype: v["datatype"].as_str().map(|v| NamedNode::new(v).unwrap()),
                    language: None,
                    normalize: v["normalize"].as_bool().unwrap(),
                },
                _ => panic!("unknown original initial constructor"),
            },
        })
        .collect()
}
pub(super) fn dataset(
    session: &Rc<QueryNumericSession>,
    token: Option<Arc<AtomicBool>>,
) -> SourceMemoryGraph {
    let graph = dataset_inputs(session, token, &fixture()["data_inputs"]);
    assert_eq!(graph.len(), 6);
    graph
}
pub(super) fn dataset_inputs(
    session: &Rc<QueryNumericSession>,
    token: Option<Arc<AtomicBool>>,
    inputs: &Value,
) -> SourceMemoryGraph {
    let mut graph = SourceMemoryGraph::new(Rc::clone(session), token).unwrap();
    for input in inputs.as_array().unwrap() {
        let object = if let Some(value) = input["object_iri"].as_str() {
            iri(value)
        } else {
            session
                .construct_source_direct_string(
                    input["lexical"].as_str().unwrap(),
                    input["datatype"]
                        .as_str()
                        .map(|s| NamedNode::new(s).unwrap()),
                    None,
                    input["normalize"].as_bool().unwrap(),
                )
                .unwrap()
        };
        let s = graph
            .admit_source(iri(input["subject"].as_str().unwrap()))
            .unwrap();
        let p = graph
            .admit_source(iri(input["predicate"].as_str().unwrap()))
            .unwrap();
        let o = graph.admit_source(object).unwrap();
        graph.insert([s, p, o]).unwrap();
    }
    graph
}
pub(super) fn term_json(raw: &RawExpressionValue, session: &QueryNumericSession) -> Value {
    let hash = session.source_hash().unwrap().term_hash(raw).unwrap();
    let repr = session
        .source_repr(&SolutionValue::Rdf(raw.clone()))
        .unwrap();
    match raw.rdf().unwrap() {
        Term::NamedNode(v) => {
            json!({"kind":"URIRef", "value":v.as_str(), "hash":hash, "repr":repr})
        }
        Term::BlankNode(v) => json!({"kind":"BNode", "value":v.as_str(), "hash":hash, "repr":repr}),
        Term::Literal(v) => {
            let LiteralSourceState::Literal(meta) = raw.literal_source() else {
                panic!("actual source metadata missing");
            };
            let dt = match &meta.datatype {
                SourceDatatype::Absent => Value::Null,
                SourceDatatype::Specified(dt) => json!(dt.as_str()),
                SourceDatatype::Unknown => panic!("unknown source datatype"),
            };
            let SourceFact::Known(language) = &meta.language_original else {
                panic!("unknown language facts");
            };
            let SourceFact::Known(ill_typed) = meta.ill_typed else {
                panic!("unknown cast facts");
            };
            let (cache_type, cache) = match (&meta.cached_value_kind, raw.numeric()) {
                (
                    SourceFact::Known(SourceLiteralCacheKind::String),
                    NumericCacheState::NotNumeric,
                ) => ("builtins.str", json!(v.value())),
                (SourceFact::Known(SourceLiteralCacheKind::Boolean(b)), _) => {
                    ("builtins.bool", json!(b))
                }
                (SourceFact::Known(SourceLiteralCacheKind::DateTime(Some(d))), _) => {
                    let (year, month, day) = d.date_parts();
                    let [hour, minute, second, microsecond] = d.time_parts();
                    (
                        "datetime.datetime",
                        json!({"datetime_iso":d.isoformat(), "fold":0,
                        "year":year, "month":month, "day":day, "hour":hour, "minute":minute,
                        "second":second, "microsecond":microsecond,
                        "utc_offset_seconds":d.offset_micros().map(|v| v as f64 / 1_000_000.0)}),
                    )
                }
                (SourceFact::Known(SourceLiteralCacheKind::Time(Some(d))), _) => {
                    let [hour, minute, second, microsecond] = d.parts();
                    (
                        "datetime.time",
                        json!({"time_iso":d.isoformat(), "fold":0, "hour":hour, "minute":minute,
                        "second":second,"microsecond":microsecond,
                        "utc_offset_seconds":d.offset_micros().map(|v| v as f64 / 1_000_000.0)}),
                    )
                }
                (SourceFact::Known(SourceLiteralCacheKind::Date(Some(d))), _) => {
                    let (year, month, day) = d.parts();
                    (
                        "datetime.date",
                        json!({"date_iso":d.isoformat(),"year":year,"month":month,"day":day,
                        "repr":format!("datetime.date({year}, {month}, {day})")}),
                    )
                }
                (SourceFact::Known(SourceLiteralCacheKind::Duration(cache)),_) => {
                    match &cache.value {
                        None => ("builtins.NoneType",Value::Null),
                        Some(value) => {
                            let d=value.delta();let (days,seconds,microseconds)=d.parts();
                            let delta=json!({"timedelta_days":days,"seconds":seconds,"microseconds":microseconds,"repr":d.repr()});
                            if let Some((years,months))=value.calendar_parts() {
                                let decimal=|v:&semantica_temporal::DurationDecimal| decimal_cache_json(&v.as_decimal());
                                ("rdflib.xsd_datetime.Duration",json!({"duration_years":decimal(years),"duration_months":decimal(months),"duration_timedelta":delta,"repr":value.repr()}))
                            } else {("datetime.timedelta",delta)}
                        }
                    }
                }
                (SourceFact::Known(SourceLiteralCacheKind::DayTimeDuration(Some(d))), _) => {
                    let (days, seconds, microseconds) = d.parts();
                    (
                        "datetime.timedelta",
                        json!({"timedelta_days":days,"seconds":seconds,
                        "microseconds":microseconds,"repr":d.repr()}),
                    )
                }
                (
                    SourceFact::Known(
                        SourceLiteralCacheKind::None
                        | SourceLiteralCacheKind::DateTime(None)
                        | SourceLiteralCacheKind::Date(None)
                        | SourceLiteralCacheKind::Time(None)
                        | SourceLiteralCacheKind::DayTimeDuration(None)
                        | SourceLiteralCacheKind::UnmappedDatatype,
                    ),
                    _,
                ) => ("builtins.NoneType", Value::Null),
                (
                    SourceFact::Known(SourceLiteralCacheKind::Numeric),
                    NumericCacheState::Known(n),
                ) => match n.value() {
                    NumericValue::Integer(n) => (
                        "builtins.int",
                        serde_json::from_str(&n.to_string()).unwrap(),
                    ),
                    NumericValue::Binary64(n) => (
                        "builtins.float",
                        json!({"float_bits":format!("{:016x}",n.to_bits()),
                            "repr":semantica_json::python_float_repr(*n).unwrap()}),
                    ),
                    NumericValue::Decimal(n) => ("decimal.Decimal", decimal_cache_json(n)),
                },
                (
                    SourceFact::Known(SourceLiteralCacheKind::ErrorObject),
                    NumericCacheState::ErrorObject(error),
                ) => {
                    let source = error
                        .source_exception()
                        .expect("original error cache constructor");
                    let class = match source.class {
                        SourceExceptionClass::Sparql => "rdflib.plugins.sparql.sparql.SPARQLError",
                        SourceExceptionClass::SparqlType => {
                            "rdflib.plugins.sparql.sparql.SPARQLTypeError"
                        }
                        SourceExceptionClass::NotBound => {
                            "rdflib.plugins.sparql.sparql.NotBoundError"
                        }
                        SourceExceptionClass::AlreadyBound => {
                            "rdflib.plugins.sparql.sparql.AlreadyBound"
                        }
                    };
                    (
                        class,
                        source_function_registry_receiving::sparql_json(error),
                    )
                }
                _ => panic!("unreceived graph cache family"),
            };
            json!({"kind":"Literal", "lexical":v.value(), "datatype":dt, "language":language,
                "ill_typed":ill_typed, "cache_type":cache_type, "cache":cache, "hash":hash, "repr":repr})
        }
        #[cfg(feature = "sparql-12")]
        Term::Triple(_) => panic!("unreceived term kind"),
    }
}
fn decimal_cache_json(value: &semantica_numeric::DecimalValue) -> Value {
    use semantica_numeric::DecimalValue;
    let digits = |s: &str| s.bytes().map(|v| v - b'0').collect::<Vec<_>>();
    let (negative, coefficient, exponent, text) = match value {
        DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        } => {
            let s = coefficient.to_str_radix(10);
            // Observation encoding is bounded independently from evaluation.
            let e: i64 = exponent
                .to_string()
                .parse()
                .expect("observed exponent fits i64");
            let adjusted = e + s.len() as i64 - 1;
            let text = if e > 0 || adjusted < -6 {
                format!(
                    "{}{}{}E{:+}",
                    if *negative { "-" } else { "" },
                    &s[..1],
                    if s.len() > 1 {
                        format!(".{}", &s[1..])
                    } else {
                        String::new()
                    },
                    adjusted
                )
            } else {
                semantica_numeric::format_decimal_fixed(value, &NumericLimits::default()).unwrap()
            };
            (*negative, digits(&s), json!(e), text)
        }
        DecimalValue::Infinity { negative } => (
            *negative,
            vec![0],
            json!("F"),
            format!("{}Infinity", if *negative { "-" } else { "" }),
        ),
        DecimalValue::NaN {
            negative,
            signaling,
            payload,
        } => (
            *negative,
            digits(payload),
            json!(if *signaling { "N" } else { "n" }),
            format!(
                "{}{}NaN{}",
                if *negative { "-" } else { "" },
                if *signaling { "s" } else { "" },
                payload
            ),
        ),
    };
    json!({"decimal_sign":if negative {1} else {0}, "digits":coefficient, "exponent":exponent, "str":text})
}

fn without_token(value: &Value) -> Value {
    let mut result = value.clone();
    result.as_object_mut().unwrap().remove("object_token");
    result
}
fn exception(error: &QueryEvaluationError) -> Value {
    if let Some(error) = crate::numeric::raw_numeric_fault(error)
        .and_then(RawNumericFault::source_language_exception)
    {
        return source_function_registry_receiving::exception_json(error);
    }
    if matches!(
        crate::numeric::raw_numeric_fault(error),
        Some(RawNumericFault::SourceContext(
            crate::numeric::SourceContextFault::EmptyValuesAttribute
        ))
    ) {
        return json!({"kind":"Exception", "module":"builtins", "class_name":"AttributeError",
            "args":["'list' object has no attribute 'name'"]});
    }
    if let Some(nullable::NullableAggregateError::UncaughtSparql(error)) =
        nullable::nullable_aggregate_error(error)
    {
        let source = error
            .source_exception()
            .expect("source error constructor retained");
        assert_eq!(error.kind(), SparqlErrorKind::OtherSparql);
        assert_eq!(source.class, SourceExceptionClass::AlreadyBound);
        assert_eq!(source.argument, SourceExceptionArgument::None);
        return json!({"kind":"Exception", "module":"rdflib.plugins.sparql.sparql", "class_name":"AlreadyBound", "args":[null]});
    }
    if let QueryEvaluationError::Unexpected(error) = error {
        if let Some(error) = error.downcast_ref::<source_compvalue::SourceCompValueError>() {
            return json!({"kind":"Exception", "module":error.module, "class_name":error.class, "args":[error.argument]});
        }
    }
    panic!("unexpected candidate query failure: {error:?}")
}
fn receive_query(id: &str) {
    let root = fixture();
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    receive_query_case(case, &root["data_inputs"]);
}

/// Shared test adapter; the production parser/plan/store/decoder are unchanged
/// by the choice of a frozen original input. New receivers must explicitly
/// bind their facts to the original six-fact factory used here.
pub(super) fn receive_query_case(case: &Value, data_inputs: &Value) {
    assert_eq!(data_inputs, &fixture()["data_inputs"]);
    receive_query_using(case, data_inputs, None, None, None);
}
pub(super) fn receive_path_query_case(case: &Value, data_inputs: &Value, scans: &Value) {
    receive_query_using(case, data_inputs, Some(scans), None, None);
}
pub(super) fn receive_query_case_with_constructor(
    case: &Value,
    data_inputs: &Value,
    constructor: spargebra::SourceBlankNodeConstructor,
) {
    assert_eq!(data_inputs, &fixture()["data_inputs"]);
    receive_query_using(case, data_inputs, None, Some(constructor), None);
}
pub(super) fn receive_query_case_with_registry(
    case: &Value,
    data_inputs: &Value,
    session: Rc<QueryNumericSession>,
) {
    assert_eq!(data_inputs, &fixture()["data_inputs"]);
    assert!(session.source_functions().is_some());
    receive_query_using(case, data_inputs, None, None, Some(session));
}
pub(super) fn receive_query_case_without_registry(
    case: &Value,
    data_inputs: &Value,
    session: Rc<QueryNumericSession>,
) {
    assert_eq!(data_inputs, &fixture()["data_inputs"]);
    assert!(session.source_functions().is_none());
    receive_query_using(case, data_inputs, None, None, Some(session));
}
fn receive_query_using(
    case: &Value,
    data_inputs: &Value,
    scans: Option<&Value>,
    blank_constructor: Option<spargebra::SourceBlankNodeConstructor>,
    registry_session: Option<Rc<QueryNumericSession>>,
) {
    let id = case["input"]["id"].as_str().unwrap();
    let session = registry_session.unwrap_or_else(session);
    if !session.admits_source_calendar_duration() {
        source_function_registry_receiving::lifecycle("before_parse");
    }
    let scan_events = Rc::new(std::cell::RefCell::new(Vec::<
        [Option<SolutionValue<RawExpressionValue>>; 3],
    >::new()));
    let mut graph = dataset_inputs(&session, None, data_inputs);
    if scans.is_some() {
        let events = scan_events.clone();
        graph = graph.with_scan_observer(Rc::new(move |cells| events.borrow_mut().push(cells)));
    }
    let native_path = scans.is_some();
    let factory = if native_path {
        numeric_query::prepare_source_path_numeric_query
    } else {
        numeric_query::prepare_source_graph_numeric_query
    };
    let prepared = if let Some(constructor) = blank_constructor {
        assert!(
            !native_path,
            "constructor replay must declare its receiving factory"
        );
        numeric_query::prepare_source_graph_numeric_query_with_constructor(
            case["input"]["query"].as_str().unwrap(),
            graph,
            Rc::clone(&session),
            initial(case),
            Some(constructor),
        )
    } else {
        factory(
            case["input"]["query"].as_str().unwrap(),
            graph,
            Rc::clone(&session),
            initial(case),
        )
    };
    if let Err(numeric_query::NumericPrepareError::InitialLiteralConstruction(error))=&prepared {
        assert_eq!(case["error_stage"],"initial_construction","{id} actual constructor phase");
        let RawNumericFault::SourceLanguageException(error)=error else {panic!("{id} unreceived initial constructor failure: {error:?}")};
        assert_eq!(source_function_registry_receiving::exception_json(error),case["error"],"{id} initial constructor exception");
        let mut flags=session.flags().iter().map(|signal|format!("{signal:?}")).collect::<Vec<_>>();flags.sort();
        assert_eq!(json!(flags),case["flags"],"{id} initial constructor decimal flags");
        return;
    }
    if case["error_stage"] == "parse_query" {
        let actual = match prepared {
            Err(numeric_query::NumericPrepareError::LiteralConstruction(
                RawNumericFault::SourceParserLiteralType { argument },
            )) => {
                json!({"kind":"Exception","module":"builtins","class_name":"TypeError","args":[argument]})
            }
            Err(numeric_query::NumericPrepareError::SourceSyntax(
                spargebra::SourceParserError::ParseException {
                    input,
                    location,
                    message,
                },
            )) => {
                json!({"kind":"Exception","module":"pyparsing.exceptions","class_name":"ParseException","args":[input,location,message]})
            }
            Err(numeric_query::NumericPrepareError::SourceSyntax(
                spargebra::SourceParserError::ValueError { message },
            )) => {
                json!({"kind":"Exception","module":"builtins","class_name":"ValueError","args":[message]})
            }
            Err(error) => {
                panic!("{id} source parser exception has an unreceived constructor: {error:?}")
            }
            Ok(_) => panic!("{id} source parser error was accepted"),
        };
        assert_eq!(actual, case["error"], "{id} source parse exception");
        assert_eq!(
            session.flags(),
            semantica_numeric::DecimalSignals::default()
        );
        return;
    }
    let prepared = prepared.unwrap_or_else(|error| panic!("{id} preparation: {error:?}"));
    if native_path {
        super::source_path_query_receiving::receive_prepared_metadata(&prepared, case, &session);
    }
    source_function_registry_receiving::lifecycle("after_translate");
    let diagnostic_plan = format!("{:?}", prepared.plan());
    let columns = source_context::source_query_projection(prepared.plan()).unwrap();
    assert_eq!(
        json!(columns.iter().map(Variable::as_str).collect::<Vec<_>>()),
        json!(
            case["columns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["value"].as_str().unwrap())
                .collect::<Vec<_>>()
        ),
        "{id} column order"
    );
    source_function_registry_receiving::lifecycle("evaluate_start");
    let execution = prepared.execute_source_presence();
    if case["error_stage"] == "evaluate_start" {
        let error = match execution {
            Err(error) => error,
            Ok(_) => panic!("{id} source start error was deferred or lost"),
        };
        assert_eq!(exception(&error), case["error"], "{id} start exception");
        assert_eq!(
            session.flags(),
            semantica_numeric::DecimalSignals::default()
        );
        return;
    }
    let mut rows = execution.unwrap_or_else(|e| panic!("{id} start: {e:?}"));
    source_function_registry_receiving::lifecycle("after_evaluate");
    if native_path {
        assert!(
            scan_events.borrow().is_empty(),
            "{id} scan occurred before first query poll"
        );
    }
    let wanted = case["rows"].as_array().unwrap();
    let mut objects: Vec<(RawExpressionValue, String)> = Vec::new();
    for (ordinal, wanted) in wanted.iter().enumerate() {
        source_function_registry_receiving::lifecycle(&format!("before_poll_{ordinal}"));
        let actual = rows
            .next()
            .unwrap_or_else(|| panic!("{id} missing row {ordinal}"))
            .unwrap_or_else(|e| panic!("{id} row {ordinal}: {e:?}"));
        if let Some(scans) = scans {
            assert_eq!(
                scan_events.borrow().len(),
                scans["scan_starts_after_poll"][ordinal].as_u64().unwrap() as usize,
                "{id} scans after row {ordinal}"
            );
        }
        let want: BTreeMap<_, _> = wanted
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (pair[0]["value"].as_str().unwrap(), &pair[1]))
            .collect();
        let mut got = BTreeMap::new();
        for (v, cell) in actual.variables.iter().zip(&actual.values) {
            if let Some(cell) = cell {
                let value = match cell {
                    SolutionValue::Rdf(raw) => {
                        let w = want.get(v.as_str()).unwrap();
                        objects.push((raw.clone(), w["object_token"].as_str().unwrap().to_owned()));
                        source_function_registry_receiving::row(ordinal, v, raw);
                        term_json(raw, &session)
                    }
                    SolutionValue::ExplicitNone => Value::Null,
                    SolutionValue::ReturnedError(_) => panic!("unreceived returned error output"),
                };
                got.insert(v.as_str(), value);
            }
        }
        let want: BTreeMap<_, _> = want
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    if v.is_null() {
                        Value::Null
                    } else {
                        without_token(v)
                    },
                )
            })
            .collect();
        assert_eq!(
            got, want,
            "{id} row {ordinal}; actual compiled plan: {diagnostic_plan}"
        );
    }
    source_function_registry_receiving::lifecycle(&format!("before_poll_{}", wanted.len()));
    if case["status"] == "original_error" {
        assert_eq!(
            case["error_stage"],
            if wanted.is_empty() {
                "first_poll".to_owned()
            } else {
                format!("poll_after_{}_rows", wanted.len())
            }
        );
        let error = rows
            .next()
            .expect("original failure must be an actual iterator item")
            .err()
            .expect("original exception became a row");
        assert_eq!(
            exception(&error),
            if session.source_functions().is_some() {
                source_function_registry_receiving::without_tokens(&case["error"])
            } else {
                case["error"].clone()
            },
            "{id} exception constructor/domain/argument"
        );
    } else {
        assert!(
            rows.next().is_none(),
            "{id} extra result; actual compiled plan: {diagnostic_plan}"
        );
    }
    if let Some(scans) = scans {
        let count = if case["status"] == "original_error" {
            scans["scan_starts_after_error"].as_u64().unwrap()
        } else {
            scans["scan_starts_after_poll"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_u64()
                .unwrap()
        };
        assert_eq!(
            scan_events.borrow().len(),
            count as usize,
            "{id} terminal scan count"
        );
        let events = scan_events.borrow();
        let expected = scans["scan_starts"].as_array().unwrap();
        assert_eq!(events.len(), expected.len());
        for (ordinal, (actual, want)) in events.iter().zip(expected).enumerate() {
            let encoded:Vec<Value>=actual.iter().map(|v|match v {
                None|Some(SolutionValue::ExplicitNone)=>Value::Null,
                Some(SolutionValue::Rdf(v))=>term_json(v,&session),
                Some(SolutionValue::ReturnedError(v))=>{let source=v.source_exception().unwrap();assert_eq!(source.class,SourceExceptionClass::NotBound);assert_eq!(source.argument,SourceExceptionArgument::None);json!({"kind":"Exception","module":"rdflib.plugins.sparql.sparql","class_name":"NotBoundError","args":[null]})},
            }).collect();
            let expected: Vec<Value> = want
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    if v.is_null() {
                        Value::Null
                    } else {
                        without_token(v)
                    }
                })
                .collect();
            assert_eq!(encoded, expected, "{id} actual scan {ordinal} selectors");
            for (cell, want) in actual.iter().zip(want.as_array().unwrap()) {
                if let Some(SolutionValue::Rdf(raw)) = cell {
                    objects.push((raw.clone(), want["object_token"].as_str().unwrap().into()));
                }
            }
        }
    }
    for _ in 0..3 {
        assert!(
            rows.next().is_none(),
            "terminal source row iterator resumed"
        );
    }
    for (a, wa) in &objects {
        for (b, wb) in &objects {
            assert_eq!(
                a.same_source_object(b),
                wa == wb,
                "{id} returned source-object sharing"
            );
        }
    }
    let mut flags = session
        .flags()
        .iter()
        .map(|flag| format!("{flag:?}"))
        .collect::<Vec<_>>();
    flags.sort();
    assert_eq!(json!(flags), case["flags"], "{id} decimal flags");
}

pub(super) fn order_input(value: &Value, session: &QueryNumericSession) -> SourceTripleOrderCell {
    let name = value["value"].as_str().unwrap();
    match value["kind"].as_str().unwrap() {
        "Variable" => SourceTripleOrderCell::Variable(Variable::new(name).unwrap()),
        "BNode" => SourceTripleOrderCell::BlankNode(BlankNode::new(name).unwrap()),
        "URIRef" => SourceTripleOrderCell::Rdf(iri(name)),
        "Literal" => SourceTripleOrderCell::Rdf(
            session
                .construct_source_direct_string(
                    name,
                    value["datatype"]
                        .as_str()
                        .map(|s| NamedNode::new(s).unwrap()),
                    value["language"].as_str().map(str::to_owned),
                    value["normalize"].as_bool().unwrap(),
                )
                .unwrap(),
        ),
        _ => panic!("unknown original direct triple constructor"),
    }
}
pub(super) fn order_term_json(
    cell: &SourceTripleOrderCell,
    session: &QueryNumericSession,
) -> Value {
    match cell {
        SourceTripleOrderCell::Variable(v) => {
            json!({"kind":"Variable", "value":v.as_str(), "hash":session.source_hash().unwrap().variable_hash(v.as_str()), "repr":format!("rdflib.term.Variable({})", semantica_json::python_repr(&json!(v.as_str())).unwrap())})
        }
        SourceTripleOrderCell::BlankNode(v) => {
            let raw = RawExpressionValue::from_import(v.clone().into());
            let mut data = term_json(&raw, session);
            data.as_object_mut()
                .unwrap()
                .insert("value".into(), json!(v.as_str()));
            data
        }
        SourceTripleOrderCell::Rdf(v) => {
            let data = term_json(v, session);
            json!({"kind":data["kind"], "value": match v.rdf().unwrap() { Term::Literal(v) => v.value(), Term::NamedNode(v) => v.as_str(), _ => panic!() }, "hash":data["hash"], "repr":data["repr"]})
        }
        SourceTripleOrderCell::Path(_) => {
            panic!("old ordering fixture does not grant native path admission")
        }
    }
}
fn receive_order(id: &str) {
    let root = fixture();
    let case = root["ordering"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    let session = session();
    let rows: Vec<[SourceTripleOrderCell; 3]> = case["input"]["triples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            assert_eq!(row.as_array().unwrap().len(), 3);
            [
                order_input(&row[0], &session),
                order_input(&row[1], &session),
                order_input(&row[2], &session),
            ]
        })
        .collect();
    let encode = |row: &[SourceTripleOrderCell; 3]| {
        row.iter()
            .map(|c| order_term_json(c, &session))
            .collect::<Vec<_>>()
    };
    let before: Vec<_> = rows.iter().map(encode).collect();
    assert_eq!(
        json!(before),
        case["constructed"],
        "{id} actual direct constructor"
    );
    let grouped;
    let ordered_rows = if case["input"]["entry"] == "triples_rows" {
        let inputs: Vec<_> = rows
            .iter()
            .map(|row| spargebra::SourceTripleRow::List(row.as_slice()))
            .collect();
        grouped = spargebra::source_group_triple_rows(&inputs).unwrap();
        &grouped
    } else {
        &rows
    };
    let indices = source_reorder_triples(&session, ordered_rows).unwrap();
    assert_eq!(
        json!(
            indices
                .iter()
                .map(|&at| encode(&ordered_rows[at]))
                .collect::<Vec<_>>()
        ),
        case["ordered"],
        "{id} actual source ordering"
    );
    assert_eq!(before, rows.iter().map(encode).collect::<Vec<_>>());
    assert!(
        case["container_types"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v == "builtins.tuple")
    );
    assert_eq!(
        session.flags(),
        semantica_numeric::DecimalSignals::default()
    );
}
fn receive_runtime_order(id: &str) {
    let root = fixture();
    let case = root["runtime_order"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    let mut variables = Vec::<String>::new();
    let mut slot = |name: &str| {
        if let Some(at) = variables.iter().position(|v| v == name) {
            at
        } else {
            variables.push(name.to_owned());
            variables.len() - 1
        }
    };
    let patterns: Vec<[source_bgp::SourceBgpTerm<RawExpressionValue>; 3]> =
        case["constructed_triple_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let mut term = |v: &Value| match v["kind"].as_str().unwrap() {
                    "Variable" => {
                        source_bgp::SourceBgpTerm::Binding(slot(v["value"].as_str().unwrap()))
                    }
                    "URIRef" => {
                        source_bgp::SourceBgpTerm::Constant(iri(v["value"].as_str().unwrap()))
                    }
                    _ => panic!("unreceived runtime ordering term"),
                };
                [term(&row[0]), term(&row[1]), term(&row[2])]
            })
            .collect();
    let initial: Vec<_> = case["input"]["initial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let at = slot(v["variable"].as_str().unwrap());
            let cell = match v["value"].as_str().unwrap() {
                "iri" => SolutionValue::Rdf(iri("urn:ex:value")),
                "none" => SolutionValue::ExplicitNone,
                "error" => SolutionValue::ReturnedError(not_bound()),
                _ => panic!(),
            };
            (at, cell)
        })
        .collect();
    let mut bindings = InternalTuple::with_capacity(variables.len());
    for (at, cell) in initial {
        bindings.set_cell(at, cell);
    }
    let order =
        source_bgp::source_bgp_runtime_order(&patterns, &bindings, &CancellationToken::new())
            .unwrap();
    assert_eq!(
        json!(order),
        case["entry_calls"][0]["ordered_input_rows"],
        "{id} native evalPart key-only order"
    );
    assert_eq!(case["entry_calls"][0]["same_context"], true);
}

#[test]
fn source_bgp_native_dp08_error_keys_reach_actual_dictionary_lookup() {
    let root = fixture();
    let case = &root["native_dataset_error_protocol"];
    assert_eq!(case["input"]["id"], "DP08");
    let session = session();
    let mut graph = SourceMemoryGraph::new(Rc::clone(&session), None).unwrap();
    let s = graph.admit_source(iri("urn:ex:s")).unwrap();
    let p = graph.admit_source(iri("urn:ex:p")).unwrap();
    let o = graph
        .admit_source(
            session
                .construct_source_direct_string("a", None, None, true)
                .unwrap(),
        )
        .unwrap();
    graph.insert([s.clone(), p.clone(), o.clone()]).unwrap();
    assert_eq!(json!([graph.len()]), case["insertion_sizes"]);
    let error = not_bound();
    assert_eq!(case["scans"].as_array().unwrap().len(), 11);
    for scan in case["scans"].as_array().unwrap() {
        let cell = |value: &Value| {
            if value.is_null() {
                return None;
            }
            Some(match value["kind"].as_str().unwrap() {
                "URIRef" => SolutionValue::Rdf(match value["value"].as_str().unwrap() {
                    "urn:ex:s" => s.clone(),
                    "urn:ex:p" => p.clone(),
                    _ => panic!(),
                }),
                "Literal" => SolutionValue::Rdf(o.clone()),
                "Exception" => {
                    assert_eq!(value["module"], "rdflib.plugins.sparql.sparql");
                    assert_eq!(value["class_name"], "NotBoundError");
                    assert_eq!(value["args"], json!([null]));
                    SolutionValue::ReturnedError(error.clone())
                }
                _ => panic!(),
            })
        };
        let pattern = [
            cell(&scan["pattern"][0]),
            cell(&scan["pattern"][1]),
            cell(&scan["pattern"][2]),
        ];
        let rows = graph
            .source_quads_for_cells(pattern)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut hashes = Vec::new();
        let got: Vec<_> = rows
            .into_iter()
            .map(|q| {
                let triple = [q.subject, q.predicate, q.object];
                hashes.push(graph.triple_hash(&triple).unwrap());
                triple
                    .into_iter()
                    .map(|t| {
                        let raw = graph.externalize_source(t).unwrap();
                        let mut value = term_json(&raw, &session);
                        if !matches!(raw.rdf().unwrap(), Term::Literal(_)) {
                            value.as_object_mut().unwrap().remove("repr");
                        }
                        value
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let want: Vec<_> = scan["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(without_token)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(got, want);
        assert_eq!(json!(hashes), scan["row_hashes"]);
    }
}

#[test]
fn source_bgp_real_graph_factory_rejects_an_equal_but_different_session() {
    let a = session();
    let b = session();
    let result = numeric_query::prepare_source_graph_numeric_query(
        "SELECT * WHERE { ?s ?p ?o }",
        dataset(&a, None),
        b,
        Vec::new(),
    );
    let Err(numeric_query::NumericPrepareError::Query(error)) = result else {
        panic!("foreign session admitted");
    };
    assert!(matches!(
        crate::numeric::raw_numeric_fault(&error),
        Some(RawNumericFault::InvalidSourceOrigin)
    ));
}
#[test]
fn source_bgp_real_source_graph_does_not_upgrade_old_frontend_admission() {
    let session = session();
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        "SELECT * WHERE { ?s ?p ?o }",
        dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    let error = prepared
        .execute_source_presence()
        .err()
        .expect("old admission remains graph-gated");
    assert!(matches!(
        crate::numeric::raw_numeric_fault(&error),
        Some(RawNumericFault::UnsupportedSourceLiteralOperator(_))
            | Some(RawNumericFault::InvalidSourceOrigin)
    ));
}
#[test]
fn source_bgp_real_graph_store_rejects_literals_without_construction_facts() {
    let session = session();
    let graph = dataset(&session, None);
    assert!(matches!(
        graph.admit_source(RawExpressionValue::from_import(
            Literal::new_simple_literal("a").into()
        )),
        Err(SourceGraphError::UnknownSourceFacts)
    ));
}
#[test]
fn source_bgp_literal_profile_alone_does_not_admit_an_actual_source_scan() {
    let session = session();
    let data = NumericDataset::new_with_literal_semantics(
        dataset(&session, None),
        QueryLiteralSemantics::SourceLiteralV1,
    );
    assert!(!data.has_source_graph_session(&session));
    assert!(matches!(
        crate::numeric::raw_numeric_fault(
            &data
                .source_triples_for_cells([None, None, None])
                .err()
                .unwrap()
        ),
        Some(RawNumericFault::InvalidSourceOrigin)
    ));
}
#[test]
fn source_bgp_actual_same_graph_token_cancels_before_query_start() {
    let session = session();
    let flag = Arc::new(AtomicBool::new(false));
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "SELECT * WHERE { ?s ?p ?o }",
        dataset(&session, Some(flag.clone())),
        session,
        Vec::new(),
    )
    .unwrap();
    flag.store(true, atomic::Ordering::Relaxed);
    assert!(matches!(
        prepared.execute_source_presence().err().unwrap(),
        QueryEvaluationError::Cancelled
    ));
}
#[test]
fn source_bgp_actual_same_graph_token_cancels_between_rows_and_fuses() {
    let session = session();
    let flag = Arc::new(AtomicBool::new(false));
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "SELECT * WHERE { ?s ?p ?o }",
        dataset(&session, Some(flag.clone())),
        session,
        Vec::new(),
    )
    .unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    assert!(rows.next().unwrap().is_ok());
    flag.store(true, atomic::Ordering::Relaxed);
    assert!(matches!(
        rows.next().unwrap(),
        Err(QueryEvaluationError::Cancelled)
    ));
    flag.store(false, atomic::Ordering::Relaxed);
    for _ in 0..3 {
        assert!(rows.next().is_none());
    }
}
#[test]
fn source_bgp_original_path_inputs_remain_unqualified() {
    let session = session();
    let result = numeric_query::prepare_source_graph_numeric_query(
        "PREFIX : <urn:ex:> SELECT * WHERE { ?s :p/:q ?o }",
        dataset(&session, None),
        session,
        Vec::new(),
    );
    assert!(matches!(
        result,
        Err(numeric_query::NumericPrepareError::LiteralConstruction(
            RawNumericFault::UnsupportedSourceLiteralOperator("source graph path operator")
        ))
    ));
}

#[test]
fn source_bgp_query_gt01() {
    receive_query("GT01");
}

#[test]
fn source_bgp_query_gt02() {
    receive_query("GT02");
}

#[test]
fn source_bgp_query_gt03() {
    receive_query("GT03");
}

#[test]
fn source_bgp_query_gt04() {
    receive_query("GT04");
}

#[test]
fn source_bgp_query_gt05() {
    receive_query("GT05");
}

#[test]
fn source_bgp_query_gt06() {
    receive_query("GT06");
}

#[test]
fn source_bgp_query_gt07() {
    receive_query("GT07");
}

#[test]
fn source_bgp_query_gt08() {
    receive_query("GT08");
}

#[test]
fn source_bgp_query_gt09() {
    receive_query("GT09");
}

#[test]
fn source_bgp_query_gt11() {
    receive_query("GT11");
}

#[test]
fn source_bgp_query_gt12() {
    receive_query("GT12");
}

#[test]
fn source_bgp_query_gt13() {
    receive_query("GT13");
}

#[test]
fn source_bgp_query_gt15() {
    receive_query("GT15");
}

#[test]
fn source_bgp_query_gt16() {
    receive_query("GT16");
}

#[test]
fn source_bgp_query_gt17() {
    receive_query("GT17");
}

#[test]
fn source_bgp_query_gt18() {
    receive_query("GT18");
}

#[test]
fn source_bgp_query_gt19() {
    receive_query("GT19");
}

#[test]
fn source_bgp_query_gt20() {
    receive_query("GT20");
}

#[test]
fn source_bgp_query_gt21() {
    receive_query("GT21");
}

#[test]
fn source_bgp_query_gt22() {
    receive_query("GT22");
}

#[test]
fn source_bgp_query_gt23() {
    receive_query("GT23");
}

#[test]
fn source_bgp_query_gt24() {
    receive_query("GT24");
}

#[test]
fn source_bgp_query_gb01() {
    receive_query("GB01");
}

#[test]
fn source_bgp_query_gb02() {
    receive_query("GB02");
}

#[test]
fn source_bgp_query_gb03() {
    receive_query("GB03");
}

#[test]
fn source_bgp_query_gb04() {
    receive_query("GB04");
}

#[test]
fn source_bgp_query_gb05() {
    receive_query("GB05");
}

#[test]
fn source_bgp_query_gb06() {
    receive_query("GB06");
}

#[test]
fn source_bgp_query_gb07() {
    receive_query("GB07");
}

#[test]
fn source_bgp_query_gb08() {
    receive_query("GB08");
}

#[test]
fn source_bgp_query_gb09() {
    receive_query("GB09");
}

#[test]
fn source_bgp_query_gb10() {
    receive_query("GB10");
}

#[test]
fn source_bgp_query_gb11() {
    receive_query("GB11");
}

#[test]
fn source_bgp_query_gb12() {
    receive_query("GB12");
}

#[test]
fn source_bgp_algebra_order_bo01() {
    receive_order("BO01");
}

#[test]
fn source_bgp_algebra_order_bo02() {
    receive_order("BO02");
}

#[test]
fn source_bgp_algebra_order_bo03() {
    receive_order("BO03");
}

#[test]
fn source_bgp_algebra_order_bo04() {
    receive_order("BO04");
}

#[test]
fn source_bgp_algebra_order_bo05() {
    receive_order("BO05");
}

#[test]
fn source_bgp_algebra_order_bo06() {
    receive_order("BO06");
}

#[test]
fn source_bgp_algebra_order_bo07() {
    receive_order("BO07");
}

#[test]
fn source_bgp_algebra_order_bo08() {
    receive_order("BO08");
}

#[test]
fn source_bgp_algebra_order_bo09() {
    receive_order("BO09");
}

#[test]
fn source_bgp_algebra_order_bo10() {
    receive_order("BO10");
}

#[test]
fn source_bgp_algebra_order_bo11() {
    receive_order("BO11");
}

#[test]
fn source_bgp_algebra_order_bo12() {
    receive_order("BO12");
}

#[test]
fn source_bgp_runtime_order_br01() {
    receive_runtime_order("BR01");
}

#[test]
fn source_bgp_runtime_order_br02() {
    receive_runtime_order("BR02");
}

#[test]
fn source_bgp_runtime_order_br03() {
    receive_runtime_order("BR03");
}

#[test]
fn source_bgp_runtime_order_br04() {
    receive_runtime_order("BR04");
}

#[test]
fn source_bgp_runtime_order_br05() {
    receive_runtime_order("BR05");
}

#[test]
fn source_bgp_runtime_order_br06() {
    receive_runtime_order("BR06");
}

#[test]
fn source_bgp_runtime_order_br07() {
    receive_runtime_order("BR07");
}

#[test]
fn source_bgp_runtime_order_br08() {
    receive_runtime_order("BR08");
}
