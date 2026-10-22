//! Frozen P04 software controls; none of these are native NX qualification.
use anyhow::{Result, bail};
use oe_contracts::*;
use oe_nx_protocol::RelativePath;
use oe_nx_worker::{
    assembly,
    geometry::{self, NativeApi, PartInfo, Workspace, component_copy},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn fixture() -> (
    NxComponentCopyIntent1,
    NxAssemblyBudget,
    NxAssemblyObservation,
    NxComponentCopyOutcome1,
    NxAssemblyObservation,
) {
    let budget = NxAssemblyBudget {
        max_occurrences: 16,
        max_prototypes: 8,
        max_depth: 8,
    };
    let intent: NxComponentCopyIntent1=serde_json::from_value(json!({
        "schema":"oev2.nx-component-copy-intent/1","object":{"object_id":"pilot","revision":"copy1","source_sha256":"b".repeat(64),"condition_scope":"development_replay"},
        "source":{"schema":"oev2.nx-assembly-source-closure/1","root_part":"root.prt","max_total_bytes":3,
            "files":[{"path":"root.prt","bytes":1,"sha256":"a".repeat(64)},{"path":"pin.prt","bytes":1,"sha256":"a".repeat(64)},{"path":"plate.prt","bytes":1,"sha256":"a".repeat(64)}]},
        "source_occurrence_path":["PIN_A"],"new_instance_name":"PIN_C","source_refs":["P04-original"]})).unwrap();
    let proto = |id: &str, tag: u64| {
        let corners: Vec<f64> = [0., 10.]
            .into_iter()
            .flat_map(|x| {
                [0., 10.]
                    .into_iter()
                    .flat_map(move |y| [0., 10.].into_iter().flat_map(move |z| [x, y, z]))
            })
            .collect();
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
                    exact_box_corners_m: corners.iter().map(|x| x * 0.001).collect(),
                    exact_box_corners_native: corners,
                    volume_m3: Some(0.000001),
                }]
            },
        }
    };
    let occurrence = |name: &str, index: u64, proto: &str, x: f64| {
        let raw = vec![
            1.,
            0.,
            0.,
            x * 1000.,
            0.,
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
        ];
        let pose = vec![
            1., 0., 0., x, 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        NxAssemblyOccurrenceObservation {
            occurrence_id: format!("root/{index:06}"),
            parent_occurrence_id: None,
            prototype_id: proto.into(),
            part_occurrence_tag: 1000 + index,
            instance_tag: 2000 + index,
            name: name.into(),
            reference_set: "None".into(),
            raw_parent_origin: vec![x * 1000., 0., 0.],
            raw_parent_csys: vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
            raw_parent_transform: raw.clone(),
            raw_root_transform: raw,
            local_to_parent_m: pose.clone(),
            local_to_root_m: pose,
        }
    };
    let before = NxAssemblyObservation {
        schema: "oev2.nx-native-assembly-observation/1".into(),
        root_prototype_id: "root.prt".into(),
        prototypes: vec![
            proto("root.prt", 1),
            proto("pin.prt", 2),
            proto("plate.prt", 3),
        ],
        occurrences: vec![
            occurrence("PIN_A", 0, "pin.prt", -0.02),
            occurrence("PIN_B", 1, "pin.prt", 0.02),
            occurrence("PLATE", 2, "plate.prt", 0.),
        ],
        complete: true,
        source_role: "reference_observation".into(),
        engineering_adoption: "not_assessed".into(),
    };
    let mut after = before.clone();
    after
        .occurrences
        .push(occurrence("PIN_C", 3, "pin.prt", -0.02));
    let call:NxComponentCopyOutcome1=serde_json::from_value(json!({"schema":"oev2.nx-component-copy-outcome/1","call_entered":true,"call_returned":true,"wrapper_code":0,"exception_code":0,
        "source_component_tag":1000,"source_instance_tag":2000,"source_parent_tag":1,"source_prototype_tag":2,
        "returned_count":1,"returned_component_tags":[1003],"new_instance_tag":2003,"new_parent_tag":1,"new_prototype_tag":2,
        "rename_attempted":true,"rename_code":0,"complete":true})).unwrap();
    (intent, budget, before, call, after)
}
fn reference_sets(v: &NxAssemblyObservation) -> NxAssemblyReferenceSets2 {
    NxAssemblyReferenceSets2 {
        schema: "oev2.nx-assembly-reference-sets/2".into(),
        api: "UF_ASSEM_ask_component_data".into(),
        root_prototype_id: v.root_prototype_id.clone(),
        complete: true,
        members: v
            .occurrences
            .iter()
            .map(|o| NxAssemblyReferenceSetMember2 {
                occurrence_id: o.occurrence_id.clone(),
                part_occurrence_tag: o.part_occurrence_tag,
                instance_tag: o.instance_tag,
                instance_reference_set: "None".into(),
                part_occurrence_reference_set: "None".into(),
            })
            .collect(),
    }
}
#[test]
fn copy_change_accepts_only_new_identity_and_preserved_source() {
    let (i, b, before, call, mut after) = fixture();
    assembly::validate_observation(&before, &b).unwrap();
    for (index, o) in after.occurrences.iter_mut().enumerate() {
        o.occurrence_id = format!("renumbered/{index:06}");
    }
    after.occurrences.reverse();
    let report = component_copy::check_change(&i, &b, &before, &call, &after).unwrap();
    assert_eq!(report["new_engineering_adoption"], "not_established");
    assert_eq!(
        report["independent_saved_reopen"],
        "required_separate_process"
    );
    component_copy::check_reference_sets(
        &before,
        &reference_sets(&before),
        &after,
        &reference_sets(&after),
        &call,
    )
    .unwrap();
}
#[test]
fn copy_change_rejects_partial_and_wrong_lineage() {
    let (i, b, before, call, after) = fixture();
    let mut cases = Vec::new();
    macro_rules! fault {
        ($field:ident,$value:expr) => {{
            let mut c = call.clone();
            c.$field = $value;
            cases.push(c);
        }};
    }
    fault!(call_entered, false);
    fault!(call_returned, false);
    fault!(complete, false);
    fault!(wrapper_code, -1);
    fault!(exception_code, -1);
    fault!(returned_count, 0);
    fault!(returned_count, 2);
    fault!(returned_component_tags, vec![]);
    fault!(returned_component_tags, vec![1003, 1004]);
    fault!(source_component_tag, 1001);
    fault!(source_instance_tag, 2001);
    fault!(source_parent_tag, 3);
    fault!(source_prototype_tag, 3);
    fault!(new_parent_tag, 3);
    fault!(new_prototype_tag, 3);
    fault!(new_instance_tag, 2000);
    fault!(returned_component_tags, vec![1000]);
    fault!(rename_attempted, false);
    fault!(rename_code, 17);
    for (index, c) in cases.into_iter().enumerate() {
        assert!(
            component_copy::check_change(&i, &b, &before, &c, &after).is_err(),
            "frozen call fault {index}"
        );
    }
}
#[test]
fn changed_originals_new_pose_or_geometry_cannot_pass_as_copy() {
    let (i, b, before, call, after) = fixture();
    let mut cases = Vec::new();
    let mut x = after.clone();
    x.occurrences.remove(1);
    cases.push(x);
    let mut x = after.clone();
    x.occurrences[0].name = "renamed_source".into();
    cases.push(x);
    let mut x = after.clone();
    x.occurrences[3].name = "PIN_B".into();
    cases.push(x);
    let mut x = after.clone();
    x.occurrences[3].local_to_root_m[3] += 0.001;
    cases.push(x);
    let mut x = after.clone();
    x.occurrences[3].reference_set = "Other".into();
    cases.push(x);
    let mut x = after.clone();
    x.prototypes[1].solids[0].volume_m3 = Some(0.000002);
    cases.push(x);
    let mut x = after.clone();
    x.prototypes[1].sheet_count = 1;
    cases.push(x);
    let mut x = after.clone();
    x.occurrences[3].part_occurrence_tag = 1000;
    cases.push(x);
    let mut x = after.clone();
    x.complete = false;
    cases.push(x);
    for (index, x) in cases.into_iter().enumerate() {
        assert!(
            component_copy::check_change(&i, &b, &before, &call, &x).is_err(),
            "frozen inventory fault {index}"
        );
    }
}
#[test]
fn both_reference_set_observations_must_remain_source_consistent() {
    let (_, _, before, call, after) = fixture();
    let a = reference_sets(&before);
    let b = reference_sets(&after);
    component_copy::check_reference_sets(&before, &a, &after, &b, &call).unwrap();
    for member in [0, 3] {
        let mut x = b.clone();
        x.members[member].part_occurrence_reference_set = "Entire Part".into();
        assert!(component_copy::check_reference_sets(&before, &a, &after, &x, &call).is_err());
    }
    let mut x = b;
    x.members.pop();
    assert!(component_copy::check_reference_sets(&before, &a, &after, &x, &call).is_err());
}
#[test]
fn copy_intent_and_parameter_profile_reject_ambiguous_selection() {
    let (i, b, before, call, after) = fixture();
    let params:geometry::Parameters=serde_json::from_value(json!({"action":"copy_assembly_component","schema":component_copy::PARAMS_SCHEMA,"intent":{"path":"copy.json","sha256":"a".repeat(64)},"budget":b})).unwrap();
    params.validate().unwrap();
    assert_eq!(params.action(), "copy_assembly_component");
    assert_eq!(params.parameter_schema(), component_copy::PARAMS_SCHEMA);
    let mut bad = i.clone();
    bad.source_occurrence_path = vec!["SUB".into(), "PIN_A".into()];
    assert!(component_copy::validate(&bad, &b).is_err());
    bad = i.clone();
    bad.new_instance_name = "pin_b".into();
    assert!(component_copy::check_change(&bad, &b, &before, &call, &after).is_err());
    bad = i;
    bad.source.files[0].sha256 = "invalid".into();
    assert!(component_copy::validate(&bad, &b).is_err());
}
struct Api {
    before: NxAssemblyObservation,
    after: NxAssemblyObservation,
    call: NxComponentCopyOutcome1,
    reads: usize,
    saves: usize,
    copies: usize,
    mass_override: Option<NxGeometricalMassObservation1>,
}
impl NativeApi for Api {
    fn geometrical_mass_properties(
        &mut self,
        body: u64,
        owning_part: u64,
    ) -> Result<NxGeometricalMassObservation1> {
        if let Some(value) = &self.mass_override {
            return Ok(value.clone());
        }
        let state = if self.copies == 0 {
            &self.before
        } else {
            &self.after
        };
        let proto = state
            .prototypes
            .iter()
            .find(|p| p.native_part_tag == owning_part)
            .unwrap();
        let solid = proto.solids.iter().find(|s| s.body_tag == body).unwrap();
        let mut values = [0.; 47];
        values[1] = solid.volume_m3.unwrap();
        let mut accuracy_values = vec![0.; 11];
        accuracy_values[0] = 0.999999;
        Ok(NxGeometricalMassObservation1 {
            schema: geometry::mass_probe::OBSERVATION_SCHEMA.into(),
            body_tag: body,
            owning_part_tag: owning_part,
            analysis_type: 1,
            units: 4,
            density: 0.,
            accuracy_mode: 1,
            accuracy_values,
            return_code: self.call.exception_code,
            raw47_binary64: geometry::mass_probe::words(&values),
            statistics13_binary64: geometry::mass_probe::words(&[0.; 13]),
        })
    }
    fn copy_assembly_component(
        &mut self,
        parent: u32,
        source: &NxAssemblyOccurrenceObservation,
        name: &str,
    ) -> Result<NxComponentCopyOutcome1> {
        assert_eq!(parent, 1);
        assert_eq!(source.name, "PIN_A");
        assert_eq!(name, "PIN_C");
        self.copies += 1;
        Ok(self.call.clone())
    }
    fn persist_load_diagnostics(&mut self, _: &mut dyn FnMut(Value) -> Result<()>) -> Result<()> {
        Ok(())
    }
    fn prepare_owned_assembly_context(&mut self, _: u32, _: &Path) -> Result<()> {
        Ok(())
    }
    fn loaded_assembly_files(&mut self, _: &Path) -> Result<Vec<String>> {
        Ok(vec![
            "root.prt".into(),
            "pin.prt".into(),
            "plate.prt".into(),
        ])
    }
    fn full_assembly_info(
        &mut self,
        _: u32,
        _: &Path,
        _: &NxAssemblyBudget,
        _: &str,
    ) -> Result<NxAssemblyObservation> {
        let v = if self.reads == 0 {
            self.before.clone()
        } else {
            self.after.clone()
        };
        self.reads += 1;
        Ok(v)
    }
    fn assembly_reference_sets(
        &mut self,
        v: &NxAssemblyObservation,
    ) -> Result<NxAssemblyReferenceSets2> {
        Ok(reference_sets(v))
    }
    fn save_all_owned_assembly(&mut self, _: &Path) -> Result<Value> {
        self.saves += 1;
        Ok(json!({"api":"UF_PART_save_all","return_code":0,"file_error_count":0,"file_errors":[]}))
    }
    fn ensure_empty_session(&mut self) -> Result<()> {
        Ok(())
    }
    fn open_part(&mut self, _: &Path) -> Result<u32> {
        Ok(1)
    }
    fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
        bail!("unexpected create")
    }
    fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
        bail!("unexpected expression")
    }
    fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
        bail!("unexpected block")
    }
    fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
        bail!("unexpected edit")
    }
    fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
        bail!("unexpected representation")
    }
    fn save_work_part(&mut self) -> Result<()> {
        panic!("Copy must use full owned assembly save")
    }
    fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
        panic!("Copy must never fall back to AddComponent")
    }
    fn part_info(&mut self, _: u32) -> Result<PartInfo> {
        bail!("unexpected legacy info")
    }
    fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
        bail!("unexpected export")
    }
    fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
        bail!("unexpected import")
    }
    fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
        bail!("unexpected import")
    }
}
#[derive(Default)]
struct Work {
    events: Vec<(String, Value)>,
}
impl Workspace for Work {
    fn prepare_motion_source(&mut self, c: &NxAssemblySourceClosure) -> Result<(PathBuf, PathBuf)> {
        assert_eq!(c.files.len(), 3);
        Ok((
            PathBuf::from("C:/owned/root.prt"),
            PathBuf::from("C:/owned"),
        ))
    }
    fn verify_motion_source(&self) -> Result<()> {
        Ok(())
    }
    fn record_bound(&mut self, phase: &str, value: Value) -> Result<ArtifactReference> {
        let sha = oe_core::sha256(serde_json::to_vec(&value)?);
        self.record(phase, value)?;
        Ok(ArtifactReference {
            path: format!("receipts/{:04}-{phase}.json", self.events.len()),
            sha256: sha,
        })
    }
    fn record(&mut self, phase: &str, value: Value) -> Result<()> {
        self.events.push((phase.into(), value));
        Ok(())
    }
    fn input(&self, path: &RelativePath) -> Result<PathBuf> {
        if path.as_str() == "inputs/root.prt" {
            Ok(PathBuf::from("C:/owned/root.prt"))
        } else {
            bail!("unexpected input")
        }
    }
    fn output(&self, _: &str) -> Result<PathBuf> {
        bail!("unexpected output")
    }
    fn require_regular_output(&self, _: &str) -> Result<Vec<u8>> {
        bail!("unexpected output")
    }
    fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
        bail!("unexpected output")
    }
}
#[test]
fn copy_execution_persists_call_and_checks_before_and_after_save() {
    let (i, b, before, call, after) = fixture();
    let mut api = Api {
        mass_override: None,
        before,
        after,
        call,
        reads: 0,
        saves: 0,
        copies: 0,
    };
    let mut w = Work::default();
    let r = ArtifactReference {
        path: "copy.json".into(),
        sha256: "a".repeat(64),
    };
    let result = component_copy::execute(&mut api, &mut w, &r, &i, &b).unwrap();
    assert_eq!((api.copies, api.saves, api.reads), (1, 1, 3));
    assert_eq!(result["independent_reopen_checked"], false);
    assert_eq!(result["engineering_adoption"], "not_assessed");
    let names: Vec<_> = w.events.iter().map(|x| x.0.as_str()).collect();
    assert!(
        names
            .iter()
            .position(|x| *x == "component_copy_returned")
            .unwrap()
            < names
                .iter()
                .position(|x| *x == "component_copy_after")
                .unwrap()
    );
    assert!(names.contains(&"component_copy_saved_reference_sets"));
    assert!(!names.contains(&"assembly_mass_probe_sample"));
}

