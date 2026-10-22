//! Controlled decision/observation tests. These do not claim native SDK execution.
use anyhow::{Result, bail};
use oe_nx_protocol::RelativePath;
use oe_nx_worker::geometry::*;
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy)]
enum BoundsFault {
    MissingVertices,
    UnknownPlanarity,
    ExtraVertices,
    NonfiniteVertex,
    NonfiniteEnvelope,
    InvertedVertices,
    InvertedEnvelope,
}
#[derive(Default)]
struct Api {
    drafting_failure: bool,
    partial_read_failure: bool,
    detailed_reads: usize,
    parasolid_fault: Option<&'static str>,
    calls: usize,
    refset_ready: bool,
    refset_fault: Option<&'static str>,
    after_save: bool,
    jt_after_save: Option<bool>,
    length: f64,
    width: f64,
    height: f64,
    components: Vec<Component>,
    wrong_units: bool,
    update_errors: i32,
    stale_shape: bool,
    wrong_transform: bool,
    envelope_padding: f64,
    vertex_extent_delta: f64,
    envelope_upper_delta: f64,
    bounds_fault: Option<BoundsFault>,
}
impl Api {
    fn block(&self, path: &str) -> PartInfo {
        let mut info = PartInfo {
            path: path.into(),
            part_tag: 1,
            representation_baseline: REPRESENTATION_BASELINE.into(),
            controlled_jt_mapping: None,
            units: if self.wrong_units { "inch" } else { "mm" }.into(),
            solid_count: 1,
            solid_tags: vec![100],
            controlled_reference_set: ControlledReferenceSet {
                name: CONTROLLED_REFSET.into(),
                found: self.refset_ready,
                reference_set_tag: if self.refset_ready { 101 } else { 0 },
                member_tags: if self.refset_ready { vec![100] } else { vec![] },
            },
            sheet_count: 0,
            bbox: vec![[0.0, 0.0, 0.0, self.length, self.width, self.height]],
            bbox_role: "fixture only".into(),
            planar_vertex_bounds: vec![Some([0.0, 0.0, 0.0, self.length, self.width, self.height])],
            exact_work_csys_bounds: vec![
                ExactBounds::new(
                    [0.0; 3],
                    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    [self.length, self.width, self.height],
                )
                .unwrap(),
            ],
            expressions: vec![Expression {
                name: "cad_length".into(),
                rhs: self.length.to_string(),
                value: self.length,
            }],
            components: vec![],
        };
        for axis in 0..3 {
            info.bbox[0][axis] -= self.envelope_padding;
            info.bbox[0][axis + 3] += self.envelope_padding;
        }
        info.bbox[0][3] += self.envelope_upper_delta;
        info.planar_vertex_bounds[0].as_mut().unwrap()[3] += self.vertex_extent_delta;
        match self.bounds_fault {
            Some(BoundsFault::MissingVertices) => info.planar_vertex_bounds.clear(),
            Some(BoundsFault::UnknownPlanarity) => info.planar_vertex_bounds[0] = None,
            Some(BoundsFault::ExtraVertices) => {
                info.planar_vertex_bounds.push(info.planar_vertex_bounds[0])
            }
            Some(BoundsFault::NonfiniteVertex) => {
                info.planar_vertex_bounds[0].as_mut().unwrap()[0] = f64::NAN
            }
            Some(BoundsFault::NonfiniteEnvelope) => info.bbox[0][3] = f64::INFINITY,
            Some(BoundsFault::InvertedVertices) => {
                info.planar_vertex_bounds[0].as_mut().unwrap()[0] = self.length + 1.0
            }
            Some(BoundsFault::InvertedEnvelope) => info.bbox[0][0] = self.length + 1.0,
            None => {}
        }
        if self.refset_ready {
            match self.refset_fault {
                Some("body_identity") => {
                    info.solid_tags = vec![200];
                    info.controlled_reference_set.member_tags = vec![200];
                }
                Some("save_body_identity") if self.after_save => {
                    info.solid_tags = vec![300];
                    info.controlled_reference_set.member_tags = vec![300];
                }
                Some("wrong_member") => info.controlled_reference_set.member_tags = vec![999],
                Some("extra_member") => info.controlled_reference_set.member_tags.push(999),
                Some("wrong_name") => info.controlled_reference_set.name = "MODEL".into(),
                Some("geometry") => info.planar_vertex_bounds[0].as_mut().unwrap()[3] += 1.0,
                Some("save_extra_member") if self.after_save => {
                    info.controlled_reference_set.member_tags.push(999)
                }
                Some("save_bad_geometry") if self.after_save => {
                    info.planar_vertex_bounds[0].as_mut().unwrap()[3] += 1.0
                }
                Some("save_loses_members") if self.after_save => {
                    info.controlled_reference_set.member_tags.clear()
                }
                _ => {}
            }
        }
        if self.refset_ready
            && self.after_save
            && let Some(current) = self.jt_after_save
        {
            info.controlled_reference_set.member_tags.push(102);
            let mut mapping = valid_mapping(&info);
            mapping.up_to_date = Some(current);
            info.controlled_jt_mapping = Some(mapping);
        }
        info
    }
}
impl NativeApi for Api {
    fn set_work_part(&mut self, _: u32) -> Result<()> {
        Ok(())
    }
    fn detailed_info(&mut self, _: u32, _: bool) -> Result<Value> {
        if !self.drafting_failure {
            bail!("native_detailed_readback_unsupported")
        }
        self.detailed_reads += 1;
        if self.detailed_reads > 1 && self.partial_read_failure {
            bail!("fixture_partial_read_failure")
        }
        Ok(json!({"fixture":true,"read":self.detailed_reads}))
    }
    fn create_drafting_sheet(
        &mut self,
        _: u32,
        _: &oe_contracts::NativeDraftingPlan,
    ) -> Result<Value> {
        assert!(self.drafting_failure);
        Ok(
            json!({"complete":false,"error":"native_drafting_position_failed",
            "native_codes":{"set_reference":1405087}}),
        )
    }
    fn ensure_empty_session(&mut self) -> Result<()> {
        self.calls += 1;
        Ok(())
    }
    fn create_mm_part(&mut self, path: &Path) -> Result<u32> {
        self.calls += 1;
        Ok(if path.ends_with("two_instances.prt") {
            2
        } else {
            1
        })
    }
    fn open_part(&mut self, _: &Path) -> Result<u32> {
        self.calls += 1;
        self.length = 55.0;
        self.width = 20.0;
        self.height = 10.0;
        Ok(1)
    }
    fn create_mm_expression(&mut self, _: u32, f: &str) -> Result<u32> {
        self.length = f.split('=').nth(1).unwrap().parse()?;
        Ok(3)
    }
    fn create_block(&mut self, _: [f64; 3], lengths: [String; 3]) -> Result<u32> {
        self.width = lengths[1].parse()?;
        self.height = lengths[2].parse()?;
        Ok(4)
    }
    fn edit_expression_update(&mut self, _: u32, _: u32, formula: &str) -> Result<i32> {
        if !self.stale_shape {
            self.length = formula.parse()?
        }
        Ok(self.update_errors)
    }
    fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
        self.calls += 1;
        if self.refset_fault == Some("already_exists") {
            bail!("controlled_reference_set_already_exists")
        }
        self.refset_ready = true;
        Ok(json!({"scope":"synthetic","name":CONTROLLED_REFSET}))
    }
    fn save_work_part(&mut self) -> Result<()> {
        self.calls += 1;
        self.after_save = true;
        Ok(())
    }
    fn add_component(
        &mut self,
        _: u32,
        _: u32,
        path: &Path,
        name: &str,
        origin: [f64; 3],
    ) -> Result<()> {
        let mut matrix = [0.0; 9];
        matrix[0] = 1.0;
        matrix[4] = 1.0;
        matrix[8] = if self.wrong_transform { 0.0 } else { 1.0 };
        self.components.push(Component {
            name: name.into(),
            prototype_path: path.to_string_lossy().into(),
            reference_set: if self.refset_fault == Some("component_representation") {
                "MODEL"
            } else {
                CONTROLLED_REFSET
            }
            .into(),
            origin,
            matrix,
        });
        Ok(())
    }
    fn part_info(&mut self, part: u32) -> Result<PartInfo> {
        let mut info = self.block("fixture/part.prt");
        match self.parasolid_fault {
            Some("no_exact_bounds") => info.exact_work_csys_bounds.clear(),
            Some("sheet") => info.sheet_count = 1,
            Some("wrong_body") => info.solid_tags = vec![999],
            Some("nonfinite") => info.exact_work_csys_bounds[0].distances[0] = f64::NAN,
            Some("modified_box") => info.exact_work_csys_bounds[0].absolute_axis_bounds[3] += 1.0,
            _ => {}
        }
        if part == 2 {
            info.part_tag = 2;
            info.controlled_jt_mapping = None;
            info.solid_count = 0;
            info.solid_tags.clear();
            info.controlled_reference_set.found = false;
            info.controlled_reference_set.member_tags.clear();
            info.bbox.clear();
            info.planar_vertex_bounds.clear();
            info.expressions.clear();
            info.components = self.components.clone()
        }
        Ok(info)
    }
    fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
        self.calls += 1;
        Ok(())
    }
    fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
        self.calls += 1;
        Ok(())
    }
    fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
        self.calls += 1;
        if self.parasolid_fault == Some("native_error") {
            bail!("native_import_error")
        }
        self.length = 5.0;
        self.width = 2.0;
        self.height = 2.0;
        Ok(if self.parasolid_fault == Some("empty") {
            vec![]
        } else {
            vec![100]
        })
    }
}

