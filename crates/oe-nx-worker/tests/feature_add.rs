//! Independent frozen state-transition controls; no native or engineering success inferred.
use anyhow::{Result, bail};
use oe_contracts::*;
use oe_nx_protocol::RelativePath;
use oe_nx_worker::geometry::{self, NativeApi, PartInfo, Workspace, feature_add as add};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
#[path = "support/hole_plate.rs"]
mod fixture;
use fixture::{assembly, budget, detail, reference_sets};
fn intent() -> NxFeatureAddIntent1 {
    let mut v = serde_json::to_value(fixture::intent()).unwrap();
    v["schema"] = json!(add::INTENT_SCHEMA);
    v["tool_end_z_mm"] = json!(9.);
    serde_json::from_value(v).unwrap()
}
fn before_inventory() -> NxFeatureInventory1 {
    let mut v = fixture::inventory();
    v.features.remove(1);
    v
}
fn after_inventory() -> NxFeatureInventory1 {
    let mut v = fixture::inventory();
    v.features[1].feature_tag = 1501;
    v
}
fn outcome() -> NxFeatureAddOutcome1 {
    serde_json::from_value(json!({"schema":add::OUTCOME_SCHEMA,"part_tag":3,"body_tag":300,"feature_tag":1501,
      "origin_mm":[-20.,0.,-1.],"height_mm":10.,"diameter_mm":12.,"call_entered":true,"call_returned":true,
      "return_code":0,"update_code":0,"complete":true})).unwrap()
}
#[test]
fn feature_add_preserves_profile_and_rejects_wrong_changes() {
    let i = intent();
    add::validate(&i, &budget()).unwrap();
    add::check_geometry(&i, &detail(true), false).unwrap();
    add::check_geometry(&i, &detail(false), true).unwrap();
    add::check_assembly(&i, &budget(), &assembly(true), &assembly(false)).unwrap();
    assert!(add::check_geometry(&i, &detail(false), false).is_err());
    assert!(add::check_assembly(&i, &budget(), &assembly(true), &assembly(true)).is_err());
    for n in 0..5 {
        let mut v = detail(false);
        match n {
            0 => v["solid_face_inventory_complete"] = json!(false),
            1 => v["solid_face_inventory"][7]["radius"] = json!(7.),
            2 => v["solid_face_inventory"][0]["axis_point"][0] = json!(-39.),
            3 => v["model_edges"]["complete"] = json!(false),
            _ => v["units"] = json!("inch"),
        }
        assert!(
            add::check_geometry(&i, &v, true).is_err(),
            "geometry fault {n}"
        );
    }
    for n in 0..6 {
        let mut v = i.clone();
        match n {
            0 => v.object.source_sha256 = "f".repeat(64),
            1 => v.tool_end_z_mm = 8.,
            2 => v.feature_location_mm[2] = 0.,
            3 => v.feature_location_mm = vec![],
            4 => v.target.center[0] = 20.,
            _ => v.tool_end_z_mm = f64::INFINITY,
        }
        assert!(add::validate(&v, &budget()).is_err(), "intent fault {n}");
    }
    for n in 0..4 {
        let mut v = assembly(false);
        match n {
            0 => v.prototypes[1].solids[0].volume_m3 = Some(0.000002),
            1 => v.prototypes[2].solids[0].body_tag = 301,
            2 => v.occurrences[0].name = "moved".into(),
            _ => {
                v.occurrences.pop();
            }
        }
        assert!(
            add::check_assembly(&i, &budget(), &assembly(true), &v).is_err(),
            "assembly fault {n}"
        );
    }
}
#[test]
fn feature_add_requires_new_feature_and_keeps_previous_inventory() {
    let i = intent();
    let before = before_inventory();
    let after = after_inventory();
    let call = outcome();
    let verify = |a: &NxFeatureInventory1, c: &NxFeatureAddOutcome1| {
        add::check_lineage(&i, &before, &detail(true), a, &detail(false), c)
    };
    let good = verify(&after, &call).unwrap();
    assert_eq!(good["feature_tag"], 1501);
    for n in 0..9 {
        let mut a = after.clone();
        let mut c = call.clone();
        match n {
            0 => c.feature_tag = 500,
            1 => a.features[1].owning_part_tag = 2,
            2 => a.features[1].body_tag = 999,
            3 => a.features[1].feature_type = "HOLE".into(),
            4 => {
                a.features.remove(0);
            }
            5 => {
                let mut extra = a.features[1].clone();
                extra.feature_tag = 1502;
                a.features.push(extra);
            }
            6 => a.features[0].location_mm[0] = 1.,
            7 => a.features[1].query_codes[2] = 17,
            _ => a.complete = false,
        }
        assert!(verify(&a, &c).is_err(), "lineage fault {n}");
    }
    for n in 0..5 {
        let mut c = call.clone();
        match n {
            0 => c.origin_mm[0] = 20.,
            1 => c.diameter_mm = 10.,
            2 => c.height_mm = 9.,
            3 => c.return_code = 17,
            _ => c.update_code = Some(17),
        }
        assert!(verify(&after, &c).is_err(), "raw call fault {n}");
    }
}
struct Api {
    calls: usize,
    saves: usize,
    work_part: u32,
    fail_selection_after_add: bool,
    fail_selection_after_save: bool,
    call: NxFeatureAddOutcome1,
    before_inventory: NxFeatureInventory1,
    after_inventory: NxFeatureInventory1,
    after_detail: Value,
    after_assembly: NxAssemblyObservation,
    save_failure: bool,
    native_error: bool,
}
impl Default for Api {
    fn default() -> Self {
        Self {
            calls: 0,
            saves: 0,
            work_part: 0,
            fail_selection_after_add: false,
            fail_selection_after_save: false,
            call: outcome(),
            before_inventory: before_inventory(),
            after_inventory: after_inventory(),
            after_detail: detail(false),
            after_assembly: assembly(false),
            save_failure: false,
            native_error: false,
        }
    }
}
impl NativeApi for Api {
    fn feature_inventory(&mut self, part: u64, body: u64) -> Result<NxFeatureInventory1> {
        assert_eq!((part, body), (3, 300));
        // Match the native feature owner: enumeration requires the current
        // prototype, while observe_assembly finishes with the root selected.
        anyhow::ensure!(
            self.work_part as u64 == part,
            "feature_delete_current_work_part_differs"
        );
        Ok(if self.calls == 0 {
            self.before_inventory.clone()
        } else {
            self.after_inventory.clone()
        })
    }
    fn add_hole_feature(
        &mut self,
        part: u64,
        body: u64,
        origin: [f64; 3],
        height: f64,
        diameter: f64,
    ) -> Result<NxFeatureAddOutcome1> {
        assert_eq!(
            (part, body, origin, height, diameter),
            (3, 300, [-20., 0., -1.], 10., 12.)
        );
        self.calls += 1;
        if self.native_error {
            bail!("frozen native failure");
        }
        Ok(self.call.clone())
    }
    fn detailed_info(&mut self, part: u32, _: bool) -> Result<Value> {
        assert_eq!(part, 3);
        Ok(if self.calls == 0 {
            detail(true)
        } else {
            self.after_detail.clone()
        })
    }
    fn set_work_part(&mut self, part: u32) -> Result<()> {
        assert_eq!(part, 3);
        if (self.fail_selection_after_add && self.calls > 0)
            || (self.fail_selection_after_save && self.saves > 0)
        {
            bail!("frozen work-part selection failure");
        }
        self.work_part = part;
        Ok(())
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
        root: u32,
        _: &Path,
        _: &NxAssemblyBudget,
        _: &str,
    ) -> Result<NxAssemblyObservation> {
        self.work_part = root;
        Ok(if self.calls == 0 {
            assembly(true)
        } else {
            self.after_assembly.clone()
        })
    }
    fn assembly_reference_sets(
        &mut self,
        a: &NxAssemblyObservation,
    ) -> Result<NxAssemblyReferenceSets2> {
        Ok(reference_sets(a))
    }
    fn save_all_owned_assembly(&mut self, _: &Path) -> Result<Value> {
        self.saves += 1;
        Ok(json!({"api":"UF_PART_save_all",
        "return_code":if self.save_failure{17}else{0},"file_error_count":0,"file_errors":[]}))
    }
    fn ensure_empty_session(&mut self) -> Result<()> {
        Ok(())
    }
    fn open_part(&mut self, _: &Path) -> Result<u32> {
        Ok(1)
    }
    fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
        bail!("must not rebuild plate")
    }
    fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
        bail!("must not rebuild")
    }
    fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
        bail!("must not rebuild")
    }
    fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
        bail!("unexpected expression edit")
    }
    fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
        bail!("unexpected representation")
    }
    fn save_work_part(&mut self) -> Result<()> {
        panic!("must save owned closure only")
    }
    fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
        panic!("must not rebuild assembly")
    }
    fn part_info(&mut self, _: u32) -> Result<PartInfo> {
        bail!("detailed observation required")
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
    fn prepare_motion_source(&mut self, _: &NxAssemblySourceClosure) -> Result<(PathBuf, PathBuf)> {
        Ok(("C:/owned/root.prt".into(), "C:/owned".into()))
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
    fn input(&self, _: &RelativePath) -> Result<PathBuf> {
        bail!("unexpected input")
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

fn run(api: &mut Api, w: &mut Work) -> Result<Value> {
    add::execute(
        api,
        w,
        &ArtifactReference {
            path: "add.json".into(),
            sha256: "f".repeat(64),
        },
        &intent(),
        &budget(),
    )
}
#[test]
fn feature_add_failure_never_saves_or_retries() {
    for fault in 0..8 {
        let mut api = Api::default();
        let mut w = Work::default();
        match fault {
            0 => api.before_inventory.features[0].owning_part_tag = 2,
            1 => api.call.return_code = 17,
            2 => api.call.update_code = Some(9),
            3 => api.after_detail["solid_face_inventory_complete"] = json!(false),
            4 => api.after_assembly.prototypes[1].solids[0].volume_m3 = Some(0.000002),
            5 => api.native_error = true,
            6 => api.after_inventory.features[1].feature_tag = 500,
            _ => api.after_inventory.complete = false,
        }
        assert!(run(&mut api, &mut w).is_err(), "failure {fault}");
        assert_eq!(api.saves, 0);
        assert_eq!(api.calls, usize::from(fault != 0));
        if fault == 1 || fault == 2 {
            assert!(w.events.iter().any(|(p, _)| p == "feature_add_returned"));
        }
    }
    let mut api = Api {
        save_failure: true,
        ..Api::default()
    };
    let mut w = Work::default();
    assert!(run(&mut api, &mut w).is_err());
    assert_eq!((api.calls, api.saves), (1, 1));
    assert!(
        !w.events
            .iter()
            .any(|(p, _)| p == "feature_add_saved_inventory")
    );
}
#[test]
fn feature_add_context_selection_failure_retains_partial_state_without_retry() {
    for after_save in [false, true] {
        let mut api = Api {
            fail_selection_after_add: !after_save,
            fail_selection_after_save: after_save,
            ..Api::default()
        };
        let mut w = Work::default();
        let err = run(&mut api, &mut w).unwrap_err();
        assert!(
            err.to_string()
                .contains("frozen work-part selection failure")
        );
        assert_eq!(api.calls, 1);
        assert_eq!(api.saves, usize::from(after_save));
        assert!(w.events.iter().any(|(p, _)| p == "feature_add_returned"));
        assert!(
            !w.events
                .iter()
                .any(|(p, _)| p == "feature_add_saved_features")
        );
        assert_eq!(
            w.events
                .iter()
                .any(|(p, _)| p == "feature_add_save_returned"),
            after_save
        );
    }
}
#[test]
fn feature_add_success_keeps_raw_order_and_requires_independent_reopen() {
    let mut api = Api::default();
    let mut w = Work::default();
    let r = run(&mut api, &mut w).unwrap();
    assert_eq!((api.calls, api.saves), (1, 1));
    assert_eq!(r["independent_reopen_checked"], false);
    assert_eq!(r["engineering_adoption"], "not_assessed");
    let phases = [
        "before",
        "before_reference_sets",
        "before_detail",
        "before_features",
        "pending",
        "returned",
        "after_detail",
        "after",
        "after_features",
        "lineage",
        "after_reference_sets",
        "save_pending",
        "save_returned",
        "saved_detail",
        "saved_features",
        "saved_inventory",
        "saved_reference_sets",
    ];
    assert_eq!(
        w.events
            .iter()
            .map(|(p, _)| p.strip_prefix("feature_add_").unwrap())
            .collect::<Vec<_>>(),
        phases
    );
}
#[test]
fn feature_add_explicit_parameter_version_does_not_change_legacy_delete() {
    let v = json!({"action":"add_hole_feature","schema":add::PARAMS_SCHEMA,
      "intent":{"path":"add.json","sha256":"f".repeat(64)},"budget":budget()});
    let p: geometry::Parameters = serde_json::from_value(v.clone()).unwrap();
    p.validate().unwrap();
    assert_eq!(p.action(), add::ACTION);
    assert_eq!(p.parameter_schema(), add::PARAMS_SCHEMA);
    let mut wrong = v;
    wrong["schema"] = json!("oev2.nx-feature-delete-params/1");
    assert!(
        serde_json::from_value::<geometry::Parameters>(wrong)
            .unwrap()
            .validate()
            .is_err()
    );
}
