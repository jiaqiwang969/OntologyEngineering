use oe_contracts::NativeDraftingPlan;
use oe_kernel::{drafting, engineering, project::Project};
use serde_json::{Value, json};

// This is a synthetic raw-query DTO for current-Project tests. Actual NX
// execution, saved/reopened bytes and visual review have separate evidence.
fn observed_part(plan: &NativeDraftingPlan) -> Value {
    let metadata = |name: &str| {
        json!({"type_query":{"return_code":0},
        "owning_part_query":{"return_code":0,"value_valid":true,"matches_expected_owner":true},
        "name_query":{"return_code":0,"value_valid":true,"utf8":name}})
    };
    let rotation = json!({"return_code":0,"matrix":[1,0,0,0,1,0,0,0,1]});
    json!({"path":"C:/controlled/coupon.prt","units":"mm","solid_count":1,
        "drafting_readback_schema":"oev2.native-drafting-observation/1","drawing_catalog_complete":true,
        "drawing_sheet_count":1,"drawing_sheets":[{"drawing_tag":40,"metadata":metadata(&plan.sheet_name),
            "sheet_info":{"wrapper_code":0,"query_code":0,"value_valid":true,"unit":plan.unit,"size_state":"custom",
                "width":plan.width,"height":plan.height,"scale":0.5,"projection_angle":plan.projection_angle},
            "view_catalog_complete":true,"views":[{"view_tag":41,"metadata":metadata("Top@1"),
                "stale_query_code":0,"stale_after":false,"view_type":{"return_code":0,"kind":"drawing_member","subtype":"imported"},
                "owning_drawing":{"return_code":0,"drawing_tag":40},"model_part":{"return_code":0,"path":"C:/controlled/coupon.prt"},
                "rotation":rotation,"scale":{"return_code":0,"value":0.5},"reference_point":{"return_code":0,"point":plan.view_center},
                "borders":{"return_code":0,"values":[100,60,195,140]},
                "visible_objects":{"return_code":0,"capture_complete":true,"visible_count":4,"clipped_count":0}}]}],
        "modeling_view_catalog_complete":true,"modeling_views":[{"view_tag":31,"metadata":metadata(&plan.model_view_name),
            "view_type":{"return_code":0,"kind":"model"},"rotation":rotation}]})
}

fn fixture() -> (Project, NativeDraftingPlan, Value) {
    let plan: NativeDraftingPlan = serde_json::from_value(json!({
        "schema":"oev2.native-drafting-plan/1","sheet_name":"prototype-reference",
        "object":{"object_id":"coupon","revision":"b","condition_scope":"design","source_sha256":"a".repeat(64)},
        "unit":"mm","width":297.0,"height":210.0,"scale_numerator":1.0,"scale_denominator":2.0,
        "projection_angle":"third","model_view_name":"Top","view_center":[148.5,105.0],
        "purpose":"prototype_geometry_only","source_refs":["independent-design"]})).unwrap();
    let observation = json!({"schema":"oev2.native-drawing-observation/1","object":plan.object,
        "native_result_ref":"native-receipt","native_result_sha256":"b".repeat(64),"part":observed_part(&plan)});
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"coupon","object_revision":"b",
        "purpose":"source-bound prototype drawing obligation","acceptance":["current page and view"],"facts":[
            {"property":"drafting_plan:prototype-reference","value":plan,"status":"declared","source_refs":["independent-design"]},
            {"property":"native_drafting:prototype-reference","value":observation,"status":"declared","source_refs":["native-receipt","cad"]}],
        "assertions":[{"assertion_id":"prototype-page","kind":"cad_drafting_sheet","subject":"prototype-reference",
            "requirements":plan,"source_refs":["independent-design"],"affected_decisions":["drawing"]}]});
    let decisions = ["drawing","inspection","cost"].map(|id| json!({"id":id,"object_id":"coupon","object_revision":"b",
        "condition_scope":"design","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["independent-design"]}));
    let project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"drawing-current","revision":"b",
        "purpose":basis["purpose"],"objects":{"coupon":"b"},"conditions":{"design":serde_json::to_string(&basis).unwrap()},
        "evidence":{"cad":"a".repeat(64),"native-receipt":"b".repeat(64),"independent-design":"c".repeat(64)},
        "decisions":decisions,"dependencies":{"inspection":["drawing"],"cost":["inspection"]}})).unwrap();
    (project, plan, basis)
}
fn save(p: &mut Project, b: &Value) {
    p.conditions
        .insert("design".into(), serde_json::to_string(b).unwrap());
}

