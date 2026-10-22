use oe_contracts::{
    NxAssemblyBudget, NxAssemblyFixtureIntent, NxAssemblyObservation,
    NxAssemblyOccurrenceObservation, NxAssemblyPrototypeObservation,
};
use oe_nx_worker::assembly::{
    NXOPEN_PARAMS_SCHEMA, PARAMS_SCHEMA, check_nxopen_add, nxopen_column_rotation,
    validate_construction_profile,
};
use oe_nx_worker::assembly::{Rigid, unit_to_m};
use oe_nx_worker::assembly::{check_fixture, validate_observation};

fn synthetic_graph() -> (
    NxAssemblyFixtureIntent,
    NxAssemblyObservation,
    NxAssemblyBudget,
) {
    let intent: NxAssemblyFixtureIntent=serde_json::from_value(serde_json::json!({
        "schema":"oev2.nx-native-assembly-fixture-intent/1","oracle_sha256":"a".repeat(64),"root_prototype_id":"ROOT",
        "prototypes":[{"prototype_id":"ROOT","native_units":"mm","box_corner_native":[],"box_lengths_native":[]},
            {"prototype_id":"SUB","native_units":"inch","box_corner_native":[],"box_lengths_native":[]}],
        "instances":[{"parent_prototype_id":"ROOT","prototype_id":"SUB","name":"A","origin_m":[0.1,0.2,0.3],"rotation":[0.,-1.,0.,1.,0.,0.,0.,0.,1.]}]
    })).unwrap();
    let pose =
        Rigid::from_rotation_origin(&[0., -1., 0., 1., 0., 0., 0., 0., 1.], &[0.1, 0.2, 0.3])
            .unwrap();
    let raw = [
        0., -1., 0., 100., 1., 0., 0., 200., 0., 0., 1., 300., 0., 0., 0., 1.,
    ];
    let proto = |id: &str, tag: u64, unit: &str| NxAssemblyPrototypeObservation {
        prototype_id: format!("{id}.prt"),
        part_path: format!("C:/synthetic/{id}.prt"),
        native_part_tag: tag,
        native_units: unit.into(),
        unit_to_m: unit_to_m(unit).unwrap(),
        load_state_raw: 1,
        fully_loaded: true,
        solids: vec![],
        sheet_count: 0,
    };
    let graph = NxAssemblyObservation {
        schema: "oev2.nx-native-assembly-observation/1".into(),
        root_prototype_id: "ROOT.prt".into(),
        prototypes: vec![proto("ROOT", 1, "mm"), proto("SUB", 2, "inch")],
        occurrences: vec![NxAssemblyOccurrenceObservation {
            occurrence_id: "root/000000".into(),
            parent_occurrence_id: None,
            prototype_id: "SUB.prt".into(),
            part_occurrence_tag: 10,
            instance_tag: 20,
            name: "A".into(),
            reference_set: "None".into(),
            raw_parent_origin: vec![100., 200., 300.],
            raw_parent_csys: vec![0., 1., 0., -1., 0., 0., 0., 0., 1.],
            raw_parent_transform: raw.to_vec(),
            raw_root_transform: raw.to_vec(),
            local_to_parent_m: pose.0.to_vec(),
            local_to_root_m: pose.0.to_vec(),
        }],
        complete: true,
        source_role: "independent_software_fixture".into(),
        engineering_adoption: "not_assessed".into(),
    };
    (
        intent,
        graph,
        NxAssemblyBudget {
            max_occurrences: 128,
            max_prototypes: 128,
            max_depth: 64,
        },
    )
}