#[test]
fn accurate_csys_box_uses_all_corners_when_rotated() {
    let x = 2.0_f64.sqrt() / 2.0;
    let b = ExactBounds::new(
        [10.0, 20.0, 30.0],
        [[x, x, 0.0], [-x, x, 0.0], [0.0, 0.0, 1.0]],
        [2.0, 4.0, 5.0],
    )
    .unwrap();
    let expected = [
        10.0 - 4.0 * x,
        20.0,
        30.0,
        10.0 + 2.0 * x,
        20.0 + 6.0 * x,
        35.0,
    ];
    for (a, b) in b.absolute_axis_bounds.iter().zip(expected) {
        assert!((a - b).abs() < 1e-12);
    }
}

#[test]
fn native_box_rejects_invalid_frame_nonfinite_negative_and_overflow() {
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    assert!(ExactBounds::new([f64::NAN, 0.0, 0.0], axes, [1.0; 3]).is_err());
    assert!(ExactBounds::new([0.0; 3], axes, [-1.0, 1.0, 1.0]).is_err());
    assert!(ExactBounds::new([0.0; 3], [[1.0, 0.0, 0.0]; 3], [1.0; 3]).is_err());
    assert!(ExactBounds::new([f64::MAX; 3], axes, [f64::MAX; 3]).is_err());
}

