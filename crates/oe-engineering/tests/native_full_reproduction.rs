use oe_contracts::{
    CodeBudget, CodeBuildRequest, CodeBuildSupervisionContext, CodeBuildSupervisionRequest,
    CodeNativeBuildSupervisionRequest3, CodeNativeReproductionRequest3,
};
use oe_core::{SkillRoot, write_new};
use oe_engineering::{artifacts, native_reproduction, registry};
use serde_json::json;
use std::{fs, path::Path};

fn root() -> (tempfile::TempDir, SkillRoot) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    fs::create_dir_all(workspace.join("var/builds")).unwrap();
    let t = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
    fs::write(
        t.path().join("SKILL.md"),
        "independent reproduction content/denial fixture",
    )
    .unwrap();
    fs::write(
        t.path().join("Cargo.toml"),
        "[workspace]\n[workspace.package]\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    let r = SkillRoot::new(t.path()).unwrap();
    (t, r)
}
#[test]
fn complete_file_bytes_compare_and_same_size_mutation_rejects() {
    let (_t, r) = root();
    fs::create_dir_all(r.path().join("var/input")).unwrap();
    let a = r.path().join("var/input/a");
    let b = r.path().join("var/input/b");
    fs::write(&a, b"complete native content control:AB").unwrap();
    fs::write(&b, b"complete native content control:AB").unwrap();
    let first = oe_evolution::binary_reference(&a, r.path()).unwrap();
    let second = oe_evolution::binary_reference(&b, r.path()).unwrap();
    assert!(native_reproduction::full_file_equal(&first, &second).unwrap());
    fs::write(&b, b"complete native content control:BA").unwrap();
    assert_eq!(fs::metadata(&b).unwrap().len(), second.bytes);
    assert!(native_reproduction::full_file_equal(&first, &second).is_err());
    let changed = oe_evolution::binary_reference(&b, r.path()).unwrap();
    assert!(!native_reproduction::full_file_equal(&first, &changed).unwrap());
    assert!(!r.path().join("var/state/controlled-versions").exists());
}
#[test]
fn original_full_failed_pair_remains_different_with_equal_compiled_identity() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let file = workspace
        .parent()
        .unwrap()
        .join("ORIGINAL-FAILED-PAIR-TEST-INPUT.json");
    let input: serde_json::Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
    let sides: Vec<oe_contracts::CodeSourceArtifact> = input["sides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| serde_json::from_value(v.clone()).unwrap())
        .collect();
    assert_eq!(sides[0].runner.bytes, 21543360);
    assert_eq!(sides[1].runner.bytes, 21543360);
    assert_eq!(sides[0].compiled, sides[1].compiled);
    assert_ne!(sides[0].runner.sha256, sides[1].runner.sha256);
    assert!(!native_reproduction::full_file_equal(&sides[0].runner, &sides[1].runner).unwrap());
}
fn request(id: &str, actor: &str) -> CodeNativeBuildSupervisionRequest3 {
    CodeNativeBuildSupervisionRequest3 {
        schema: "oev2.native-build-supervision-request/3".into(),
        owner_request: CodeBuildSupervisionRequest {
            schema: "oev2.code-build-supervision-request/1".into(),
            binding_id: id.into(),
            revision: "r1".into(),
            scope_id: "semantica.full-library/1".into(),
            actor: actor.into(),
        },
    }
}
fn selected(root: &SkillRoot, second_budget: u64) -> CodeNativeReproductionRequest3 {
    fs::create_dir_all(root.path().join("var/input")).unwrap();
    write_new(
        &root.path().join("var/input/profile.json"),
        &json!({"schema":"oev2.rust-runtime-profile/1"}),
    )
    .unwrap();
    let profile = artifacts::reference(root, &root.path().join("var/input/profile.json")).unwrap();
    let pin = oe_evolution::binary_reference(Path::new("/usr/bin/false"), root.path()).unwrap();
    let path = |p: &str| root.path().join(p).to_str().unwrap().to_owned();
    let mut bindings = serde_json::Map::new();
    for (id, n, budget) in [("first", "a", 10_000), ("second", "b", second_budget)] {
        let context = CodeBuildSupervisionContext {
            schema: "oev2.code-build-supervision-context/2".into(),
            purpose: "observe_compilation_origin".into(),
            binding_id: id.into(),
            revision: "r1".into(),
            scope_id: "semantica.full-library/1".into(),
            builder: pin.clone(),
            expected_tools: vec![pin.clone(), pin.clone(), pin.clone()],
            source_profile: profile.clone(),
            dispatch_budget: CodeBudget {
                timeout_ms: budget,
                output_bytes: 1024 * 1024,
            },
            build: CodeBuildRequest {
                artifact_id: format!("negative-{n}"),
                source_root: root.path().to_str().unwrap().into(),
                cargo_home: path("var/cache"),
                cargo_path: pin.path.clone(),
                rustc_path: pin.path.clone(),
                ggen_path: pin.path.clone(),
                target_dir: path(&format!("var/target-{n}")),
                evidence_path: path(&format!("var/evidence-{n}")),
                platform: "aarch64-apple-darwin".into(),
            },
        };
        let file = root.path().join(format!("var/input/{id}.json"));
        write_new(&file, &serde_json::to_value(context).unwrap()).unwrap();
        let cref = artifacts::reference(root, &file).unwrap();
        bindings.insert(
            id.into(),
            json!({"kind":"code_artifact","scope_id":"semantica.full-library/1",
            "actor_scopes":{"maintainer":["supervise_build"]},"code_build_supervision":cref}),
        );
    }
    let file = root.path().join(registry::POLICY);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(
        &file,
        serde_json::to_vec(
            &json!({"schema":"oev2.controlled-version-authority/3","bindings":bindings}),
        )
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    CodeNativeReproductionRequest3 {
        schema: "oev2.native-reproduction-request/3".into(),
        first: request("first", "maintainer"),
        second: request("second", "maintainer"),
        evidence_path: "var/reproduction".into(),
    }
}
fn no_build(root: &SkillRoot) {
    for p in [
        "var/reproduction",
        "var/target-a",
        "var/target-b",
        "var/evidence-a",
        "var/evidence-b",
        "var/state/controlled-versions",
    ] {
        assert!(!root.path().join(p).exists(), "{p}");
    }
}
#[test]
fn existing_owner_actor_denies_before_any_build_or_reproduction_output() {
    let (_t, r) = root();
    let mut input = selected(&r, 10_000);
    input.second.owner_request.actor = "outside".into();
    use oe_contracts::CodeNativeReproduction3Port;
    let executor = registry::NativeFullCodeReproductionExecutor { root: &r };
    assert!(executor.reproduce_full_native(&input).is_err());
    no_build(&r);
}
#[test]
fn second_context_cannot_add_an_independent_budget() {
    let (_t, r) = root();
    let input = selected(&r, 11_000);
    let error = native_reproduction::reproduce_full_native(&r, &input)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("native_reproduction_selected_conditions_differ"),
        "{error}"
    );
    no_build(&r);
}
#[test]
fn legacy_profile_never_becomes_full_native_by_wrapper() {
    let (_t, r) = root();
    let input = selected(&r, 10_000);
    assert!(native_reproduction::reproduce_full_native(&r, &input).is_err());
    no_build(&r);
}
#[test]
fn unknown_caller_trust_field_is_rejected_by_generated_contract() {
    let (_t, r) = root();
    let input = selected(&r, 10_000);
    let mut value = serde_json::to_value(input).unwrap();
    value["passed"] = json!(true);
    assert!(serde_json::from_value::<CodeNativeReproductionRequest3>(value).is_err());
    no_build(&r);
}
