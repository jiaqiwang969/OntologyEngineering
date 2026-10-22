use oe_contracts::NativeDraftingPlan;
use oe_nx_worker::geometry::drafting::{check, validate};
use serde_json::{Value, json};

#[test]
fn matching_invalid_rotations_do_not_establish_view_orientation() {
    let (plan, part) = fixture();
    for matrix in [
        json!(vec![0; 9]),
        json!([2, 0, 0, 0, 2, 0, 0, 0, 2]),
        json!([-1, 0, 0, 0, 1, 0, 0, 0, 1]),
        json!([1, 0.2, 0, 0, 1, 0, 0, 0, 1]),
    ] {
        let mut bad = part.clone();
        bad["modeling_views"][0]["rotation"]["matrix"] = matrix.clone();
        bad["drawing_sheets"][0]["views"][0]["rotation"]["matrix"] = matrix;
        let result = check(&plan, &bad);
        assert_eq!(result["conclusion"], "unknown");
        assert_eq!(result["reason"], "native_drafting_rotation_invalid");
    }
    let n = std::f64::consts::FRAC_1_SQRT_2;
    let mut valid = part;
    let matrix = json!([n, -n, 0, n, n, 0, 0, 0, 1]);
    valid["modeling_views"][0]["rotation"]["matrix"] = matrix.clone();
    valid["drawing_sheets"][0]["views"][0]["rotation"]["matrix"] = matrix;
    assert_eq!(check(&plan, &valid)["conclusion"], "pass");
}

#[test]
fn native_visibility_lists_must_agree_with_counts_when_declared() {
    let (plan, part) = fixture();
    assert_eq!(check(&plan, &part)["conclusion"], "pass"); // Original count-only DTO.
    let mut valid = part;
    let inventory = &mut valid["drawing_sheets"][0]["views"][0]["visible_objects"];
    inventory["visible_tags"] = json!([51, 52, 53, 54]);
    inventory["clipped_tags"] = json!([]);
    assert_eq!(check(&plan, &valid)["conclusion"], "pass");
    for (field, value) in [
        ("visible_tags", json!([])),
        ("clipped_tags", json!([72])),
        ("visible_tags", json!([51, 52, 53, 0])),
        ("visible_tags", json!([51, 52, 53, 4294967296_u64])),
        ("visible_tags", json!([51, 52, 53, "54"])),
        ("clipped_tags", Value::Null),
    ] {
        let mut bad = valid.clone();
        bad["drawing_sheets"][0]["views"][0]["visible_objects"][field] = value;
        let result = check(&plan, &bad);
        assert_eq!(result["conclusion"], "unknown");
        assert_eq!(
            result["reason"],
            "native_drafting_visible_inventory_inconsistent"
        );
    }
    let mut missing = valid.clone();
    missing["drawing_sheets"][0]["views"][0]["visible_objects"]
        .as_object_mut()
        .unwrap()
        .remove("clipped_tags");
    assert_eq!(check(&plan, &missing)["conclusion"], "unknown");
    let mut clipped = valid;
    clipped["drawing_sheets"][0]["views"][0]["visible_objects"]["clipped_tags"] = json!([72]);
    clipped["drawing_sheets"][0]["views"][0]["visible_objects"]["clipped_count"] = json!(1);
    assert_eq!(
        check(&plan, &clipped)["reason"],
        "native_drafting_empty_or_clipped_view"
    );
}

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
#[test]
fn prototype_sheet_preserves_geometry_scope_and_independent_inputs() {
    let (plan, part) = fixture();
    validate(&plan).unwrap();
    let result = check(&plan, &part);
    assert_eq!(result["conclusion"], "pass");
    assert_eq!(result["manufacturing_drawing"], "not_established");
    assert_eq!(result["engineering_adoption"], "not_established");
    assert_eq!(result["visual_readability"], "not_assessed");
    let mut revised = plan;
    revised.width = 325.0;
    assert_eq!(
        check(&revised, &part)["reason"],
        "native_drafting_sheet_plan_mismatch"
    );
}
#[test]
fn sheet_view_identity_unit_placement_and_clipping_are_not_success_when_invalid() {
    let (plan, part) = fixture();
    let cases = [
        (
            "/drawing_sheets/0/metadata/name_query/utf8",
            json!("ANOTHER"),
            "native_drafting_named_sheet_missing",
        ),
        (
            "/drawing_sheets/0/sheet_info/width",
            json!(300),
            "native_drafting_sheet_plan_mismatch",
        ),
        (
            "/drawing_sheets/0/sheet_info/unit",
            json!("inch"),
            "native_drafting_sheet_plan_mismatch",
        ),
        (
            "/drawing_sheets/0/sheet_info/projection_angle",
            json!("third"),
            "native_drafting_sheet_plan_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/stale_after",
            json!(true),
            "native_drafting_view_state_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/owning_drawing/drawing_tag",
            json!(999),
            "native_drafting_view_state_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/model_part/path",
            json!(r"C:\controlled\other.prt"),
            "native_drafting_model_part_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/rotation/matrix",
            json!([0, 1, 0, 1, 0, 0, 0, 0, -1]),
            "native_drafting_named_view_orientation_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/scale/value",
            json!(1.0),
            "native_drafting_view_placement_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/reference_point/point",
            json!([155, 110]),
            "native_drafting_view_placement_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/borders/values",
            json!([-1, 0, 190, 130]),
            "native_drafting_view_blank_or_outside_sheet",
        ),
        (
            "/drawing_sheets/0/views/0/borders/values",
            json!([110, 60, 110, 130]),
            "native_drafting_view_blank_or_outside_sheet",
        ),
        (
            "/drawing_sheets/0/views/0/visible_objects/visible_count",
            json!(0),
            "native_drafting_empty_or_clipped_view",
        ),
        (
            "/drawing_sheets/0/views/0/visible_objects/clipped_count",
            json!(1),
            "native_drafting_empty_or_clipped_view",
        ),
    ];
    for (pointer, value, reason) in cases {
        let mut bad = part.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        let result = check(&plan, &bad);
        assert_eq!(result["conclusion"], "violated", "{pointer}");
        assert_eq!(result["reason"], reason, "{pointer}");
    }
    let mut duplicate = part.clone();
    let sheet = duplicate["drawing_sheets"][0].clone();
    duplicate["drawing_sheets"]
        .as_array_mut()
        .unwrap()
        .push(sheet);
    duplicate["drawing_sheet_count"] = json!(2);
    assert_eq!(
        check(&plan, &duplicate)["reason"],
        "native_drafting_named_sheet_ambiguous"
    );
}
#[test]
fn incomplete_native_queries_and_catalogs_remain_unknown() {
    let (plan, part) = fixture();
    let cases = [
        ("/drawing_catalog_complete", json!(false)),
        ("/modeling_view_catalog_complete", json!(false)),
        ("/drawing_sheets/0/view_catalog_complete", json!(false)),
        ("/drawing_sheets/0/sheet_info/query_code", json!(17)),
        (
            "/drawing_sheets/0/views/0/model_part/return_code",
            json!(17),
        ),
        ("/drawing_sheets/0/views/0/rotation/return_code", json!(17)),
        (
            "/drawing_sheets/0/views/0/reference_point/point",
            Value::Null,
        ),
        (
            "/drawing_sheets/0/views/0/visible_objects/capture_complete",
            json!(false),
        ),
        ("/modeling_views/0/rotation/matrix", Value::Null),
        (
            "/drawing_sheets/0/metadata/name_query/value_valid",
            json!(false),
        ),
        (
            "/drawing_sheets/0/metadata/name_query/return_code",
            json!(17),
        ),
        (
            "/drawing_sheets/0/metadata/owning_part_query/return_code",
            json!(17),
        ),
        (
            "/drawing_sheets/0/views/0/visible_objects/return_code",
            json!(17),
        ),
    ];
    for (pointer, value) in cases {
        let mut bad = part.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(check(&plan, &bad)["conclusion"], "unknown", "{pointer}");
    }
    let mut historical = part;
    historical
        .as_object_mut()
        .unwrap()
        .remove("drafting_readback_schema");
    assert_eq!(
        check(&plan, &historical)["reason"],
        "native_drafting_catalog_incomplete"
    );
}
#[test]
fn invalid_plan_rejects_nonfinite_aliases_missing_sources_and_unbounded_names() {
    let (plan, part) = fixture();
    let mut invalid = plan.clone();
    invalid.width = f64::NAN;
    assert!(validate(&invalid).is_err());
    invalid = plan.clone();
    invalid.scale_denominator = 0.0;
    assert!(validate(&invalid).is_err());
    invalid = plan.clone();
    invalid.view_center = vec![1.0];
    assert!(validate(&invalid).is_err());
    invalid = plan.clone();
    invalid.source_refs.clear();
    assert!(validate(&invalid).is_err());
    invalid = plan.clone();
    invalid.sheet_name = "a".repeat(65);
    assert!(validate(&invalid).is_err());
    invalid = plan.clone();
    invalid.model_view_name = "TOP\n".into();
    assert!(validate(&invalid).is_err());
    invalid = plan;
    invalid.object.source_sha256 = "x".repeat(64);
    assert!(validate(&invalid).is_err());
    assert_eq!(check(&invalid, &part)["conclusion"], "unknown");
}
#[test]
fn standard_size_interoperability_has_its_own_version_and_exact_dimension_rules() {
    use oe_nx_worker::geometry::drafting::standard_sheet_dimensions;
    assert_eq!(standard_sheet_dimensions(1, 5, "mm"), Some([297.0, 210.0]));
    assert_eq!(standard_sheet_dimensions(2, 1, "inch"), Some([11.0, 8.5]));
    for (family, code, unit) in [
        (1, 5, "inch"),
        (2, 1, "mm"),
        (1, 0, "mm"),
        (1, 6, "mm"),
        (2, 9, "inch"),
        (9, 1, "mm"),
    ] {
        assert!(standard_sheet_dimensions(family, code, unit).is_none());
    }
    let (mut plan, mut part) = fixture();
    plan.width = 297.0;
    let info = &mut part["drawing_sheets"][0]["sheet_info"];
    info["schema"] = json!("oev2.native-drafting-sheet-info/2");
    info["size_state"] = json!("standard");
    info["size_state_raw"] = json!(0);
    info["standard_family"] = json!(1);
    info["standard_code"] = json!(5);
    info["width"] = json!(297);
    assert_eq!(check(&plan, &part)["conclusion"], "pass");
    for (field, value, reason) in [
        (
            "standard_code",
            Value::Null,
            "native_drafting_standard_size_unavailable",
        ),
        (
            "standard_code",
            json!(99),
            "native_drafting_standard_size_unavailable",
        ),
        (
            "standard_family",
            json!(2),
            "native_drafting_standard_size_unavailable",
        ),
        (
            "size_state_raw",
            json!(1),
            "native_drafting_standard_size_unavailable",
        ),
        (
            "width",
            json!(300),
            "native_drafting_standard_size_inconsistent",
        ),
    ] {
        let mut bad = part.clone();
        bad["drawing_sheets"][0]["sheet_info"][field] = value;
        assert_eq!(check(&plan, &bad)["reason"], reason);
    }
    let mut historical = part.clone();
    historical["drawing_sheets"][0]["sheet_info"]["schema"] =
        json!("oev2.native-drafting-sheet-info/1");
    assert_eq!(
        check(&plan, &historical)["reason"],
        "native_drafting_sheet_size_kind_unsupported"
    );
    let info = &mut part["drawing_sheets"][0]["sheet_info"];
    info["standard_code"] = json!(4);
    info["width"] = json!(420);
    info["height"] = json!(297);
    assert_eq!(
        check(&plan, &part)["reason"],
        "native_drafting_sheet_plan_mismatch"
    );
}
#[test]
fn geometry_only_boundary_requires_all_solids_and_keeps_page_limits() {
    let (plan, mut part) = fixture();
    part["drafting_readback_schema"] = json!("oev2.native-drafting-observation/2");
    part["solid_count"] = json!(2);
    part["solid_tags"] = json!([71, 72]);
    part["drawing_sheets"][0]["views"][0]["boundary"] = json!({"type_query_code":0,"type_raw":4,
        "kind":"bound_by_objects","objects_query_code":0,"capture_complete":true,"object_count":2,"object_tags":[72,71]});
    assert_eq!(check(&plan, &part)["conclusion"], "pass");
    for (field, value, reason) in [
        (
            "type_query_code",
            json!(17),
            "native_drafting_object_boundary_unavailable",
        ),
        (
            "objects_query_code",
            json!(17),
            "native_drafting_object_boundary_unavailable",
        ),
        (
            "capture_complete",
            json!(false),
            "native_drafting_object_boundary_unavailable",
        ),
        (
            "object_count",
            json!(1),
            "native_drafting_object_boundary_inventory_invalid",
        ),
        (
            "object_tags",
            json!([71, 71]),
            "native_drafting_object_boundary_inventory_invalid",
        ),
        (
            "object_tags",
            json!([71, 73]),
            "native_drafting_object_boundary_mismatch",
        ),
        (
            "kind",
            json!("manual_rectangle"),
            "native_drafting_object_boundary_mismatch",
        ),
        (
            "type_raw",
            json!(2),
            "native_drafting_object_boundary_mismatch",
        ),
    ] {
        let mut bad = part.clone();
        bad["drawing_sheets"][0]["views"][0]["boundary"][field] = value;
        assert_eq!(check(&plan, &bad)["reason"], reason);
    }
    let mut reduced = part.clone();
    reduced["drawing_sheets"][0]["views"][0]["boundary"]["object_count"] = json!(1);
    reduced["drawing_sheets"][0]["views"][0]["boundary"]["object_tags"] = json!([71]);
    assert_eq!(
        check(&plan, &reduced)["reason"],
        "native_drafting_object_boundary_mismatch"
    );
    part["drawing_sheets"][0]["views"][0]["borders"]["values"] = json!([0, 0, 330, 220]);
    assert_eq!(
        check(&plan, &part)["reason"],
        "native_drafting_view_blank_or_outside_sheet"
    );
}

