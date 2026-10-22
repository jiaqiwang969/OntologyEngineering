mod common;
use common::{Fixture, protocol_two};
use oe_contracts::{ControlledVersionDefinition, VersionSelectionRequest};
use oe_engineering::{artifacts, registry};
use oe_kernel::learning;
use serde_json::json;
use std::{fs, path::Path};
fn policy(f: &Fixture) {
    let p = f.temp.path().join(registry::POLICY);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(
        &p,
        serde_json::to_vec(&json!({"schema":"oev2.controlled-version-authority/1",
        "bindings":{"guide-method":{"kind":"operation_method","scope_id":"mechanism-diagnosis",
            "actor_scopes":{"maintainer":["register","activate","restore"]}}}}))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn request(revision: &str, head: Option<String>) -> VersionSelectionRequest {
    VersionSelectionRequest {
        schema: "oev2.version-selection-request/1".into(),
        actor: "maintainer".into(),
        binding_id: "guide-method".into(),
        revision: revision.into(),
        reason: "bounded observed retrieval change or explicit baseline restoration".into(),
        expected_head_sha256: head,
    }
}
fn definition(f: &Fixture, revision: &str, kind: &str) -> ControlledVersionDefinition {
    let source = artifacts::reference(
        &f.root,
        &f.temp.path().join("var/candidates/candidate/freeze.json"),
    )
    .unwrap();
    let assurance = if kind == "method_baseline" {
        source.clone()
    } else {
        artifacts::reference(&f.root, &f.temp.path().join("var/assessment.json")).unwrap()
    };
    ControlledVersionDefinition {
        schema: "oev2.controlled-version-definition/1".into(),
        binding_id: "guide-method".into(),
        revision: revision.into(),
        kind: kind.into(),
        scope_id: "mechanism-diagnosis".into(),
        source,
        assurance,
    }
}
fn add_retrieval(f: &mut Fixture) {
    let replay = json!({"schema":"oev2.retrieval-pools/1","query":"same guide conditions","context_sha256":"a".repeat(64),"provider":"frozen-test", "index_identity":{"sha256":"b".repeat(64)},"limit":4,
        "pools":[{"label":"broad-context","pages":[{"id":"wrong-1"},{"id":"wrong-2"}]},
            {"label":"query_and_object","pages":[{"id":"necessary"},{"id":"related"}]},
            {"label":"shared-context","pages":[{"id":"common"}]}]});
    let path = "var/inputs/retrieval.json";
    let raw = serde_json::to_vec(&replay).unwrap();
    fs::write(f.temp.path().join(path), &raw).unwrap();
    f.case["retrieval"] = json!({"replay":{"path":path,"sha256":oe_core::sha256(raw)},"inapplicable_page_ids":["wrong-1","wrong-2"],"necessary_page_ids":["necessary"]});
    let mut e = f.read("var/inputs/evaluation-current.json");
    e["cases"] = json!([f.case]);
    let raw = serde_json::to_vec(&e).unwrap();
    fs::write(
        f.temp.path().join("var/inputs/evaluation-retrieval.json"),
        &raw,
    )
    .unwrap();
    f.input["evaluation"] =
        json!({"path":"var/inputs/evaluation-retrieval.json","sha256":oe_core::sha256(raw)});
}
#[test]
fn selected_method_changes_actual_next_task_retrieval_and_can_restore_exact_baseline() {
    let mut f = Fixture::new("registry-gain", "required_semantic_review");
    protocol_two(&mut f);
    add_retrieval(&mut f);
    let path = f.freeze();
    let a = learning::assess(&f.root, &path, Path::new("var/assessment.json")).unwrap();
    assert_eq!(a["assessment"]["verdict"], "admissible_trial");
    learning::decide(
        &f.root,
        &path,
        "accept-trial",
        "independent frozen expectations retain all hard guards",
    )
    .unwrap();
    policy(&f);
    registry::register(
        &f.root,
        &definition(&f, "baseline-1", "method_baseline"),
        "maintainer",
    )
    .unwrap();
    registry::register(
        &f.root,
        &definition(&f, "trial-2", "operation_method"),
        "maintainer",
    )
    .unwrap();
    registry::select(&f.root, &request("baseline-1", None), false).unwrap();
    let baseline = registry::use_method(
        &f.root,
        "guide-method",
        &f.case,
        Path::new("var/next-task/baseline"),
    )
    .unwrap();
    assert_eq!(
        baseline["trial"]["result"]["retrieval"]["inapplicable_selected"],
        2
    );
    let (_, h) = registry::active(&f.root, "guide-method").unwrap();
    assert!(registry::select(&f.root, &request("trial-2", None), false).is_err());
    registry::select(
        &f.root,
        &request("trial-2", Some(h["event_sha256"].as_str().unwrap().into())),
        false,
    )
    .unwrap();
    let trial = registry::use_method(
        &f.root,
        "guide-method",
        &f.case,
        Path::new("var/next-task/trial"),
    )
    .unwrap();
    assert_eq!(
        trial["trial"]["result"]["retrieval"]["inapplicable_selected"],
        1
    );
    assert_eq!(
        trial["trial"]["result"]["retrieval"]["necessary_pages_retained"],
        true
    );
    assert_eq!(trial["trial"]["result"]["receipt"]["conclusion"], "unknown");
    let (_, h) = registry::active(&f.root, "guide-method").unwrap();
    registry::select(
        &f.root,
        &request(
            "baseline-1",
            Some(h["event_sha256"].as_str().unwrap().into()),
        ),
        true,
    )
    .unwrap();
    let restored = registry::use_method(
        &f.root,
        "guide-method",
        &f.case,
        Path::new("var/next-task/restored"),
    )
    .unwrap();
    assert_eq!(
        restored["trial"]["result"]["retrieval"]["inapplicable_selected"],
        2
    );
    assert_eq!(
        restored["trial"]["selected_sha256"],
        baseline["trial"]["selected_sha256"]
    );
}
#[test]
fn no_delta_authority_drift_and_mutated_sources_cannot_activate() {
    let mut f = Fixture::new("registry-no-delta", "required_semantic_review");
    protocol_two(&mut f);
    let path = f.freeze();
    assert_eq!(
        learning::assess(&f.root, &path, Path::new("var/assessment.json")).unwrap()["assessment"]["verdict"],
        "no_delta"
    );
    policy(&f);
    registry::register(
        &f.root,
        &definition(&f, "trial-2", "operation_method"),
        "maintainer",
    )
    .unwrap();
    assert!(registry::select(&f.root, &request("trial-2", None), false).is_err());
    registry::register(
        &f.root,
        &definition(&f, "baseline-1", "method_baseline"),
        "maintainer",
    )
    .unwrap();
    let mut r = request("baseline-1", None);
    r.actor = "self-authorized".into();
    assert!(registry::select(&f.root, &r, false).is_err());
    registry::select(&f.root, &request("baseline-1", None), false).unwrap();
    fs::write(
        f.temp
            .path()
            .join("var/candidates/candidate/baseline-plan.json"),
        b"{}",
    )
    .unwrap();
    assert!(registry::active(&f.root, "guide-method").is_err());
    assert!(!f.temp.path().join("var/failed-use").exists());
}
