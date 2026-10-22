//! Synthetic observations only. Fixed A geometry derives from the preregistered fixture.
use oe_contracts::NxSteppedShaftIntent;
use oe_nx_worker::geometry::{CONTROLLED_REFSET, REPRESENTATION_BASELINE, stepped_shaft};
use serde_json::{Value, json};
fn intent_value() -> Value {
    json!({"schema":"oev2.nx-stepped-shaft-intent/1",
"object":{"object_id":"urn:test:stepped","revision":"r1","condition_scope":"synthetic","source_sha256":"a".repeat(64)},
"feature_id":"shaft","unit":"mm","coordinate_system":"absolute_part_cartesian","origin_mm":[11,-17,3],
"axis_direction":[0,0,1],"segments":[{"diameter_mm":16,"length_mm":12},{"diameter_mm":30,"length_mm":20},{"diameter_mm":20,"length_mm":15}],
"source_role":"independent_software_fixture"})
}
fn intent() -> NxSteppedShaftIntent {
    serde_json::from_value(intent_value()).unwrap()
}
fn metadata(subtype: i64) -> Value {
    json!({"type_query":{"return_code":0,"type_raw":70,"subtype_raw":subtype},"owning_part_query":{"value_valid":true,"matches_expected_owner":true}})
}
fn observation() -> Value {
    let rims = [
        (8.0, 3.0),
        (8.0, 15.0),
        (15.0, 15.0),
        (15.0, 35.0),
        (10.0, 35.0),
        (10.0, 50.0),
    ];
    let entries:Vec<_>=rims.iter().enumerate().map(|(i,(r,z))|json!({"edge_tag":301+i,"body_tag":100,"metadata":metadata(3),
 "query":{"capture_complete":true,"wrapper_code":0,"initialize_code":0,"is_arc_code":0,"arc_code":0,"free_code":0,"is_arc":true},
 "arc":{"value_valid":true,"is_periodic":true,"limits":[0.0,std::f64::consts::TAU],"radius":r,"center":[11.0,-17.0,z],"x_axis":[1,0,0],"y_axis":[0,1,0]}})).collect();
    let face = |tag, kind, radius, z, edges: Vec<u32>| {
        json!({"face_tag":tag,"body_tag":100,"query_code":0,"surface_type":kind,
 "axis_point":[11.0,-17.0,z],"axis_direction":[0,0,1],"radius":radius,"face_body_tag":100,"face_body_query_code":0,
 "face_edges_query_code":0,"face_edges_complete":true,"edge_tags":edges})
    };
    let mut mass = vec![0.0; 47];
    mass[0] = 1542.0 * std::f64::consts::PI / 1e6;
    mass[1] = 21262.29907949572 / 1e9;
    json!({"path":"synthetic.prt","part_tag":77,"representation_baseline":REPRESENTATION_BASELINE,"controlled_jt_mapping":null,
 "units":"mm","solid_count":1,"solid_tags":[100],"sheet_count":0,"components":[],"expressions":[],
 "rotating_body_inventory_complete":true,"rotating_body_inventory_tags":[100],
 "controlled_reference_set":{"name":CONTROLLED_REFSET,"found":true,"reference_set_tag":7,"member_tags":[100]},
 "bbox_role":"synthetic independent envelope","bbox":[[-4,-32,3,26,-2,50]],"planar_vertex_bounds":[null],
 "exact_work_csys_bounds":[{"min_corner":[-4,-32,3],"directions":[[1,0,0],[0,1,0],[0,0,1]],"distances":[30,30,47],"absolute_axis_bounds":[-4,-32,3,26,-2,50]}],
 "solid_face_inventory_complete":true,"solid_face_inventory":[face(201,16,8.0,3.0,vec![301,302]),face(202,16,15.0,15.0,vec![303,304]),face(203,16,10.0,35.0,vec![305,306]),
 face(204,22,0.0,3.0,vec![301]),face(205,22,0.0,15.0,vec![302,303]),face(206,22,0.0,35.0,vec![304,305]),face(207,22,0.0,50.0,vec![306])],
 "model_edges":{"schema":"oev2.nx-model-edge-observation/1","complete":true,"unit":"mm","coordinate_basis":"UF_EVAL ABS coordinates; single-part solid edges",
 "solid_type_raw":70,"edge_subtype_raw":3,"bodies":[{"body_tag":100,"metadata":metadata(0),"edge_list_return_code":0,"capture_complete":true,"edge_tags":[301,302,303,304,305,306]}],"entries":entries},
 "native_volume":{"schema":"oev2.nx-native-solid-mass-properties/1","return_code":0,"body_tags":[100],"object_type":1,"unit_selector":4,"unit":"m3",
 "raw47":mass,"volume_m3":mass[1],"surface_area_m2":mass[0]}})
}
#[test]
fn stepped_complete_synthetic_shape_matches_frozen_intent() {
    let v = stepped_shaft::check(&intent(), &observation()).unwrap();
    assert_eq!(v["segments_checked"], 3);
    assert_eq!(v["manufacturing_or_physical_acceptance"], "not_assessed");
}
#[test]
fn stepped_face_edge_corruption_cannot_hide_behind_correct_mass_and_bounds() {
    let original = observation();
    stepped_shaft::check(&intent(), &original).unwrap();
    let controls: Vec<(&str, Value)> = vec![
        ("/solid_face_inventory/0/edge_tags", json!([303, 304])),
        ("/solid_face_inventory/0/radius", json!(15)),
        ("/solid_face_inventory/0/face_edges_complete", json!(false)),
        ("/solid_face_inventory/0/face_body_tag", json!(999)),
        ("/solid_face_inventory/0/face_body_query_code", json!(5)),
        ("/solid_face_inventory/0/edge_tags", json!([301, 301])),
        ("/solid_face_inventory/0/edge_tags", json!([301, 999])),
        ("/solid_face_inventory/1/face_tag", json!(201)),
        ("/solid_face_inventory/4/edge_tags", json!([302, 304])),
        ("/solid_face_inventory/4/axis_point", json!([11, -17, 35])),
        (
            "/solid_face_inventory/0/axis_direction",
            json!([0.01, 0, 1]),
        ),
        ("/solid_face_inventory/0/surface_type", json!(17)),
        ("/solid_face_inventory_complete", json!(false)),
        ("/rotating_body_inventory_complete", json!(false)),
        ("/sheet_count", json!(1)),
        ("/native_volume/unit_selector", json!(3)),
        ("/native_volume/volume_m3", json!(0.1)),
        ("/model_edges/entries/0/query/free_code", json!(4)),
        (
            "/exact_work_csys_bounds/0/absolute_axis_bounds",
            json!([0, 0, 0, 1, 1, 1]),
        ),
    ];
    for (pointer, value) in controls {
        let mut v = original.clone();
        *v.pointer_mut(pointer).unwrap() = value;
        assert!(stepped_shaft::check(&intent(), &v).is_err(), "{pointer}");
    }
    let mut fewer = original;
    fewer["solid_face_inventory"].as_array_mut().unwrap().pop();
    assert!(stepped_shaft::check(&intent(), &fewer).is_err());
}
#[test]
fn stepped_invalid_intent_and_parameter_paths_refuse() {
    for (key, value) in [
        ("schema", json!("old/1")),
        ("unit", json!("inch")),
        ("axis_direction", json!([0, 0, -1])),
        ("origin_mm", json!([0, 0])),
        ("segments", json!([])),
        ("feature_id", json!("bad;command")),
        ("source_role", json!("native_pass")),
    ] {
        let mut v = intent_value();
        v[key] = value;
        let intent: NxSteppedShaftIntent = serde_json::from_value(v).unwrap();
        assert!(stepped_shaft::validate(&intent).is_err());
    }
    for member in ["inputs/intent.json", "../intent.json", "/intent.json"] {
        let p = oe_nx_worker::geometry::Parameters::CreateSteppedShaft {
            schema: stepped_shaft::PARAMS_SCHEMA.into(),
            intent: oe_contracts::ArtifactReference {
                path: member.into(),
                sha256: "a".repeat(64),
            },
        };
        assert!(p.validate().is_err());
    }
}
struct Inputs {
    path: std::path::PathBuf,
}
impl oe_nx_worker::geometry::Workspace for Inputs {
    fn input(&self, _: &oe_nx_protocol::RelativePath) -> anyhow::Result<std::path::PathBuf> {
        Ok(self.path.clone())
    }
    fn output(&self, _: &str) -> anyhow::Result<std::path::PathBuf> {
        anyhow::bail!("no_mutation")
    }
    fn require_regular_output(&self, _: &str) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("no_output")
    }
    fn optional_regular_output(&self, _: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn record(&mut self, _: &str, _: Value) -> anyhow::Result<()> {
        anyhow::bail!("no_record")
    }
}
#[derive(Default)]
struct NoNative {
    calls: usize,
}
impl oe_nx_worker::geometry::NativeApi for NoNative {
    fn ensure_empty_session(&mut self) -> anyhow::Result<()> {
        self.calls += 1;
        anyhow::bail!("unexpected_native_call")
    }
    fn create_mm_part(&mut self, _: &std::path::Path) -> anyhow::Result<u32> {
        self.calls += 1;
        anyhow::bail!("unexpected_mutation")
    }
    fn open_part(&mut self, _: &std::path::Path) -> anyhow::Result<u32> {
        self.calls += 1;
        anyhow::bail!("unexpected_open")
    }
    fn create_mm_expression(&mut self, _: u32, _: &str) -> anyhow::Result<u32> {
        anyhow::bail!("unused")
    }
    fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> anyhow::Result<u32> {
        anyhow::bail!("unused")
    }
    fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> anyhow::Result<i32> {
        anyhow::bail!("unused")
    }
    fn prepare_controlled_representation(&mut self, _: u32) -> anyhow::Result<Value> {
        anyhow::bail!("unused")
    }
    fn save_work_part(&mut self) -> anyhow::Result<()> {
        anyhow::bail!("unused")
    }
    fn add_component(
        &mut self,
        _: u32,
        _: u32,
        _: &std::path::Path,
        _: &str,
        _: [f64; 3],
    ) -> anyhow::Result<()> {
        anyhow::bail!("unused")
    }
    fn part_info(&mut self, _: u32) -> anyhow::Result<oe_nx_worker::geometry::PartInfo> {
        anyhow::bail!("unused")
    }
    fn export_ap214(&mut self, _: &std::path::Path, _: &std::path::Path) -> anyhow::Result<()> {
        anyhow::bail!("unused")
    }
    fn import_step_new_part(
        &mut self,
        _: &std::path::Path,
        _: &std::path::Path,
    ) -> anyhow::Result<()> {
        anyhow::bail!("unused")
    }
    fn import_parasolid(&mut self, _: &std::path::Path, _: u32) -> anyhow::Result<Vec<u32>> {
        anyhow::bail!("unused")
    }
}

