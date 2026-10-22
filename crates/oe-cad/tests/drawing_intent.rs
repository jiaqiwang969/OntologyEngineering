//! Independent geometric requirements, never expectations copied from readback.
use oe_cad::drawing::intent::check_points;
use oe_contracts::DrawingReferenceIntent;
use serde_json::{Value, json};

fn fixture() -> (DrawingReferenceIntent, Value) {
    let intent = serde_json::from_value(json!({"schema":"oev2.drawing-reference-intent/1","intent_id":"two-datums",
        "object":{"object_id":"coupon","revision":"b","condition_scope":"design","source_sha256":"a".repeat(64)},
        "local_frame":"part_absolute","unit":"mm","position_tolerance":0.001,"proof_scope":"reference_points",
        "points":[{"point_id":"left","position":[0,0,0],"point_type_raw":1,"line_type_raw":0,"parent_type_raw":3,"parent_subtype_raw":0},
        {"point_id":"right","position":[20,0,0],"point_type_raw":1,"line_type_raw":0,"parent_type_raw":3,"parent_subtype_raw":0}],
        "source_refs":["independent-design"]})).unwrap();
    let entry = |x| {
        json!({"association_position_valid":true,"association_position":[x,0,0],"unit":"mm",
        "coordinate_basis":"UF_DRF assoc_point absolute coordinates","point_type_raw":1,"line_type_raw":0,
        "object_1":{"present":true,"tag":42,"metadata":{"type_query":{"return_code":0,"type_raw":3,"subtype_raw":0},
        "owning_part_query":{"value_valid":true,"matches_expected_owner":true}}}})
    };
    let dimension = json!({"dimension_tag":900,"retained":false,"retained_query_code":0,
        "associativity_parents":{"capture_complete":true,"wrapper_code":0,"uf_return_code":0,"entries":[entry(0),entry(20)]}});
    let part = json!({"annotations":{"schema":"oev2.nx-annotation-observation/2","units_query":{"return_code":0,"unit":"mm"},"dimensions":[dimension]}});
    (intent, part)
}
#[test]
fn legacy_native_observation_wire_identity_remains_without_current_locators() {
    let (intent, part) = fixture();
    let original = json!({"schema":"oev2.native-drawing-observation/1","object":intent.object,
        "native_result_ref":"native","native_result_sha256":"b".repeat(64),"part":part});
    let observation: oe_contracts::NativeDrawingObservation =
        serde_json::from_value(original.clone()).unwrap();
    assert_eq!(
        oe_cad::drawing::intent::observation_value(&observation).unwrap(),
        original
    );
    let mut invalid = observation;
    invalid.job_directory = Some("var/job".into());
    assert!(oe_cad::drawing::intent::observation_value(&invalid).is_err());
}
fn entries(p: &mut Value) -> &mut Vec<Value> {
    p["annotations"]["dimensions"][0]["associativity_parents"]["entries"]
        .as_array_mut()
        .unwrap()
}
#[test]
fn independent_reference_points_detect_wrong_and_ambiguous_dimensions_without_adoption() {
    let (intent, part) = fixture();
    let passed = check_points(&intent, &part).unwrap();
    assert_eq!(passed["conclusion"], "pass");
    assert_eq!(passed["engineering_adoption"], "not_established");
    assert_eq!(passed["intended_parent_correctness"], "unknown");
    let mut shuffled = part.clone();
    entries(&mut shuffled).reverse();
    for e in entries(&mut shuffled) {
        e["object_1"]["tag"] = json!(1234);
    }
    assert_eq!(
        check_points(&intent, &shuffled).unwrap()["conclusion"],
        "pass"
    );
    let mut wrong = part.clone();
    entries(&mut wrong)[1]["association_position"][0] = json!(19.5);
    assert_eq!(
        check_points(&intent, &wrong).unwrap()["reason"],
        "drawing_reference_no_dimension_matches"
    );
    let mut duplicate = part.clone();
    duplicate["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(part["annotations"]["dimensions"][0].clone());
    assert_eq!(
        check_points(&intent, &duplicate).unwrap()["reason"],
        "drawing_reference_ambiguous"
    );
    let mut overlap = intent.clone();
    overlap.position_tolerance = 20.0;
    assert_eq!(
        check_points(&overlap, &part).unwrap()["reason"],
        "drawing_reference_ambiguous"
    );
}
#[test]
fn failed_scope_and_units_cannot_be_hidden_by_one_good_dimension() {
    let (intent, part) = fixture();
    let mut failed = part.clone();
    let mut d = part["annotations"]["dimensions"][0].clone();
    d["associativity_parents"]["uf_return_code"] = json!(1001);
    failed["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(d);
    assert_eq!(
        check_points(&intent, &failed).unwrap()["reason"],
        "drawing_reference_incomplete_scope"
    );
    let mut unit = part.clone();
    unit["annotations"]["units_query"]["unit"] = json!("inch");
    assert_eq!(
        check_points(&intent, &unit).unwrap()["reason"],
        "drawing_reference_unit_mismatch"
    );
    let mut missing = part.clone();
    missing["annotations"]["units_query"]["unit"] = Value::Null;
    assert_eq!(
        check_points(&intent, &missing).unwrap()["reason"],
        "drawing_reference_unit_unknown"
    );
    let mut invalid = part;
    entries(&mut invalid)[0]["association_position"] = json!([0, 0]);
    assert_eq!(
        check_points(&intent, &invalid).unwrap()["conclusion"],
        "unknown"
    );
}
#[test]
fn query_success_does_not_override_retained_or_parent_and_proof_scope() {
    let (intent, part) = fixture();
    let mut retained = part.clone();
    retained["annotations"]["dimensions"][0]["retained"] = json!(true);
    assert_eq!(
        check_points(&intent, &retained).unwrap()["reason"],
        "drawing_reference_dimension_retained"
    );
    for field in ["point_type_raw", "line_type_raw"] {
        let mut wrong = part.clone();
        entries(&mut wrong)[0][field] = json!(7);
        assert_eq!(
            check_points(&intent, &wrong).unwrap()["conclusion"],
            "violated"
        );
    }
    let mut foreign = part.clone();
    entries(&mut foreign)[0]["object_1"]["metadata"]["owning_part_query"]["matches_expected_owner"] =
        json!(false);
    assert_eq!(
        check_points(&intent, &foreign).unwrap()["conclusion"],
        "violated"
    );
    let mut topology = intent;
    topology.proof_scope = "unique_model_parent".into();
    let report = check_points(&topology, &part).unwrap();
    assert_eq!(report["reference_point_agreement"], "pass");
    assert_eq!(report["conclusion"], "unknown");
    assert_eq!(
        report["reason"],
        "drawing_reference_unique_model_parent_unproven"
    );
}

#[test]
fn version_two_checks_actual_model_parent_and_computed_value_without_changing_v1() {
    let (old, mut part) = fixture();
    let mut request = oe_cad::drawing::intent::intent_value(&old).unwrap();
    request["schema"] = json!("oev2.drawing-reference-intent/2");
    request["proof_scope"] = json!("unique_model_parent");
    request["dimension_value"] = json!({"metric":"computed_linear","value":20.0,"numeric_tolerance":0.001,"unit":"mm","dimension_subtype_raw":3});
    let meta = |subtype| {
        json!({"type_query":{"return_code":0,"type_raw":70,"subtype_raw":subtype},
        "owning_part_query":{"value_valid":true,"matches_expected_owner":true}})
    };
    let mut model_entries = Vec::new();
    for (i, x) in [0.0, 20.0].into_iter().enumerate() {
        request["points"][i]["point_type_raw"] = json!(2);
        request["points"][i]["parent_type_raw"] = json!(70);
        request["points"][i]["parent_subtype_raw"] = json!(3);
        request["points"][i]["parent_edge"] = json!({"feature_id":format!("hole-{i}"),"center":[x,0,0],"normal":[0,0,1],
            "radius":2.0,"linear_tolerance":0.001,"angular_tolerance_radians":1e-6,"unit":"mm","local_frame":"part_absolute","full_circle":true});
        let e = &mut entries(&mut part)[i];
        e["point_type_raw"] = json!(2);
        e["object_1"]["tag"] = json!(100 + i);
        e["object_1"]["metadata"] = meta(3);
        model_entries.push(json!({"body_tag":90,"edge_tag":100+i,"metadata":meta(3),
            "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,"is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":true},
            "arc":{"value_valid":true,"is_periodic":true,"limits":[0,std::f64::consts::TAU],"radius":2.0,
                "center":[x,0,0],"x_axis":[1,0,0],"y_axis":[0,1,0]}}));
    }
    part["units"] = json!("mm");
    part["solid_tags"] = json!([90]);
    part["solid_count"] = json!(1);
    part["model_edges"] = json!({"schema":"oev2.nx-model-edge-observation/1","complete":true,"unit":"mm",
        "coordinate_basis":"UF_EVAL ABS coordinates; single-part solid edges","solid_type_raw":70,"edge_subtype_raw":3,
        "bodies":[{"body_tag":90,"metadata":meta(0),"edge_list_return_code":0,"capture_complete":true,"edge_tags":[100,101]}],"entries":model_entries});
    part["annotations"]["dimensions"][0]["computed_size"] = json!({"wrapper_code":0,"value_valid":true,"value":20.0,"unit":"mm",
        "basis":"NXOpen::Annotations::Dimension::ComputedSize","metadata":{"type_query":{"return_code":0,"type_raw":26,"subtype_raw":3},
            "owning_part_query":{"value_valid":true,"matches_expected_owner":true}}});
    let intent: DrawingReferenceIntent = serde_json::from_value(request.clone()).unwrap();
    let result = check_points(&intent, &part).unwrap();
    assert_eq!(result["conclusion"], "pass");
    assert_eq!(result["intended_parent_correctness"], "pass");
    assert_eq!(result["computed_dimension_agreement"], "pass");
    assert_eq!(result["engineering_adoption"], "not_established");
    let mut forged_parent = part.clone();
    entries(&mut forged_parent)[0]["object_1"]["tag"] = json!(777);
    assert_eq!(
        check_points(&intent, &forged_parent).unwrap()["reason"],
        "drawing_reference_model_parent_mismatch"
    );
    let mut wrong = part.clone();
    wrong["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(19.9);
    assert_eq!(
        check_points(&intent, &wrong).unwrap()["reason"],
        "drawing_reference_computed_value_mismatch"
    );
    let mut missing = part.clone();
    missing["annotations"]["dimensions"][0]["computed_size"] = Value::Null;
    assert_eq!(
        check_points(&intent, &missing).unwrap()["reason"],
        "drawing_reference_computed_value_unknown"
    );
    let mut old_scope = old;
    old_scope.proof_scope = "unique_model_parent".into();
    assert_eq!(
        check_points(&old_scope, &fixture().1).unwrap()["reason"],
        "drawing_reference_unique_model_parent_unproven"
    );
    let legacy = oe_cad::drawing::intent::intent_value(&old_scope).unwrap();
    assert!(legacy.get("dimension_value").is_none());
    assert!(legacy["points"][0].get("parent_edge").is_none());
}

#[test]
fn version_three_selector_does_not_change_old_wire_or_allow_protocol_mixing() {
    let (old, _) = fixture();
    let frozen_v1 = oe_cad::drawing::intent::intent_value(&old).unwrap();
    assert!(frozen_v1["points"][0].get("parent_edge_v2").is_none());
    let mut v2 = frozen_v1.clone();
    v2["schema"] = json!("oev2.drawing-reference-intent/2");
    v2["dimension_value"] = Value::Null;
    for p in v2["points"].as_array_mut().unwrap() {
        p["parent_edge"] = Value::Null;
    }
    let old_v2: DrawingReferenceIntent = serde_json::from_value(v2.clone()).unwrap();
    assert_eq!(oe_cad::drawing::intent::intent_value(&old_v2).unwrap(), v2);
    let mut v3 = v2.clone();
    v3["schema"] = json!("oev2.drawing-reference-intent/3");
    for (i, p) in v3["points"].as_array_mut().unwrap().iter_mut().enumerate() {
        p["point_type_raw"] = json!(2);
        p["parent_type_raw"] = json!(70);
        p["parent_subtype_raw"] = json!(3);
        p["parent_edge_v2"] = json!({"feature_id":format!("hole-{i}"),"center":p["position"],"normal":[0,0,1],"radius":2.0,
            "center_tolerance":0.015,"radius_tolerance":0.007,"angular_tolerance_radians":1e-6,
            "unit":"mm","local_frame":"part_absolute","full_circle":true});
    }
    let current: DrawingReferenceIntent = serde_json::from_value(v3.clone()).unwrap();
    oe_cad::drawing::intent::validate(&current).unwrap();
    let wire = oe_cad::drawing::intent::intent_value(&current).unwrap();
    assert!(wire["points"][0].get("parent_edge").is_none());
    for schema in [
        "oev2.drawing-reference-intent/1",
        "oev2.drawing-reference-intent/2",
    ] {
        let mut wrong = v3.clone();
        wrong["schema"] = json!(schema);
        assert!(
            oe_cad::drawing::intent::validate(&serde_json::from_value(wrong).unwrap()).is_err()
        );
    }
    let mut mixed = v3;
    mixed["points"][0]["parent_edge"] = json!({"feature_id":"legacy","center":[0,0,0],"normal":[0,0,1],
        "radius":2.0,"linear_tolerance":0.007,"angular_tolerance_radians":1e-6,"unit":"mm","local_frame":"part_absolute","full_circle":true});
    assert!(oe_cad::drawing::intent::validate(&serde_json::from_value(mixed).unwrap()).is_err());
}