#[test]
fn layout_anchor_covers_all_finite_current_solids_without_claiming_precision() {
    use oe_nx_worker::geometry::drafting::layout_anchor_from_bounds;
    // Synthetic far-origin geometry; private project coordinates stay in var/.
    assert_eq!(
        layout_anchor_from_bounds(&[[-24.0, 70.0, -820.0, -18.0, 94.0, -800.0]]).unwrap(),
        [-21.0, 82.0, -810.0]
    );
    assert_eq!(
        layout_anchor_from_bounds(&[
            [-4.0, 10.0, -3.0, 4.0, 20.0, 7.0],
            [8.0, -10.0, -5.0, 12.0, 25.0, 1.0]
        ])
        .unwrap(),
        [4.0, 7.5, 1.0]
    );
    assert!(layout_anchor_from_bounds(&[]).is_err());
    assert!(layout_anchor_from_bounds(&vec![[0.0; 6]; 10_001]).is_err());
    for bounds in [
        [0.0, 0.0, 0.0, f64::NAN, 1.0, 1.0],
        [0.0, 0.0, 0.0, f64::INFINITY, 1.0, 1.0],
        [2.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    ] {
        assert!(layout_anchor_from_bounds(&[bounds]).is_err());
    }
    assert_eq!(
        layout_anchor_from_bounds(&[[-f64::MAX; 6], [f64::MAX; 6]]).unwrap(),
        [0.0; 3]
    );
}

#[test]
fn versioned_layout_anchor_keeps_raw_queries_identity_and_geometry_mandatory() {
    let (plan, mut part) = fixture();
    part["drafting_readback_schema"] = json!("oev2.native-drafting-observation/3");
    part["part_tag"] = json!(50);
    part["solid_tags"] = json!([71]);
    part["bbox"] = json!([[-24, 70, -820, -18, 94, -800]]);
    let view = &mut part["drawing_sheets"][0]["views"][0];
    view["boundary"] = json!({"type_query_code":0,"type_raw":4,"kind":"bound_by_objects",
        "objects_query_code":0,"capture_complete":true,"object_count":1,"object_tags":[71]});
    view["layout_anchor"] = json!({"query_code":0,"point_tag":81,"point_data":{"point_tag":81,
        "point_query_code":0,"coordinates":[-21,82,-810],"smart_query_code":0,"is_smart":true,"smart_raw":1,
        "owner_query_code":0,"owning_part":50,"matches_expected_owner":true,"capture_complete":true}});
    assert_eq!(check(&plan, &part)["conclusion"], "pass");
    for (pointer, value, reason) in [
        (
            "/query_code",
            json!(17),
            "native_drafting_layout_anchor_unavailable",
        ),
        (
            "/point_data/point_query_code",
            json!(17),
            "native_drafting_layout_anchor_unavailable",
        ),
        (
            "/point_data/smart_query_code",
            json!(17),
            "native_drafting_layout_anchor_unavailable",
        ),
        (
            "/point_data/owner_query_code",
            json!(17),
            "native_drafting_layout_anchor_unavailable",
        ),
        (
            "/point_data/smart_raw",
            json!(0),
            "native_drafting_layout_anchor_unavailable",
        ),
        (
            "/point_data/point_tag",
            json!(82),
            "native_drafting_layout_anchor_identity_mismatch",
        ),
        (
            "/point_data/owning_part",
            json!(51),
            "native_drafting_layout_anchor_identity_mismatch",
        ),
        (
            "/point_data/coordinates",
            json!([-21, 82, -809]),
            "native_drafting_layout_anchor_position_mismatch",
        ),
        (
            "/point_data/coordinates",
            Value::Null,
            "native_drafting_layout_anchor_geometry_unavailable",
        ),
    ] {
        let mut bad = part.clone();
        *bad["drawing_sheets"][0]["views"][0]["layout_anchor"]
            .pointer_mut(pointer)
            .unwrap() = value;
        assert_eq!(check(&plan, &bad)["reason"], reason, "{pointer}");
    }
    let mut old = part.clone();
    old["drafting_readback_schema"] = json!("oev2.native-drafting-observation/2");
    old["drawing_sheets"][0]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("layout_anchor");
    assert_eq!(check(&plan, &old)["conclusion"], "pass");
    let mut mapped = part.clone();
    mapped["drafting_readback_schema"] = json!("oev2.native-drafting-observation/4");
    mapped["display_part"] =
        json!({"return_code":0,"expected_part":50,"actual_part":50,"matches_expected":true});
    mapped["drawing_sheets"][0]["display_open_code"] = json!(0);
    mapped["drawing_sheets"][0]["views"][0]["layout_anchor"]["drawing_mapping"] = json!({"return_code":0,
        "capture_complete":true,"coordinates":plan.view_center});
    assert_eq!(check(&plan, &mapped)["conclusion"], "pass");
    for (pointer, value, reason) in [
        (
            "/display_part/return_code",
            json!(17),
            "native_drafting_display_part_unavailable",
        ),
        (
            "/display_part/actual_part",
            json!(51),
            "native_drafting_display_part_unavailable",
        ),
        (
            "/display_part/expected_part",
            json!(51),
            "native_drafting_display_part_unavailable",
        ),
        (
            "/display_part/matches_expected",
            json!(false),
            "native_drafting_display_part_unavailable",
        ),
        (
            "/drawing_sheets/0/display_open_code",
            json!(17),
            "native_drafting_display_sheet_unavailable",
        ),
    ] {
        let mut bad = mapped.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(check(&plan, &bad)["reason"], reason, "{pointer}");
    }
    mapped["drawing_sheets"][0]["views"][0]["layout_anchor"]["drawing_mapping"]["return_code"] =
        json!(17);
    assert_eq!(
        check(&plan, &mapped)["reason"],
        "native_drafting_anchor_mapping_unavailable"
    );
    mapped["drawing_sheets"][0]["views"][0]["layout_anchor"]["drawing_mapping"]["return_code"] =
        json!(0);
    mapped["drawing_sheets"][0]["views"][0]["layout_anchor"]["drawing_mapping"]["coordinates"] =
        json!([1, 1]);
    assert_eq!(
        check(&plan, &mapped)["reason"],
        "native_drafting_anchor_mapping_mismatch"
    );
    part["bbox"] = json!([]);
    assert_eq!(
        check(&plan, &part)["reason"],
        "native_drafting_layout_anchor_geometry_unavailable"
    );
}

#[test]
fn import_coordinates_preserve_final_point_and_reject_unverified_view_frames() {
    use oe_nx_worker::geometry::drafting::layout_import_reference as import;
    let right = [0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
    assert_eq!(
        import([-21.0, 82.0, -810.0], right, [0.0; 3], [150.0, 110.0], 2.0).unwrap(),
        [-14.0, 1730.0]
    );
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    assert_eq!(
        import([3.0, 4.0, 5.0], identity, [0.0; 3], [40.0, 40.0], 0.5).unwrap(),
        [38.5, 38.0]
    );
    assert_eq!(
        import([0.0; 3], identity, [0.0; 3], [40.0, 40.0], 1.0).unwrap(),
        [40.0, 40.0]
    );
    assert!(
        import([0.0; 3], identity, [1.0, 0.0, 0.0], [40.0, 40.0], 1.0)
            .unwrap_err()
            .to_string()
            .contains("nonzero_model_view_center_unsupported")
    );
    let mut bad = identity;
    bad[0] = 2.0;
    assert!(import([0.0; 3], bad, [0.0; 3], [40.0, 40.0], 1.0).is_err());
    bad = identity;
    bad[8] = -1.0;
    assert!(import([0.0; 3], bad, [0.0; 3], [40.0, 40.0], 1.0).is_err());
    assert!(import([f64::NAN; 3], identity, [0.0; 3], [40.0, 40.0], 1.0).is_err());
    assert!(import([0.0; 3], identity, [0.0; 3], [40.0, 40.0], f64::INFINITY).is_err());
    assert!(import([f64::MAX; 3], identity, [0.0; 3], [40.0, 40.0], 2.0).is_err());
}

#[test]
fn centered_model_view_requires_actual_center_and_display_prerequisites() {
    let (plan, mut part) = fixture();
    part["drafting_readback_schema"] = json!("oev2.native-drafting-observation/5");
    part["part_tag"] = json!(50);
    part["solid_tags"] = json!([71]);
    part["bbox"] = json!([[-24, 70, -820, -18, 94, -800]]);
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
    view["layout_anchor"] = json!({"query_code":0,"point_tag":81,"point_data":{"point_tag":81,
        "point_query_code":0,"coordinates":[-21,82,-810],"smart_query_code":0,"is_smart":true,"smart_raw":1,
        "owner_query_code":0,"owning_part":50,"matches_expected_owner":true,"capture_complete":true},
        "drawing_mapping":{"return_code":0,"capture_complete":true,"coordinates":plan.view_center}});
    part["modeling_views"][0]["view_center"] = json!({"query_code":0,"coordinates":[-21,82,-810]});
    assert_eq!(check(&plan, &part)["conclusion"], "pass");
    for (pointer, value, reason) in [
        (
            "/modeling_views/0/view_center/query_code",
            json!(17),
            "native_drafting_model_center_unavailable",
        ),
        (
            "/modeling_views/0/view_center/coordinates",
            json!([0, 0, 0]),
            "native_drafting_model_center_mismatch",
        ),
        (
            "/modeling_views/0/view_center/coordinates",
            Value::Null,
            "native_drafting_model_center_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/set_code",
            json!(17),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/query_code",
            json!(17),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/state_raw",
            json!(1),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/current_drawing_code",
            json!(17),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/current_drawing_tag",
            json!(43),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/work_view_code",
            json!(17),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/work_view_tag",
            json!(0),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/work_view_type/return_code",
            json!(17),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/work_view_type/type_raw",
            json!(0),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/work_view_type/kind",
            json!("model"),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/visible_objects/return_code",
            json!(1170056),
            "native_drafting_view_readback_incomplete",
        ),
    ] {
        let mut bad = part.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(check(&plan, &bad)["reason"], reason, "{pointer}");
    }
    let mut old = part.clone();
    old["drafting_readback_schema"] = json!("oev2.native-drafting-observation/4");
    old["drawing_sheets"][0]
        .as_object_mut()
        .unwrap()
        .remove("display_state");
    old["modeling_views"][0]
        .as_object_mut()
        .unwrap()
        .remove("view_center");
    assert_eq!(check(&plan, &old)["conclusion"], "pass");
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

#[test]
fn direct_layout_reference_has_its_own_protocol_and_preserves_old_anchor_contracts() {
    let (plan, part) = direct_fixture();
    let result = check(&plan, &part);
    assert_eq!(result["conclusion"], "pass");
    assert_eq!(result["engineering_adoption"], "not_established");
    for schema in [
        "oev2.native-drafting-observation/3",
        "oev2.native-drafting-observation/4",
        "oev2.native-drafting-observation/5",
    ] {
        let mut historical = part.clone();
        historical["drafting_readback_schema"] = json!(schema);
        assert_eq!(
            check(&plan, &historical)["reason"],
            "native_drafting_layout_anchor_unavailable"
        );
    }
    // An optional condemned anchor is not dereferenced or used as evidence.
    let mut condemned = part;
    condemned["drawing_sheets"][0]["views"][0]["layout_anchor"] = json!({"query_code":0,"point_tag":81,
        "point_status":{"state":"condemned"},"point_data":{"capture_complete":false}});
    assert_eq!(check(&plan, &condemned)["conclusion"], "pass");
}

#[test]
fn direct_layout_reference_rejects_incomplete_mismatched_or_noncurrent_mapping() {
    let (plan, part) = direct_fixture();
    for (pointer, value, reason) in [
        (
            "/drawing_sheets/0/views/0/layout_reference/schema",
            json!("other"),
            "native_drafting_layout_reference_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/coordinate_basis",
            json!("center_of_mass"),
            "native_drafting_layout_reference_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/unit",
            json!("inch"),
            "native_drafting_layout_reference_unit_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/model_part_tag",
            json!(51),
            "native_drafting_layout_reference_identity_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_view_tag",
            json!(42),
            "native_drafting_layout_reference_identity_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_view_tag",
            json!(0),
            "native_drafting_layout_reference_identity_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/model_coordinates",
            json!([2, 8, 1]),
            "native_drafting_layout_reference_position_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_mapping/model_point",
            json!([0, 0, 0]),
            "native_drafting_layout_reference_mapping_input_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_mapping/model_point",
            Value::Null,
            "native_drafting_layout_reference_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_mapping/return_code",
            json!(17),
            "native_drafting_layout_reference_mapping_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_mapping/capture_complete",
            json!(false),
            "native_drafting_layout_reference_mapping_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_mapping/coordinates",
            json!([151, 110]),
            "native_drafting_layout_reference_mapping_mismatch",
        ),
        (
            "/drawing_sheets/0/views/0/layout_reference/drawing_mapping/coordinates",
            json!([150]),
            "native_drafting_layout_reference_mapping_unavailable",
        ),
        (
            "/bbox",
            json!([]),
            "native_drafting_layout_reference_geometry_unavailable",
        ),
        (
            "/drawing_sheets/0/display_state/state_raw",
            json!(1),
            "native_drafting_display_state_unavailable",
        ),
        (
            "/drawing_sheets/0/views/0/visible_objects/visible_tags",
            json!([]),
            "native_drafting_visible_inventory_inconsistent",
        ),
    ] {
        let mut bad = part.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(check(&plan, &bad)["reason"], reason, "{pointer}");
    }
    let mut count_only = part;
    let visible = count_only["drawing_sheets"][0]["views"][0]["visible_objects"]
        .as_object_mut()
        .unwrap();
    visible.remove("visible_tags");
    visible.remove("clipped_tags");
    assert_eq!(
        check(&plan, &count_only)["reason"],
        "native_drafting_visible_inventory_inconsistent"
    );
}

#[test]
fn direct_layout_contract_errors_remain_unknown_with_specific_diagnostics() {
    let (plan, mut part) = direct_fixture();
    part["drawing_sheets"][0]["views"][0]["layout_reference"]["drawing_mapping"]["coordinate_basis"] =
        json!("undeclared wrapper annotation");
    let result = check(&plan, &part);
    assert_eq!(result["conclusion"], "unknown");
    assert_eq!(
        result["reason"],
        "native_drafting_layout_reference_unavailable"
    );
    assert_eq!(result["details"]["contract_stage"], "deserialize");
    let message = result["details"]["contract_error"].as_str().unwrap();
    assert!(message.contains("unknown field") && message.contains("coordinate_basis"));
    // Removing only an undeclared producer field is a software diagnostic,
    // never a new original native receipt or a save/reopen acceptance.
    part["drawing_sheets"][0]["views"][0]["layout_reference"]["drawing_mapping"]
        .as_object_mut()
        .unwrap()
        .remove("coordinate_basis");
    assert_eq!(check(&plan, &part)["conclusion"], "pass");
    part["drawing_sheets"][0]["views"][0]["layout_reference"]["coordinate_basis"] =
        json!("center_of_mass");
    let invalid = check(&plan, &part);
    assert_eq!(invalid["conclusion"], "unknown");
    assert_eq!(invalid["details"]["contract_stage"], "validate");
    assert!(
        invalid["details"]["contract_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error.as_str().unwrap().contains("coordinate_basis:status"))
    );
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

#[test]
fn sheet_dimension_axial_centers_do_not_require_coplanar_circles() {
    let (sheet, reference, part) = sheet_dimension_fixture();
    let result = oe_nx_worker::geometry::sheet_dimensions::prepare(
        &sheet,
        &reference,
        [100.0, 140.0],
        b"independent-fixture-cad",
        &part,
    )
    .unwrap();
    assert_eq!(result["edge_tags"], json!([101, 102]));
    assert_eq!(result["sheet_tag"], 40);
    assert_eq!(result["view_tag"], 41);
    // Half-scale changes rendered length, not the required model-space dimension.
    assert_eq!(result["projected_model_length"], 32.0);
    assert_eq!(result["native_creation"], "not_performed");
    assert_eq!(result["manufacturing_drawing"], "not_established");
}

#[test]
fn sheet_dimension_rejects_foreshortened_or_wrong_native_view() {
    let (sheet, reference, part) = sheet_dimension_fixture();
    let prepare = |p: &Value| {
        oe_nx_worker::geometry::sheet_dimensions::prepare(
            &sheet,
            &reference,
            [100.0, 140.0],
            b"independent-fixture-cad",
            p,
        )
    };
    let mut top = part.clone();
    let rotation = json!({"return_code":0,"matrix":[1,0,0,0,1,0,0,0,1]});
    top["modeling_views"][0]["rotation"] = rotation.clone();
    top["drawing_sheets"][0]["views"][0]["rotation"] = rotation;
    assert_eq!(
        prepare(&top).unwrap_err().to_string(),
        "sheet_dimension_projected_length_mismatch"
    );
    for (pointer, value) in [
        ("/drawing_sheets/0/views/0/stale_after", json!(true)),
        (
            "/drawing_sheets/0/views/0/owning_drawing/drawing_tag",
            json!(99),
        ),
        (
            "/drawing_sheets/0/metadata/name_query/utf8",
            json!("wrong-sheet"),
        ),
        (
            "/drawing_sheets/0/views/0/visible_objects/visible_tags",
            json!([]),
        ),
    ] {
        let mut bad = part.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            prepare(&bad).unwrap_err().to_string(),
            "sheet_dimension_drawing_plan_unproven",
            "{pointer}"
        );
    }
}

#[test]
fn sheet_dimension_rejects_foreign_ambiguous_missing_and_incomplete_parents() {
    let (sheet, reference, part) = sheet_dimension_fixture();
    let prepare = |p: &Value| {
        oe_nx_worker::geometry::sheet_dimensions::prepare(
            &sheet,
            &reference,
            [100.0, 140.0],
            b"independent-fixture-cad",
            p,
        )
    };
    for (pointer, value) in [
        (
            "/model_edges/entries/0/metadata/owning_part_query/matches_expected_owner",
            json!(false),
        ),
        ("/model_edges/entries/0/query/arc_code", json!(7)),
        ("/model_edges/entries/0/arc/radius", json!(20)),
        ("/model_edges/entries", json!([])),
    ] {
        let mut bad = part.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            prepare(&bad).unwrap_err().to_string(),
            "sheet_dimension_unique_parent_unproven",
            "{pointer}"
        );
    }
    let mut duplicate = part;
    let mut edge = duplicate["model_edges"]["entries"][0].clone();
    edge["edge_tag"] = json!(103);
    duplicate["model_edges"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(edge);
    duplicate["model_edges"]["bodies"][0]["edge_tags"]
        .as_array_mut()
        .unwrap()
        .push(json!(103));
    assert_eq!(
        prepare(&duplicate).unwrap_err().to_string(),
        "sheet_dimension_unique_parent_unproven"
    );
}

#[test]
fn sheet_dimension_rejects_source_identity_and_invalid_intent_before_native_selection() {
    use oe_nx_worker::geometry::sheet_dimensions::{prepare, validate};
    let (sheet, reference, part) = sheet_dimension_fixture();
    assert_eq!(
        prepare(&sheet, &reference, [100.0, 140.0], b"changed-cad", &part)
            .unwrap_err()
            .to_string(),
        "sheet_dimension_source_mismatch"
    );
    for text in [
        [0.0, 100.0],
        [100.0, sheet.height],
        [f64::NAN, 10.0],
        [f64::INFINITY, 10.0],
    ] {
        assert_eq!(
            validate(&sheet, &reference, text).unwrap_err().to_string(),
            "sheet_dimension_text_origin_outside_page"
        );
    }
    let mut wrong = reference.clone();
    wrong.object.revision = "different".into();
    assert_eq!(
        validate(&sheet, &wrong, [10.0, 10.0])
            .unwrap_err()
            .to_string(),
        "sheet_dimension_object_or_unit_mismatch"
    );
    wrong = reference.clone();
    wrong.dimension_value.as_mut().unwrap().value = 31.0;
    assert_eq!(
        validate(&sheet, &wrong, [10.0, 10.0])
            .unwrap_err()
            .to_string(),
        "sheet_dimension_independent_length_mismatch"
    );
    wrong = reference.clone();
    wrong.source_refs.clear();
    assert!(validate(&sheet, &wrong, [10.0, 10.0]).is_err());
    wrong = reference;
    wrong.points[1].point_id = wrong.points[0].point_id.clone();
    assert_eq!(
        validate(&sheet, &wrong, [10.0, 10.0])
            .unwrap_err()
            .to_string(),
        "sheet_dimension_parent_scope_invalid"
    );
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

fn retirement_fixture() -> (oe_contracts::DrawingRetirementIntent, Value, Value) {
    use oe_nx_worker::geometry::drawing_retirement as check;
    let (sheet, reference, _, mut kept) = sheet_dimension_created_fixture();
    kept["view_refresh_requested"] = json!(false);
    let d = &mut kept["annotations"]["dimensions"][0];
    d["text_box_query_code"] = json!(0);
    d["text_box_error"] = Value::Null;
    d["text_boxes"] = json!([{"origin":[0,10,0],"length":5,"height":3,"angle_radians":0,
        "distance":3,"coordinate_basis":"independent fixture model coordinates"}]);
    for (index, entry) in d["associativity_parents"]["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        entry["association_position_valid"] = json!(true);
        entry["association_position"] = json!([0, 0, if index == 0 { -16 } else { 16 }]);
        entry["coordinate_basis"] = json!("independent fixture model coordinates");
        for role in ["object_1", "view"] {
            entry[role]["geometry"] = json!({"called":false});
        }
    }
    let mut obsolete = d.clone();
    obsolete["dimension_tag"] = json!(201);
    obsolete["retained"] = json!(true);
    obsolete["text_boxes"][0]["origin"] = json!([0, -10, 0]);
    obsolete["associativity_parents"]["entries"][0]["object_1"] = json!({"present":false,"tag":0});
    let target = check::signature(&obsolete, "mm").unwrap()["sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut before = kept.clone();
    before["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(obsolete);
    let intent = oe_contracts::DrawingRetirementIntent {
        schema: check::INTENT_SCHEMA.into(),
        object: sheet.object.clone(),
        sheet,
        inventory_signatures: check::inventory(&before).unwrap().into_keys().collect(),
        retire_signatures: vec![target],
        preserve: vec![oe_contracts::DrawingDimensionPreservation {
            reference,
            text_origin_drawing: vec![100.0, 140.0],
        }],
        source_refs: vec!["independent-retirement-fixture:one-obsolete-one-valid".into()],
    };
    (intent, before, kept)
}

fn pdf_fixture() -> (oe_contracts::DrawingPdfIntent, Value) {
    let (old, _, part) = retirement_fixture();
    (
        oe_contracts::DrawingPdfIntent {
            schema: "oev2.drawing-pdf-intent/1".into(),
            sheet: old.sheet,
            inventory_signatures: oe_nx_worker::geometry::drawing_retirement::inventory(&part)
                .unwrap()
                .into_keys()
                .collect(),
            source_refs: vec!["independent-one-dimension-fixture".into()],
        },
        part,
    )
}
fn pdf_call(sheet: u32) -> Value {
    json!({"api":"NXOpen::PrintPDFBuilder","wrapper_code":0,"sheet_tag":sheet,"abi_version":1,
    "display_part_matches":true,"sheet_owner_matches":true,"selected_count":1,"selected_sheet_matches":true,
    "settings_match":true,"validated":true,"commit_entered":true,"commit_returned":true,"builder_destroyed":true})
}
#[test]
fn pdf_source_sheet_inventory_and_call_fail_closed() {
    use oe_nx_worker::geometry::drawing_pdf as p;
    let (i, part) = pdf_fixture();
    let plan = p::prepare(&i, b"independent-fixture-cad", &part).unwrap();
    let tag = plan["sheet_tag"].as_u64().unwrap() as u32;
    assert!(p::prepare(&i, b"other", &part).is_err());
    for field in ["view_refresh_requested", "drawing_catalog_complete"] {
        let mut v = part.clone();
        v[field] = json!(field == "view_refresh_requested");
        assert!(p::prepare(&i, b"independent-fixture-cad", &v).is_err());
    }
    let mut v = part.clone();
    v["annotations"]["dimensions"][0]["retained"] = json!(true);
    assert!(p::prepare(&i, b"independent-fixture-cad", &v).is_err());
    let mut wrong = i.clone();
    wrong.inventory_signatures[0] = "0".repeat(64);
    assert!(p::prepare(&wrong, b"independent-fixture-cad", &part).is_err());
    wrong = i.clone();
    wrong
        .inventory_signatures
        .push(wrong.inventory_signatures[0].clone());
    assert!(p::validate(&wrong).is_err());
    wrong = i.clone();
    wrong.sheet.sheet_name = "other sheet".into();
    assert!(p::prepare(&wrong, b"independent-fixture-cad", &part).is_err());
    let call = pdf_call(tag);
    p::check_call(&call, tag).unwrap();
    for key in [
        "display_part_matches",
        "sheet_owner_matches",
        "selected_sheet_matches",
        "settings_match",
        "validated",
        "commit_entered",
        "commit_returned",
        "builder_destroyed",
    ] {
        let mut bad = call.clone();
        bad[key] = json!(false);
        assert!(p::check_call(&bad, tag).is_err(), "{key}");
    }
    let mut bad = call.clone();
    bad["selected_count"] = json!(2);
    assert!(p::check_call(&bad, tag).is_err());
    assert!(p::check_call(&call, tag + 1).is_err());
}
#[test]
fn pdf_envelope_and_output_binding_are_not_visual_acceptance() {
    use oe_nx_worker::geometry::drawing_pdf as p;
    // Explicitly not a real PDF: this isolates the limited envelope check.
    let bytes = b"%PDF-1.7\nnot parsed content\n%%EOF\n";
    let e = p::envelope(bytes).unwrap();
    assert_eq!(e["PDF_parser_acceptance"], "requires_independent_consumer");
    for bad in [
        b"<html>failure</html>".as_slice(),
        b"%PDF-1.7\nno EOF\n",
        b"%PDF-1.7\n%%EOF\ninjected",
    ] {
        assert!(p::envelope(bad).is_err());
    }
    let (i, part) = pdf_fixture();
    let plan = p::prepare(&i, b"independent-fixture-cad", &part).unwrap();
    let r = json!({"action":"export_drawing_pdf","intent":i,"before":part,"after":part,"output":"drawing.pdf","pdf":e,"export":pdf_call(plan["sheet_tag"].as_u64().unwrap() as u32)});
    p::check_execution(&i, b"independent-fixture-cad", &r, bytes).unwrap();
    assert!(
        p::check_execution(
            &i,
            b"independent-fixture-cad",
            &r,
            b"%PDF-1.7\nchanged content\n%%EOF"
        )
        .is_err()
    );
    let mut v = r.clone();
    v["after"]["annotations"]["dimensions"][0]["text_boxes"][0]["origin"] = json!([1, 10, 0]);
    assert!(p::check_execution(&i, b"independent-fixture-cad", &v, bytes).is_err());
    let mut v = r;
    v["after"]["bbox"] = json!([]);
    assert!(p::check_execution(&i, b"independent-fixture-cad", &v, bytes).is_err());
}
#[test]
fn pdf_protocol_is_explicit() {
    use oe_nx_worker::geometry::Parameters;
    let (i, _) = pdf_fixture();
    let mut value = json!({"action":"export_drawing_pdf","schema":"oev2.nx-drawing-pdf-params/1","part":"input.prt","intent":i});
    let decode =
        |v: &Value| oe_nx_protocol::decode::<Parameters>(&serde_json::to_vec(v).unwrap()).unwrap();
    let p = decode(&value);
    p.validate().unwrap();
    assert_eq!(p.action(), "export_drawing_pdf");
    value["schema"] = json!("cad-agent.nx-native-acceptance-params/v2");
    assert!(decode(&value).validate().is_err());
}
#[test]
fn pdf_execution_is_read_only_and_keeps_partial_failure() {
    use anyhow::Result;
    use oe_nx_worker::geometry::{NativeApi, PartInfo, Workspace, drawing_pdf};
    use std::path::{Path, PathBuf};
    struct Api {
        part: Value,
        reads: usize,
        exports: usize,
        fault: &'static str,
    }
    impl NativeApi for Api {
        fn ensure_empty_session(&mut self) -> Result<()> {
            Ok(())
        }
        fn open_part(&mut self, _: &Path) -> Result<u32> {
            Ok(50)
        }
        fn set_work_part(&mut self, _: u32) -> Result<()> {
            Ok(())
        }
        fn detailed_info(&mut self, _: u32, refresh: bool) -> Result<Value> {
            assert!(!refresh);
            self.reads += 1;
            let mut v = self.part.clone();
            if self.reads == 2 && self.fault == "after_drift" {
                v["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(999);
            }
            Ok(v)
        }
        fn export_drawing_pdf(&mut self, _: u32, sheet: u32, out: &Path) -> Result<Value> {
            assert_eq!(self.reads, 1);
            self.exports += 1;
            std::fs::write(
                out,
                if self.fault == "bad_pdf" {
                    b"not a PDF".as_slice()
                } else {
                    b"%PDF-1.7\nfixture envelope\n%%EOF\n"
                },
            )?;
            let mut c = pdf_call(sheet);
            if self.fault == "sdk_partial" {
                c["commit_returned"] = json!(false);
            }
            Ok(c)
        }
        fn save_work_part(&mut self) -> Result<()> {
            panic!("export must never save")
        }
        fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
            panic!("unexpected")
        }
        fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
            panic!("unexpected")
        }
        fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
            panic!("unexpected")
        }
        fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
            panic!("unexpected")
        }
        fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
            panic!("unexpected")
        }
        fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
            panic!("unexpected")
        }
        fn part_info(&mut self, _: u32) -> Result<PartInfo> {
            panic!("unexpected")
        }
        fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected")
        }
        fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected")
        }
        fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
            panic!("unexpected")
        }
    }
    struct Files {
        root: PathBuf,
        events: Vec<String>,
    }
    impl Workspace for Files {
        fn input(&self, _: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
            Ok(self.root.join("input.prt"))
        }
        fn output(&self, n: &str) -> Result<PathBuf> {
            Ok(self.root.join("out").join(n))
        }
        fn require_regular_output(&self, n: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.output(n)?)?)
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("unexpected")
        }
        fn record(&mut self, k: &str, _: Value) -> Result<()> {
            self.events.push(k.into());
            Ok(())
        }
    }
    for fault in [
        "none",
        "source",
        "existing",
        "sdk_partial",
        "bad_pdf",
        "after_drift",
    ] {
        let (i, part) = pdf_fixture();
        let temp = tempfile::tempdir().unwrap();
        let source = if fault == "source" {
            b"wrong!".as_slice()
        } else {
            b"independent-fixture-cad"
        };
        std::fs::write(temp.path().join("input.prt"), source).unwrap();
        std::fs::create_dir(temp.path().join("out")).unwrap();
        if fault == "existing" {
            std::fs::write(temp.path().join("out/drawing.pdf"), "existing").unwrap();
        }
        let mut files = Files {
            root: temp.path().into(),
            events: vec![],
        };
        let mut api = Api {
            part,
            reads: 0,
            exports: 0,
            fault,
        };
        let result = drawing_pdf::execute(&mut api, &mut files, "copy.prt", &i);
        assert_eq!(result.is_ok(), fault == "none", "{fault}: {result:?}");
        assert_eq!(
            api.exports,
            usize::from(!["source", "existing"].contains(&fault))
        );
        assert_eq!(
            std::fs::read(temp.path().join("input.prt")).unwrap(),
            source
        );
        if api.exports == 1 {
            assert!(temp.path().join("out/drawing.pdf").is_file());
            assert!(files.events.contains(&"drawing_pdf_export_returned".into()));
            assert_eq!(
                std::fs::read(temp.path().join("out/copy.prt")).unwrap(),
                source
            );
        }
    }
}