#[test]
fn stepped_source_refusal_precedes_any_native_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(dir.path())
        .unwrap()
        .join("intent.json");
    for bad_hash in [true, false] {
        let mut v = intent_value();
        if !bad_hash {
            v["axis_direction"] = json!([1, 0, 0]);
        }
        let raw = serde_json::to_vec(&v).unwrap();
        std::fs::write(&path, &raw).unwrap();
        let p = oe_nx_worker::geometry::Parameters::CreateSteppedShaft {
            schema: stepped_shaft::PARAMS_SCHEMA.into(),
            intent: oe_contracts::ArtifactReference {
                path: "intent.json".into(),
                sha256: if bad_hash {
                    "b".repeat(64)
                } else {
                    oe_core::sha256(&raw)
                },
            },
        };
        let mut api = NoNative::default();
        let mut w = Inputs { path: path.clone() };
        let error = oe_nx_worker::geometry::execute(&mut api, &mut w, &p)
            .unwrap_err()
            .to_string();
        assert_eq!(api.calls, 0);
        assert_eq!(
            error,
            if bad_hash {
                "stepped_source_changed"
            } else {
                "stepped_frame_invalid"
            }
        );
    }
}

// Recording double verifies orchestration only, never a native execution claim.
struct Recording {
    log: Vec<String>,
    output: Option<std::path::PathBuf>,
    next: u32,
    bad_topology: bool,
}
impl oe_nx_worker::geometry::NativeApi for Recording {
    fn ensure_empty_session(&mut self) -> anyhow::Result<()> {
        self.log.push("empty".into());
        Ok(())
    }
    fn create_mm_part(&mut self, p: &std::path::Path) -> anyhow::Result<u32> {
        self.output = Some(p.into());
        self.log.push("new".into());
        Ok(77)
    }
    fn open_part(&mut self, _: &std::path::Path) -> anyhow::Result<u32> {
        self.log.push("open".into());
        Ok(77)
    }
    fn set_work_part(&mut self, _: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn create_cylinder_solid(&mut self, o: [f64; 3], l: f64, d: f64) -> anyhow::Result<u32> {
        self.next += 1;
        self.log.push(format!("cylinder:{o:?}:{l}:{d}"));
        Ok(self.next)
    }
    fn feature_body(&mut self, f: u32) -> anyhow::Result<u32> {
        Ok(99 + f)
    }
    fn unite_bodies(&mut self, t: u32, u: u32) -> anyhow::Result<()> {
        self.log.push(format!("unite:{t}:{u}"));
        Ok(())
    }
    fn prepare_controlled_representation(&mut self, _: u32) -> anyhow::Result<Value> {
        self.log.push("representation".into());
        Ok(json!({}))
    }
    fn stepped_part_info(&mut self, _: u32) -> anyhow::Result<Value> {
        self.log.push("read".into());
        let mut v = observation();
        if self.bad_topology {
            v["solid_face_inventory"][0]["edge_tags"] = json!([303, 304]);
        }
        Ok(v)
    }
    fn save_work_part(&mut self) -> anyhow::Result<()> {
        self.log.push("save".into());
        std::fs::write(self.output.as_ref().unwrap(), b"synthetic saved part")?;
        Ok(())
    }
    fn create_mm_expression(&mut self, _: u32, _: &str) -> anyhow::Result<u32> {
        anyhow::bail!("unused")
    }
    fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> anyhow::Result<u32> {
        anyhow::bail!("unused")
    }
    fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> anyhow::Result<i32> {
        anyhow::bail!("unused")
    }
    fn add_component(
        &mut self,
        _: u32,
        _: u32,
        _: &std::path::Path,
        _: &str,
        _: [f64; 3],
    ) -> anyhow::Result<()> {
        anyhow::bail!("unused")
    }
    fn part_info(&mut self, _: u32) -> anyhow::Result<oe_nx_worker::geometry::PartInfo> {
        anyhow::bail!("unused")
    }
    fn export_ap214(&mut self, _: &std::path::Path, _: &std::path::Path) -> anyhow::Result<()> {
        anyhow::bail!("unused")
    }
    fn import_step_new_part(
        &mut self,
        _: &std::path::Path,
        _: &std::path::Path,
    ) -> anyhow::Result<()> {
        anyhow::bail!("unused")
    }
    fn import_parasolid(&mut self, _: &std::path::Path, _: u32) -> anyhow::Result<Vec<u32>> {
        anyhow::bail!("unused")
    }
}
struct Files {
    root: std::path::PathBuf,
}
impl oe_nx_worker::geometry::Workspace for Files {
    fn input(&self, p: &oe_nx_protocol::RelativePath) -> anyhow::Result<std::path::PathBuf> {
        Ok(self.root.join(p.as_str().strip_prefix("inputs/").unwrap()))
    }
    fn output(&self, p: &str) -> anyhow::Result<std::path::PathBuf> {
        Ok(self.root.join(p))
    }
    fn require_regular_output(&self, p: &str) -> anyhow::Result<Vec<u8>> {
        Ok(std::fs::read(self.root.join(p))?)
    }
    fn optional_regular_output(&self, _: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn record(&mut self, _: &str, _: Value) -> anyhow::Result<()> {
        Ok(())
    }
}
#[test]
fn stepped_execution_orders_explicit_unions_and_checks_before_save() {
    use oe_nx_worker::geometry::{Parameters, execute};
    for bad in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let raw = serde_json::to_vec(&intent_value()).unwrap();
        std::fs::write(root.join("intent.json"), &raw).unwrap();
        let reference = oe_contracts::ArtifactReference {
            path: "intent.json".into(),
            sha256: oe_core::sha256(&raw),
        };
        let params = Parameters::CreateSteppedShaft {
            schema: stepped_shaft::PARAMS_SCHEMA.into(),
            intent: reference.clone(),
        };
        let mut api = Recording {
            log: vec![],
            output: None,
            next: 0,
            bad_topology: bad,
        };
        let mut files = Files { root: root.clone() };
        let result = execute(&mut api, &mut files, &params);
        assert_eq!(
            &api.log[..7],
            [
                "empty",
                "new",
                "cylinder:[11.0, -17.0, 3.0]:12:16",
                "cylinder:[11.0, -17.0, 15.0]:20:30",
                "unite:100:101",
                "cylinder:[11.0, -17.0, 35.0]:15:20",
                "unite:100:102"
            ]
        );
        if bad {
            assert!(result.is_err());
            assert!(!api.log.iter().any(|s| s == "save"));
            assert!(!root.join("stepped-shaft.prt").exists());
        } else {
            let result = result.unwrap();
            assert_eq!(result["engineering_adoption"], false);
            assert_eq!(&api.log[7..], ["representation", "read", "save", "read"]);
            let saved = std::fs::read(root.join("stepped-shaft.prt")).unwrap();
            let params = Parameters::InspectSteppedShaft {
                schema: stepped_shaft::PARAMS_SCHEMA.into(),
                part: "stepped-shaft.prt".into(),
                intent: reference,
            };
            let mut inspection = Recording {
                log: vec![],
                output: None,
                next: 0,
                bad_topology: false,
            };
            let result = execute(&mut inspection, &mut files, &params).unwrap();
            assert_eq!(result["input_saved"], false);
            assert_eq!(inspection.log, ["empty", "open", "read"]);
            assert_eq!(
                std::fs::read(root.join("stepped-shaft.prt")).unwrap(),
                saved
            );
        }
    }
}
