//! Explicit full native /3 CONTENT checker. It never runs tools or creates
//! owner execution/registry/adoption authority; original /2 remains unchanged.
use crate::native_profile::verify_file;
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
pub const PROFILE: &str = "semantica.rust-runtime-native-debug/3";
pub const FULL_ALGORITHM: &str = "darwin-archive-response/1";
pub const FULL_INPUT_DOMAIN: &str =
    "own_objects_own_regular_archives_external_toolchain_archives_gnu_response";
const JSON_LIMIT: usize = 16 * 1024 * 1024;
const MAX_ROWS: usize = 4096;
const TOOL_ROLES: [&str; 8] = [
    "linker_adapter",
    "clang_driver",
    "native_linker",
    "archive_reader",
    "debug_mapper",
    "debug_linker",
    "dwarf_reader",
    "signature_verifier",
];
fn reference_bytes(root: &SkillRoot, reference: &ArtifactReference) -> Result<Vec<u8>> {
    reference
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_reference:{e:?}"))?;
    let path = root.private_path(Path::new(&reference.path))?;
    let bytes = read_regular_bytes(&path, JSON_LIMIT)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "native3_reference_changed"
    );
    Ok(bytes)
}
fn reference_json(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    Ok(strict_json(&reference_bytes(root, reference)?)?)
}
/// Validate full recipe CONTENT with the actual SDK alias inspector.
pub fn inspect_recipe(
    root: &SkillRoot,
    reference: &ArtifactReference,
    baseline: &CodeRustRuntimeProfile,
) -> Result<CodeNativeBuildRecipe3> {
    let recipe: CodeNativeBuildRecipe3 = serde_json::from_value(reference_json(root, reference)?)?;
    recipe
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_recipe:{e:?}"))?;
    ensure!(
        recipe.schema == "oev2.native-build-recipe/3"
            && recipe.algorithm == FULL_ALGORITHM
            && recipe.input_domain == FULL_INPUT_DOMAIN
            && recipe.target == "aarch64-apple-darwin"
            && recipe.target == baseline.expected_target
            && recipe.native_parameter_preset == "darwin-reproducible-prefix-sdk/1",
        "native3_recipe_domain"
    );
    let debug = &recipe.debug;
    debug
        .validate()
        .map_err(|e| anyhow::anyhow!("native_debug_conditions:{e:?}"))?;
    ensure!(
        debug.cargo_profile == "dev"
            && debug.debug == 2
            && debug.split_debuginfo == "unpacked"
            && debug.strip == "none"
            && debug.codegen_units == 256
            && debug.incremental
            && !debug.lto
            && !debug.scope_packages.is_empty()
            && debug.scope_packages.len() <= MAX_ROWS,
        "native_debug_conditions_unsupported"
    );
    let cargo = root.expand(Path::new(&debug.cargo_manifest.path))?;
    ensure!(
        cargo == root.path().join("Cargo.toml"),
        "native_cargo_manifest_not_current"
    );
    no_symlinks(&cargo)?;
    let cargo_bytes = read_regular_bytes(&cargo, JSON_LIMIT)?;
    ensure!(
        sha256(&cargo_bytes) == debug.cargo_manifest.sha256,
        "native_cargo_manifest_changed"
    );
    let cargo_value: toml::Value = toml::from_str(std::str::from_utf8(&cargo_bytes)?)?;
    ensure!(
        cargo_value.get("workspace").is_some(),
        "native_cargo_workspace_missing"
    );
    let mut scopes = BTreeSet::new();
    for name in &debug.scope_packages {
        ensure!(
            !name.trim().is_empty() && scopes.insert(name),
            "native_debug_scope_invalid"
        );
        let manifest = root.path().join("crates").join(name).join("Cargo.toml");
        no_symlinks(&manifest)?;
        let value: toml::Value = toml::from_str(std::str::from_utf8(&read_regular_bytes(
            &manifest, JSON_LIMIT,
        )?)?)?;
        ensure!(
            value
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(toml::Value::as_str)
                == Some(name.as_str()),
            "native_debug_package_not_current"
        );
    }
    ensure!(recipe.tools.len() == TOOL_ROLES.len(), "native_tool_set");
    let mut roles = BTreeSet::new();
    for tool in &recipe.tools {
        tool.validate()
            .map_err(|e| anyhow::anyhow!("native_tool:{e:?}"))?;
        ensure!(
            TOOL_ROLES.contains(&tool.role.as_str()) && roles.insert(tool.role.as_str()),
            "native_tool_role"
        );
        verify_file(&tool.file)?;
        ensure!(tool.file.mode & 0o111 != 0, "native_tool_not_executable");
        if let Some(version) = &tool.version_observation {
            reference_bytes(root, version)?;
        }
    }
    let adapter = recipe
        .tools
        .iter()
        .find(|t| t.role == "linker_adapter")
        .context("native_adapter_missing")?;
    let wanted = format!("-C\u{1f}linker={}", adapter.file.path);
    ensure!(
        recipe.raw_encoded_rustflags == wanted
            && sha256(recipe.raw_encoded_rustflags.as_bytes())
                == recipe.raw_encoded_rustflags_sha256,
        "native_raw_encoded_flags_changed"
    );
    crate::native_sdk::inspect_sdk(root, &recipe.sdk_layout)?;
    ensure!(
        !recipe.external_archive_bindings.is_empty()
            && recipe.external_archive_bindings.len() <= MAX_ROWS,
        "native_external_archive_set"
    );
    let mut archives = BTreeSet::new();
    for file in &recipe.external_archive_bindings {
        ensure!(
            archives.insert(&file.path),
            "native_external_archive_duplicate"
        );
        verify_file(file)?;
    }
    Ok(recipe)
}
/// Inspect one independently sourced image's CONTENT only. Real DWARF and
/// signature observations must be produced separately by the native runtime.
pub fn inspect_debug_image(
    root: &SkillRoot,
    recipe: &CodeNativeBuildRecipe3,
    image: &CodeNativeImageDebugExpectations3,
) -> Result<CodeNativeDebugExpectations> {
    image
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_debug_image:{e:?}"))?;
    let package = match image.image_role.as_str() {
        "normalizer" => "oe-design",
        "runner" => "oe-code-runner",
        _ => anyhow::bail!("native3_debug_image_role"),
    };
    ensure!(
        image.package == package && recipe.debug.scope_packages.iter().any(|p| p == package),
        "native3_debug_image_package_scope"
    );
    let package_root = root.path().join("crates").join(package);
    let manifest = package_root.join("Cargo.toml");
    no_symlinks(&manifest)?;
    let manifest_bytes = read_regular_bytes(&manifest, JSON_LIMIT)?;
    let value: toml::Value = toml::from_str(std::str::from_utf8(&manifest_bytes)?)?;
    ensure!(
        value
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            == Some(package),
        "native3_debug_image_package_missing"
    );
    let expected = crate::native_profile::inspect_shared_expectations(root, &image.expectations)?;
    for check in &expected.checks {
        let path = root.expand(Path::new(&check.source.path))?;
        no_symlinks(&path)?;
        ensure!(
            path.canonicalize()?.starts_with(&package_root),
            "native3_debug_image_source_wrong_package"
        );
    }
    ensure!(
        crate::native_profile::inspect_shared_expectations(root, &image.expectations)? == expected,
        "native3_debug_image_changed_during_inspection"
    );
    ensure!(
        read_regular_bytes(&manifest, JSON_LIMIT)? == manifest_bytes,
        "native3_debug_image_manifest_changed"
    );
    Ok(expected)
}
fn inspect_debug_bundle(
    root: &SkillRoot,
    reference: &ArtifactReference,
    recipe: &CodeNativeBuildRecipe3,
) -> Result<CodeNativeDebugExpectationsBundle3> {
    let bundle: CodeNativeDebugExpectationsBundle3 =
        serde_json::from_value(reference_json(root, reference)?)?;
    bundle
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_debug_bundle:{e:?}"))?;
    ensure!(
        bundle.schema == "oev2.native-debug-expectations-bundle/3"
            && !bundle.scope.trim().is_empty()
            && bundle.images.len() == 2,
        "native3_debug_bundle_domain"
    );
    let mut roles = BTreeSet::new();
    let mut refs = BTreeSet::new();
    let mut expected = Vec::new();
    for image in &bundle.images {
        ensure!(
            roles.insert(image.image_role.as_str())
                && refs.insert((&image.expectations.path, &image.expectations.sha256)),
            "native3_debug_image_duplicate"
        );
        expected.push(inspect_debug_image(root, recipe, image)?);
    }
    ensure!(
        roles == BTreeSet::from(["normalizer", "runner"]),
        "native3_debug_image_missing"
    );
    for (image, first) in bundle.images.iter().zip(expected) {
        ensure!(
            inspect_debug_image(root, recipe, image)? == first,
            "native3_debug_bundle_image_changed"
        );
    }
    ensure!(
        reference_json(root, reference)? == serde_json::to_value(&bundle)?,
        "native3_debug_bundle_changed"
    );
    Ok(bundle)
}
/// Return both independently checked package expectations. This is content,
/// never a replacement for two actual native debug/signature observations.
pub fn load_debug_images(
    root: &SkillRoot,
    profile_ref: &ArtifactReference,
) -> Result<CodeNativeDebugExpectationsBundle3> {
    let profile = inspect_profile(root, profile_ref)?;
    let (baseline, _) = crate::runtime_identity::inspect_profile(root, &profile.baseline_profile)?;
    let recipe = inspect_recipe(root, &profile.recipe, &baseline)?;
    inspect_debug_bundle(root, &profile.debug_expectations, &recipe)
}

