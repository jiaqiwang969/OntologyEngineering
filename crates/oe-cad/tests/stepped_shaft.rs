//! Local planning only; no worker/SDK/native claims.
use oe_cad::stepped_shaft::plan;
use serde_json::json;
#[test]
fn stepped_plan_binds_actual_source_and_rejects_prefixed_member() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("SKILL.md"), "v2 private fixture").unwrap();
    let root = oe_core::SkillRoot::new(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let path = root.path().join("intent.json");
    let raw=serde_json::to_vec(&json!({"schema":"oev2.nx-stepped-shaft-intent/1",
        "object":{"object_id":"urn:test:cylinder","revision":"r1","condition_scope":"scope","source_sha256":"a".repeat(64)},
        "feature_id":"shaft","unit":"mm","coordinate_system":"absolute_part_cartesian","origin_mm":[10,-15,7],
        "axis_direction":[0,0,1],"segments":[{"diameter_mm":24,"length_mm":40},{"diameter_mm":12,"length_mm":20}],"source_role":"independent_software_fixture"})).unwrap();
    std::fs::write(&path, &raw).unwrap();
    let result = plan(&root, &path, "intent.json").unwrap();
    assert_eq!(
        result["parameters"]["intent"]["sha256"],
        oe_core::sha256(&raw)
    );
    assert_eq!(result["parameters"]["intent"]["path"], "intent.json");
    assert_eq!(result["input_binding"]["member"], "inputs/intent.json");
    assert_eq!(result["native_execution"], "not_run");
    assert_eq!(result["engineering_adoption"], false);
    assert_eq!(std::fs::read(&path).unwrap(), raw);
    assert!(plan(&root, &path, "inputs/intent.json").is_err());
}
