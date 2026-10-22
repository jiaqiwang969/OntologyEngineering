//! Explicit full3 same owner entry's independent denial boundary. These cases do not model a
//! successful Rust build or inherit qualification from the old owner tests.
use oe_contracts::{
    CodeBudget, CodeBuildRequest, CodeBuildSupervisionContext, CodeBuildSupervisionRequest,
    CodeNativeBuildSupervisionRequest3,
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
        "native owner independent denial fixture",
    )
    .unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[workspace]\n[workspace.package]\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let root = SkillRoot::new(temp.path()).unwrap();
    fs::create_dir_all(root.path().join("var/input")).unwrap();
    // Deliberately original profile bytes: the new entry must reject them.
    write_new(
        &root.path().join("var/input/profile.json"),
        &json!({
            "schema":"oev2.rust-runtime-profile/1",
            "profile":"semantica.rust-runtime-source-bound/1",
            "language":"rust"
        }),
    )
    .unwrap();
    let pin = oe_evolution::binary_reference(Path::new("/usr/bin/false"), root.path()).unwrap();
    let path = |leaf: &str| root.path().join(leaf).to_str().unwrap().to_owned();
    let context = CodeBuildSupervisionContext {
        schema: "oev2.code-build-supervision-context/2".into(),
        purpose: "observe_compilation_origin".into(),
        binding_id: "code".into(),
        revision: "r-native".into(),
        scope_id: "semantica.full-library/1".into(),
        builder: pin.clone(),
        expected_tools: vec![pin.clone(), pin.clone(), pin.clone()],
        build: CodeBuildRequest {
            artifact_id: "negative-native-only".into(),
            cargo_home: path("var/cargo-home"),
            cargo_path: pin.path.clone(),
            rustc_path: pin.path.clone(),
            ggen_path: pin.path,
            source_root: root.path().to_str().unwrap().into(),
            platform: "aarch64-apple-darwin".into(),
            target_dir: path("var/target"),
            evidence_path: "var/actual-build".into(),
        },
        source_profile: artifacts::reference(&root, &root.path().join("var/input/profile.json"))
            .unwrap(),
        dispatch_budget: CodeBudget {
            timeout_ms: 10_000,
            output_bytes: 1024 * 1024,
        },
    };
    (temp, root, context)
}
fn request() -> CodeNativeBuildSupervisionRequest3 {
    CodeNativeBuildSupervisionRequest3 {
        schema: "oev2.native-build-supervision-request/3".into(),
        owner_request: CodeBuildSupervisionRequest {
            schema: "oev2.code-build-supervision-request/1".into(),
            binding_id: "code".into(),
            revision: "r-native".into(),
            scope_id: "semantica.full-library/1".into(),
            actor: "maintainer".into(),
        },
    }
}
fn policy(root: &SkillRoot, schema: &str, context: Option<Value>, actions: &[&str]) {
    let mut binding = json!({"kind":"code_artifact", "scope_id":"semantica.full-library/1",
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
    assert!(!root.path().join("var/target").exists());
    assert!(!root.path().join("var/target-native-stage").exists());
}
fn call(root: &SkillRoot) -> anyhow::Result<oe_contracts::CodeNativeBuildSupervisionResult3> {
    oe_engineering::code_build_supervision::supervise_full_native(root, &request())
}

fn keep_failed_dispatch_fixture(
    temp: tempfile::TempDir,
    root: &SkillRoot,
    result: &oe_contracts::CodeNativeBuildSupervisionResult3,
    control: &str,
) {
    let record = root.path().join("var/owner3-failure-fixture-record.json");
    write_new(&record,&json!({"schema":"private.owner3-failed-dispatch-fixture/1","control":control,"root":root.path(),"result":result,"qualification":"actual denied/failed dispatch only; no native build success"})).unwrap();
    let retained = temp.keep();
    assert_eq!(retained, root.path());
    println!("owner3_failed_dispatch_fixture={}", record.display());
}

// Synthetic recipe files are NEVER executed. Only the explicitly pinned false
// or shell builder below executes, exercising failure and budget propagation.
// Content acceptance of these files is not native-tool availability/provenance.
fn private_json(root: &SkillRoot, leaf: &str, value: &Value) -> oe_contracts::ArtifactReference {
    let file = root.path().join("var/input").join(leaf);
    fs::write(&file, serde_json::to_vec(value).unwrap()).unwrap();
    artifacts::reference(root, &file).unwrap()
}
fn fixture_file(root: &SkillRoot, leaf: &str, mode: u32) -> Value {
    let file = root.path().join("var/input").join(leaf);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let bytes = format!("nonexecuted native content fixture: {leaf}\n");
    fs::write(&file, &bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(mode)).unwrap();
    }
    json!({"path":file,"sha256":sha256(bytes.as_bytes()),"bytes":bytes.len(),"mode":mode})
}
fn full3_fixture(
    builder_script: Option<&[u8]>,
) -> (tempfile::TempDir, SkillRoot, CodeBuildSupervisionContext) {
    let (temp, root, mut context) = fixture();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers=[\"crates/oe-design\",\"crates/oe-code-runner\"]\n[workspace.package]\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("crates/oe-code-runner/src")).unwrap();
    fs::write(
        root.path().join("crates/oe-code-runner/Cargo.toml"),
        "[package]\nname=\"oe-code-runner\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        root.path().join("crates/oe-code-runner/src/main.rs"),
        "fn main() { let value: i32 = 1; }\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("crates/oe-design/src")).unwrap();
    fs::write(
        root.path().join("crates/oe-design/Cargo.toml"),
        "[package]\nname=\"oe-design\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        root.path().join("crates/oe-design/src/main.rs"),
        "fn main() { let value: i32 = 1; }\n",
    )
    .unwrap();
    let manifest = private_json(
        &root,
        "manifest.json",
        &oe_evolution::public_manifest(root.path()).unwrap(),
    );
    let stages = private_json(
        &root,
        "stages.json",
        &json!({"schema":"oev2.rust-runtime-stages/1",
        "stages":["build-normalizer","actual-ggen","build-runner","actual-ggen-check","runner-identity"]}),
    );
    let network = private_json(
        &root,
        "network.json",
        &json!({"schema":"oev2.rust-runtime-network/1",
        "cargo":"locked_offline","model_assets":"no_fetch","providers":"not_run"}),
    );
    let prior = private_json(
        &root,
        "prior.json",
        &json!({"schema":"oev2.rust-runtime-profile/1",
        "profile":oe_evolution::runtime_identity::PROFILE,"language":"rust","source_kind":"frozen_export",
        "expected_git_commit":null,"expected_package_version":"0.1.0","expected_target":"aarch64-apple-darwin",
        "git":null,"source_manifest":manifest,"network_policy":network,"stage_expectations":stages,"expected_build_capture":null}),
    );
    let roles = [
        "linker_adapter",
        "clang_driver",
        "native_linker",
        "archive_reader",
        "debug_mapper",
        "debug_linker",
        "dwarf_reader",
        "signature_verifier",
    ];
    let tools: Vec<Value> = roles
        .iter()
        .map(|r| {
            json!({"role":r,
        "file":fixture_file(&root,&format!("tools/{r}"),0o700),"version_observation":null})
        })
        .collect();
    let flags = format!(
        "-C\u{1f}linker={}",
        tools[0]["file"]["path"].as_str().unwrap()
    );
    let sdk_path = std::env::var("OE_NATIVE_ACTUAL_SDK_LAYOUT")
        .expect("explicit fixed SDK3 input required for content only");
    let sdk: Value = serde_json::from_slice(&fs::read(sdk_path).unwrap()).unwrap();
    let sdk = private_json(&root, "sdk-layout3.json", &sdk);
    let stdlib = fixture_file(
        &root,
        "toolchain/lib/rustlib/aarch64-apple-darwin/lib/libstd-fixture.rlib",
        0o600,
    );
    let cargo_file = root.path().join("Cargo.toml");
    let recipe = private_json(
        &root,
        "recipe.json",
        &json!({"schema":"oev2.native-build-recipe/3",
        "algorithm":"darwin-archive-response/1","input_domain":"own_objects_own_regular_archives_external_toolchain_archives_gnu_response",
        "target":"aarch64-apple-darwin","raw_encoded_rustflags":flags,
        "raw_encoded_rustflags_sha256":sha256(flags.as_bytes()),
        "debug":{"cargo_manifest":{"path":cargo_file,"sha256":sha256(fs::read(&cargo_file).unwrap())},
            "cargo_profile":"dev","debug":2,"split_debuginfo":"unpacked","strip":"none","codegen_units":256,
            "incremental":true,"lto":false,"scope_packages":["oe-design","oe-code-runner"]},
        "tools":tools,"native_parameter_preset":"darwin-reproducible-prefix-sdk/1",
        "sdk_layout":sdk,"external_archive_bindings":[stdlib]}),
    );
    let source_file = root.path().join("crates/oe-code-runner/src/main.rs");
    let source = json!({"path":source_file,"sha256":sha256(fs::read(&source_file).unwrap())});
    let checks: Vec<Value> = ["source_line","function","type_member","parameter","local"]
        .iter().enumerate().map(|(i,k)| json!({"check_id":format!("E{i}"),"kind":k,"source":source,
        "function":"fixture::main","line":1,"name":"value","expected_type":if ["type_member","parameter","local"].contains(k){json!("i32")}else{Value::Null},
        "matcher":"exact"})).collect();
    let expected = private_json(
        &root,
        "expectations.json",
        &json!({"schema":"oev2.native-debug-expectations/2",
        "scope":"content predicates for FAILED builder only; no actual DWARF claim", "checks":checks,
        "all_required_oso":true,"signature_required":true}),
    );
    let mut normalizer = serde_json::from_slice::<Value>(
        &fs::read(root.expand(Path::new(&expected.path)).unwrap()).unwrap(),
    )
    .unwrap();
    let normalizer_source = root.path().join("crates/oe-design/src/main.rs");
    for check in normalizer["checks"].as_array_mut().unwrap() {
        check["source"] = json!({"path":normalizer_source,"sha256":sha256(fs::read(&normalizer_source).unwrap())});
    }
    let normalizer = private_json(&root, "normalizer-expectations.json", &normalizer);
    let expected = private_json(
        &root,
        "debug-image-bundle3.json",
        &json!({"schema":"oev2.native-debug-expectations-bundle/3","scope":"nonexecuted predicates for failed owner dispatch only","images":[{"image_role":"normalizer","package":"oe-design","expectations":normalizer},{"image_role":"runner","package":"oe-code-runner","expectations":expected}]}),
    );
    context.source_profile = private_json(
        &root,
        "profile.json",
        &json!({"schema":"oev2.native-runtime-profile/3",
        "profile":"semantica.rust-runtime-native-debug/3","baseline_profile":prior,"recipe":recipe,"debug_expectations":expected}),
    );
    context.expected_tools = ["cargo", "rustc", "ggen"]
        .iter()
        .map(|n| {
            let file = fixture_file(&root, &format!("toolchain/bin/{n}"), 0o700);
            oe_evolution::binary_reference(Path::new(file["path"].as_str().unwrap()), root.path())
                .unwrap()
        })
        .collect();
    context.build.cargo_path = context.expected_tools[0].path.clone();
    context.build.rustc_path = context.expected_tools[1].path.clone();
    context.build.ggen_path = context.expected_tools[2].path.clone();
    context.builder = if let Some(script) = builder_script {
        let file = root.path().join("var/input/fake-builder");
        fs::write(&file, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
        }
        oe_evolution::binary_reference(&file, root.path()).unwrap()
    } else {
        oe_evolution::binary_reference(Path::new("/usr/bin/false"), root.path()).unwrap()
    };
    (temp, root, context)
}

#[test]
fn full3_entry_does_not_upgrade_legacy_authority() {
    let (_temp, root, context) = fixture();
    let reference = set_context(&root, &context);
    for schema in [
        "oev2.controlled-version-authority/1",
        "oev2.controlled-version-authority/2",
    ] {
        policy(&root, schema, Some(reference.clone()), &["supervise_build"]);
        assert!(
            call(&root)
                .unwrap_err()
                .to_string()
                .contains("explicit_policy3")
        );
        no_dispatch(&root);
    }
}
#[test]
fn full3_entry_requires_same_actor_action_and_context_slot() {
    let (_temp, root, context) = fixture();
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
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
}
#[test]
fn full3_entry_cannot_change_approved_scope_or_revision() {
    let (_temp, root, context) = fixture();
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    let mut input = request();
    input.owner_request.scope_id = "other".into();
    assert!(
        oe_engineering::code_build_supervision::supervise_full_native(&root, &input)
            .unwrap_err()
            .to_string()
            .contains("scope_changed")
    );
    input = request();
    input.owner_request.revision = "other".into();
    assert!(
        oe_engineering::code_build_supervision::supervise_full_native(&root, &input)
            .unwrap_err()
            .to_string()
            .contains("binding_changed")
    );
    no_dispatch(&root);
}
#[test]
fn full3_entry_rejects_dispatch_only_context_before_profile_read() {
    let (_temp, root, mut context) = fixture();
    context.schema = "oev2.code-build-supervision-context/1".into();
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
            .contains("requires_total_budget_context2")
    );
    no_dispatch(&root);
}
#[test]
fn full3_entry_rejects_original_profile_bytes_and_changed_reference() {
    let (_temp, root, mut context) = fixture();
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    assert!(call(&root).is_err());
    no_dispatch(&root);
    context.source_profile.sha256 = sha256("not the selected profile bytes");
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    assert!(call(&root).is_err());
    no_dispatch(&root);
}
#[test]
fn caller_full3_json_cannot_certify_or_choose_execution_inputs() {
    for key in [
        "source_profile",
        "recipe",
        "side_context",
        "target_dir",
        "budget",
        "capture",
        "builder",
        "compilation_origin",
        "release_eligibility",
        "owner_context",
    ] {
        let mut value = serde_json::to_value(request()).unwrap();
        value[key] = json!(true);
        assert!(serde_json::from_value::<CodeNativeBuildSupervisionRequest3>(value).is_err());
    }
    for key in [
        "passed",
        "trusted",
        "capture",
        "builder",
        "release_eligibility",
    ] {
        let mut value = serde_json::to_value(request()).unwrap();
        value["owner_request"][key] = json!(true);
        assert!(serde_json::from_value::<CodeNativeBuildSupervisionRequest3>(value).is_err());
    }
}
#[test]
fn full3_legacy_and_cross_version_requests_deny() {
    let (_temp, root, context) = fixture();
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    let mut input = request();
    input.schema = "oev2.native-build-supervision-request/1".into();
    assert!(oe_engineering::code_build_supervision::supervise_full_native(&root, &input).is_err());
    input = request();
    input.owner_request.schema = "oev2.code-build-supervision-request/2".into();
    assert!(oe_engineering::code_build_supervision::supervise_full_native(&root, &input).is_err());
    no_dispatch(&root);
}

#[test]
fn full3_total_budget_exhaustion_prevents_dispatch_and_owner_creation() {
    let (_temp, root, mut context) = full3_fixture(None);
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
fn full3_context_policy_actor_fresh_budget_deny() {
    for leaf in ["var/target", "var/target-native-stage", "var/actual-build"] {
        let (_temp, root, context) = full3_fixture(None);
        fs::create_dir_all(root.path().join(leaf)).unwrap();
        let reference = set_context(&root, &context);
        policy(
            &root,
            "oev2.controlled-version-authority/3",
            Some(reference),
            &["supervise_build"],
        );
        let error = call(&root).unwrap_err().to_string();
        assert!(error.contains("not_fresh"), "{leaf}: {error}");
        assert!(!root.path().join("var/state/controlled-versions").exists());
    }
}
#[test]
fn full3_false_and_forged_shell_keep_unknown() {
    let (temp, root, mut context) = full3_fixture(None);
    context.dispatch_budget.timeout_ms = 2000;
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference.clone()),
        &["supervise_build"],
    );
    let result = call(&root).unwrap();
    assert_eq!(result.execution, "failed");
    assert_eq!(result.compilation_origin, "unknown");
    assert_eq!(result.release_eligibility, "unknown");
    assert!(result.artifact.is_none() && result.capture.is_none());
    let observation = artifacts::json(&root, &result.observation).unwrap();
    let process: oe_contracts::ArtifactReference =
        serde_json::from_value(observation["process"].clone()).unwrap();
    let process = artifacts::json(&root, &process).unwrap();
    assert_eq!(process["exit_code"], 1);
    assert_eq!(process["capture_complete"], true);
    assert_eq!(process["arguments"][0], "build-full-native-budgeted");
    let folder = root
        .path()
        .join(Path::new(&result.observation.path).parent().unwrap());
    let bounded: oe_contracts::CodeNativeBudgetedBuildRequest3 =
        serde_json::from_slice(&fs::read(folder.join("native-build-request.json")).unwrap())
            .unwrap();
    assert_eq!(bounded.build, context.build);
    assert!(bounded.budget.timeout_ms > 0 && bounded.budget.timeout_ms <= 1750);
    assert_eq!(
        bounded.budget.output_bytes,
        context.dispatch_budget.output_bytes
    );
    let side = oe_evolution::native_profile3::load_side(&root, &bounded.side_context).unwrap();
    assert_eq!(side.body_budget, bounded.budget);
    assert_eq!(side.source_profile, context.source_profile);
    assert_eq!(serde_json::to_value(side.owner_context).unwrap(), reference);
    assert_eq!(
        side.stage_root,
        root.path()
            .join("var/target-native-stage")
            .to_str()
            .unwrap()
    );
    assert_eq!(side.logical_debug_root, "oe-native-debug");
    assert_eq!(observation["selection"], "not_run");
    assert_eq!(observation["budget_observation"]["cleanup_reserve_ms"], 250);
    assert!(registry::active(&root, "code").is_err());
    assert!(
        !root
            .path()
            .join("var/state/controlled-versions")
            .join(sha256("code"))
            .join("events")
            .exists()
    );
    assert!(!root.path().join("var/target").exists());
    keep_failed_dispatch_fixture(temp, &root, &result, "false-exit1");
}
#[test]
fn full3_exit_zero_self_declared_capture_does_not_establish_origin() {
    for schema in ["oev2.native-build-capture/2", "oev2.native-build-capture/3"] {
        let shell = format!(
            "#!/bin/sh\nprintf '%s\\n' '{{\"schema\":\"{schema}\",\"compilation_origin\":\"owner_observed\"}}'\n"
        );
        let (temp, root, context) = full3_fixture(Some(shell.as_bytes()));
        let reference = set_context(&root, &context);
        policy(
            &root,
            "oev2.controlled-version-authority/3",
            Some(reference),
            &["supervise_build"],
        );
        let result = call(&root).unwrap();
        assert_eq!(result.execution, "failed");
        assert_eq!(result.compilation_origin, "unknown");
        assert_eq!(result.release_eligibility, "unknown");
        assert!(result.artifact.is_none() && result.capture.is_none());
        let observation = artifacts::json(&root, &result.observation).unwrap();
        let process: oe_contracts::ArtifactReference =
            serde_json::from_value(observation["process"].clone()).unwrap();
        assert_eq!(artifacts::json(&root, &process).unwrap()["exit_code"], 0);
        assert!(registry::active(&root, "code").is_err());
        assert!(!root.path().join("var/actual-build").exists());
        keep_failed_dispatch_fixture(temp, &root, &result, schema);
    }
}

#[test]
fn full3_uses_original_lock_without_dispatch() {
    let (_temp, root, context) = full3_fixture(None);
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    let owner = root
        .path()
        .join("var/state/controlled-versions")
        .join(sha256("code"));
    fs::create_dir_all(&owner).unwrap();
    let held = oe_core::AdvisoryFileLock::acquire(&owner.join("selection.lock"), true).unwrap();
    let error = call(&root).unwrap_err().to_string();
    assert!(error.contains("resource_busy"), "{error}");
    assert!(!owner.join("build-observations").exists());
    assert!(!root.path().join("var/target").exists());
    assert!(!root.path().join("var/target-native-stage").exists());
    assert!(!root.path().join("var/actual-build").exists());
    held.ensure_current().unwrap();
    drop(held);
    // The original lock inode remains the shared coordination point.
    let reacquired =
        oe_core::AdvisoryFileLock::acquire(&owner.join("selection.lock"), true).unwrap();
    reacquired.ensure_current().unwrap();
}

#[test]
fn full3_entry_cannot_project_profile2_to_pass_content_guards() {
    let (_temp, root, mut context) = full3_fixture(None);
    let mut v: Value = serde_json::from_slice(
        &fs::read(
            root.expand(Path::new(&context.source_profile.path))
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    v["schema"] = json!("oev2.native-runtime-profile/2");
    v["profile"] = json!("semantica.rust-runtime-native-debug/2");
    context.source_profile = private_json(&root, "profile.json", &v);
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["supervise_build"],
    );
    assert!(call(&root).is_err());
    no_dispatch(&root);
}
#[test]
fn full3_approved_builder_tool_and_prior_source_drift_deny_before_lock() {
    for case in 0..3 {
        let (_temp, root, mut context) = full3_fixture(None);
        match case {
            0 => context.builder.sha256 = sha256("wrong approved builder bytes"),
            1 => context.expected_tools[0].sha256 = sha256("wrong approved tool bytes"),
            _ => {
                fs::write(
                    root.path().join("crates/oe-code-runner/src/main.rs"),
                    "fn main(){ let revised:u64=2; }\n",
                )
                .unwrap();
            }
        }
        let reference = set_context(&root, &context);
        policy(
            &root,
            "oev2.controlled-version-authority/3",
            Some(reference),
            &["supervise_build"],
        );
        assert!(call(&root).is_err(), "case={case}");
        no_dispatch(&root);
    }
}
#[test]
fn full3_registry_generated_port_has_same_failure_record_boundary() {
    use oe_contracts::CodeNativeBuildSupervision3Port;
    let (_temp, root, context) = full3_fixture(None);
    let reference = set_context(&root, &context);
    policy(
        &root,
        "oev2.controlled-version-authority/3",
        Some(reference),
        &["register"],
    );
    let failure = registry::NativeFullCodeBuildSupervisionExecutor { root: &root }
        .supervise_full_native(&request())
        .unwrap_err();
    assert!(
        serde_json::to_string(&failure)
            .unwrap()
            .contains("actor_not_authorized")
    );
    no_dispatch(&root);
}
