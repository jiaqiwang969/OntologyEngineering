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

fn goal<'a>(checks: &'a [oe_kernel::CheckResult]) -> &'a oe_kernel::CheckResult {
    checks
        .iter()
        .find(|c| c.id == "engineering_goal_support")
        .expect("versioned goal check")
}

#[test]
fn legacy_purpose_check_remains_exact_and_goal_relations_do_not_certify_completion() {
    let (mut project, object, mut basis) = fixture();
    let checks = engineering::checks(&project, &object);
    let relation = goal(&checks);
    assert_eq!(relation.conclusion, "pass");
    assert_eq!(
        relation.result["details"]["total_goal_achievement"],
        "not_established"
    );
    assert_eq!(relation.result["engineering_adoption"], "not_established");
    assert_eq!(
        relation.result["details"]["remaining_parent_obligations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(project.decisions.iter().filter(|d| d.adopted).count(), 0);
    basis.as_object_mut().unwrap().remove("goal_support");
    basis["schema"] = json!("oev2.engineering-basis/1");
    set(&mut project, &basis);
    let legacy = engineering::checks(&project, &object);
    assert_eq!(legacy[0].id, "engineering_purpose");
    assert_eq!(legacy[0].conclusion, "violated");
    assert!(
        legacy[0]
            .failures
            .iter()
            .any(|v| v == "engineering_purpose_changed")
    );
    basis["purpose"] = json!(project.purpose);
    set(&mut project, &basis);
    assert_eq!(engineering::checks(&project, &object)[0].conclusion, "pass");
}

#[test]
fn goal_support_binds_current_parent_local_body_and_sources() {
    let (project, object, basis) = fixture();
    for (field, value, reason) in [
        (
            "project_purpose",
            json!("other goal"),
            "engineering_goal_project_changed",
        ),
        (
            "project_revision",
            json!("r0"),
            "engineering_goal_project_changed",
        ),
        (
            "local_basis_sha256",
            json!("0".repeat(64)),
            "engineering_goal_local_basis_changed",
        ),
        (
            "parent_condition_sha256",
            json!("0".repeat(64)),
            "engineering_goal_parent_condition_changed",
        ),
        (
            "claim_scope",
            json!("engineering_adoption"),
            "engineering_goal_scope_broadened",
        ),
    ] {
        let mut p = project.clone();
        let mut b = basis.clone();
        b["goal_support"][field] = value;
        set(&mut p, &b);
        let checks = engineering::checks(&p, &object);
        let relation = goal(&checks);
        assert_eq!(relation.conclusion, "violated", "{field}");
        assert!(
            relation.failures.iter().any(|v| v == reason),
            "{field}: {:?}",
            relation.failures
        );
        assert!(
            relation.result["details"]["affected_decisions"]
                .as_array()
                .unwrap()
                .contains(&json!("manufacture"))
        );
    }
    let mut p = project.clone();
    let mut b = basis.clone();
    b["facts"][0]["value"] = q(30.0, "N");
    set(&mut p, &b);
    assert!(
        goal(&engineering::checks(&p, &object))
            .failures
            .contains(&"engineering_goal_local_basis_changed".into())
    );
    let mut p = project.clone();
    let mut b = basis.clone();
    b["goal_support"]["sources"][0]["source_sha256"] = json!("c".repeat(64));
    set(&mut p, &b);
    assert!(
        goal(&engineering::checks(&p, &object))
            .failures
            .contains(&"engineering_goal_source_changed".into())
    );
    let mut p = project.clone();
    let mut b = basis.clone();
    b["goal_support"]["obligations"][0]["parent_acceptance"] = json!("invented waiver");
    set(&mut p, &b);
    assert!(
        goal(&engineering::checks(&p, &object))
            .failures
            .contains(&"engineering_goal_acceptance_changed".into())
    );
    let mut p = project.clone();
    let mut b = basis.clone();
    b["goal_support"]["obligations"][0]["assertion_ids"] = json!(["invented"]);
    set(&mut p, &b);
    assert!(
        goal(&engineering::checks(&p, &object))
            .failures
            .contains(&"engineering_goal_reference_unbound".into())
    );
}

#[test]
fn goal_support_preserves_unknowns_and_all_original_checks() {
    let (mut parent_project, parent_object, mut parent_child) = fixture();
    let mut parent: Value = serde_json::from_str(&parent_project.conditions["parent"]).unwrap();
    parent["schema"] = json!("oev2.engineering-basis/2");
    let parent_raw = serde_json::to_string(&parent).unwrap();
    parent_child["goal_support"]["parent_condition_sha256"] =
        json!(oe_core::sha256(parent_raw.as_bytes()));
    parent_project
        .conditions
        .insert("parent".into(), parent_raw);
    set(&mut parent_project, &parent_child);
    let unsupported = engineering::checks(&parent_project, &parent_object);
    assert_eq!(goal(&unsupported).conclusion, "unknown");
    assert!(
        goal(&unsupported)
            .failures
            .contains(&"engineering_goal_parent_version_unsupported".into())
    );
    let (mut project, object, mut basis) = fixture();
    let checks = engineering::checks(&project, &object);
    assert!(
        goal(&checks).result["details"]["unresolved_local_facts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["property"] == "holding"
                && v["value"]["not_measured"] == "load capacity unknown")
    );
    basis["facts"][0]["value"] = q(30.0, "N");
    basis["goal_support"]["local_basis_sha256"] = json!(local_hash(&basis));
    set(&mut project, &basis);
    let checks = engineering::checks(&project, &object);
    assert_eq!(goal(&checks).conclusion, "pass");
    assert!(checks.iter().any(|c| {
        c.conclusion == "violated"
            && c.failures
                .contains(&"engineering_model_condition_mismatch".into())
    }));
    basis.as_object_mut().unwrap().remove("goal_support");
    set(&mut project, &basis);
    let checks = engineering::checks(&project, &object);
    assert_eq!(goal(&checks).conclusion, "unknown");
    assert!(
        goal(&checks)
            .failures
            .contains(&"engineering_goal_support_missing".into())
    );
    assert!(checks.iter().any(|c| {
        c.failures
            .contains(&"engineering_model_condition_mismatch".into())
    }));
    basis["goal_support"] = json!({"schema":"oev2.engineering-goal-support/1","claim_scope":"local_obligation_only","obligations":"wrong type"});
    set(&mut project, &basis);
    let checks = engineering::checks(&project, &object);
    assert_eq!(goal(&checks).conclusion, "unknown");
    assert!(checks.iter().any(|c| {
        c.failures
            .contains(&"engineering_model_condition_mismatch".into())
    }));
}

#[test]
fn empty_goal_fields_remain_unknown_without_masking_current_conflicts() {
    let (project, object, mut basis) = fixture();
    basis["facts"][0]["value"] = q(30.0, "N");
    basis["assertions"].as_array_mut().unwrap().push(json!({"assertion_id":"open-support-counterexample","kind":"counterexample","subject":"unverified-physical-support","requirements":{"status":"open","evidence_scope":"physical_support_unknown"},"source_refs":["child-doc"],"affected_decisions":["local-review"]}));
    basis["goal_support"]["local_basis_sha256"] = json!(local_hash(&basis));
    // Expected classifications were frozen before the narrow production patch.
    // These are development regressions; no physical input or adoption is added.
    let cases:Value=serde_json::from_str(r#"[{"case_id":"local-empty","pointer":"/goal_support/local_basis_sha256","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"local-nonhex64","pointer":"/goal_support/local_basis_sha256","operation":"replace","value":"gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"local-short","pointer":"/goal_support/local_basis_sha256","operation":"replace","value":"000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"local-uppercase","pointer":"/goal_support/local_basis_sha256","operation":"replace","value":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"local-missing","pointer":"/goal_support/local_basis_sha256","operation":"remove","value":null,"expected_conclusion":"unknown","basis":"Missing required digest leaves support unknown; original checks still execute."},{"case_id":"local-wrong-valid64","pointer":"/goal_support/local_basis_sha256","operation":"replace","value":"0000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"violated","basis":"Well-formed lowercase64hex identity differs from current content and must be rejected."},{"case_id":"parent-empty","pointer":"/goal_support/parent_condition_sha256","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"parent-nonhex64","pointer":"/goal_support/parent_condition_sha256","operation":"replace","value":"gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"parent-short","pointer":"/goal_support/parent_condition_sha256","operation":"replace","value":"000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"parent-uppercase","pointer":"/goal_support/parent_condition_sha256","operation":"replace","value":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","expected_conclusion":"unknown","basis":"Required canonical digest identity is unavailable or malformed; do not compare an invalid digest as a changed bound input."},{"case_id":"parent-missing","pointer":"/goal_support/parent_condition_sha256","operation":"remove","value":null,"expected_conclusion":"unknown","basis":"Missing required digest leaves support unknown; original checks still execute."},{"case_id":"parent-wrong-valid64","pointer":"/goal_support/parent_condition_sha256","operation":"replace","value":"0000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"violated","basis":"Well-formed lowercase64hex identity differs from current content and must be rejected."},{"case_id":"source-empty","pointer":"/goal_support/sources/0/source_sha256","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Missing or malformed source identity is not a comparable current-source change."},{"case_id":"source-nonhex64","pointer":"/goal_support/sources/0/source_sha256","operation":"replace","value":"gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg","expected_conclusion":"unknown","basis":"Missing or malformed source identity is not a comparable current-source change."},{"case_id":"source-short","pointer":"/goal_support/sources/0/source_sha256","operation":"replace","value":"000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"unknown","basis":"Missing or malformed source identity is not a comparable current-source change."},{"case_id":"source-uppercase","pointer":"/goal_support/sources/0/source_sha256","operation":"replace","value":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","expected_conclusion":"unknown","basis":"Missing or malformed source identity is not a comparable current-source change."},{"case_id":"source-wrong-valid64","pointer":"/goal_support/sources/0/source_sha256","operation":"replace","value":"0000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"violated","basis":"Valid source identity disagrees with current Project evidence declaration."},{"case_id":"scope-empty","pointer":"/goal_support/claim_scope","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Empty required claim scope is missing information, not an explicit adoption claim."},{"case_id":"scope-missing","pointer":"/goal_support/claim_scope","operation":"remove","value":null,"expected_conclusion":"unknown","basis":"Missing scope cannot establish the required local-only relation."},{"case_id":"scope-expanded","pointer":"/goal_support/claim_scope","operation":"replace","value":"engineering_adoption","expected_conclusion":"violated","basis":"Explicit broader adoption scope cannot be satisfied by structural goal-support consistency."},{"case_id":"local-acceptance-empty","pointer":"/goal_support/obligations/0/local_acceptance","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-acceptance-empty","pointer":"/goal_support/obligations/0/parent_acceptance","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"project-purpose-empty","pointer":"/goal_support/project_purpose","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-revision-empty","pointer":"/goal_support/local_object/revision","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"project-id-empty","pointer":"/goal_support/project_id","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"project-revision-empty","pointer":"/goal_support/project_revision","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-id-empty","pointer":"/goal_support/local_object/object_id","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-scope-empty","pointer":"/goal_support/local_object/condition_scope","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-source-empty","pointer":"/goal_support/local_object/source_sha256","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-id-empty","pointer":"/goal_support/parent_object/object_id","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-revision-empty","pointer":"/goal_support/parent_object/revision","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-scope-empty","pointer":"/goal_support/parent_object/condition_scope","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-source-empty","pointer":"/goal_support/parent_object/source_sha256","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"source-ref-empty","pointer":"/goal_support/sources/0/source_ref","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"contribution-empty","pointer":"/goal_support/obligations/0/contribution","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"obligation-id-empty","pointer":"/goal_support/obligations/0/obligation_id","operation":"replace","value":"","expected_conclusion":"unknown","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"project-id-valid-conflict","pointer":"/goal_support/project_id","operation":"replace","value":"another-project","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"project-revision-valid-conflict","pointer":"/goal_support/project_revision","operation":"replace","value":"another-revision","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"project-purpose-valid-conflict","pointer":"/goal_support/project_purpose","operation":"replace","value":"another-purpose","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-id-valid-conflict","pointer":"/goal_support/local_object/object_id","operation":"replace","value":"another-object","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-revision-valid-conflict","pointer":"/goal_support/local_object/revision","operation":"replace","value":"another-revision","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-scope-valid-conflict","pointer":"/goal_support/local_object/condition_scope","operation":"replace","value":"another-condition","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"local-object-source-valid-conflict","pointer":"/goal_support/local_object/source_sha256","operation":"replace","value":"0000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-id-valid-conflict","pointer":"/goal_support/parent_object/object_id","operation":"replace","value":"another-object","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-revision-valid-conflict","pointer":"/goal_support/parent_object/revision","operation":"replace","value":"another-revision","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-scope-valid-conflict","pointer":"/goal_support/parent_object/condition_scope","operation":"replace","value":"another-condition","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-object-source-valid-conflict","pointer":"/goal_support/parent_object/source_sha256","operation":"replace","value":"0000000000000000000000000000000000000000000000000000000000000000","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"source-ref-valid-conflict","pointer":"/goal_support/sources/0/source_ref","operation":"replace","value":"unbound-source","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."},{"case_id":"parent-acceptance-valid-conflict","pointer":"/goal_support/obligations/0/parent_acceptance","operation":"replace","value":"invented waiver","expected_conclusion":"violated","basis":"Required unavailable value remains unknown; valid nonempty current conflict remains violated."}]"#).unwrap();
    for case in cases.as_array().unwrap() {
        let mut p = project.clone();
        let mut b = basis.clone();
        let pointer = case["pointer"].as_str().unwrap();
        if case["operation"] == "remove" {
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            b.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
        } else {
            *b.pointer_mut(pointer).unwrap() = case["value"].clone();
        }
        set(&mut p, &b);
        let checks = engineering::checks(&p, &object);
        let relation = goal(&checks);
        assert_eq!(
            relation.conclusion,
            case["expected_conclusion"].as_str().unwrap(),
            "{}: {:?}",
            case["case_id"],
            relation.failures
        );
        assert!(
            checks.iter().any(|c| c
                .failures
                .contains(&"engineering_open_counterexample".into())),
            "{} lost original open counterexample",
            case["case_id"]
        );
        assert!(
            checks.iter().any(|c| c
                .failures
                .contains(&"engineering_model_condition_mismatch".into())),
            "{} lost original model mismatch",
            case["case_id"]
        );
        assert_eq!(
            relation.result["details"]["total_goal_achievement"],
            "not_established"
        );
    }
}
