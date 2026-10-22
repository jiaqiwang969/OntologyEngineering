//! Revision 02: add complete owned-body metadata, with missing/false/wrong-tag controls.
//! Original A/B numbers, tolerance and all prior rejection expectations are unchanged.
//! Independently authored local geometry controls. No SDK/native result is fabricated.
//! A/B numbers are frozen in INDEPENDENT-CYLINDER-NUMERIC-ORACLE-01.json (d2fc3c05...).
//! Metadata uses the existing model_edges observation contract. Every observation
//! below is synthetic, including raw mass properties calculated from that oracle.
use oe_contracts::NxCylinderIntent;
use oe_nx_worker::geometry::{CONTROLLED_REFSET, REPRESENTATION_BASELINE, rotating_part::check};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct Case {
    id: &'static str,
    origin: [f64; 3],
    diameter: f64,
    length: f64,
    radius: f64,
    bounds: [f64; 6],
    volume_mm3: f64,
    area_mm2: f64,
}

const A: Case = Case {
    id: "A",
    origin: [10.0, -15.0, 7.0],
    diameter: 24.0,
    length: 40.0,
    radius: 12.0,
    bounds: [-2.0, -27.0, 7.0, 22.0, -3.0, 47.0],
    volume_mm3: 18095.573684677209,
    area_mm2: 3920.707631680062,
};
const B: Case = Case {
    id: "B",
    origin: [-20.0, 30.0, -5.0],
    diameter: 10.0,
    length: 25.0,
    radius: 5.0,
    bounds: [-25.0, 25.0, -5.0, -15.0, 35.0, 20.0],
    volume_mm3: 1963.4954084936208,
    area_mm2: 942.477796076938,
};

fn metadata(subtype: i64) -> Value {
    json!({"type_query":{"return_code":0,"type_raw":70,"subtype_raw":subtype},
        "owning_part_query":{"value_valid":true,"matches_expected_owner":true}})
}

fn circle(case: Case, tag: u32, z: f64) -> Value {
    json!({"body_tag":100,"edge_tag":tag,"metadata":metadata(3),
        "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,
            "is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":true},
        "arc":{"value_valid":true,"is_periodic":true,"limits":[0.0,std::f64::consts::TAU],
            "radius":case.radius,"center":[case.origin[0],case.origin[1],z],
            "x_axis":[1.0,0.0,0.0],"y_axis":[0.0,1.0,0.0]}})
}