#[test]
fn retirement_source_inventory_and_preservation_are_mandatory() {
    use oe_nx_worker::geometry::drawing_retirement as check;
    let (intent, before, _) = retirement_fixture();
    let source = b"independent-fixture-cad";
    let result = check::prepare(&intent, source, &before).unwrap();
    assert_eq!(result["targets"][0]["tag"], 201);
    assert_eq!(result["preserved"].as_array().unwrap().len(), 1);
    assert_eq!(
        check::prepare(&intent, b"wrong-original", &before)
            .unwrap_err()
            .to_string(),
        "retirement_source_mismatch"
    );
    let mut wrong = intent.clone();
    wrong.inventory_signatures[0] = "a".repeat(64);
    assert!(check::prepare(&wrong, source, &before).is_err());
    let mut wrong = intent.clone();
    wrong.retire_signatures = vec![
        check::signature(&before["annotations"]["dimensions"][0], "mm").unwrap()["sha256"]
            .as_str()
            .unwrap()
            .into(),
    ];
    assert_eq!(
        check::prepare(&wrong, source, &before)
            .unwrap_err()
            .to_string(),
        "retirement_target_is_not_retained"
    );
    for (pointer, value, reason) in [
        (
            "/annotations/dimensions/1/retained",
            Value::Null,
            "retirement_retained_unknown",
        ),
        (
            "/annotations/dimensions/1/computed_size/metadata/owning_part_query/matches_expected_owner",
            json!(false),
            "retirement_owner_or_type_unknown",
        ),
        (
            "/annotations/dimensions/1/associativity_parents/capture_complete",
            json!(false),
            "retirement_associations_unknown",
        ),
    ] {
        let mut bad = before.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            check::prepare(&intent, source, &bad)
                .unwrap_err()
                .to_string(),
            reason
        );
    }
    let mut duplicate = before.clone();
    let mut same = before["annotations"]["dimensions"][1].clone();
    same["dimension_tag"] = json!(202);
    duplicate["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(same);
    assert_eq!(
        check::inventory(&duplicate).unwrap_err().to_string(),
        "retirement_ambiguous_signature"
    );
    let mut missing = before.clone();
    missing["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(
        check::prepare(&intent, source, &missing)
            .unwrap_err()
            .to_string(),
        "retirement_inventory_mismatch"
    );
    // Even a newly frozen inventory cannot validate an incorrect intended parent.
    let mut wrong_parent = before.clone();
    wrong_parent["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]["object_1"]
        ["tag"] = json!(999);
    assert_eq!(
        check::prepare(&intent, source, &wrong_parent)
            .unwrap_err()
            .to_string(),
        "retirement_preserved_dimension_unproven"
    );
}

#[test]
fn retirement_signature_ignores_only_transient_identity() {
    use oe_nx_worker::geometry::drawing_retirement as check;
    let (_, before, _) = retirement_fixture();
    let d = &before["annotations"]["dimensions"][0];
    let original = check::signature(d, "mm").unwrap();
    let mut renumbered = d.clone();
    renumbered["dimension_tag"] = json!(300);
    renumbered["computed_size"]["metadata"]["name_query"] = json!({"utf8":"another process name"});
    renumbered["associativity_parents"]["entries"][0]["object_1"]["tag"] = json!(800);
    assert_eq!(check::signature(&renumbered, "mm").unwrap(), original);
    for (pointer, value) in [
        ("/text_boxes/0/origin/0", json!(0.00001)),
        ("/computed_size/value", json!(31.99999)),
        ("/retained", json!(true)),
    ] {
        let mut changed = d.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert_ne!(
            check::signature(&changed, "mm").unwrap(),
            original,
            "{pointer}"
        );
    }
    let mut incomplete = d.clone();
    incomplete["text_box_query_code"] = json!(1);
    assert!(check::signature(&incomplete, "mm").is_err());
}

#[test]
fn retirement_progress_and_reopen_require_exact_remaining_inventory() {
    use oe_nx_worker::geometry::drawing_retirement as check;
    let (intent, before, kept) = retirement_fixture();
    let source = b"independent-fixture-cad";
    let removed = intent.retire_signatures.iter().cloned().collect();
    check::check_progress(&intent, source, &before, &kept, &removed).unwrap();
    check::check_reopened(&intent, b"new-saved-source", &kept, &kept).unwrap();
    assert_eq!(
        check::check_progress(&intent, source, &before, &before, &removed)
            .unwrap_err()
            .to_string(),
        "retirement_remaining_inventory_mismatch"
    );
    let mut missing = kept.clone();
    missing["annotations"]["dimensions"] = json!([]);
    assert!(check::check_progress(&intent, source, &before, &missing, &removed).is_err());
    let mut changed = kept.clone();
    changed["bbox"] = json!([1, 2, 3, 4, 5, 6]);
    assert!(check::check_progress(&intent, source, &before, &changed, &removed).is_err());
    let mut changed = kept.clone();
    changed["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
    assert_eq!(
        check::check_progress(&intent, source, &before, &changed, &removed)
            .unwrap_err()
            .to_string(),
        "retirement_remaining_inventory_mismatch"
    );
    assert_eq!(
        check::check_reopened(&intent, source, &kept, &changed)
            .unwrap_err()
            .to_string(),
        "retirement_reopened_inventory_mismatch"
    );
    let mut changed = kept.clone();
    changed["view_refresh_requested"] = json!(true);
    assert_eq!(
        check::check_reopened(&intent, source, &kept, &changed)
            .unwrap_err()
            .to_string(),
        "retirement_reopen_refresh_forbidden"
    );
}

#[test]
fn retirement_protocol_is_explicit_and_legacy_unchanged() {
    use oe_nx_worker::geometry::{Parameters, drawing_retirement as check};
    let (intent, _, _) = retirement_fixture();
    let params = Parameters::RetireDrawingDimensions {
        schema: check::PARAMS_SCHEMA.into(),
        part: "shaft.prt".into(),
        intent: intent.clone(),
    };
    params.validate().unwrap();
    assert_eq!(params.action(), "retire_drawing_dimensions");
    let mut raw = json!({"schema":check::PARAMS_SCHEMA,"action":"retire_drawing_dimensions","part":"shaft.prt","intent":intent});
    raw["schema"] = json!("oev2.nx-geometry-params/1");
    let wrong: Parameters = oe_nx_protocol::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(wrong.validate().is_err());
    raw["schema"] = json!(check::PARAMS_SCHEMA);
    raw["action"] = json!("maintain_drawings");
    assert!(oe_nx_protocol::decode::<Parameters>(&serde_json::to_vec(&raw).unwrap()).is_err());
}

#[test]
fn retirement_execution_orders_checks_and_preserves_failures() {
    use anyhow::Result;
    use oe_nx_worker::geometry::{NativeApi, Parameters, PartInfo, Workspace, execute};
    use std::path::{Path, PathBuf};
    struct Api {
        before: Value,
        after: Value,
        saved: Value,
        reads: usize,
        saves: usize,
        deletes: usize,
        fault: &'static str,
    }
    impl NativeApi for Api {
        fn ensure_empty_session(&mut self) -> Result<()> {
            Ok(())
        }
        fn open_part(&mut self, _: &Path) -> Result<u32> {
            Ok(50)
        }
        fn set_work_part(&mut self, p: u32) -> Result<()> {
            assert_eq!(p, 50);
            Ok(())
        }
        fn detailed_info(&mut self, _: u32, refresh: bool) -> Result<Value> {
            assert!(!refresh);
            self.reads += 1;
            Ok(match self.reads {
                1 => self.before.clone(),
                2 => self.after.clone(),
                3 => self.saved.clone(),
                _ => panic!("unexpected read"),
            })
        }
        fn retire_retained_dimension(&mut self, part: u32, dimension: u32) -> Result<Value> {
            assert_eq!((part, dimension, self.reads, self.saves), (50, 201, 1, 0));
            self.deletes += 1;
            Ok(json!({"api":"UF_OBJ_delete_object","dimension_tag":201,
                "return_code":if self.fault=="delete_return" {7} else {0},
                "after_lifecycle":{"status_observed":true,"status":if self.fault=="still_alive" {"alive"} else {"deleted"}}}))
        }
        fn save_work_part(&mut self) -> Result<()> {
            assert_eq!(self.reads, 2);
            self.saves += 1;
            Ok(())
        }
        fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
            panic!("unexpected operation")
        }
        fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
            panic!("unexpected operation")
        }
        fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
            panic!("unexpected operation")
        }
        fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
            panic!("unexpected operation")
        }
        fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
            panic!("unexpected operation")
        }
        fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
            panic!("unexpected operation")
        }
        fn part_info(&mut self, _: u32) -> Result<PartInfo> {
            panic!("unexpected operation")
        }
        fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected operation")
        }
        fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected operation")
        }
        fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
            panic!("unexpected operation")
        }
    }
    struct Files {
        root: PathBuf,
        events: Vec<String>,
    }
    impl Workspace for Files {
        fn input(&self, _: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
            Ok(self.root.join("input.prt"))
        }
        fn output(&self, name: &str) -> Result<PathBuf> {
            Ok(self.root.join("out").join(name))
        }
        fn require_regular_output(&self, name: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.root.join("out").join(name))?)
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("unexpected operation")
        }
        fn record(&mut self, kind: &str, _: Value) -> Result<()> {
            self.events.push(kind.into());
            Ok(())
        }
    }
    for fault in [
        "none",
        "source",
        "inventory",
        "delete_return",
        "still_alive",
        "not_removed",
        "postsave",
    ] {
        let (intent, before, kept) = retirement_fixture();
        let mut api = Api {
            before,
            after: kept.clone(),
            saved: kept,
            reads: 0,
            saves: 0,
            deletes: 0,
            fault,
        };
        if fault == "inventory" {
            api.before["annotations"]["dimensions"]
                .as_array_mut()
                .unwrap()
                .pop();
        }
        if fault == "not_removed" {
            api.after = api.before.clone();
        }
        if fault == "postsave" {
            api.saved["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("out")).unwrap();
        let source: &[u8] = if fault == "source" {
            b"incorrect source"
        } else {
            b"independent-fixture-cad"
        };
        std::fs::write(root.path().join("input.prt"), source).unwrap();
        let mut files = Files {
            root: root.path().to_owned(),
            events: Vec::new(),
        };
        let params = Parameters::RetireDrawingDimensions {
            schema: "oev2.nx-drawing-retirement-params/1".into(),
            part: "shaft.prt".into(),
            intent,
        };
        let result = execute(&mut api, &mut files, &params);
        if fault == "none" {
            assert_eq!(result.unwrap()["check"]["conclusion"], "pass");
        } else {
            assert_eq!(
                result.unwrap_err().to_string(),
                match fault {
                    "source" => "retirement_source_mismatch",
                    "inventory" => "retirement_inventory_mismatch",
                    "delete_return" | "still_alive" => "retirement_native_delete_failed",
                    _ => "retirement_remaining_inventory_mismatch",
                }
            );
        }
        assert_eq!(
            api.deletes,
            usize::from(!["source", "inventory"].contains(&fault))
        );
        assert_eq!(
            api.saves,
            usize::from(["none", "postsave"].contains(&fault))
        );
        if api.deletes > 0 {
            assert!(
                files
                    .events
                    .iter()
                    .position(|s| s == "drawing_retirement_attempt")
                    .unwrap()
                    < files
                        .events
                        .iter()
                        .position(|s| s == "drawing_retirement_returned")
                        .unwrap()
            );
        }
        assert_eq!(
            std::fs::read(root.path().join("input.prt")).unwrap(),
            source
        );
    }
}

#[test]
fn sheet_dimension_native_check_requires_actual_value_and_member_view() {
    use oe_nx_worker::geometry::sheet_dimensions::{check_addition, check_created};
    let (sheet, reference, before, after) = sheet_dimension_created_fixture();
    let check = |p: &Value| {
        check_created(
            &sheet,
            &reference,
            [100.0, 140.0],
            b"independent-fixture-cad",
            p,
            200,
        )
    };
    let result = check(&after).unwrap();
    assert_eq!(result["conclusion"], "pass");
    assert_eq!(result["manufacturing_drawing"], "not_established");
    check_addition(&before, &after, 200).unwrap();
    for (pointer, value, reason) in [
        (
            "/annotations/dimensions/0/computed_size/value",
            json!(31),
            "sheet_dimension_computed_value_mismatch",
        ),
        (
            "/annotations/dimensions/0/computed_size/unit",
            json!("inch"),
            "sheet_dimension_computed_value_unavailable",
        ),
        (
            "/annotations/dimensions/0/computed_size/metadata/owning_part_query/matches_expected_owner",
            json!(false),
            "sheet_dimension_computed_value_unavailable",
        ),
        (
            "/annotations/dimensions/0/associativity_parents/entries/0/view/tag",
            json!(42),
            "sheet_dimension_member_view_mismatch",
        ),
        (
            "/annotations/dimensions/0/associativity_parents/entries/0/view/present",
            json!(false),
            "sheet_dimension_member_view_mismatch",
        ),
        (
            "/annotations/dimensions/0/associativity_parents/entries/0/object_1/tag",
            json!(999),
            "sheet_dimension_unique_parents_differ",
        ),
        (
            "/annotations/dimensions/0/associativity_parents/entries/0/object_1/metadata/owning_part_query/matches_expected_owner",
            json!(false),
            "sheet_dimension_parent_association_mismatch",
        ),
        (
            "/annotations/dimensions/0/associativity_parents/entries/0/point_type_raw",
            json!(1),
            "sheet_dimension_parent_association_mismatch",
        ),
        (
            "/annotations/dimensions/0/associativity_parents/uf_return_code",
            json!(17),
            "sheet_dimension_associations_unavailable",
        ),
        (
            "/annotations/dimensions/0/retained",
            json!(true),
            "sheet_dimension_retained_or_unknown",
        ),
    ] {
        let mut bad = after.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(check(&bad).unwrap_err().to_string(), reason, "{pointer}");
    }
}

#[test]
fn sheet_dimension_addition_preserves_geometry_and_prior_annotation_inventory() {
    use oe_nx_worker::geometry::sheet_dimensions::check_addition;
    let (_, _, mut before, mut after) = sheet_dimension_created_fixture();
    let mut existing = after["annotations"]["dimensions"][0].clone();
    existing["dimension_tag"] = json!(199);
    before["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(existing.clone());
    assert_eq!(
        check_addition(&before, &after, 200)
            .unwrap_err()
            .to_string(),
        "sheet_dimension_annotation_addition_mismatch"
    );
    after["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(existing);
    check_addition(&before, &after, 200).unwrap();
    for field in ["bbox", "solid_tags", "model_edges"] {
        let mut bad = after.clone();
        bad[field] = Value::Null;
        assert_eq!(
            check_addition(&before, &bad, 200).unwrap_err().to_string(),
            "sheet_dimension_original_geometry_changed"
        );
    }
    assert_eq!(
        check_addition(&before, &after, 199)
            .unwrap_err()
            .to_string(),
        "sheet_dimension_created_tag_not_new"
    );
    let duplicate = after["annotations"]["dimensions"][0].clone();
    after["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert_eq!(
        check_addition(&before, &after, 200)
            .unwrap_err()
            .to_string(),
        "sheet_dimension_inventory_invalid"
    );
}

#[test]
fn sheet_dimension_action_has_explicit_protocol_and_preserves_model_action() {
    use oe_nx_worker::geometry::Parameters;
    let (sheet, reference, _) = sheet_dimension_fixture();
    let value = json!({"schema":"oev2.nx-sheet-dimension-params/1","action":"create_sheet_reference_dimension",
        "part":"shaft.prt","sheet":sheet,"reference":reference,"text_origin_drawing":[100.0,140.0]});
    let params: Parameters = serde_json::from_value(value.clone()).unwrap();
    params.validate().unwrap();
    assert_eq!(params.action(), "create_sheet_reference_dimension");
    assert_eq!(
        params.parameter_schema(),
        "oev2.nx-sheet-dimension-params/1"
    );
    let mut old = value.clone();
    old["schema"] = json!(oe_nx_worker::geometry::PARAMS_SCHEMA);
    assert_eq!(
        serde_json::from_value::<Parameters>(old)
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string(),
        "sheet_dimension_params_schema_mismatch"
    );
    let mut legacy = value;
    legacy["action"] = json!("create_reference_dimension");
    assert!(serde_json::from_value::<Parameters>(legacy).is_err());
}

#[test]
fn sheet_dimension_execution_checks_before_save_and_rejects_postsave_drift() {
    use anyhow::Result;
    use oe_nx_worker::geometry::{NativeApi, Parameters, PartInfo, Workspace, execute};
    use std::path::{Path, PathBuf};
    struct Api {
        before: Value,
        after: Value,
        saved: Value,
        reads: usize,
        saves: usize,
        complete: bool,
    }
    impl NativeApi for Api {
        fn ensure_empty_session(&mut self) -> Result<()> {
            Ok(())
        }
        fn open_part(&mut self, _: &Path) -> Result<u32> {
            Ok(50)
        }
        fn set_work_part(&mut self, p: u32) -> Result<()> {
            assert_eq!(p, 50);
            Ok(())
        }
        fn detailed_info(&mut self, _: u32, refresh: bool) -> Result<Value> {
            assert!(!refresh);
            self.reads += 1;
            Ok(match self.reads {
                1 => self.before.clone(),
                2 => self.after.clone(),
                3 => self.saved.clone(),
                _ => panic!("unexpected read"),
            })
        }
        fn create_sheet_parallel_reference(
            &mut self,
            sheet: u32,
            view: u32,
            edges: [u32; 2],
            text: [f64; 2],
        ) -> Result<Value> {
            assert_eq!(
                (sheet, view, edges, text),
                (40, 41, [101, 102], [100.0, 140.0])
            );
            Ok(json!({"complete":self.complete,"dimension_tag":200}))
        }
        fn save_work_part(&mut self) -> Result<()> {
            self.saves += 1;
            Ok(())
        }
        fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
            panic!("unexpected CAD operation")
        }
        fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
            panic!("unexpected CAD operation")
        }
        fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn part_info(&mut self, _: u32) -> Result<PartInfo> {
            panic!("unexpected CAD operation")
        }
        fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
            panic!("unexpected CAD operation")
        }
    }
    struct Files(PathBuf);
    impl Workspace for Files {
        fn input(&self, _: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
            Ok(self.0.join("input.prt"))
        }
        fn output(&self, name: &str) -> Result<PathBuf> {
            Ok(self.0.join("out").join(name))
        }
        fn require_regular_output(&self, name: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.0.join("out").join(name))?)
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("unexpected optional output")
        }
        fn record(&mut self, _: &str, _: Value) -> Result<()> {
            Ok(())
        }
    }
    for fault in ["none", "create", "value", "saved_value"] {
        let (sheet, reference, before, after) = sheet_dimension_created_fixture();
        let mut api = Api {
            before,
            after: after.clone(),
            saved: after,
            reads: 0,
            saves: 0,
            complete: fault != "create",
        };
        if fault == "value" {
            api.after["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
        }
        if fault == "saved_value" {
            api.saved["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("out")).unwrap();
        std::fs::write(root.path().join("input.prt"), b"independent-fixture-cad").unwrap();
        let mut files = Files(root.path().to_owned());
        let params = Parameters::CreateSheetReferenceDimension {
            schema: "oev2.nx-sheet-dimension-params/1".into(),
            part: "shaft.prt".into(),
            sheet,
            reference,
            text_origin_drawing: [100.0, 140.0],
        };
        let result = execute(&mut api, &mut files, &params);
        if fault == "none" {
            assert_eq!(
                result.unwrap()["independent_reopen"],
                "required_separate_process"
            );
            assert_eq!(api.saves, 1);
        } else {
            let reason = result.unwrap_err().to_string();
            assert_eq!(
                reason,
                if fault == "create" {
                    "sheet_dimension_native_creation_incomplete"
                } else {
                    "sheet_dimension_computed_value_mismatch"
                }
            );
            assert_eq!(api.saves, usize::from(fault == "saved_value"));
        }
        assert_eq!(
            std::fs::read(root.path().join("input.prt")).unwrap(),
            b"independent-fixture-cad"
        );
    }
}

fn identified_fixture() -> (oe_contracts::DrawingIdentifiedPdfIntent, Value) {
    let (old, _, mut part) = retirement_fixture();
    let (base, _) = pdf_fixture();
    let mut refs = Vec::new();
    part["bbox"] = json!([[-4, -10, 0, 40, 25, 10]]);
    part["drawing_sheets"][0]["views"][0]["layout_reference"]["model_coordinates"] =
        json!([18, 7.5, 5]);
    part["drawing_sheets"][0]["views"][0]["layout_reference"]["drawing_mapping"]["model_point"] =
        json!([18, 7.5, 5]);
    let rotation = json!({"return_code":0,"matrix":[1,0,0,0,1,0,0,0,1]});
    part["modeling_views"][0]["rotation"] = rotation.clone();
    part["drawing_sheets"][0]["views"][0]["rotation"] = rotation;
    let mut edges = Vec::new();
    let mut dims = Vec::new();
    for (j, (surface, z)) in [("upper", 10.0), ("lower", 0.0)].into_iter().enumerate() {
        let mut dim = old.preserve[0].clone();
        dim.reference.intent_id = format!("{surface}-centers");
        let mut d = part["annotations"]["dimensions"][0].clone();
        d["dimension_tag"] = json!(200 + j);
        for k in 0..2 {
            let pos = vec![32.0 * k as f64, 0.0, z];
            dim.reference.points[k].position = pos.clone();
            dim.reference.points[k].parent_edge.as_mut().unwrap().center = pos.clone();
            let tag = 101 + j * 2 + k;
            let mut e = part["model_edges"]["entries"][k].clone();
            e["edge_tag"] = json!(tag);
            e["arc"]["center"] = json!(pos);
            edges.push(e);
            d["associativity_parents"]["entries"][k]["object_1"]["tag"] = json!(tag);
            d["associativity_parents"]["entries"][k]["association_position"] = json!(pos);
        }
        refs.push(oe_contracts::DrawingSurfaceReference {
            surface: surface.into(),
            dimension: dim,
        });
        dims.push(d);
    }
    part["model_edges"]["entries"] = json!(edges);
    part["model_edges"]["bodies"][0]["edge_tags"] = json!([101, 102, 103, 104]);
    part["annotations"]["dimensions"] = json!(dims);
    let mut i = oe_contracts::DrawingIdentifiedPdfIntent {
        schema: "oev2.drawing-identified-pdf-intent/1".into(),
        base,
        references: refs,
    };
    i.base.inventory_signatures = oe_nx_worker::geometry::drawing_retirement::inventory(&part)
        .unwrap()
        .into_keys()
        .collect();
    (i, part)
}
fn label_observation(tag: u32, label: Option<&str>) -> Value {
    let s = label.unwrap_or("");
    json!({"api":"NXOpen::Annotations::Dimension::AppendedText","wrapper_code":0,"abi_version":1,"dimension_tag":tag,"owner_matches":true,"dimension_type_matches":true,"counts":[0,0,0,if s.is_empty(){0}else{1}],"below":s,"write_returned":label.is_some()})
}
fn wcs_observation(visible: bool, write: bool) -> Value {
    json!({"api":"NXOpen::WCS::Visibility","wrapper_code":0,"abi_version":1,"display_part_matches":true,"visible":visible,"write_returned":write})
}
#[test]
fn identified_pdf_rejects_wrong_surface_or_incomplete_coverage() {
    use oe_nx_worker::geometry::{Parameters, drawing_identification as m};
    let (i, p) = identified_fixture();
    let plan = m::prepare(&i, b"independent-fixture-cad", &p).unwrap();
    assert_eq!(
        plan["targets"][0]["label"],
        "UPPER CIRCLE CENTERS (MODEL Z=10 mm)"
    );
    assert_eq!(
        plan["targets"][1]["label"],
        "LOWER CIRCLE CENTERS (MODEL Z=0 mm)"
    );
    let mut bad = i.clone();
    bad.references[0].surface = "lower".into();
    assert_eq!(
        m::prepare(&bad, b"independent-fixture-cad", &p)
            .unwrap_err()
            .to_string(),
        "identified_pdf_surface_mismatch"
    );
    bad = i.clone();
    bad.references.pop();
    assert!(m::prepare(&bad, b"independent-fixture-cad", &p).is_err());
    bad = i.clone();
    bad.references[1].dimension = bad.references[0].dimension.clone();
    assert!(m::prepare(&bad, b"independent-fixture-cad", &p).is_err());
    assert!(m::prepare(&i, b"wrong", &p).is_err());
    let mut wrong = p;
    wrong["solid_count"] = json!(2);
    assert!(m::prepare(&i, b"independent-fixture-cad", &wrong).is_err());
    let mut wire = json!({"action":"export_identified_drawing_pdf","schema":m::PARAMS_SCHEMA,"part":"input.prt","intent":i});
    let v: Parameters = serde_json::from_value(wire.clone()).unwrap();
    v.validate().unwrap();
    wire["schema"] = json!("oev2.nx-drawing-pdf-params/1");
    assert!(
        serde_json::from_value::<Parameters>(wire)
            .unwrap()
            .validate()
            .is_err()
    );
}
fn identified_report() -> (oe_contracts::DrawingIdentifiedPdfIntent, Value) {
    use oe_nx_worker::geometry::drawing_identification as m;
    let (i, p) = identified_fixture();
    let plan = m::prepare(&i, b"independent-fixture-cad", &p).unwrap();
    let mut before = Vec::new();
    let mut written = Vec::new();
    let mut restored = Vec::new();
    for t in plan["targets"].as_array().unwrap() {
        let tag = t["dimension_tag"].as_u64().unwrap() as u32;
        before.push(label_observation(tag, None));
        written.push(label_observation(tag, t["label"].as_str()));
        restored.push(label_observation(tag, Some("")));
    }
    let pdf = oe_nx_worker::geometry::drawing_pdf::envelope(b"%PDF-1.7\nfixture envelope\n%%EOF\n")
        .unwrap();
    let r = json!({"action":"export_identified_drawing_pdf","intent":i,"input":"input.prt","output":"drawing.pdf","native_save_called":false,"plan":plan,"before":p,"decorated":p,"after":p,"labels_before":before,"labels_written":written,"labels_restored":restored,"wcs_before":wcs_observation(true,false),"wcs_hidden":wcs_observation(false,true),"wcs_restored":wcs_observation(true,true),"export":pdf_call(40),"pdf":pdf});
    (i, r)
}
#[test]
fn identified_pdf_receiving_recomputes_readback() {
    use oe_nx_worker::geometry::drawing_identification as m;
    let (i, r) = identified_report();
    let pdf = b"%PDF-1.7\nfixture envelope\n%%EOF\n";
    let check = m::check_execution(&i, b"independent-fixture-cad", &r, pdf).unwrap();
    assert_eq!(check["visual_legibility"], "not_assessed");
    for (ptr, v) in [
        ("/labels_written/0/below", json!("LOWER")),
        ("/labels_before/0/counts", json!([1, 0, 0, 0])),
        ("/labels_restored/1/wrapper_code", json!(7)),
        ("/wcs_hidden/visible", json!(true)),
        ("/wcs_before/visible", Value::Null),
        ("/wcs_restored/visible", json!(false)),
        (
            "/decorated/annotations/dimensions/0/computed_size/value",
            json!(99),
        ),
        ("/after/bbox", json!([])),
        ("/native_save_called", json!(true)),
    ] {
        let mut bad = r.clone();
        *bad.pointer_mut(ptr).unwrap() = v;
        assert!(
            m::check_execution(&i, b"independent-fixture-cad", &bad, pdf).is_err(),
            "{ptr}"
        );
    }
    assert!(
        m::check_execution(
            &i,
            b"independent-fixture-cad",
            &r,
            b"%PDF-1.7\nreplacement\n%%EOF\n"
        )
        .is_err()
    );
}
#[test]
fn identified_pdf_restores_after_partial_failure() {
    use anyhow::Result;
    use oe_nx_worker::geometry::{NativeApi, PartInfo, Workspace, drawing_identification};
    use std::path::{Path, PathBuf};
    struct Api {
        part: Value,
        labels: std::collections::BTreeMap<u32, String>,
        visible: bool,
        exports: usize,
        fault: &'static str,
    }
    impl NativeApi for Api {
        fn ensure_empty_session(&mut self) -> Result<()> {
            Ok(())
        }
        fn open_part(&mut self, _: &Path) -> Result<u32> {
            Ok(50)
        }
        fn set_work_part(&mut self, _: u32) -> Result<()> {
            Ok(())
        }
        fn detailed_info(&mut self, _: u32, refresh: bool) -> Result<Value> {
            assert!(!refresh);
            Ok(self.part.clone())
        }
        fn dimension_label(&mut self, _: u32, tag: u32, label: Option<&str>) -> Result<Value> {
            if let Some(v) = label {
                self.labels.insert(tag, v.into());
                if tag == 201 && !v.is_empty() && self.fault == "second_label" {
                    anyhow::bail!("injected partial mutation");
                }
            }
            let mut r = label_observation(tag, label);
            if self.fault == "restore" && label == Some("") {
                r["wrapper_code"] = json!(7);
            }
            Ok(r)
        }
        fn drawing_wcs(&mut self, _: u32, v: Option<bool>) -> Result<Value> {
            if let Some(v) = v {
                self.visible = v;
            }
            Ok(wcs_observation(self.visible, v.is_some()))
        }
        fn export_drawing_pdf(&mut self, _: u32, sheet: u32, out: &Path) -> Result<Value> {
            assert!(!self.visible);
            assert_eq!(self.labels.len(), 2);
            assert!(self.labels.values().all(|s| !s.is_empty()));
            self.exports += 1;
            std::fs::write(out, b"%PDF-1.7\nfixture envelope\n%%EOF\n")?;
            if self.fault == "export" {
                anyhow::bail!("partial export");
            }
            Ok(pdf_call(sheet))
        }
        fn save_work_part(&mut self) -> Result<()> {
            panic!("never save")
        }
        fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
            panic!("unexpected")
        }
        fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
            panic!("unexpected")
        }
        fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
            panic!("unexpected")
        }
        fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
            panic!("unexpected")
        }
        fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
            panic!("unexpected")
        }
        fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
            panic!("unexpected")
        }
        fn part_info(&mut self, _: u32) -> Result<PartInfo> {
            panic!("unexpected")
        }
        fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected")
        }
        fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected")
        }
        fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
            panic!("unexpected")
        }
    }
    struct Files {
        root: PathBuf,
        events: Vec<String>,
    }
    impl Workspace for Files {
        fn input(&self, _: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
            Ok(self.root.join("input.prt"))
        }
        fn output(&self, n: &str) -> Result<PathBuf> {
            Ok(self.root.join("out").join(n))
        }
        fn require_regular_output(&self, n: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.output(n)?)?)
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("unexpected")
        }
        fn record(&mut self, k: &str, _: Value) -> Result<()> {
            self.events.push(k.into());
            Ok(())
        }
    }

    for fault in ["none", "second_label", "export", "restore"] {
        let (i, part) = identified_fixture();
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("input.prt"), b"independent-fixture-cad").unwrap();
        std::fs::create_dir(tmp.path().join("out")).unwrap();
        let mut files = Files {
            root: tmp.path().into(),
            events: vec![],
        };
        let mut api = Api {
            part,
            labels: Default::default(),
            visible: true,
            exports: 0,
            fault,
        };
        let r = drawing_identification::execute(&mut api, &mut files, "copy.prt", &i);
        assert_eq!(r.is_ok(), fault == "none", "{fault}: {r:?}");
        assert!(api.visible);
        assert_eq!(api.labels.len(), 2);
        assert!(api.labels.values().all(|s| s.is_empty()));
        assert_eq!(api.exports, usize::from(fault != "second_label"));
        assert!(files.events.contains(&"identified_pdf_restoration".into()));
        assert_eq!(
            std::fs::read(tmp.path().join("out/copy.prt")).unwrap(),
            b"independent-fixture-cad"
        );
        if api.exports > 0 {
            assert!(tmp.path().join("out/drawing.pdf").is_file());
        }
    }
}

