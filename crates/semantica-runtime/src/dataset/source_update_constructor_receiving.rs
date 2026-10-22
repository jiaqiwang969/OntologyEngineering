//! Independent software controls for explicitly supplied construction input.
//! No original oracle/output is read. Tape validation is a private harness
//! qualification, not a new collision/UUID rule in the ordinary RDF engine.
use super::*;
use super::source_update_loader::SourceUpdateBytesLoader;
use spareval::SourceTermInput;
use std::{cell::RefCell,rc::Rc};

const A:&str="00000004000040008000000000000001";
const B:&str="00000004000040008000000000000002";
const C:&str="00000004000040008000000000000003";
#[derive(Debug,PartialEq,Eq)]
enum TapeFault { InvalidUuid,Duplicate,ExistingCollision,Exhausted,UnusedInput }
impl std::fmt::Display for TapeFault {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"private_external_blank_{self:?}")}
}
impl std::error::Error for TapeFault {}
#[derive(Debug)]
struct TapeState { labels:Vec<String>,next:usize,attempts:usize,trace:Vec<(usize,String)> }
#[derive(Debug,Clone)]
struct Tape(Rc<RefCell<TapeState>>);
impl Tape {
    fn new(hex:&[&str],existing:&BTreeSet<String>)->std::result::Result<Self,TapeFault> {
        let mut labels=Vec::new(); let mut seen=BTreeSet::new();
        for h in hex {
            if h.len()!=32 || !h.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))
                || h.as_bytes()[12]!=b'4' || !matches!(h.as_bytes()[16],b'8'|b'9'|b'a'|b'b') {
                return Err(TapeFault::InvalidUuid);
            }
            let label=format!("N{h}");
            if !seen.insert(label.clone()) { return Err(TapeFault::Duplicate); }
            if existing.contains(&label) { return Err(TapeFault::ExistingCollision); }
            labels.push(label);
        }
        Ok(Self(Rc::new(RefCell::new(TapeState{labels,next:0,attempts:0,trace:Vec::new()}))))
    }
    fn hook(&self)->spargebra::SourceBlankNodeConstructor {
        let state=self.0.clone();
        Rc::new(move || {
            let mut state=state.borrow_mut();state.attempts+=1;
            let label=state.labels.get(state.next).cloned().ok_or(TapeFault::Exhausted)?;
            state.next+=1; let ordinal=state.next;
            state.trace.push((ordinal,label.clone()));
            Ok(oxrdf::BlankNode::new(label)?)
        })
    }
    fn finish(&self)->std::result::Result<(),TapeFault> {
        let state=self.0.borrow();
        if state.attempts>state.labels.len() {Err(TapeFault::Exhausted)}
        else if state.next!=state.labels.len(){Err(TapeFault::UnusedInput)}else{Ok(())}
    }
    fn calls(&self)->usize {self.0.borrow().attempts}
}
fn tape(hex:&[&str])->Tape {Tape::new(hex,&BTreeSet::new()).unwrap()}
fn label(h:&str)->String {format!("N{h}")}
fn source(t:SourceTermInput<'_>)->Value {
    match t {
        SourceTermInput::Iri(v)=>json!(["iri",v,null,null]),
        SourceTermInput::Blank(v)=>json!(["blank",v,null,null]),
        SourceTermInput::Variable(v)=>json!(["variable",v,null,null]),
        SourceTermInput::Literal{value,datatype,language}=>json!(["literal",value,datatype,language]),
    }
}
fn rows(s:&DatasetSession)->Vec<Value> {
    let mut values=Vec::new();
    s.rdflib_projection.source_state.as_ref().unwrap().visit_quads(|(t,g)|{
        values.push(json!([source(t[0]),source(t[1]),source(t[2]),g.map(source)]));Ok(())
    }).unwrap();values
}
fn fixture(two:bool)->DatasetSession {
    let mut s=DatasetSession::new_source_update_internal(true);
    let data=if two {"<urn:a> <urn:p> <urn:one> .\n<urn:b> <urn:p> <urn:two> ."}
        else {"<urn:a> <urn:p> <urn:one> ."};
    s.load_rdflib_with_base(data.as_bytes(),RdfFormat::NQuads,None,None).unwrap();s
}
fn update(s:&mut DatasetSession,q:Value,t:&Tape)->Result<SemanticMutationResult> {
    s.update_source_full_with_constructor_internal(&q,&BTreeMap::new(),&BTreeMap::new(),
        &mut SourceUpdateBytesLoader::new(BTreeMap::new(),0,0).unwrap(),Some(t.hook()))
}
fn copies(s:&DatasetSession,p:&str)->Vec<Value> {rows(s).into_iter().filter(|r|r[1]==json!(["iri",p,null,null])).collect()}
fn assert_emitted(t:&Tape,values:&[&str]) {
    assert_eq!(t.calls(),values.len());
    assert_eq!(t.0.borrow().trace,values.iter().enumerate().map(|(i,h)|(i+1,label(h))).collect::<Vec<_>>());
    t.finish().unwrap();
}

