//! Independent denial controls for explicit stream11 on the original owner.
//! These fixtures never execute a compiler and cannot establish native success.
use oe_contracts::{
    CodeBudget, CodeBuildRequest, CodeBuildSupervisionContext, CodeBuildSupervisionRequest,
    CodeNativeBuildSupervision11Port, CodeNativeBuildSupervisionRequest10,
    CodeNativeBuildSupervisionRequest11, CodeNativeReproductionRequest11,
};
use oe_core::{SkillRoot, write_new};
use oe_engineering::{artifacts, code_build_supervision, native_reproduction11, registry};
use serde_json::json;
use std::{fs, path::Path};

fn root() -> (tempfile::TempDir, SkillRoot) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    fs::create_dir_all(workspace.join("var/builds")).unwrap();
    let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
    fs::write(
        temp.path().join("SKILL.md"),
        "stream11 same-owner denial control",
    )
    .unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[workspace]\n[workspace.package]\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    let root = SkillRoot::new(temp.path()).unwrap();
    (temp, root)
}

fn request(id: &str) -> CodeNativeBuildSupervisionRequest11 {
    CodeNativeBuildSupervisionRequest11 {
        schema: "oev2.native-build-supervision-request/11".into(),
        owner_request: CodeBuildSupervisionRequest {
            schema: "oev2.code-build-supervision-request/1".into(),
            binding_id: id.into(),
            revision: "r11".into(),
            scope_id: "semantica.full-library/1".into(),
            actor: "maintainer".into(),
        },
        resources: oe_evolution::native_stream11::selected_resources11(),
    }
}

fn pair() -> CodeNativeReproductionRequest11 {
    CodeNativeReproductionRequest11 {
        schema: "oev2.native-reproduction-request/11".into(),
        first: request("first"),
        second: request("second"),
        evidence_path: "var/reproduction".into(),
    }
}