fn display_coordinate() -> Value {
    json!({"tag":700,"wrapper_code":0,"abi_version":1,"alive":true,"owner_matches":true,"owner_tag":50,"type_raw":45,"subtype_raw":0,"is_csys":true,"is_wcs":false,"wcs_tag":701,"csys_complete":true,"display_complete":true,"blank":false,"matrix_tag":702,"origin":[0,0,0],"matrix":[1,0,0,0,1,0,0,0,1],"layer":1,"color":1,"font":1,"line_width":1,"write_returned":false})
}
fn display_inventory() -> Value {
    json!({"schema":"oev2.native-drawing-display/1","capture_complete":true,"view_tag":30,"part_tag":50,"coordinate_systems":[display_coordinate()],"objects":[],"wcs_tag":701})
}
fn display_fixture() -> (oe_contracts::DrawingDisplayPdfIntent, Value) {
    let (base, p) = identified_fixture();
    (
        oe_contracts::DrawingDisplayPdfIntent {
            schema: "oev2.drawing-display-pdf-intent/1".into(),
            base,
            hidden_csys: vec![oe_contracts::DrawingCoordinateSystemIdentity {
                origin: vec![0.0; 3],
                matrix: vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
                tolerance: 1e-8,
            }],
        },
        p,
    )
}
fn display_label_observation(tag: u32, label: Option<&str>) -> Value {
    let mut v = label_observation(tag, label);
    v["abi_version"] = json!(2);
    v["counts"][3] = json!(if label.is_none_or(|v| v.is_empty()) {
        0
    } else {
        2
    });
    v
}
#[test]
fn display_pdf_source_and_selection_controls() {
    use oe_nx_worker::geometry::{Parameters, drawing_display as m};
    let (i, p) = display_fixture();
    let plan = m::prepare(&i, b"independent-fixture-cad", &p).unwrap();
    assert_eq!(plan["targets"][0]["label"], "UPPER CENTERS\nMODEL Z=10 mm");
    assert_eq!(plan["targets"][1]["label"], "LOWER CENTERS\nMODEL Z=0 mm");
    assert!(m::prepare(&i, b"wrong", &p).is_err());
    let inv = display_inventory();
    assert_eq!(m::select(&i, &inv).unwrap().len(), 1);
    for (pointer, value) in [
        ("/coordinate_systems/0/is_wcs", json!(true)),
        ("/coordinate_systems/0/owner_matches", json!(false)),
        ("/coordinate_systems/0/blank", json!(true)),
        ("/coordinate_systems/0/blank", Value::Null),
        ("/capture_complete", json!(false)),
        ("/coordinate_systems/0/csys_complete", json!(false)),
    ] {
        let mut bad = inv.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(m::select(&i, &bad).is_err(), "{pointer}");
    }
    let mut bad = inv.clone();
    let mut duplicate = display_coordinate();
    duplicate["tag"] = json!(799);
    bad["coordinate_systems"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(m::select(&i, &bad).is_err());
    let mut bad = i.clone();
    bad.hidden_csys[0].tolerance = 0.;
    assert!(m::validate(&bad).is_err());
    bad = i.clone();
    bad.hidden_csys.push(bad.hidden_csys[0].clone());
    assert!(m::select(&bad, &inv).is_err());
    let wire = json!({"schema":m::PARAMS_SCHEMA,"action":"export_display_drawing_pdf","part":"input.prt","intent":i});
    serde_json::from_value::<Parameters>(wire)
        .unwrap()
        .validate()
        .unwrap();
}
#[test]
fn display_pdf_receiving_rejects_drift() {
    use oe_nx_worker::geometry::drawing_display as m;
    let (i, p) = display_fixture();
    let (_, mut r) = identified_report();
    let plan = m::prepare(&i, b"independent-fixture-cad", &p).unwrap();
    r["action"] = json!("export_display_drawing_pdf");
    r["intent"] = serde_json::to_value(&i).unwrap();
    r["plan"] = plan.clone();
    for (key, kind) in [
        ("labels_before", 0),
        ("labels_written", 1),
        ("labels_restored", 2),
    ] {
        r[key] = json!(
            plan["targets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| display_label_observation(
                    t["dimension_tag"].as_u64().unwrap() as u32,
                    match kind {
                        0 => None,
                        1 => t["label"].as_str(),
                        _ => Some(""),
                    }
                ))
                .collect::<Vec<_>>()
        );
    }
    let mut inv = display_inventory();
    inv["view_tag"] = json!(m::view(&plan).unwrap());
    r["display_before"] = inv.clone();
    r["display_after"] = inv;
    let mut written = display_coordinate();
    written["write_returned"] = json!(true);
    written["blank"] = json!(true);
    let mut restored = written.clone();
    restored["blank"] = json!(false);
    r["display_written"] = json!([written]);
    r["display_restored"] = json!([restored]);
    let pdf = b"%PDF-1.7\nfixture envelope\n%%EOF\n";
    m::check_execution(&i, b"independent-fixture-cad", &r, pdf).unwrap();
    for (ptr, v) in [
        ("/labels_written/0/counts/3", json!(1)),
        ("/labels_written/0/below", json!("UPPER")),
        ("/display_written/0/blank", json!(false)),
        ("/display_before/view_tag", json!(999)),
        ("/display_restored/0/write_returned", json!(false)),
        (
            "/display_after/coordinate_systems/0/origin",
            json!([1, 0, 0]),
        ),
        (
            "/decorated/annotations/dimensions/0/computed_size/value",
            json!(99),
        ),
        ("/after/bbox", json!([])),
    ] {
        let mut bad = r.clone();
        *bad.pointer_mut(ptr).unwrap() = v;
        assert!(
            m::check_execution(&i, b"independent-fixture-cad", &bad, pdf).is_err(),
            "{ptr}"
        );
    }
}

