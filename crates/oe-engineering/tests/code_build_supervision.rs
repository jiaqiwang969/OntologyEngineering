use oe_contracts::{
    CodeBudget, CodeBuildRequest, CodeBuildSupervisionContext, CodeBuildSupervisionPort,
    CodeBuildSupervisionRequest, CodeRustRuntimeProfile,
};
use oe_core::{SkillRoot, sha256, write_new};
use oe_engineering::{artifacts, registry};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn fixture() -> (tempfile::TempDir, SkillRoot, CodeBuildSupervisionContext) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    fs::create_dir_all(workspace.join("var/builds")).unwrap();
    let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
    fs::write(
        temp.path().join("SKILL.md"),
        "private supervised build negative fixture",
    )
    .unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[workspace]\n[workspace.package]\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let root = SkillRoot::new(temp.path()).unwrap();
    let inputs = root.path().join("var/input");
    fs::create_dir_all(&inputs).unwrap();
    write_new(
        &inputs.join("manifest.json"),
        &oe_evolution::public_manifest(root.path()).unwrap(),
    )
    .unwrap();
    write_new(
        &inputs.join("stages.json"),
        &json!({"schema":"oev2.rust-runtime-stages/1","stages":[
        "build-normalizer","actual-ggen","build-runner","actual-ggen-check","runner-identity"]}),
    )
    .unwrap();
    write_new(
        &inputs.join("network.json"),
        &json!({"schema":"oev2.rust-runtime-network/1",
        "cargo":"locked_offline","model_assets":"no_fetch","providers":"not_run"}),
    )
    .unwrap();
    let profile = CodeRustRuntimeProfile {
        schema: "oev2.rust-runtime-profile/1".into(),
        profile: oe_evolution::runtime_identity::PROFILE.into(),
        language: "rust".into(),
        source_kind: "frozen_export".into(),
        expected_git_commit: None,
        expected_package_version: "0.1.0".into(),
        expected_target: "native-negative-fixture".into(),
        git: None,
        source_manifest: artifacts::reference(&root, &inputs.join("manifest.json")).unwrap(),
        network_policy: artifacts::reference(&root, &inputs.join("network.json")).unwrap(),
        stage_expectations: artifacts::reference(&root, &inputs.join("stages.json")).unwrap(),
        expected_build_capture: None,
    };
    write_new(
        &inputs.join("profile.json"),
        &serde_json::to_value(profile).unwrap(),
    )
    .unwrap();
    // A real, explicitly selected exit-1 executable exercises process failure.
    // It cannot stand in for a successful Rust builder or source-origin proof.
    let pin = oe_evolution::binary_reference(Path::new("/usr/bin/false"), root.path()).unwrap();
    let path = |leaf: &str| root.path().join(leaf).to_str().unwrap().to_owned();
    let context = CodeBuildSupervisionContext {
        schema: "oev2.code-build-supervision-context/1".into(),
        purpose: "observe_compilation_origin".into(),
        binding_id: "code".into(),
        revision: "r2".into(),
        scope_id: "semantica.full-library/1".into(),
        builder: pin.clone(),
        expected_tools: vec![pin.clone(), pin.clone(), pin.clone()],
        build: CodeBuildRequest {
            artifact_id: "negative-only".into(),
            cargo_home: path("var/cargo-home"),
            cargo_path: pin.path.clone(),
            rustc_path: pin.path.clone(),
            ggen_path: pin.path,
            source_root: root.path().to_str().unwrap().into(),
            platform: "native-negative-fixture".into(),
            target_dir: path("var/target"),
            evidence_path: "var/actual-build".into(),
        },
        source_profile: artifacts::reference(&root, &inputs.join("profile.json")).unwrap(),
        dispatch_budget: CodeBudget {
            timeout_ms: 10_000,
            output_bytes: 1024 * 1024,
        },
    };
    (temp, root, context)
}
fn request() -> CodeBuildSupervisionRequest {
    CodeBuildSupervisionRequest {
        schema: "oev2.code-build-supervision-request/1".into(),
        binding_id: "code".into(),
        revision: "r2".into(),
        scope_id: "semantica.full-library/1".into(),
        actor: "maintainer".into(),
    }
}
fn policy(root: &SkillRoot, schema: &str, context: Option<Value>, actions: &[&str]) {
    let mut binding = json!({"kind":"code_artifact","scope_id":"semantica.full-library/1",
        "actor_scopes":{"maintainer":actions}});
    if let Some(context) = context {
        binding["code_build_supervision"] = context;
    }
    let file = root.path().join(registry::POLICY);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(
        &file,
        serde_json::to_vec(&json!({"schema":schema,"bindings":{"code":binding}})).unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn set_context(root: &SkillRoot, context: &CodeBuildSupervisionContext) -> Value {
    let file = root.path().join("var/input/context.json");
    fs::write(&file, serde_json::to_vec(context).unwrap()).unwrap();
    serde_json::to_value(artifacts::reference(root, &file).unwrap()).unwrap()
}
fn no_dispatch(root: &SkillRoot) {
    assert!(!root.path().join("var/state/controlled-versions").exists());
    assert!(!root.path().join("var/actual-build").exists());
}
fn call(root: &SkillRoot) -> anyhow::Result<oe_contracts::CodeBuildSupervisionResult> {
    oe_engineering::code_build_supervision::supervise(root, &request())
}

#[test]
fn legacy_policy_does_not_gain_build_permissions() {
    let (_temp, root, context) = fixture();
    let reference = set_context(&root, &context);
    for schema in [
        "oev2.controlled-version-authority/1",
        "oev2.controlled-version-authority/2",
    ] {
        for supplied in [reference.clone(), Value::Null] {
            policy(&root, schema, Some(supplied), &["supervise_build"]);
            assert!(
                call(&root)
                    .unwrap_err()
                    .to_string()
                    .contains("explicit_policy3")
            );
            no_dispatch(&root);
        }
    }
}

#[test]
fn actor_scope_and_context_are_required_before_dispatch() {
    let (_temp, root, context) = fixture();
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference.clone()),
        &["register"],
    );
    assert!(
        call(&root)
            .unwrap_err()
            .to_string()
            .contains("actor_not_authorized")
    );
    no_dispatch(&root);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        None,
        &["supervise_build"],
    );
    assert!(
        call(&root)
            .unwrap_err()
            .to_string()
            .contains("context_missing")
    );
    no_dispatch(&root);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    let mut other = request();
    other.scope_id = "other-scope".into();
    assert!(
        oe_engineering::code_build_supervision::supervise(&root, &other)
            .unwrap_err()
            .to_string()
            .contains("scope_changed")
    );
    other = request();
    other.revision = "other-revision".into();
    assert!(
        oe_engineering::code_build_supervision::supervise(&root, &other)
            .unwrap_err()
            .to_string()
            .contains("binding_changed")
    );
    no_dispatch(&root);
}

