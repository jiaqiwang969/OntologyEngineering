use oe_contracts::DrawingReferenceIntent;
use oe_kernel::{drawing_intent, engineering, project::Project};
use serde_json::{Value, json};
fn fixture() -> (Project, DrawingReferenceIntent, Value) {
    let intent: DrawingReferenceIntent = serde_json::from_value(json!({"schema":"oev2.drawing-reference-intent/1","intent_id":"datum",
        "object":{"object_id":"coupon","revision":"b","condition_scope":"design","source_sha256":"a".repeat(64)},
        "local_frame":"part_absolute","unit":"mm","position_tolerance":0.001,"proof_scope":"reference_points",
        "points":[{"point_id":"datum","position":[20,0,0],"point_type_raw":1,"line_type_raw":0,"parent_type_raw":3,"parent_subtype_raw":0}],
        "source_refs":["independent-design"]})).unwrap();
    let observation = json!({"schema":"oev2.native-drawing-observation/1","object":intent.object,
        "native_result_ref":"native-receipt","native_result_sha256":"b".repeat(64),"part":{"annotations":{
            "schema":"oev2.nx-annotation-observation/2","units_query":{"return_code":0,"unit":"mm"},"dimensions":[{
                "retained":false,"retained_query_code":0,"associativity_parents":{"capture_complete":true,"wrapper_code":0,"uf_return_code":0,
                    "entries":[{"association_position_valid":true,"association_position":[20,0,0],"unit":"mm","coordinate_basis":"UF_DRF assoc_point absolute coordinates",
                        "point_type_raw":1,"line_type_raw":0,"object_1":{"present":true,"metadata":{"type_query":{"return_code":0,"type_raw":3,"subtype_raw":0},
                            "owning_part_query":{"value_valid":true,"matches_expected_owner":true}}}}]}}]}}});
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"coupon","object_revision":"b","purpose":"source-bound drawing obligation",
        "acceptance":["current reference points"],"facts":[
            {"property":"drawing_reference:datum","value":intent,"status":"declared","source_refs":["independent-design"]},
            {"property":"native_drawing:datum","value":observation,"status":"declared","source_refs":["native-receipt","cad"]}],
        "assertions":[{"assertion_id":"datum-obligation","kind":"cad_drawing_reference","subject":"datum","requirements":intent,
            "source_refs":["independent-design"],"affected_decisions":["drawing"]}]});
    let decisions = ["drawing","inspection","cost"].map(|id| json!({"id":id,"object_id":"coupon","object_revision":"b","condition_scope":"design",
        "source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["independent-design"]}));
    let project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"drawing-current","revision":"b","purpose":basis["purpose"],
        "objects":{"coupon":"b"},"conditions":{"design":serde_json::to_string(&basis).unwrap()},
        "evidence":{"cad":"a".repeat(64),"native-receipt":"b".repeat(64),"independent-design":"c".repeat(64)},
        "decisions":decisions,"dependencies":{"inspection":["drawing"],"cost":["inspection"]}})).unwrap();
    (project, intent, basis)
}
fn save(p: &mut Project, b: &Value) {
    p.conditions
        .insert("design".into(), serde_json::to_string(b).unwrap());
}
#[test]
fn changed_design_or_native_source_reopens_drawing_inspection_and_cost() {
    let (mut p, intent, mut b) = fixture();
    let old = drawing_intent::check(&p, &intent);
    assert_eq!(old.conclusion, "pass");
    b["facts"][0]["value"]["points"][0]["position"][0] = json!(25);
    save(&mut p, &b);
    let checks = engineering::checks(&p, &intent.object);
    let current = checks
        .iter()
        .find(|c| c.id == "engineering:datum-obligation")
        .unwrap();
    assert_eq!(current.conclusion, "violated");
    assert_eq!(
        current.failures,
        ["drawing_reference_design_content_changed"]
    );
    assert_eq!(
        current.result["details"]["affected_decisions"],
        json!(["cost", "drawing", "inspection"])
    );
    assert_eq!(
        old.conclusion, "pass",
        "historical result remains unchanged"
    );
    assert_eq!(current.result["engineering_adoption"], "not_established");
}
#[test]
fn missing_observation_source_changes_and_same_result_as_expectation_do_not_pass() {
    let (mut p, intent, mut b) = fixture();
    b["facts"].as_array_mut().unwrap().pop();
    save(&mut p, &b);
    assert_eq!(
        drawing_intent::check(&p, &intent).failures,
        ["drawing_reference_native_observation_missing"]
    );
    let (mut p, intent, _) = fixture();
    p.evidence.insert("native-receipt".into(), "d".repeat(64));
    assert_eq!(
        drawing_intent::check(&p, &intent).failures,
        ["drawing_reference_native_source_mismatch"]
    );
    let (mut p, intent, _) = fixture();
    p.evidence
        .insert("independent-design".into(), "b".repeat(64));
    assert_eq!(
        drawing_intent::check(&p, &intent).failures,
        ["drawing_reference_expectation_not_independent"]
    );
    let (mut p, intent, _) = fixture();
    p.objects.insert("coupon".into(), "c".into());
    assert_eq!(
        drawing_intent::check(&p, &intent).failures,
        ["drawing_reference_current_object_mismatch"]
    );
}
#[test]
fn kernel_recomputes_points_and_preserves_topology_unknown() {
    let (mut p, intent, mut b) = fixture();
    b["facts"][1]["value"]["part"]["annotations"]["dimensions"][0]["associativity_parents"]["entries"]
        [0]["association_position"][0] = json!(19);
    b["facts"][1]["value"]["part"]["claimed_pass"] = json!(true);
    save(&mut p, &b);
    assert_eq!(drawing_intent::check(&p, &intent).conclusion, "violated");
    let (mut p, mut intent, mut b) = fixture();
    intent.proof_scope = "unique_model_parent".into();
    b["facts"][0]["value"] = json!(intent);
    save(&mut p, &b);
    assert_eq!(
        drawing_intent::check(&p, &intent).failures,
        ["drawing_reference_unique_model_parent_unproven"]
    );
}

