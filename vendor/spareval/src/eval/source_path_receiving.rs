//! Native constructor/Graph.triples comparisons to pre-implementation source
//! observations. Full query integration and path syntax are separately pending.
use super::source_path::CompiledSourcePath;
use super::*;
use crate::dataset::nullable_numeric::{CachedInternalTerm, NumericDataset};
use crate::dataset::source_memory::{SourceGraphTerm, SourceMemoryGraph};
use crate::numeric::PreparedSourceLiteralRegistry;
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use oxrdf::NamedNode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spargebra::{
    SourcePathDefinition as P, SourcePathNegatedArgument as N, SourcePathRepeat as M,
    SourceSyntaxAttributes, SourceSyntaxNode, SourceSyntaxSnapshot,
};
use std::cell::RefCell;

const FIXTURE: &str = include_str!("fixtures/source_path_native_cases.json");
type D = NumericDataset<'static, SourceMemoryGraph>;
type T = CachedInternalTerm<SourceGraphTerm>;
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "0bd70b0af8f0ce2558b712affcfc9eedc7ca6e9b588caccfd11d6f06e9f7d8f3"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(
        root["source_record"]["sha256"],
        "26ff9d9a308901fd8280a25693761d462ad9578c2b66444ad188a19db513b916"
    );
    assert_eq!(root["cases"].as_array().unwrap().len(), 40);
    root
}
fn named(v: &Value) -> NamedNode {
    NamedNode::new(v["iri"].as_str().unwrap()).unwrap()
}
fn definition(v: &Value) -> P {
    match v["kind"].as_str().unwrap() {
        "named" => P::NamedNode(named(v)),
        "inverse" => P::Inverse(Box::new(definition(&v["arg"]))),
        "sequence" => P::Sequence(
            v["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(definition)
                .collect(),
        ),
        "alternative" => P::Alternative(
            v["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(definition)
                .collect(),
        ),
        "repeat" => P::Repeat {
            inner: Box::new(definition(&v["arg"])),
            mode: match v["mod"].as_str().unwrap() {
                "?" => M::ZeroOrOne,
                "*" => M::ZeroOrMore,
                "+" => M::OneOrMore,
                _ => panic!(),
            },
        },
        "negated" => P::Negated(
            v["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| match v["kind"].as_str().unwrap() {
                    "named" => N::NamedNode(named(v)),
                    "inverse" => N::Inverse(Box::new(definition(&v["arg"]))),
                    "untranslated" => {
                        assert!(v["fields"].as_array().unwrap().is_empty());
                        N::Untranslated(
                            SourceSyntaxSnapshot::from_ordered_nodes(
                                vec![SourceSyntaxNode {
                                    name: v["name"].as_str().unwrap().to_owned(),
                                    fields: vec![],
                                    attributes: SourceSyntaxAttributes::default(),
                                }],
                                0,
                            )
                            .unwrap(),
                        )
                    }
                    _ => panic!("unobserved negated constructor"),
                })
                .collect(),
        ),
        _ => panic!("unobserved constructor"),
    }
}
fn iri(v: &str) -> RawExpressionValue {
    RawExpressionValue::from_import(NamedNode::new(v).unwrap().into()).ensure_source_object()
}
fn dataset(
    session: &Rc<QueryNumericSession>,
    token: Option<Arc<AtomicBool>>,
    root: &Value,
    cycles: bool,
) -> D {
    let mut graph = SourceMemoryGraph::new(session.clone(), token).unwrap();
    let facts = root["base_data"].as_array().unwrap().iter().chain(
        root["extra_data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|_| cycles),
    );
    for input in facts {
        let object = if let Some(v) = input["object_iri"].as_str() {
            iri(v)
        } else {
            session
                .construct_source_direct_string(
                    input["lexical"].as_str().unwrap(),
                    input["datatype"]
                        .as_str()
                        .map(|v| NamedNode::new(v).unwrap()),
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
    assert_eq!(graph.len(), if cycles { 14 } else { 6 });
    NumericDataset::new_with_source_dataset(graph, session.clone()).unwrap()
}
fn endpoint(v: &Value, session: &QueryNumericSession, dataset: &D) -> Option<SolutionValue<T>> {
    if v.is_null() {
        return None;
    }
    Some(match v["kind"].as_str().unwrap() {
        "named" => SolutionValue::Rdf(
            dataset
                .internalize_raw(iri(v["iri"].as_str().unwrap()))
                .unwrap(),
        ),
        "literal" => SolutionValue::Rdf(
            dataset
                .internalize_raw(
                    session
                        .construct_source_direct_string(
                            v["lexical"].as_str().unwrap(),
                            v["datatype"].as_str().map(|v| NamedNode::new(v).unwrap()),
                            None,
                            v["normalize"].as_bool().unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap(),
        ),
        "error" => SolutionValue::ReturnedError(ErrorValue::with_source(
            SparqlErrorKind::NotBound,
            "None",
            SourceException {
                class: SourceExceptionClass::NotBound,
                argument: SourceExceptionArgument::None,
            },
        )),
        _ => panic!(),
    })
}
fn without_token(value: &Value) -> Value {
    let mut out = value.clone();
    if let Some(map) = out.as_object_mut() {
        map.remove("object_token");
    }
    out
}
fn cell_json(
    cell: &SolutionValue<T>,
    dataset: &D,
    session: &QueryNumericSession,
    wanted: &Value,
    objects: &mut Vec<(RawExpressionValue, String)>,
) -> Value {
    match cell {
        SolutionValue::Rdf(value) => {
            let raw = dataset.externalize_raw(value.clone()).unwrap();
            objects.push((
                raw.clone(),
                wanted["object_token"].as_str().unwrap().to_owned(),
            ));
            source_bgp_receiving::term_json(&raw, session)
        }
        SolutionValue::ExplicitNone => Value::Null,
        SolutionValue::ReturnedError(value) => {
            assert_eq!(
                value.source_exception(),
                Some(&SourceException {
                    class: SourceExceptionClass::NotBound,
                    argument: SourceExceptionArgument::None
                })
            );
            json!({"kind":"Exception","module":"rdflib.plugins.sparql.sparql","class_name":"NotBoundError","args":[null]})
        }
    }
}
fn receive(id: &str) {
    let root = fixture();
    let case = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    let session = source_bgp_receiving::session();
    let dataset = dataset(&session, None, &root, case["input"]["dataset"] == "cycles");
    let registry = PreparedSourceLiteralRegistry::new_for_graph_execution(session.clone());
    let def = definition(&case["input"]["path"]);
    let compiled = dataset.compile_source_path(&def, registry).unwrap();
    let scan_fixture: Value =
        serde_json::from_str(include_str!("fixtures/source_path_native_scan_cases.json")).unwrap();
    assert_eq!(
        scan_fixture["source_record_sha256"],
        "c6e37f6cad2381c07a2d8c83c5352a8adee7618c2fc5e4b2b22f0b50b07e1361"
    );
    let scan_case = scan_fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    assert_eq!(scan_case["input"], case["input"]);
    let starts = Rc::new(RefCell::new(Vec::<Value>::new()));
    let scan_starts = starts.clone();
    let actual = compiled.runtime().clone();
    let scan_runtime = actual.clone();
    let observed = Rc::new(source_path::SourcePathRuntime {
        read: actual.read.clone(),
        write: actual.write.clone(),
        session: actual.session.clone(),
        registry: actual.registry.clone(),
        token: actual.token.clone(),
        scan: Rc::new(move |cells| {
            let mut encoded = Vec::new();
            for cell in &cells {
                encoded.push(match cell {
                    None|Some(SolutionValue::ExplicitNone)=>Value::Null,
                    Some(SolutionValue::Rdf(value))=>source_bgp_receiving::term_json(&(scan_runtime.read)(value.clone())?,&scan_runtime.session),
                    Some(SolutionValue::ReturnedError(_))=>json!({"kind":"Exception","module":"rdflib.plugins.sparql.sparql","class_name":"NotBoundError","args":[null]}),
                });
            }
            scan_starts.borrow_mut().push(json!(encoded));
            (scan_runtime.scan)(cells)
        }),
    });
    let compiled = source_path::compile_source_path(observed, &def).unwrap();
    let expected_class = case["path"]["path_class"]
        .as_str()
        .unwrap_or_else(|| case["path"]["kind"].as_str().unwrap());
    assert_eq!(compiled.source_class, expected_class, "{id} native class");
    assert_eq!(
        compiled.source_repr,
        case["path"]["repr"].as_str().unwrap(),
        "{id} native repr"
    );
    assert_eq!(
        compiled.source_hash,
        case["path"]["hash"].as_i64().unwrap(),
        "{id} native hash"
    );
    let cells = [
        endpoint(&case["input"]["subject"], &session, &dataset),
        endpoint(&case["input"]["object"], &session, &dataset),
    ];
    let mut objects = Vec::new();
    for (cell, wanted) in cells.iter().zip([&case["subject"], &case["object"]]) {
        if let Some(cell) = cell {
            assert_eq!(
                cell_json(cell, &dataset, &session, wanted, &mut objects),
                without_token(wanted),
                "{id} constructor"
            );
        }
    }
    assert_eq!(case["iterator_created"], true);
    let mut rows = compiled.start(cells.clone());
    assert!(
        starts.borrow().is_empty(),
        "{id} no scans at iterator creation"
    );
    for (at, wanted) in case["rows"].as_array().unwrap().iter().enumerate() {
        let pair = rows
            .next()
            .unwrap_or_else(|| panic!("{id} missing row {at}"))
            .unwrap_or_else(|e| panic!("{id} row {at}: {e:?}"));
        assert_eq!(
            starts.borrow().len() as u64,
            scan_case["scan_starts_after_poll"][at].as_u64().unwrap(),
            "{id} scan starts before row {at}"
        );
        for (cell, wanted) in pair.iter().zip([&wanted[0], &wanted[2]]) {
            assert_eq!(
                cell_json(cell, &dataset, &session, wanted, &mut objects),
                without_token(wanted),
                "{id} row {at}"
            );
        }
        assert_eq!(
            wanted[1]["object_token"], case["path"]["object_token"],
            "original predicate keeps one path object"
        );
        for cell in pair
            .iter()
            .filter(|c| matches!(c, SolutionValue::ReturnedError(_)))
        {
            assert!(
                cells.iter().flatten().any(|v| v == cell),
                "{id} actual error object clone"
            );
        }
    }
    if case["status"] == "original_error" {
        assert_eq!(case["error_stage"], "first_poll");
        let error = rows
            .next()
            .expect("original exception item")
            .err()
            .expect("exception became row");
        assert_eq!(
            starts.borrow().len() as u64,
            scan_case["scan_starts_after_error"].as_u64().unwrap(),
            "{id} scan starts before error"
        );
        let QueryEvaluationError::Unexpected(error) = error else {
            panic!("{id} wrong error domain");
        };
        let error = error
            .downcast_ref::<source_compvalue::SourceCompValueError>()
            .unwrap();
        assert_eq!(
            json!({"kind":"Exception","module":error.module,"class_name":error.class,"args":[error.argument]}),
            case["error"]
        );
    } else {
        assert!(rows.next().is_none(), "{id} extra row");
        assert_eq!(case["terminal_again"], true);
        assert_eq!(
            starts.borrow().len() as u64,
            scan_case["scan_starts_after_poll"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_u64()
                .unwrap(),
            "{id} scan starts at EOF"
        );
    }
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    let wanted: Vec<Value> = scan_case["scan_starts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|triple| {
            json!(
                triple
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(without_token)
                    .collect::<Vec<_>>()
            )
        })
        .collect();
    assert_eq!(
        *starts.borrow(),
        wanted,
        "{id} actual scan operation order/selectors"
    );
    for (a, ta) in &objects {
        for (b, tb) in &objects {
            assert_eq!(
                a.same_source_object(b),
                ta == tb,
                "{id} actual source aliasing"
            );
        }
    }
    assert_eq!(case["flags"], json!([]));
    assert_eq!(
        session.flags(),
        semantica_numeric::DecimalSignals::default()
    );
}

fn simple<'a>(
    dataset: &NumericDataset<'a, SourceMemoryGraph>,
    session: &Rc<QueryNumericSession>,
) -> CompiledSourcePath<'a, T> {
    dataset
        .compile_source_path(
            &P::Repeat {
                inner: Box::new(P::NamedNode(NamedNode::new("urn:ex:q").unwrap())),
                mode: M::ZeroOrMore,
            },
            PreparedSourceLiteralRegistry::new_for_graph_execution(session.clone()),
        )
        .unwrap()
}
#[test]
fn source_native_path_foreign_session_and_rdf_only_dataset_are_rejected() {
    let session = source_bgp_receiving::session();
    let root = fixture();
    let data = dataset(&session, None, &root, false);
    let def = P::NamedNode(NamedNode::new("urn:ex:q").unwrap());
    assert!(
        data.compile_source_path(
            &def,
            PreparedSourceLiteralRegistry::new_for_graph_execution(source_bgp_receiving::session())
        )
        .is_err()
    );
    let bare = NumericDataset::new(source_bgp_receiving::dataset(&session, None));
    assert!(
        bare.compile_source_path(
            &def,
            PreparedSourceLiteralRegistry::new_for_graph_execution(session)
        )
        .is_err()
    );
}
#[test]
fn source_native_path_unadmitted_registry_is_rejected() {
    let session = source_bgp_receiving::session();
    let data = dataset(&session, None, &fixture(), false);
    let def = P::NamedNode(NamedNode::new("urn:ex:q").unwrap());
    assert!(
        data.compile_source_path(&def, PreparedSourceLiteralRegistry::new(session.clone()))
            .is_err()
    );
    assert!(
        data.compile_source_path(
            &def,
            PreparedSourceLiteralRegistry::new_for_exists_execution(session)
        )
        .is_err()
    );
}
#[test]
fn source_native_path_foreign_graph_endpoint_fails_on_first_poll_then_fuses() {
    let session = source_bgp_receiving::session();
    let root = fixture();
    let data = dataset(&session, None, &root, false);
    let foreign = dataset(&session, None, &root, false);
    let endpoint = foreign.internalize_raw(iri("urn:ex:a")).unwrap();
    let compiled = simple(&data, &session);
    let mut rows = compiled.start([Some(SolutionValue::Rdf(endpoint)), None]);
    assert!(rows.next().unwrap().is_err());
    assert!(rows.next().is_none());
}
#[test]
fn source_native_path_cancellation_before_first_poll_and_after_row_fuses() {
    let session = source_bgp_receiving::session();
    let token = Arc::new(AtomicBool::new(false));
    let data = dataset(&session, Some(token.clone()), &fixture(), false);
    let compiled = simple(&data, &session);
    let mut rows = compiled.start([None, None]);
    assert!(rows.next().unwrap().is_ok());
    token.store(true, atomic::Ordering::Relaxed);
    assert!(matches!(
        rows.next(),
        Some(Err(QueryEvaluationError::Cancelled))
    ));
    assert!(rows.next().is_none());
    let mut rows = compiled.start([None, None]);
    assert!(matches!(
        rows.next(),
        Some(Err(QueryEvaluationError::Cancelled))
    ));
    assert!(rows.next().is_none());
}
#[test]
fn source_native_path_compile_cancellation_and_depth_guard_keep_graph_unchanged() {
    let session = source_bgp_receiving::session();
    let token = Arc::new(AtomicBool::new(false));
    let data = dataset(&session, Some(token.clone()), &fixture(), false);
    let mut def = P::NamedNode(NamedNode::new("urn:ex:q").unwrap());
    for _ in 0..256 {
        def = P::Inverse(Box::new(def));
    }
    assert!(
        data.compile_source_path(
            &def,
            PreparedSourceLiteralRegistry::new_for_graph_execution(session.clone())
        )
        .is_err()
    );
    let compiled = simple(&data, &session);
    assert_eq!(compiled.start([None, None]).count(), 9);
    token.store(true, atomic::Ordering::Relaxed);
    let def = P::NamedNode(NamedNode::new("urn:ex:q").unwrap());
    assert!(matches!(
        data.compile_source_path(
            &def,
            PreparedSourceLiteralRegistry::new_for_graph_execution(session)
        ),
        Err(QueryEvaluationError::Cancelled)
    ));
}

#[test]
fn source_native_path_pn01() {
    receive("PN01");
}

#[test]
fn source_native_path_pn02() {
    receive("PN02");
}

#[test]
fn source_native_path_pn03() {
    receive("PN03");
}

#[test]
fn source_native_path_pn04() {
    receive("PN04");
}

#[test]
fn source_native_path_pn05() {
    receive("PN05");
}

#[test]
fn source_native_path_pn06() {
    receive("PN06");
}

#[test]
fn source_native_path_pn07() {
    receive("PN07");
}

#[test]
fn source_native_path_pn08() {
    receive("PN08");
}

#[test]
fn source_native_path_pn09() {
    receive("PN09");
}

#[test]
fn source_native_path_pn10() {
    receive("PN10");
}

#[test]
fn source_native_path_pn11() {
    receive("PN11");
}

#[test]
fn source_native_path_pn12() {
    receive("PN12");
}

#[test]
fn source_native_path_pn13() {
    receive("PN13");
}

#[test]
fn source_native_path_pn14() {
    receive("PN14");
}

#[test]
fn source_native_path_pn15() {
    receive("PN15");
}

#[test]
fn source_native_path_pn16() {
    receive("PN16");
}

#[test]
fn source_native_path_pn17() {
    receive("PN17");
}

#[test]
fn source_native_path_pn18() {
    receive("PN18");
}

#[test]
fn source_native_path_pn19() {
    receive("PN19");
}

#[test]
fn source_native_path_pn20() {
    receive("PN20");
}

#[test]
fn source_native_path_pn21() {
    receive("PN21");
}

#[test]
fn source_native_path_pn22() {
    receive("PN22");
}

#[test]
fn source_native_path_pn23() {
    receive("PN23");
}

#[test]
fn source_native_path_pn24() {
    receive("PN24");
}

#[test]
fn source_native_path_pn25() {
    receive("PN25");
}

#[test]
fn source_native_path_pn26() {
    receive("PN26");
}

#[test]
fn source_native_path_pn27() {
    receive("PN27");
}

#[test]
fn source_native_path_pn28() {
    receive("PN28");
}

#[test]
fn source_native_path_pn29() {
    receive("PN29");
}

#[test]
fn source_native_path_pn30() {
    receive("PN30");
}

#[test]
fn source_native_path_pn31() {
    receive("PN31");
}

#[test]
fn source_native_path_pn32() {
    receive("PN32");
}

#[test]
fn source_native_path_pn33() {
    receive("PN33");
}

#[test]
fn source_native_path_pn34() {
    receive("PN34");
}

#[test]
fn source_native_path_pn35() {
    receive("PN35");
}

#[test]
fn source_native_path_pn36() {
    receive("PN36");
}

#[test]
fn source_native_path_pn37() {
    receive("PN37");
}

#[test]
fn source_native_path_pn38() {
    receive("PN38");
}

#[test]
fn source_native_path_pn39() {
    receive("PN39");
}

#[test]
fn source_native_path_pn40() {
    receive("PN40");
}

fn receive_boolean_string(id: &str) {
    let fixture = include_str!("fixtures/source_boolean_string_cases.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        "622192f2c753f7ea4a62276a698088d2262300676b718650a2b67112c25d3ec2"
    );
    let root: Value = serde_json::from_str(fixture).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "324814f534ba09dfdbc0886524ae3b79eef622958c2742f3fc7370135ae906fc"
    );
    let case = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    let session = source_bgp_receiving::session();
    let raw = session
        .construct_source_direct_string(
            case["input"]["lexical"].as_str().unwrap(),
            Some(oxrdf::vocab::xsd::BOOLEAN.into_owned()),
            None,
            case["input"]["normalize"].as_bool().unwrap(),
        )
        .unwrap();
    let mut wanted = case.clone();
    wanted.as_object_mut().unwrap().remove("input");
    wanted.as_object_mut().unwrap().remove("truth");
    wanted.as_object_mut().unwrap().remove("flags");
    wanted
        .as_object_mut()
        .unwrap()
        .insert("kind".into(), json!("Literal"));
    assert_eq!(source_bgp_receiving::term_json(&raw, &session), wanted);
    assert_eq!(
        session.source_literal_python_truth(&raw).unwrap(),
        case["truth"]
    );
    let data = dataset(&session, None, &fixture_json(), false);
    let internal = data.internalize_raw(raw.clone()).unwrap();
    let returned = data.externalize_raw(internal).unwrap();
    assert!(raw.same_source_object(&returned));
    assert_eq!(source_bgp_receiving::term_json(&returned, &session), wanted);
    assert_eq!(
        session.flags(),
        semantica_numeric::DecimalSignals::default()
    );
}
fn fixture_json() -> Value {
    fixture()
}
#[test]
fn source_native_path_boolean_string_seal_rejects_metadata_reclassification_and_detached_parts() {
    use crate::numeric::{LiteralSourceState, SourceFact};
    let session = source_bgp_receiving::session();
    let raw = session
        .construct_source_direct_string(
            "false",
            Some(oxrdf::vocab::xsd::BOOLEAN.into_owned()),
            None,
            false,
        )
        .unwrap();
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!()
    };
    let mut changed = metadata.clone();
    changed.ill_typed = SourceFact::Known(None);
    assert!(raw.clone().with_source_metadata(changed).is_err());
    let (rdf, numeric, source) = raw.into_source_parts().unwrap();
    assert!(RawExpressionValue::from_source_parts(rdf, numeric, source).is_err());
}
#[test]
fn source_native_path_boolean_string_unreceived_casts_and_legacy_profile_stay_gated() {
    let session = source_bgp_receiving::session();
    for lexical in ["FALSE", "True", " true ", "invalid", ""] {
        assert!(
            session
                .construct_source_direct_string(
                    lexical,
                    Some(oxrdf::vocab::xsd::BOOLEAN.into_owned()),
                    None,
                    false
                )
                .is_err()
        );
    }
    let legacy = QueryNumericSession::new(
        semantica_numeric::DecimalContext::default(),
        semantica_numeric::NumericLimits::default(),
        None,
        true,
    )
    .unwrap();
    assert!(
        legacy
            .construct_source_direct_string(
                "false",
                Some(oxrdf::vocab::xsd::BOOLEAN.into_owned()),
                None,
                false
            )
            .is_err()
    );
}

#[test]
fn source_native_path_boolean_string_bc01() {
    receive_boolean_string("BC01");
}

#[test]
fn source_native_path_boolean_string_bc02() {
    receive_boolean_string("BC02");
}

#[test]
fn source_native_path_boolean_string_bc03() {
    receive_boolean_string("BC03");
}

#[test]
fn source_native_path_boolean_string_bc04() {
    receive_boolean_string("BC04");
}

#[test]
fn source_native_path_boolean_string_bc05() {
    receive_boolean_string("BC05");
}

#[test]
fn source_native_path_boolean_string_bc06() {
    receive_boolean_string("BC06");
}

#[test]
fn source_native_path_boolean_string_bc07() {
    receive_boolean_string("BC07");
}

#[test]
fn source_native_path_boolean_string_bc08() {
    receive_boolean_string("BC08");
}