#[test]
fn drafting_plan_and_native_source_remain_current_with_downstream_impact() {
    let (mut p, plan, mut b) = fixture();
    let old = drafting::check(&p, &plan);
    assert_eq!(
        old.conclusion, "pass",
        "the frozen declared geometry diagnostic has bounded meaning"
    );
    b["facts"][0]["value"]["scale_denominator"] = json!(3.0);
    save(&mut p, &b);
    let checks = engineering::checks(&p, &plan.object);
    let c = checks
        .iter()
        .find(|c| c.id == "engineering:prototype-page")
        .unwrap();
    assert_eq!(c.conclusion, "violated");
    assert_eq!(c.failures, ["drafting_design_content_changed"]);
    assert_eq!(
        c.result["details"]["affected_decisions"],
        json!(["cost", "drawing", "inspection"])
    );
    assert_eq!(
        old.conclusion, "pass",
        "history is not re-signed after current edits"
    );
    assert_eq!(c.result["engineering_adoption"], "not_established");

    let (mut p, plan, _) = fixture();
    p.objects.insert("coupon".into(), "c".into());
    assert_eq!(
        drafting::check(&p, &plan).failures,
        ["drafting_current_object_mismatch"]
    );
    let (mut p, plan, _) = fixture();
    p.evidence.insert("native-receipt".into(), "d".repeat(64));
    assert_eq!(
        drafting::check(&p, &plan).failures,
        ["drafting_native_source_mismatch"]
    );
}

#[test]
fn missing_plan_and_independent_source_are_not_pass() {
    let (mut p, plan, mut b) = fixture();
    b["assertions"][0]["requirements"] = Value::Null;
    b["facts"][0]["value"] = Value::Null;
    b["facts"][0]["status"] = json!("unknown");
    save(&mut p, &b);
    let c = engineering::checks(&p, &plan.object)
        .into_iter()
        .find(|c| c.id == "engineering:prototype-page")
        .unwrap();
    assert_eq!(c.conclusion, "unknown");
    assert_eq!(c.failures, ["drafting_current_plan_missing"]);
    assert_eq!(
        c.result["details"]["affected_decisions"],
        json!(["cost", "drawing", "inspection"])
    );
    assert!(
        c.result["details"]["required_inputs"]
            .as_array()
            .unwrap()
            .contains(&json!("independent_prototype_drawing_plan"))
    );

    let (mut p, plan, _) = fixture();
    p.evidence
        .insert("independent-design".into(), "b".repeat(64));
    assert_eq!(
        drafting::check(&p, &plan).failures,
        ["drafting_expectation_not_independent"]
    );
    let (mut p, plan, mut b) = fixture();
    b["facts"].as_array_mut().unwrap().pop();
    save(&mut p, &b);
    assert_eq!(
        drafting::check(&p, &plan).failures,
        ["drafting_native_observation_missing"]
    );
}

#[test]
fn current_native_source_required_without_hiding_geometry() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("SKILL.md"), "independent fixture").unwrap();
    let root = oe_core::SkillRoot::new(temp.path().canonicalize().unwrap()).unwrap();
    let (mut p, plan, mut b) = fixture();
    let checks = engineering::checks_current(&root, &p, &plan.object);
    let geometry = checks
        .iter()
        .find(|c| c.id == "engineering:prototype-page")
        .unwrap();
    assert_eq!(
        geometry.conclusion, "pass",
        "the original bounded geometry diagnostic still executes"
    );
    let source = checks
        .iter()
        .find(|c| c.id == "engineering_native_source:prototype-page")
        .unwrap();
    assert_eq!(source.conclusion, "unknown");
    assert_eq!(source.failures, ["drawing_native_current_locator_missing"]);
    assert_eq!(
        source.result["details"]["affected_decisions"],
        json!(["cost", "drawing", "inspection"])
    );
    b["facts"][0]["value"]["model_view_name"] = json!("Front");
    save(&mut p, &b);
    let checks = engineering::checks_current(&root, &p, &plan.object);
    assert_eq!(checks[1].failures, ["drafting_design_content_changed"]);
    assert_eq!(
        checks[2].failures,
        ["drawing_native_current_locator_missing"]
    );
}
