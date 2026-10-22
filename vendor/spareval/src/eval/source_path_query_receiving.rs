//! Actual parser/plan/graph/compiled-path/decoder receiving of frozen original queries.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
const FIXTURE: &str = include_str!("fixtures/source_path_query_cases.json");
const SCANS: &str = include_str!("fixtures/source_path_query_scan_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "688a360184822e6410432a3e12b17341595180dfcc42ab9fb99299f22568c03b"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(
        value["groups"]["base"]["cases"].as_array().unwrap().len(),
        26
    );
    assert_eq!(
        value["groups"]["cycles"]["cases"].as_array().unwrap().len(),
        16
    );
    value
}
fn receive(group: &str, id: &str) {
    let root = fixture();
    let group = &root["groups"][group];
    let case = group["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(SCANS.as_bytes())),
        "4ab5093fc23446754629426a209dd2f64ea3c9c275a1f18900b6690a70e81630"
    );
    let scans: Value = serde_json::from_str(SCANS).unwrap();
    assert_eq!(scans["candidate_results_used"], false);
    let scan = scans["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    assert_eq!(scan["input"], case["input"]);
    super::source_bgp_receiving::receive_path_query_case(case, &group["data_inputs"], scan);
}

/// Compare actual prepared predicates/reorder output, including instance-only
/// EXISTS children, before execution. Raw projection mappings grant no child.
pub(super) fn receive_prepared_metadata(
    prepared: &numeric_query::PreparedNumericQuery<
        '_,
        crate::dataset::source_memory::SourceMemoryGraph,
    >,
    case: &Value,
    session: &Rc<QueryNumericSession>,
) {
    use serde_json::json;
    use spargebra::algebra::SourceContextAnnotation as A;
    use spargebra::term::{NamedNodePattern, TermPattern};
    use spargebra::{SourcePathDefinition, SourceTriplePattern, SourceTriplePredicate as P};
    let registry = prepared.source_literal_registry().unwrap();
    assert!(registry.admits_path_execution(session));
    fn encode_term(
        v: &TermPattern,
        session: &Rc<QueryNumericSession>,
        registry: &crate::numeric::PreparedSourceLiteralRegistry,
    ) -> Value {
        match v {
            TermPattern::Variable(v) => {
                json!({"kind":"Variable","value":v.as_str(),"hash":session.source_hash().unwrap().variable_hash(v.as_str()),"repr":format!("rdflib.term.Variable({})",semantica_json::python_repr(&json!(v.as_str())).unwrap())})
            }
            TermPattern::NamedNode(v) => super::source_bgp_receiving::term_json(
                &RawExpressionValue::from_import(v.clone().into()),
                session,
            ),
            TermPattern::SourceLiteral(v) => {
                super::source_bgp_receiving::term_json(&registry.resolve(v).unwrap(), session)
            }
            _ => panic!("unreceived prepared endpoint kind"),
        }
    }
    fn encode_pattern(
        p: &SourceTriplePattern,
        session: &Rc<QueryNumericSession>,
        registry: &crate::numeric::PreparedSourceLiteralRegistry,
    ) -> Value {
        let predicate = match &p.predicate {
            P::Rdf(NamedNodePattern::NamedNode(v)) => {
                encode_term(&TermPattern::NamedNode(v.clone()), session, registry)
            }
            P::Rdf(NamedNodePattern::Variable(v)) => {
                encode_term(&TermPattern::Variable(v.clone()), session, registry)
            }
            P::Path { definition, .. } => {
                assert!(!matches!(definition, SourcePathDefinition::NamedNode(_)));
                let (class, repr) =
                    source_path::source_path_text(definition, session, registry, &mut || Ok(()))
                        .unwrap();
                json!({"path_class":class,"hash":session.source_hash().unwrap().variable_hash(&repr),"repr":repr})
            }
        };
        json!([
            encode_term(&p.subject, session, registry),
            predicate,
            encode_term(&p.object, session, registry)
        ])
    }
    fn walk(
        p: &GraphPattern,
        session: &Rc<QueryNumericSession>,
        registry: &crate::numeric::PreparedSourceLiteralRegistry,
        out: &mut Vec<Value>,
    ) {
        match p {
            GraphPattern::SourceContext { inner, annotation } => match annotation {
                A::PathBgp { patterns } => out.push(Value::Array(
                    patterns
                        .iter()
                        .map(|p| encode_pattern(p, session, registry))
                        .collect(),
                )),
                A::Bgp { patterns } => out.push(Value::Array(
                    patterns
                        .iter()
                        .map(|p| {
                            encode_pattern(
                                &SourceTriplePattern {
                                    subject: p.subject.clone(),
                                    predicate: P::Rdf(p.predicate.clone()),
                                    object: p.object.clone(),
                                },
                                session,
                                registry,
                            )
                        })
                        .collect(),
                )),
                _ => walk(inner, session, registry, out),
            },
            GraphPattern::Project { inner, .. } => walk(inner, session, registry, out),
            GraphPattern::Extend {
                inner, expression, ..
            } => {
                walk(inner, session, registry, out);
                if let Expression::SourceExists(e) = expression {
                    if let Some(g) = &e.graph {
                        walk(g, session, registry, out);
                    }
                }
            }
            GraphPattern::Join { left, right, .. } => {
                walk(left, session, registry, out);
                walk(right, session, registry, out);
            }
            GraphPattern::Values { .. } => (),
            _ => panic!("unreceived prepared pattern shape: {p:?}"),
        }
    }
    fn comparable(v: &Value) -> Value {
        if !v["path_class"].is_null() {
            return json!({"path_class":v["path_class"],"hash":v["hash"],"repr":v["repr"]});
        }
        match v {
            Value::Object(v) => Value::Object(
                v.iter()
                    .filter(|(k, _)| k.as_str() != "object_token")
                    .map(|(k, v)| (k.clone(), comparable(v)))
                    .collect(),
            ),
            Value::Array(v) => Value::Array(v.iter().map(comparable).collect()),
            v => v.clone(),
        }
    }
    let mut actual = Vec::new();
    walk(prepared.plan(), session, registry, &mut actual);
    let mut wanted = Vec::new();
    for name in ["translated_nodes", "exists_instance_nodes"] {
        if let Some(nodes) = case[name].as_array() {
            for n in nodes {
                if n["name"] == "BGP" {
                    let rows = &n["tree"]["fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|f| f[0] == "triples")
                        .unwrap()[1];
                    wanted.push(comparable(rows));
                }
            }
        }
    }
    assert_eq!(
        actual, wanted,
        "{} actual native predicates and source reorder output",
        case["input"]["id"]
    );
}

#[test]
fn source_path_full_query_gt14() {
    receive("base", "GT14");
}

#[test]
fn source_path_full_query_pq01() {
    receive("base", "PQ01");
}

#[test]
fn source_path_full_query_pq02() {
    receive("base", "PQ02");
}

#[test]
fn source_path_full_query_pq03() {
    receive("base", "PQ03");
}

#[test]
fn source_path_full_query_pq04() {
    receive("base", "PQ04");
}

#[test]
fn source_path_full_query_pq05() {
    receive("base", "PQ05");
}

#[test]
fn source_path_full_query_pq06() {
    receive("base", "PQ06");
}

#[test]
fn source_path_full_query_pq07() {
    receive("base", "PQ07");
}

#[test]
fn source_path_full_query_pq08() {
    receive("base", "PQ08");
}

#[test]
fn source_path_full_query_pq09() {
    receive("base", "PQ09");
}

#[test]
fn source_path_full_query_pq10() {
    receive("base", "PQ10");
}

#[test]
fn source_path_full_query_pq11() {
    receive("base", "PQ11");
}

#[test]
fn source_path_full_query_pq12() {
    receive("base", "PQ12");
}

#[test]
fn source_path_full_query_pq13() {
    receive("base", "PQ13");
}

#[test]
fn source_path_full_query_pq14() {
    receive("base", "PQ14");
}

#[test]
fn source_path_full_query_pq15() {
    receive("base", "PQ15");
}

#[test]
fn source_path_full_query_pq16() {
    receive("base", "PQ16");
}

#[test]
fn source_path_full_query_pq17() {
    receive("base", "PQ17");
}

#[test]
fn source_path_full_query_pq18() {
    receive("base", "PQ18");
}

#[test]
fn source_path_full_query_pq19() {
    receive("base", "PQ19");
}

#[test]
fn source_path_full_query_pq20() {
    receive("base", "PQ20");
}

#[test]
fn source_path_full_query_pq21() {
    receive("base", "PQ21");
}

#[test]
fn source_path_full_query_pq22() {
    receive("base", "PQ22");
}

#[test]
fn source_path_full_query_pq23() {
    receive("base", "PQ23");
}

#[test]
fn source_path_full_query_pq24() {
    receive("base", "PQ24");
}

#[test]
fn source_path_full_query_pq25() {
    receive("base", "PQ25");
}

#[test]
fn source_path_full_query_pc01() {
    receive("cycles", "PC01");
}

#[test]
fn source_path_full_query_pc02() {
    receive("cycles", "PC02");
}

#[test]
fn source_path_full_query_pc03() {
    receive("cycles", "PC03");
}

#[test]
fn source_path_full_query_pc04() {
    receive("cycles", "PC04");
}

#[test]
fn source_path_full_query_pc05() {
    receive("cycles", "PC05");
}

#[test]
fn source_path_full_query_pc06() {
    receive("cycles", "PC06");
}

#[test]
fn source_path_full_query_pc07() {
    receive("cycles", "PC07");
}

#[test]
fn source_path_full_query_pc08() {
    receive("cycles", "PC08");
}

#[test]
fn source_path_full_query_pc09() {
    receive("cycles", "PC09");
}

#[test]
fn source_path_full_query_pc10() {
    receive("cycles", "PC10");
}

#[test]
fn source_path_full_query_pc11() {
    receive("cycles", "PC11");
}

#[test]
fn source_path_full_query_pc12() {
    receive("cycles", "PC12");
}

#[test]
fn source_path_full_query_pc13() {
    receive("cycles", "PC13");
}

#[test]
fn source_path_full_query_pc14() {
    receive("cycles", "PC14");
}

#[test]
fn source_path_full_query_pc15() {
    receive("cycles", "PC15");
}

#[test]
fn source_path_full_query_pc16() {
    receive("cycles", "PC16");
}

#[test]
fn source_path_query_rejects_foreign_session_before_parse() {
    let a = super::source_bgp_receiving::session();
    let b = super::source_bgp_receiving::session();
    let got = numeric_query::prepare_source_path_numeric_query(
        "SELECT * WHERE {?s <urn:ex:q>* ?o}",
        super::source_bgp_receiving::dataset(&a, None),
        b,
        Vec::new(),
    );
    let Err(numeric_query::NumericPrepareError::Query(error)) = got else {
        panic!("foreign session admitted")
    };
    assert!(matches!(
        crate::numeric::raw_numeric_fault(&error),
        Some(RawNumericFault::InvalidSourceOrigin)
    ));
}
#[test]
fn source_path_query_shares_graph_cancellation_before_start() {
    let s = super::source_bgp_receiving::session();
    let flag = Arc::new(AtomicBool::new(false));
    let prepared = numeric_query::prepare_source_path_numeric_query(
        "SELECT * WHERE {?s <urn:ex:q>* ?o}",
        super::source_bgp_receiving::dataset(&s, Some(flag.clone())),
        s,
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
fn source_path_query_shares_cancellation_between_rows_and_fuses() {
    let s = super::source_bgp_receiving::session();
    let flag = Arc::new(AtomicBool::new(false));
    let prepared = numeric_query::prepare_source_path_numeric_query(
        "SELECT * WHERE {?s <urn:ex:q>* ?o}",
        super::source_bgp_receiving::dataset(&s, Some(flag.clone())),
        s,
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
fn source_path_query_does_not_upgrade_old_graph_path_gate() {
    let s = super::source_bgp_receiving::session();
    let got = numeric_query::prepare_source_graph_numeric_query(
        "SELECT * WHERE {?s <urn:ex:q>* ?o}",
        super::source_bgp_receiving::dataset(&s, None),
        s,
        Vec::new(),
    );
    assert!(matches!(
        got,
        Err(numeric_query::NumericPrepareError::LiteralConstruction(
            RawNumericFault::UnsupportedSourceLiteralOperator("source graph path operator")
        ))
    ));
}
#[test]
fn source_path_query_requires_new_private_registry_owner() {
    let s = super::source_bgp_receiving::session();
    let old = crate::numeric::PreparedSourceLiteralRegistry::new_for_graph_execution(s.clone());
    let native = crate::numeric::PreparedSourceLiteralRegistry::new_for_path_execution(s.clone());
    assert!(!old.admits_path_execution(&s));
    assert!(native.admits_path_execution(&s));
    assert!(!native.admits_path_execution(&super::source_bgp_receiving::session()));
}
#[test]
fn source_path_runtime_order_treats_native_predicate_as_known() {
    use super::source_bgp::{SourceBgpTerm as T, source_bgp_runtime_order};
    let patterns: Vec<[T<String>; 3]> = vec![
        [T::Binding(0), T::NativePath, T::Binding(1)],
        [T::Binding(0), T::Binding(2), T::Binding(1)],
    ];
    let cells = InternalTuple::with_capacity(3);
    let token = CancellationToken::new();
    assert_eq!(
        source_bgp_runtime_order(&patterns, &cells, &token).unwrap(),
        vec![0, 1]
    );
}
