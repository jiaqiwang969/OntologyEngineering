//! Independent UPDATE execution controls, distinct from original exposed replay.
//! Read actual source memberships; no native RDF reconstruction or second engine.
use super::*;
use super::source_update_loader::SourceUpdateBytesLoader;
use spareval::SourceTermInput;

const STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
fn iri(v:&str)->Value { json!(["iri",v,null,null]) }
fn literal(v:&str,dt:Option<&str>,lang:Option<&str>)->Value { json!(["literal",v,dt,lang]) }
fn term(v:SourceTermInput<'_>)->Value {
    match v {
        SourceTermInput::Iri(v)=>iri(v),
        SourceTermInput::Blank(v)=>json!(["blank",v,null,null]),
        SourceTermInput::Variable(v)=>json!(["variable",v,null,null]),
        SourceTermInput::Literal{value,datatype,language}=>literal(value,datatype,language),
    }
}
fn rows(s:&DatasetSession)->Vec<Value>{
    let mut rows=Vec::new();
    s.rdflib_projection.source_state.as_ref().unwrap().visit_quads(|(t,g)| {
        rows.push(json!([term(t[0]),term(t[1]),term(t[2]),g.map(term)])); Ok(())
    }).unwrap();rows
}
fn load(text:&str)->DatasetSession {
    let mut s=DatasetSession::new_source_update_internal(true);
    s.load_rdflib_with_base(text.as_bytes(),RdfFormat::NQuads,None,None).unwrap();s
}
fn update(s:&mut DatasetSession,text:&str,bindings:BTreeMap<String,Value>)->Result<SemanticMutationResult>{
    s.update_source_full_internal(&json!(text),&BTreeMap::new(),&bindings,
        &mut SourceUpdateBytesLoader::new(BTreeMap::new(),0,0).unwrap())
}
fn assert_exact_rows(s:&DatasetSession,expected:Vec<Value>){
    let actual=rows(s);assert_eq!(actual.len(),expected.len());
    let actual:std::collections::BTreeSet<_>=actual.iter().map(|v|serde_json::to_string(v).unwrap()).collect();
    let expected:std::collections::BTreeSet<_>=expected.iter().map(|v|serde_json::to_string(v).unwrap()).collect();
    assert_eq!(actual,expected);
}
fn optional_fixture()->DatasetSession {
    load("<urn:a> <urn:p> \"left\" .\n<urn:a> <urn:q> \"value\"@EN-us .\n<urn:b> <urn:p> \"without-right\" .")
}
fn optional_expected()->Vec<Value>{
    vec![json!([iri("urn:a"),iri("urn:p"),literal("left",None,None),null]),
        json!([iri("urn:a"),iri("urn:q"),literal("value",None,Some("EN-us")),null]),
        json!([iri("urn:b"),iri("urn:p"),literal("without-right",None,None),null]),
        json!([iri("urn:a"),iri("urn:out"),literal("value",None,Some("EN-us")),null])]
}
#[test]
fn source_update_execution_named_graph_source_metadata(){
    let mut s=load("<urn:s> <urn:p> \"same\" <urn:g> .\n<urn:s> <urn:p> \"same\"^^<http://www.w3.org/2001/XMLSchema#string> <urn:g> .\n<urn:t> <urn:p> \"value\"@EN-us <urn:h> .\n<urn:d> <urn:p> \"default\" .");
    let mut expected=rows(&s);
    expected.extend([
        json!([iri("urn:s"),iri("urn:copied"),literal("same",None,None),iri("urn:g")]),
        json!([iri("urn:s"),iri("urn:copied"),literal("same",Some(STRING),None),iri("urn:g")]),
        json!([iri("urn:t"),iri("urn:copied"),literal("value",None,Some("EN-us")),iri("urn:h")])]);
    let result=update(&mut s,"INSERT {GRAPH ?g {?s <urn:copied> ?o}} WHERE {GRAPH ?g {?s <urn:p> ?o}}",BTreeMap::new()).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(3,7,2));assert_exact_rows(&s,expected);
}
#[test]
fn source_update_execution_initial_graph_binding_restricts_scope(){
    let mut s=load("<urn:s> <urn:p> \"g\" <urn:g> .\n<urn:s> <urn:p> \"h\" <urn:h> .\n<urn:d> <urn:p> \"default\" .");
    let mut expected=rows(&s);expected.push(json!([iri("urn:s"),iri("urn:copied"),literal("g",None,None),iri("urn:g")]));
    let result=update(&mut s,"INSERT {GRAPH ?g {?s <urn:copied> ?o}} WHERE {GRAPH ?g {?s <urn:p> ?o}}",BTreeMap::from([("g".into(),json!({"term_type":"iri","value":"urn:g"}))])).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(1,4,2));assert_exact_rows(&s,expected);
}
#[test]
fn source_update_execution_optional_match_keeps_source_and_absence(){
    let mut s=optional_fixture();let result=update(&mut s,"INSERT {?s <urn:out> ?v} WHERE {?s <urn:p> ?o OPTIONAL {?s <urn:q> ?v}}",BTreeMap::new()).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(1,4,2));assert_exact_rows(&s,optional_expected());
}
#[test]
fn source_update_execution_optional_authored_true_not_synthetic(){
    let mut s=optional_fixture();let result=update(&mut s,"INSERT {?s <urn:out> ?v} WHERE {?s <urn:p> ?o OPTIONAL {?s <urn:q> ?v FILTER(true)}}",BTreeMap::new()).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(1,4,2));assert_exact_rows(&s,optional_expected());
}
#[test]
fn source_update_execution_optional_false_literal_follows_source_translation(){
    // Original algebra.translateGroupGraphPattern tests `if filters` after
    // and_(one) returns that literal. Literal(false) is omitted at translation;
    // OPTIONAL therefore receives original TrueFilter, not an evaluated false.
    // Independently observed OF05: added1/quad4/revision2, original EN-us intact.
    let mut s=optional_fixture();let result=update(&mut s,"INSERT {?s <urn:out> ?v} WHERE {?s <urn:p> ?o OPTIONAL {?s <urn:q> ?v FILTER(false)}}",BTreeMap::new()).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(1,4,2));assert_exact_rows(&s,optional_expected());
}
#[test]
fn source_update_execution_union_keeps_plain_and_typed(){
    let mut s=load("<urn:s> <urn:p> \"same\" .\n<urn:s> <urn:q> \"same\"^^<http://www.w3.org/2001/XMLSchema#string> .");
    let mut expected=rows(&s);expected.extend([
        json!([iri("urn:s"),iri("urn:seen"),literal("same",None,None),null]),
        json!([iri("urn:s"),iri("urn:seen"),literal("same",Some(STRING),None),null])]);
    let result=update(&mut s,"INSERT {?s <urn:seen> ?o} WHERE {{?s <urn:p> ?o} UNION {?s <urn:q> ?o}}",BTreeMap::new()).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(2,4,2));assert_exact_rows(&s,expected);
}
#[test]
fn source_update_execution_outer_update_join_deduplicates_union_solutions(){
    // Original evalUnion retains both rows. UPDATE keeps an outer non-lazy
    // Join(BGP(), Union(...)); evalJoin materializes its right side as a set,
    // leaving one solution for template construction (original OU05 observed).
    let mut s=load("<urn:s> <urn:p> \"value\" .");
    let result=update(&mut s,"INSERT {_:fresh <urn:copy> ?o} WHERE {{<urn:s> <urn:p> ?o} UNION {<urn:s> <urn:p> ?o}}",BTreeMap::new()).unwrap();
    assert_eq!((result.added,result.quad_count,result.revision),(1,2,2));
    let copies:Vec<_>=rows(&s).into_iter().filter(|r|r[1]==iri("urn:copy")).collect();assert_eq!(copies.len(),1);
    for row in copies {assert_eq!(row[0][0],"blank");assert_eq!(row[2],literal("value",None,None));assert!(row[3].is_null());}
}
#[test]
fn source_update_execution_union_failure_retains_prior_operation(){
    let mut s=load("<urn:s> <urn:p> \"value\" .");let mut expected=rows(&s);
    expected.push(json!([iri("urn:prefix"),iri("urn:p"),iri("urn:kept"),null]));
    let result=update(&mut s,"INSERT DATA {<urn:prefix> <urn:p> <urn:kept>}; INSERT {?s <urn:seen> ?o} WHERE {{?s <urn:p> ?o} UNION {VALUES () {}}}",BTreeMap::new());
    let error=result.unwrap_err();
    let phase=error.downcast_ref::<spareval::SourceUpdateError>()
        .expect("actual source UPDATE phase error; unrelated failure cannot satisfy this control");
    assert!(!phase.before_backend,"empty VALUES is an original reached-backend failure");
    assert_eq!(phase.error.to_string(),"AttributeError: 'list' object has no attribute 'name'");
    assert_eq!(s.revision(),1);assert_eq!(s.quad_count(),2);assert_exact_rows(&s,expected);
}

// Candidate only: source-derived independent expectation, not an original
// observation and not yet compiled or run. Append beside the existing helpers.
#[test]
fn source_update_execution_optional_absent_scope_does_not_retry() {
    let mut s = load("<urn:seed> <urn:p> <urn:initial> .");
    let expected = vec![
        json!([iri("urn:seed"), iri("urn:p"), iri("urn:initial"), null]),
        json!([iri("urn:s"), iri("urn:seen"), iri("urn:kept"), null]),
    ];
    let result = update(&mut s,
        "INSERT {<urn:s> <urn:seen> ?marker} WHERE { VALUES ?x {<urn:outside>} OPTIONAL { BIND(<urn:kept> AS ?marker) OPTIONAL { VALUES ?x {<urn:other>} } } }",
        BTreeMap::new(),
    ).unwrap();
    assert_eq!(
        (result.added, result.quad_count, result.revision),
        (1, 2, 2)
    );
    assert_exact_rows(&s, expected);
}
