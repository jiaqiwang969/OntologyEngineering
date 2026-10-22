//! Independent synthetic input observations, not native execution.
use oe_contracts::NativeDraftingPlan;
use serde_json::{Value, json};
fn metadata(name: &str) -> Value {
    json!({"type_query":{"return_code":0},
        "owning_part_query":{"return_code":0,"value_valid":true,"matches_expected_owner":true},
        "name_query":{"return_code":0,"value_valid":true,"utf8":name}})
}

fn fixture() -> (NativeDraftingPlan, Value) {
    let plan = serde_json::from_value(json!({"schema":"oev2.native-drafting-plan/1",
        "object":{"object_id":"urn:oev2:test:bracket","revision":"r4","condition_scope":"fixture",
            "source_sha256":"b".repeat(64)},"sheet_name":"TRIAL-REF","unit":"mm",
        "width":320.0,"height":210.0,"scale_numerator":1.0,"scale_denominator":2.0,
        "projection_angle":"first","model_view_name":"TOP","view_center":[150.0,110.0],
        "source_refs":["urn:oev2:test:independent-layout"],"purpose":"prototype_geometry_only"}))
    .unwrap();
    let rotation = json!({"return_code":0,"matrix":[1,0,0,0,1,0,0,0,1]});
    let view = json!({"view_tag":41,"metadata":metadata("TOP@1"),"stale_query_code":0,"stale_after":false,
        "view_type":{"return_code":0,"kind":"drawing_member","subtype":"imported"},
        "owning_drawing":{"return_code":0,"drawing_tag":40},
        "model_part":{"return_code":0,"path":r"C:\controlled\part.prt"},"rotation":rotation,
        "scale":{"return_code":0,"value":0.5},"reference_point":{"return_code":0,"point":[150,110]},
        "borders":{"return_code":0,"values":[110,60,190,130]},
        "visible_objects":{"return_code":0,"capture_complete":true,"visible_count":4,"clipped_count":0}});
    let part = json!({"path":r"C:\controlled\part.prt","units":"mm","solid_count":1,
        "drafting_readback_schema":"oev2.native-drafting-observation/1","drawing_catalog_complete":true,
        "drawing_sheet_count":1,"drawing_sheets":[{"drawing_tag":40,"metadata":metadata("TRIAL-REF"),
            "sheet_info":{"wrapper_code":0,"query_code":0,"value_valid":true,"unit":"mm","size_state":"custom",
                "width":320,"height":210,"scale":0.5,"projection_angle":"first"},
            "view_catalog_complete":true,"views":[view]}],
        "modeling_view_catalog_complete":true,"modeling_views":[{"view_tag":31,"metadata":metadata("TOP"),
            "view_type":{"return_code":0,"kind":"model"},"rotation":rotation}]});
    (plan, part)
}

fn direct_fixture() -> (NativeDraftingPlan, Value) {
    let (plan, mut part) = fixture();
    part["drafting_readback_schema"] = json!("oev2.native-drafting-observation/6");
    part["part_tag"] = json!(50);
    part["solid_tags"] = json!([71]);
    part["bbox"] = json!([[-4, -10, -8, 8, 25, 10]]);
    part["display_part"] =
        json!({"return_code":0,"expected_part":50,"actual_part":50,"matches_expected":true});
    let sheet = &mut part["drawing_sheets"][0];
    sheet["display_open_code"] = json!(0);
    sheet["display_state"] = json!({"set_code":0,"query_code":0,"state_raw":2,
        "current_drawing_code":0,"current_drawing_tag":40,"work_view_code":0,"work_view_tag":42,
        "work_view_type":{"return_code":0,"type_raw":2,"kind":"drawing_sheet"}});
    let view = &mut sheet["views"][0];
    view["boundary"] = json!({"type_query_code":0,"type_raw":4,"kind":"bound_by_objects",
        "objects_query_code":0,"capture_complete":true,"object_count":1,"object_tags":[71]});
    view["visible_objects"]["visible_tags"] = json!([51, 52, 53, 54]);
    view["visible_objects"]["clipped_tags"] = json!([]);
    view["layout_anchor"] = json!({"query_code":0,"point_tag":0,"point_data":null});
    view["layout_reference"] = json!({"schema":"oev2.native-layout-reference/1","unit":"mm",
        "model_part_tag":50,"drawing_view_tag":41,"coordinate_basis":"absolute_current_part_bbox_midpoint",
        "model_coordinates":[2,7.5,1],"drawing_mapping":{"return_code":0,"capture_complete":true,
            "model_point":[2,7.5,1],"coordinates":plan.view_center}});
    part["modeling_views"][0]["view_center"] = json!({"query_code":0,"coordinates":[0,0,0]});
    (plan, part)
}

