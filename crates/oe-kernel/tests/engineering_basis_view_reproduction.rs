//! New goal relations bind declarations; they cannot certify the parent goal.
use oe_contracts::ObjectIdentity;
use oe_kernel::{engineering, project};
use serde_json::{Value, json};

fn q(value: f64, unit: &str) -> Value {
    json!({"value":value,"unit":unit,"precision":"independent synthetic"})
}

fn local_hash(basis: &Value) -> String {
    let mut local = basis.clone();
    local.as_object_mut().unwrap().remove("goal_support");
    oe_core::sha256(oe_wire::json::canonical_json(&local).unwrap())
}

fn set(project: &mut project::Project, basis: &Value) {
    project
        .conditions
        .insert("local".into(), serde_json::to_string(basis).unwrap());
}

fn fixture() -> (project::Project, ObjectIdentity, Value) {
    let local = ObjectIdentity {
        object_id: "urn:local".into(),
        revision: "r1".into(),
        condition_scope: "local".into(),
        source_sha256: "b".repeat(64),
    };
    let parent = ObjectIdentity {
        object_id: "urn:parent".into(),
        revision: "r1".into(),
        condition_scope: "parent".into(),
        source_sha256: "a".repeat(64),
    };
    let purpose = "complete the engineering design with bounded evidence";
    let parent_basis = json!({"schema":"oev2.engineering-basis/1","object_id":"urn:parent","object_revision":"r1","purpose":purpose,
        "acceptance":["provide manufacturing input evidence","retain final metrology unknown"],"facts":[],"assertions":[]});
    let parent_raw = serde_json::to_string(&parent_basis).unwrap();
    let mut project:project::Project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"goal-synthetic","revision":"r1","purpose":purpose,
        "objects":{"urn:local":"r1","urn:parent":"r1"},"conditions":{"parent":parent_raw,"local":"pending"},
        "evidence":{"parent-doc":"a".repeat(64),"child-doc":"b".repeat(64)},
        "decisions":[{"id":"local-review","object_id":"urn:local","object_revision":"r1","condition_scope":"local","source_sha256":"b".repeat(64),"conclusion":"unknown","adopted":false,"basis":["beam"]},
          {"id":"manufacture","object_id":"urn:parent","object_revision":"r1","condition_scope":"parent","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["local-review"]}],
        "dependencies":{"manufacture":["local-review"]}})).unwrap();
    let facts:Vec<Value> = [ ("force",q(20.0,"N")), ("span",q(100.0,"mm")), ("elastic_modulus",q(200000.0,"MPa")), ("second_moment",q(1000.0,"mm4")),
        ("support",json!("cantilever_fixed_free")),("load_position",json!("end")),("regime",json!("linear_elastic_small_deflection")) ]
        .into_iter().map(|(property,value)|json!({"property":property,"value":value,"status":"declared","source_refs":["child-doc"]})).chain([
            json!({"property":"holding","value":{"not_measured":"load capacity unknown"},"status":"unknown","source_refs":["child-doc"]})]).collect();
    let mut basis = json!({"schema":"oev2.engineering-basis/2","object_id":"urn:local","object_revision":"r1","purpose":"review nominal geometry while retaining unverified physical conditions",
        "acceptance":["review nominal geometry only","retain physical unknown"],"facts":facts,
        "assertions":[{"assertion_id":"beam","kind":"model_boundary","subject":"beam-model","requirements":{"method":"beam-cantilever-end-load/1",
            "inputs":{"force":q(20.0,"N"),"span":q(100.0,"mm"),"elastic_modulus":q(200000.0,"MPa"),"second_moment":q(1000.0,"mm4")}},"source_refs":["child-doc"],"affected_decisions":["local-review"]}]});
    basis["goal_support"] = json!({"schema":"oev2.engineering-goal-support/1","project_id":project.project_id,"project_revision":project.revision,"project_purpose":project.purpose,
        "local_object":local,"parent_object":parent,"parent_condition_sha256":oe_core::sha256(parent_raw.as_bytes()),"local_basis_sha256":local_hash(&basis),
        "sources":[{"source_ref":"parent-doc","source_sha256":"a".repeat(64)},{"source_ref":"child-doc","source_sha256":"b".repeat(64)}],
        "obligations":[{"obligation_id":"nominal","local_acceptance":"review nominal geometry only","parent_acceptance":"provide manufacturing input evidence","contribution":"Nominal geometry supplies a bounded design input; it does not prove manufacturing readiness.","assertion_ids":["beam"],"fact_properties":[]},
            {"obligation_id":"unknown","local_acceptance":"retain physical unknown","parent_acceptance":"retain final metrology unknown","contribution":"Keep unverified holding and metrology visible before later adoption.","assertion_ids":[],"fact_properties":["holding"]}],
        "affected_decisions":["local-review"],"claim_scope":"local_obligation_only"});
    set(&mut project, &basis);
    (project, local, basis)
}