#[test]
fn source_or_tool_drift_prevents_dispatch() {
    let (_temp, root, mut context) = fixture();
    context.expected_tools[0].sha256 = "0".repeat(64);
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    assert!(
        call(&root)
            .unwrap_err()
            .to_string()
            .contains("executable_changed")
    );
    no_dispatch(&root);
    fs::write(root.path().join("SKILL.md"), "changed public source").unwrap();
    assert!(
        call(&root)
            .unwrap_err()
            .to_string()
            .contains("source_manifest_mismatch")
    );
    no_dispatch(&root);
}

#[test]
fn failed_actual_builder_retains_unknown_and_unselected() {
    let (_temp, root, context) = fixture();
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    let result = registry::CodeBuildSupervisionExecutor { root: &root }
        .supervise(&request())
        .unwrap();
    assert_eq!(result.execution, "failed");
    assert_eq!(result.compilation_origin, "unknown");
    assert_eq!(result.release_eligibility, "unknown");
    assert!(result.artifact.is_none() && result.capture.is_none());
    let observed = artifacts::json(&root, &result.observation).unwrap();
    let process: oe_contracts::ArtifactReference =
        serde_json::from_value(observed["process"].clone()).unwrap();
    let process = artifacts::json(&root, &process).unwrap();
    assert_eq!(process["status"], "process_failed");
    assert_eq!(process["exit_code"], 1);
    assert_eq!(process["capture_complete"], true);
    assert_eq!(process["arguments"][0], "build");
    assert_eq!(observed["selection"], "not_run");
    assert_eq!(observed["release_eligibility"], "unknown");
    assert!(registry::active(&root, "code").is_err());
    assert!(
        !root
            .path()
            .join("var/state/controlled-versions")
            .join(sha256("code"))
            .join("events")
            .exists()
    );
    assert!(!root.path().join("var/actual-build").exists());
}

#[test]
fn original_request_rejects_self_certification() {
    let original = serde_json::to_value(request()).unwrap();
    for key in [
        "passed",
        "trusted",
        "capture",
        "builder",
        "release_eligibility",
    ] {
        let mut value = original.clone();
        value[key] = json!(true);
        assert!(serde_json::from_value::<CodeBuildSupervisionRequest>(value).is_err());
    }
}
