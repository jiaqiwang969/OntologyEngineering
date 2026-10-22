use anyhow::{Result, bail};
use oe_contracts::*;
use oe_nx_protocol::RelativePath;
use oe_nx_worker::{
    assembly::Rigid,
    geometry::{self, motion_task, *},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
fn graph() -> NxAssemblyPositionObservation {
    let solid = |tag, owner| NxAssemblySolidObservation {
        body_tag: tag,
        owning_part_tag: owner,
        is_occurrence: false,
        exact_box_corners_m: vec![0.; 24],
        exact_box_corners_native: vec![0.; 24],
        volume_m3: Some(1e-9),
    };
    let proto = |name: &str, tag| NxAssemblyPrototypeObservation {
        prototype_id: name.into(),
        part_path: format!("C:/fixture/{name}"),
        native_part_tag: tag,
        native_units: "mm".into(),
        unit_to_m: 0.001,
        load_state_raw: 1,
        fully_loaded: true,
        solids: vec![solid(tag + 100, tag)],
        sheet_count: 0,
    };
    NxAssemblyPositionObservation {
        schema: "oev2.nx-native-assembly-position-observation/2".into(),
        root_prototype_id: "ROOT.prt".into(),
        root_occurrence_tag: 99,
        prototypes: vec![proto("ROOT.prt", 1), proto("LEAF.prt", 2)],
        occurrences: vec![NxAssemblyOccurrenceObservation {
            occurrence_id: "root/000000".into(),
            parent_occurrence_id: None,
            prototype_id: "LEAF.prt".into(),
            part_occurrence_tag: 10,
            instance_tag: 20,
            name: "A".into(),
            reference_set: "None".into(),
            raw_parent_origin: vec![0.; 3],
            raw_parent_csys: vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
            raw_parent_transform: Rigid::identity().0.to_vec(),
            raw_root_transform: Rigid::identity().0.to_vec(),
            local_to_parent_m: Rigid::identity().0.to_vec(),
            local_to_root_m: Rigid::identity().0.to_vec(),
        }],
        position_overrides: vec![NxAssemblyPositionOverride {
            occurrence_id: "root/000000".into(),
            part_occurrence_tag: 10,
            raw_type: 0,
            parent_occurrence_tag: 0,
        }],
        complete: true,
        source_role: "independent_software_fixture".into(),
        engineering_adoption: "not_assessed".into(),
    }
}
struct Api {
    value: NxAssemblyPositionObservation,
    moves: usize,
    saves: usize,
    empty: usize,
    fault: u8,
}
impl Api {
    fn new(fault: u8) -> Self {
        Self {
            value: graph(),
            moves: 0,
            saves: 0,
            empty: 0,
            fault,
        }
    }
}
impl NativeApi for Api {
    fn ensure_empty_session(&mut self) -> Result<()> {
        self.empty += 1;
        Ok(())
    }
    fn open_part(&mut self, _: &Path) -> Result<u32> {
        Ok(1)
    }
    fn loaded_assembly_files(&mut self, _: &Path) -> Result<Vec<String>> {
        Ok(if self.fault == 1 {
            vec!["ROOT.prt".into(), "OUTSIDE.prt".into()]
        } else {
            vec!["ROOT.prt".into(), "LEAF.prt".into()]
        })
    }
    fn prepare_owned_assembly_context(&mut self, _: u32, _: &Path) -> Result<()> {
        Ok(())
    }
    fn assembly_positions(
        &mut self,
        _: u32,
        _: &Path,
        _: &NxAssemblyBudget,
        role: &str,
    ) -> Result<NxAssemblyPositionObservation> {
        self.value.source_role = role.into();
        Ok(self.value.clone())
    }
    fn move_assembly_sample(
        &mut self,
        _: u32,
        _: &Path,
        _: &NxAssemblyBudget,
        s: &NxAssemblyMotionSample,
        r: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<NxAssemblyPositionObservation> {
        r(json!({"phase":"motion_sample_before","sample_id":s.sample_id}))?;
        r(
            json!({"phase":"motion_reposition_pending","sample_id":s.sample_id,"occurrence_id":"root/000000"}),
        )?;
        self.moves += 1;
        let o = &mut self.value.occurrences[0];
        o.local_to_parent_m = s.targets[0].local_to_root_m.clone();
        o.local_to_root_m = o.local_to_parent_m.clone();
        o.raw_root_transform = o.local_to_root_m.clone();
        for k in [3, 7, 11] {
            o.raw_root_transform[k] *= 1000.;
        }
        self.value.position_overrides[0].raw_type = 2;
        self.value.position_overrides[0].parent_occurrence_tag = 99;
        if self.fault == 2 {
            self.value.occurrences[0].local_to_root_m[3] += 1.;
        }
        r(
            json!({"phase":"motion_reposition_returned","sample_id":s.sample_id,"occurrence_id":"root/000000","wrapper_code":0,"native_code":0,"update_code":0}),
        )?;
        r(json!({"phase":"motion_sample_after","sample_id":s.sample_id}))?;
        Ok(self.value.clone())
    }
    fn check_assembly_pairs(
        &mut self,
        _: u32,
        _: &Path,
        _: &NxAssemblyBudget,
        _: &NxAssemblyMotionBudget,
        _: u64,
        r: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<Value> {
        r(json!({"phase":"motion_pairs_before","expected_pairs":1}))?;
        let id = json!({"pair_index":0,"a":{"body_tag":101},"b":{"body_tag":102}});
        r(json!({"phase":"interference_pending","identity":id}))?;
        if self.fault != 3 {
            r(
                json!({"phase":"interference_returned","identity":id,"query_code":0,"cleanup_code":0,"result_raw":3}),
            )?;
        }
        r(json!({"phase":"motion_pairs_after_cleanup"}))?;
        Ok(
            json!({"checked_pairs":1,"interference_pairs":0,"touching_pairs":1,"no_interference_pairs":0,"continuous_sweep_checked":false,"engineering_adoption":"not_assessed"}),
        )
    }
    fn save_all_owned_assembly(&mut self, _: &Path) -> Result<Value> {
        self.saves += 1;
        if self.fault == 4 {
            self.value.position_overrides[0].raw_type = 3;
        }
        Ok(json!({"saved":true}))
    }
    fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
        bail!("unused")
    }
    fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
        bail!("unused")
    }
    fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
        bail!("unused")
    }
    fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
        bail!("unused")
    }
    fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
        bail!("unused")
    }
    fn save_work_part(&mut self) -> Result<()> {
        bail!("unused")
    }
    fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
        bail!("unused")
    }
    fn part_info(&mut self, _: u32) -> Result<PartInfo> {
        bail!("unused")
    }
    fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
        bail!("unused")
    }
    fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
        bail!("unused")
    }
    fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
        bail!("unused")
    }
}
struct Work {
    dir: tempfile::TempDir,
    prepared: bool,
    records: Vec<Value>,
}
impl Workspace for Work {
    fn verify_motion_copy_files(&self) -> Result<()> {
        for entry in std::fs::read_dir(self.dir.path().join("out/assembly"))? {
            let entry = entry?;
            anyhow::ensure!(
                std::fs::read(entry.path())?
                    == std::fs::read(self.dir.path().join("inputs").join(entry.file_name()))?,
                "copy_changed"
            );
        }
        Ok(())
    }

