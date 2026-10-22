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

// Literal values from the source-frozen independent oracle, before native
// assembly observations. No STEP text count or current readback is an oracle.
#[test]
fn mixed_units_and_noncommuting_nested_transforms_match_frozen_oracle() {
    let rz90 = [0., -1., 0., 1., 0., 0., 0., 0., 1.];
    let rzm90 = [0., 1., 0., -1., 0., 0., 0., 0., 1.];
    let rx90 = [1., 0., 0., 0., 0., -1., 0., 1., 0.];
    let identity = [1., 0., 0., 0., 1., 0., 0., 0., 1.];
    let sub_a = Rigid::from_rotation_origin(&rz90, &[0.1, 0., 0.]).unwrap();
    let sub_b = Rigid::from_rotation_origin(&rzm90, &[0., 0.2, 0.]).unwrap();
    let metric = Rigid::from_rotation_origin(&identity, &[0.01, 0., 0.]).unwrap();
    let english = Rigid::from_rotation_origin(&rx90, &[0., 0.05, 0.]).unwrap();
    for (actual, expected) in [
        (
            sub_a.compose(metric).unwrap().point([0., 0., 0.]).unwrap(),
            [0.1, 0.01, 0.],
        ),
        (
            sub_b.compose(metric).unwrap().point([0., 0., 0.]).unwrap(),
            [0., 0.19, 0.],
        ),
        (
            sub_a.compose(english).unwrap().point([0., 0., 0.]).unwrap(),
            [0.05, 0., 0.],
        ),
        (
            sub_b.compose(english).unwrap().point([0., 0., 0.]).unwrap(),
            [0.05, 0.2, 0.],
        ),
    ] {
        for i in 0..3 {
            assert!((actual[i] - expected[i]).abs() <= 1e-10);
        }
    }
    // Native CSYS lists axis vectors, not row-major rotation entries.
    let native_csys = [0., 1., 0., -1., 0., 0., 0., 0., 1.];
    let from_uf = Rigid::from_parent_component(&[100., 0., 0.], &native_csys, "mm").unwrap();
    assert!(from_uf.near(sub_a));
    assert!(
        !Rigid::from_parent_component(&[100., 0., 0.], &rz90, "mm")
            .unwrap()
            .near(sub_a)
    );
    assert_eq!(unit_to_m("mm").unwrap(), 0.001);
    assert_eq!(unit_to_m("inch").unwrap(), 0.0254);
    assert!((10. * 20. * 30. * 0.001_f64.powi(3) - 6e-6).abs() < 1e-12);
    assert!((1. * 2. * 3. * 0.0254_f64.powi(3) - 0.0254 * 0.0508 * 0.0762).abs() < 1e-12);
    // Different native units use a non-unit w in root-absolute UF data.
    // Every origin coordinate is normalized; the SDK's printed z=y typo is
    // rejected by the independent .3m z translation witness.
    let raw = [
        1.,
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        0.,
        0.,
        0.,
        1.,
        300. * 0.03937007874015748,
        0.,
        0.,
        0.,
        0.03937007874015748,
    ];
    let global = Rigid::from_root_native(&raw, "mm").unwrap();
    assert!((global.point([0., 0., 0.]).unwrap()[2] - 0.3).abs() < 1e-10);
    assert!(
        sub_a
            .inverse()
            .unwrap()
            .compose(sub_a)
            .unwrap()
            .near(Rigid::identity())
    );
}

#[test]
fn unit_matrix_and_homogeneous_counterexamples_are_not_silent_identity() {
    assert!(unit_to_m("unknown").is_err());
    assert!(Rigid::checked(&[1.; 15]).is_err());
    let mut wrong = Rigid::identity().0;
    wrong[0] = 2.;
    assert!(Rigid::checked(&wrong).is_err());
    wrong = Rigid::identity().0;
    wrong[15] = 0.;
    assert!(Rigid::from_root_native(&wrong, "mm").is_err());
    wrong = Rigid::identity().0;
    wrong[3] = f64::NAN;
    assert!(Rigid::checked(&wrong).is_err());
    wrong = Rigid::identity().0;
    wrong[12] = 0.1;
    assert!(Rigid::checked(&wrong).is_err());
    // Reflections are documented by the native API and remain explicit.
    wrong = Rigid::identity().0;
    wrong[0] = -1.;
    assert!(Rigid::checked(&wrong).is_ok());
}