#[test]
fn source_update_constructor_shares_one_blank_per_template_row() {
    let mut s=fixture(false);let t=tape(&[A]);
    let result=update(&mut s,json!("INSERT {_:fresh <urn:left> ?s; <urn:right> ?o} WHERE {?s <urn:p> ?o}"),&t).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(2,3,2));assert_emitted(&t,&[A]);
    for p in ["urn:left","urn:right"] {let r=copies(&s,p);assert_eq!(r.len(),1);assert_eq!(r[0][0],json!(["blank",label(A),null,null]));}
    assert_eq!(copies(&s,"urn:left")[0][2],json!(["iri","urn:a",null,null]));
    assert_eq!(copies(&s,"urn:right")[0][2],json!(["iri","urn:one",null,null]));
}
#[test]
fn source_update_constructor_preserves_callback_sequence_and_output_bijection() {
    let mut s=fixture(true);let t=tape(&[B,A]);
    let result=update(&mut s,json!("INSERT {_:fresh <urn:copy> ?s} WHERE {?s <urn:p> ?o}"),&t).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(2,4,2));assert_emitted(&t,&[B,A]);
    let r=copies(&s,"urn:copy");assert_eq!(r.len(),2);
    // Memory readback and the original nonlazy UPDATE join both use sets.
    // This software control claims no insertion-order row-to-UUID assignment.
    // Exact associations are independently received by the frozen original27.
    for row in &r {
        assert_eq!(row.as_array().unwrap().len(),4);
        assert_eq!(row[1],json!(["iri","urn:copy",null,null]));assert!(row[3].is_null());
    }
    for h in [A,B] {assert_eq!(r.iter().filter(|row|row[0]==json!(["blank",label(h),null,null])).count(),1);}
    for iri in ["urn:a","urn:b"] {assert_eq!(r.iter().filter(|row|row[2]==json!(["iri",iri,null,null])).count(),1);}
}
#[test]
fn source_update_constructor_distinct_template_labels_are_distinct_calls() {
    let mut s=fixture(false);let t=tape(&[A,B]);
    update(&mut s,json!("INSERT {_:first <urn:left> ?s . _:second <urn:right> ?s} WHERE {?s <urn:p> ?o}"),&t).unwrap();assert_emitted(&t,&[A,B]);
    assert_eq!(copies(&s,"urn:left")[0][0],json!(["blank",label(A),null,null]));
    assert_eq!(copies(&s,"urn:right")[0][0],json!(["blank",label(B),null,null]));
}
#[test]
fn source_update_constructor_multiple_operations_share_input_not_template_map() {
    let mut s=fixture(false);let t=tape(&[A,B]);
    update(&mut s,json!("INSERT {_:fresh <urn:left> ?s} WHERE {?s <urn:p> ?o}; INSERT {_:fresh <urn:right> ?s} WHERE {?s <urn:p> ?o}"),&t).unwrap();assert_emitted(&t,&[A,B]);
    assert_eq!(copies(&s,"urn:left")[0][0],json!(["blank",label(A),null,null]));
    assert_eq!(copies(&s,"urn:right")[0][0],json!(["blank",label(B),null,null]));
}
#[test]
fn source_update_constructor_parser_and_execution_share_explicit_input() {
    let mut s=fixture(false);let t=tape(&[A,B]);
    update(&mut s,json!("INSERT DATA {<urn:list> <urn:head> [<urn:leaf> <urn:value>]}; INSERT {_:fresh <urn:copy> ?o} WHERE {<urn:list> <urn:head> ?o}"),&t).unwrap();assert_emitted(&t,&[A,B]);
    assert_eq!(copies(&s,"urn:head")[0][2],json!(["blank",label(A),null,null]));
    assert_eq!(copies(&s,"urn:copy")[0][0],json!(["blank",label(B),null,null]));
    assert_eq!(copies(&s,"urn:copy")[0][2],json!(["blank",label(A),null,null]));
}
#[test]
fn source_update_constructor_unbound_graph_uses_same_explicit_input() {
    let mut s=fixture(false);let t=tape(&[A]);
    update(&mut s,json!("INSERT {GRAPH ?unbound {<urn:out> <urn:copy> ?o}} WHERE {<urn:a> <urn:p> ?o}"),&t).unwrap();assert_emitted(&t,&[A]);
    let r=copies(&s,"urn:copy");assert_eq!(r.len(),1);assert_eq!(r[0][3],json!(["blank",label(A),null,null]));
}
#[test]
fn source_update_constructor_exhaustion_preserves_reached_state_and_phase() {
    let mut s=fixture(true);let t=tape(&[A]);
    s.helper_source_internal("select","SELECT ?s WHERE {?s <urn:p> ?o}",&BTreeMap::new(),&BTreeMap::new(),true).unwrap();
    s.snapshot_rdflib("2026-10-12T04:00:00Z").unwrap();
    let e=update(&mut s,json!("INSERT DATA {<urn:prefix> <urn:kept> <urn:value>}; INSERT {_:fresh <urn:copy> ?s} WHERE {?s <urn:p> ?o}"),&t).unwrap_err();
    let phase=e.downcast_ref::<spareval::SourceUpdateError>().unwrap();assert!(!phase.before_backend);
    let spareval::QueryEvaluationError::Unexpected(inner)=&phase.error else {panic!("constructor error changed owner")};
    assert_eq!(inner.downcast_ref::<TapeFault>(),Some(&TapeFault::Exhausted));
    assert_eq!(t.calls(),2);assert_eq!(t.finish(),Err(TapeFault::Exhausted));
    assert_eq!(copies(&s,"urn:kept").len(),1);assert_eq!(copies(&s,"urn:copy").len(),1);
    assert_eq!(s.quad_count(),4);assert_eq!(s.revision(),1);assert!(s.query_cache.is_empty());assert!(s.rdflib_snapshot_cache.get().is_none());
}
#[test]
fn source_update_constructor_failure_never_uses_default_entropy() {
    let mut s=fixture(false);let t=tape(&[]);let before=rows(&s);
    let e=update(&mut s,json!("INSERT {_:fresh <urn:copy> ?s} WHERE {?s <urn:p> ?o}"),&t).unwrap_err();
    let phase=e.downcast_ref::<spareval::SourceUpdateError>().unwrap();assert!(!phase.before_backend);
    let spareval::QueryEvaluationError::Unexpected(inner)=&phase.error else {panic!("constructor error changed owner")};
    assert_eq!(inner.downcast_ref::<TapeFault>(),Some(&TapeFault::Exhausted));
    assert_eq!(rows(&s),before);assert_eq!(t.calls(),1);assert_eq!(s.revision(),1);
}
#[test]
fn source_update_constructor_no_fresh_operation_does_not_consume() {
    let mut s=fixture(false);let t=tape(&[]);
    let r=update(&mut s,json!("INSERT DATA {<urn:s> <urn:q> <urn:o>}"),&t).unwrap();
    assert_eq!(r.added,1);assert_emitted(&t,&[]);
}
#[test]
fn source_update_constructor_bad_admission_never_calls_constructor() {
    for q in [Value::Null,json!(1),json!("\u{001c}\u{001f} ")] {
        let mut s=fixture(false);let t=tape(&[A]);let before=rows(&s);
        assert!(update(&mut s,q,&t).is_err());assert_eq!(t.calls(),0);assert_eq!(rows(&s),before);assert_eq!(s.revision(),1);
    }
    let mut old=DatasetSession::new_source_internal(true);let t=tape(&[A]);
    assert!(update(&mut old,json!("INSERT DATA {_:fresh <urn:p> <urn:o>}"),&t).is_err());assert_eq!(t.calls(),0);
}
#[test]
fn source_update_constructor_is_per_call_default_not_retained() {
    let mut s=fixture(false);let t=tape(&[A]);
    update(&mut s,json!("INSERT {_:fresh <urn:first> ?s} WHERE {?s <urn:p> ?o}"),&t).unwrap();assert_emitted(&t,&[A]);
    s.update_source_full_internal(&json!("INSERT {_:fresh <urn:second> ?s} WHERE {?s <urn:p> ?o}"),&BTreeMap::new(),&BTreeMap::new(),
        &mut SourceUpdateBytesLoader::new(BTreeMap::new(),0,0).unwrap()).unwrap();
    assert_emitted(&t,&[A]);let r=copies(&s,"urn:second");assert_eq!(r.len(),1);assert_eq!(r[0][0][0],"blank");
}
#[test]
fn source_update_constructor_qualification_rejects_duplicate_input() {
    assert_eq!(Tape::new(&[A,A],&BTreeSet::new()).unwrap_err(),TapeFault::Duplicate);
}
#[test]
fn source_update_constructor_qualification_rejects_invalid_uuid() {
    for h in ["", "00000004000030008000000000000001", "00000004000040007000000000000001", "0000000400004000800000000000000G"] {
        assert_eq!(Tape::new(&[h],&BTreeSet::new()).unwrap_err(),TapeFault::InvalidUuid);
    }
}
#[test]
fn source_update_constructor_qualification_rejects_existing_collision() {
    assert_eq!(Tape::new(&[A],&BTreeSet::from([label(A)])).unwrap_err(),TapeFault::ExistingCollision);
}
#[test]
fn source_update_constructor_qualification_rejects_missing_calls() {
    let mut s=fixture(false);let t=tape(&[A,B,C]);
    update(&mut s,json!("INSERT {_:fresh <urn:copy> ?s} WHERE {?s <urn:p> ?o}"),&t).unwrap();
    assert_eq!(t.calls(),1);assert_eq!(t.finish(),Err(TapeFault::UnusedInput));
    // The qualification failure does not rewrite or roll back actual state.
    assert_eq!(copies(&s,"urn:copy").len(),1);assert_eq!(s.revision(),2);
}