fn sheet_dimension_fixture() -> (
    NativeDraftingPlan,
    oe_contracts::DrawingReferenceIntent,
    Value,
) {
    let (mut sheet, mut part) = direct_fixture();
    sheet.object.source_sha256 = oe_nx_protocol::BlobIdentity::of(b"independent-fixture-cad")
        .sha256
        .as_str()
        .to_owned();
    let point = |id: &str, z: f64| {
        json!({"point_id":id,"position":[0,0,z],"point_type_raw":2,
        "line_type_raw":0,"parent_type_raw":70,"parent_subtype_raw":3,"parent_edge":{
            "feature_id":id,"center":[0,0,z],"normal":[0,0,1],"radius":2.5,"linear_tolerance":0.001,
            "angular_tolerance_radians":1e-6,"unit":"mm","local_frame":"part_absolute","full_circle":true}})
    };
    let reference = serde_json::from_value(json!({"schema":"oev2.drawing-reference-intent/2","intent_id":"axial-length",
        "object":sheet.object,"local_frame":"part_absolute","unit":"mm","position_tolerance":0.001,
        "proof_scope":"unique_model_parent","source_refs":["independent-shaft-length"],"points":[point("a",0.0),point("b",32.0)],
        "dimension_value":{"metric":"computed_linear","value":32.0,"numeric_tolerance":0.001,"unit":"mm","dimension_subtype_raw":3}})).unwrap();
    let rotation = json!({"return_code":0,"matrix":[1,0,0,0,0,1,0,-1,0]});
    part["modeling_views"][0]["rotation"] = rotation.clone();
    part["drawing_sheets"][0]["views"][0]["rotation"] = rotation;
    let metadata = |subtype| {
        json!({"type_query":{"return_code":0,"type_raw":70,"subtype_raw":subtype},
        "owning_part_query":{"value_valid":true,"matches_expected_owner":true}})
    };
    let entry = |tag, z| {
        json!({"body_tag":71,"edge_tag":tag,"metadata":metadata(3),
        "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,"is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":true},
        "arc":{"value_valid":true,"is_periodic":true,"limits":[0,std::f64::consts::TAU],"radius":2.5,
            "center":[0,0,z],"x_axis":[1,0,0],"y_axis":[0,1,0]}})
    };
    part["model_edges"] = json!({"schema":"oev2.nx-model-edge-observation/1","complete":true,"unit":"mm",
        "coordinate_basis":"UF_EVAL ABS coordinates; single-part solid edges","solid_type_raw":70,"edge_subtype_raw":3,
        "bodies":[{"body_tag":71,"metadata":metadata(0),"edge_list_return_code":0,"capture_complete":true,"edge_tags":[101,102]}],
        "entries":[entry(101,0.0),entry(102,32.0)]});
    (sheet, reference, part)
}

fn sheet_dimension_created_fixture() -> (
    NativeDraftingPlan,
    oe_contracts::DrawingReferenceIntent,
    Value,
    Value,
) {
    let (sheet, reference, mut before) = sheet_dimension_fixture();
    before["annotations"] = json!({"schema":"oev2.nx-annotation-observation/2","units_query":{"return_code":0,"unit":"mm"},"dimensions":[]});
    let metadata = |kind, subtype| {
        json!({"type_query":{"return_code":0,"type_raw":kind,"subtype_raw":subtype},
        "owning_part_query":{"value_valid":true,"matches_expected_owner":true}})
    };
    let entry = |edge| {
        json!({"object_1":{"tag":edge,"present":true,"metadata":metadata(70,3)},
        "object_2":{"tag":0,"present":false},"view":{"tag":41,"present":true,"metadata":metadata(60,1)},
        "point_type_raw":2,"line_type_raw":0,"unit":"mm"})
    };
    let mut after = before.clone();
    after["annotations"]["dimensions"] = json!([{"dimension_tag":200,"retained_query_code":0,"retained":false,
        "computed_size":{"wrapper_code":0,"value_valid":true,"value":32.0,"unit":"mm",
            "basis":"NXOpen::Annotations::Dimension::ComputedSize","metadata":metadata(26,3)},
        "associativity_parents":{"wrapper_code":0,"uf_return_code":0,"capture_complete":true,"entries":[entry(101),entry(102)]}}]);
    (sheet, reference, before, after)
}

pub fn diameter_fixture() -> (oe_contracts::SheetDiameterIntent, Value, Value) {
    let (sheet, _, mut before, mut after) = sheet_dimension_created_fixture();
    for part in [&mut before, &mut after] {
        let rot = json!({"return_code":0,"matrix":[1,0,0,0,1,0,0,0,1]});
        part["modeling_views"][0]["rotation"] = rot.clone();
        part["drawing_sheets"][0]["views"][0]["rotation"] = rot;
    }
    let d = &mut after["annotations"]["dimensions"][0];
    d["computed_size"]["value"] = json!(5.0);
    d["computed_size"]["metadata"]["type_query"]["subtype_raw"] = json!(10);
    d["associativity_parents"]["entries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    let intent=serde_json::from_value(json!({"schema":"oev2.sheet-diameter-intent/1","sheet":sheet,
        "edge":{"feature_id":"near-circle","center":[0,0,0],"normal":[0,0,1],"radius":2.5,
            "center_tolerance":0.001,"radius_tolerance":0.001,"angular_tolerance_radians":1e-6,
            "local_frame":"part_absolute","unit":"mm","full_circle":true},
        "diameter":5.0,"numeric_tolerance":0.001,"text_origin_drawing":[100,140],"source_refs":["independent-diameter-five"]})).unwrap();
    (intent, before, after)
}