fn fixture(case: Case) -> (NxCylinderIntent, Value) {
    let intent: NxCylinderIntent = serde_json::from_value(json!({
        "schema":"oev2.nx-cylinder-intent/1",
        "object":{"object_id":format!("urn:oev2:primitive:nx-cylinder:{}",case.id),
            "revision":"r1","condition_scope":"primitive-solid-cylinder-only",
            "source_sha256":"be71aca56c6ecc73a46f7d5289b969e18722b2deb3233ca808d6eb6263e27c49"},
        "feature_id":format!("cylinder-{}",case.id),"unit":"mm",
        "coordinate_system":"absolute_part_cartesian","axis_direction":[0.0,0.0,1.0],
        "origin_mm":case.origin,"diameter_mm":case.diameter,"length_mm":case.length,
        "source_role":"independent_software_fixture"
    }))
    .unwrap();
    let cylinder = json!({"body_tag":100,"face_tag":201,"query_code":0,"surface_type":16,
        // A native axis point is not necessarily the bottom-center point.
        "axis_point":[case.origin[0],case.origin[1],case.origin[2]+3.0],
        "axis_direction":[0.0,0.0,1.0],"radius":case.radius,
        "face_bounds":case.bounds,"normal_direction":1});
    let plane = |tag, z, direction| {
        json!({"body_tag":100,"face_tag":tag,"query_code":0,
        "surface_type":22,"axis_point":[case.origin[0]+1.0,case.origin[1]-2.0,z],
        "axis_direction":[0.0,0.0,direction],"radius":0.0})
    };
    let bottom = circle(case, 301, case.origin[2]);
    let top = circle(case, 302, case.origin[2] + case.length);
    // A noncircular seam is fully queried. The contract does not fix edge count.
    let seam = json!({"body_tag":100,"edge_tag":303,"metadata":metadata(3),
        "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,
            "is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":false},"arc":null});
    let mut mass = vec![0.0; 47];
    mass[0] = case.area_mm2 / 1e6;
    mass[1] = case.volume_mm3 / 1e9;
    let observation = json!({
        "path":"C:/private-cylinder-control/cylinder.prt","part_tag":77,
        "representation_baseline":REPRESENTATION_BASELINE,"controlled_jt_mapping":null,
        "units":"mm","solid_count":1,"solid_tags":[100],"sheet_count":0,
        "rotating_body_inventory_complete":true,"rotating_body_inventory_tags":[100],
        "controlled_reference_set":{"name":CONTROLLED_REFSET,"found":true,
            "reference_set_tag":7,"member_tags":[100]},
        "bbox":[case.bounds],"bbox_role":"synthetic envelope, not measured geometry",
        "planar_vertex_bounds":[null],"expressions":[],"components":[],
        "exact_work_csys_bounds":[{"min_corner":[case.bounds[0],case.bounds[1],case.bounds[2]],
            "directions":[[1.0,0.0,0.0],[0.0,1.0,0.0],[0.0,0.0,1.0]],
            "distances":[case.diameter,case.diameter,case.length],"absolute_axis_bounds":case.bounds}],
        "cylindrical_faces":[cylinder.clone()],"solid_face_inventory_complete":true,
        "solid_face_inventory":[cylinder,plane(202,case.origin[2],-1.0),
            plane(203,case.origin[2]+case.length,1.0)],
        "model_edges":{"schema":"oev2.nx-model-edge-observation/1","complete":true,"unit":"mm",
            "coordinate_basis":"UF_EVAL ABS coordinates; single-part solid edges",
            "solid_type_raw":70,"edge_subtype_raw":3,
            "bodies":[{"body_tag":100,"metadata":metadata(0),"edge_list_return_code":0,
                "capture_complete":true,"edge_tags":[301,302,303]}],"entries":[bottom,top,seam]},
        "native_volume":{"schema":"oev2.nx-native-solid-mass-properties/1","return_code":0,
            "body_tags":[100],"object_type":1,"unit_selector":4,"unit":"m3",
            "accuracy_mode":1,"accuracy_requested":0.99,"raw47":mass,"statistics13":vec![0.0;13],
            "volume_m3":case.volume_mm3/1e9,"surface_area_m2":case.area_mm2/1e6,
            "density_and_material":"not_qualified"}
    });
    (intent, observation)
}