use oe_contracts::{NxAssemblyPositionObservation, NxAssemblyPositionOverride};
use oe_nx_worker::assembly_position::{SCHEMA, validate_position_observation};
fn positions(g: NxAssemblyObservation) -> NxAssemblyPositionObservation {
    let overrides = g
        .occurrences
        .iter()
        .map(|o| NxAssemblyPositionOverride {
            occurrence_id: o.occurrence_id.clone(),
            part_occurrence_tag: o.part_occurrence_tag,
            raw_type: 0,
            parent_occurrence_tag: 0,
        })
        .collect();
    NxAssemblyPositionObservation {
        schema: SCHEMA.into(),
        root_prototype_id: g.root_prototype_id,
        root_occurrence_tag: 99,
        prototypes: g.prototypes,
        occurrences: g.occurrences,
        position_overrides: overrides,
        complete: g.complete,
        source_role: g.source_role,
        engineering_adoption: g.engineering_adoption,
    }
}
#[test]
fn override_is_explicitly_versioned_and_ancestor_bound() {
    let (_, mut g, b) = synthetic_graph();
    validate_observation(&g, &b).unwrap();
    validate_position_observation(&positions(g.clone()), &b).unwrap();
    // Actual translation changes 0.1m -> 0.4m; definition remains 100mm.
    g.occurrences[0].local_to_parent_m[3] = 0.4;
    g.occurrences[0].local_to_root_m[3] = 0.4;
    g.occurrences[0].raw_root_transform[3] = 400.;
    assert!(validate_observation(&g, &b).is_err());
    let mut good = positions(g);
    assert!(validate_position_observation(&good, &b).is_err());
    good.position_overrides[0].raw_type = 2;
    good.position_overrides[0].parent_occurrence_tag = 99;
    validate_position_observation(&good, &b).unwrap();
    for fault in 0..10 {
        let mut x = good.clone();
        match fault {
            0 => x.position_overrides.clear(),
            1 => x.position_overrides.push(x.position_overrides[0].clone()),
            2 => x.position_overrides[0].part_occurrence_tag = 11,
            3 => x.position_overrides[0].parent_occurrence_tag = 10,
            4 => x.position_overrides[0].parent_occurrence_tag = 123,
            5 => x.position_overrides[0].raw_type = 1,
            6 => x.position_overrides[0].raw_type = 5,
            7 => x.position_overrides[0].raw_type = 0,
            8 => x.occurrences[0].raw_parent_csys[0] = f64::NAN,
            9 => x.root_occurrence_tag = 10,
            _ => unreachable!(),
        }
        assert!(validate_position_observation(&x, &b).is_err(), "{fault}");
    }
}

#[test]
fn parent_override_does_not_exempt_unoverridden_child() {
    let (_, mut g, b) = synthetic_graph();
    let mut proto = g.prototypes[1].clone();
    proto.prototype_id = "LEAF.prt".into();
    proto.native_part_tag = 3;
    g.prototypes.push(proto);
    let mut child = g.occurrences[0].clone();
    child.occurrence_id = "root/000000/000000".into();
    child.parent_occurrence_id = Some("root/000000".into());
    child.prototype_id = "LEAF.prt".into();
    child.name = "L".into();
    child.part_occurrence_tag = 12;
    child.instance_tag = 22;
    child.raw_parent_origin = vec![0.; 3];
    child.raw_parent_csys = vec![1., 0., 0., 0., 1., 0., 0., 0., 1.];
    child.raw_parent_transform = Rigid::identity().0.to_vec();
    child.local_to_parent_m = Rigid::identity().0.to_vec();
    g.occurrences.push(child);
    let initial = positions(g);
    validate_position_observation(&initial, &b).unwrap();
    let mut moved = initial.clone();
    for o in &mut moved.occurrences {
        o.local_to_root_m[3] = 0.4;
        o.raw_root_transform[3] = 400.;
    }
    moved.occurrences[0].local_to_parent_m[3] = 0.4;
    moved.position_overrides[0].raw_type = 2;
    moved.position_overrides[0].parent_occurrence_tag = 99;
    validate_position_observation(&moved, &b).unwrap();
    let sample = oe_contracts::NxAssemblyMotionSample {
        sample_id: "s".into(),
        time_s: 0.,
        targets: vec![oe_contracts::NxAssemblyPoseTarget {
            occurrence_path: vec!["A".into()],
            prototype_id: "SUB.prt".into(),
            local_to_root_m: moved.occurrences[0].local_to_root_m.clone(),
        }],
    };
    oe_nx_worker::assembly_motion::check_position_sample(&initial, &moved, &b, &sample).unwrap();
    // Child local translation = 1m, inherited parent Rz90 maps it to root +y.
    moved.occurrences[1].local_to_parent_m[3] = 1.;
    moved.occurrences[1].local_to_root_m[7] += 1.;
    moved.occurrences[1].raw_root_transform[7] += 1000.;
    assert!(
        validate_position_observation(&moved, &b)
            .unwrap_err()
            .to_string()
            .contains("assembly_native_parent_frame_differs")
    );
    moved.position_overrides[1].raw_type = 2;
    moved.position_overrides[1].parent_occurrence_tag = 10;
    validate_position_observation(&moved, &b).unwrap();
    // Valid actual override is still an unexpected motion of an unselected child.
    assert!(
        oe_nx_worker::assembly_motion::check_position_sample(&initial, &moved, &b, &sample)
            .is_err()
    );
}

#[test]
fn position_source_role_is_not_event_phase() {
    let (_, graph, budget) = synthetic_graph();
    let mut p = positions(graph);
    p.source_role = "motion_sample_before".into();
    assert!(validate_position_observation(&p, &budget).is_err());
    p.source_role = "reference_observation".into();
    validate_position_observation(&p, &budget).unwrap();
}