fn parasolid_parameters() -> Parameters {
    serde_json::from_value(
        json!({"schema":PARAMS_SCHEMA,"action":"import_parasolid","parasolid":"supplier.x_b"}),
    )
    .unwrap()
}

#[test]
fn parasolid_unsafe_inputs_rejected_before_native_execution() {
    for name in [
        "../escape.x_b",
        "C:/outside.x_b",
        "supplier.step",
        "supplier.dll",
        "supplier.x_b/../outside.x_b",
    ] {
        let params: Parameters = serde_json::from_value(
            json!({"schema":PARAMS_SCHEMA,"action":"import_parasolid","parasolid":name}),
        )
        .unwrap();
        let mut api = Api::default();
        assert!(execute(&mut api, &mut Files::default(), &params).is_err());
        assert_eq!(api.calls, 0);
    }
}

#[test]
fn parasolid_failure_is_observed_without_saving_or_repeating() {
    for fault in [
        "native_error",
        "empty",
        "wrong_body",
        "sheet",
        "no_exact_bounds",
        "nonfinite",
        "modified_box",
    ] {
        let mut api = Api {
            parasolid_fault: Some(fault),
            ..Default::default()
        };
        let mut files = Files::default();
        assert!(
            execute(&mut api, &mut files, &parasolid_parameters()).is_err(),
            "{fault}"
        );
        assert!(!api.after_save, "{fault}");
        assert_eq!(api.calls, 3, "{fault}");
        assert!(
            files
                .events
                .iter()
                .any(|e| e["phase"] == "parasolid_import_intent")
        );
    }
    let mut api = Api {
        wrong_units: true,
        ..Default::default()
    };
    assert!(execute(&mut api, &mut Files::default(), &parasolid_parameters()).is_err());
    assert!(!api.after_save);
}

fn valid_mapping(info: &PartInfo) -> JtMapping {
    JtMapping {
        schema: JT_MAPPING_SCHEMA.into(),
        owner_tag: info.part_tag,
        solid_tag: info.solid_tags[0],
        reference_set_tag: info.controlled_reference_set.reference_set_tag,
        member_tags_before: info.controlled_reference_set.member_tags.clone(),
        member_tags_after: Some(info.controlled_reference_set.member_tags.clone()),
        reference_set_after: Some(info.controlled_reference_set.reference_set_tag),
        wrapper_code: 0,
        call_entered: true,
        call_returned: true,
        complete: true,
        exception_code: None,
        solid_owner_tag: Some(info.part_tag),
        jt_tag: Some(102),
        up_to_date: Some(true),
        type_query_code: Some(0),
        jt_type: Some(139),
        jt_subtype: Some(3),
        owner_query_code: Some(0),
        jt_owner_tag: Some(info.part_tag),
        error: None,
    }
}

#[test]
fn current_jt_mapping_preserves_raw_members_and_physical_create_contract() {
    let mut api = Api {
        jt_after_save: Some(true),
        ..Default::default()
    };
    let mut files = Files::default();
    let result = execute(&mut api, &mut files, &parameters("create")).unwrap();
    let saved = files
        .events
        .iter()
        .find(|e| e["phase"] == "controlled_refset_after_save")
        .unwrap();
    assert_eq!(
        saved["value"]["controlled_reference_set"]["member_tags"],
        json!([100, 102])
    );
    assert_eq!(saved["value"]["controlled_jt_mapping"]["jt_tag"], 102);
    assert_eq!(
        saved["value"]["controlled_jt_mapping"]["member_tags_before"],
        json!([100, 102])
    );
    assert_eq!(
        result["after"]["planar_vertex_bounds"],
        json!([[0.0, 0.0, 0.0, 55.0, 20.0, 10.0]])
    );
    assert_eq!(api.components.len(), 2);
    assert_eq!(api.components[1].origin, [80.0, 0.0, 0.0]);
}