fn install_contexts(root: &SkillRoot, second_timeout: u64, output: u64, schema: &str) {
    fs::create_dir_all(root.path().join("var/input")).unwrap();
    // The old profile is intentionally incomplete for native6: it must never
    // gain authority from a stream resource selection or wrapper version.
    write_new(
        &root.path().join("var/input/profile.json"),
        &json!({"schema":"oev2.rust-runtime-profile/1"}),
    )
    .unwrap();
    let profile = artifacts::reference(root, &root.path().join("var/input/profile.json")).unwrap();
    let pin = oe_evolution::binary_reference(Path::new("/usr/bin/false"), root.path()).unwrap();
    let path = |p: &str| root.path().join(p).to_str().unwrap().to_owned();
    let mut bindings = serde_json::Map::new();
    for (id, timeout) in [("first", 10_000), ("second", second_timeout)] {
        let context = CodeBuildSupervisionContext {
            schema: schema.into(),
            purpose: "observe_compilation_origin".into(),
            binding_id: id.into(),
            revision: "r11".into(),
            scope_id: "semantica.full-library/1".into(),
            builder: pin.clone(),
            expected_tools: vec![pin.clone(); 3],
            source_profile: profile.clone(),
            dispatch_budget: CodeBudget {
                timeout_ms: timeout,
                output_bytes: output,
            },
            build: CodeBuildRequest {
                artifact_id: format!("denied-stream11-{id}"),
                source_root: root.path().to_str().unwrap().into(),
                cargo_home: path("var/cargo-home"),
                cargo_path: pin.path.clone(),
                rustc_path: pin.path.clone(),
                ggen_path: pin.path.clone(),
                target_dir: path(&format!("var/target-{id}")),
                evidence_path: path(&format!("var/evidence-{id}")),
                platform: "aarch64-apple-darwin".into(),
            },
        };
        let file = root.path().join(format!("var/input/{id}.json"));
        write_new(&file, &serde_json::to_value(context).unwrap()).unwrap();
        let reference = artifacts::reference(root, &file).unwrap();
        bindings.insert(
            id.into(),
            json!({"kind":"code_artifact","scope_id":"semantica.full-library/1",
            "actor_scopes":{"maintainer":["supervise_build"]},"code_build_supervision":reference}),
        );
    }
    let policy = root.path().join(registry::POLICY);
    fs::create_dir_all(policy.parent().unwrap()).unwrap();
    fs::write(
        &policy,
        serde_json::to_vec(
            &json!({"schema":"oev2.controlled-version-authority/3","bindings":bindings}),
        )
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&policy, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn no_dispatch(root: &SkillRoot) {
    for p in [
        "var/reproduction",
        "var/target-first",
        "var/target-second",
        "var/target-first-native-stage",
        "var/target-second-native-stage",
        "var/evidence-first",
        "var/evidence-second",
        "var/state/controlled-versions",
    ] {
        assert!(!root.path().join(p).exists(), "{p}");
    }
}

#[test]
fn caller_cannot_change_any_selected_stream_resource_before_owner_lookup() {
    let (_temp, root) = root();
    let original = serde_json::to_value(request("first")).unwrap();
    for field in [
        "raw_stream_bytes",
        "preview_bytes",
        "segment_bytes",
        "maximum_line_bytes",
        "maximum_lines",
        "maximum_page_bytes",
        "maximum_derived_bytes",
    ] {
        let mut value = original.clone();
        value["resources"][field] = json!(value["resources"][field].as_u64().unwrap() + 1);
        let changed: CodeNativeBuildSupervisionRequest11 = serde_json::from_value(value).unwrap();
        let error = code_build_supervision::supervise_stream11_native(&root, &changed)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("stream11_unselected_resource_baseline"),
            "{field}: {error}"
        );
    }
    assert!(!root.path().join("var").exists());
}

#[test]
fn legacy_schema_missing_resources_and_caller_authority_fields_cannot_cross_versions() {
    let (_temp, root) = root();
    let original = serde_json::to_value(request("first")).unwrap();
    let mut value = original.clone();
    value.as_object_mut().unwrap().remove("resources");
    assert!(serde_json::from_value::<CodeNativeBuildSupervisionRequest11>(value).is_err());
    assert!(
        serde_json::from_value::<CodeNativeBuildSupervisionRequest10>(original.clone()).is_err()
    );
    for schema in [
        "oev2.native-build-supervision-request/10",
        "oev2.native-build-supervision-request/6",
    ] {
        let mut changed = request("first");
        changed.schema = schema.into();
        assert!(code_build_supervision::supervise_stream11_native(&root, &changed).is_err());
    }
    for field in [
        "builder",
        "budget",
        "trusted",
        "capture",
        "release_eligibility",
    ] {
        let mut value = original.clone();
        value[field] = json!(true);
        assert!(
            serde_json::from_value::<CodeNativeBuildSupervisionRequest11>(value).is_err(),
            "{field}"
        );
    }
    let mut nested = pair();
    nested.second.owner_request.schema = "oev2.code-build-supervision-request/0".into();
    assert!(native_reproduction11::reproduce_stream11_native(&root, &nested).is_err());
    assert!(!root.path().join("var").exists());
}

#[test]
fn second_pair_resource_cannot_be_replaced_with_an_independent_limit() {
    let (_temp, root) = root();
    let mut input = pair();
    input.second.resources.raw_stream_bytes *= 2;
    let error = native_reproduction11::reproduce_stream11_native(&root, &input)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("stream11_unselected_resource_baseline"),
        "{error}"
    );
    assert!(!root.path().join("var").exists());
}

#[test]
fn same_existing_owner_denies_unknown_actor_and_revision_before_outputs() {
    let (_temp, root) = root();
    install_contexts(
        &root,
        10_000,
        1024 * 1024,
        "oev2.code-build-supervision-context/2",
    );
    let mut changed = request("first");
    changed.owner_request.actor = "outside".into();
    let executor = registry::NativeStream11CodeBuildSupervisionExecutor { root: &root };
    let error = executor.supervise_full_native(&changed).unwrap_err();
    assert!(format!("{error:?}").contains("version_actor_not_authorized"));
    let mut changed = request("first");
    changed.owner_request.revision = "unselected-r12".into();
    let error = code_build_supervision::supervise_stream11_native(&root, &changed)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("code_build_supervision_context_binding_changed"),
        "{error}"
    );
    no_dispatch(&root);
}

