//! Frozen software examples; synthetic success is not native NX acceptance.
use anyhow::{Result, bail};
use oe_contracts::*;
use oe_nx_protocol::RelativePath;
use oe_nx_worker::geometry::{self, NativeApi, PartInfo, Workspace, feature_delete as delete};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[path = "support/hole_plate.rs"]
mod fixture;
use fixture::*;

#[test]
fn feature_delete_selection_rejects_missing_ambiguous_partial_wrong_owner() {
    let i = intent();
    let inv = inventory();
    let d = detail(false);
    delete::validate(&i, &budget()).unwrap();
    assert_eq!(delete::select_feature(&i, &inv, &d).unwrap(), 501);
    let mut cases = Vec::new();
    let mut c = inv.clone();
    c.features.remove(1);
    cases.push(c);
    let mut c = inv.clone();
    let mut duplicate = c.features[1].clone();
    duplicate.feature_tag = 503;
    c.features.push(duplicate);
    cases.push(c);
    let mut c = inv.clone();
    c.complete = false;
    cases.push(c);
    let mut c = inv.clone();
    c.features[2].complete = false;
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].owning_part_tag = 2;
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].body_tag = 200;
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].feature_type = "HOLE".into();
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].location_mm[0] = 20.;
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].face_tags = vec![920];
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].query_codes[4] = 17;
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].is_occurrence = true;
    cases.push(c);
    let mut c = inv.clone();
    c.part_tag = 99;
    for f in &mut c.features {
        f.owning_part_tag = 99;
    }
    cases.push(c);
    let mut c = inv.clone();
    c.features[1].face_tags = vec![9999];
    cases.push(c);
    for (n, c) in cases.iter().enumerate() {
        assert!(
            delete::select_feature(&i, c, &d).is_err(),
            "frozen inventory fault {n}"
        );
    }
    let mut stale = d;
    stale["part_tag"] = json!(4);
    assert!(delete::select_feature(&i, &inv, &stale).is_err());
    let mut stale_source = i;
    stale_source.object.source_sha256 = "d".repeat(64);
    assert!(delete::validate(&stale_source, &budget()).is_err());
}
#[test]
fn feature_delete_public_geometry_rejects_malformed_intents_without_panicking() {
    let original = intent();
    let before = detail(false);
    let after = detail(true);
    delete::check_geometry(&original, &before, false).unwrap();
    delete::check_geometry(&original, &after, true).unwrap();
    let mut cases = Vec::new();
    for size in [0, 1, 2, 3, 4, 5, 7, 8] {
        let mut i = original.clone();
        i.plate_bounds_mm.resize(size, 0.);
        cases.push((format!("plate bounds length {size}"), i));
    }
    for size in [0, 1, 2, 4] {
        let mut i = original.clone();
        i.feature_location_mm.resize(size, 0.);
        cases.push((format!("feature location length {size}"), i));
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut i = original.clone();
        i.plate_bounds_mm[0] = value;
        cases.push((format!("nonfinite plate bound {value}"), i));
        let mut i = original.clone();
        i.feature_location_mm[0] = value;
        cases.push((format!("nonfinite feature location {value}"), i));
    }
    // Exercise both selectors independently, including the z coordinate that
    // check_geometry subsequently projects onto each end plane. Malformed
    // source data must be rejected before that projection can hide it.
    for retained in [false, true] {
        for size in [0, 1, 2, 4] {
            for normal in [false, true] {
                let mut i = original.clone();
                let c = if retained {
                    &mut i.retained
                } else {
                    &mut i.target
                };
                if normal {
                    c.normal.resize(size, 0.);
                } else {
                    c.center.resize(size, 0.);
                }
                cases.push((
                    format!("retained={retained} normal={normal} length {size}"),
                    i,
                ));
            }
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for field in 0..6 {
                let mut i = original.clone();
                let c = if retained {
                    &mut i.retained
                } else {
                    &mut i.target
                };
                match field {
                    0 => c.center[2] = value,
                    1 => c.normal[2] = value,
                    2 => c.radius = value,
                    3 => c.center_tolerance = value,
                    4 => c.radius_tolerance = value,
                    _ => c.angular_tolerance_radians = value,
                }
                cases.push((
                    format!("retained={retained} field={field} nonfinite={value}"),
                    i,
                ));
            }
        }
        for field in 0..5 {
            let mut i = original.clone();
            let c = if retained {
                &mut i.retained
            } else {
                &mut i.target
            };
            match field {
                0 => c.normal = vec![0.; 3],
                1 => c.radius = -1.,
                2 => c.center_tolerance = -1.,
                3 => c.radius_tolerance = -1.,
                _ => c.angular_tolerance_radians = -1.,
            }
            cases.push((
                format!("retained={retained} invalid circle field={field}"),
                i,
            ));
        }
    }
    for (name, i) in cases {
        for (deleted, observation) in [(false, &before), (true, &after)] {
            let result =
                std::panic::catch_unwind(|| delete::check_geometry(&i, observation, deleted));
            assert!(
                result.is_ok(),
                "public check panicked: {name}; deleted={deleted}"
            );
            assert!(
                result.unwrap().is_err(),
                "malformed intent was accepted: {name}; deleted={deleted}"
            );
        }
        // select_feature is a second public consumer of the same geometry
        // guard. It must not index or select a feature from malformed intent.
        let result = std::panic::catch_unwind(|| delete::select_feature(&i, &inventory(), &before));
        assert!(result.is_ok(), "public selection panicked: {name}");
        assert!(
            result.unwrap().is_err(),
            "selection accepted malformed intent: {name}"
        );
    }
}