#[test]
fn stale_or_unbound_jt_never_reaches_assembly_and_complete_snapshot_survives() {
    let mut api = Api {
        jt_after_save: Some(false),
        ..Default::default()
    };
    let mut files = Files::default();
    let error = execute(&mut api, &mut files, &parameters("create")).unwrap_err();
    assert_eq!(error.to_string(), "controlled_refset_jt_mapping_invalid");
    assert!(api.components.is_empty());
    let saved = files
        .events
        .iter()
        .find(|e| e["phase"] == "controlled_refset_after_save_observed")
        .unwrap();
    assert_eq!(saved["value"]["controlled_jt_mapping"]["up_to_date"], false);
    assert_eq!(
        saved["value"]["controlled_reference_set"]["member_tags"],
        json!([100, 102])
    );
    assert!(
        !files
            .events
            .iter()
            .any(|e| e["phase"] == "controlled_refset_after_save")
    );
}

#[test]
fn associated_jt_requires_same_observation_owner_body_and_known_success() {
    let api = Api {
        refset_ready: true,
        after_save: true,
        jt_after_save: Some(true),
        ..Default::default()
    };
    let original = api.block("part.prt");
    require_controlled_refset(&original).unwrap();
    for (field, value) in [
        ("schema", json!("other")),
        ("owner_tag", json!(9)),
        ("solid_tag", json!(999)),
        ("solid_owner_tag", json!(9)),
        ("jt_owner_tag", json!(9)),
        ("jt_tag", json!(999)),
        ("jt_tag", Value::Null),
        ("up_to_date", Value::Null),
        ("up_to_date", json!(false)),
        ("wrapper_code", json!(1)),
        ("complete", json!(false)),
        ("call_entered", json!(false)),
        ("call_returned", json!(false)),
        ("exception_code", json!(123)),
        ("error", json!("unknown")),
        ("type_query_code", json!(1)),
        ("jt_type", json!(70)),
        ("jt_subtype", json!(0)),
        ("owner_query_code", Value::Null),
        ("reference_set_tag", json!(999)),
        ("reference_set_after", json!(999)),
        ("member_tags_after", Value::Null),
        ("member_tags_after", json!([100, 999])),
        ("member_tags_before", json!([100, 999])),
    ] {
        let mut info = original.clone();
        let mut record =
            serde_json::to_value(info.controlled_jt_mapping.as_ref().unwrap()).unwrap();
        record[field] = value;
        info.controlled_jt_mapping = Some(serde_json::from_value(record).unwrap());
        assert!(require_controlled_refset(&info).is_err(), "{field}");
    }
    let mut other_process = original.clone();
    other_process.part_tag = 7;
    assert!(require_controlled_refset(&other_process).is_err());
}

#[test]
fn jt_member_set_never_filters_duplicates_or_extra_objects() {
    let api = Api {
        refset_ready: true,
        after_save: true,
        jt_after_save: Some(true),
        ..Default::default()
    };
    let original = api.block("part.prt");
    for members in [
        vec![100, 102, 999],
        vec![100, 102, 102],
        vec![100, 100],
        vec![102],
        vec![],
    ] {
        let mut info = original.clone();
        info.controlled_reference_set.member_tags = members.clone();
        let m = info.controlled_jt_mapping.as_mut().unwrap();
        m.member_tags_before = members.clone();
        m.member_tags_after = Some(members);
        assert!(require_controlled_refset(&info).is_err());
    }
    let mut reordered = original.clone();
    reordered.controlled_reference_set.member_tags.reverse();
    reordered
        .controlled_jt_mapping
        .as_mut()
        .unwrap()
        .member_tags_before
        .reverse();
    require_controlled_refset(&reordered).unwrap();
    let mut extra_solid = original.clone();
    extra_solid.solid_count = 2;
    extra_solid.solid_tags.push(103);
    assert!(require_controlled_refset(&extra_solid).is_err());
    let mut missing_mapping = original;
    missing_mapping.controlled_jt_mapping = None;
    assert!(require_controlled_refset(&missing_mapping).is_err());
}
struct Files {
    step: Vec<u8>,
    step_observations: Vec<Option<Vec<u8>>>,
    step_reads: Cell<usize>,
    step_error: Option<&'static str>,
    events: Vec<Value>,
    deny_input: bool,
    deny_after_save_record: bool,
}
impl Default for Files {
    fn default() -> Self {
        Self {
            step: b"ISO-10303-21;\nMANIFOLD_SOLID_BREP\nEND-ISO-10303-21;\n".to_vec(),
            step_observations: vec![],
            step_reads: Cell::new(0),
            step_error: None,
            events: vec![],
            deny_input: false,
            deny_after_save_record: false,
        }
    }
}
impl Workspace for Files {
    fn input(&self, p: &RelativePath) -> Result<PathBuf> {
        if self.deny_input {
            bail!("not_manifest_input")
        }
        Ok(PathBuf::from("fixture").join(p.as_str()))
    }
    fn output(&self, name: &str) -> Result<PathBuf> {
        Ok(PathBuf::from("fixture/out").join(name))
    }
    fn require_regular_output(&self, name: &str) -> Result<Vec<u8>> {
        Ok(if name.ends_with(".step") {
            self.step.clone()
        } else {
            b"fixture native bytes".to_vec()
        })
    }
    fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
        if let Some(reason) = self.step_error {
            bail!("{reason}");
        }
        let read = self.step_reads.get();
        self.step_reads.set(read + 1);
        if self.step_observations.is_empty() {
            return Ok(Some(self.step.clone()));
        }
        Ok(self.step_observations[read.min(self.step_observations.len() - 1)].clone())
    }
    fn record(&mut self, phase: &str, v: Value) -> Result<()> {
        if self.deny_after_save_record && phase == "controlled_refset_after_save_observed" {
            bail!("owned_fixture_record_failure");
        }
        self.events.push(json!({"phase":phase,"value":v}));
        Ok(())
    }
}
fn parameters(action: &str) -> Parameters {
    serde_json::from_value(json!({"schema":PARAMS_SCHEMA,"action":action})).unwrap()
}
#[test]
fn defaults_preserve_old_fixture_but_do_not_claim_release() {
    let mut api = Api::default();
    let mut files = Files::default();
    let result = execute(&mut api, &mut files, &parameters("create")).unwrap();
    assert_eq!(result["before"]["bbox"][0][3], 40.0);
    assert_eq!(result["after"]["bbox"][0][3], 55.0);
    assert_eq!(
        result["assembly"]["components"].as_array().unwrap().len(),
        2
    );
    assert_eq!(result["engineering_verdict"], "not_assessed");
    assert_eq!(result["step_roundtrip"], "requires_separate_native_process");
}

