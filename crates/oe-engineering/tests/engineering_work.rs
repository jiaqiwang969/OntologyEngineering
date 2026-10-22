use oe_contracts::{
    EngineeringCalculation, EngineeringGap, EngineeringSourceRequest, ObjectIdentity,
};
use oe_engineering::workflow::{Reference, WorkInput, calculate, verify_project_binding};
use oe_kernel::project::Project;
use serde_json::json;
use std::collections::BTreeMap;
fn object() -> ObjectIdentity {
    ObjectIdentity {
        object_id: "guide".into(),
        revision: "r1".into(),
        condition_scope: "study".into(),
        source_sha256: "a".repeat(64),
    }
}
fn c(method: &str, inputs: serde_json::Value) -> EngineeringCalculation {
    EngineeringCalculation {
        calculation_id: "stiffness".into(),
        object: object(),
        method: method.into(),
        inputs,
        assumptions: vec!["model only".into()],
        validity_conditions: vec!["linear elastic small displacement".into()],
        invalid_when: vec!["unverified support or large displacement".into()],
        source_refs: vec!["mechanics-source".into()],
    }
}
fn q(value: f64, unit: &str) -> serde_json::Value {
    json!({"value":value,"unit":unit,"precision":"declared input"})
}
#[test]
fn current_model_comparison_preserves_arithmetic_and_blocks_stale_support() {
    let calc = c(
        "beam-cantilever-end-load/1",
        json!({"force":q(10.0,"N"),"span":q(100.0,"mm"),"elastic_modulus":q(200000.0,"MPa"),"second_moment":q(1000.0,"mm4")}),
    );
    let facts:Vec<_>=[("force",q(20.0,"N")),("span",q(100.0,"mm")),("elastic_modulus",q(200000.0,"MPa")),("second_moment",q(1000.0,"mm4")),("support",json!("cantilever_fixed_free")),("load_position",json!("end")),("regime",json!("linear_elastic_small_deflection"))].into_iter().map(|(property,value)|json!({"property":property,"value":value,"status":"declared","source_refs":["mechanics-source"]})).collect();
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"guide","object_revision":"r1","purpose":"guide","acceptance":["0.02mm"],"facts":facts,"assertions":[{"assertion_id":"beam-boundary","kind":"model_boundary","subject":"stiffness","requirements":{"method":calc.method,"inputs":calc.inputs},"source_refs":["mechanics-source"],"affected_decisions":[]}]});
    let p:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"model-current","revision":"r1","purpose":"guide","objects":{"guide":"r1"},"conditions":{"study":serde_json::to_string(&basis).unwrap()},"evidence":{"mechanics-source":"a".repeat(64)},"decisions":[],"dependencies":{}})).unwrap();
    let check = oe_kernel::engineering::calculation_check(&p, &calc);
    assert_eq!(check.conclusion, "violated");
    let diagnostic = calculate(&calc).unwrap();
    assert!((diagnostic["result"]["value"].as_f64().unwrap() - 1.0 / 60.0).abs() < 1e-12);
    // Independent corrected-load numeric reference: 20*100^3/(3*200000*1000).
    let mut corrected = calc.clone();
    corrected.inputs["force"] = q(20.0, "N");
    assert!(
        (calculate(&corrected).unwrap()["result"]["value"]
            .as_f64()
            .unwrap()
            - 1.0 / 30.0)
            .abs()
            < 1e-12
    );
    let missing_basis = Project {
        conditions: BTreeMap::from([("study".into(), "load20, unsupported free text".into())]),
        ..p
    };
    assert_eq!(
        oe_kernel::engineering::calculation_check(&missing_basis, &corrected).conclusion,
        "unknown"
    );
}
#[test]
fn independent_beam_error_and_missing_input_oracles_keep_units() {
    let calc = c(
        "beam-cantilever-end-load/1",
        json!({"force":q(10.0,"N"),"span":q(100.0,"mm"),"elastic_modulus":q(200000.0,"MPa"),"second_moment":q(1000.0,"mm4")}),
    );
    let result = calculate(&calc).unwrap();
    assert!((result["result"]["value"].as_f64().unwrap() - 1.0 / 60.0).abs() < 1e-12);
    assert_eq!(result["engineering_adoption"], "not_established");
    let mut unit = calc.clone();
    unit.inputs["span"]["unit"] = json!("m");
    assert!(
        calculate(&unit)
            .unwrap_err()
            .to_string()
            .contains("unit_mismatch")
    );
    let mut absent = calc;
    absent.inputs["force"] = serde_json::Value::Null;
    assert_eq!(calculate(&absent).unwrap()["execution"], "not_run");
    let budget = c(
        "linear-error-worst-case/1",
        json!({"components":[q(0.001,"mm"),q(0.002,"mm")]}),
    );
    assert!(
        (calculate(&budget).unwrap()["result"]["value"]
            .as_f64()
            .unwrap()
            - 0.003)
            .abs()
            < 1e-12
    );
    let mut negative = budget;
    negative.inputs["components"][0] = q(-0.001, "mm");
    assert!(calculate(&negative).is_err());
    let mut blank = negative;
    blank.assumptions = vec![" ".into()];
    assert!(
        calculate(&blank)
            .unwrap_err()
            .to_string()
            .contains("boundary_missing")
    );
}

