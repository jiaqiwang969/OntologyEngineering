//! Bounded system regressions from the frozen engineering-method requirements.
use oe_contracts::ObjectIdentity;
use oe_kernel::{engineering, project};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn q(value: f64, unit: &str) -> Value {
    json!({"value":value,"unit":unit,"precision":"declared synthetic"})
}
fn fixture() -> (project::Project, ObjectIdentity, Value) {
    let object = ObjectIdentity {
        object_id: "urn:guide".into(),
        revision: "r1".into(),
        condition_scope: "service".into(),
        source_sha256: "a".repeat(64),
    };
    let decisions=["route","inspection","cost","delivery","independent"].map(|id|json!({"id":id,"object_id":"urn:guide","object_revision":"r1","condition_scope":"service","source_sha256":"a".repeat(64),"conclusion":"supported","adopted":true,"basis":[if id=="independent"{"unrelated"}else{"old-model"}]}));
    let mut project:project::Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"engineering-synthetic","revision":"r1","purpose":"limit guide deflection",
        "objects":{"urn:guide":"r1"},"conditions":{"service":"original"},"evidence":{"drawing":"a".repeat(64),"experiment":"b".repeat(64)},
        "decisions":decisions,
        "dependencies":{"route":["claim:seal"],"inspection":["route"],"cost":["inspection"],"delivery":["cost"]}})).unwrap();
    let facts:Vec<_>=[("force",q(20.0,"N")),("span",q(100.0,"mm")),("elastic_modulus",q(200000.0,"MPa")),("second_moment",q(1000.0,"mm4")),
        ("support",json!("cantilever_fixed_free")),("load_position",json!("end")),("regime",json!("linear_elastic_small_deflection")),("length",q(2180.0,"mm")),("medium",json!("glycol25"))]
        .into_iter().map(|(property,value)|json!({"property":property,"value":value,"status":"declared","source_refs":["drawing"]})).collect();
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"urn:guide","object_revision":"r1","purpose":project.purpose,"acceptance":["maximum deflection 0.02 mm"],"facts":facts,"assertions":[{
        "assertion_id":"beam-boundary","kind":"model_boundary","subject":"beam","requirements":{"method":"beam-cantilever-end-load/1","inputs":{"force":q(10.0,"N"),"span":q(100.0,"mm"),"elastic_modulus":q(200000.0,"MPa"),"second_moment":q(1000.0,"mm4")}},"source_refs":["drawing"],"affected_decisions":["route"]}]});
    set(&mut project, &basis);
    (project, object, basis)
}
fn set(p: &mut project::Project, b: &Value) {
    p.conditions
        .insert("service".into(), serde_json::to_string(b).unwrap());
}
fn assertion(kind: &str, subject: &str, requirements: Value) -> Value {
    json!({"assertion_id":"new-obligation","kind":kind,"subject":subject,"requirements":requirements,"source_refs":["experiment"],"affected_decisions":[]})
}

#[test]
fn engineering_counterexamples_preserve_reasons_and_downstream_scope() {
    let (mut p, o, mut b) = fixture();
    let checks = engineering::checks(&p, &o);
    assert_eq!(checks[1].conclusion, "violated");
    assert_eq!(
        checks[1].result["details"]["mismatches"][0]["property"],
        "force"
    );
    assert!(
        checks[1]
            .failures
            .iter()
            .any(|s| s == "engineering_model_condition_mismatch")
    );
    // Re-signing content identities does not synchronize a physical premise.
    let current = project::context(&p, &o).unwrap();
    assert_eq!(
        project::check_current_context(&p, &o, Some(&current)).conclusion,
        "pass"
    );
    b["assertions"][0]["requirements"]["inputs"]["force"] = q(20.0, "N");
    b["facts"][4]["value"] = json!("simple_two_end");
    set(&mut p, &b);
    let support = engineering::checks(&p, &o);
    assert!(
        support[1].result["details"]["mismatches"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["property"] == "support")
    );
    b["assertions"] = json!([assertion(
        "scope_transfer",
        "claim:seal",
        json!({"limits":{"length":{"minimum":0.0,"maximum":100.0,"unit":"mm"}}})
    )]);
    set(&mut p, &b);
    let scope = engineering::checks(&p, &o);
    assert_eq!(scope[1].conclusion, "violated");
    assert_eq!(
        scope[1].failures,
        vec!["engineering_evidence_scope_exceeded"]
    );
    b["assertions"] = json!([assertion(
        "function_continuity",
        "claim:seal",
        json!({"function_id":"fluid_sealing","inventory_scope":"declared_complete","current_realization":null,"mechanism":null,"verification_plan":"leak and pressure testing at approved conditions"})
    )]);
    set(&mut p, &b);
    let function = engineering::checks(&p, &o);
    assert_eq!(function[1].conclusion, "violated");
    assert_eq!(
        function[1].result["details"]["mismatches"][0]["function_id"],
        "fluid_sealing"
    );
    b["assertions"] = json!([assertion(
        "source_applicability",
        "claim:seal",
        json!({"limits":{"medium":["air"]}})
    )]);
    set(&mut p, &b);
    let source = engineering::checks(&p, &o);
    assert_eq!(source[1].conclusion, "violated");
    assert_eq!(
        source[1].result["details"]["mismatches"][0]["property"],
        "medium"
    );
    b["assertions"] = json!([assertion(
        "counterexample",
        "claim:seal",
        json!({"status":"open","evidence_scope":"model_only"})
    )]);
    set(&mut p, &b);
    let challenge = engineering::checks(&p, &o);
    assert_eq!(challenge[1].conclusion, "violated");
    let affected: BTreeSet<_> = challenge[1].result["details"]["affected_decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        affected,
        BTreeSet::from(["route", "inspection", "cost", "delivery"])
    );
    assert_eq!(
        challenge[1].result["details"]["mismatches"][0]["physical_failure"],
        "not_inferred"
    );
    p.purpose = "production release with new acceptance".into();
    let purpose = engineering::checks(&p, &o);
    assert_eq!(purpose[0].conclusion, "violated");
}

