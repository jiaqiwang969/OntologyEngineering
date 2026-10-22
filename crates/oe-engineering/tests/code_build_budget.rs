use oe_contracts::{
    CodeBudget, CodeBudgetedBuildRequest, CodeBuildRequest, CodeBuildSupervisionContext,
    CodeBuildSupervisionPort, CodeBuildSupervisionRequest, CodeRustRuntimeProfile,
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
fn context2_total_budget_exhausted_in_preflight_dispatches_nothing() {
    let (_temp, root, mut context) = fixture();
    context.schema = "oev2.code-build-supervision-context/2".into();
    context.dispatch_budget.timeout_ms = 1;
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    let error = call(&root).unwrap_err().to_string();
    assert!(error.contains("total_budget_exhausted"), "{error}");
    no_dispatch(&root);
}
#[test]
fn context2_dispatches_generated_budgeted_request_and_never_grants_eligibility() {
    let (_temp, root, mut context) = fixture();
    context.schema = "oev2.code-build-supervision-context/2".into();
    context.dispatch_budget.timeout_ms = 2000;
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    let observed = call(&root).unwrap();
    assert_eq!(observed.execution, "failed");
    assert_eq!(observed.compilation_origin, "unknown");
    assert_eq!(observed.release_eligibility, "unknown");
    let observation = artifacts::json(&root, &observed.observation).unwrap();
    assert_eq!(
        observation["budget_observation"]["profile"],
        "owner_supervise_total/2"
    );
    assert_eq!(observation["budget_observation"]["cleanup_reserve_ms"], 250);
    let folder = Path::new(&observed.observation.path).parent().unwrap();
    let bytes = fs::read(root.path().join(folder).join("build-request.json")).unwrap();
    let bounded: CodeBudgetedBuildRequest = serde_json::from_slice(&bytes).unwrap();
    bounded.validate().unwrap();
    assert_eq!(bounded.schema, "oev2.code-budgeted-build-request/1");
    assert_eq!(bounded.build, context.build);
    assert!(bounded.budget.timeout_ms > 0 && bounded.budget.timeout_ms <= 1750);
    assert_eq!(
        bounded.budget.output_bytes,
        context.dispatch_budget.output_bytes
    );
    let reference: oe_contracts::ArtifactReference =
        serde_json::from_value(observation["process"].clone()).unwrap();
    let process = artifacts::json(&root, &reference).unwrap();
    assert_eq!(process["arguments"][0], "build-budgeted");
    assert!(registry::active(&root, "code").is_err());
}
#[test]
fn budgeted_wire_is_explicit_and_not_a_self_certification_protocol() {
    let (_temp, _root, context) = fixture();
    let original = serde_json::to_value(CodeBudgetedBuildRequest {
        schema: "oev2.code-budgeted-build-request/1".into(),
        build: context.build,
        budget: context.dispatch_budget,
    })
    .unwrap();
    let valid: CodeBudgetedBuildRequest = serde_json::from_value(original.clone()).unwrap();
    valid.validate().unwrap();
    for field in [
        "trusted",
        "passed",
        "compilation_origin",
        "release_eligibility",
    ] {
        let mut value = original.clone();
        value[field] = json!(true);
        assert!(serde_json::from_value::<CodeBudgetedBuildRequest>(value).is_err());
    }
    let mut wrong = original.clone();
    wrong["schema"] = json!("oev2.code-build-request/1");
    assert!(
        serde_json::from_value::<CodeBudgetedBuildRequest>(wrong)
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(serde_json::from_value::<CodeBuildRequest>(original).is_err());
}