#[test]
fn mass_probe_keeps_exact_copy_failure_and_raw_samples() {
    let (i, b, mut before, call, mut after) = fixture();
    let original = f64::from_bits(0x3ef8f232c4da5913);
    before.prototypes[2].solids[0].volume_m3 = Some(original);
    after.prototypes[2].solids[0].volume_m3 = Some(f64::from_bits(original.to_bits() + 1));
    assert!(
        component_copy::check_change(&i, &b, &before, &call, &after)
            .unwrap_err()
            .to_string()
            .contains("copy_prototype_geometry_or_units_changed")
    );
    let mut api = Api {
        mass_override: None,
        before,
        after,
        call,
        reads: 0,
        copies: 0,
        saves: 0,
    };
    let mut workspace = Work::default();
    let reference = ArtifactReference {
        path: "intent.json".into(),
        sha256: "a".repeat(64),
    };
    let error = component_copy::execute_probed(&mut api, &mut workspace, &reference, &i, &b, 2)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("copy_prototype_geometry_or_units_changed")
    );
    assert_eq!((api.copies, api.saves, api.reads), (1, 0, 2));
    let samples: Vec<_> = workspace
        .events
        .iter()
        .filter(|(name, _)| name == "assembly_mass_probe_sample")
        .map(|(_, v)| v)
        .collect();
    assert_eq!(samples.len(), 8);
    for stage in ["before", "after"] {
        let plate: Vec<_> = samples
            .iter()
            .filter(|v| v["stage"] == stage && v["prototype_id"] == "plate.prt")
            .collect();
        assert_eq!(plate.len(), 2);
        let expected = if stage == "before" {
            "3ef8f232c4da5913"
        } else {
            "3ef8f232c4da5914"
        };
        for p in plate {
            assert_eq!(p["observation"]["raw47_binary64"][1], expected);
            assert_eq!(
                p["observation"]["statistics13_binary64"]
                    .as_array()
                    .unwrap()
                    .len(),
                13
            );
        }
    }
    assert!(
        !workspace
            .events
            .iter()
            .any(|(name, _)| name == "component_copy_save_pending")
    );
}