fn exercise_display_restoration(version: u8) {
    let v2 = version >= 2;
    let v3 = version >= 3;
    let v4 = version >= 4;
    let v5 = version >= 5;
    let v6 = version == 6;
    use anyhow::Result;
    use oe_nx_worker::geometry::{NativeApi, PartInfo, Workspace, drawing_display};
    use std::path::{Path, PathBuf};
    struct Api {
        part: Value,
        baseline: Value,
        v4: bool,
        v5: bool,
        layout: std::collections::BTreeMap<u32, Value>,
        layout_writes: Vec<u32>,
        labels: std::collections::BTreeMap<u32, String>,
        visible: bool,
        blank: bool,
        exports: usize,
        wcs: u32,
        identity_restores: usize,
        v2: bool,
        v3: bool,
        preparations: usize,
        observations: usize,
        fault: &'static str,
    }
    impl NativeApi for Api {
        fn ensure_empty_session(&mut self) -> Result<()> {
            Ok(())
        }
        fn open_part(&mut self, _: &Path) -> Result<u32> {
            Ok(50)
        }
        fn set_work_part(&mut self, _: u32) -> Result<()> {
            Ok(())
        }
        fn detailed_info(&mut self, _: u32, refresh: bool) -> Result<Value> {
            assert!(!refresh);
            self.preparations += 1;
            if self.v3 {
                assert_eq!(self.preparations, 1, "must not reenter preparation");
            }
            Ok(self.part.clone())
        }
        fn observe_prepared_drawing(&mut self, part: u32, sheet: u32) -> Result<Value> {
            assert!(self.v3);
            assert_eq!(self.preparations, 1);
            assert_eq!(u64::from(part), self.part["part_tag"].as_u64().unwrap());
            assert_eq!(
                u64::from(sheet),
                self.part["drawing_sheets"][0]["drawing_tag"]
                    .as_u64()
                    .unwrap()
            );
            self.observations += 1;
            if self.fault == "observation" && self.observations == 2 {
                anyhow::bail!("prepared context unavailable; no activation fallback");
            }
            if self.fault == "diameter_mutation" && self.observations == 2 {
                self.part["annotations"]["dimensions"][2]["computed_size"]["value"] = json!(999);
            }
            Ok(prepared_observation_fixture(&self.part))
        }
        fn dimension_annotation_layout(
            &mut self,
            part: u32,
            tag: u32,
            origin: Option<[f64; 3]>,
            centered: Option<bool>,
        ) -> Result<Value> {
            assert!(self.v4);
            if origin.is_some() || centered.is_some() {
                self.layout_writes.push(tag);
                if self.fault == "layout_restore" && tag == 200 {
                    anyhow::bail!("injected first restoration failure");
                }
                let state = self.layout.get_mut(&tag).unwrap();
                if let Some(centered) = centered {
                    state["centered"] = json!(centered);
                }
                if let Some(origin) = origin {
                    state["origin"] = json!(origin);
                }
                if *state == annotation_layout_state(tag) {
                    let dims = self.part["annotations"]["dimensions"]
                        .as_array_mut()
                        .unwrap();
                    let d = dims.iter_mut().find(|d| d["dimension_tag"] == tag).unwrap();
                    let old = self.baseline["annotations"]["dimensions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|d| d["dimension_tag"] == tag)
                        .unwrap();
                    d["text_boxes"] = old["text_boxes"].clone();
                }
            }
            let mut v = annotation_layout_receipt(
                tag,
                self.layout[&tag].clone(),
                origin.is_some(),
                centered.is_some(),
            );
            v["part_tag"] = json!(part);
            if self.fault == "original_association" {
                v["state"]["associative_origin"] = json!(true);
            }
            if self.fault == "original_unknown" {
                v["query_complete"] = json!(false);
            }
            Ok(v)
        }
        fn dimension_label_lines_updated(
            &mut self,
            part: u32,
            tag: u32,
            label: &str,
        ) -> Result<Value> {
            assert!(self.v5);
            let mut r = self.dimension_label_lines(part, tag, Some(label))?;
            let mut update = local_update_receipt(part, tag);
            if self.fault == "update_error" && tag == 201 && !label.is_empty() {
                update["counts"]["errors_after_update"] = json!(1);
                update["rollback_called"] = json!(true);
            }
            if label.is_empty() && self.fault != "update_skip" {
                let old = self.baseline["annotations"]["dimensions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|d| d["dimension_tag"] == tag)
                    .unwrap();
                let current = self.part["annotations"]["dimensions"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|d| d["dimension_tag"] == tag)
                    .unwrap();
                current["text_boxes"] = old["text_boxes"].clone();
            }
            if label.is_empty() && self.fault == "update_cleanup" {
                update["mark_deleted"] = json!(false);
            }
            r["update"] = update;
            Ok(r)
        }
        fn dimension_label_lines(
            &mut self,
            _: u32,
            tag: u32,
            label: Option<&str>,
        ) -> Result<Value> {
            if let Some(v) = label {
                self.labels.insert(tag, v.into());
                if self.v4 && !v.is_empty() && self.fault != "layout_noop" {
                    if !self.v5 && self.fault != "layout_stale_box" {
                        let state = self.layout.get_mut(&tag).unwrap();
                        state["origin"][0] = json!(132.9);
                        state["origin"][1] = json!(146.57);
                        state["centered"] = json!(false);
                    }
                    let dims = self.part["annotations"]["dimensions"]
                        .as_array_mut()
                        .unwrap();
                    let d = dims.iter_mut().find(|d| d["dimension_tag"] == tag).unwrap();
                    let x = d["text_boxes"][0]["origin"][0].as_f64().unwrap();
                    let y = d["text_boxes"][0]["origin"][1].as_f64().unwrap();
                    d["text_boxes"][0]["origin"] =
                        json!([x - 15.59600954979535, y + 6.57264665757162, 0]);
                }
                if tag == 201 && !v.is_empty() && self.fault == "second_label" {
                    anyhow::bail!("injected partial mutation");
                }
            }
            let mut r = display_label_observation(tag, label);
            if self.fault == "restore" && label == Some("") {
                r["wrapper_code"] = json!(7);
            }
            Ok(r)
        }
        fn drawing_display_inventory(&mut self, _: u32, view: u32) -> Result<Value> {
            let mut v = display_inventory();
            v["view_tag"] = json!(view);
            v["wcs_tag"] = json!(self.wcs);
            v["coordinate_systems"][0]["wcs_tag"] = json!(self.wcs);
            v["coordinate_systems"][0]["blank"] = json!(self.blank);
            Ok(v)
        }
        fn restore_drawing_wcs_identity(&mut self, _: u32, original: u32) -> Result<Value> {
            assert!(self.v2);
            assert_eq!(original, 701);
            self.identity_restores += 1;
            if self.fault == "identity_restore" {
                anyhow::bail!("original WCS not alive");
            }
            let before = self.wcs;
            self.wcs = original;
            Ok(wcs_identity_receipt(before))
        }
        fn drawing_csys_blank(&mut self, _: u32, tag: u32, blank: bool) -> Result<Value> {
            assert_eq!(tag, 700);
            self.blank = blank;
            if blank && self.fault == "blank" {
                anyhow::bail!("partial blank");
            }
            let mut v = display_coordinate();
            v["blank"] = json!(blank);
            v["wcs_tag"] = json!(self.wcs);
            v["write_returned"] = json!(true);
            if !blank && self.fault == "display_restore" {
                v["wrapper_code"] = json!(7);
            }
            Ok(v)
        }
        fn drawing_wcs(&mut self, _: u32, v: Option<bool>) -> Result<Value> {
            if let Some(v) = v {
                self.visible = v;
            }
            Ok(wcs_observation(self.visible, v.is_some()))
        }
        fn export_drawing_pdf(&mut self, _: u32, sheet: u32, out: &Path) -> Result<Value> {
            assert!(!self.visible);
            assert_eq!(self.labels.len(), 2);
            assert!(self.labels.values().all(|s| !s.is_empty()));
            self.exports += 1;
            if self.v2 {
                self.wcs = 799;
            }
            std::fs::write(out, b"%PDF-1.7\nfixture envelope\n%%EOF\n")?;
            if self.fault == "export" {
                anyhow::bail!("partial export");
            }
            Ok(pdf_call(sheet))
        }
        fn save_work_part(&mut self) -> Result<()> {
            panic!("never save")
        }
        fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
            panic!("unexpected")
        }
        fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
            panic!("unexpected")
        }
        fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
            panic!("unexpected")
        }
        fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
            panic!("unexpected")
        }
        fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
            panic!("unexpected")
        }
        fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
            panic!("unexpected")
        }
        fn part_info(&mut self, _: u32) -> Result<PartInfo> {
            panic!("unexpected")
        }
        fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected")
        }
        fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected")
        }
        fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
            panic!("unexpected")
        }
    }
    struct Files {
        root: PathBuf,
        events: Vec<String>,
    }
    impl Workspace for Files {
        fn input(&self, _: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
            Ok(self.root.join("input.prt"))
        }
        fn output(&self, n: &str) -> Result<PathBuf> {
            Ok(self.root.join("out").join(n))
        }
        fn require_regular_output(&self, n: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.output(n)?)?)
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("unexpected")
        }
        fn record(&mut self, k: &str, _: Value) -> Result<()> {
            self.events.push(k.into());
            Ok(())
        }
    }

    let mut faults = vec![
        "none",
        "second_label",
        "blank",
        "export",
        "restore",
        "display_restore",
    ];
    if v2 {
        faults.push("identity_restore");
    }
    if v3 {
        faults.push("observation");
    }
    if v4 {
        faults.extend([
            "layout_restore",
            "layout_noop",
            "layout_stale_box",
            "original_association",
            "original_unknown",
        ]);
    }
    if v5 {
        faults.retain(|f| *f != "layout_restore");
        faults.extend(["update_error", "update_cleanup", "update_skip"]);
    }
    if v6 {
        faults.push("diameter_mutation");
    }
    for fault in faults {
        let (mut i, part) = if v6 {
            mixed_display_fixture()
        } else {
            display_fixture()
        };
        if v2 {
            i.schema = if v6 {
                drawing_display::INTENT_SCHEMA6
            } else if v5 {
                drawing_display::INTENT_SCHEMA5
            } else if v4 {
                drawing_display::INTENT_SCHEMA4
            } else if v3 {
                drawing_display::INTENT_SCHEMA3
            } else {
                drawing_display::INTENT_SCHEMA2
            }
            .into();
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("input.prt"), b"independent-fixture-cad").unwrap();
        std::fs::create_dir(tmp.path().join("out")).unwrap();
        let mut files = Files {
            root: tmp.path().into(),
            events: vec![],
        };
        let mut api = Api {
            baseline: part.clone(),
            v4,
            v5,
            layout: [200u32, 201]
                .into_iter()
                .map(|t| (t, annotation_layout_state(t)))
                .collect(),
            layout_writes: Vec::new(),
            part,
            labels: Default::default(),
            visible: true,
            blank: false,
            exports: 0,
            wcs: 701,
            identity_restores: 0,
            v2,
            v3,
            preparations: 0,
            observations: 0,
            fault,
        };
        let r = drawing_display::execute(&mut api, &mut files, "copy.prt", &i);
        assert_eq!(
            r.is_ok(),
            ["none", "layout_noop"].contains(&fault) || (v5 && fault == "layout_stale_box"),
            "{fault}: {r:?}"
        );
        if ["original_association", "original_unknown"].contains(&fault) {
            assert!(api.labels.is_empty() && api.layout_writes.is_empty());
            assert_eq!(api.exports, 0);
            assert_eq!(api.identity_restores, 0);
            assert_eq!(api.wcs, 701);
            assert!(api.visible && !api.blank);
            continue;
        }
        assert_eq!(api.identity_restores, usize::from(v2));
        assert_eq!(
            api.preparations,
            if v3 {
                1
            } else {
                if ["second_label", "blank", "update_error"].contains(&fault) {
                    2
                } else {
                    3
                }
            }
        );
        if v3 {
            assert_eq!(
                api.observations,
                if ["second_label", "blank", "update_error"].contains(&fault) {
                    2
                } else {
                    3
                }
            );
        }
        if v4 {
            if v5 || ["layout_noop", "layout_stale_box"].contains(&fault) {
                assert!(api.layout_writes.is_empty());
            } else {
                assert_eq!(
                    api.layout_writes,
                    vec![200, 201],
                    "cleanup continues for both dimensions: {fault}"
                );
            }
        }
        if fault != "identity_restore" {
            assert_eq!(api.wcs, 701);
        }
        if let Ok(report) = &r {
            if v6 {
                assert_eq!(
                    report["before"]["annotations"]["dimensions"][2],
                    report["decorated"]["annotations"]["dimensions"][2]
                );
                for path in [
                    "/decorated/annotations/dimensions/2/computed_size/value",
                    "/after/annotations/dimensions/2/associativity_parents/entries/0/point_type_raw",
                ] {
                    let mut bad = report.clone();
                    *bad.pointer_mut(path).unwrap() = json!(999);
                    assert!(
                        drawing_display::check_execution(
                            &i,
                            b"independent-fixture-cad",
                            &bad,
                            b"%PDF-1.7\nfixture envelope\n%%EOF\n"
                        )
                        .is_err(),
                        "{path}"
                    );
                }
            }
            if v3 {
                for ptr in [
                    "/context_preparation/part_tag",
                    "/before/display_part/set_called",
                    "/decorated/drawing_sheets/0/display_open_called",
                    "/after/context_mode",
                ] {
                    let mut bad = report.clone();
                    *bad.pointer_mut(ptr).unwrap() = Value::Null;
                    assert!(
                        drawing_display::check_execution(
                            &i,
                            b"independent-fixture-cad",
                            &bad,
                            b"%PDF-1.7\nfixture envelope\n%%EOF\n"
                        )
                        .is_err(),
                        "{ptr}"
                    );
                }
            }
            if v4 {
                for ptr in [
                    "/annotation_original/0/state/associative_origin",
                    "/annotation_phases/0/observations/0/owner_matches",
                    "/annotation_restoration/0/after_restore/state/origin/0",
                    "/annotation_phases/3/observations/0/state/centered",
                ] {
                    let mut bad = report.clone();
                    *bad.pointer_mut(ptr).unwrap() = Value::Null;
                    assert!(
                        drawing_display::check_execution(
                            &i,
                            b"independent-fixture-cad",
                            &bad,
                            b"%PDF-1.7\nfixture envelope\n%%EOF\n"
                        )
                        .is_err(),
                        "{ptr}"
                    );
                }
            }
            if v2 {
                assert_eq!(report["display_phases"].as_array().unwrap().len(), 5);
                assert_eq!(report["display_phases"][3]["observation"]["wcs_tag"], 799);
            }
        }
        assert!(api.visible);
        assert!(!api.blank);
        assert_eq!(api.labels.len(), 2);
        assert!(api.labels.values().all(|s| s.is_empty()));
        assert_eq!(
            api.exports,
            usize::from(
                ![
                    "second_label",
                    "blank",
                    "observation",
                    "update_error",
                    "diameter_mutation"
                ]
                .contains(&fault)
            )
        );
        assert!(files.events.contains(&"identified_pdf_restoration".into()));
        assert_eq!(
            std::fs::read(tmp.path().join("out/copy.prt")).unwrap(),
            b"independent-fixture-cad"
        );
        if api.exports > 0 {
            assert!(tmp.path().join("out/drawing.pdf").is_file());
        }
    }
}