#[test]
fn model_context_receives_the_real_excerpt_window_and_keeps_missing_text_partial() {
    let page = json!({"id":"book:page:1","source_sha256":"a".repeat(64),"excerpt":{"text":"beam boundary and support conditions","start":17,"end":53},"native_text_missing":false});
    let summary = oe_engineering::workflow::source_summary(&page);
    assert_eq!(summary["excerpt"], "beam boundary and support conditions");
    assert_eq!(summary["text_available"], true);
    assert_eq!(
        oe_engineering::workflow::text_coverage(&[page]),
        "source_candidates"
    );
    let missing = json!({"id":"book:page:2","excerpt":{"text":"","start":0,"end":0},"native_text_missing":true,"navigation_only":true});
    assert_eq!(
        oe_engineering::workflow::source_summary(&missing)["text_available"],
        false
    );
    assert_eq!(
        oe_engineering::workflow::text_coverage(&[missing]),
        "partial_text_coverage"
    );
}
#[test]
fn a_revised_object_cannot_reuse_the_previous_work_binding() {
    let mut p = Project {
        schema: "oev2.project/1".into(),
        project_id: "guide-project".into(),
        revision: "p1".into(),
        purpose: "guide repeatability study".into(),
        objects: BTreeMap::from([("guide".into(), "r1".into())]),
        conditions: BTreeMap::from([("study".into(), "assumption study".into())]),
        evidence: BTreeMap::from([("mechanics-source".into(), "a".repeat(64))]),
        decisions: vec![],
        dependencies: BTreeMap::new(),
    };
    let work = WorkInput {
        project_context: None,
        schema: "oev2.engineering-work/1".into(),
        project: Reference {
            path: "var/projects/guide/project.json".into(),
            sha256: "b".repeat(64),
        },
        object: object(),
        acceptance: vec!["declared model boundary".into()],
        necessary_functions: vec!["guide translation".into()],
        physical_mechanisms: vec!["support stiffness".into()],
        gaps: vec![],
        source_requests: vec![],
        calculations: vec![],
        handoff: None,
    };
    verify_project_binding(&p, &work).unwrap();
    let mut current = work.clone();
    current.schema = "oev2.engineering-work/2".into();
    current.project_context = Some(oe_kernel::project::context(&p, &current.object).unwrap());
    verify_project_binding(&p, &current).unwrap();
    let mut changed_condition = p.clone();
    changed_condition
        .conditions
        .insert("study".into(), "changed load; same named scope".into());
    // Updating the file reference alone must not bless stale calculation conditions.
    current.project.sha256 = "c".repeat(64);
    assert!(
        verify_project_binding(&changed_condition, &current)
            .unwrap_err()
            .to_string()
            .contains("current_project_context_changed_or_missing")
    );
    let mut mislabeled = work.clone();
    mislabeled.gaps = vec![EngineeringGap {
        gap_id: "stiffness".into(),
        question: "deflection?".into(),
        necessary_function: "guide".into(),
        physical_mechanism: "bending".into(),
        required_conditions: vec!["supports".into()],
        needed_evidence: vec!["measurement".into()],
    }];
    mislabeled.source_requests = vec![EngineeringSourceRequest {
        gap_id: "stiffness".into(),
        source_role: "strength".into(),
        provider: "local-pdf".into(),
        document_id: "norton".into(),
        query: "deflection".into(),
        maximum_pages: 1,
    }];
    assert!(
        verify_project_binding(&p, &mislabeled)
            .unwrap_err()
            .to_string()
            .contains("source_role_mismatch")
    );
    mislabeled.source_requests[0].document_id = "roloff-matek".into();
    verify_project_binding(&p, &mislabeled).unwrap();
    p.objects.insert("guide".into(), "r2".into());
    assert!(
        verify_project_binding(&p, &work)
            .unwrap_err()
            .to_string()
            .contains("binding_mismatch")
    );
}