    fn motion_job_id(&self) -> Result<String> {
        Ok("motion-test".into())
    }
    fn input(&self, p: &RelativePath) -> Result<PathBuf> {
        Ok(self.dir.path().join(p.as_str()))
    }
    fn output(&self, n: &str) -> Result<PathBuf> {
        let p = self.dir.path().join("out").join(n);
        anyhow::ensure!(!p.exists(), "exists");
        Ok(p)
    }
    fn require_regular_output(&self, n: &str) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.dir.path().join("out").join(n))?)
    }
    fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
        bail!("unused")
    }
    fn record(&mut self, _: &str, v: Value) -> Result<()> {
        self.records.push(v);
        Ok(())
    }
    fn prepare_motion_source(&mut self, c: &NxAssemblySourceClosure) -> Result<(PathBuf, PathBuf)> {
        let d = self.dir.path().join("out/assembly");
        std::fs::create_dir(&d)?;
        for f in &c.files {
            let raw = std::fs::read(self.dir.path().join("inputs").join(&f.path))?;
            oe_nx_worker::motion_source::verify_source_file(f, &raw)?;
            std::fs::write(d.join(&f.path), raw)?;
        }
        self.prepared = true;
        Ok((d.join(&c.root_part), d))
    }
    fn verify_motion_source(&self) -> Result<()> {
        anyhow::ensure!(self.prepared, "not_prepared");
        Ok(())
    }
    fn create_motion_evidence(
        &mut self,
        b: NxMotionEvidenceBudget,
        v: NxMotionEvidenceBinding,
    ) -> Result<Box<dyn oe_nx_worker::motion_evidence::sink::EvidenceSink + '_>> {
        use oe_nx_worker::motion_evidence::*;
        let path = self.dir.path().join("out/motion-evidence");
        std::fs::create_dir(&path)?;
        let dest = path.clone();
        let writer = EvidenceWriter::new(
            move |i| {
                Ok(std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(dest.join(segment_name(i)))?)
            },
            b.clone(),
            v.clone(),
        )?;
        Ok(Box::new(sink::VerifiedSink::new(
            writer,
            move |s: &NxMotionEvidenceSummary| {
                let mut names = std::fs::read_dir(&path)?
                    .map(|e| Ok(e?.file_name().into_string().unwrap()))
                    .collect::<Result<Vec<_>>>()?;
                names.sort();
                receiving::verify_evidence(s, &b, &v, &names, |n, visit| {
                    visit(&mut std::fs::File::open(path.join(n))?)
                })
            },
        )))
    }
}
fn fixture() -> (Work, Parameters) {
    let w = Work {
        dir: tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap(),
        prepared: false,
        records: vec![],
    };
    std::fs::create_dir(w.dir.path().join("inputs")).unwrap();
    std::fs::create_dir(w.dir.path().join("out")).unwrap();
    let mut files = vec![];
    for n in ["ROOT.prt", "LEAF.prt"] {
        std::fs::write(w.dir.path().join("inputs").join(n), n.as_bytes()).unwrap();
        files.push(json!({"path":n,"bytes":n.len(),"sha256":oe_core::sha256(n.as_bytes())}));
    }
    let closure = json!({"schema":"oev2.nx-assembly-source-closure/1","root_part":"ROOT.prt","files":files,"max_total_bytes":1024});
    let raw = serde_json::to_vec(&closure).unwrap();
    std::fs::write(w.dir.path().join("inputs/source.json"), &raw).unwrap();
    let mut matrix = Rigid::identity().0;
    matrix[3] = 0.1;
    let s = |id: &str, time| json!({"sample_id":id,"time_s":time,"targets":[{"occurrence_path":["A"],"prototype_id":"LEAF.prt","local_to_root_m":matrix}]});
    let intent = json!({"schema":"oev2.nx-assembly-motion-intent/1","object":{"object_id":"fixture","revision":"r1","condition_scope":"test","source_sha256":oe_core::sha256(&raw)},"assembly_source":{"path":"source.json","sha256":oe_core::sha256(&raw)},"root_part":"ROOT.prt","samples":[s("first",0.),s("second",1.)],"budget":{"max_samples":2,"max_placed_bodies":2,"max_pair_checks":2},"source_refs":["independent_software_fixture"]});
    let raw = serde_json::to_vec(&intent).unwrap();
    std::fs::write(w.dir.path().join("inputs/motion.json"), &raw).unwrap();
    let p=serde_json::from_value(json!({"action":"run_assembly_motion","schema":motion_task::PARAMS_SCHEMA,"intent":{"path":"motion.json","sha256":oe_core::sha256(&raw)},"budget":{"max_occurrences":10,"max_prototypes":10,"max_depth":10},"evidence":{"max_events":100,"max_bytes":65536,"max_event_bytes":8192,"segment_bytes":16384,"max_segments":4}})).unwrap();
    (w, p)
}
#[test]
fn full_action_preserves_sample_identity_touching_and_owned_saved_result() {
    let (mut w, p) = fixture();
    let mut a = Api::new(0);
    let r = geometry::execute(&mut a, &mut w, &p).unwrap();
    assert_eq!(a.moves, 2);
    assert_eq!(a.saves, 1);
    assert_eq!(r["checked_pairs"], 2);
    assert_eq!(r["touching_pairs"], 2);
    assert_eq!(r["no_interference_pairs"], 0);
    assert_eq!(r["independent_reopen_checked"], false);
    let mut ids = vec![];
    for segment in r["evidence"]["segments"].as_array().unwrap() {
        let raw = std::fs::read(
            w.dir
                .path()
                .join("out/motion-evidence")
                .join(segment["path"].as_str().unwrap()),
        )
        .unwrap();
        for line in raw.split(|b| *b == b'\n').filter(|s| !s.is_empty()) {
            let e: Value = serde_json::from_slice(line).unwrap();
            if e["phase"] == "interference_returned" {
                ids.push(e["observation"]["sample_id"].as_str().unwrap().to_owned());
            }
        }
    }
    assert_eq!(ids, vec!["first", "second"]);
    assert!(w.dir.path().join("out/motion-saved-position.json").exists());
}
#[test]
fn failed_closure_pose_coverage_or_save_cannot_return_success() {
    for fault in 1..=4 {
        let (mut w, p) = fixture();
        let mut a = Api::new(fault);
        let error = geometry::execute(&mut a, &mut w, &p).unwrap_err();
        assert!(w.records.is_empty());
        assert_eq!(
            a.moves,
            match fault {
                1 => 0,
                4 => 2,
                _ => 1,
            },
            "{fault}: {error}"
        );
        assert_eq!(a.saves, if fault == 4 { 1 } else { 0 }, "{fault}: {error}");
        if fault == 1 {
            assert!(error.to_string().contains("undeclared"));
        }
        if fault == 3 {
            assert!(error.to_string().contains("coverage"));
        }
        if fault == 4 {
            assert!(error.to_string().contains("metadata_changed"));
        }
    }
}
#[test]
fn changed_input_and_insufficient_plan_budget_stop_before_motion() {
    let (mut w, p) = fixture();
    std::fs::write(w.dir.path().join("inputs/motion.json"), b"{}").unwrap();
    let mut a = Api::new(0);
    assert!(geometry::execute(&mut a, &mut w, &p).is_err());
    assert_eq!(a.empty, 0);
    let (mut w, mut p) = fixture();
    if let Parameters::RunAssemblyMotion { evidence, .. } = &mut p {
        evidence.max_events = 2;
    }
    let mut a = Api::new(0);
    assert!(geometry::execute(&mut a, &mut w, &p).is_err());
    assert_eq!(a.moves, 0);
    assert_eq!(a.saves, 0);
}