#[test]
fn display_pdf_restores_after_partial_failure() {
    exercise_display_restoration(1);
}
#[test]
fn display2_restores_changed_identity_and_preserves_failure() {
    exercise_display_restoration(2);
}
fn wcs_identity_receipt(before: u32) -> Value {
    json!({"api":"UF_CSYS_set_wcs_guarded","wrapper_code":0,"abi_version":1,"original_tag":701,"original_alive":true,"owner_matches":true,"type_matches":true,"query_before_code":0,"query_after_code":0,"after_tag":701,"set_code":0,"before_tag":before,"set_called":before!=701})
}
#[test]
fn display2_restoration_requires_original_identity_and_preserves_v1() {
    use oe_nx_worker::geometry::{Parameters, drawing_display as m};
    let (mut i, p) = display_fixture();
    assert_eq!(
        m::prepare(&i, b"independent-fixture-cad", &p).unwrap()["targets"][0]["label"],
        "UPPER CENTERS\nMODEL Z=10 mm"
    );
    i.schema = m::INTENT_SCHEMA2.into();
    let plan = m::prepare(&i, b"independent-fixture-cad", &p).unwrap();
    assert_eq!(plan["targets"][0]["label"], "UPPER CTRS\nMODEL Z=10 mm");
    assert_eq!(plan["targets"][1]["label"], "LOWER CTRS\nMODEL Z=0 mm");
    for schema in [m::PARAMS_SCHEMA, m::PARAMS_SCHEMA2] {
        let params:Parameters=serde_json::from_value(json!({"schema":schema,"action":"export_display_drawing_pdf","part":"input.prt","intent":i})).unwrap();
        assert_eq!(params.validate().is_ok(), schema == m::PARAMS_SCHEMA2);
    }
    for before in [701, 799] {
        m::check_wcs_restoration(&wcs_identity_receipt(before), 701).unwrap();
    }
    let good = wcs_identity_receipt(799);
    for (k, v) in [
        ("original_tag", json!(799)),
        ("original_alive", json!(false)),
        ("owner_matches", json!(false)),
        ("type_matches", json!(false)),
        ("query_before_code", json!(-1)),
        ("query_after_code", json!(-1)),
        ("after_tag", json!(799)),
        ("set_code", json!(7)),
        ("before_tag", Value::Null),
        ("set_called", json!(false)),
    ] {
        let mut bad = good.clone();
        bad[k] = v;
        assert!(m::check_wcs_restoration(&bad, 701).is_err(), "{k}");
    }
    let mut inv = display_inventory();
    inv["view_tag"] = json!(m::view(&plan).unwrap());
    let mut written = display_coordinate();
    written["blank"] = json!(true);
    written["write_returned"] = json!(true);
    let mut restored = written.clone();
    restored["blank"] = json!(false);
    let phases: Vec<_> = [
        "after_labels",
        "after_visibility",
        "after_decorated_observation",
        "after_export",
        "before_identity_restore",
    ]
    .into_iter()
    .map(|n| json!({"phase":n,"observation":inv}))
    .collect();
    let report = json!({"plan":plan,"display_before":inv,"display_after":inv,"display_written":[written],"display_restored":[restored],"wcs_identity_restored":good,"display_phases":phases});
    m::check_display(&i, &report).unwrap();
    for (ptr, v) in [
        ("/display_after/wcs_tag", json!(799)),
        ("/display_phases/3/phase", json!("before_export")),
        (
            "/display_phases/3/observation/capture_complete",
            json!(false),
        ),
        ("/wcs_identity_restored/after_tag", json!(799)),
    ] {
        let mut bad = report.clone();
        *bad.pointer_mut(ptr).unwrap() = v;
        assert!(m::check_display(&i, &bad).is_err(), "{ptr}");
    }
}

