use oe_contracts::ObjectIdentity;
use oe_kernel::{
    engineering,
    project::{self, Project},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[path = "common/relations_fixture.rs"]
mod fixture;

fn setup() -> (Project, ObjectIdentity, Value) {
    let o = ObjectIdentity {
        object_id: "assembly".into(),
        revision: "r1".into(),
        condition_scope: "loaded".into(),
        source_sha256: "a".repeat(64),
    };
    let b = fixture::basis(
        &o,
        "fixture",
        "positioning system",
        "route",
        "assembly alignment <= 0.03 mm",
    );
    let p = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"relations","revision":"p1","purpose":"positioning system",
        "objects":{"assembly":"r1"},"conditions":{"loaded":b.to_string()},"evidence":{"fixture":"a".repeat(64)},
        "decisions":[{"id":"route","object_id":"assembly","object_revision":"r1","condition_scope":"loaded","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["fixture"]}],"dependencies":{}})).unwrap();
    (p, o, b)
}
fn set(p: &mut Project, b: &Value) {
    p.conditions.insert("loaded".into(), b.to_string());
}
fn check(p: &Project, o: &ObjectIdentity, id: &str) -> oe_kernel::CheckResult {
    engineering::checks(p, o)
        .into_iter()
        .find(|c| c.id == format!("engineering:{id}"))
        .unwrap()
}

#[test]
fn typed_changes_keep_namespaces_separate() {
    let (mut p, _, _) = setup();
    p.evidence.insert("assembly".into(), "b".repeat(64));
    let mut other = p.decisions[0].clone();
    other.id = "other".into();
    other.basis = vec!["assembly".into()];
    other.source_sha256 = "b".repeat(64);
    other.object_id = "another".into();
    p.objects.insert("another".into(), "r1".into());
    p.decisions.push(other);
    let mut current = p.clone();
    current.evidence.insert("assembly".into(), "c".repeat(64));
    let r = project::reconcile(&p, &current).unwrap();
    assert_eq!(
        r["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["decision_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["other"]
    );
    let r = project::impact_typed(&p, &[project::entity("object", "assembly")]).unwrap();
    assert!(
        r["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["decision_id"] == "route")
    );
    let legacy = project::impact(&p, &BTreeSet::from(["assembly".into()])).unwrap();
    assert!(
        legacy["legacy_ambiguities"]
            .as_array()
            .unwrap()
            .contains(&json!("assembly"))
    );
    // A decision ID equal to a source ID must not become a source change.
    p.evidence.insert("route".into(), "d".repeat(64));
    let r = project::decision_impact(&p, &BTreeSet::from(["route".into()])).unwrap();
    assert_eq!(r["decisions"].as_array().unwrap().len(), 1);
}

#[test]
fn function_requires_current_structure_and_owned_checks() {
    let (mut p, o, mut b) = setup();
    let mut a = json!({"assertion_id":"allocation","kind":"function_continuity","subject":"position","source_refs":["fixture"],"affected_decisions":["route"],
        "requirements":{"function_id":"position","inventory_scope":"declared_complete","current_realization":"imaginary","mechanism":"text","verification_plan":"text"}});
    b["assertions"].as_array_mut().unwrap().push(a.clone());
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "allocation").conclusion, "unknown");
    a["requirements"] = json!({"schema":"oev2.function-realization/1","function_id":"position","inventory_scope":"declared_complete","state":"assigned",
        "realization":o,"mechanism_assertion":"mechanism","verification_assertions":["alignment"]});
    b["assertions"][4] = a.clone();
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "allocation").conclusion, "pass");
    for (field, value) in [
        ("state", json!("candidate")),
        ("verification_assertions", json!(["missing"])),
    ] {
        b["assertions"][4] = a.clone();
        b["assertions"][4]["requirements"][field] = value;
        set(&mut p, &b);
        assert_eq!(check(&p, &o, "allocation").conclusion, "unknown");
    }
    for (field, value) in [
        ("object_id", "imaginary"),
        ("revision", "old"),
        ("condition_scope", "unbound"),
    ] {
        b["assertions"][4] = a.clone();
        b["assertions"][4]["requirements"]["realization"][field] = json!(value);
        set(&mut p, &b);
        assert_eq!(check(&p, &o, "allocation").conclusion, "unknown");
    }
    b["assertions"][4] = a;
    b["assertions"][3]["requirements"]["observation"]["values"][0]["value"] = json!(0.04);
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "allocation").conclusion, "violated");
}