#[test]
fn unknown_assumptions_units_and_unbound_sources_never_pass() {
    let (mut p, o, mut b) = fixture();
    b["assertions"][0]["requirements"]["inputs"]["force"] = q(20.0, "N");
    set(&mut p, &b);
    assert_eq!(engineering::checks(&p, &o)[1].conclusion, "pass");
    b["facts"][0]["status"] = json!("assumed");
    set(&mut p, &b);
    assert_eq!(engineering::checks(&p, &o)[1].conclusion, "unknown");
    b["facts"][0]["status"] = json!("declared");
    b["facts"][0]["value"] = q(20.0, "kN");
    set(&mut p, &b);
    assert_eq!(engineering::checks(&p, &o)[1].conclusion, "violated");
    b["facts"][0]["source_refs"] = json!(["invented-source"]);
    set(&mut p, &b);
    assert_eq!(engineering::checks(&p, &o)[0].conclusion, "unknown");
    // A closed challenge cannot be obtained solely by rewriting its state.
    let (mut p, o, mut b) = fixture();
    b["assertions"] = json!([assertion(
        "counterexample",
        "claim:seal",
        json!({"status":"resolved"})
    )]);
    set(&mut p, &b);
    assert_eq!(engineering::checks(&p, &o)[1].conclusion, "unknown");
}

#[test]
fn purpose_only_change_reopens_existing_decisions_without_manual_edges() {
    let (before, _, _) = fixture();
    let mut after = before.clone();
    after.purpose = "new acceptance and release goal".into();
    let report = project::reconcile(&before, &after).unwrap();
    assert_eq!(report["stale_adoptions"].as_array().unwrap().len(), 5);
    assert!(
        report["affected"]
            .as_array()
            .unwrap()
            .contains(&json!("project:purpose"))
    );
    assert_eq!(before.purpose, "limit guide deflection");
}

#[test]
fn explicit_decision_seed_reopens_downstream_without_a_claim_edge() {
    let (mut p, o, mut b) = fixture();
    let mut a = assertion(
        "counterexample",
        "unlisted-original-claim",
        json!({"status":"open","evidence_scope":"model_only"}),
    );
    a["affected_decisions"] = json!(["route"]);
    b["assertions"] = json!([a]);
    set(&mut p, &b);
    let checks = engineering::checks(&p, &o);
    assert_eq!(checks[1].conclusion, "violated");
    let affected: BTreeSet<_> = checks[1].result["details"]["affected_decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        affected,
        BTreeSet::from(["route", "inspection", "cost", "delivery"])
    );
    assert_eq!(
        checks[1].result["details"]["mismatches"][0]["physical_failure"],
        "not_inferred"
    );
}

#[test]
fn current_purpose_check_and_reconciliation_share_the_declared_scope() {
    let (mut before, o, _) = fixture();
    before
        .dependencies
        .insert("route".into(), BTreeSet::from(["purpose".into()]));
    let mut current = before.clone();
    current.purpose = "new bounded acceptance goal".into();
    let checks = engineering::checks(&current, &o);
    assert_eq!(checks[0].conclusion, "violated");
    let affected: BTreeSet<_> = checks[0].result["details"]["affected_decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        affected,
        BTreeSet::from(["route", "inspection", "cost", "delivery"])
    );
    let reconciled = project::reconcile(&before, &current).unwrap();
    let downstream: BTreeSet<_> = reconciled["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["decision_id"].as_str().unwrap())
        .collect();
    assert_eq!(affected, downstream);
    let unchanged = engineering::checks(&before, &o);
    assert_eq!(unchanged[0].conclusion, "pass");
    assert!(
        unchanged[0].result["details"]["affected_decisions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
