//! Software controls; never a native geometry PASS.
use oe_contracts::NxCylinderIntent;
use oe_nx_worker::geometry::rotating_part::{self, validate};
use serde_json::{Value, json};
fn value() -> Value {
    json!({"schema":"oev2.nx-cylinder-intent/1",
    "object":{"object_id":"urn:test:controlled-cylinder","revision":"r1","condition_scope":"fixture-mm-positive-z","source_sha256":"a".repeat(64)},
    "feature_id":"shaft-cylinder","unit":"mm","coordinate_system":"absolute_part_cartesian",
    "origin_mm":[10,-15,7],"axis_direction":[0,0,1],"diameter_mm":24,"length_mm":40,
    "source_role":"independent_software_fixture"})
}
#[test]
fn source_bound_cylinder_intent_and_independent_geometry_controls() {
    let intent: NxCylinderIntent = serde_json::from_value(value()).unwrap();
    validate(&intent).unwrap();
    for (key, replacement) in [
        ("unit", json!("inch")),
        ("schema", json!("old/1")),
        ("axis_direction", json!([0, 0, -1])),
        ("axis_direction", json!([0.0000001, 0, 1])),
        ("diameter_mm", json!(0)),
        ("length_mm", json!(-1)),
        ("origin_mm", json!([10, -15])),
        ("feature_id", json!("bad;expression")),
        ("source_role", json!("native_pass")),
    ] {
        let mut wrong = value();
        wrong[key] = replacement;
        let intent: NxCylinderIntent = serde_json::from_value(wrong).unwrap();
        assert!(validate(&intent).is_err(), "{key}");
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
#[test]
fn source_reference_reads_actual_bytes_and_refuses_coherent_bad_intent() {
    let dir = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(dir.path())
        .unwrap()
        .join("intent.json");
    let raw = serde_json::to_vec(&value()).unwrap();
    std::fs::write(&path, &raw).unwrap();
    let reference = oe_contracts::ArtifactReference {
        path: "intent.json".into(),
        sha256: oe_core::sha256(&raw),
    };
    let input = Inputs { path };
    assert_eq!(rotating_part::source(&input, &reference).unwrap().1, raw);
    let mut wrong = reference.clone();
    wrong.sha256 = "b".repeat(64);
    assert!(rotating_part::source(&input, &wrong).is_err());
    let mut data = value();
    data["axis_direction"] = json!([1, 0, 0]);
    let raw = serde_json::to_vec(&data).unwrap();
    std::fs::write(&input.path, &raw).unwrap();
    wrong.sha256 = oe_core::sha256(&raw);
    assert_eq!(
        rotating_part::source(&input, &wrong)
            .unwrap_err()
            .to_string(),
        "cylinder_supported_frame_mm_positive_z_only"
    );
    wrong.path = "inputs/intent.json".into();
    assert!(rotating_part::source(&input, &wrong).is_err());
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
fn incoherent_or_invalid_intent_refuses_before_any_native_api_call_or_part_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(dir.path())
        .unwrap()
        .join("intent.json");
    for wrong_axis in [false, true] {
        let mut data = value();
        if wrong_axis {
            data["axis_direction"] = json!([1, 0, 0]);
        }
        let raw = serde_json::to_vec(&data).unwrap();
        std::fs::write(&path, &raw).unwrap();
        let mut inputs = Inputs { path: path.clone() };
        let mut native = NoNative::default();
        let params = oe_nx_worker::geometry::Parameters::CreateCylinderSolid {
            schema: rotating_part::PARAMS_SCHEMA.into(),
            intent: oe_contracts::ArtifactReference {
                path: "intent.json".into(),
                sha256: if wrong_axis {
                    oe_core::sha256(&raw)
                } else {
                    "b".repeat(64)
                },
            },
        };
        let error = oe_nx_worker::geometry::execute(&mut native, &mut inputs, &params)
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            if wrong_axis {
                "cylinder_supported_frame_mm_positive_z_only"
            } else {
                "cylinder_intent_bytes_changed"
            }
        );
        assert_eq!(native.calls, 0);
        assert_eq!(std::fs::read(&path).unwrap(), raw);
    }
}