fn prepared_observation_fixture(p: &Value) -> Value {
    let mut p = p.clone();
    p["drafting_readback_schema"] = json!("oev2.native-drafting-observation/7");
    p["context_mode"] = json!("observe_prepared");
    p["prepared_drawing_tag"] = p["drawing_sheets"][0]["drawing_tag"].clone();
    p["display_part"]["return_code"] = Value::Null;
    p["display_part"]["set_called"] = json!(false);
    p["display_part"]["query"] = json!("UF_PART_ask_display_part");
    for s in p["drawing_sheets"].as_array_mut().unwrap() {
        for v in s["views"].as_array_mut().unwrap() {
            v["update_code"] = Value::Null;
        }
        s["display_open_code"] = Value::Null;
        s["display_open_called"] = json!(false);
        s["display_state"]["set_code"] = Value::Null;
        s["display_state"]["set_called"] = json!(false);
        s["display_state"]["expected_drawing_tag"] = s["drawing_tag"].clone();
        s["display_state"]["capture_complete"] = json!(true);
    }
    p["context_after"] = json!({"display_part":p["display_part"],"display_state":p["drawing_sheets"][0]["display_state"]});
    p
}
#[test]
fn display3_observation_never_reenters_context_and_recovers_failure() {
    exercise_display_restoration(3);
}
#[test]
fn display3_context_requires_actual_queries_and_preserves_legacy() {
    use oe_nx_worker::geometry::{Parameters, drawing_display as m, drawing_pdf};
    let (mut i, old) = display_fixture();
    let p = prepared_observation_fixture(&old);
    assert!(drawing_pdf::prepare(&i.base.base, b"independent-fixture-cad", &p).is_err());
    assert!(m::prepare(&i, b"independent-fixture-cad", &p).is_err());
    i.schema = m::INTENT_SCHEMA2.into();
    assert!(m::prepare(&i, b"independent-fixture-cad", &p).is_err());
    i.schema = m::INTENT_SCHEMA3.into();
    assert!(m::prepare(&i, b"independent-fixture-cad", &old).is_err());
    let plan = m::prepare(&i, b"independent-fixture-cad", &p).unwrap();
    assert_eq!(plan["targets"][0]["label"], "UPPER CTRS\nMODEL Z=10 mm");
    for schema in [m::PARAMS_SCHEMA, m::PARAMS_SCHEMA2, m::PARAMS_SCHEMA3] {
        let args:Parameters=serde_json::from_value(json!({"schema":schema,"action":"export_display_drawing_pdf","part":"input.prt","intent":i})).unwrap();
        assert_eq!(args.validate().is_ok(), schema == m::PARAMS_SCHEMA3);
    }
    for (ptr, v) in [
        ("/display_part/return_code", json!(0)),
        ("/display_part/set_called", json!(true)),
        ("/display_part/actual_part", json!(999)),
        ("/context_mode", json!("prepare")),
        ("/prepared_drawing_tag", json!(999)),
        ("/view_refresh_requested", json!(true)),
        ("/drawing_sheets/0/display_open_code", json!(0)),
        ("/drawing_sheets/0/display_open_called", json!(true)),
        ("/drawing_sheets/0/display_state/set_code", json!(0)),
        (
            "/drawing_sheets/0/display_state/current_drawing_tag",
            json!(999),
        ),
        ("/drawing_sheets/0/display_state/query_code", json!(7)),
        (
            "/drawing_sheets/0/display_state/work_view_type/type_raw",
            json!(0),
        ),
        ("/context_after/display_part/actual_part", json!(999)),
    ] {
        let mut bad = p.clone();
        *bad.pointer_mut(ptr).unwrap() = v;
        assert!(
            m::prepare(&i, b"independent-fixture-cad", &bad).is_err(),
            "{ptr}"
        );
    }
}

fn annotation_layout_state(tag: u32) -> Value {
    json!({"origin":[148.5,if tag==200{140.0}else{70.0},0.0],"centered":true,"associative_origin":false})
}
fn annotation_layout_receipt(tag: u32, state: Value, origin: bool, centered: bool) -> Value {
    json!({"api":"NXOpen::Annotations::Dimension::AnnotationLayout","abi_version":1,"wrapper_code":0,"part_tag":50,"dimension_tag":tag,"alive":true,"owner_matches":true,"dimension_type_matches":true,"query_complete":true,"state":state,"origin_set_called":origin,"origin_set_returned":origin,"center_set_called":centered,"center_set_returned":centered})
}
#[test]
fn display4_layout_preserves_full_inventory_and_cleanup() {
    exercise_display_restoration(4);
}
#[test]
fn display4_layout_unknown_associative_and_noop_controls() {
    use oe_nx_worker::geometry::drawing_annotation as a;
    let old = annotation_layout_receipt(200, annotation_layout_state(200), false, false);
    a::original(&old, 50, 200).unwrap();
    let unchanged = json!({"dimension_tag":200,"before_restore":old,"after_restore":old});
    a::check_restoration(50, 200, &old, &unchanged).unwrap();
    for (ptr, value) in [
        ("/state/associative_origin", json!(true)),
        ("/state/centered", Value::Null),
        ("/state/origin", json!([1, 2])),
        ("/owner_matches", json!(false)),
        ("/alive", json!(false)),
        ("/query_complete", json!(false)),
        ("/part_tag", json!(51)),
        ("/dimension_tag", json!(201)),
    ] {
        let mut bad = old.clone();
        *bad.pointer_mut(ptr).unwrap() = value;
        assert!(a::original(&bad, 50, 200).is_err(), "{ptr}");
    }
    let mut blind = unchanged.clone();
    blind["after_restore"]["origin_set_called"] = json!(true);
    blind["after_restore"]["origin_set_returned"] = json!(true);
    assert!(a::check_restoration(50, 200, &old, &blind).is_err());
    let mut moved = old.clone();
    moved["state"]["origin"] = json!([132.9, 146.57, 0.0]);
    moved["state"]["centered"] = json!(false);
    let after = annotation_layout_receipt(200, annotation_layout_state(200), true, true);
    let restored = json!({"dimension_tag":200,"before_restore":moved,"after_restore":after});
    a::check_restoration(50, 200, &old, &restored).unwrap();
    let mut failed = restored.clone();
    failed["after_restore"]["state"]["origin"][0] = json!(148.500001);
    assert!(a::check_restoration(50, 200, &old, &failed).is_err());
}

fn local_update_receipt(part: u32, tag: u32) -> Value {
    json!({"api":"NXOpen::Update::StartLocalUpdate/DoUpdate/EndLocalUpdate","abi_version":1,"wrapper_code":0,"part_tag":part,"dimension_tag":tag,
      "counts":{"errors_before":0,"errors_after_update":0,"errors_final":0,"delete_before":0,"delete_final":0,"cleanup_errors":0},
      "display_part_matches":true,"mark_created":true,"mark_id":123,"local_started":true,"write_called":true,"write_code":0,
      "update_called":true,"update_returned":true,"update_errors":0,"local_ended":true,"rollback_called":false,"rollback_returned":false,
      "mark_deleted":true,"locked_before":false,"locked_final":false,"prior_marks_preserved":true,"own_mark_absent":true,"readback_code":0,"error_list_cleared":false,"error_codes":[]})
}
#[test]
fn display5_local_update_restores_boxes_and_preserves_cleanup() {
    exercise_display_restoration(5);
}
#[test]
fn display5_local_update_rejects_unknown_or_failed_proofs() {
    use oe_nx_worker::geometry::drawing_update as m;
    let good = local_update_receipt(50, 200);
    m::check(&good, 50, 200).unwrap();
    for key in good.as_object().unwrap().keys() {
        let mut v = good.clone();
        v[key] = Value::Null;
        assert!(m::check(&v, 50, 200).is_err(), "{key}");
    }
    for key in good["counts"].as_object().unwrap().keys() {
        let mut v = good.clone();
        v["counts"][key] = json!(1);
        assert!(m::check(&v, 50, 200).is_err(), "{key}");
    }
    for key in [
        "rollback_called",
        "error_list_cleared",
        "locked_before",
        "locked_final",
    ] {
        let mut v = good.clone();
        v[key] = json!(true);
        assert!(m::check(&v, 50, 200).is_err(), "{key}");
    }
    assert!(m::check(&good, 51, 200).is_err());
    assert!(m::check(&good, 50, 201).is_err());
}