#[test]
fn system_claim_requires_interfaces_and_preserves_unknown() {
    let (mut p, o, mut b) = setup();
    let a = fixture::composition(&o, "fixture", "route", "assembly alignment <= 0.03 mm");
    b["assertions"].as_array_mut().unwrap().push(a.clone());
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "whole").conclusion, "pass");
    b["assertions"][4]["requirements"]["interfaces"] = json!([]);
    set(&mut p, &b);
    let c = check(&p, &o, "whole");
    assert_eq!(c.conclusion, "unknown");
    assert_eq!(
        c.result["details"]["missing_obligations"],
        json!(["system_interface_or_aggregate_check"])
    );
    b["assertions"][4] = a.clone();
    b["assertions"][3]["requirements"]["observation"]["values"][0]["value"] = json!(0.04);
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "part-a").conclusion, "pass");
    assert_eq!(check(&p, &o, "part-b").conclusion, "pass");
    assert_eq!(check(&p, &o, "whole").conclusion, "violated");
    b["assertions"][3]["requirements"]["observation"]["values"][0]["value"] = json!(0.03);
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "whole").conclusion, "unknown");
    // OR alternatives: failed + pass succeeds; failed + unknown stays unknown.
    b["assertions"][4]["requirements"]["all_of"] = json!([]);
    b["assertions"][4]["requirements"]["any_of"] =
        json!([{"alternatives":a["requirements"]["all_of"]}]);
    b["assertions"][3]["requirements"]["observation"]["values"][0]["value"] = json!(0.01);
    b["assertions"][1]["requirements"]["observation"]["values"][0]["value"] = json!(0.04);
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "whole").conclusion, "pass");
    b["assertions"][2]["requirements"]["observation"]["values"][0]["value"] = json!(0.03);
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "whole").conclusion, "unknown");
    b["assertions"][4] = a;
    b["assertions"][4]["requirements"]["criterion"] = json!("different goal");
    set(&mut p, &b);
    assert!(check(&p, &o, "whole").failures[0].contains("criterion_mismatch"));
}

#[test]
fn child_object_changes_reopen_composed_claim_without_manual_dependency() {
    let (mut p, o, mut b) = setup();
    let mut a = fixture::composition(&o, "fixture", "route", "assembly alignment <= 0.03 mm");
    a["requirements"]["all_of"][0]["object"]["object_id"] = json!("child");
    b["assertions"].as_array_mut().unwrap().push(a);
    set(&mut p, &b);
    p.objects.insert("child".into(), "r1".into());
    let mut current = p.clone();
    current.objects.insert("child".into(), "r2".into());
    let report = project::reconcile(&p, &current).unwrap();
    assert_eq!(report["decisions"][0]["decision_id"], "route");
    assert_eq!(check(&current, &o, "whole").conclusion, "unknown");
}

#[test]
fn changed_goal_cannot_reuse_passing_local_predicates() {
    let (mut p, o, mut b) = setup();
    b["assertions"]
        .as_array_mut()
        .unwrap()
        .push(fixture::composition(
            &o,
            "fixture",
            "route",
            "assembly alignment <= 0.03 mm",
        ));
    set(&mut p, &b);
    assert_eq!(check(&p, &o, "whole").conclusion, "pass");
    p.purpose = "a different operating purpose".into();
    assert_eq!(check(&p, &o, "part-a").conclusion, "pass");
    let c = check(&p, &o, "whole");
    assert_eq!(c.conclusion, "unknown");
    assert_eq!(
        c.result["details"]["context_check"]["conclusion"],
        "violated"
    );
}
