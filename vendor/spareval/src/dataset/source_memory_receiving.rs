//! Receives actual original Graph/Memory observations frozen before this
//! backend existed. Root authored both probes and receivers; no separate
//! independent review or complete source BGP qualification is claimed.
use super::QueryableDataset;
use super::nullable_numeric::NumericDataset;
use super::source_memory::{SourceGraphError, SourceGraphTerm, SourceMemoryGraph};
use crate::numeric::{
    LiteralSourceState, NumericCacheState, QueryContextSemantics, QueryGroupSemantics,
    QueryLiteralSemantics, QueryNumericCapabilities, QueryNumericSession, RawExpressionValue,
    SourceDatatype, SourceFact, SourceHashPolicy, SourceHashRuntime, SourceLiteralCacheKind,
};
use crate::tuple::SolutionValue;
use oxrdf::{Literal, NamedNode, Term};
use semantica_numeric::{DecimalContext, NumericLimits, NumericValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const FIXTURE: &str = include_str!("fixtures/source_memory_graph_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "f7e1ce1ecfdc811baee7617cc1b246c157c6d7a87c3495a7f99dd30f157947c0"
    );
    let data: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        data["source_record_sha256"],
        "dc02204249d4ebbea592e66b5df30dc9b0ddf8a52bad47df9fab16230b21bba2"
    );
    assert_eq!(data["candidate_results_used"], false);
    data
}
fn session() -> Rc<QueryNumericSession> {
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            DecimalContext::default(),
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
fn string(session: &QueryNumericSession, text: &str) -> RawExpressionValue {
    session
        .construct_source_direct_string(text, None, None, true)
        .unwrap()
}
fn iri(graph: &SourceMemoryGraph, name: &str) -> SourceGraphTerm {
    graph
        .internalize_term(NamedNode::new(name).unwrap().into())
        .unwrap()
}
#[derive(Default)]
struct Encoder {
    objects: Vec<RawExpressionValue>,
}
impl Encoder {
    fn encode(&mut self, raw: &RawExpressionValue, session: &QueryNumericSession) -> Value {
        let at = self
            .objects
            .iter()
            .position(|value| value.same_source_object(raw))
            .unwrap_or_else(|| {
                self.objects.push(raw.clone());
                self.objects.len() - 1
            });
        let token = format!("object_{at}");
        let hash = session.source_hash().unwrap().term_hash(raw).unwrap();
        match raw.rdf().unwrap() {
            Term::NamedNode(node) => json!({"kind":"URIRef", "object_token":token,
                "value":node.as_str(), "hash":hash}),
            Term::Literal(literal) => {
                let LiteralSourceState::Literal(meta) = raw.literal_source() else {
                    panic!()
                };
                let datatype = match &meta.datatype {
                    SourceDatatype::Absent => Value::Null,
                    SourceDatatype::Specified(node) => json!(node.as_str()),
                    SourceDatatype::Unknown => panic!("unknown source datatype"),
                };
                let SourceFact::Known(language) = &meta.language_original else {
                    panic!()
                };
                let SourceFact::Known(ill_typed) = meta.ill_typed else {
                    panic!()
                };
                let (cache_type, cache) = match (&meta.cached_value_kind, raw.numeric()) {
                    (
                        SourceFact::Known(SourceLiteralCacheKind::String),
                        NumericCacheState::NotNumeric,
                    ) => ("builtins.str", json!(literal.value())),
                    (
                        SourceFact::Known(SourceLiteralCacheKind::None),
                        NumericCacheState::CastFailed,
                    ) => ("builtins.NoneType", Value::Null),
                    (
                        SourceFact::Known(SourceLiteralCacheKind::Numeric),
                        NumericCacheState::Known(value),
                    ) => {
                        let NumericValue::Integer(value) = value.value() else {
                            panic!("unreceived cache family")
                        };
                        (
                            "builtins.int",
                            serde_json::from_str(&value.to_string()).unwrap(),
                        )
                    }
                    _ => panic!("unreceived cache family"),
                };
                json!({"kind":"Literal", "object_token":token, "lexical":literal.value(),
                    "datatype":datatype, "language":language, "ill_typed":ill_typed,
                    "cache":cache, "cache_type":cache_type, "hash":hash,
                    "repr":session.source_repr(&SolutionValue::Rdf(raw.clone())).unwrap()})
            }
            _ => panic!("unreceived term family"),
        }
    }
}
fn receive(id: &str) {
    let root = fixture();
    let case = root["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["input"]["id"] == id)
        .unwrap();
    assert_eq!(case["status"], "observed_complete");
    let session = session();
    let mut graph = SourceMemoryGraph::new(session.clone(), None).unwrap();
    let mut encoder = Encoder::default();
    let terms: Vec<_> = case["input"]["terms"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(at, value)| {
            let raw = session
                .construct_source_direct_string(
                    value["lexical"].as_str().unwrap(),
                    value["datatype"]
                        .as_str()
                        .map(|s| NamedNode::new(s).unwrap()),
                    value["language"].as_str().map(str::to_owned),
                    value["normalize"].as_bool().unwrap(),
                )
                .unwrap();
            assert_eq!(
                encoder.encode(&raw, &session),
                case["terms"][at],
                "{id} constructor {at}"
            );
            graph.admit_source(raw).unwrap()
        })
        .collect();
    let nodes: Vec<_> = [
        "urn:ex:s",
        "urn:ex:s1",
        "urn:ex:s2",
        "urn:ex:p",
        "urn:ex:p1",
    ]
    .into_iter()
    .map(|s| iri(&graph, s))
    .collect();
    for (at, index) in case["input"]["insert"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let (s, p, o) = if case["input"]["edges"].is_array() {
            let edge = &case["input"]["edges"][at];
            (
                edge[0].as_u64().unwrap() as usize,
                edge[1].as_u64().unwrap() as usize,
                edge[2].as_u64().unwrap() as usize,
            )
        } else {
            (0, 0, index.as_u64().unwrap() as usize)
        };
        graph
            .insert([nodes[s].clone(), nodes[3 + p].clone(), terms[o].clone()])
            .unwrap();
        assert_eq!(
            json!(graph.len()),
            case["insertion_sizes"][at],
            "{id} insert {at}"
        );
    }
    for eq in case["equalities"].as_array().unwrap() {
        assert_eq!(
            terms[eq["left"].as_u64().unwrap() as usize]
                == terms[eq["right"].as_u64().unwrap() as usize],
            eq["equal"].as_bool().unwrap(),
            "{id} source key equality"
        );
    }
    for (at, scan) in case["scans"].as_array().unwrap().iter().enumerate() {
        let pattern: Vec<_> = scan["pattern"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                if value.is_null() {
                    None
                } else if value["kind"] == "Literal" {
                    let index: usize = value["object_token"]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("object_")
                        .parse()
                        .unwrap();
                    Some(terms[index].clone())
                } else {
                    Some(
                        nodes
                            .iter()
                            .find(|node| {
                                graph.externalize_term((*node).clone()).unwrap().to_string()
                                    == format!("<{}>", value["value"].as_str().unwrap())
                            })
                            .unwrap()
                            .clone(),
                    )
                }
            })
            .collect();
        let rows = graph.internal_quads_for_pattern(
            pattern[0].as_ref(),
            pattern[1].as_ref(),
            pattern[2].as_ref(),
            Some(None),
        );
        let mut found = Vec::new();
        let mut hashes = Vec::new();
        for row in rows {
            let row = row.unwrap();
            let triple = [row.subject, row.predicate, row.object];
            hashes.push(graph.triple_hash(&triple).unwrap());
            found
                .push(triple.map(|term| {
                    encoder.encode(&graph.externalize_source(term).unwrap(), &session)
                }));
        }
        let actual_pattern: Vec<_> = pattern
            .into_iter()
            .map(|term| {
                term.map_or(Value::Null, |term| {
                    encoder.encode(&graph.externalize_source(term).unwrap(), &session)
                })
            })
            .collect();
        assert_eq!(
            json!(actual_pattern),
            scan["pattern"],
            "{id} pattern objects {at}"
        );
        assert_eq!(json!(found), scan["rows"], "{id} rows {at}");
        assert_eq!(json!(hashes), scan["row_hashes"], "{id} tuple hash {at}");
    }
}
macro_rules! receive_case {
    ($name:ident, $id:literal) => {
        #[test]
        fn $name() {
            receive($id);
        }
    };
}
receive_case!(source_memory_dp01_same_object_repeated, "DP01");
receive_case!(source_memory_dp02_plain_and_typed_string, "DP02");
receive_case!(source_memory_dp03_equal_distinct_objects, "DP03");
receive_case!(source_memory_dp04_original_language_case, "DP04");
receive_case!(source_memory_dp05_normalization_disabled, "DP05");
receive_case!(source_memory_dp06_normalization_enabled, "DP06");
receive_case!(source_memory_dp07_ill_typed_cache_none, "DP07");
receive_case!(
    source_memory_dp09_three_index_object_representatives,
    "DP09"
);
receive_case!(source_memory_dp10_native_set_copy_growth, "DP10");
receive_case!(source_memory_dp11_native_unicode_hashes, "DP11");
receive_case!(source_memory_dp12_empty_graph, "DP12");

#[test]
fn source_memory_same_dataset_raw_scan_retains_exact_value_object() {
    let session = session();
    let mut graph = SourceMemoryGraph::new(session.clone(), None).unwrap();
    let raw = string(&session, "a");
    let value = graph.admit_source(raw.clone()).unwrap();
    graph
        .insert([iri(&graph, "urn:ex:s"), iri(&graph, "urn:ex:p"), value])
        .unwrap();
    let dataset = NumericDataset::new_with_source_dataset(graph, session.clone()).unwrap();
    let scanned = dataset
        .internal_quads_for_pattern(None, None, None, Some(None))
        .next()
        .unwrap()
        .unwrap()
        .object;
    let output = dataset.externalize_raw(scanned.clone()).unwrap();
    assert!(output.same_source_object(&raw));
    let encoded = dataset.internalize_raw(output).unwrap();
    assert!(encoded.same_transport_instance(&scanned));
    assert!(
        dataset
            .clone()
            .externalize_raw(encoded)
            .unwrap()
            .same_source_object(&raw)
    );
}
#[test]
fn source_memory_equal_rdf_values_keep_distinct_source_caches_and_tokens() {
    let session = session();
    let mut graph = SourceMemoryGraph::new(session.clone(), None).unwrap();
    let plain = string(&session, "a");
    let typed = session
        .construct_source_direct_string(
            "a",
            Some(NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap()),
            None,
            true,
        )
        .unwrap();
    assert_eq!(plain.rdf().unwrap(), typed.rdf().unwrap());
    for raw in [&plain, &typed] {
        let value = graph.admit_source(raw.clone()).unwrap();
        graph
            .insert([iri(&graph, "urn:ex:s"), iri(&graph, "urn:ex:p"), value])
            .unwrap();
    }
    let dataset = NumericDataset::new_with_source_dataset(graph, session.clone()).unwrap();
    let p = dataset.internalize_raw(plain.clone()).unwrap();
    let t = dataset.internalize_raw(typed.clone()).unwrap();
    assert_ne!(p, t);
    for term in [p, t] {
        let row = dataset
            .internal_quads_for_pattern(None, None, Some(&term), Some(None))
            .next()
            .unwrap()
            .unwrap();
        assert!(
            dataset
                .externalize_raw(row.object)
                .unwrap()
                .same_source_object(term.source_raw().unwrap())
        );
    }
}
#[test]
fn source_memory_foreign_graph_owner_refused_on_scan_read_and_insert() {
    let session = session();
    let mut graph = SourceMemoryGraph::new(session.clone(), None).unwrap();
    let foreign = SourceMemoryGraph::new(session, None).unwrap();
    let f = iri(&foreign, "urn:ex:s");
    assert_eq!(
        graph.externalize_term(f.clone()),
        Err(SourceGraphError::ForeignOwner)
    );
    assert!(matches!(
        graph
            .internal_quads_for_pattern(Some(&f), None, None, Some(None))
            .next(),
        Some(Err(SourceGraphError::ForeignOwner))
    ));
    let p = iri(&graph, "urn:ex:p");
    let o = graph.admit_source(string(graph.session(), "a")).unwrap();
    assert_eq!(graph.insert([f, p, o]), Err(SourceGraphError::ForeignOwner));
    assert_eq!(graph.len(), 0);
}
#[test]
fn source_memory_distinct_equivalent_session_cannot_borrow_admission() {
    let graph = SourceMemoryGraph::new(session(), None).unwrap();
    assert!(NumericDataset::new_with_source_dataset(graph, session()).is_err());
}
#[test]
fn source_memory_unknown_imports_cannot_invent_literal_provenance() {
    let graph = SourceMemoryGraph::new(session(), None).unwrap();
    assert!(matches!(
        graph.internalize_term(Literal::new_simple_literal("a").into()),
        Err(SourceGraphError::UnknownSourceFacts)
    ));
    assert!(matches!(
        graph.admit_source(RawExpressionValue::from_import(
            Literal::new_typed_literal(
                "1",
                NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap()
            )
            .into()
        )),
        Err(SourceGraphError::UnknownSourceFacts)
    ));
}
#[test]
fn source_memory_named_graph_operations_keep_explicit_refusal() {
    let graph = SourceMemoryGraph::new(session(), None).unwrap();
    assert!(matches!(
        graph
            .internal_quads_for_pattern(None, None, None, None)
            .next(),
        Some(Err(SourceGraphError::UnqualifiedGraph))
    ));
    assert!(matches!(
        graph.internal_named_graphs().next(),
        Some(Err(SourceGraphError::UnqualifiedGraph))
    ));
    assert_eq!(
        graph.contains_internal_graph_name(&iri(&graph, "urn:ex:g")),
        Err(SourceGraphError::UnqualifiedGraph)
    );
}
#[test]
fn source_memory_cancelled_dataset_cannot_publish_cached_values_or_rows() {
    let cancellation = Arc::new(AtomicBool::new(false));
    let session = session();
    let mut graph = SourceMemoryGraph::new(session.clone(), Some(cancellation.clone())).unwrap();
    let raw = string(&session, "a");
    let o = graph.admit_source(raw.clone()).unwrap();
    graph
        .insert([iri(&graph, "urn:ex:s"), iri(&graph, "urn:ex:p"), o])
        .unwrap();
    let dataset = NumericDataset::new_with_source_dataset(graph, session).unwrap();
    let row = dataset
        .internal_quads_for_pattern(None, None, None, Some(None))
        .next()
        .unwrap()
        .unwrap();
    cancellation.store(true, Ordering::Relaxed);
    assert!(matches!(
        dataset
            .internal_quads_for_pattern(None, None, None, Some(None))
            .next(),
        Some(Err(SourceGraphError::Cancelled))
    ));
    assert!(dataset.externalize_raw(row.object).is_err());
    assert!(dataset.internalize_raw(raw).is_err());
}

#[test]
fn source_memory_cancelled_between_polls_fuses_actual_native_iterator() {
    let session = session();
    let cancellation = Arc::new(AtomicBool::new(false));
    let mut graph = SourceMemoryGraph::new(session.clone(), Some(cancellation.clone())).unwrap();
    for text in ["a", "b"] {
        let o = graph.admit_source(string(&session, text)).unwrap();
        graph
            .insert([iri(&graph, "urn:ex:s"), iri(&graph, "urn:ex:p"), o])
            .unwrap();
    }
    let mut rows = graph.internal_quads_for_pattern(None, None, None, Some(None));
    assert!(rows.next().unwrap().is_ok());
    cancellation.store(true, Ordering::Relaxed);
    assert!(matches!(
        rows.next(),
        Some(Err(SourceGraphError::Cancelled))
    ));
    assert!(rows.next().is_none());
    cancellation.store(false, Ordering::Relaxed);
    assert!(rows.next().is_none());
}
#[test]
fn source_memory_foreign_numeric_codec_cannot_return_cached_snapshot() {
    let session = session();
    let source = NumericDataset::new_with_source_dataset(
        SourceMemoryGraph::new(session.clone(), None).unwrap(),
        session.clone(),
    )
    .unwrap();
    let target = NumericDataset::new_with_source_dataset(
        SourceMemoryGraph::new(session.clone(), None).unwrap(),
        session.clone(),
    )
    .unwrap();
    let value = source.internalize_raw(string(&session, "a")).unwrap();
    let error = target.externalize_raw(value).unwrap_err();
    let crate::QueryEvaluationError::Dataset(error) = error else {
        panic!("actual backend error")
    };
    assert_eq!(
        error.downcast_ref::<SourceGraphError>(),
        Some(&SourceGraphError::ForeignOwner)
    );
}
#[test]
fn source_memory_literal_profile_alone_does_not_grant_scan_provenance() {
    let session = session();
    let mut graph = SourceMemoryGraph::new(session.clone(), None).unwrap();
    let o = graph.admit_source(string(&session, "a")).unwrap();
    graph
        .insert([iri(&graph, "urn:ex:s"), iri(&graph, "urn:ex:p"), o])
        .unwrap();
    let dataset =
        NumericDataset::new_with_literal_semantics(graph, QueryLiteralSemantics::SourceLiteralV1);
    let row = dataset
        .internal_quads_for_pattern(None, None, None, Some(None))
        .next()
        .unwrap()
        .unwrap();
    assert!(row.object.source_raw().is_err());
    assert!(
        matches!(dataset.externalize_raw(row.object).unwrap().literal_source(),
        LiteralSourceState::Literal(metadata) if matches!(metadata.datatype, SourceDatatype::Unknown))
    );
}

// Actual named-graph transport controls over the sealed live owner.
fn named_view(session:Rc<QueryNumericSession>)->(super::source_memory::SourceGraphView,RawExpressionValue){
    let mut graph=SourceMemoryGraph::new_dataset(session,None,NamedNode::new("urn:default").unwrap().into()).unwrap();
    let raw=RawExpressionValue::from_import(NamedNode::new("urn:named").unwrap().into()).with_new_source_object();
    let name=graph.admit_source(raw.clone()).unwrap();graph.register_context(&name).unwrap();
    (super::source_memory::SourceGraphView::new(Rc::new(std::cell::RefCell::new(graph)),None).unwrap(),raw)
}
#[test]
fn source_memory_named_graph_transport_keeps_original_object(){
    let session=session();let (view,original)=named_view(session.clone());
    let dataset=NumericDataset::new_with_source_dataset(view,session).unwrap();
    let mut names=dataset.internal_named_graphs();let name=names.next().unwrap().unwrap();assert!(names.next().is_none());
    let raw=name.source_raw().unwrap();assert!(raw.same_source_object(&original));assert_eq!(raw,&original);
    assert!(name.source_identity_key().is_ok());let actual=dataset.externalize_raw(name).unwrap();
    assert!(actual.same_source_object(&original));assert_eq!(actual.literal_source(),original.literal_source());
}
#[test]
fn source_memory_named_graph_transport_foreign_owner_refused(){
    let session=session();let (a,_)=named_view(session.clone());let (b,_)=named_view(session.clone());
    let a=NumericDataset::new_with_source_dataset(a,session.clone()).unwrap();let b=NumericDataset::new_with_source_dataset(b,session).unwrap();
    let value=a.internal_named_graphs().next().unwrap().unwrap();
    let crate::QueryEvaluationError::Dataset(error)=b.externalize_raw(value).unwrap_err() else {panic!("real backend identity refusal")};
    assert_eq!(error.downcast_ref::<SourceGraphError>(),Some(&SourceGraphError::ForeignOwner));
}
#[test]
fn source_memory_named_graph_literal_profile_is_not_provenance(){
    let (view,_)=named_view(session());let dataset=NumericDataset::new_with_literal_semantics(view,QueryLiteralSemantics::SourceLiteralV1);
    let value=dataset.internal_named_graphs().next().unwrap().unwrap();assert!(value.source_raw().is_err());assert!(value.source_identity_key().is_err());
}
#[test]
fn source_memory_named_graph_backend_error_propagates(){
    let session=session();let graph=SourceMemoryGraph::new(session.clone(),None).unwrap();let dataset=NumericDataset::new_with_source_dataset(graph,session).unwrap();
    let mut names=dataset.internal_named_graphs();assert_eq!(names.next().unwrap().unwrap_err(),SourceGraphError::UnqualifiedGraph);assert!(names.next().is_none());
}