#[test]
fn reference_dimension_rejects_invalid_plane_and_changed_cad_before_copy_or_creation() {
    let point = |id, x| {
        json!({"point_id":id,"position":[x,0,0],"point_type_raw":2,"line_type_raw":0,"parent_type_raw":70,"parent_subtype_raw":3,
        "parent_edge":{"feature_id":id,"center":[x,0,0],"normal":[0,0,1],"radius":2.5,
            "linear_tolerance":0.001,"angular_tolerance_radians":1e-6,"unit":"mm","local_frame":"part_absolute","full_circle":true}})
    };
    let params = json!({"schema":PARAMS_SCHEMA,"action":"create_reference_dimension","part":"part.prt",
        "intent":{"schema":"oev2.drawing-reference-intent/2","intent_id":"two-holes",
            "object":{"object_id":"coupon","revision":"r1","condition_scope":"trial","source_sha256":"a".repeat(64)},
            "local_frame":"part_absolute","unit":"mm","position_tolerance":0.001,"proof_scope":"unique_model_parent",
            "source_refs":["independent-plan"],"points":[point("a",0.0),point("b",30.0)],
            "dimension_value":{"metric":"computed_linear","value":30.0,"numeric_tolerance":0.001,"unit":"mm","dimension_subtype_raw":3}},
        "plane_origin":[0,0,0],"plane_x_axis":[1,0,0],"plane_y_axis":[0,1,0],"text_origin_wcs":[15,7,0]});
    let correct: Parameters = serde_json::from_value(params.clone()).unwrap();
    correct.validate().unwrap();
    let mut wrong = params.clone();
    wrong["plane_origin"] = json!([0, 0, 0.1]);
    let invalid: Parameters = serde_json::from_value(wrong).unwrap();
    let mut api = Api::default();
    let mut files = Files::default();
    assert!(
        execute(&mut api, &mut files, &invalid)
            .unwrap_err()
            .to_string()
            .contains("plane_or_center_mismatch")
    );
    assert_eq!(api.calls, 0);
    let mut wrong = params;
    wrong["plane_y_axis"] = json!([1, 0, 0]);
    assert!(
        serde_json::from_value::<Parameters>(wrong)
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string()
            .contains("orthonormal")
    );
    struct SourceFiles(PathBuf);
    impl Workspace for SourceFiles {
        fn input(&self, _: &RelativePath) -> Result<PathBuf> {
            Ok(self.0.join("part.prt"))
        }
        fn output(&self, _: &str) -> Result<PathBuf> {
            bail!("copy_must_not_be_requested")
        }
        fn require_regular_output(&self, _: &str) -> Result<Vec<u8>> {
            bail!("no_output")
        }
        fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
            bail!("no_output")
        }
        fn record(&mut self, _: &str, _: Value) -> Result<()> {
            bail!("no_record")
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("part.prt"), b"changed CAD bytes").unwrap();
    let mut files = SourceFiles(tmp.path().to_owned());
    assert_eq!(
        execute(&mut api, &mut files, &correct)
            .unwrap_err()
            .to_string(),
        "reference_dimension_cad_source_mismatch"
    );
    assert!(std::fs::read_dir(tmp.path()).unwrap().count() == 1);
}
#[test]
fn drafting_failure_preserves_partial_diagnostics_without_save_or_input_changes() {
    struct DraftFiles {
        root: PathBuf,
        events: Vec<Value>,
    }
    impl Workspace for DraftFiles {
        fn input(&self, _: &RelativePath) -> Result<PathBuf> {
            Ok(self.root.join("part.prt"))
        }
        fn output(&self, name: &str) -> Result<PathBuf> {
            Ok(self.root.join("out").join(name))
        }
        fn require_regular_output(&self, name: &str) -> Result<Vec<u8>> {
            Ok(std::fs::read(self.output(name)?)?)
        }
        fn optional_regular_output(&self, name: &str) -> Result<Option<Vec<u8>>> {
            Ok(Some(self.require_regular_output(name)?))
        }
        fn record(&mut self, phase: &str, value: Value) -> Result<()> {
            self.events.push(json!({"phase":phase,"value":value}));
            Ok(())
        }
    }
    for partial_read_failure in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("out")).unwrap();
        let bytes = b"unchanged frozen native input";
        std::fs::write(tmp.path().join("part.prt"), bytes).unwrap();
        let params: Parameters = serde_json::from_value(json!({"schema":PARAMS_SCHEMA,
            "action":"create_drafting_sheet","part":"part.prt",
            "plan":{"schema":"oev2.native-drafting-plan/1",
                "object":{"object_id":"coupon","revision":"r1","condition_scope":"trial",
                    "source_sha256":oe_nx_protocol::BlobIdentity::of(bytes).sha256},
                "sheet_name":"TRIAL","unit":"mm","width":297,"height":210,
                "scale_numerator":1,"scale_denominator":1,"projection_angle":"third",
                "model_view_name":"Right","view_center":[148.5,105],
                "source_refs":["frozen-plan"],"purpose":"prototype_geometry_only"}}))
        .unwrap();
        let mut api = Api {
            drafting_failure: true,
            partial_read_failure,
            ..Default::default()
        };
        let mut files = DraftFiles {
            root: tmp.path().into(),
            events: vec![],
        };
        assert_eq!(
            execute(&mut api, &mut files, &params)
                .unwrap_err()
                .to_string(),
            "drafting_native_creation_incomplete"
        );
        assert!(!api.after_save);
        assert_eq!(api.detailed_reads, 2);
        assert_eq!(std::fs::read(tmp.path().join("part.prt")).unwrap(), bytes);
        assert_eq!(files.require_regular_output("part.prt").unwrap(), bytes);
        assert_eq!(
            files.events[1]["value"]["native_codes"]["set_reference"],
            1405087
        );
        assert_eq!(
            files.events[2]["phase"],
            if partial_read_failure {
                "drafting_partial_observation_failed"
            } else {
                "drafting_partial_after_creation"
            }
        );
    }
}
#[test]
fn wrong_units_cannot_pass_native_dimensions() {
    let mut api = Api {
        wrong_units: true,
        ..Default::default()
    };
    let mut files = Files::default();
    assert!(execute(&mut api, &mut files, &parameters("create")).is_err());
    assert_eq!(files.events.len(), 1);
}
#[test]
fn native_update_error_prevents_save_and_translation() {
    let mut api = Api {
        update_errors: 2,
        ..Default::default()
    };
    let mut files = Files::default();
    assert!(execute(&mut api, &mut files, &parameters("create")).is_err());
    assert_eq!(api.calls, 2);
    assert!(files.events.iter().any(|e| e["phase"] == "native_update"));
}
#[test]
fn changed_expression_without_changed_geometry_is_rejected() {
    let mut api = Api {
        stale_shape: true,
        ..Default::default()
    };
    assert!(execute(&mut api, &mut Files::default(), &parameters("create")).is_err());
}
#[test]
fn singular_occurrence_transform_cannot_pass() {
    let mut api = Api {
        wrong_transform: true,
        ..Default::default()
    };
    assert!(execute(&mut api, &mut Files::default(), &parameters("create")).is_err());
}
#[test]
fn header_only_step_is_not_solid_evidence() {
    let mut files = Files {
        step: b"ISO-10303-21;\nEND-ISO-10303-21;".to_vec(),
        ..Default::default()
    };
    assert!(execute(&mut Api::default(), &mut files, &parameters("create")).is_err());
}
#[test]
fn step_export_waits_for_missing_and_partial_output() {
    let mut files = Files {
        step_observations: vec![
            None,
            Some(b"ISO-10303-21;\nMANIFOLD_SOLID_BREP".to_vec()),
            Some(Files::default().step),
        ],
        ..Default::default()
    };
    execute(&mut Api::default(), &mut files, &parameters("create")).unwrap();
    assert_eq!(files.step_reads.get(), 3);
    assert!(
        files
            .events
            .iter()
            .any(|e| e["phase"] == "step_export_completed")
    );
}
#[test]
fn step_export_timeout_does_not_accept_partial_output() {
    let files = Files {
        step_observations: vec![Some(b"ISO-10303-21;\nMANIFOLD_SOLID_BREP".to_vec())],
        ..Default::default()
    };
    let error = await_step_export(&files, "part.step", Duration::ZERO, Duration::ZERO).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("step_export_incomplete_after_timeout")
    );
}
#[test]
fn step_export_file_error_fails_without_retry() {
    let files = Files {
        step_error: Some("unsafe_output_path"),
        ..Default::default()
    };
    let error =
        await_step_export(&files, "part.step", Duration::from_secs(1), Duration::ZERO).unwrap_err();
    assert!(error.to_string().contains("unsafe_output_path"));
    assert_eq!(files.step_reads.get(), 0);
}
#[test]
fn invalid_parameters_and_unknown_fields_never_fall_back_to_create() {
    for value in [
        json!({"schema":PARAMS_SCHEMA,"action":"create_typo"}),
        json!({"schema":PARAMS_SCHEMA,"action":"create","shell":"arbitrary"}),
        json!({"schema":PARAMS_SCHEMA,"action":"inspect","parts":["../escape.prt"]}),
        json!({"schema":PARAMS_SCHEMA,"action":"create","width_mm":0.0}),
        json!({"schema":PARAMS_SCHEMA,"action":"import_step","step":"binary.dll"}),
    ] {
        let parsed = serde_json::from_value::<Parameters>(value);
        assert!(parsed.is_err() || parsed.unwrap().validate().is_err());
    }
}
#[test]
fn inspect_is_staged_and_does_not_save() {
    let params: Parameters = serde_json::from_value(
        json!({"schema":PARAMS_SCHEMA,"action":"inspect","parts":["prototype.prt"]}),
    )
    .unwrap();
    let mut api = Api::default();
    let result = execute(&mut api, &mut Files::default(), &params).unwrap();
    assert_eq!(api.calls, 2);
    assert_eq!(result["save_requested"], false);
    assert!(
        execute(
            &mut Api::default(),
            &mut Files {
                deny_input: true,
                ..Default::default()
            },
            &params
        )
        .is_err()
    );
}
#[test]
fn import_wrong_units_is_preserved_as_failed_observation() {
    let params: Parameters = serde_json::from_value(
        json!({"schema":PARAMS_SCHEMA,"action":"import_step","step":"model.step"}),
    )
    .unwrap();
    let mut files = Files::default();
    assert!(
        execute(
            &mut Api {
                wrong_units: true,
                ..Default::default()
            },
            &mut files,
            &params
        )
        .is_err()
    );
    assert_eq!(
        files.events.last().unwrap()["phase"],
        "step_import_native_read"
    );
}