fn persistent_nested() -> (NxAssemblyPositionObservation, NxAssemblyBudget) {
    let (_, mut g, b) = synthetic_graph();
    let mut proto = g.prototypes[1].clone();
    proto.prototype_id = "LEAF.prt".into();
    proto.native_part_tag = 3;
    g.prototypes.push(proto);
    let mut child = g.occurrences[0].clone();
    child.occurrence_id = "root/000000/000000".into();
    child.parent_occurrence_id = Some("root/000000".into());
    child.prototype_id = "LEAF.prt".into();
    child.name = "L".into();
    child.part_occurrence_tag = 12;
    child.instance_tag = 22;
    child.raw_parent_origin = vec![0.; 3];
    child.raw_parent_csys = vec![1., 0., 0., 0., 1., 0., 0., 0., 1.];
    child.raw_parent_transform = Rigid::identity().0.to_vec();
    child.local_to_parent_m = Rigid::identity().0.to_vec();
    g.occurrences.push(child);
    let mut p = positions(g);
    p.position_overrides[0].raw_type = 2;
    p.position_overrides[0].parent_occurrence_tag = 99;
    p.position_overrides[1].raw_type = 2;
    p.position_overrides[1].parent_occurrence_tag = 10;
    (p, b)
}
#[test]
fn reopened_identity_ignores_process_tags_and_enumeration_order() {
    use oe_nx_worker::assembly_position::persistence::compare;
    let (before, b) = persistent_nested();
    let mut after = before.clone();
    after.root_occurrence_tag += 1000;
    for p in &mut after.prototypes {
        p.native_part_tag += 1000;
        p.part_path = p.part_path.replace("synthetic", "new_namespace");
    }
    for o in &mut after.occurrences {
        o.part_occurrence_tag += 1000;
        o.instance_tag += 1000;
        o.occurrence_id = format!("new:{}", o.occurrence_id);
        o.parent_occurrence_id = o.parent_occurrence_id.as_ref().map(|p| format!("new:{p}"));
    }
    for o in &mut after.position_overrides {
        o.part_occurrence_tag += 1000;
        if o.parent_occurrence_tag != 0 {
            o.parent_occurrence_tag += 1000;
        }
        o.occurrence_id = format!("new:{}", o.occurrence_id);
    }
    after.prototypes.reverse();
    after.occurrences.reverse();
    after.position_overrides.reverse();
    compare(&before, &after, &b).unwrap();
    let mut bad = after.clone();
    bad.position_overrides
        .iter_mut()
        .find(|x| x.occurrence_id.ends_with("/000000/000000"))
        .unwrap()
        .parent_occurrence_tag = after.root_occurrence_tag;
    assert!(
        compare(&before, &bad, &b)
            .unwrap_err()
            .to_string()
            .contains("override_identity")
    );
    let mut bad = after.clone();
    let child = bad.occurrences.iter_mut().find(|x| x.name == "L").unwrap();
    child.raw_parent_origin[0] = 1.;
    child.raw_parent_transform[3] = 1.;
    assert!(
        compare(&before, &bad, &b)
            .unwrap_err()
            .to_string()
            .contains("definition_changed")
    );
    let mut bad = after;
    bad.occurrences
        .iter_mut()
        .find(|x| x.name == "L")
        .unwrap()
        .name = "different".into();
    assert!(compare(&before, &bad, &b).is_err());
}
#[test]
fn persistent_paths_reject_ambiguity_and_preserve_geometry_multiplicity() {
    use oe_nx_worker::assembly_position::persistence::compare;
    let (_, g, b) = synthetic_graph();
    let mut before = positions(g);
    let solid = |tag, native: f64| oe_contracts::NxAssemblySolidObservation {
        body_tag: tag,
        owning_part_tag: 2,
        is_occurrence: false,
        exact_box_corners_native: vec![native; 24],
        exact_box_corners_m: vec![native * 0.0254; 24],
        volume_m3: Some(1e-6),
    };
    before.prototypes[1].solids = vec![solid(101, 1.), solid(102, 2.)];
    let mut after = before.clone();
    after.prototypes[1].solids.reverse();
    for s in &mut after.prototypes[1].solids {
        s.body_tag += 1000;
    }
    compare(&before, &after, &b).unwrap();
    after.prototypes[1].solids[0].volume_m3 = Some(2e-6);
    assert!(compare(&before, &after, &b).is_err());
    let mut ambiguous = before;
    let mut node = ambiguous.occurrences[0].clone();
    node.occurrence_id = "another".into();
    node.part_occurrence_tag = 11;
    node.instance_tag = 21;
    ambiguous.occurrences.push(node);
    let mut meta = ambiguous.position_overrides[0].clone();
    meta.occurrence_id = "another".into();
    meta.part_occurrence_tag = 11;
    ambiguous.position_overrides.push(meta);
    assert!(
        compare(&ambiguous, &ambiguous, &b)
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
}