#[test]
fn feature_delete_geometry_preserves_other_hole_and_full_domain() {
    let i = intent();
    let before = detail(false);
    let after = detail(true);
    delete::check_geometry(&i, &before, false).unwrap();
    delete::check_geometry(&i, &after, true).unwrap();
    assert!(delete::check_geometry(&i, &before, true).is_err());
    assert!(delete::check_geometry(&i, &after, false).is_err());
    let mut cases = Vec::new();
    let mut c = after.clone();
    c["model_edges"]["entries"][0]["arc"]["radius"] = json!(7.);
    cases.push(c);
    let mut c = after.clone();
    c["model_edges"]["entries"][1]["arc"]["center"][2] = json!(9.);
    cases.push(c);
    let mut c = after.clone();
    c["model_edges"]["entries"][3]["query"]["capture_complete"] = json!(false);
    cases.push(c);
    let mut c = after.clone();
    c["model_edges"]["entries"].as_array_mut().unwrap().pop();
    cases.push(c);
    let mut c = after.clone();
    c["solid_face_inventory_complete"] = json!(false);
    cases.push(c);
    let mut c = after.clone();
    c["solid_face_inventory"][6]["radius"] = json!(7.);
    cases.push(c);
    let mut c = after.clone();
    c["solid_face_inventory"][6]["query_code"] = json!(1);
    cases.push(c);
    let mut c = after.clone();
    c["solid_face_inventory"][0]["axis_point"][0] = json!(-39.);
    cases.push(c);
    let mut c = after.clone();
    c["solid_face_inventory"][0]["body_tag"] = json!(200);
    cases.push(c);
    let mut c = after.clone();
    c["exact_work_csys_bounds"][0]["absolute_axis_bounds"][3] = json!(41.);
    cases.push(c);
    let mut c = after.clone();
    c["units"] = json!("inch");
    cases.push(c);
    for (n, c) in cases.iter().enumerate() {
        assert!(
            delete::check_geometry(&i, c, true).is_err(),
            "frozen geometry fault {n}"
        );
    }
    // SDK expressly permits a nonminimal enclosing face box. Exact whole-body
    // bounds and complete topology remain independently checked.
    let mut enlarged = after;
    enlarged["solid_face_inventory"][6]["face_bounds"] = json!([13., -7., -1., 27., 7., 9.]);
    delete::check_geometry(&i, &enlarged, true).unwrap();
}
#[test]
fn feature_delete_exact_bounds_reject_missing_raw_fields_and_conflicting_projection() {
    let i = intent();
    for deleted in [false, true] {
        let observed = detail(deleted);
        delete::check_geometry(&i, &observed, deleted).unwrap();
        let mut cases = Vec::new();
        for field in [
            "min_corner",
            "directions",
            "distances",
            "absolute_axis_bounds",
        ] {
            let mut c = observed.clone();
            c["exact_work_csys_bounds"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            cases.push((format!("missing {field}"), c));
            let mut c = observed.clone();
            c["exact_work_csys_bounds"][0][field] = json!([]);
            cases.push((format!("wrong length {field}"), c));
        }
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["min_corner"][0] = json!(-39.);
        cases.push(("raw corner conflicts with retained projection".into(), c));
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["distances"][0] = json!(79.);
        cases.push(("raw extent conflicts with retained projection".into(), c));
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["distances"][2] = json!(-8.);
        cases.push(("negative raw extent".into(), c));
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["directions"][0][0] = json!(2.);
        cases.push(("non-unit raw direction".into(), c));
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["directions"][1] = json!([1., 0., 0.]);
        cases.push(("non-orthogonal raw frame".into(), c));
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["absolute_axis_bounds"][3] = json!(41.);
        cases.push(("projection conflicts with retained raw data".into(), c));
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["min_corner"][0] = Value::Null;
        cases.push(("raw numeric value missing".into(), c));
        let mut c = observed.clone();
        c["exact_work_csys_bounds"][0]["min_corner"][0] = json!(1e308);
        c["exact_work_csys_bounds"][0]["distances"][0] = json!(1e308);
        cases.push(("finite raw values overflow a derived corner".into(), c));
        for (name, c) in cases {
            let result = std::panic::catch_unwind(|| delete::check_geometry(&i, &c, deleted));
            assert!(result.is_ok(), "exact-bounds check panicked: {name}");
            assert!(
                result.unwrap().is_err(),
                "inconsistent raw bounds accepted: {name}"
            );
        }
        // An independently specified, rotated work frame encodes the same
        // box. The check must recompute it, not demand an identity matrix.
        let mut rotated = observed;
        rotated["exact_work_csys_bounds"][0]["min_corner"] = json!([40., -20., 0.]);
        rotated["exact_work_csys_bounds"][0]["directions"] =
            json!([[0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]]);
        rotated["exact_work_csys_bounds"][0]["distances"] = json!([40., 80., 8.]);
        delete::check_geometry(&i, &rotated, deleted).unwrap();
    }
}

#[test]
fn feature_delete_keeps_originals_and_assembly_relations() {
    let i = intent();
    let b = budget();
    let before = assembly(false);
    let after = assembly(true);
    delete::check_assembly(&i, &b, &before, &after).unwrap();
    delete::check_reference_sets(
        &before,
        &reference_sets(&before),
        &after,
        &reference_sets(&after),
    )
    .unwrap();
    let mut cases = Vec::new();
    let mut c = after.clone();
    c.prototypes[1].solids[0].volume_m3 = Some(0.000002);
    cases.push(c);
    let mut c = after.clone();
    c.prototypes[2].solids[0].volume_m3 = before.prototypes[2].solids[0].volume_m3;
    cases.push(c);
    let mut c = after.clone();
    c.prototypes[2].solids[0].body_tag = 301;
    cases.push(c);
    let mut c = after.clone();
    c.occurrences[0].name = "CHANGED".into();
    cases.push(c);
    let mut c = after.clone();
    c.occurrences[0].part_occurrence_tag = 1010;
    cases.push(c);
    let mut c = after.clone();
    c.occurrences.pop();
    cases.push(c);
    for (n, c) in cases.iter().enumerate() {
        assert!(
            delete::check_assembly(&i, &b, &before, c).is_err(),
            "frozen assembly fault {n}"
        );
    }
}

struct Api {
    calls: usize,
    saves: usize,
    call: NxFeatureDeleteOutcome1,
    inventory: NxFeatureInventory1,
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
            call: outcome(),
            inventory: inventory(),
            after_detail: detail(true),
            after_assembly: assembly(true),
            save_failure: false,
            native_error: false,
        }
    }
}
impl NativeApi for Api {
    fn feature_inventory(&mut self, part: u64, body: u64) -> Result<NxFeatureInventory1> {
        assert_eq!((part, body), (3, 300));
        Ok(self.inventory.clone())
    }
    fn delete_feature(
        &mut self,
        part: u64,
        body: u64,
        feature: u64,
    ) -> Result<NxFeatureDeleteOutcome1> {
        assert_eq!((part, body, feature), (3, 300, 501));
        self.calls += 1;
        if self.native_error {
            bail!("frozen native failure");
        }
        Ok(self.call.clone())
    }
    fn detailed_info(&mut self, part: u32, _: bool) -> Result<Value> {
        assert_eq!(part, 3);
        Ok(if self.calls == 0 {
            detail(false)
        } else {
            self.after_detail.clone()
        })
    }
    fn set_work_part(&mut self, part: u32) -> Result<()> {
        assert_eq!(part, 3);
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
        _: u32,
        _: &Path,
        _: &NxAssemblyBudget,
        _: &str,
    ) -> Result<NxAssemblyObservation> {
        Ok(if self.calls == 0 {
            assembly(false)
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
    delete::execute(
        api,
        w,
        &ArtifactReference {
            path: "delete.json".into(),
            sha256: "f".repeat(64),
        },
        &intent(),
        &budget(),
    )
}
#[test]
fn feature_delete_failure_never_saves_or_retries() {
    for fault in 0..6 {
        let mut api = Api::default();
        let mut w = Work::default();
        match fault {
            0 => api.inventory.features[1].owning_part_tag = 2,
            1 => api.call.return_code = 17,
            2 => api.call.update_code = Some(9),
            3 => api.after_detail["solid_face_inventory_complete"] = json!(false),
            4 => api.after_assembly.prototypes[1].solids[0].volume_m3 = Some(0.000002),
            _ => api.native_error = true,
        }
        assert!(run(&mut api, &mut w).is_err(), "failure {fault}");
        assert_eq!(api.saves, 0);
        assert_eq!(api.calls, usize::from(fault != 0));
        assert!(
            !w.events
                .iter()
                .any(|(p, _)| p == "feature_delete_save_pending")
        );
        if fault == 1 || fault == 2 {
            assert!(w.events.iter().any(|(p, _)| p == "feature_delete_returned"));
            assert!(w.events.iter().any(|(p, _)| p == "feature_delete_after"));
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
            .any(|(p, _)| p == "feature_delete_saved_inventory")
    );
}
#[test]
fn feature_delete_success_records_frozen_order_without_claiming_reopen() {
    let mut api = Api::default();
    let mut w = Work::default();
    let result = run(&mut api, &mut w).unwrap();
    assert_eq!((api.calls, api.saves), (1, 1));
    assert_eq!(result["independent_reopen_checked"], false);
    assert_eq!(result["engineering_adoption"], "not_assessed");
    let expected = [
        "before",
        "before_reference_sets",
        "before_detail",
        "feature_inventory",
        "selection",
        "pending",
        "returned",
        "after_detail",
        "after",
        "after_reference_sets",
        "save_pending",
        "save_returned",
        "saved_detail",
        "saved_inventory",
        "saved_reference_sets",
    ];
    assert_eq!(
        w.events
            .iter()
            .map(|(p, _)| p.strip_prefix("feature_delete_").unwrap())
            .collect::<Vec<_>>(),
        expected
    );
    let params: geometry::Parameters = serde_json::from_value(
        json!({"action":delete::ACTION,"schema":delete::PARAMS_SCHEMA,
        "intent":{"path":"delete.json","sha256":"a".repeat(64)},"budget":budget()}),
    )
    .unwrap();
    params.validate().unwrap();
    assert_eq!(params.parameter_schema(), delete::PARAMS_SCHEMA);
}
