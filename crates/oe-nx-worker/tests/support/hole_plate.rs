//! Original independently authored one/two-hole plate observations, shared without changing their values.
#![allow(dead_code, unused_imports)]
use anyhow::{Result, bail};
use oe_contracts::*;
use oe_nx_protocol::RelativePath;
use oe_nx_worker::geometry::{self, NativeApi, PartInfo, Workspace, feature_delete as delete};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub fn intent() -> NxFeatureDeleteIntent1 {
    let circle = |id: &str, x: f64| {
        json!({"feature_id":id,"center":[x,0.,8.],"normal":[0.,0.,1.],"radius":6.,
        "center_tolerance":1e-7,"radius_tolerance":1e-7,"angular_tolerance_radians":1e-7,
        "unit":"mm","local_frame":"part_absolute","full_circle":true})
    };
    serde_json::from_value(json!({"schema":delete::INTENT_SCHEMA,
        "object":{"object_id":"plate","revision":"v1","source_sha256":"a".repeat(64),"condition_scope":"development_replay"},
        "source":{"schema":"oev2.nx-assembly-source-closure/1","root_part":"root.prt","max_total_bytes":3,
            "files":[{"path":"root.prt","bytes":1,"sha256":"b".repeat(64)},{"path":"plate.prt","bytes":1,"sha256":"a".repeat(64)},
            {"path":"pin.prt","bytes":1,"sha256":"c".repeat(64)}]},
        "target_part":"plate.prt","target":circle("hole-left",-20.),"retained":circle("hole-right",20.),
        "plate_bounds_mm":[-40.,-20.,0.,40.,20.,8.],"feature_type":"CYLINDER","feature_location_mm":[-20.,0.,-1.],
        "source_refs":["frozen-P05-two-hole-plate"]})).unwrap()
}
pub fn budget() -> NxAssemblyBudget {
    NxAssemblyBudget {
        max_occurrences: 16,
        max_prototypes: 8,
        max_depth: 8,
    }
}
pub fn detail(deleted: bool) -> Value {
    let metadata = |subtype| {
        json!({"type_query":{"return_code":0,"type_raw":70,"subtype_raw":subtype},
        "owning_part_query":{"value_valid":true,"matches_expected_owner":true}})
    };
    let mut entries = Vec::new();
    for (x, base) in [(-20., 710), (20., 720)] {
        if deleted && x == -20. {
            continue;
        }
        for (z, id) in [(0., base), (8., base + 1)] {
            entries.push(json!({"body_tag":300,"edge_tag":id,"metadata":metadata(3),
                "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,"is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":true},
                "arc":{"value_valid":true,"is_periodic":true,"limits":[0.,std::f64::consts::TAU],"radius":6.,
                    "center":[x,0.,z],"x_axis":[1.,0.,0.],"y_axis":[0.,1.,0.]}}));
        }
    }
    // Complete straight-edge catalogue remains present; it must not be silently
    // dropped just because the target selector is circular.
    for id in 800..812 {
        entries.push(json!({"body_tag":300,"edge_tag":id,"metadata":metadata(3),
            "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,"is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":false}}));
    }
    let edge_ids: Vec<_> = entries.iter().map(|e| e["edge_tag"].clone()).collect();
    let mut faces = Vec::new();
    for (id, point, dir, face_box) in [
        (
            900,
            [-40., 0., 4.],
            [1., 0., 0.],
            [-40., -20., 0., -40., 20., 8.],
        ),
        (
            901,
            [40., 0., 4.],
            [1., 0., 0.],
            [40., -20., 0., 40., 20., 8.],
        ),
        (
            902,
            [0., -20., 4.],
            [0., 1., 0.],
            [-40., -20., 0., 40., -20., 8.],
        ),
        (
            903,
            [0., 20., 4.],
            [0., 1., 0.],
            [-40., 20., 0., 40., 20., 8.],
        ),
        (
            904,
            [0., 0., 0.],
            [0., 0., 1.],
            [-40., -20., 0., 40., 20., 0.],
        ),
        (
            905,
            [0., 0., 8.],
            [0., 0., 1.],
            [-40., -20., 8., 40., 20., 8.],
        ),
    ] {
        faces.push(json!({"body_tag":300,"face_tag":id,"query_code":0,"surface_type":22,
            "axis_point":point,"axis_direction":dir,"radius":0.,"face_bounds":face_box,"normal_direction":1}));
    }
    for (x, id) in [(-20., 910), (20., 920)] {
        if deleted && x == -20. {
            continue;
        }
        faces.push(
            json!({"body_tag":300,"face_tag":id,"query_code":0,"surface_type":16,
            "axis_point":[x,0.,-1.],"axis_direction":[0.,0.,1.],"radius":6.,
            "face_bounds":[x-6.,-6.,0.,x+6.,6.,8.],"normal_direction":-1}),
        );
    }
    json!({"part_tag":3,"path":"C:/owned/plate.prt","units":"mm","solid_count":1,"solid_tags":[300],"sheet_count":0,
        "bbox":[[-40.,-20.,0.,40.,20.,8.]],"exact_work_csys_bounds":[{
            "min_corner":[-40.,-20.,0.],"directions":[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],"distances":[80.,40.,8.],
            "absolute_axis_bounds":[-40.,-20.,0.,40.,20.,8.]}],
        "model_edges":{"schema":"oev2.nx-model-edge-observation/1","complete":true,"unit":"mm",
            "coordinate_basis":"UF_EVAL ABS coordinates; single-part solid edges","solid_type_raw":70,"edge_subtype_raw":3,
            "bodies":[{"body_tag":300,"metadata":metadata(0),"edge_list_return_code":0,"capture_complete":true,"edge_tags":edge_ids}],"entries":entries},
        "solid_face_inventory_complete":true,"solid_face_inventory":faces})
}
pub fn inventory() -> NxFeatureInventory1 {
    let feature = |tag, kind: &str, x, faces: Vec<u64>| NxFeatureObservation1 {
        feature_tag: tag,
        owning_part_tag: 3,
        body_tag: 300,
        is_occurrence: false,
        feature_type: kind.into(),
        location_mm: vec![x, 0., -1.],
        face_tags: faces,
        query_codes: vec![0; 5],
        complete: true,
    };
    NxFeatureInventory1 {
        schema: delete::INVENTORY_SCHEMA.into(),
        part_tag: 3,
        body_tag: 300,
        complete: true,
        features: vec![
            feature(500, "BLOCK", 0., vec![900, 901, 902, 903, 904, 905]),
            feature(501, "CYLINDER", -20., vec![910]),
            feature(502, "CYLINDER", 20., vec![920]),
        ],
    }
}
pub fn outcome() -> NxFeatureDeleteOutcome1 {
    serde_json::from_value(json!({"schema":delete::OUTCOME_SCHEMA,"part_tag":3,"body_tag":300,"feature_tag":501,
        "call_entered":true,"call_returned":true,"return_code":0,"list_create_code":0,"list_put_code":0,
        "list_count_code":0,"list_item_code":0,"list_delete_code":0,"list_count":1,"list_item_tag":501,"update_code":0,"complete":true})).unwrap()
}
pub fn assembly(deleted: bool) -> NxAssemblyObservation {
    let proto = |id: &str, tag: u64, bounds: [f64; 6], volume: Option<f64>| {
        let mut corners = Vec::new();
        for x in [bounds[0], bounds[3]] {
            for y in [bounds[1], bounds[4]] {
                for z in [bounds[2], bounds[5]] {
                    corners.extend([x, y, z]);
                }
            }
        }
        NxAssemblyPrototypeObservation {
            prototype_id: id.into(),
            part_path: format!("C:/owned/{id}"),
            native_part_tag: tag,
            native_units: "mm".into(),
            unit_to_m: 0.001,
            load_state_raw: 1,
            fully_loaded: true,
            sheet_count: 0,
            solids: if tag == 1 {
                vec![]
            } else {
                vec![NxAssemblySolidObservation {
                    body_tag: tag * 100,
                    owning_part_tag: tag,
                    is_occurrence: false,
                    exact_box_corners_m: corners.iter().map(|n| n * 0.001).collect(),
                    exact_box_corners_native: corners,
                    volume_m3: volume,
                }]
            },
        }
    };
    let occurrence = |index: u64, name: &str, proto: &str| {
        let pose = vec![
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        NxAssemblyOccurrenceObservation {
            occurrence_id: format!("root/{index:06}"),
            parent_occurrence_id: None,
            prototype_id: proto.into(),
            part_occurrence_tag: 1000 + index,
            instance_tag: 2000 + index,
            name: name.into(),
            reference_set: "None".into(),
            raw_parent_origin: vec![0.; 3],
            raw_parent_csys: vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
            raw_parent_transform: pose.clone(),
            raw_root_transform: pose.clone(),
            local_to_parent_m: pose.clone(),
            local_to_root_m: pose,
        }
    };
    // Independent numeric oracle: 80*40*8 minus one/two pi*6^2*8 mm3.
    let v = if deleted {
        0.00002469522131576614
    } else {
        0.00002379044263153228
    };
    NxAssemblyObservation {
        schema: "oev2.nx-native-assembly-observation/1".into(),
        root_prototype_id: "root.prt".into(),
        prototypes: vec![
            proto("root.prt", 1, [0.; 6], None),
            proto("pin.prt", 2, [0., 0., 0., 10., 10., 10.], Some(0.000001)),
            proto("plate.prt", 3, [-40., -20., 0., 40., 20., 8.], Some(v)),
        ],
        occurrences: vec![
            occurrence(0, "PIN", "pin.prt"),
            occurrence(1, "PLATE", "plate.prt"),
        ],
        complete: true,
        source_role: "reference_observation".into(),
        engineering_adoption: "not_assessed".into(),
    }
}
pub fn reference_sets(a: &NxAssemblyObservation) -> NxAssemblyReferenceSets2 {
    serde_json::from_value(json!({"schema":"oev2.nx-assembly-reference-sets/2","api":"UF_ASSEM_ask_component_data",
        "root_prototype_id":a.root_prototype_id,"complete":true,"members":a.occurrences.iter().map(|o|json!({
            "occurrence_id":o.occurrence_id,"part_occurrence_tag":o.part_occurrence_tag,"instance_tag":o.instance_tag,
            "instance_reference_set":"None","part_occurrence_reference_set":"None"})).collect::<Vec<_>>()})).unwrap()
}