#[test]
fn pair_cannot_restart_or_increase_the_second_owner_budget() {
    let (_temp, root) = root();
    install_contexts(
        &root,
        11_000,
        1024 * 1024,
        "oev2.code-build-supervision-context/2",
    );
    let error = native_reproduction11::reproduce_stream11_native(&root, &pair())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("native_reproduction_selected_conditions_differ"),
        "{error}"
    );
    no_dispatch(&root);
}

#[test]
fn selected_cargo_raw_capacity_never_expands_original_dispatch_body_budget() {
    let (_temp, root) = root();
    install_contexts(
        &root,
        10_000,
        64 * 1024 * 1024,
        "oev2.code-build-supervision-context/2",
    );
    let error = code_build_supervision::supervise_stream11_native(&root, &request("first"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("owner_native_dispatch_budget"), "{error}");
    let error = native_reproduction11::reproduce_stream11_native(&root, &pair())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("native_reproduction_selected_budget"),
        "{error}"
    );
    no_dispatch(&root);
}

#[test]
fn original_context1_cannot_become_a_shared_total_deadline_context() {
    let (_temp, root) = root();
    install_contexts(
        &root,
        10_000,
        1024 * 1024,
        "oev2.code-build-supervision-context/1",
    );
    let error = code_build_supervision::supervise_stream11_native(&root, &request("first"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("owner_native_requires_total_budget_context2"),
        "{error}"
    );
    let error = native_reproduction11::reproduce_stream11_native(&root, &pair())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("native_reproduction_selected_conditions_differ"),
        "{error}"
    );
    no_dispatch(&root);
}

#[test]
fn exact_resources_and_selected_owner_do_not_qualify_a_legacy_profile() {
    let (_temp, root) = root();
    install_contexts(
        &root,
        10_000,
        1024 * 1024,
        "oev2.code-build-supervision-context/2",
    );
    let error = code_build_supervision::supervise_stream11_native(&root, &request("first"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing field"), "{error}");
    let error = native_reproduction11::reproduce_stream11_native(&root, &pair())
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing field"), "{error}");
    no_dispatch(&root);
}

#[test]
fn normalizer_preparation_uses_original_actor_revision_and_budget_checks() {
    use oe_contracts::{
        CodeNativeNormalizerSupervision1Port, CodeNativeNormalizerSupervisionRequest1,
    };
    let (_temp, root) = root();
    install_contexts(
        &root,
        10_000,
        1024 * 1024,
        "oev2.code-build-supervision-context/2",
    );
    let old = request("first");
    let mut input = CodeNativeNormalizerSupervisionRequest1 {
        schema: "oev2.native-normalizer-supervision-request/1".into(),
        owner_request: old.owner_request,
        resources: old.resources,
    };
    input.owner_request.actor = "outside".into();
    let executor = registry::NativeNormalizerSupervisionExecutor { root: &root };
    let error = executor.prepare_normalizer(&input).unwrap_err();
    assert!(format!("{error:?}").contains("version_actor_not_authorized"));
    input.owner_request.actor = "maintainer".into();
    input.owner_request.revision = "unselected".into();
    let error = code_build_supervision::supervise_normalizer(&root, &input)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("code_build_supervision_context_binding_changed"),
        "{error}"
    );
    input.owner_request.revision = "r11".into();
    let mut value = serde_json::to_value(&input).unwrap();
    value["budget"] = json!({"timeout_ms":3600000});
    assert!(serde_json::from_value::<CodeNativeNormalizerSupervisionRequest1>(value).is_err());
    no_dispatch(&root);
}
