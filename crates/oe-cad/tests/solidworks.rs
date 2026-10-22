use oe_cad::solidworks::{capabilities, verify};
use oe_contracts::{ArtifactReference, SolidWorksEvidenceRequest};
use oe_core::{SkillRoot, sha256};
use serde_json::{Value, json};
use std::path::Path;
fn save(p: &Path, v: &Value) -> String {
    let b = serde_json::to_vec_pretty(v).unwrap();
    std::fs::write(p, &b).unwrap();
    sha256(b)
}
fn setup() -> (
    tempfile::TempDir,
    SkillRoot,
    SolidWorksEvidenceRequest,
    Value,
) {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("SKILL.md"), "fixture").unwrap();
    let root = SkillRoot::new(t.path().canonicalize().unwrap()).unwrap();
    let dir = root.path().join("var/cad");
    std::fs::create_dir_all(&dir).unwrap();
    let model = b"synthetic-file-not-a-native-CAD-run";
    std::fs::write(dir.join("part.SLDPRT"), model).unwrap();
    let dep = json!({"repository":"https://github.com/czuryk/SolidworksMCP","commit":"a".repeat(40),"solidworks":"2025 SP5.0","validation":"external declared history only","sha256":{"part.SLDPRT":sha256(model)}});
    let h = save(&dir.join("deployment.json"), &dep);
    let packet = json!({"record_type":"ontology-engineering.cad-evidence/v1","packet_id":"sw-history","project":{"project_id":"study","configuration_id":"default","model_revision":"r1","product_state":"saved"},"sources":[{"id":"deployment","path":"deployment.json","sha256":h,"media_type":"application/json","role":"other"},{"id":"model","path":"part.SLDPRT","sha256":sha256(model),"media_type":"application/octet-stream","role":"native_model"}],"objects":[{"id":"plate","kind":"part","statement":"Historical declared artifact","status":"candidate","identity_scope":"model_revision","basis":"engineering_inference","source_id":"model","source_locator":"whole-file"}],"relations":[],"requirements":[],"assertions":[],"questions":[]});
    let h = save(&dir.join("packet.json"), &packet);
    let r = SolidWorksEvidenceRequest {
        schema: "oev2.solidworks-evidence/1".into(),
        packet: ArtifactReference {
            path: "var/cad/packet.json".into(),
            sha256: h,
        },
        project_root: "var/cad".into(),
        deployment_source_id: "deployment".into(),
        native_model_source_id: "model".into(),
    };
    (t, root, r, packet)
}
#[test]
fn solidworks_external_receipts_keep_native_and_process_unknowns() {
    let (_t, root, r, _) = setup();
    let out = verify(&root, &r).unwrap();
    assert_eq!(out["adapter_kind"], "third_party_python_mcp");
    assert_eq!(out["current_native_execution"], "not_run");
    assert_eq!(out["independent_cad_process_reopen"], "not_established");
    assert_eq!(out["engineering_adoption"], "not_assessed");
    assert_eq!(
        capabilities()["native_worker"],
        "Rust_finite_worker_implemented_requires_current_native_validation"
    );
    assert_eq!(
        capabilities()["native_worker_contract"]["existing_instance_attachment"],
        "prohibited"
    );
    assert_eq!(capabilities()["native_process_reopen"], "not_qualified");
    std::fs::write(root.path().join("var/cad/part.SLDPRT"), b"drift").unwrap();
    assert!(verify(&root, &r).is_err());
}
#[test]
fn deployment_model_claim_mismatch_is_not_native_evidence() {
    let (_t, root, mut r, mut p) = setup();
    let dp = root.path().join("var/cad/deployment.json");
    let mut d: Value = serde_json::from_slice(&std::fs::read(&dp).unwrap()).unwrap();
    d["sha256"]["part.SLDPRT"] = json!("0".repeat(64));
    p["sources"][0]["sha256"] = json!(save(&dp, &d));
    r.packet.sha256 = save(&root.path().join("var/cad/packet.json"), &p);
    assert!(verify(&root, &r).is_err());
}