#[test]
fn incomplete_tree_prototype_units_load_and_raw_transform_are_rejected() {
    let (intent, graph, budget) = synthetic_graph();
    validate_observation(&graph, &budget).unwrap();
    check_fixture(&intent, &graph).unwrap();
    let mut missing = graph.clone();
    missing.prototypes.pop();
    assert!(validate_observation(&missing, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.prototypes[1].native_units = "unknown".into();
    assert!(validate_observation(&wrong, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.prototypes[1].fully_loaded = false;
    assert!(validate_observation(&wrong, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.occurrences[0].parent_occurrence_id = Some("absent".into());
    assert!(validate_observation(&wrong, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.occurrences[0].parent_occurrence_id = Some(wrong.occurrences[0].occurrence_id.clone());
    assert!(validate_observation(&wrong, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.occurrences[0].raw_parent_transform[3] += 1.;
    assert!(validate_observation(&wrong, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.occurrences[0].raw_root_transform[7] += 1.;
    assert!(validate_observation(&wrong, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.complete = false;
    assert!(validate_observation(&wrong, &budget).is_err());
    let mut wrong = graph.clone();
    wrong.occurrences.clear();
    assert!(check_fixture(&intent, &wrong).is_err());
    let mut wrong = graph.clone();
    wrong.occurrences[0].name = "different".into();
    assert!(check_fixture(&intent, &wrong).is_err());
    assert_eq!(graph.engineering_adoption, "not_assessed");
}

#[test]
fn nxopen_add_requires_complete_owned_status_and_exact_binding() {
    use oe_contracts::NxAssemblyAddOutcome;
    let good = NxAssemblyAddOutcome {
        normal_return: true,
        status_present: true,
        status_copied: true,
        status_released: true,
        unloaded_count: 0,
        component_tag: 101,
        definition_instance_tag: 102,
        prototype_tag: 2,
        definition_parent_tag: 1,
        reference_set: "actual-vendor-entire".into(),
        entire_reference_set: "actual-vendor-entire".into(),
        complete: true,
    };
    check_nxopen_add(&good, 1, 2).unwrap();
    for fault in 0..12 {
        let mut bad = good.clone();
        match fault {
            0 => bad.normal_return = false,
            1 => bad.status_present = false,
            2 => bad.status_copied = false,
            3 => bad.status_released = false,
            4 => bad.unloaded_count = 1,
            5 => bad.component_tag = 0,
            6 => bad.definition_instance_tag = 0,
            7 => bad.prototype_tag = 3,
            8 => bad.definition_parent_tag = 3,
            9 => bad.reference_set = "subset".into(),
            10 => bad.entire_reference_set = String::new(),
            _ => bad.complete = false,
        }
        assert!(check_nxopen_add(&bad, 1, 2).is_err(), "fault{fault}");
    }
}
#[test]
fn nxopen_profile_uses_proper_column_rotation_without_legacy_upgrade() {
    use oe_contracts::NxAssemblyConstructionProfile;
    let profile = NxAssemblyConstructionProfile {
        schema: "oev2.nx-assembly-construction-profile/1".into(),
        backend: "nxopen_loaded_part_entire".into(),
    };
    validate_construction_profile(&profile).unwrap();
    for (schema, backend) in [
        (
            "oev2.nx-assembly-construction-profile/2",
            "nxopen_loaded_part_entire",
        ),
        ("oev2.nx-assembly-construction-profile/1", "uf_fallback"),
    ] {
        assert!(
            validate_construction_profile(&NxAssemblyConstructionProfile {
                schema: schema.into(),
                backend: backend.into()
            })
            .is_err()
        );
    }
    let z = [0., -1., 0., 1., 0., 0., 0., 0., 1.];
    let columns = nxopen_column_rotation(&z).unwrap();
    assert_eq!(columns, [0., 1., 0., -1., 0., 0., 0., 0., 1.]);
    assert_eq!(
        Rigid::from_parent_component(&[0., 0., 0.], &columns, "mm").unwrap(),
        Rigid::from_rotation_origin(&z, &[0., 0., 0.]).unwrap()
    );
    assert!(nxopen_column_rotation(&[-1., 0., 0., 0., 1., 0., 0., 0., 1.]).is_err());
    assert!(nxopen_column_rotation(&[1.; 8]).is_err());
    let mut invalid = z;
    invalid[0] = f64::NAN;
    assert!(nxopen_column_rotation(&invalid).is_err());
}
#[test]
fn nxopen_params_require_explicit_action_version_and_profile() {
    use oe_nx_worker::geometry::Parameters;
    let (intent, _, budget) = synthetic_graph();
    let profile = serde_json::json!({"schema":"oev2.nx-assembly-construction-profile/1","backend":"nxopen_loaded_part_entire"});
    let base = serde_json::json!({"schema":"cad-agent.nx-native-assembly-params/v2","action":"create_assembly_fixture_nxopen",
        "intent":intent,"budget":budget,"construction_profile":profile});
    let parsed: Parameters = serde_json::from_value(base.clone()).unwrap();
    parsed.validate().unwrap();
    assert_eq!(parsed.action(), "create_assembly_fixture_nxopen");
    assert_eq!(parsed.parameter_schema(), NXOPEN_PARAMS_SCHEMA);
    let mut wrong = base.clone();
    wrong["schema"] = serde_json::json!(PARAMS_SCHEMA);
    assert!(
        serde_json::from_value::<Parameters>(wrong)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut missing = base.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("construction_profile");
    assert!(serde_json::from_value::<Parameters>(missing).is_err());
    let mut legacy = base;
    legacy["action"] = serde_json::json!("create_assembly_fixture");
    legacy["schema"] = serde_json::json!(PARAMS_SCHEMA);
    assert!(serde_json::from_value::<Parameters>(legacy).is_err());
}

#[test]
fn cycle_body_boundary_separates_known_subobjects_and_unknowns() {
    use oe_nx_worker::assembly::prototype_body_candidate;
    assert!(prototype_body_candidate(0, 70, 0, 70, 0, 1, 1, 0).unwrap());
    for subtype in 1..=6 {
        assert!(!prototype_body_candidate(0, 70, subtype, 70, 0, 1, 1, 0).unwrap());
    }
    assert!(!prototype_body_candidate(0, 70, 0, 70, 0, 2, 1, 0).unwrap());
    assert!(!prototype_body_candidate(0, 70, 0, 70, 0, 1, 1, 1).unwrap());
    for (typecode, kind, subtype, ownercode, owner, occurrence) in [
        (1, 70, 0, 0, 1, 0),
        (0, -1, 0, 0, 1, 0),
        (0, 70, -1, 0, 1, 0),
        (0, 70, 7, 0, 1, 0),
        (0, 71, 0, 0, 1, 0),
        (0, 70, 0, 1, 1, 0),
        (0, 70, 0, 0, 0, 0),
        (0, 70, 0, 0, 1, 2),
    ] {
        assert!(
            prototype_body_candidate(typecode, kind, subtype, 70, ownercode, owner, 1, occurrence)
                .is_err()
        );
    }
}