#[test]
fn missing_reference_requirements_name_unknown_and_downstream_obligations() {
    let (mut p, intent, mut b) = fixture();
    b["assertions"][0]["requirements"] = Value::Null;
    b["facts"][0]["value"] = Value::Null;
    b["facts"][0]["status"] = json!("unknown");
    save(&mut p, &b);
    let checks = engineering::checks(&p, &intent.object);
    let current = checks
        .iter()
        .find(|c| c.id == "engineering:datum-obligation")
        .unwrap();
    assert_eq!(current.execution, "completed");
    assert_eq!(current.conclusion, "unknown");
    assert_eq!(current.failures, ["drawing_reference_intent_missing"]);
    assert_eq!(
        current.result["details"]["affected_decisions"],
        json!(["cost", "drawing", "inspection"])
    );
    assert!(
        current.result["details"]["required_inputs"]
            .as_array()
            .unwrap()
            .contains(&json!("independent_design_reference_points"))
    );
    assert_eq!(current.result["engineering_adoption"], "not_established");
}

#[test]
fn current_checks_require_native_job_bytes_without_hiding_declared_geometry() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("SKILL.md"), "independent fixture").unwrap();
    let root = oe_core::SkillRoot::new(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let (mut p, intent, mut b) = fixture();
    assert_eq!(
        drawing_intent::check(&p, &intent).conclusion,
        "pass",
        "frozen /1 declared diagnostic keeps its scope"
    );
    let checks = engineering::checks_current(&root, &p, &intent.object);
    assert_eq!(checks[1].conclusion, "pass");
    let source = checks
        .iter()
        .find(|c| c.id == "engineering_native_source:datum-obligation")
        .unwrap();
    assert_eq!(source.conclusion, "unknown");
    assert_eq!(source.failures, ["drawing_native_current_locator_missing"]);
    assert_eq!(
        source.result["details"]["affected_decisions"],
        json!(["cost", "drawing", "inspection"])
    );
    b["facts"][1]["value"]["part"]["annotations"]["dimensions"][0]["associativity_parents"]["entries"]
        [0]["association_position"][0] = json!(19);
    save(&mut p, &b);
    let checks = engineering::checks_current(&root, &p, &intent.object);
    assert_eq!(
        checks[1].conclusion, "violated",
        "the original geometry counterexample still executes"
    );
    assert_eq!(
        checks[2].conclusion, "unknown",
        "a missing source is not a fabricated failure or pass"
    );
}

#[test]
fn current_native_locator_does_not_authorize_foreign_or_missing_receipts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("SKILL.md"), "independent fixture").unwrap();
    std::fs::create_dir(dir.path().join("var")).unwrap();
    std::fs::create_dir(dir.path().join("var/job")).unwrap();
    let root = oe_core::SkillRoot::new(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let (mut p, intent, mut b) = fixture();
    for job in ["outside-root/job", "var/job"] {
        b["facts"][1]["value"]["schema"] = json!("oev2.native-drawing-observation/2");
        b["facts"][1]["value"]["job_directory"] = json!(job);
        b["facts"][1]["value"]["part_input"] = json!("inputs/coupon.prt");
        save(&mut p, &b);
        let c = drawing_intent::source_check(&root, &p, &intent);
        assert_ne!(c.conclusion, "pass");
        assert_eq!(c.result["engineering_adoption"], "not_established");
        assert_eq!(drawing_intent::check(&p, &intent).conclusion, "pass");
    }
}
