use oe_cad::drawing::associations::observation_projection;
use serde_json::{Value, json};

fn observation() -> Value {
    let parent = json!({"present":true,"tag":42,"metadata":{
        "type_query":{"return_code":0,"type_raw":70,"subtype_raw":3},
        "owning_part_query":{"value_valid":true,"matches_expected_owner":true},"name_query":{"utf8":"EDGE"}},
        "geometry":{"called":true,"return_code":0,"value_valid":true,"absolute_bounds":[0,0,0,10,0,0],"unit":"mm"}});
    let entry = json!({"object_1":parent,"object_2":{"present":false},"view":{"present":false},
        "association_position_valid":true,"association_position":[10,0,0],"point_type_raw":1,"line_type_raw":0,"unit":"mm"});
    let dimension = json!({"dimension_tag":101,"retained_query_code":0,"retained":false,
        "associativity_parents":{"capture_complete":true,"wrapper_code":0,"uf_return_code":0,"entries":[entry]}});
    json!({"annotations":{"schema":"oev2.nx-annotation-observation/2",
        "units_query":{"return_code":0,"unit":"mm"},"dimensions":[dimension]}})
}

#[test]
fn persistence_excludes_transient_tags_and_detects_changed_reference_observations() {
    let original = observation();
    let base = observation_projection(&original).unwrap();
    assert_eq!(base["unknowns"], json!([]));
    let mut another = original.clone();
    another["annotations"]["dimensions"][0]["dimension_tag"] = json!(909);
    another["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]["object_1"]["tag"] =
        json!(705);
    assert_eq!(observation_projection(&another).unwrap(), base);
    another["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]["association_position"]
        [0] = json!(11);
    assert_ne!(observation_projection(&another).unwrap(), base);
    let mut duplicate = original.clone();
    duplicate["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(original["annotations"]["dimensions"][0].clone());
    assert_ne!(
        observation_projection(&duplicate).unwrap(),
        base,
        "multiset counts cannot be silently collapsed"
    );
}

#[test]
fn query_failure_empty_association_and_missing_unit_remain_unknown() {
    let original = observation();
    let mut failure = original.clone();
    failure["annotations"]["dimensions"][0]["associativity_parents"]["uf_return_code"] =
        json!(1001);
    assert_eq!(
        observation_projection(&failure).unwrap()["unknowns"][0]["reason"],
        "retained_or_association_query_incomplete"
    );
    let mut absent = original.clone();
    absent["annotations"]["dimensions"][0]["associativity_parents"]["entries"] = json!([]);
    assert!(
        !observation_projection(&absent).unwrap()["unknowns"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut unit = original.clone();
    unit["annotations"]["units_query"]["unit"] = Value::Null;
    assert_eq!(
        observation_projection(&unit).unwrap_err().to_string(),
        "drawing_association_units_unknown"
    );
    let mut failed_parent = original;
    failed_parent["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]["object_1"]
        ["geometry"]["value_valid"] = json!(false);
    assert_eq!(
        observation_projection(&failed_parent).unwrap()["unknowns"][0]["reason"],
        "parent_geometry_query_failed"
    );
}
