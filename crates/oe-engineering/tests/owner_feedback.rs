mod common;
use common::{Fixture, protocol_two};
use oe_contracts::{ControlledVersionDefinition, VersionSelectionRequest};
use oe_core::{sha256, write_new};
use oe_engineering::{artifacts, registry};
use oe_kernel::learning;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn setup(id: &str) -> (Fixture, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new(id, "required_semantic_review");
    protocol_two(&mut f);
    let path = f.freeze();
    let pp = f.temp.path().join(registry::POLICY);
    fs::create_dir_all(pp.parent().unwrap()).unwrap();
    write_new(&pp,&json!({"schema":"oev2.controlled-version-authority/1","bindings":{"method":{"kind":"operation_method","scope_id":"private-diagnostic","actor_scopes":{"maintainer":["register","activate","restore"]}}}})).unwrap();
    fs::set_permissions(&pp, fs::Permissions::from_mode(0o600)).unwrap();
    let freeze = f.temp.path().join(&path).join("freeze.json");
    let reference = artifacts::reference(&f.root, &freeze).unwrap();
    registry::register(
        &f.root,
        &ControlledVersionDefinition {
            schema: "oev2.controlled-version-definition/1".into(),
            binding_id: "method".into(),
            revision: "baseline".into(),
            kind: "method_baseline".into(),
            scope_id: "private-diagnostic".into(),
            source: reference.clone(),
            assurance: reference,
        },
        "maintainer",
    )
    .unwrap();
    registry::select(
        &f.root,
        &VersionSelectionRequest {
            schema: "oev2.version-selection-request/1".into(),
            actor: "maintainer".into(),
            binding_id: "method".into(),
            revision: "baseline".into(),
            reason: "baseline use grants no trial acceptance".into(),
            expected_head_sha256: None,
        },
        false,
    )
    .unwrap();
    (f, path)
}
#[test]
fn registered_baseline_appends_source_bound_feedback_to_existing_learning_authority() {
    let (f, _path) = setup("registered-frozen-feedback");
    let r =
        registry::use_method(&f.root, "method", &f.case, Path::new("var/actual-baseline")).unwrap();
    assert_eq!(r["event"]["event"]["event"], "Feedback");
    assert_eq!(r["event"]["event"]["state"], "frozen");
    let event_path = r["event"]["path"].as_str().unwrap();
    assert!(event_path.starts_with("var/state/knowledge/experience-events/"));
    let bytes = fs::read(f.temp.path().join(event_path)).unwrap();
    assert_eq!(sha256(bytes), r["event"]["sha256"]);
    let binding = &r["event"]["event"]["body"]["version_selection"];
    let selection_bytes = fs::read(f.temp.path().join(binding["path"].as_str().unwrap())).unwrap();
    assert_eq!(sha256(&selection_bytes), binding["sha256"]);
    let selection: serde_json::Value = serde_json::from_slice(&selection_bytes).unwrap();
    assert_eq!(selection, r["version_selection"]);
    assert_eq!(r["trial"]["selected_plan"], "baseline-plan.json");
    assert_eq!(r["trial"]["result"]["receipt"]["conclusion"], "unknown");
    assert_eq!(
        learning::list(&f.root, "").unwrap()["results"][0]["state"],
        "frozen"
    );
}
#[test]
fn baseline_feedback_preserves_assessment_without_accepting_candidate() {
    let (f, path) = setup("registered-assessed-feedback");
    let a = learning::assess(&f.root, &path, Path::new("var/assessment.json")).unwrap();
    assert_eq!(a["assessment"]["verdict"], "no_delta");
    let r = registry::use_method(
        &f.root,
        "method",
        &f.case,
        Path::new("var/assessed-baseline"),
    )
    .unwrap();
    assert_eq!(r["event"]["event"]["event"], "Feedback");
    assert_eq!(r["event"]["event"]["state"], "assessed");
    assert!(!r["event"]["event"]["body"]["assessment"].is_null());
    assert!(
        learning::decide(
            &f.root,
            &path,
            "accept-trial",
            "feedback cannot authorize no_delta"
        )
        .is_err()
    );
    learning::decide(
        &f.root,
        &path,
        "reject",
        "original no_delta assessment remains valid",
    )
    .unwrap();
    assert_eq!(
        learning::list(&f.root, "").unwrap()["results"][0]["state"],
        "rejected"
    );
}
