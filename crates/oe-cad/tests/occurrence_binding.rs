//! Explicit historical native-data replay. This launches no NX or model API.
//! Independent expected nominal positions/mutations were frozen before coding.
use oe_cad::evidence::correspondence::occurrence;
use oe_contracts::CadOccurrenceBindingRequest1;
use oe_core::{SkillRoot,read_regular_bytes,sha256};
use serde_json::{Value,json};
use std::path::Path;

#[test]
#[ignore="requires explicit frozen private source04 native replay inputs"]
fn current_occurrence_binding_frozen_native_replay() {
    let root=SkillRoot::new(std::path::PathBuf::from(std::env::var_os("OE_CAD_INSTANCE_REPLAY_ROOT").expect("explicit skill root required"))).unwrap();
    let file=std::env::var_os("OE_CAD_INSTANCE_REPLAY_MANIFEST").expect("explicit frozen manifest required");
    let bytes=read_regular_bytes(Path::new(&file),1024*1024).unwrap();
    assert_eq!(sha256(&bytes),"30e741ea05c6e16d86b64ab6c80b22d351d866b64bea916d1fca5af8d2bc6fba");
    let manifest:Value=oe_core::json::strict_json(&bytes).unwrap();
    let mut actual=Vec::new();let mut capabilities=Vec::new();
    for case in manifest["cases"].as_array().unwrap() {
        let path=root.expand(Path::new(case["request"]["path"].as_str().unwrap())).unwrap();
        let data=read_regular_bytes(&path,1024*1024).unwrap();
        assert_eq!(sha256(&data),case["request"]["sha256"].as_str().unwrap());
        let request:CadOccurrenceBindingRequest1=serde_json::from_value(oe_core::json::strict_json(&data).unwrap()).unwrap();
        let checked=occurrence::verify(&root,&request);
        match checked {
            Ok(checked)=>{
                actual.push(json!({"id":case["id"],"report":checked.report()}));
                for binding in checked.report()["bindings"].as_array().unwrap() {
                    let cap=checked.require_current_feature(binding["feature_id"].as_str().unwrap(),&request.assembly.object,
                        checked.report()["assembly"]["native_result_sha256"].as_str().unwrap(),
                        checked.report()["definition"]["native_result_sha256"].as_str().unwrap());
                    capabilities.push(json!({"case":case["id"],"binding_id":binding["binding_id"],"admitted":cap.is_ok()}));
                }
            },
            Err(error)=>actual.push(json!({"id":case["id"],"error":format!("{error:#}")})),
        }
    }
    println!("OE_OCCURRENCE_NATIVE_REPLAY={}",json!({"actual":actual,"capabilities":capabilities,
        "native_dispatch":0,"api_calls":0,"exposure":"development_replay","native_source":"historical source04 only; no new worker/platform execution qualification"}));
    assert_eq!(actual.len(),5);
    for (case,row) in manifest["cases"].as_array().unwrap().iter().zip(&actual) {
        let bindings=row["report"]["bindings"].as_array().unwrap_or_else(||panic!("case failed: {row}"));
        assert_eq!(bindings.len(),2);
        assert_eq!(row["report"]["engineering_adoption"],"not_established");
        for binding in bindings {
            let id=binding["binding_id"].as_str().unwrap();
            assert_eq!(binding["status"],case["expected_status"][id],"{} {id}",case["id"]);
            let capability=capabilities.iter().find(|c|c["case"]==case["id"] && c["binding_id"]==id).unwrap();
            assert_eq!(capability["admitted"]==true,binding["status"]=="bounded_instance_use");
            if let Some(center)=case["expected_root_centers_m"].get(id) {
                let observed=binding["derived_circle_root_m"]["center"].as_array().unwrap();
                for (a,b) in observed.iter().zip(center.as_array().unwrap()) {assert!((a.as_f64().unwrap()-b.as_f64().unwrap()).abs()<1e-12);}
                assert!((binding["derived_circle_root_m"]["radius"].as_f64().unwrap()-0.005).abs()<1e-12);
                assert_eq!(binding["native_occurrence_edge_observed"],false);
                assert_eq!(binding["parent_chain"].as_array().unwrap().len(),1);
            }
        }
    }
}
