//! Independent geometry cases; SDK and physical use are verified separately.
use oe_contracts::CircularEdgeIntent;
use oe_nx_worker::geometry::model_edges::select;
use serde_json::{Value, json};

fn metadata(subtype: i64) -> Value {
    json!({"type_query":{"return_code":0,"type_raw":70,"subtype_raw":subtype},
    "owning_part_query":{"value_valid":true,"matches_expected_owner":true}})
}
fn fixture() -> (CircularEdgeIntent, Value) {
    let intent = serde_json::from_value(
        json!({"feature_id":"location-hole","center":[7.5,-3.25,4.0],"normal":[0,0,1],
        "radius":2.4,"linear_tolerance":0.0001,"angular_tolerance_radians":0.00001,
        "unit":"mm","local_frame":"part_absolute","full_circle":true}),
    )
    .unwrap();
    let entry = json!({"body_tag":100,"edge_tag":101,"metadata":metadata(3),
        "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,"is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":true},
        "arc":{"value_valid":true,"is_periodic":true,"limits":[0,std::f64::consts::TAU],"radius":2.4,
            "center":[7.5,-3.25,4.0],"x_axis":[1,0,0],"y_axis":[0,1,0]}});
    let part = json!({"units":"mm","solid_count":1,"solid_tags":[100],"model_edges":{
        "schema":"oev2.nx-model-edge-observation/1","complete":true,"unit":"mm",
        "coordinate_basis":"UF_EVAL ABS coordinates; single-part solid edges","solid_type_raw":70,"edge_subtype_raw":3,
        "bodies":[{"body_tag":100,"metadata":metadata(0),"edge_list_return_code":0,"capture_complete":true,"edge_tags":[101]}],
        "entries":[entry]}});
    (intent, part)
}
#[test]
fn complete_circle_selection_distinguishes_wrong_duplicate_trimmed_and_incomplete_edges() {
    let (intent, part) = fixture();
    assert_eq!(
        select(&intent, &part).unwrap()["candidate_edge_tags"],
        json!([101])
    );
    let mut reversed = intent.clone();
    reversed.normal = vec![0.0, 0.0, -1.0];
    assert_eq!(select(&reversed, &part).unwrap()["conclusion"], "pass");
    let mut radius = intent.clone();
    radius.radius = 2.7;
    assert_eq!(
        select(&radius, &part).unwrap()["reason"],
        "model_circle_no_edge_matches"
    );
    let mut plane = intent.clone();
    plane.normal = vec![1.0, 0.0, 0.0];
    assert_eq!(select(&plane, &part).unwrap()["conclusion"], "violated");
    let mut duplicate = part.clone();
    let mut e = duplicate["model_edges"]["entries"][0].clone();
    e["edge_tag"] = json!(102);
    duplicate["model_edges"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(e);
    duplicate["model_edges"]["bodies"][0]["edge_tags"]
        .as_array_mut()
        .unwrap()
        .push(json!(102));
    assert_eq!(
        select(&intent, &duplicate).unwrap()["reason"],
        "model_circle_ambiguous"
    );
    let mut trimmed = part.clone();
    trimmed["model_edges"]["entries"][0]["arc"]["limits"] = json!([0, std::f64::consts::PI]);
    assert_eq!(select(&intent, &trimmed).unwrap()["conclusion"], "violated");
    for field in ["initialize_code", "is_arc_code", "arc_code", "free_code"] {
        let mut failed = part.clone();
        failed["model_edges"]["entries"][0]["query"][field] = json!(7);
        assert_eq!(select(&intent, &failed).unwrap()["conclusion"], "unknown");
    }
    let mut omitted = duplicate;
    omitted["model_edges"]["entries"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(select(&intent, &omitted).unwrap()["conclusion"], "unknown");
    let mut foreign = part.clone();
    foreign["model_edges"]["entries"][0]["metadata"]["owning_part_query"]["matches_expected_owner"] =
        json!(false);
    assert_eq!(select(&intent, &foreign).unwrap()["conclusion"], "unknown");
    let mut units = part;
    units["model_edges"]["unit"] = json!("inch");
    assert_eq!(
        select(&intent, &units).unwrap()["reason"],
        "model_circle_unit_mismatch"
    );
}

#[test]
fn incomplete_unrelated_edge_cannot_hide_another_possible_match() {
    let (intent, mut part) = fixture();
    let mut unknown = part["model_edges"]["entries"][0].clone();
    unknown["edge_tag"] = json!(102);
    unknown["query"]["is_arc"] = Value::Null;
    part["model_edges"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(unknown);
    part["model_edges"]["bodies"][0]["edge_tags"]
        .as_array_mut()
        .unwrap()
        .push(json!(102));
    assert_eq!(select(&intent, &part).unwrap()["conclusion"], "unknown");
    assert_eq!(
        select(&intent, &part).unwrap()["candidate_edge_tags"],
        json!([101])
    );
}

#[test]
fn separate_tolerances_preserve_legacy_geometry_and_reject_radius_substitution() {
    let (mut legacy, mut part) = fixture();
    legacy.linear_tolerance = 0.007;
    part["model_edges"]["entries"][0]["arc"]["center"] = json!([7.510, -3.25, 4.0]);
    assert_eq!(select(&legacy, &part).unwrap()["conclusion"], "violated");
    let separate: oe_contracts::CircularEdgeIntentV2 = serde_json::from_value(json!({
        "feature_id":"location-hole","center":[7.5,-3.25,4.0],"normal":[0,0,1],"radius":2.4,
        "center_tolerance":0.015,"radius_tolerance":0.007,"angular_tolerance_radians":0.00001,
        "unit":"mm","local_frame":"part_absolute","full_circle":true}))
    .unwrap();
    let result = oe_nx_worker::geometry::model_edges::select_v2(&separate, &part).unwrap();
    assert_eq!(result["conclusion"], "pass");
    assert_eq!(result["schema"], "oev2.model-circle-edge-check/2");
    assert_eq!(result["tolerance_policy"]["center_tolerance"], 0.015);
    assert_eq!(result["tolerance_policy"]["radius_tolerance"], 0.007);
    part["model_edges"]["entries"][0]["arc"]["radius"] = json!(2.408);
    assert_eq!(
        oe_nx_worker::geometry::model_edges::select_v2(&separate, &part).unwrap()["conclusion"],
        "violated"
    );
    let mut invalid = separate;
    invalid.radius_tolerance = -0.007;
    assert!(oe_nx_worker::geometry::model_edges::select_v2(&invalid, &part).is_err());
}