#[test]
fn source_update_constructor_parser_exhaustion_has_prepare_phase_and_no_prefix() {
    let mut s=fixture(false);let t=tape(&[]);let before=rows(&s);
    s.helper_source_internal("select","SELECT ?s WHERE {?s <urn:p> ?o}",&BTreeMap::new(),&BTreeMap::new(),true).unwrap();
    s.snapshot_rdflib("2026-10-12T04:00:00Z").unwrap();
    assert!(!s.query_cache.is_empty());assert!(s.rdflib_snapshot_cache.get().is_some());
    let e=update(&mut s,json!("INSERT DATA {<urn:prefix> <urn:kept> <urn:value>}; INSERT DATA {[] <urn:p> <urn:o>}"),&t).unwrap_err();
    let phase=e.downcast_ref::<spareval::SourceUpdateError>().unwrap();
    assert!(!phase.before_backend,"whole-request source preparation is inside the original backend attempt");
    assert_eq!(phase.original_exception(),None,"private callback failure is not an original RDFLib exception");
    // The transparent QueryEvaluationError source skips the boxed error itself.
    // Follow the actual prepare owner and inspect its retained callback box.
    let spareval::QueryEvaluationError::Unexpected(prepared)=&phase.error else {panic!("prepare error changed owner")};
    let query=prepared.as_ref().source().and_then(|cause|cause.downcast_ref::<spareval::QueryEvaluationError>()).expect("typed prepare query error");
    let spareval::QueryEvaluationError::Unexpected(callback)=query else {panic!("callback error changed owner")};
    assert_eq!(callback.downcast_ref::<TapeFault>(),Some(&TapeFault::Exhausted));
    assert_eq!(t.calls(),1);assert!(t.0.borrow().trace.is_empty());assert_eq!(t.finish(),Err(TapeFault::Exhausted));
    // No operation executes before whole-request preparation has succeeded.
    assert_eq!(rows(&s),before);assert!(copies(&s,"urn:kept").is_empty());
    assert_eq!(s.quad_count(),1);assert_eq!(s.revision(),1);
    assert!(s.query_cache.is_empty());assert!(s.rdflib_snapshot_cache.get().is_none());
}
