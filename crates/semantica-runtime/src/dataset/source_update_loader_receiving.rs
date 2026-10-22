//! Independent source UPDATE admission/resource controls; no network/filesystem loader.
//! Staged only. Original 884/105 receipts remain independent acceptance obligations.
use super::*;
use super::source_failure::SourceFailure;
use super::source_update_loader::{SourceUpdateBytesLoader, SourceUpdateResource};
use serde_json::{Value, json};
use spareval::{QueryEvaluationError, SourceTermInput, SourceUpdateLoader};

const RESOURCE: &str = "https://example.invalid/source.ttl";
const ONE: &[u8] = b"<urn:s> <urn:p> <urn:o> .";
const TWO: &[u8] = b"<urn:first> <urn:p> <urn:o> .\n<urn:second> <urn:p> <urn:o> .";
fn loader<'a>(bytes: &'a [u8], digest: &'a str, triples: usize) -> SourceUpdateBytesLoader<'a> {
    SourceUpdateBytesLoader::new(BTreeMap::from([(RESOURCE.into(), SourceUpdateResource::Turtle { bytes, sha256:digest })]), bytes.len(), triples).unwrap()
}
fn empty_loader() -> SourceUpdateBytesLoader<'static> { SourceUpdateBytesLoader::new(BTreeMap::new(),0,0).unwrap() }
fn update(s:&mut DatasetSession,text:&str,l:&mut dyn SourceUpdateLoader)->Result<SemanticMutationResult>{
    s.update_source_full_internal(&json!(text),&BTreeMap::new(),&BTreeMap::new(),l)
}
fn warm(s:&mut DatasetSession){
    s.query_source_internal("ASK {}",&BTreeMap::new(),&BTreeMap::new(),true,None).unwrap();
    s.snapshot_rdflib("2000-01-01T00:00:00Z").unwrap();
    assert_eq!(s.query_cache.len(),1);assert!(s.rdflib_snapshot_cache.get().is_some());
}
fn ask(s:&mut DatasetSession,text:&str)->bool{
    s.query_source_internal(text,&BTreeMap::new(),&BTreeMap::new(),false,None).unwrap().result["boolean"].as_bool().unwrap()
}
fn empty_input_error(error:anyhow::Error){
    let e=error.downcast_ref::<SourceFailure>().expect("original runtime input exception, not a parser fallback");
    assert_eq!(e.class,"SemanticInputError");assert_eq!(e.to_string(),"SPARQL update must be a non-empty string");
}
#[test]
fn source_runtime_update_loader_exact_bytes_sha_iri_and_base(){
    let bytes=b"<subject> <predicate> <object> .";let digest=crate::sha256(bytes);let mut l=loader(bytes,&digest,1);let mut actual=Vec::new();
    assert!(l.load(RESOURCE,&mut |t|{actual.push(t.map(|x|match x {SourceTermInput::Iri(v)=>v.to_owned(),_=>panic!("independent fixture expects IRIs")}));Ok(())}).unwrap());
    assert_eq!(actual,vec![["https://example.invalid/subject".to_owned(),"https://example.invalid/predicate".to_owned(),"https://example.invalid/object".to_owned()]]);
}
#[test]
fn source_runtime_update_loader_wrong_sha_refused_before_load(){
    let e=SourceUpdateBytesLoader::new(BTreeMap::from([(RESOURCE.into(),SourceUpdateResource::Turtle{bytes:ONE,sha256:"not-the-source-digest"})]),ONE.len(),1).err().expect("identity mismatch");
    assert!(format!("{e:#}").contains("source_update_resource_identity_mismatch"));
}
#[test]
fn source_runtime_update_loader_observed_absence_is_false(){
    let mut l=SourceUpdateBytesLoader::new(BTreeMap::from([(RESOURCE.into(),SourceUpdateResource::ObservedAbsent)]),0,0).unwrap();let mut emitted=0;
    assert!(!l.load(RESOURCE,&mut |_|{emitted+=1;Ok(())}).unwrap());assert_eq!(emitted,0);
}
#[test]
fn source_runtime_update_loader_unknown_iri_is_not_absence(){
    let digest=crate::sha256(ONE);let mut l=loader(ONE,&digest,1);let mut emitted=0;
    let e=l.load("https://example.invalid/unregistered.ttl",&mut |_|{emitted+=1;Ok(())}).unwrap_err();
    assert!(e.to_string().contains("source_update_resource_not_authorized"));assert_eq!(emitted,0);
}
#[test]
fn source_runtime_update_loader_cumulative_byte_limit_refused(){
    let digest=crate::sha256(ONE);let entry=SourceUpdateResource::Turtle{bytes:ONE,sha256:&digest};
    let e=SourceUpdateBytesLoader::new(BTreeMap::from([(RESOURCE.into(),entry),("https://example.invalid/second.ttl".into(),entry)]),ONE.len()*2-1,2).err().expect("aggregate byte budget");
    assert!(e.to_string().contains("source_update_resource_byte_budget"));
}
#[test]
fn source_runtime_update_loader_resource_count_refused(){
    let resources=(0..129).map(|n|(format!("https://example.invalid/{n}"),SourceUpdateResource::ObservedAbsent)).collect();
    let e=SourceUpdateBytesLoader::new(resources,0,0).err().expect("resource count budget");assert!(e.to_string().contains("source_update_resource_count_budget"));
}
#[test]
fn source_runtime_update_loader_triple_limit_retains_emitted_prefix(){
    let digest=crate::sha256(TWO);let mut l=loader(TWO,&digest,1);let mut subjects=Vec::new();
    let e=l.load(RESOURCE,&mut |t|{subjects.push(match t[0]{SourceTermInput::Iri(v)=>v.to_owned(),_=>panic!("IRI fixture")});Ok(())}).unwrap_err();
    assert!(e.to_string().contains("source_update_resource_triple_budget"));assert_eq!(subjects,vec!["urn:first"]);
}
#[test]
fn source_runtime_update_loader_triple_budget_spans_loads(){
    let digest=crate::sha256(ONE);let mut l=loader(ONE,&digest,1);let mut count=0;
    assert!(l.load(RESOURCE,&mut |_|{count+=1;Ok(())}).unwrap());
    assert!(l.load(RESOURCE,&mut |_|{count+=1;Ok(())}).unwrap_err().to_string().contains("source_update_resource_triple_budget"));assert_eq!(count,1);
}
#[test]
fn source_runtime_update_loader_emission_failure_propagates(){
    let digest=crate::sha256(ONE);let mut l=loader(ONE,&digest,1);let mut called=0;
    let e=l.load(RESOURCE,&mut |_|{called+=1;Err(QueryEvaluationError::Unexpected("independent sink refusal".into()))}).unwrap_err();
    assert!(e.to_string().contains("independent sink refusal"));assert_eq!(called,1);
}
#[test]
fn source_runtime_update_loader_silent_unknown_authority_refused(){
    let mut s=DatasetSession::new_source_update_internal(true);let e=update(&mut s,"LOAD SILENT <https://example.invalid/source.ttl>",&mut empty_loader()).unwrap_err();
    assert!(format!("{e:#}").contains("source_update_resource_not_authorized"));assert_eq!(s.quad_count(),0);assert_eq!(s.revision(),0);
}
#[test]
fn source_runtime_update_loader_silent_observed_absence_allowed(){
    let mut s=DatasetSession::new_source_update_internal(true);let mut l=SourceUpdateBytesLoader::new(BTreeMap::from([(RESOURCE.into(),SourceUpdateResource::ObservedAbsent)]),0,0).unwrap();
    update(&mut s,"LOAD SILENT <https://example.invalid/source.ttl>",&mut l).unwrap();assert_eq!(s.quad_count(),0);
}
#[test]
fn source_runtime_update_loader_silent_parser_error_refused(){
    let bytes=b"<broken> <unfinished";let digest=crate::sha256(bytes);let mut l=loader(bytes,&digest,8);let mut s=DatasetSession::new_source_update_internal(true);
    assert!(update(&mut s,"LOAD SILENT <https://example.invalid/source.ttl>",&mut l).is_err());assert_eq!(s.quad_count(),0);assert_eq!(s.revision(),0);
}
#[test]
fn source_runtime_update_loader_silent_budget_error_keeps_reached_state(){
    let digest=crate::sha256(TWO);let mut l=loader(TWO,&digest,1);let mut s=DatasetSession::new_source_update_internal(true);warm(&mut s);
    let e=update(&mut s,"LOAD SILENT <https://example.invalid/source.ttl>",&mut l).unwrap_err();assert!(format!("{e:#}").contains("source_update_resource_triple_budget"));
    assert_eq!(s.quad_count(),1);assert_eq!(s.revision(),0);assert!(s.query_cache.is_empty());assert!(s.rdflib_snapshot_cache.get().is_none());
    assert!(ask(&mut s,"ASK {<urn:first> <urn:p> <urn:o>}"));assert!(!ask(&mut s,"ASK {<urn:second> <urn:p> <urn:o>}"));
}
struct FailEmission<'a>{inner:SourceUpdateBytesLoader<'a>}
impl SourceUpdateLoader for FailEmission<'_>{
    fn load(&mut self,iri:&str,_emit:&mut dyn FnMut([SourceTermInput<'_>;3])->std::result::Result<(),QueryEvaluationError>)->std::result::Result<bool,QueryEvaluationError>{
        self.inner.load(iri,&mut |_|Err(QueryEvaluationError::Unexpected("independent sink refusal".into())))
    }
}
#[test]
fn source_runtime_update_loader_silent_emission_error_refused(){
    let digest=crate::sha256(ONE);let mut l=FailEmission{inner:loader(ONE,&digest,1)};let mut s=DatasetSession::new_source_update_internal(true);
    let e=update(&mut s,"LOAD SILENT <https://example.invalid/source.ttl>",&mut l).unwrap_err();assert!(format!("{e:#}").contains("independent sink refusal"));assert_eq!(s.quad_count(),0);assert_eq!(s.revision(),0);
}
#[test]
fn source_runtime_update_loader_v3_text_entry_requires_explicit_options(){
    let mut s=DatasetSession::new_source_update_internal(true);warm(&mut s);let e=s.update_rdflib("INSERT DATA {<urn:s> <urn:p> <urn:o>}").unwrap_err();
    assert!(e.to_string().contains("source_UPDATE_v3_requires_explicit_options_and_loader"));assert_eq!(s.quad_count(),0);assert_eq!(s.revision(),0);assert_eq!(s.query_cache.len(),1);assert!(s.rdflib_snapshot_cache.get().is_some());
}
#[test]
fn source_runtime_update_loader_v1_and_v2_keep_previous_admission(){
    let mut old=DatasetSession::new_source_internal(true);assert!(old.update_rdflib("INSERT DATA {<urn:s> <urn:p> <urn:o>}").is_err());assert!(update(&mut old,"CREATE GRAPH <urn:g>",&mut empty_loader()).is_err());assert_eq!(old.quad_count(),0);assert_eq!(old.revision(),0);
    let mut v2=DatasetSession::new_source_data_internal(true);v2.update_rdflib("INSERT DATA {<urn:s> <urn:p> <urn:o>}").unwrap();assert_eq!(v2.quad_count(),1);assert_eq!(v2.revision(),1);
    assert!(update(&mut v2,"CREATE GRAPH <urn:g>",&mut empty_loader()).is_err());assert_eq!(v2.quad_count(),1);assert_eq!(v2.revision(),1);
    assert!(v2.update_rdflib("INSERT DATA {<urn:second> <urn:p> <urn:o>}; CREATE GRAPH <urn:g>").is_err());assert_eq!(v2.quad_count(),2);assert_eq!(v2.revision(),1);
}
#[test]
fn source_runtime_update_loader_v3_create_failure_preserves_reached_prefix() {
    let mut s = DatasetSession::new_source_update_internal(true);
    warm(&mut s);
    // The query text is byte-identical to the old, incorrectly-successful control.
    let error = update(&mut s, "INSERT DATA {<urn:s> <urn:p> <urn:o>}; CREATE GRAPH <urn:g>", &mut empty_loader()).unwrap_err();
    assert_eq!(s.quad_count(), 1);
    assert_eq!(s.revision(), 0);
    assert!(s.query_cache.is_empty());
    assert!(s.rdflib_snapshot_cache.get().is_none());
    assert!(ask(&mut s, "ASK {<urn:s> <urn:p> <urn:o>}"));
    let original = error.downcast_ref::<SourceFailure>().expect("source-bound original runtime error; generic Rust error is not acceptance");
    assert_eq!(original.module, "semantica.ontology.runtime");
    assert_eq!(original.class, "SemanticInputError");
    assert_eq!(original.to_string(), "SPARQL update failed: Create not implemented!");
}
#[test]
fn source_runtime_update_loader_v3_legal_insert_delete_sequence_succeeds() {
    let mut s = DatasetSession::new_source_update_internal(true);
    warm(&mut s);
    let result = update(&mut s, "INSERT DATA {<urn:first> <urn:p> <urn:o>}; DELETE DATA {<urn:first> <urn:p> <urn:o>}; INSERT DATA {<urn:final> <urn:p> <urn:o>}", &mut empty_loader()).unwrap();
    assert_eq!(result.added, 1);
    assert_eq!(result.removed, 0);
    assert_eq!(result.quad_count, 1);
    assert_eq!(result.revision, 1);
    assert_eq!(s.quad_count(), 1);
    assert_eq!(s.revision(), 1);
    assert!(s.query_cache.is_empty());
    assert!(s.rdflib_snapshot_cache.get().is_none());
    assert!(!ask(&mut s, "ASK {<urn:first> <urn:p> <urn:o>}"));
    assert!(ask(&mut s, "ASK {<urn:final> <urn:p> <urn:o>}"));
}
#[test]
fn source_runtime_update_loader_nonstring_admission_keeps_warm_state(){
    for value in [Value::Null,json!(false),json!(42),json!([])]{
        let mut s=DatasetSession::new_source_update_internal(true);warm(&mut s);empty_input_error(s.update_source_full_internal(&value,&BTreeMap::new(),&BTreeMap::new(),&mut empty_loader()).unwrap_err());
        assert_eq!(s.quad_count(),0);assert_eq!(s.revision(),0);assert_eq!(s.query_cache.len(),1);assert!(s.rdflib_snapshot_cache.get().is_some());
    }
}
#[test]
fn source_runtime_update_loader_python_strip_control_empty_boundary(){
    for text in ["", " \t\n", "\u{1c}", "\u{1f}", "\u{1c} \u{1f}"]{
        assert!(semantica_contracts::json::python_strip(text).is_empty());let mut s=DatasetSession::new_source_update_internal(true);warm(&mut s);
        empty_input_error(update(&mut s,text,&mut empty_loader()).unwrap_err());assert_eq!(s.quad_count(),0);assert_eq!(s.revision(),0);assert_eq!(s.query_cache.len(),1);assert!(s.rdflib_snapshot_cache.get().is_some());
    }
}
#[test]
fn source_runtime_update_loader_python_strip_zero_width_is_not_empty(){
    let text="\u{200b}";assert!(!semantica_contracts::json::python_strip(text).is_empty());let mut s=DatasetSession::new_source_update_internal(true);warm(&mut s);
    let e=update(&mut s,text,&mut empty_loader()).unwrap_err();assert_ne!(e.to_string(),"SPARQL update must be a non-empty string");assert_eq!(s.revision(),0);assert_eq!(s.quad_count(),0);assert!(s.query_cache.is_empty());assert!(s.rdflib_snapshot_cache.get().is_none());
}
#[test]
fn source_runtime_update_loader_authorized_load_reaches_runtime_state(){
    let digest=crate::sha256(ONE);let mut l=loader(ONE,&digest,1);let mut s=DatasetSession::new_source_update_internal(true);
    update(&mut s,"LOAD <https://example.invalid/source.ttl>",&mut l).unwrap();assert_eq!(s.quad_count(),1);assert_eq!(s.revision(),1);assert!(ask(&mut s,"ASK {<urn:s> <urn:p> <urn:o>}"));
}
