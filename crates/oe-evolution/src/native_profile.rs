//! Explicit native /2 CONTENT inspection, not an executor or authority owner.
//! The original profile /1 verifier is reused unchanged. Passing these checks
//! never establishes that any builder, linker, observer or actor actually ran.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub const PROFILE: &str = "semantica.rust-runtime-native-debug/2";
pub const DIRECT_ALGORITHM: &str = "darwin-direct-objects/1";
const JSON_LIMIT: usize = 16 * 1024 * 1024;
const FILE_LIMIT: u64 = 512 * 1024 * 1024;
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
        .map_err(|e| anyhow::anyhow!("native_reference:{e:?}"))?;
    let path = root.private_path(Path::new(&reference.path))?;
    let bytes = read_regular_bytes(&path, JSON_LIMIT)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "native_reference_changed"
    );
    Ok(bytes)
}
fn reference_json(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    Ok(strict_json(&reference_bytes(root, reference)?)?)
}
fn mode(path: &Path) -> Result<u64> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "native_file_not_regular"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(u64::from(metadata.permissions().mode() & 0o7777))
    }
    #[cfg(not(unix))]
    {
        anyhow::bail!("native_file_mode_platform_uncovered")
    }
}
/// Exact file-content inspection only. Pinning a tool does not attest its source
/// or authorize executing it. The recipe caller owns the selected input scope.
pub fn verify_file(pin: &CodeNativeFileBinding) -> Result<()> {
    pin.validate()
        .map_err(|e| anyhow::anyhow!("native_file:{e:?}"))?;
    let path = Path::new(&pin.path);
    ensure!(path.is_absolute(), "native_file_not_absolute");
    no_symlinks(path)?;
    ensure!(
        pin.bytes > 0 && pin.bytes <= FILE_LIMIT && pin.mode <= 0o7777,
        "native_file_bounds"
    );
    let before = mode(path)?;
    let first = fingerprint_file(path, FILE_LIMIT)?;
    let after = mode(path)?;
    let second = fingerprint_file(path, FILE_LIMIT)?;
    ensure!(
        before == after
            && before == pin.mode
            && first == second
            && first.sha256 == pin.sha256
            && first.bytes == pin.bytes,
        "native_file_identity_changed"
    );
    Ok(())
}
fn source_reference(root: &SkillRoot, reference: &ArtifactReference) -> Result<()> {
    reference
        .validate()
        .map_err(|e| anyhow::anyhow!("native_debug_source:{e:?}"))?;
    let path = root.expand(Path::new(&reference.path))?;
    ensure!(
        path.starts_with(root.path()),
        "native_debug_source_outside_root"
    );
    no_symlinks(&path)?;
    let bytes = read_regular_bytes(&path, JSON_LIMIT)?;
    ensure!(
        !bytes.is_empty() && sha256(&bytes) == reference.sha256,
        "native_debug_source_changed"
    );
    Ok(())
}
fn validate_expectations(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<CodeNativeDebugExpectations> {
    let expected: CodeNativeDebugExpectations =
        serde_json::from_value(reference_json(root, reference)?)?;
    expected
        .validate()
        .map_err(|e| anyhow::anyhow!("native_debug_expectations:{e:?}"))?;
    ensure!(
        expected.schema == "oev2.native-debug-expectations/2"
            && expected.all_required_oso
            && expected.signature_required
            && !expected.checks.is_empty()
            && expected.checks.len() <= MAX_ROWS,
        "native_debug_expectations_incomplete"
    );
    let mut ids = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for c in &expected.checks {
        c.validate()
            .map_err(|e| anyhow::anyhow!("native_debug_check:{e:?}"))?;
        ensure!(ids.insert(&c.check_id), "native_debug_duplicate_check");
        ensure!(
            [
                "source_line",
                "function",
                "type_member",
                "parameter",
                "local"
            ]
            .contains(&c.kind.as_str()),
            "native_debug_kind"
        );
        ensure!(
            !c.function.trim().is_empty() && !c.name.trim().is_empty(),
            "native_debug_check_identity"
        );
        ensure!(c.line != Some(0), "native_debug_line");
        if c.kind == "source_line" {
            ensure!(
                c.line.is_some() && c.matcher == "exact",
                "native_debug_source_line"
            );
        }
        if ["type_member", "parameter", "local"].contains(&c.kind.as_str()) {
            ensure!(
                c.expected_type
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty()),
                "native_debug_type_missing"
            );
        } else {
            ensure!(c.expected_type.is_none(), "native_debug_unexpected_type");
        }
        ensure!(
            ["exact", "type_contains"].contains(&c.matcher.as_str()),
            "native_debug_matcher"
        );
        source_reference(root, &c.source)?;
        kinds.insert(c.kind.as_str());
    }
    ensure!(
        [
            "source_line",
            "function",
            "type_member",
            "parameter",
            "local"
        ]
        .iter()
        .all(|k| kinds.contains(k)),
        "native_debug_required_kind_missing"
    );
    Ok(expected)
}
/// Strict recipe inspection. Complete own-archive/response support is explicitly
/// unsupported until a separately implemented/frozen recipe revision is received.
pub fn inspect_recipe(
    root: &SkillRoot,
    reference: &ArtifactReference,
    baseline: &CodeRustRuntimeProfile,
) -> Result<CodeNativeBuildRecipe> {
    let recipe: CodeNativeBuildRecipe = serde_json::from_value(reference_json(root, reference)?)?;
    ensure!(
        recipe.algorithm == DIRECT_ALGORITHM,
        "native_algorithm_unsupported"
    );
    recipe
        .validate()
        .map_err(|e| anyhow::anyhow!("native_recipe:{e:?}"))?;
    ensure!(
        recipe.schema == "oev2.native-build-recipe/2"
            && recipe.input_domain == "own_objects_external_archives"
            && recipe.target == "aarch64-apple-darwin"
            && recipe.target == baseline.expected_target
            && recipe.native_parameter_preset == "darwin-reproducible-prefix-sdk/1",
        "native_recipe_domain"
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
    ensure!(recipe.sdk_bindings.len() == 4, "native_sdk_set");
    let mut sdk_paths = BTreeSet::new();
    let settings = recipe
        .sdk_bindings
        .iter()
        .find(|p| {
            Path::new(&p.path)
                .file_name()
                .is_some_and(|n| n == "SDKSettings.json")
        })
        .context("native_sdk_settings_missing")?;
    let sdk = Path::new(&settings.path)
        .parent()
        .context("native_sdk_root")?;
    let required = [
        sdk.join("SDKSettings.json"),
        sdk.join("usr/lib/libSystem.tbd"),
        sdk.join("usr/lib/libc.tbd"),
        sdk.join("usr/lib/libm.tbd"),
    ];
    for file in &recipe.sdk_bindings {
        ensure!(
            required.iter().any(|p| p == Path::new(&file.path)) && sdk_paths.insert(&file.path),
            "native_sdk_binding"
        );
        verify_file(file)?;
    }
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
/// Returns validated CONTENT only. No builder is invoked; no origin/adoption
/// field or execution proof is created by this function.
pub fn inspect_profile(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<CodeNativeRuntimeProfile> {
    let p: CodeNativeRuntimeProfile = serde_json::from_value(reference_json(root, reference)?)?;
    p.validate()
        .map_err(|e| anyhow::anyhow!("native_profile:{e:?}"))?;
    ensure!(
        p.schema == "oev2.native-runtime-profile/2" && p.profile == PROFILE,
        "native_profile_unsupported"
    );
    let (baseline, manifest) = crate::runtime_identity::inspect_profile(root, &p.baseline_profile)?;
    ensure!(
        baseline.expected_build_capture.is_none() && baseline.source_kind == "frozen_export",
        "native_prior_conditions_invalid"
    );
    inspect_recipe(root, &p.recipe, &baseline)?;
    validate_expectations(root, &p.debug_expectations)?;
    ensure!(
        crate::runtime_identity::inspect_profile(root, &p.baseline_profile)?
            == (baseline, manifest),
        "native_source_changed_during_inspection"
    );
    // Re-read every recipe/file/expectation on the second pass; no stat-only cache.
    let (baseline, _) = crate::runtime_identity::inspect_profile(root, &p.baseline_profile)?;
    inspect_recipe(root, &p.recipe, &baseline)?;
    validate_expectations(root, &p.debug_expectations)?;
    ensure!(
        reference_json(root, reference)? == serde_json::to_value(&p)?,
        "native_profile_changed_during_inspection"
    );
    Ok(p)
}
/// Validate a side's serialized source/build/path/budget CONTENT against the
/// referenced original context. Actor/registry authority, freshness, elapsed
/// deadline and actual dispatch must still be established by the original owner.
pub fn inspect_side(
    root: &SkillRoot,
    profile_ref: &ArtifactReference,
    side: &CodeNativeSideContext,
) -> Result<()> {
    side.validate()
        .map_err(|e| anyhow::anyhow!("native_side:{e:?}"))?;
    let p = inspect_profile(root, profile_ref)?;
    let (baseline, _) = crate::runtime_identity::inspect_profile(root, &p.baseline_profile)?;
    let recipe = inspect_recipe(root, &p.recipe, &baseline)?;
    ensure!(
        side.schema == "oev2.native-side-context/2"
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
pub fn load_side(root: &SkillRoot, reference: &ArtifactReference) -> Result<CodeNativeSideContext> {
    let side: CodeNativeSideContext = serde_json::from_value(reference_json(root, reference)?)?;
    inspect_side(root, &side.source_profile, &side)?;
    ensure!(
        reference_json(root, reference)? == serde_json::to_value(&side)?,
        "native_side_changed_during_inspection"
    );
    Ok(side)
}
/// Load the selected profile's recipe, without a second parser or owner.
pub fn load_recipe(root: &SkillRoot, profile: &ArtifactReference) -> Result<CodeNativeBuildRecipe> {
    let p = inspect_profile(root, profile)?;
    let (baseline, _) = crate::runtime_identity::inspect_profile(root, &p.baseline_profile)?;
    inspect_recipe(root, &p.recipe, &baseline)
}

pub struct NativeProfileInspector<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeRuntimeProfileInspectPort for NativeProfileInspector<'_> {
    fn inspect_native_profile(
        &self,
        input: &ArtifactReference,
    ) -> std::result::Result<CodeNativeRuntimeProfile, FailureRecord> {
        inspect_profile(self.root, input).map_err(|e| FailureRecord {
            code: ErrorCode::InvalidContract,
            phase: "native_profile_content_inspection".into(),
            retryable: false,
            partial_result: Some(json!({"inspection":"rejected","reason":format!("{e:#}"),"execution":"not_run","compilation_origin":"unknown","engineering_adoption":"not_established"})),
        })
    }
}

/// Shared CONTENT predicate only; no authority or actual-debug observation.
pub(crate) fn inspect_shared_expectations(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<CodeNativeDebugExpectations> {
    validate_expectations(root, reference)
}
