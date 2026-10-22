//! Frozen actual RDFLib Memory actions and ordered source-object readbacks.
//! No expected graph order, representative selection or context management is
//! computed here; all are received from the independent original execution.
use super::QueryableDataset;
use super::source_memory::{SourceGraphTerm, SourceMemoryGraph};
use crate::numeric::{
    LiteralSourceState, QueryContextSemantics, QueryGroupSemantics, QueryLiteralSemantics,
    QueryNumericCapabilities, QueryNumericSession, SourceDatatype, SourceFact, SourceHashPolicy,
    SourceHashRuntime,
};
use oxrdf::{BlankNode, NamedNode, Term};
use semantica_numeric::{DecimalContext, NumericLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::rc::Rc;

const FIXTURE: &str = include_str!("fixtures/source_mutable_memory_cases.json");
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
fn receive(id: &str) { receive_with_transfer(id, false); }
fn receive_with_transfer(id: &str, transfer:bool) {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "24a59242fdabb5036cd54d624861fbe1be51fa971985ab1f2ab3544a94745068"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["frozen_sha256"],
        "884425b2ec30fe482105fceaa617278db36603c2b43a90b41348558fe49d4635"
    );
    let case = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    let session = session();
    let mut graph = SourceMemoryGraph::new_dataset(
        session.clone(),
        None,
        NamedNode::new("urn:default").unwrap().into(),
    )
    .unwrap();
    let terms: BTreeMap<String, SourceGraphTerm> = root["terms"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, spec)| {
            let text = spec["value"].as_str().unwrap();
            let term = match spec["kind"].as_str().unwrap() {
                "URIRef" => graph
                    .internalize_term(NamedNode::new(text).unwrap().into())
                    .unwrap(),
                "BNode" => graph
                    .internalize_term(BlankNode::new(text).unwrap().into())
                    .unwrap(),
                "Literal" => graph
                    .admit_source(
                        session
                            .construct_source_direct_string(
                                text,
                                spec["datatype"]
                                    .as_str()
                                    .map(|v| NamedNode::new(v).unwrap()),
                                spec["language"].as_str().map(str::to_owned),
                                spec["normalize"].as_bool().unwrap_or(true),
                            )
                            .unwrap(),
                    )
                    .unwrap(),
                other => panic!("unreceived node {other}"),
            };
            (name.clone(), term)
        })
        .collect();
    let pattern = |value: &Value| -> [Option<SourceGraphTerm>; 3] {
        std::array::from_fn(|i| value[i].as_str().map(|v| terms[v].clone()))
    };
    let encode = |graph: &SourceMemoryGraph, term: &SourceGraphTerm| {
        let raw = graph.externalize_source(term.clone()).unwrap();
        let token = terms
            .iter()
            .find(|(_, v)| {
                graph
                    .externalize_source((*v).clone())
                    .unwrap()
                    .same_source_object(&raw)
            })
            .map(|(name, _)| name)
            .expect("actual input object retained");
        let (kind, text, datatype, language) = match raw.rdf().unwrap() {
            Term::NamedNode(v) => ("URIRef", v.as_str(), Value::Null, Value::Null),
            Term::BlankNode(v) => ("BNode", v.as_str(), Value::Null, Value::Null),
            Term::Literal(v) => {
                let LiteralSourceState::Literal(meta) = raw.literal_source() else {
                    panic!("missing source metadata")
                };
                let dt = match &meta.datatype {
                    SourceDatatype::Absent => Value::Null,
                    SourceDatatype::Specified(v) => json!(v.as_str()),
                    SourceDatatype::Unknown => panic!("unknown datatype"),
                };
                let SourceFact::Known(lang) = &meta.language_original else {
                    panic!("unknown language")
                };
                ("Literal", v.value(), dt, json!(lang))
            }
        };
        json!({"token":token,"kind":kind,"text":text,"hash":session.source_hash().unwrap().term_hash(&raw).unwrap(),"datatype":datatype,"language":language})
    };
    for (at, action) in case["input"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let name = action["scope"].as_str().unwrap();
        let scope = if name == "union" {
            None
        } else {
            Some(&terms[name])
        };
        match action["op"].as_str().unwrap() {
            "add" => {
                for triple in action["triples"].as_array().unwrap() {
                    graph
                        .insert_context(scope.unwrap(), pattern(triple).map(Option::unwrap))
                        .unwrap();
                }
            }
            "remove" => graph
                .remove_context(scope, pattern(&action["pattern"]))
                .unwrap(),
            "remove_many" => {
                for pat in action["patterns"].as_array().unwrap() {
                    graph.remove_context(scope, pattern(pat)).unwrap();
                }
            }
            "register" => graph.register_context(scope.unwrap()).unwrap(),
            "drop" => graph.drop_context(scope.unwrap()).unwrap(),
            other => panic!("unreceived operation {other}"),
        }
        if transfer {
            let state=graph.into_state();
            let state=std::thread::spawn(move || state).join().unwrap();
            graph=SourceMemoryGraph::from_state(state,session.clone(),None).unwrap();
        }
        let contexts: Vec<_> = graph
            .context_names()
            .unwrap()
            .iter()
            .map(|v| encode(&graph, v))
            .collect();
        let mut views = BTreeMap::new();
        for name in std::iter::once("union").chain(
            root["graph_names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap()),
        ) {
            let scope = if name == "union" {
                None
            } else {
                Some(&terms[name])
            };
            let scans:Vec<_>=root["scans"].as_array().unwrap().iter().map(|scan|{
                graph.scan_context(scope,pattern(scan)).unwrap().iter().map(|triple|{
                    let contexts:Vec<_>=graph.triple_contexts(triple).unwrap().iter().map(|v|encode(&graph,v)).collect();
                    json!({"terms":triple.each_ref().map(|v|encode(&graph,v)),"hash":graph.triple_hash(triple).unwrap(),"contexts":contexts})
                }).collect::<Vec<_>>()
            }).collect();
            views.insert(
                name,
                json!({"length":graph.len_context(scope).unwrap(),"scans":scans}),
            );
        }
        let actual = json!({"contexts":contexts,"views":views});
        let expected = &case["expected"][at];
        // A bounded failing path keeps the original full records on disk while
        // avoiding megabytes of assertion output for a single changed row.
        assert_eq!(
            actual["contexts"], expected["contexts"],
            "{id}/{at} registered graphs"
        );
        for name in ["union", "d", "g", "h", "e"] {
            assert_eq!(
                actual["views"][name]["length"], expected["views"][name]["length"],
                "{id}/{at} {name} length"
            );
            for scan in 0..8 {
                let a = actual["views"][name]["scans"][scan].as_array().unwrap();
                let e = expected["views"][name]["scans"][scan].as_array().unwrap();
                assert_eq!(a.len(), e.len(), "{id}/{at} {name} scan {scan} length");
                for (row, (a, e)) in a.iter().zip(e).enumerate() {
                    assert_eq!(a, e, "{id}/{at} {name} scan {scan} row {row}");
                }
            }
        }
    }
}
macro_rules! receiving{($($name:ident=>$id:literal),+$(,)?)=>{$(#[test]fn $name(){receive($id);})+};}
receiving! {
 source_mutable_memory_mg01_default_representatives=>"MG01",
 source_mutable_memory_mg02_retained_index_parents=>"MG02",
 source_mutable_memory_mg03_set_growth_delete_reinsert=>"MG03",
 source_mutable_memory_mg04_shared_context_membership=>"MG04",
 source_mutable_memory_mg05_empty_registration_clear_drop=>"MG05",
 source_mutable_memory_mg06_union_removal=>"MG06",
 source_mutable_memory_mg07_default_identifier_context=>"MG07",
 source_mutable_memory_mg08_blank_named_context=>"MG08",
 source_mutable_memory_mg09_generalized_nodes=>"MG09",
 source_mutable_memory_mg10_history_not_reconstruction=>"MG10",
 source_mutable_memory_mg11_same_final_values_other_history=>"MG11",
 source_mutable_memory_mg12_missing_context_no_mutation=>"MG12",
}

#[test]
fn source_mutable_memory_foreign_context_and_pattern_preserve_live_state() {
    use super::source_memory::SourceGraphError;
    let session = session();
    let mut graph = SourceMemoryGraph::new_dataset(
        session.clone(),
        None,
        NamedNode::new("urn:d").unwrap().into(),
    )
    .unwrap();
    let foreign =
        SourceMemoryGraph::new_dataset(session, None, NamedNode::new("urn:d").unwrap().into())
            .unwrap();
    let node = |g: &SourceMemoryGraph, text: &str| {
        g.internalize_term(NamedNode::new(text).unwrap().into())
            .unwrap()
    };
    let name = node(&graph, "urn:g");
    let triple = [
        node(&graph, "urn:s"),
        node(&graph, "urn:p"),
        node(&graph, "urn:o"),
    ];
    graph.insert_context(&name, triple.clone()).unwrap();
    let foreign_name = node(&foreign, "urn:g");
    assert_eq!(
        graph.drop_context(&foreign_name),
        Err(SourceGraphError::ForeignOwner)
    );
    assert_eq!(
        graph.register_context(&foreign_name),
        Err(SourceGraphError::ForeignOwner)
    );
    assert_eq!(
        graph.remove_context(Some(&name), [Some(node(&foreign, "urn:s")), None, None]),
        Err(SourceGraphError::ForeignOwner)
    );
    assert_eq!(
        graph.scan_context(Some(&name), [None, None, None]).unwrap(),
        [triple]
    );
    assert_eq!(graph.context_names().unwrap(), [name]);
}

#[test]
fn source_mutable_memory_preflight_cancellation_leaves_all_contexts_unchanged() {
    use super::source_memory::SourceGraphError;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let mut graph = SourceMemoryGraph::new_dataset(
        session(),
        Some(cancel.clone()),
        NamedNode::new("urn:d").unwrap().into(),
    )
    .unwrap();
    let node = |text: &str| {
        graph
            .internalize_term(NamedNode::new(text).unwrap().into())
            .unwrap()
    };
    let name = node("urn:g");
    let other = node("urn:h");
    let triple = [node("urn:s"), node("urn:p"), node("urn:o")];
    graph.insert_context(&name, triple.clone()).unwrap();
    graph.insert_context(&other, triple.clone()).unwrap();
    let contexts = graph.context_names().unwrap();
    cancel.store(true, Ordering::SeqCst);
    assert_eq!(
        graph.remove_context(None, [None, None, None]),
        Err(SourceGraphError::Cancelled)
    );
    assert_eq!(graph.drop_context(&name), Err(SourceGraphError::Cancelled));
    assert_eq!(
        graph.insert_context(&name, triple.clone()),
        Err(SourceGraphError::Cancelled)
    );
    cancel.store(false, Ordering::SeqCst);
    assert_eq!(graph.context_names().unwrap(), contexts);
    assert_eq!(
        graph.scan_context(Some(&name), [None, None, None]).unwrap(),
        [triple.clone()]
    );
    assert_eq!(
        graph
            .scan_context(Some(&other), [None, None, None])
            .unwrap(),
        [triple]
    );
    assert_eq!(graph.len_context(None).unwrap(), 1);
}

#[test]
fn source_persistent_memory_all_original_states_survive_thread_transfer() {
    for n in 1..=12 {receive_with_transfer(&format!("MG{n:02}"),true);}
}