type Control = (&'static str, fn(&mut Value));
fn reject(controls: &[Control]) {
    let (intent, valid) = fixture(A);
    assert!(
        check(&intent, &valid).is_ok(),
        "complete synthetic positive control must pass"
    );
    for (name, mutation) in controls {
        let mut observed = valid.clone();
        mutation(&mut observed);
        assert!(
            check(&intent, &observed).is_err(),
            "{name}: invalid observation was accepted"
        );
    }
}

#[test]
fn frozen_a_b_complete_geometry_matches_without_engineering_adoption() {
    for case in [A, B] {
        let (intent, observed) = fixture(case);
        let checked = check(&intent, &observed).unwrap();
        assert_eq!(checked["conclusion"], "matches_declared_geometry");
        assert_eq!(checked["expected_bounds_mm"], json!(case.bounds));
        assert_eq!(
            checked["object"],
            serde_json::to_value(intent.object).unwrap()
        );
        assert_eq!(
            checked["manufacturing_or_physical_acceptance"],
            "not_assessed"
        );
    }
}

#[test]
fn same_bbox_planar_box_cannot_pass_cylinder_shape() {
    reject(&[("same bbox with planar topology", |v| {
        v["cylindrical_faces"] = json!([]);
        let plane = v["solid_face_inventory"][1].clone();
        v["solid_face_inventory"] = Value::Array(
            (0..6)
                .map(|i| {
                    let mut f = plane.clone();
                    f["face_tag"] = json!(210 + i);
                    f
                })
                .collect(),
        );
    })]);
}

#[test]
fn missing_cap_is_rejected_with_an_otherwise_complete_edge_list() {
    reject(&[("one circular cap absent", |v| {
        v["model_edges"]["entries"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        v["model_edges"]["bodies"][0]["edge_tags"] = json!([301, 303]);
    })]);
}

#[test]
fn cylinder_and_circle_radius_must_each_match_the_intent() {
    reject(&[
        ("wrong cylinder radius with same bbox", |v| {
            v["cylindrical_faces"][0]["radius"] = json!(11.0);
            v["solid_face_inventory"][0]["radius"] = json!(11.0);
        }),
        ("wrong cap radius with valid cylinder", |v| {
            v["model_edges"]["entries"][0]["arc"]["radius"] = json!(11.0);
        }),
    ]);
}

#[test]
fn edge_owners_and_body_membership_are_checked() {
    reject(&[
        ("foreign edge owner", |v| {
            v["model_edges"]["entries"][0]["metadata"]["owning_part_query"]["matches_expected_owner"] =
                json!(false)
        }),
        ("foreign body owner", |v| {
            v["model_edges"]["bodies"][0]["metadata"]["owning_part_query"]["matches_expected_owner"] =
                json!(false)
        }),
        ("edge entry disagrees with complete membership", |v| {
            v["model_edges"]["entries"][0]["body_tag"] = json!(999)
        }),
    ]);
}

#[test]
fn incomplete_catalogs_and_query_failures_cannot_be_hidden() {
    reject(&[
        ("missing complete owned-body inventory", |v| {
            v.as_object_mut()
                .unwrap()
                .remove("rotating_body_inventory_complete");
            v.as_object_mut()
                .unwrap()
                .remove("rotating_body_inventory_tags");
        }),
        ("false complete owned-body inventory", |v| {
            v["rotating_body_inventory_complete"] = json!(false);
        }),
        ("owned-body inventory contradicts current solid", |v| {
            v["rotating_body_inventory_tags"] = json!([999]);
        }),
        ("partial face list", |v| {
            v["solid_face_inventory_complete"] = json!(false)
        }),
        ("partial edge catalog", |v| {
            v["model_edges"]["complete"] = json!(false)
        }),
        ("failed face query", |v| {
            v["solid_face_inventory"][1]["query_code"] = json!(7)
        }),
        ("failed arc release", |v| {
            v["model_edges"]["entries"][0]["query"]["free_code"] = json!(7)
        }),
        ("incomplete unrelated seam", |v| {
            v["model_edges"]["entries"][2]["query"]["capture_complete"] = json!(false)
        }),
    ]);
}

#[test]
fn native_frame_axes_and_arc_orthogonality_are_required() {
    reject(&[
        ("wrong edge frame", |v| {
            v["model_edges"]["coordinate_basis"] = json!("WCS")
        }),
        ("nonorthogonal circular basis", |v| {
            v["model_edges"]["entries"][0]["arc"]["y_axis"] = json!([1.0, 0.0, 0.0])
        }),
        ("invalid exact box frame", |v| {
            v["exact_work_csys_bounds"][0]["directions"][1] = json!([1.0, 0.0, 0.0])
        }),
        ("off-axis cylinder beyond frozen vector tolerance", |v| {
            v["cylindrical_faces"][0]["axis_direction"] = json!([5e-7, 0.0, 1.0]);
            v["solid_face_inventory"][0]["axis_direction"] = json!([5e-7, 0.0, 1.0]);
        }),
    ]);
}

#[test]
fn face_tags_must_be_unique_nonzero_and_sdk_u32() {
    reject(&[
        ("duplicate face tag", |v| {
            v["solid_face_inventory"][2]["face_tag"] = json!(202)
        }),
        ("null native tag", |v| {
            v["solid_face_inventory"][1]["face_tag"] = json!(0)
        }),
        ("native tag overflow", |v| {
            v["solid_face_inventory"][1]["face_tag"] = json!(u64::from(u32::MAX) + 1)
        }),
    ]);
}

#[test]
fn serialized_exact_bounds_cannot_replace_the_observed_frame() {
    reject(&[
        ("frame and serialized bounds disagree", |v| {
            v["exact_work_csys_bounds"][0]["min_corner"][0] = json!(-3.0)
        }),
        ("forged absolute bounds", |v| {
            v["exact_work_csys_bounds"][0]["absolute_axis_bounds"][3] = json!(23.0)
        }),
    ]);
}

#[test]
fn controlled_reference_set_guard_is_preserved() {
    reject(&[
        ("wrong representation baseline", |v| {
            v["representation_baseline"] = json!("uncontrolled")
        }),
        ("missing controlled member", |v| {
            v["controlled_reference_set"]["member_tags"] = json!([])
        }),
        ("unaccounted extra member", |v| {
            v["controlled_reference_set"]["member_tags"] = json!([100, 999])
        }),
        ("null current part tag", |v| v["part_tag"] = json!(0)),
    ]);
}

#[test]
fn volume_requires_consistent_raw_query_units_and_body_identity() {
    reject(&[
        ("raw47 conflicts with volume summary", |v| {
            v["native_volume"]["raw47"][1] = json!(1.0)
        }),
        ("truncated raw47", |v| {
            v["native_volume"]["raw47"].as_array_mut().unwrap().pop();
        }),
        ("wrong native length unit", |v| {
            v["native_volume"]["unit_selector"] = json!(3)
        }),
        ("wrong result volume unit", |v| {
            v["native_volume"]["unit"] = json!("mm3")
        }),
        ("failed mass query", |v| {
            v["native_volume"]["return_code"] = json!(7)
        }),
        ("mass belongs to another body", |v| {
            v["native_volume"]["body_tags"] = json!([999])
        }),
    ]);
}

fn set_volume(observed: &mut Value, mm3: f64) {
    let m3 = json!(mm3 / 1e9);
    observed["native_volume"]["volume_m3"] = m3.clone();
    observed["native_volume"]["raw47"][1] = m3;
}

#[test]
fn frozen_volume_tolerance_is_max_absolute_and_relative_before_native_execution() {
    // The small third case is a local math control for the absolute floor only.
    // V = pi/4 mm3. It is not a third prepared or qualified native job.
    let absolute_floor = Case {
        id: "software-floor",
        origin: [0.0, 0.0, 0.0],
        diameter: 1.0,
        length: 1.0,
        radius: 0.5,
        bounds: [-0.5, -0.5, 0.0, 0.5, 0.5, 1.0],
        volume_mm3: 0.7853981633974483,
        area_mm2: 4.71238898038469,
    };
    for case in [A, B, absolute_floor] {
        let (intent, baseline) = fixture(case);
        assert!(check(&intent, &baseline).is_ok());
        let tolerance = 1e-5_f64.max(case.volume_mm3 * 1e-6);
        let mut within = baseline.clone();
        set_volume(&mut within, case.volume_mm3 + 0.5 * tolerance);
        assert!(
            check(&intent, &within).is_ok(),
            "{} within independently frozen volume tolerance",
            case.id
        );
        let mut outside = baseline;
        set_volume(&mut outside, case.volume_mm3 + 2.0 * tolerance);
        assert!(
            check(&intent, &outside).is_err(),
            "{} exceeds independently frozen volume tolerance",
            case.id
        );
    }
}