#[test]
fn mass_probe_refuses_measurement_failure_or_budget_before_copy() {
    let (i, b, before, mut call, after) = fixture();
    call.exception_code = 7;
    let mut api = Api {
        mass_override: None,
        before,
        after,
        call,
        reads: 0,
        copies: 0,
        saves: 0,
    };
    let mut workspace = Work::default();
    let reference = ArtifactReference {
        path: "intent.json".into(),
        sha256: "a".repeat(64),
    };
    assert!(
        component_copy::execute_probed(&mut api, &mut workspace, &reference, &i, &b, 2)
            .unwrap_err()
            .to_string()
            .contains("mass_probe_native_return_failed")
    );
    assert_eq!((api.copies, api.saves), (0, 0));
    let event = &workspace
        .events
        .iter()
        .find(|(n, _)| n == "assembly_mass_probe_sample")
        .unwrap()
        .1;
    assert_eq!(event["observation"]["return_code"], 7);
    assert_eq!(
        event["observation"]["raw47_binary64"]
            .as_array()
            .unwrap()
            .len(),
        47
    );
    let (i, b, mut before, call, after) = fixture();
    let solid = before.prototypes[1].solids[0].clone();
    before.prototypes[1].solids = (0..9)
        .map(|n| {
            let mut s = solid.clone();
            s.body_tag += n;
            s
        })
        .collect();
    let mut api = Api {
        mass_override: None,
        before,
        after,
        call,
        reads: 0,
        copies: 0,
        saves: 0,
    };
    let mut workspace = Work::default();
    assert!(
        component_copy::execute_probed(&mut api, &mut workspace, &reference, &i, &b, 2).is_err()
    );
    assert_eq!((api.copies, api.saves), (0, 0));
}

