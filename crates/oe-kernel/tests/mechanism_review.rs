use oe_contracts::ObjectIdentity;
use oe_kernel::{engineering, project::Project};
use serde_json::{Value, json};
fn q(v: f64, u: &str) -> Value {
    json!({"value":v,"unit":u,"precision":"fixed development oracle"})
}
fn fixture() -> (Project, ObjectIdentity, Value) {
    let o:ObjectIdentity=serde_json::from_value(json!({"object_id":"drive","revision":"r1","condition_scope":"service","source_sha256":"a".repeat(64)})).unwrap();
    let phases = ["takeup", "threaded", "held_extract", "tool_release"];
    let samples=(0..4).map(|i|json!({"sample_id":format!("s{i}"),"phase":phases[i],"time":q(i as f64,"s"),
        "screw_angle":q(if i==0{0.}else{720.},"deg"),"driver_angle":q(if i==0{3.}else{723.},"deg"),
        "screw_travel":q([0.,1.,20.,20.][i],"mm"),"driver_travel":q([0.,1.,20.,25.][i],"mm"),"next_support":"present","screw_holder":"present"})).collect::<Vec<_>>();
    let drive = json!({"schema":"oev2.coupled-drive/1","object":o,"trajectory_id":"service-clock","coordinate_frame":"world-r1","time_basis":"fixed-oracle","time_status":"bound",
        "lead":q(0.5,"mm/rev"),"lead_status":"confirmed","takeup_angle":q(3.,"deg"),"maximum_travel":q(25.,"mm"),"linear_tolerance":q(1e-6,"mm"),"angular_tolerance":q(1e-6,"deg"),"samples":samples,"source_refs":["fixture"]});
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"drive","object_revision":"r1","purpose":"bounded service motion",
        "acceptance":["retain support and accumulated angles"],"facts":[{"property":"support_configuration","value":"r1-declared","status":"declared","source_refs":["fixture"]}],
        "assertions":[{"assertion_id":"coupling","kind":"mechanism_motion","subject":"service-clock","source_refs":["fixture"],"affected_decisions":["service"],
        "requirements":{"schema":"oev2.mechanism-assessment/1","drive":drive,"condition_values":{"support_configuration":"r1-declared"},"source_versions":{"fixture":"a".repeat(64)}}}]});
    let decisions=["service","drawings","inspection","cost"].map(|id|json!({"id":id,"object_id":"drive","object_revision":"r1","condition_scope":"service","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["fixture"]}));
    let p=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"drive-fixture","revision":"r1","purpose":basis["purpose"],"objects":{"drive":"r1"},"conditions":{"service":serde_json::to_string(&basis).unwrap()},"evidence":{"fixture":"a".repeat(64)},"decisions":decisions,"dependencies":{"drawings":["service"],"inspection":["drawings"],"cost":["inspection"]}})).unwrap();
    (p, o, basis)
}
fn save(p: &mut Project, b: &Value) {
    p.conditions
        .insert("service".into(), serde_json::to_string(b).unwrap());
}
#[test]
fn mechanism_reuses_current_project_conditions_sources_and_downstream_impact() {
    let (mut p, o, mut b) = fixture();
    let checks = engineering::checks(&p, &o);
    assert_eq!(checks[1].conclusion, "pass");
    assert_eq!(checks[1].result["engineering_adoption"], "not_established");
    b["facts"][0]["value"] = json!("r2-changed-support");
    save(&mut p, &b);
    let c = engineering::checks(&p, &o).pop().unwrap();
    assert_eq!(c.conclusion, "unknown");
    assert!(c.failures[0].contains("current_condition_changed:support_configuration"));
    assert_eq!(
        c.result["details"]["motion"]["conditional_geometry"],
        "consistent"
    );
    assert_eq!(
        c.result["details"]["affected_decisions"],
        json!(["cost", "drawings", "inspection", "service"])
    );
    let (mut p, o, _) = fixture();
    p.evidence.insert("fixture".into(), "b".repeat(64));
    assert!(engineering::checks(&p, &o)[1].failures[0].contains("source_stale"));
}
#[test]
fn mechanism_keeps_specific_failure_and_unknown_reasons() {
    let (mut p, o, mut b) = fixture();
    b["assertions"][0]["requirements"]["drive"]["samples"][1]["next_support"] = json!("absent");
    save(&mut p, &b);
    let c = engineering::checks(&p, &o).pop().unwrap();
    assert_eq!(c.conclusion, "violated");
    assert!(c.failures[0].contains("next_support_absent_before_loosen"));
    let (mut p, o, mut b) = fixture();
    b["assertions"][0]["requirements"]["drive"]["lead_status"] = json!("assumed");
    save(&mut p, &b);
    let c = engineering::checks(&p, &o).pop().unwrap();
    assert_eq!(c.conclusion, "unknown");
    assert!(c.failures[0].contains("real_lead_or_handedness_unconfirmed"));
}
