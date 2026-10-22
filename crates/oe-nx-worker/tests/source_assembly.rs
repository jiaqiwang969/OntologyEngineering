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
use oe_contracts::{NxAssemblyBuildIntent, NxAssemblyPoseTarget};
use oe_nx_worker::geometry::{self, Parameters, source_assembly as build};
use serde_json::json;
fn input() -> (
    NxAssemblyBuildIntent,
    NxAssemblyObservation,
    NxAssemblyBudget,
) {
    let (_, mut g, b) = synthetic_graph();
    g.source_role = "reference_observation".into();
    let mut other = g.occurrences[0].clone();
    other.occurrence_id = "root/000001".into();
    other.name = "B".into();
    other.part_occurrence_tag = 11;
    other.instance_tag = 21;
    g.occurrences.push(other);
    let placements = g
        .occurrences
        .iter()
        .map(|o| NxAssemblyPoseTarget {
            occurrence_path: vec![o.name.clone()],
            prototype_id: o.prototype_id.clone(),
            local_to_root_m: o.local_to_root_m.clone(),
        })
        .collect::<Vec<_>>();
    let i=serde_json::from_value(json!({"schema":"oev2.nx-assembly-build-intent/1","object":{"object_id":"source-assembly","revision":"r1","condition_scope":"software-fixture","source_sha256":"a".repeat(64)},"files":[{"path":"SUB.prt","bytes":3,"sha256":oe_core::sha256(b"prt")}],"max_total_bytes":1000,"root_part":"ROOT.prt","root_native_units":"mm","placements":placements,"source_refs":["frozen-source"]})).unwrap();
    (i, g, b)
}
#[test]
fn real_source_parameters_and_inventory_preserve_shared_prototypes() {
    let (i, g, b) = input();
    build::validate(&i, &b).unwrap();
    build::check_inventory(&i, &b, &g).unwrap();
    let p:Parameters=serde_json::from_value(json!({"action":"create_source_assembly","schema":build::PARAMS_SCHEMA,"intent":{"path":"build.json","sha256":"b".repeat(64)},"budget":b})).unwrap();
    p.validate().unwrap();
    assert_eq!(p.action(), "create_source_assembly");
    assert_eq!(p.parameter_schema(), build::PARAMS_SCHEMA);
    let mut x = g.clone();
    x.occurrences.pop();
    assert!(
        build::check_inventory(&i, &b, &x)
            .unwrap_err()
            .to_string()
            .contains("count_differs")
    );
    let mut x = g.clone();
    x.occurrences[1].name = "A".into();
    assert!(build::check_inventory(&i, &b, &x).is_err());
    let mut x = i.clone();
    x.placements[1].local_to_root_m[3] += 0.1;
    assert!(
        build::check_inventory(&x, &b, &g)
            .unwrap_err()
            .to_string()
            .contains("pose_or_prototype")
    );
}
#[test]
fn real_source_rejects_root_alias_missing_prototype_and_invalid_pose() {
    let (i, _, b) = input();
    for path in ["ROOT.prt", "root.PRT", "ROOT.prt/child.prt"] {
        let mut x = i.clone();
        x.files[0].path = path.into();
        assert!(
            build::validate(&x, &b)
                .unwrap_err()
                .to_string()
                .contains("conflicts")
        );
    }
    let mut x = i.clone();
    x.placements[0].prototype_id = "undeclared.prt".into();
    assert!(
        build::validate(&x, &b)
            .unwrap_err()
            .to_string()
            .contains("not_declared")
    );
    let mut x = i.clone();
    x.placements[0].occurrence_path.push("nested".into());
    assert!(
        build::validate(&x, &b)
            .unwrap_err()
            .to_string()
            .contains("direct_instance")
    );
    let mut x = i.clone();
    x.placements[0].local_to_root_m[0] = 2.;
    assert!(build::validate(&x, &b).is_err());
    let mut x = b.clone();
    x.max_prototypes = 1;
    assert!(
        build::validate(&i, &x)
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
    let mut x = i.clone();
    x.root_native_units = "metre".into();
    assert!(build::validate(&x, &b).is_err());
}
use anyhow::{Result, bail, ensure};
use geometry::{NativeApi, PartInfo, Workspace};
use oe_contracts::ArtifactReference;
use serde_json::Value;
use std::path::{Path, PathBuf};
struct Api {
    graph: NxAssemblyObservation,
    added: Vec<String>,
    saves: usize,
    empty: usize,
    fault: u8,
    root_saved: bool,
    pending: Vec<Value>,
    used: usize,
}
impl NativeApi for Api {
    fn persist_load_diagnostics(
        &mut self,
        record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        oe_nx_worker::load_diagnostics::persist_batch(&mut self.pending, &mut self.used, record)
    }

    fn ensure_empty_session(&mut self) -> Result<()> {
        self.empty += 1;
        Ok(())
    }
    fn create_assembly_units_part(&mut self, _: &Path, unit: &str) -> Result<u32> {
        ensure!(unit == "mm", "unit");
        Ok(1)
    }
    fn open_part(&mut self, p: &Path) -> Result<u32> {
        ensure!(p.file_name().unwrap() == "SUB.prt", "prototype");
        Ok(2)
    }
    fn loaded_assembly_files(&mut self, _: &Path) -> Result<Vec<String>> {
        ensure!(
            self.root_saved,
            "new_root_not_materialized_before_namespace_read"
        );
        Ok(if self.fault == 1 {
            vec!["ROOT.prt".into(), "foreign.prt".into()]
        } else {
            vec!["ROOT.prt".into(), "SUB.prt".into()]
        })
    }
    fn prepare_owned_assembly_context(&mut self, t: u32, _: &Path) -> Result<()> {
        ensure!(t == 1, "root");
        Ok(())
    }
    fn add_assembly_entire_instance_nxopen(
        &mut self,
        parent: u32,
        child: u32,
        _: &Path,
        name: &str,
        origin: [f64; 3],
        r: &[f64],
    ) -> Result<()> {
        ensure!(parent == 1 && child == 2, "tags");
        assert_eq!(origin, [100., 200., 300.]);
        assert_eq!(r, &[0., -1., 0., 1., 0., 0., 0., 0., 1.]);
        self.added.push(name.into());
        let v = json!({"operation":"add","name":name,"raw_detail":"x".repeat(1024),"return_code":if self.fault==5{19}else{0}});
        self.used += serde_json::to_vec(&v)?.len();
        self.pending.push(v);
        ensure!(self.used <= 512 * 1024, "mock_native_original_queue_guard");
        if self.fault == 5 {
            bail!("native_add_failed");
        }
        Ok(())
    }
    fn full_assembly_info(
        &mut self,
        root: u32,
        _: &Path,
        _: &NxAssemblyBudget,
        role: &str,
    ) -> Result<NxAssemblyObservation> {
        assert_eq!(role, "reference_observation");
        let mut g = self.graph.clone();
        if root == 2 {
            g.root_prototype_id = "SUB.prt".into();
            g.prototypes.remove(0);
            g.occurrences.clear();
            if self.fault == 2 {
                g.occurrences = self.graph.occurrences.clone();
            }
        } else {
            assert_eq!(
                self.added,
                self.graph
                    .occurrences
                    .iter()
                    .map(|o| o.name.clone())
                    .collect::<Vec<_>>()
            );
            if self.fault == 3 {
                g.occurrences.pop();
            }
            if self.fault == 4 && self.saves > 0 {
                g.prototypes[1].sheet_count = 1;
            }
        }
        Ok(g)
    }
    fn save_all_owned_assembly(&mut self, _: &Path) -> Result<Value> {
        self.saves += 1;
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
    fn set_work_part(&mut self, t: u32) -> Result<()> {
        ensure!(t == 1, "work_root");
        Ok(())
    }
    fn save_work_part(&mut self) -> Result<()> {
        self.root_saved = true;
        Ok(())
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
    fail_diagnostics: bool,
}
impl Workspace for Work {
    fn input(&self, p: &oe_nx_protocol::RelativePath) -> Result<PathBuf> {
        Ok(self.dir.path().join(p.as_str()))
    }
    fn output(&self, _: &str) -> Result<PathBuf> {
        bail!("unused")
    }
    fn require_regular_output(&self, _: &str) -> Result<Vec<u8>> {
        bail!("unused")
    }
    fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
        bail!("unused")
    }
    fn record(&mut self, phase: &str, v: Value) -> Result<()> {
        ensure!(
            !(self.fail_diagnostics && phase == "native_load_status"),
            "receipt_disk_full"
        );
        self.records.push(json!({"phase":phase,"observation":v}));
        Ok(())
    }
    fn record_bound(&mut self, phase: &str, v: Value) -> Result<ArtifactReference> {
        let name = format!("{phase}.json");
        let raw = serde_json::to_vec(&v)?;
        let file = self.dir.path().join(&name);
        ensure!(!file.exists(), "receipt_exists");
        std::fs::write(file, &raw)?;
        Ok(ArtifactReference {
            path: name,
            sha256: oe_core::sha256(&raw),
        })
    }
    fn prepare_assembly_build(&mut self, i: &NxAssemblyBuildIntent) -> Result<(PathBuf, PathBuf)> {
        let d = self.dir.path().join("owned");
        std::fs::create_dir(&d)?;
        for f in &i.files {
            let raw = std::fs::read(self.dir.path().join("inputs").join(&f.path))?;
            oe_nx_worker::motion_source::verify_source_file(f, &raw)?;
            std::fs::write(d.join(&f.path), raw)?;
        }
        self.prepared = true;
        Ok((d.join(&i.root_part), d))
    }
    fn verify_motion_source(&self) -> Result<()> {
        ensure!(
            self.prepared && std::fs::read(self.dir.path().join("inputs/SUB.prt"))? == b"prt",
            "source"
        );
        Ok(())
    }
}
fn task(fault: u8) -> (Work, Api, Parameters) {
    let (i, g, b) = input();
    let w = Work {
        dir: tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap(),
        prepared: false,
        records: vec![],
        fail_diagnostics: false,
    };
    std::fs::create_dir(w.dir.path().join("inputs")).unwrap();
    std::fs::write(w.dir.path().join("inputs/SUB.prt"), b"prt").unwrap();
    let raw = serde_json::to_vec(&i).unwrap();
    std::fs::write(w.dir.path().join("inputs/build.json"), &raw).unwrap();
    let p = Parameters::CreateSourceAssembly {
        schema: build::PARAMS_SCHEMA.into(),
        intent: ArtifactReference {
            path: "build.json".into(),
            sha256: oe_core::sha256(&raw),
        },
        budget: b,
    };
    (
        w,
        Api {
            graph: g,
            added: vec![],
            saves: 0,
            empty: 0,
            fault,
            root_saved: false,
            pending: vec![],
            used: 0,
        },
        p,
    )
}
#[test]
fn source_action_copies_adds_checks_and_saves_without_claiming_reopen() {
    let (mut w, mut a, p) = task(0);
    let r = geometry::execute(&mut a, &mut w, &p).unwrap();
    assert_eq!(a.added, vec!["A", "B"]);
    assert_eq!(a.saves, 1);
    for key in ["before_save_record", "after_save_record"] {
        let raw = std::fs::read(w.dir.path().join(r[key]["path"].as_str().unwrap())).unwrap();
        assert_eq!(oe_core::sha256(&raw), r[key]["sha256"].as_str().unwrap());
        assert_eq!(
            serde_json::from_slice::<Value>(&raw).unwrap()["occurrences"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    assert!(r.get("before_save").is_none());
    assert_eq!(r["independent_reopen_checked"], false);
    assert_eq!(r["engineering_adoption"], "not_assessed");
}
#[test]
fn source_action_rejects_changed_source_uncovered_geometry_and_failed_save() {
    let (mut w, mut a, p) = task(0);
    std::fs::write(w.dir.path().join("inputs/build.json"), b"{}").unwrap();
    assert!(geometry::execute(&mut a, &mut w, &p).is_err());
    assert_eq!(a.empty, 0);
    let (mut w, mut a, p) = task(0);
    std::fs::write(w.dir.path().join("inputs/SUB.prt"), b"bad").unwrap();
    assert!(geometry::execute(&mut a, &mut w, &p).is_err());
    assert_eq!(a.saves, 0);
    assert!(a.added.is_empty());
    for fault in 1..=4 {
        let (mut w, mut a, p) = task(fault);
        let e = geometry::execute(&mut a, &mut w, &p).unwrap_err();
        assert_eq!(a.saves, if fault == 4 { 1 } else { 0 }, "{fault}: {e}");
    }
}

#[test]
fn failed_native_add_persists_complete_diagnostics_before_returning_error() {
    let (mut w, mut a, p) = task(5);
    let e = geometry::execute(&mut a, &mut w, &p).unwrap_err();
    assert_eq!(e.to_string(), "native_add_failed");
    assert_eq!(a.saves, 0);
    assert!(a.pending.is_empty());
    assert_eq!(a.used, 0);
    let records = w
        .records
        .iter()
        .filter(|r| r["phase"] == "native_load_status")
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0]["observation"]["observations"][0]["return_code"],
        19
    );
}

#[test]
fn full_instance_action_streams_over_old_total_and_keeps_bound_observations() {
    let (mut w, mut a, mut p) = task(0);
    let (mut intent, _, _) = input();
    let template = a.graph.occurrences[0].clone();
    a.graph.occurrences.clear();
    intent.placements.clear();
    for n in 0..1520 {
        let mut o = template.clone();
        o.name = format!("BOLT_{n:04}");
        o.occurrence_id = format!("root/{n:06}");
        o.part_occurrence_tag = 100 + n;
        o.instance_tag = 5000 + n;
        intent.placements.push(NxAssemblyPoseTarget {
            occurrence_path: vec![o.name.clone()],
            prototype_id: o.prototype_id.clone(),
            local_to_root_m: o.local_to_root_m.clone(),
        });
        a.graph.occurrences.push(o);
    }
    let raw = serde_json::to_vec(&intent).unwrap();
    std::fs::write(w.dir.path().join("inputs/build.json"), &raw).unwrap();
    if let Parameters::CreateSourceAssembly { intent, budget, .. } = &mut p {
        intent.sha256 = oe_core::sha256(&raw);
        budget.max_occurrences = 1520;
    }
    let r = geometry::execute(&mut a, &mut w, &p).unwrap();
    assert_eq!(r["occurrences"], 1520);
    assert_eq!(a.added.len(), 1520);
    assert!(a.pending.is_empty());
    assert_eq!(a.used, 0);
    let records = w
        .records
        .iter()
        .filter(|r| r["phase"] == "native_load_status")
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1520);
    assert!(serde_json::to_vec(&records).unwrap().len() > 512 * 1024);
    for (n, r) in records.iter().enumerate() {
        assert_eq!(
            r["observation"]["observations"][0]["name"],
            format!("BOLT_{n:04}")
        );
    }
    assert!(serde_json::to_vec(&r).unwrap().len() < 4096);
}
#[test]
fn action_writer_failure_retains_unacknowledged_diagnostics_and_stops() {
    let (mut w, mut a, p) = task(0);
    w.fail_diagnostics = true;
    let e = geometry::execute(&mut a, &mut w, &p).unwrap_err();
    assert_eq!(e.to_string(), "receipt_disk_full");
    assert_eq!(a.added, vec!["A"]);
    assert_eq!(a.saves, 0);
    assert_eq!(a.pending.len(), 1);
    assert!(a.used > 0);
}