/// Source and content only: serialized /3 never attests full archive execution.
pub fn inspect_profile(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<CodeNativeRuntimeProfile3> {
    let profile: CodeNativeRuntimeProfile3 =
        serde_json::from_value(reference_json(root, reference)?)?;
    profile
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_profile:{e:?}"))?;
    ensure!(
        profile.schema == "oev2.native-runtime-profile/3" && profile.profile == PROFILE,
        "native3_profile_unsupported"
    );
    let (baseline, manifest) =
        crate::runtime_identity::inspect_profile(root, &profile.baseline_profile)?;
    ensure!(
        baseline.expected_build_capture.is_none() && baseline.source_kind == "frozen_export",
        "native3_prior_conditions_invalid"
    );
    let recipe = inspect_recipe(root, &profile.recipe, &baseline)?;
    let expectations = inspect_debug_bundle(root, &profile.debug_expectations, &recipe)?;
    ensure!(
        crate::runtime_identity::inspect_profile(root, &profile.baseline_profile)?
            == (baseline.clone(), manifest.clone()),
        "native3_source_changed_during_inspection"
    );
    ensure!(
        inspect_recipe(root, &profile.recipe, &baseline)? == recipe,
        "native3_recipe_changed_during_inspection"
    );
    ensure!(
        inspect_debug_bundle(root, &profile.debug_expectations, &recipe)? == expectations,
        "native3_expectations_changed_during_inspection"
    );
    ensure!(
        crate::runtime_identity::inspect_profile(root, &profile.baseline_profile)?
            == (baseline, manifest),
        "native3_source_changed_after_content_inspection"
    );
    ensure!(
        reference_json(root, reference)? == serde_json::to_value(&profile)?,
        "native3_profile_changed_during_inspection"
    );
    Ok(profile)
}
pub fn inspect_side(
    root: &SkillRoot,
    profile_ref: &ArtifactReference,
    side: &CodeNativeSideContext3,
) -> Result<()> {
    side.validate()
        .map_err(|e| anyhow::anyhow!("native_side:{e:?}"))?;
    let p = inspect_profile(root, profile_ref)?;
    let (baseline, _) = crate::runtime_identity::inspect_profile(root, &p.baseline_profile)?;
    let recipe = inspect_recipe(root, &p.recipe, &baseline)?;
    ensure!(
        side.schema == "oev2.native-side-context/3"
            && side.source_profile == *profile_ref
            && side.recipe_sha256 == p.recipe.sha256,
        "native_side_profile_changed"
    );
    let context: CodeBuildSupervisionContext =
        serde_json::from_value(reference_json(root, &side.owner_context)?)?;
    context
        .validate()
        .map_err(|e| anyhow::anyhow!("native_owner_context:{e:?}"))?;
    ensure!(
        context.schema == "oev2.code-build-supervision-context/2"
            && context.purpose == "observe_compilation_origin"
            && context.source_profile == *profile_ref
            && context.build == side.build,
        "native_owner_context_changed"
    );
    ensure!(
        Path::new(&side.build.source_root) == root.path()
            && side.build.platform == recipe.target
            && context.builder.source_root == side.build.source_root,
        "native_side_source_target_changed"
    );
    crate::verify_binary(&context.builder)?;
    let expected_paths = [
        &side.build.cargo_path,
        &side.build.rustc_path,
        &side.build.ggen_path,
    ];
    ensure!(
        context.expected_tools.len() == expected_paths.len(),
        "native_side_tools"
    );
    for (pin, path) in context.expected_tools.iter().zip(expected_paths) {
        ensure!(
            pin.path == *path
                && pin.source_root == side.build.source_root
                && pin.source_manifest_sha256 == context.builder.source_manifest_sha256,
            "native_side_tool_binding_changed"
        );
        crate::verify_binary(pin)?;
    }
    let toolchain = Path::new(&side.build.rustc_path)
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == "bin"))
        .and_then(Path::parent)
        .context("native_rustc_toolchain_path_uncovered")?;
    let stdlib = toolchain
        .join("lib/rustlib")
        .join(&side.build.platform)
        .join("lib");
    for archive in &recipe.external_archive_bindings {
        ensure!(
            Path::new(&archive.path).starts_with(&stdlib),
            "native_archive_not_selected_stdlib"
        );
    }
    let target = root.private_path(Path::new(&side.build.target_dir))?;
    let evidence = root.private_path(Path::new(&side.build.evidence_path))?;
    let stage = root.private_path(Path::new(&side.stage_root))?;
    ensure!(
        !target.starts_with(&evidence) && !evidence.starts_with(&target),
        "native_side_paths_overlap"
    );
    let leaf = target
        .file_name()
        .and_then(|n| n.to_str())
        .context("native_target_leaf")?;
    let expected_stage = target.with_file_name(format!("{leaf}-native-stage"));
    ensure!(
        stage == expected_stage && !stage.starts_with(&evidence) && !evidence.starts_with(&stage),
        "native_stage_outside_side"
    );
    for tool in &recipe.tools {
        let tool_path = Path::new(&tool.file.path);
        ensure!(
            !tool_path.starts_with(&target) && !tool_path.starts_with(&evidence),
            "native_tool_in_side_paths"
        );
    }
    ensure!(
        side.logical_debug_root == "oe-native-debug",
        "native_logical_debug_root"
    );
    let dispatch = &context.dispatch_budget;
    ensure!(
        dispatch.timeout_ms > 250
            && dispatch.timeout_ms <= 3_600_000
            && dispatch.output_bytes > 0
            && dispatch.output_bytes <= 4 * 1024 * 1024
            && side.body_budget.timeout_ms > 0
            && side.body_budget.timeout_ms <= dispatch.timeout_ms - 250
            && side.body_budget.output_bytes == dispatch.output_bytes,
        "native_side_budget_invalid"
    );
    ensure!(
        reference_json(root, &side.owner_context)? == serde_json::to_value(&context)?,
        "native_owner_context_changed_during_inspection"
    );
    inspect_profile(root, profile_ref)?;
    Ok(())
}