#[test]
fn unchanged_mass_probe_never_copies_saves_or_claims_geometry_equivalence() {
    let (_, b, before, call, _) = fixture();
    let after = before.clone();
    let mut api = Api {
        mass_override: None,
        before,
        after,
        call,
        reads: 0,
        copies: 0,
        saves: 0,
    };
    let mut workspace = Work::default();
    let result =
        geometry::mass_probe::execute_unchanged(&mut api, &mut workspace, "root.prt", &b, 2)
            .unwrap();
    assert_eq!((api.copies, api.saves, api.reads), (0, 0, 2));
    assert_eq!(result["save_requested"], false);
    assert_eq!(result["mutation_requested"], false);
    assert_eq!(result["geometry_equivalence"], "not_established");
    assert_eq!(
        workspace
            .events
            .iter()
            .filter(|(name, _)| name == "assembly_mass_probe_sample")
            .count(),
        8
    );
}

#[test]
fn mass_probe_retains_foreign_owner_observation_and_refuses_before_copy() {
    let (i, b, before, call, after) = fixture();
    let mut api = Api {
        before,
        after,
        call,
        reads: 0,
        copies: 0,
        saves: 0,
        mass_override: None,
    };
    let mut actual = api.geometrical_mass_properties(200, 2).unwrap();
    actual.owning_part_tag = 999;
    api.mass_override = Some(actual);
    let mut workspace = Work::default();
    let reference = ArtifactReference {
        path: "intent.json".into(),
        sha256: "a".repeat(64),
    };
    let error = component_copy::execute_probed(&mut api, &mut workspace, &reference, &i, &b, 2)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mass_probe_current_body_or_owner_differs")
    );
    assert_eq!((api.copies, api.saves), (0, 0));
    let event = &workspace
        .events
        .iter()
        .find(|(name, _)| name == "assembly_mass_probe_sample")
        .unwrap()
        .1;
    assert_eq!(event["expected_owning_part_tag"], 2);
    assert_eq!(event["observation"]["owning_part_tag"], 999);
}
#[test]
fn partial_or_failed_rename_preserves_after_state_and_never_saves() {
    for partial in [true, false] {
        let (i, b, before, mut call, mut after) = fixture();
        call.complete = false;
        call.wrapper_code = -90002;
        if partial {
            call.returned_count = 0;
            call.returned_component_tags.clear();
            after = before.clone();
        } else {
            call.rename_code = 17;
        }
        let mut api = Api {
            mass_override: None,
            before,
            after,
            call,
            reads: 0,
            saves: 0,
            copies: 0,
        };
        let mut w = Work::default();
        assert!(
            component_copy::execute(
                &mut api,
                &mut w,
                &ArtifactReference {
                    path: "copy.json".into(),
                    sha256: "a".repeat(64)
                },
                &i,
                &b
            )
            .is_err()
        );
        assert_eq!((api.copies, api.saves), (1, 0));
        assert!(w.events.iter().any(|x| x.0 == "component_copy_returned"));
        assert!(w.events.iter().any(|x| x.0 == "component_copy_after"));
        assert!(!w.events.iter().any(|x| x.0 == "component_copy_saved"));
    }
}