#[test]
fn reopen_action_reads_bound_saved_files_without_motion_or_save() {
    let (mut w, _) = fixture();
    let raw = serde_json::to_vec(&graph()).unwrap();
    std::fs::write(w.dir.path().join("inputs/expected.json"), &raw).unwrap();
    let source = std::fs::read(w.dir.path().join("inputs/source.json")).unwrap();
    let p = Parameters::InspectAssemblyPositions {
        schema: position_reopen::PARAMS_SCHEMA.into(),
        source: ArtifactReference {
            path: "source.json".into(),
            sha256: oe_core::sha256(&source),
        },
        expected: ArtifactReference {
            path: "expected.json".into(),
            sha256: oe_core::sha256(&raw),
        },
        budget: NxAssemblyBudget {
            max_occurrences: 10,
            max_prototypes: 10,
            max_depth: 10,
        },
    };
    let mut a = Api::new(0);
    let r = geometry::execute(&mut a, &mut w, &p).unwrap();
    assert_eq!(a.moves, 0);
    assert_eq!(a.saves, 0);
    assert_eq!(a.empty, 1);
    assert_eq!(r["private_copy_bytes_unchanged"], true);
    assert_eq!(
        r["independent_process"],
        "requires_controller_process_receipts"
    );
    let (mut other, _) = fixture();
    std::fs::write(other.dir.path().join("inputs/expected.json"), b"{}").unwrap();
    let mut a = Api::new(0);
    assert!(geometry::execute(&mut a, &mut other, &p).is_err());
    assert_eq!(a.empty, 0);
}