/// Load an exact side document and inspect its CONTENT through the same checker.
/// A successful decode/check grants no actor authority or elapsed-time lease.
pub fn load_side(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<CodeNativeSideContext3> {
    let side: CodeNativeSideContext3 = serde_json::from_value(reference_json(root, reference)?)?;
    inspect_side(root, &side.source_profile, &side)?;
    ensure!(
        reference_json(root, reference)? == serde_json::to_value(&side)?,
        "native_side_changed_during_inspection"
    );
    Ok(side)
}
/// Load the selected profile's recipe, without a second parser or owner.
pub fn load_recipe(
    root: &SkillRoot,
    profile: &ArtifactReference,
) -> Result<CodeNativeBuildRecipe3> {
    let p = inspect_profile(root, profile)?;
    let (baseline, _) = crate::runtime_identity::inspect_profile(root, &p.baseline_profile)?;
    inspect_recipe(root, &p.recipe, &baseline)
}

pub struct NativeProfile3Inspector<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeRuntimeProfile3InspectPort for NativeProfile3Inspector<'_> {
    fn inspect_native_profile3(
        &self,
        input: &ArtifactReference,
    ) -> std::result::Result<CodeNativeRuntimeProfile3, FailureRecord> {
        inspect_profile(self.root,input).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,phase:"native_profile3_content_inspection".into(),retryable:false,partial_result:Some(json!({"inspection":"rejected","reason":format!("{e:#}"),"execution":"not_run","compilation_origin":"unknown","native_available":"unknown","sdk_closure":"unknown","archive_closure":"unknown","engineering_adoption":"not_established"}))})
    }
}