#[test]
fn full_original_conditions_are_preserved_without_mislabeled_common_serialization() {
    let (mut project, object, original) = fixture();
    let original_text = format!("  \n{}\n\t", serde_json::to_string(&original).unwrap());
    project
        .conditions
        .insert("local".into(), original_text.clone());
    let view = engineering::basis(&project, &object).unwrap().unwrap();
    assert_eq!(view.protocol(), "oev2.engineering-basis/2");
    assert_eq!(view.schema, "oev2.engineering-basis/1");
    assert_eq!(
        view.original_condition().as_bytes(),
        original_text.as_bytes()
    );
    assert_eq!(
        view.condition_sha256(),
        oe_core::sha256(original_text.as_bytes())
    );
    assert_eq!(view.original_value(), &original);
    let cloned = view.clone();
    assert_eq!(cloned.original_condition(), view.original_condition());
    assert_eq!(
        cloned.original_value()["goal_support"],
        original["goal_support"]
    );
    let full: oe_contracts::EngineeringBasisV2 =
        serde_json::from_value(cloned.original_value().clone()).unwrap();
    full.validate().unwrap();
    let explicitly_common = serde_json::to_value(&*view).unwrap();
    assert_eq!(explicitly_common["schema"], "oev2.engineering-basis/1");
    assert!(explicitly_common.get("goal_support").is_none());
    let checks = engineering::checks(&project, &object);
    assert_eq!(
        checks
            .iter()
            .find(|c| c.id == "engineering_goal_support")
            .unwrap()
            .conclusion,
        "pass"
    );
    assert_eq!(project.conditions["local"], original_text);
}
#[test]
fn broken_support_keeps_its_full_source_and_does_not_suppress_model_diagnostics() {
    let (mut project, object, mut original) = fixture();
    original["goal_support"]["project_revision"] = json!("r0");
    set(&mut project, &original);
    let view = engineering::basis(&project, &object).unwrap().unwrap();
    assert_eq!(view.original_value(), &original);
    let checks = engineering::checks(&project, &object);
    assert_eq!(
        checks
            .iter()
            .find(|c| c.id == "engineering_goal_support")
            .unwrap()
            .conclusion,
        "violated"
    );
    assert_eq!(
        checks
            .iter()
            .find(|c| c.id == "engineering:beam")
            .unwrap()
            .conclusion,
        "pass"
    );
    original.as_object_mut().unwrap().remove("goal_support");
    set(&mut project, &original);
    let view = engineering::basis(&project, &object).unwrap().unwrap();
    assert_eq!(view.original_value(), &original);
    let checks = engineering::checks(&project, &object);
    assert_eq!(
        checks
            .iter()
            .find(|c| c.id == "engineering_goal_support")
            .unwrap()
            .conclusion,
        "unknown"
    );
    assert_eq!(
        checks
            .iter()
            .find(|c| c.id == "engineering:beam")
            .unwrap()
            .conclusion,
        "pass"
    );
    assert!(project.decisions.iter().all(|d| !d.adopted));
}
#[test]
fn legacy_basis_and_opaque_conditions_keep_their_original_meaning() {
    let (mut project, object, mut source) = fixture();
    source.as_object_mut().unwrap().remove("goal_support");
    source["schema"] = json!("oev2.engineering-basis/1");
    set(&mut project, &source);
    let view = engineering::basis(&project, &object).unwrap().unwrap();
    assert_eq!(view.protocol(), "oev2.engineering-basis/1");
    assert_eq!(view.original_value(), &source);
    assert_eq!(serde_json::to_value(&*view).unwrap(), source);
    assert_eq!(
        engineering::checks(&project, &object)[0].conclusion,
        "violated"
    );
    project.conditions.insert(
        "local".into(),
        "unparsed historical engineering prose".into(),
    );
    assert!(engineering::basis(&project, &object).unwrap().is_none());
}
