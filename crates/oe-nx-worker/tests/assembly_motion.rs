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

use oe_contracts::{
    NxAssemblyMotionBudget, NxAssemblyMotionIntent, NxAssemblyMotionSample, NxAssemblyPoseTarget,
    NxAssemblySolidObservation,
};
use oe_nx_worker::assembly_motion::*;
fn budget() -> NxAssemblyMotionBudget {
    NxAssemblyMotionBudget {
        max_samples: 10,
        max_placed_bodies: 100,
        max_pair_checks: 1000,
    }
}
fn target(path: Vec<&str>, proto: &str) -> NxAssemblyPoseTarget {
    NxAssemblyPoseTarget {
        occurrence_path: path.into_iter().map(str::to_owned).collect(),
        prototype_id: proto.into(),
        local_to_root_m: Rigid::identity().0.to_vec(),
    }
}
#[test]
fn delta_uses_current_pose_and_native_units() {
    let current =
        Rigid::from_rotation_origin(&[0., -1., 0., 1., 0., 0., 0., 0., 1.], &[1., 2., 3.]).unwrap();
    let desired =
        Rigid::from_rotation_origin(&[1., 0., 0., 0., 1., 0., 0., 0., 1.], &[4., 5., 6.]).unwrap();
    for (unit, scale) in [("mm", 0.001), ("inch", 0.0254)] {
        let d = delta_for_target(&current.0, &desired.0, unit).unwrap();
        assert_eq!([d[0][0], d[0][1], d[1][0], d[1][1]], [0., 1., -1., 0.]);
        for (v, e) in [d[0][3], d[1][3], d[2][3]].into_iter().zip([2., 6., 3.]) {
            assert!((v - e / scale).abs() < 1e-9);
        }
        let mut si = d.into_iter().flatten().collect::<Vec<_>>();
        for k in [3, 7, 11] {
            si[k] *= scale;
        }
        assert!(
            Rigid::checked(&si)
                .unwrap()
                .compose(current)
                .unwrap()
                .near(desired)
        );
    }
    let mut bad = current.0;
    bad[0] = f64::NAN;
    assert!(delta_for_target(&bad, &desired.0, "mm").is_err());
    bad = Rigid::identity().0;
    bad[0] = -1.;
    assert!(delta_for_target(&bad, &desired.0, "mm").is_err());
    assert!(delta_for_target(&current.0, &desired.0, "metres").is_err());
}
#[test]
fn occurrence_paths_are_not_prototype_aliases() {
    let (_, mut g, b) = synthetic_graph();
    let mut other = g.occurrences[0].clone();
    other.name = "B".into();
    other.occurrence_id = "root/000001".into();
    other.part_occurrence_tag = 11;
    other.instance_tag = 21;
    g.occurrences.push(other);
    let s = NxAssemblyMotionSample {
        sample_id: "s".into(),
        time_s: 0.,
        targets: vec![target(vec!["B"], "SUB.prt")],
    };
    assert_eq!(
        resolve_targets(&g, &b, &s).unwrap()[0].occurrence_id,
        "root/000001"
    );
    let mut wrong = s.clone();
    wrong.targets[0].prototype_id = "ROOT.prt".into();
    assert!(resolve_targets(&g, &b, &wrong).is_err());
    wrong = s.clone();
    wrong.targets.push(wrong.targets[0].clone());
    assert!(resolve_targets(&g, &b, &wrong).is_err());
    g.occurrences[0].name = "B".into();
    assert!(resolve_targets(&g, &b, &s).is_err());
}
#[test]
fn parents_are_planned_before_children_without_caching_child_delta() {
    let (_, mut g, b) = synthetic_graph();
    let mut leaf = g.prototypes[1].clone();
    leaf.prototype_id = "LEAF.prt".into();
    leaf.native_part_tag = 3;
    g.prototypes.push(leaf);
    let mut child = g.occurrences[0].clone();
    child.name = "L".into();
    child.prototype_id = "LEAF.prt".into();
    child.occurrence_id = "root/000000/000000".into();
    child.parent_occurrence_id = Some("root/000000".into());
    child.part_occurrence_tag = 12;
    child.instance_tag = 22;
    child.local_to_parent_m = Rigid::identity().0.to_vec();
    child.raw_parent_origin = vec![0.; 3];
    child.raw_parent_csys = vec![1., 0., 0., 0., 1., 0., 0., 0., 1.];
    child.raw_parent_transform = Rigid::identity().0.to_vec();
    g.occurrences.push(child);
    let s = NxAssemblyMotionSample {
        sample_id: "s".into(),
        time_s: 0.,
        targets: vec![
            target(vec!["A", "L"], "LEAF.prt"),
            target(vec!["A"], "SUB.prt"),
        ],
    };
    let resolved = resolve_targets(&g, &b, &s).unwrap();
    assert_eq!(
        resolved
            .iter()
            .map(|t| t.occurrence_id.as_str())
            .collect::<Vec<_>>(),
        vec!["root/000000", "root/000000/000000"]
    );
}
#[test]
fn all_placed_solids_contribute_pairs_without_budget_truncation() {
    let (_, mut g, b) = synthetic_graph();
    let solid = |tag, owner, scale| NxAssemblySolidObservation {
        body_tag: tag,
        owning_part_tag: owner,
        is_occurrence: false,
        exact_box_corners_native: vec![1.; 24],
        exact_box_corners_m: vec![scale; 24],
        volume_m3: Some(1.),
    };
    g.prototypes[0].solids = vec![solid(101, 1, 0.001)];
    g.prototypes[1].solids = vec![solid(201, 2, 0.0254), solid(202, 2, 0.0254)];
    let mut other = g.occurrences[0].clone();
    other.name = "B".into();
    other.occurrence_id = "root/000001".into();
    other.part_occurrence_tag = 11;
    other.instance_tag = 21;
    g.occurrences.push(other);
    let p = plan_pairs(&g, &b, &budget(), 3).unwrap();
    assert_eq!(p.bodies.len(), 5);
    assert_eq!(p.pairs_per_sample, 10);
    assert_eq!(p.total_pair_checks, 30);
    assert_eq!(p.pairs().count(), 10);
    assert_eq!(
        p.bodies
            .iter()
            .filter(|b| b.prototype_body_tag == 201)
            .count(),
        2
    );
    let mut small = budget();
    small.max_pair_checks = 29;
    assert!(plan_pairs(&g, &b, &small, 3).is_err());
    small = budget();
    small.max_placed_bodies = 4;
    assert!(plan_pairs(&g, &b, &small, 3).is_err());
    let mut aliased = g.clone();
    aliased.prototypes[1].solids[0].body_tag = 101;
    assert!(
        plan_pairs(&aliased, &b, &budget(), 3)
            .unwrap_err()
            .to_string()
            .contains("motion_prototype_body_tag_aliased")
    );
    g.prototypes[1].sheet_count = 1;
    assert!(plan_pairs(&g, &b, &budget(), 3).is_err());
}
#[test]
fn raw_interference_codes_preserve_touching_and_errors() {
    assert_eq!(
        interference_status(0, 1).unwrap(),
        InterferenceStatus::Interference
    );
    assert_eq!(
        interference_status(0, 2).unwrap(),
        InterferenceStatus::NoInterference
    );
    assert_eq!(
        interference_status(0, 3).unwrap(),
        InterferenceStatus::Touching
    );
    for raw in [-1, 0, 4] {
        assert!(interference_status(0, raw).is_err());
    }
    assert!(interference_status(1, 2).is_err());
}
#[test]
fn intent_rejects_time_source_and_identity_gaps() {
    let mut i:NxAssemblyMotionIntent=serde_json::from_value(serde_json::json!({"schema":"oev2.nx-assembly-motion-intent/1","object":{"object_id":"assembly","revision":"r1","condition_scope":"fixture","source_sha256":"a".repeat(64)},"assembly_source":{"path":"assembly-manifest.json","sha256":"b".repeat(64)},"root_part":"root.prt","samples":[{"sample_id":"s0","time_s":0.,"targets":[{"occurrence_path":["A"],"prototype_id":"SUB.prt","local_to_root_m":Rigid::identity().0.to_vec()}]}],"budget":budget(),"source_refs":["fixture"]})).unwrap();
    validate_intent(&i).unwrap();
    let mut next = i.samples[0].clone();
    next.sample_id = "s1".into();
    i.samples.push(next);
    assert!(validate_intent(&i).is_err());
    i.samples[1].time_s = 1.;
    validate_intent(&i).unwrap();
    i.assembly_source.sha256 = "not-a-digest".into();
    assert!(validate_intent(&i).is_err());
}
#[test]
fn expected_tree_distinguishes_inherited_motion_explicit_children_and_unrelated_branches() {
    let (_, mut g, b) = synthetic_graph();
    let mut other = g.occurrences[0].clone();
    other.name = "B".into();
    other.occurrence_id = "root/000001".into();
    other.part_occurrence_tag = 11;
    other.instance_tag = 21;
    let original_b = Rigid::checked(&other.local_to_root_m).unwrap();
    g.occurrences.push(other);
    let mut leaf = g.prototypes[1].clone();
    leaf.prototype_id = "LEAF.prt".into();
    leaf.native_part_tag = 3;
    g.prototypes.push(leaf);
    let mut child = g.occurrences[0].clone();
    child.name = "L".into();
    child.prototype_id = "LEAF.prt".into();
    child.occurrence_id = "root/000000/000000".into();
    child.parent_occurrence_id = Some("root/000000".into());
    child.part_occurrence_tag = 12;
    child.instance_tag = 22;
    child.local_to_parent_m = Rigid::identity().0.to_vec();
    child.raw_parent_origin = vec![0.; 3];
    child.raw_parent_csys = vec![1., 0., 0., 0., 1., 0., 0., 0., 1.];
    child.raw_parent_transform = Rigid::identity().0.to_vec();
    g.occurrences.insert(0, child);
    let mut a = target(vec!["A"], "SUB.prt");
    a.local_to_root_m[3] = 4.;
    let mut s = NxAssemblyMotionSample {
        sample_id: "s".into(),
        time_s: 0.,
        targets: vec![a],
    };
    let e = expected_poses(&g, &b, &s).unwrap();
    assert_eq!(e.len(), 3);
    assert_eq!(e["root/000000"].point([0.; 3]).unwrap(), [4., 0., 0.]);
    assert_eq!(
        e["root/000000/000000"].point([0.; 3]).unwrap(),
        [4., 0., 0.]
    );
    assert!(e["root/000001"].near(original_b));
    let mut l = target(vec!["A", "L"], "LEAF.prt");
    l.local_to_root_m[3] = 9.;
    s.targets.insert(0, l);
    let e = expected_poses(&g, &b, &s).unwrap();
    assert_eq!(
        e["root/000000/000000"].point([0.; 3]).unwrap(),
        [9., 0., 0.]
    );
    assert_eq!(e["root/000000"].point([0.; 3]).unwrap(), [4., 0., 0.]);
    assert!(e["root/000001"].near(original_b));
}