#[test]
fn real_step_body_observation_requires_complete_native_bounds_and_keeps_original_single_solid_gate()
{
    let api = Api {
        length: 55.0,
        width: 20.0,
        height: 10.0,
        ..Default::default()
    };
    let mut part = api.block("task/model.prt");
    part.solid_tags = vec![11, 12];
    part.solid_count = 2;
    let bound = ExactBounds::new(
        [0.0; 3],
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        [55.0, 20.0, 10.0],
    )
    .unwrap();
    part.exact_work_csys_bounds = vec![bound.clone(), bound];
    require_body_observation(&part, "mm", 128).unwrap();
    assert!(require_block(&part, 55.0, 20.0, 10.0).is_err());
    let mut incomplete = part.clone();
    incomplete.exact_work_csys_bounds.pop();
    assert!(require_body_observation(&incomplete, "mm", 128).is_err());
    let mut wrong_units = part.clone();
    wrong_units.units = "inch".into();
    assert!(require_body_observation(&wrong_units, "mm", 128).is_err());
    assert!(require_body_observation(&part, "mm", 1).is_err());
    assert!(require_body_observation(&part, "mm", 0).is_err());
}

#[test]
fn padded_envelope_preserves_exact_vertex_dimensions() {
    let mut api = Api {
        envelope_padding: 0.01,
        ..Default::default()
    };
    let mut files = Files::default();
    let result = execute(&mut api, &mut files, &parameters("create")).unwrap();
    assert_eq!(result["before"]["planar_vertex_bounds"][0][3], 40.0);
    assert_eq!(result["after"]["planar_vertex_bounds"][0][3], 55.0);
    assert_eq!(result["before"]["bbox"][0][0], -0.01);
    assert_eq!(result["after"]["bbox"][0][3], 55.01);
}
fn rejected_before_save(mut api: Api, reason: &str) {
    let mut files = Files::default();
    let error = execute(&mut api, &mut files, &parameters("create")).unwrap_err();
    assert!(error.to_string().contains(reason), "{error:#}");
    assert_eq!(
        api.calls, 2,
        "no save, assembly, or translation after rejected measurement"
    );
    assert_eq!(files.events.len(), 1, "preserve initial native observation");
}
#[test]
fn target_sized_envelope_and_expression_cannot_hide_undersized_shape() {
    rejected_before_save(
        Api {
            vertex_extent_delta: -0.1,
            ..Default::default()
        },
        "native_block_extent_differs_axis_0",
    );
}
#[test]
fn unknown_or_misaligned_planar_vertex_observations_are_rejected() {
    for (fault, reason) in [
        (
            BoundsFault::MissingVertices,
            "native_block_representation_differs",
        ),
        (
            BoundsFault::UnknownPlanarity,
            "native_block_planar_vertices_not_observed",
        ),
        (
            BoundsFault::ExtraVertices,
            "native_block_representation_differs",
        ),
    ] {
        rejected_before_save(
            Api {
                bounds_fault: Some(fault),
                ..Default::default()
            },
            reason,
        );
    }
}
#[test]
fn nonfinite_or_inverted_native_bounds_are_rejected() {
    for fault in [
        BoundsFault::NonfiniteVertex,
        BoundsFault::NonfiniteEnvelope,
        BoundsFault::InvertedVertices,
        BoundsFault::InvertedEnvelope,
    ] {
        rejected_before_save(
            Api {
                bounds_fault: Some(fault),
                ..Default::default()
            },
            "native_block_bounds_invalid_axis_0",
        );
    }
}
#[test]
fn native_envelope_must_contain_observed_vertices() {
    rejected_before_save(
        Api {
            envelope_upper_delta: -0.1,
            ..Default::default()
        },
        "native_block_envelope_excludes_vertices_axis_0",
    );
}