fn diameter_fixture() -> (oe_contracts::SheetDiameterIntent, Value, Value) {
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
#[test]
fn sheet_diameter_preflight_and_protocol() {
    use oe_nx_worker::geometry::{Parameters, sheet_diameter as owner};
    let (i, p, _) = diameter_fixture();
    let source = b"independent-fixture-cad";
    assert_eq!(owner::prepare(&i, source, &p).unwrap()["edge_tag"], 101);
    assert!(owner::prepare(&i, b"different-cad", &p).is_err());
    for (pointer, value) in [
        ("/diameter", json!(2.5)),
        ("/edge/unit", json!("inch")),
        ("/text_origin_drawing", json!([0, 140])),
        ("/source_refs", json!([])),
        ("/edge/normal", json!([0, 1, 0])),
        ("/edge/center", json!([0, 0, 3])),
    ] {
        let mut bad = serde_json::to_value(&i).unwrap();
        *bad.pointer_mut(pointer).unwrap() = value;
        let bad = serde_json::from_value(bad).unwrap();
        assert!(owner::prepare(&bad, source, &p).is_err(), "{pointer}");
    }
    let value = json!({"schema":owner::PARAMS_SCHEMA,"action":"create_sheet_diameter_dimension","part":"shaft.prt","intent":i});
    let parsed: Parameters = serde_json::from_value(value.clone()).unwrap();
    parsed.validate().unwrap();
    assert_eq!(parsed.parameter_schema(), owner::PARAMS_SCHEMA);
    assert_eq!(parsed.action(), "create_sheet_diameter_dimension");
    let mut old = value;
    old["schema"] = json!("oev2.nx-sheet-dimension-params/1");
    assert!(
        serde_json::from_value::<Parameters>(old)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut duplicate = p.clone();
    let mut edge = p["model_edges"]["entries"][0].clone();
    edge["edge_tag"] = json!(103);
    duplicate["model_edges"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(edge);
    duplicate["model_edges"]["bodies"][0]["edge_tags"]
        .as_array_mut()
        .unwrap()
        .push(json!(103));
    assert!(owner::prepare(&i, source, &duplicate).is_err());
}
#[test]
fn sheet_diameter_native_value_parent_and_preservation() {
    use oe_nx_worker::geometry::sheet_diameter as owner;
    let (i, mut before, mut after) = diameter_fixture();
    let check = |p: &Value| owner::check_created(&i, b"independent-fixture-cad", p, 200);
    assert_eq!(check(&after).unwrap()["conclusion"], "pass");
    owner::check_addition(&before, &after, 200).unwrap();
    for (pointer, value) in [
        ("/computed_size/value", json!(2.5)),
        ("/computed_size/metadata/type_query/subtype_raw", json!(3)),
        ("/computed_size/unit", json!("inch")),
        ("/retained", json!(true)),
        ("/associativity_parents/uf_return_code", json!(7)),
        ("/associativity_parents/entries/0/view/tag", json!(42)),
        ("/associativity_parents/entries/0/object_1/tag", json!(102)),
        ("/associativity_parents/entries/0/point_type_raw", json!(1)),
    ] {
        let mut bad = after.clone();
        *bad["annotations"]["dimensions"][0]
            .pointer_mut(pointer)
            .unwrap() = value;
        assert!(check(&bad).is_err(), "{pointer}");
    }
    let mut prior = after["annotations"]["dimensions"][0].clone();
    prior["dimension_tag"] = json!(199);
    before["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(prior.clone());
    after["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(prior);
    owner::check_addition(&before, &after, 200).unwrap();
    after["annotations"]["dimensions"][1]["computed_size"]["value"] = json!(6);
    assert_eq!(
        owner::check_addition(&before, &after, 200)
            .unwrap_err()
            .to_string(),
        "diameter_prior_annotation_changed"
    );
}
#[test]
fn sheet_diameter_execution_rejects_pre_and_post_save_failures() {
    use anyhow::Result;
    use oe_nx_worker::geometry::{NativeApi, Parameters, PartInfo, Workspace, execute};
    use std::path::{Path, PathBuf};
    struct Api {
        before: Value,
        after: Value,
        saved: Value,
        reads: usize,
        saves: usize,
        complete: bool,
    }
    impl NativeApi for Api {
        fn ensure_empty_session(&mut self) -> Result<()> {
            Ok(())
        }
        fn open_part(&mut self, _: &Path) -> Result<u32> {
            Ok(50)
        }
        fn set_work_part(&mut self, p: u32) -> Result<()> {
            assert_eq!(p, 50);
            Ok(())
        }
        fn detailed_info(&mut self, _: u32, refresh: bool) -> Result<Value> {
            assert!(!refresh);
            self.reads += 1;
            Ok(match self.reads {
                1 => self.before.clone(),
                2 => self.after.clone(),
                3 => self.saved.clone(),
                _ => panic!("unexpected read"),
            })
        }
        fn create_sheet_diameter(
            &mut self,
            sheet: u32,
            view: u32,
            edge: u32,
            text: [f64; 2],
        ) -> Result<Value> {
            assert_eq!((sheet, view, edge, text), (40, 41, 101, [100.0, 140.0]));
            Ok(json!({"complete":self.complete,"dimension_tag":200}))
        }
        fn save_work_part(&mut self) -> Result<()> {
            self.saves += 1;
            Ok(())
        }
        fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
            panic!("unexpected CAD operation")
        }
        fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
            panic!("unexpected CAD operation")
        }
        fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn part_info(&mut self, _: u32) -> Result<PartInfo> {
            panic!("unexpected CAD operation")
        }
        fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
            panic!("unexpected CAD operation")
        }
    }
    struct Files(PathBuf);
    impl Workspace for Files {
        fn input(&self, _: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
            Ok(self.0.join("input.prt"))
        }
        fn output(&self, name: &str) -> Result<PathBuf> {
            Ok(self.0.join("out").join(name))
        }
        fn require_regular_output(&self, name: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.0.join("out").join(name))?)
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("unexpected optional output")
        }
        fn record(&mut self, _: &str, _: Value) -> Result<()> {
            Ok(())
        }
    }
    for fault in ["none", "create", "value", "saved_value"] {
        let (intent, before, after) = diameter_fixture();
        let mut api = Api {
            before,
            after: after.clone(),
            saved: after,
            reads: 0,
            saves: 0,
            complete: fault != "create",
        };
        if fault == "value" {
            api.after["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
        }
        if fault == "saved_value" {
            api.saved["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("out")).unwrap();
        std::fs::write(root.path().join("input.prt"), b"independent-fixture-cad").unwrap();
        let mut files = Files(root.path().to_owned());
        let params = Parameters::CreateSheetDiameterDimension {
            schema: "oev2.nx-sheet-diameter-params/1".into(),
            part: "shaft.prt".into(),
            intent,
        };
        let result = execute(&mut api, &mut files, &params);
        if fault == "none" {
            assert_eq!(
                result.unwrap()["independent_reopen"],
                "required_separate_process"
            );
            assert_eq!(api.saves, 1);
        } else {
            let reason = result.unwrap_err().to_string();
            assert_eq!(
                reason,
                if fault == "create" {
                    "diameter_native_creation_incomplete"
                } else {
                    "diameter_value_mismatch"
                }
            );
            assert_eq!(api.saves, usize::from(fault == "saved_value"));
        }
        assert_eq!(
            std::fs::read(root.path().join("input.prt")).unwrap(),
            b"independent-fixture-cad"
        );
    }
}

fn whole_diameter_fixture() -> (oe_contracts::SheetDiameterIntent, Value, Value) {
    let (mut i, b, mut a) = diameter_fixture();
    i.schema = oe_nx_worker::geometry::sheet_diameter::INTENT_SCHEMA2.into();
    let e = &mut a["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0];
    e["point_type_raw"] = json!(0);
    e["association_position_valid"] = json!(true);
    e["association_position"] = json!([0, 0, 0]);
    e["coordinate_basis"] = json!("UF_DRF assoc_point absolute coordinates");
    (i, b, a)
}
#[test]
fn sheet_diameter_whole_circle_version_and_independent_position() {
    use oe_nx_worker::geometry::{Parameters, sheet_diameter as o};
    let (i, _, a) = whole_diameter_fixture();
    let source = b"independent-fixture-cad";
    assert_eq!(
        o::check_created(&i, source, &a, 200).unwrap()["schema"],
        "oev2.sheet-diameter-check/2"
    );
    let mut legacy = i.clone();
    legacy.schema = o::INTENT_SCHEMA.into();
    assert!(o::check_created(&legacy, source, &a, 200).is_err());
    for (pointer, value) in [
        ("/point_type_raw", json!(2)),
        ("/association_position_valid", json!(false)),
        ("/association_position", json!([0, 0, 0.01])),
        ("/association_position", json!([0, 0])),
        ("/coordinate_basis", json!("drawing coordinates")),
    ] {
        let mut bad = a.clone();
        *bad["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]
            .pointer_mut(pointer)
            .unwrap() = value;
        assert!(
            o::check_created(&i, source, &bad, 200).is_err(),
            "{pointer}"
        );
    }
    let p = Parameters::CreateSheetDiameterDimension {
        schema: o::PARAMS_SCHEMA2.into(),
        part: "shaft.prt".into(),
        intent: i.clone(),
    };
    p.validate().unwrap();
    assert_eq!(p.parameter_schema(), o::PARAMS_SCHEMA2);
    let mixed = Parameters::CreateSheetDiameterDimension {
        schema: o::PARAMS_SCHEMA.into(),
        part: "shaft.prt".into(),
        intent: i,
    };
    assert!(mixed.validate().is_err());
}

#[test]
fn sheet_diameter_whole_circle_execution_checks_save_order() {
    use anyhow::Result;
    use oe_nx_worker::geometry::{NativeApi, Parameters, PartInfo, Workspace, execute};
    use std::path::{Path, PathBuf};
    struct Api {
        before: Value,
        after: Value,
        saved: Value,
        reads: usize,
        saves: usize,
        complete: bool,
    }
    impl NativeApi for Api {
        fn ensure_empty_session(&mut self) -> Result<()> {
            Ok(())
        }
        fn open_part(&mut self, _: &Path) -> Result<u32> {
            Ok(50)
        }
        fn set_work_part(&mut self, p: u32) -> Result<()> {
            assert_eq!(p, 50);
            Ok(())
        }
        fn detailed_info(&mut self, _: u32, refresh: bool) -> Result<Value> {
            assert!(!refresh);
            self.reads += 1;
            Ok(match self.reads {
                1 => self.before.clone(),
                2 => self.after.clone(),
                3 => self.saved.clone(),
                _ => panic!("unexpected read"),
            })
        }
        fn create_sheet_diameter(
            &mut self,
            sheet: u32,
            view: u32,
            edge: u32,
            text: [f64; 2],
        ) -> Result<Value> {
            assert_eq!((sheet, view, edge, text), (40, 41, 101, [100.0, 140.0]));
            Ok(json!({"complete":self.complete,"dimension_tag":200}))
        }
        fn save_work_part(&mut self) -> Result<()> {
            self.saves += 1;
            Ok(())
        }
        fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
            panic!("unexpected CAD operation")
        }
        fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
            panic!("unexpected CAD operation")
        }
        fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
            panic!("unexpected CAD operation")
        }
        fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn part_info(&mut self, _: u32) -> Result<PartInfo> {
            panic!("unexpected CAD operation")
        }
        fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected CAD operation")
        }
        fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
            panic!("unexpected CAD operation")
        }
    }
    struct Files(PathBuf);
    impl Workspace for Files {
        fn input(&self, _: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
            Ok(self.0.join("input.prt"))
        }
        fn output(&self, name: &str) -> Result<PathBuf> {
            Ok(self.0.join("out").join(name))
        }
        fn require_regular_output(&self, name: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.0.join("out").join(name))?)
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("unexpected optional output")
        }
        fn record(&mut self, _: &str, _: Value) -> Result<()> {
            Ok(())
        }
    }
    for fault in ["none", "create", "value", "saved_value"] {
        let (intent, before, after) = whole_diameter_fixture();
        let mut api = Api {
            before,
            after: after.clone(),
            saved: after,
            reads: 0,
            saves: 0,
            complete: fault != "create",
        };
        if fault == "value" {
            api.after["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
        }
        if fault == "saved_value" {
            api.saved["annotations"]["dimensions"][0]["computed_size"]["value"] = json!(31);
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("out")).unwrap();
        std::fs::write(root.path().join("input.prt"), b"independent-fixture-cad").unwrap();
        let mut files = Files(root.path().to_owned());
        let params = Parameters::CreateSheetDiameterDimension {
            schema: "oev2.nx-sheet-diameter-params/2".into(),
            part: "shaft.prt".into(),
            intent,
        };
        let result = execute(&mut api, &mut files, &params);
        if fault == "none" {
            assert_eq!(
                result.unwrap()["independent_reopen"],
                "required_separate_process"
            );
            assert_eq!(api.saves, 1);
        } else {
            let reason = result.unwrap_err().to_string();
            assert_eq!(
                reason,
                if fault == "create" {
                    "diameter_native_creation_incomplete"
                } else {
                    "diameter_value_mismatch"
                }
            );
            assert_eq!(api.saves, usize::from(fault == "saved_value"));
        }
        assert_eq!(
            std::fs::read(root.path().join("input.prt")).unwrap(),
            b"independent-fixture-cad"
        );
    }
}

fn mixed_display_fixture() -> (oe_contracts::DrawingDisplayPdfIntent, Value) {
    use oe_nx_worker::geometry::{
        drawing_display, drawing_identification, drawing_inventory, drawing_pdf,
    };
    let (mut i, mut p) = display_fixture();
    let mut d = p["annotations"]["dimensions"][0].clone();
    d["dimension_tag"] = json!(302);
    d["computed_size"]["metadata"]["type_query"]["subtype_raw"] = json!(10);
    d["computed_size"]["value"] = json!(5.0);
    let mut e = d["associativity_parents"]["entries"][0].clone();
    e["point_type_raw"] = json!(0);
    e["line_type_raw"] = json!(0);
    e["coordinate_basis"] = json!("UF_DRF assoc_point absolute coordinates");
    e["object_1"]["geometry"] = json!({"called":true,"return_code":0,"value_valid":true,
        "absolute_bounds":[-2.5,-2.5,-16,2.5,2.5,-16]});
    d["associativity_parents"]["entries"] = json!([e]);
    p["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(d);
    i.schema = drawing_display::INTENT_SCHEMA6.into();
    i.base.schema = drawing_identification::INTENT_SCHEMA2.into();
    i.base.base.schema = drawing_pdf::INTENT_SCHEMA2.into();
    i.base.base.inventory_signatures = drawing_inventory::inventory(&p)
        .unwrap()
        .into_keys()
        .collect();
    (i, p)
}
#[test]
fn mixed_pdf_explicit_schemas_complete_inventory_and_legacy_refusal() {
    use oe_nx_worker::geometry::{
        Parameters, drawing_display as d, drawing_identification as id, drawing_inventory as inv,
        drawing_pdf as pdf, drawing_retirement as old,
    };
    let (i, p) = mixed_display_fixture();
    assert_eq!(
        old::inventory(&p).unwrap_err().to_string(),
        "retirement_dimension_scope_invalid"
    );
    let all = inv::inventory(&p).unwrap();
    assert_eq!(all.len(), 3);
    for v in &p["annotations"]["dimensions"].as_array().unwrap()[..2] {
        assert_eq!(
            old::signature(v, "mm").unwrap(),
            inv::signature(v, "mm").unwrap()
        );
    }
    assert_eq!(
        pdf::prepare(&i.base.base, b"independent-fixture-cad", &p).unwrap()["dimension_count"],
        3
    );
    assert_eq!(
        id::prepare(&i.base, b"independent-fixture-cad", &p).unwrap()["targets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let prepared = prepared_observation_fixture(&p);
    d::prepare(&i, b"independent-fixture-cad", &prepared).unwrap();
    for (owner, version) in [
        ("display", d::PARAMS_SCHEMA6),
        ("identified", id::PARAMS_SCHEMA2),
        ("pdf", pdf::PARAMS_SCHEMA2),
    ] {
        let (action, intent) = match owner {
            "display" => (
                "export_display_drawing_pdf",
                serde_json::to_value(&i).unwrap(),
            ),
            "identified" => (
                "export_identified_drawing_pdf",
                serde_json::to_value(&i.base).unwrap(),
            ),
            _ => (
                "export_drawing_pdf",
                serde_json::to_value(&i.base.base).unwrap(),
            ),
        };
        let mut wire = json!({"schema":version,"action":action,"part":"input.prt","intent":intent});
        let p: Parameters = serde_json::from_value(wire.clone()).unwrap();
        p.validate().unwrap();
        assert_eq!(p.parameter_schema(), version);
        wire["schema"] = json!(version.rsplit_once('/').unwrap().0.to_owned() + "/1");
        assert!(
            serde_json::from_value::<Parameters>(wire)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for schema in [
        d::INTENT_SCHEMA,
        d::INTENT_SCHEMA2,
        d::INTENT_SCHEMA3,
        d::INTENT_SCHEMA4,
        d::INTENT_SCHEMA5,
    ] {
        let mut bad = i.clone();
        bad.schema = schema.into();
        assert!(d::validate(&bad).is_err());
    }
    let mut bad = i.clone();
    bad.base.base.schema = pdf::INTENT_SCHEMA.into();
    assert!(d::validate(&bad).is_err());
    assert!(d::prepare(&i, b"wrong-source", &prepared).is_err());
    for ptr in [
        "/computed_size/value",
        "/text_boxes/0/origin/0",
        "/associativity_parents/entries/0/association_position/0",
    ] {
        let mut p = prepared.clone();
        *p["annotations"]["dimensions"][2].pointer_mut(ptr).unwrap() = json!(999);
        assert!(
            d::prepare(&i, b"independent-fixture-cad", &p).is_err(),
            "{ptr}"
        );
    }
    for (ptr, value) in [
        ("/computed_size/metadata/type_query/subtype_raw", json!(11)),
        ("/associativity_parents/entries/0/point_type_raw", json!(2)),
        (
            "/associativity_parents/entries/0/association_position_valid",
            json!(false),
        ),
        (
            "/associativity_parents/entries/0/object_1/metadata/owning_part_query/matches_expected_owner",
            json!(false),
        ),
    ] {
        let mut p = p.clone();
        *p["annotations"]["dimensions"][2].pointer_mut(ptr).unwrap() = value;
        assert!(inv::inventory(&p).is_err(), "{ptr}");
    }
    let mut missing = prepared.clone();
    missing["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(d::prepare(&i, b"independent-fixture-cad", &missing).is_err());
    let mut retained = p.clone();
    retained["annotations"]["dimensions"][2]["retained"] = json!(true);
    let mut changed = i.clone();
    changed.base.base.inventory_signatures =
        inv::inventory(&retained).unwrap().into_keys().collect();
    assert!(pdf::prepare(&changed.base.base, b"independent-fixture-cad", &retained).is_err());
}
#[test]
fn mixed_display_preserves_diameter_across_decoration_and_failure_recovery() {
    exercise_display_restoration(6);
}
