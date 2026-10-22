use oe_contracts::{
    ArtifactReference, CodeArtifactEligibilityPort, CodeArtifactEligibilityRequest,
    ControlledVersionDefinition, VersionSelectionRequest,
};
use oe_core::{SkillRoot, sha256, write_new};
use oe_engineering::{artifacts, registry};
use serde_json::json;
use std::{fs, path::Path};
fn setup() -> (tempfile::TempDir, SkillRoot, CodeArtifactEligibilityRequest) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    fs::create_dir_all(p.join("var/builds")).unwrap();
    let t = tempfile::tempdir_in(p.join("var/builds")).unwrap();
    fs::write(t.path().join("SKILL.md"), "test boundary root").unwrap();
    let r = SkillRoot::new(t.path()).unwrap();
    let src = t.path().join("var/artifact.json");
    fs::create_dir_all(src.parent().unwrap()).unwrap();
    fs::write(
        &src,
        br#"{"passed":true,"eligible":true,"binary_sha256":"unverified","uncovered":[]}"#,
    )
    .unwrap();
    let assurance = t.path().join("var/comparison.json");
    fs::write(&assurance, br#"{"passed":true,"eligibility":"eligible"}"#).unwrap();
    let q = CodeArtifactEligibilityRequest {
        schema: "oev2.code-artifact-eligibility-request/1".into(),
        binding_id: "code".into(),
        revision: "r2".into(),
        scope_id: "development".into(),
        source: artifacts::reference(&r, &src).unwrap(),
        assurance: artifacts::reference(&r, &assurance).unwrap(),
    };
    (t, r, q)
}
#[test]
fn declared_pass_does_not_create_code_artifact_eligibility() {
    let (_t, r, q) = setup();
    let v = registry::CodeArtifactEligibilityExecutor { root: &r }
        .qualify(&q)
        .unwrap();
    assert_eq!(v.execution, "not_run");
    assert_eq!(v.eligibility, "not_connected");
    assert_eq!(v.artifact_sha256, q.source.sha256);
    assert!(!v.uncovered.is_empty());
}
#[test]
fn qualification_reads_bound_bytes_before_reporting_unconnected() {
    let (t, r, q) = setup();
    fs::write(t.path().join(&q.source.path), b"{}").unwrap();
    assert!(
        registry::CodeArtifactEligibilityExecutor { root: &r }
            .qualify(&q)
            .is_err()
    );
    let (_t, r, mut q) = setup();
    q.assurance = ArtifactReference {
        path: q.assurance.path,
        sha256: sha256(b"forged proof"),
    };
    assert!(
        registry::CodeArtifactEligibilityExecutor { root: &r }
            .qualify(&q)
            .is_err()
    );
}
#[test]
fn code_artifact_registration_does_not_connect_activation_or_use() {
    use std::os::unix::fs::PermissionsExt;
    let (t, r, q) = setup();
    let policy = t.path().join(registry::POLICY);
    fs::create_dir_all(policy.parent().unwrap()).unwrap();
    write_new(&policy,&json!({"schema":"oev2.controlled-version-authority/1","bindings":{"code":{"kind":"code_artifact","scope_id":"development","actor_scopes":{"maintainer":["register","activate","restore"]}}}})).unwrap();
    fs::set_permissions(&policy, fs::Permissions::from_mode(0o600)).unwrap();
    let d = ControlledVersionDefinition {
        schema: "oev2.controlled-version-definition/1".into(),
        binding_id: q.binding_id.clone(),
        revision: q.revision.clone(),
        kind: "code_artifact".into(),
        scope_id: q.scope_id.clone(),
        source: q.source.clone(),
        assurance: q.assurance.clone(),
    };
    assert_eq!(
        registry::register(&r, &d, "maintainer").unwrap()["activation"],
        "not_run"
    );
    let s = VersionSelectionRequest {
        schema: "oev2.version-selection-request/1".into(),
        actor: "maintainer".into(),
        binding_id: q.binding_id,
        revision: q.revision,
        reason: "declared comparison is not full eligibility".into(),
        expected_head_sha256: None,
    };
    assert!(
        registry::select(&r, &s, false)
            .unwrap_err()
            .to_string()
            .contains("not_connected")
    );
    assert!(registry::select(&r, &s, true).is_err());
    assert!(registry::active(&r, "code").is_err());
    assert!(registry::use_method(&r, "code", &json!({}), Path::new("var/use")).is_err());
    assert!(!t.path().join("var/use").exists());
    assert!(
        !t.path()
            .join("var/state/controlled-versions")
            .join(sha256("code"))
            .join("events")
            .exists()
    );
}