#[test]
fn reference_set_membership_geometry_and_save_are_not_assumed_successful() {
    for fault in [
        "already_exists",
        "wrong_member",
        "extra_member",
        "wrong_name",
        "geometry",
        "save_loses_members",
        "body_identity",
        "save_body_identity",
        "component_representation",
    ] {
        let mut api = Api {
            refset_fault: Some(fault),
            ..Default::default()
        };
        let mut files = Files::default();
        assert!(
            execute(&mut api, &mut files, &parameters("create")).is_err(),
            "{fault}"
        );
    }
}
#[test]
fn ascii_scope_rejects_non_ascii_before_any_native_method() {
    let params: Parameters = serde_json::from_value(
        json!({"schema":PARAMS_SCHEMA,"action":"inspect","parts":["轴.prt"]}),
    )
    .unwrap();
    let mut api = Api::default();
    assert!(execute(&mut api, &mut Files::default(), &params).is_err());
    assert_eq!(api.calls, 0);
}
#[test]
fn reference_set_identity_is_checked_against_this_observations_current_body() {
    let api = Api {
        refset_ready: true,
        ..Default::default()
    };
    let mut info = api.block("part.prt");
    require_controlled_refset(&info).unwrap();
    info.solid_tags = vec![200];
    assert!(require_controlled_refset(&info).is_err());
    info.controlled_reference_set.member_tags = vec![200];
    require_controlled_refset(&info).unwrap();
    info.solid_tags.push(201);
    assert!(require_controlled_refset(&info).is_err());
}

#[test]
fn complete_postsave_observation_survives_member_or_geometry_rejection() {
    for fault in [
        "save_extra_member",
        "save_loses_members",
        "save_bad_geometry",
    ] {
        let mut api = Api {
            refset_fault: Some(fault),
            ..Default::default()
        };
        let mut files = Files::default();
        let error = execute(&mut api, &mut files, &parameters("create")).unwrap_err();
        let saved = files
            .events
            .iter()
            .find(|e| e["phase"] == "controlled_refset_after_save_observed")
            .unwrap();
        assert_eq!(saved["value"]["solid_tags"], json!([100]));
        assert_eq!(saved["value"]["units"], "mm");
        if fault == "save_extra_member" {
            assert_eq!(
                saved["value"]["controlled_reference_set"]["member_tags"],
                json!([100, 999])
            );
            assert_eq!(error.to_string(), "controlled_refset_exact_members_differ");
        }
        assert!(api.components.is_empty());
        assert!(
            !files
                .events
                .iter()
                .any(|e| e["phase"] == "controlled_refset_after_save")
        );
    }
}
#[test]
fn missing_postsave_record_cannot_continue_to_assembly() {
    let mut api = Api::default();
    let mut files = Files {
        deny_after_save_record: true,
        ..Default::default()
    };
    let error = execute(&mut api, &mut files, &parameters("create")).unwrap_err();
    assert_eq!(error.to_string(), "owned_fixture_record_failure");
    assert!(api.components.is_empty());
    assert!(
        !files
            .events
            .iter()
            .any(|e| e["phase"] == "controlled_refset_after_save")
    );
}
