//! Explicit full native /6 mode of the existing five-stage builder.
//! Content capture never creates a second owner, registry or adoption policy.
use crate::native_runtime::{STAGES, deadline_timeout, epoch_ms, read, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::CommandSpec;
use oe_core::{SkillRoot, create_new_private_dir, write_new};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

pub(crate) fn role<'a>(
    recipe: &'a CodeNativeBuildRecipe6,
    role: &str,
) -> Result<&'a CodeNativeFileBinding> {
    recipe
        .tools
        .iter()
        .find(|t| t.role == role)
        .map(|t| &t.file)
        .context("native6_tool_role_missing")
}
pub(crate) fn verify_pins(
    root: &SkillRoot,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<CodeNativeSdkLayout4> {
    let pins: Vec<_> = recipe
        .tools
        .iter()
        .map(|t| &t.file)
        .chain(&recipe.external_archive_bindings)
        .collect();
    crate::native_pin_checks::verify_files(&pins)?;
    crate::native_sdk4::inspect_sdk4(root, &recipe.sdk_layout)
}
pub(crate) struct NativeBuildSession6 {
    pub root: SkillRoot,
    pub side_ref: ArtifactReference,
    pub side: CodeNativeSideContext6,
    pub profile: CodeNativeRuntimeProfile6,
    pub recipe: CodeNativeBuildRecipe6,
    pub deadline: Instant,
    pub deadline_epoch_ms: u64,
    pub output_limit: usize,
}
impl NativeBuildSession6 {
    fn new(input: &CodeNativeBudgetedBuildRequest6, deadline: Instant) -> Result<Self> {
        Self::new_parts(&input.build, &input.budget, &input.side_context, deadline)
    }
    /// Shared stage/process/budget data; this does not decode a new execution
    /// request as /6 or allocate a second elapsed-time owner.
    pub(crate) fn new_parts(
        build: &CodeBuildRequest,
        budget: &CodeBudget,
        side_ref: &ArtifactReference,
        deadline: Instant,
    ) -> Result<Self> {
        let root = SkillRoot::new(&build.source_root)?;
        // The original checked entry already jointly inspects side, profile
        // and recipe. Consume its current result within this constructor only.
        let (side, profile, recipe) =
            crate::native_profile6::load_compiler_entry(&root, side_ref)?.into_parts();
        ensure!(
            side.build == *build && side.body_budget == *budget,
            "native6_build_or_budget_changed"
        );
        verify_pins(&root, &recipe)?;
        let remaining = deadline_timeout(deadline)?;
        let deadline_epoch_ms = epoch_ms()?
            .checked_add(u64::try_from(remaining.as_millis())?)
            .context("native6_deadline_overflow")?;
        create_new_private_dir(&root.private_path(Path::new(&side.stage_root))?)?;
        Ok(Self {
            root,
            side_ref: side_ref.clone(),
            side,
            profile,
            recipe,
            deadline,
            deadline_epoch_ms,
            output_limit: usize::try_from(budget.output_bytes)?,
        })
    }
    pub(crate) fn configure_cargo(&self, spec: &mut CommandSpec, parent_stage: &str) -> Result<()> {
        ensure!(
            ["build-normalizer", "build-runner"].contains(&parent_stage),
            "native6_parent_stage_invalid"
        );
        deadline_timeout(self.deadline)?;
        verify_pins(&self.root, &self.recipe)?;
        ensure!(
            read(&self.root, &self.side_ref)? == serde_json::to_value(&self.side)?,
            "native6_side_changed"
        );
        let stage = Path::new(&self.side.stage_root).join(parent_stage);
        create_new_private_dir(&self.root.private_path(&stage)?)?;
        let temporary = self.root.new_private_dir(
            &Path::new(&self.side.build.target_dir)
                .join("native-tmp")
                .join(parent_stage),
        )?;
        let context_path = stage.join("stage-context.json");
        let compilation_sources = crate::native_sources6::prepare(self, parent_stage)?;
        let context = CodeNativeStageContext6 {
            schema: "oev2.native-build-stage/6".into(),
            source_root: self.root.path().to_str().context("native6_utf8")?.into(),
            source_manifest: public_manifest(self.root.path())?,
            side_context: self.side_ref.clone(),
            recipe: self.profile.recipe.clone(),
            parent_stage: parent_stage.into(),
            deadline_epoch_ms: self.deadline_epoch_ms,
            output_bytes: self.output_limit as u64,
            invocation_root: stage.to_str().context("native6_utf8")?.into(),
            temporary_root: temporary.to_str().context("native6_utf8")?.into(),
            compilation_sources,
        };
        context
            .validate()
            .map_err(|e| anyhow::anyhow!("native6_stage_contract:{e:?}"))?;
        write_new(&context_path, &serde_json::to_value(&context)?)?;
        let loader =
            crate::native_loader6::inspect_policy(&self.root, &self.recipe.cargo_loader_policy)?;
        create_new_private_dir(&stage.join("loader-events"))?;
        spec.environment.extend(BTreeMap::from([
            (
                "CARGO_ENCODED_RUSTFLAGS".into(),
                self.recipe.raw_encoded_rustflags.clone().into(),
            ),
            (
                "OE_NATIVE_STAGE_CONTEXT".into(),
                context_path.as_os_str().into(),
            ),
            (
                "OE_NATIVE_STAGE_CONTEXT_SHA256".into(),
                reference(&self.root, &context_path)?.sha256.into(),
            ),
            ("TMPDIR".into(), temporary.as_os_str().into()),
            (
                "RUSTC_WRAPPER".into(),
                loader.rustc_wrapper.path.clone().into(),
            ),
            (
                "CARGO_PROFILE_DEV_DEBUG".into(),
                self.recipe.debug.debug.to_string().into(),
            ),
            (
                "CARGO_PROFILE_DEV_SPLIT_DEBUGINFO".into(),
                self.recipe.debug.split_debuginfo.clone().into(),
            ),
            (
                "CARGO_PROFILE_DEV_STRIP".into(),
                self.recipe.debug.strip.clone().into(),
            ),
            (
                "CARGO_PROFILE_DEV_CODEGEN_UNITS".into(),
                self.recipe.debug.codegen_units.to_string().into(),
            ),
            (
                "CARGO_PROFILE_DEV_INCREMENTAL".into(),
                self.recipe.debug.incremental.to_string().into(),
            ),
            (
                "CARGO_PROFILE_DEV_LTO".into(),
                self.recipe.debug.lto.to_string().into(),
            ),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("LC_ALL".into(), "C".into()),
        ]));
        // Outer Cargo receives only explicitly selected values. Its existing
        // process tree and monotonic timeout remain the authority for children.
        let mut removed = std::env::vars_os()
            .map(|(k, _)| k)
            .chain(spec.remove_environment.iter().cloned())
            .filter(|k| !spec.environment.contains_key(k))
            .collect::<Vec<_>>();
        removed.sort();
        removed.dedup();
        spec.remove_environment = removed;
        spec.timeout = spec.timeout.min(deadline_timeout(self.deadline)?);
        crate::native_runtime::freeze_command(
            &self.root,
            spec,
            &Path::new(&self.side.build.evidence_path).join(parent_stage),
            self.deadline_epoch_ms,
        )?;
        Ok(())
    }
    pub(crate) fn bind_artifact(&self, a: &mut CodeSourceArtifact) -> Result<()> {
        deadline_timeout(self.deadline)?;
        verify_pins(&self.root, &self.recipe)?;
        ensure!(
            a.compiled["rustflags_sha256"] == self.recipe.raw_encoded_rustflags_sha256,
            "native6_compiled_flags_changed"
        );
        a.profile = crate::native_profile6::PROFILE.into();
        a.generation["native_build"] = native_build_facts(
            &self.root,
            &self.side_ref,
            &self.side,
            &self.profile,
            &self.recipe,
        )?;
        a.uncovered.extend(["Full native6 execution facts do not confer build owner or engineering adoption authority".into(),
            "Complete whole Semantica/platform scope remains independently unqualified".into()]);
        Ok(())
    }
}
pub(crate) fn native_build_facts(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    profile: &CodeNativeRuntimeProfile6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Value> {
    let mut contexts = Vec::new();
    let mut commands = Vec::new();
    for stage in ["build-normalizer", "build-runner"] {
        contexts.push(reference(
            root,
            &Path::new(&side.stage_root)
                .join(stage)
                .join("stage-context.json"),
        )?);
        commands.push(reference(
            root,
            &Path::new(&side.build.evidence_path)
                .join(stage)
                .with_extension("command.json"),
        )?);
    }
    Ok(
        json!({"schema":"oev2.native-build-facts/6","source_profile":side.source_profile,"side_context":side_ref,
        "recipe":profile.recipe,"raw_encoded_rustflags":recipe.raw_encoded_rustflags,
        "raw_encoded_rustflags_sha256":recipe.raw_encoded_rustflags_sha256,"stage_contexts":contexts,"cargo_commands":commands}),
    )
}
pub fn build_full_native_budgeted(
    input: &CodeNativeBudgetedBuildRequest6,
) -> Result<CodeNativeBuildCapture6> {
    let started = Instant::now();
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("native6_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.native-budgeted-build-request/6"
            && (1..=3_600_000).contains(&input.budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&input.budget.output_bytes),
        "native6_request_schema_or_bounds"
    );
    let deadline = started
        .checked_add(Duration::from_millis(input.budget.timeout_ms))
        .context("native6_deadline_overflow")?;
    let session = NativeBuildSession6::new(input, deadline)?;
    ensure!(
        !crate::native_link7::is_adapter7(&session.recipe)?
            && !crate::native_link8::is_adapter8(&session.recipe)?,
        "native6_compiler7_adapter_requires_explicit7_dispatch"
    );
    let artifact = crate::build_with_deadline_mode(
        &input.build,
        Some(deadline),
        session.output_limit,
        crate::BuildMode::Native6(&session),
    )?;
    let root = &session.root;
    let evidence = root.private_path(Path::new(&input.build.evidence_path))?;
    let artifact_ref = reference(root, &evidence.join("artifact.json"))?;
    let invocations = crate::native_link6::load_invocations(
        root,
        &session.side_ref,
        &session.side,
        &session.recipe,
    )?;
    ensure!(
        invocations
            .iter()
            .any(|x| x.parent_stage == "build-normalizer")
            && invocations.iter().any(|x| x.parent_stage == "build-runner"),
        "native6_stage_inventory_empty"
    );
    let loader_receipts =
        crate::native_loader6::prepare_retained(root, &session.side, &session.recipe)?;
    let images = crate::native_profile6::load_debug_images(root, &session.side.source_profile)?;
    create_new_private_dir(&evidence.join("native-debug"))?;
    let mut debug_images = Vec::new();
    for image in &images.images {
        let observation = crate::native_debug::observe_loader6(
            &session,
            &artifact_ref,
            &artifact,
            &invocations,
            image,
            &evidence.join("native-debug").join(&image.image_role),
        )?;
        let p = evidence.join(format!(
            "native-debug-{}-observation.json",
            image.image_role
        ));
        write_new(&p, &serde_json::to_value(&observation)?)?;
        debug_images.push(reference(root, &p)?);
    }
    let refs = STAGES
        .iter()
        .map(|s| reference(root, &evidence.join(s).join("process.json")))
        .collect::<Result<Vec<_>>>()?;
    let own = &crate::driver_root().context("native6_driver_root")?;
    let capture = CodeNativeBuildCapture6 {
        schema: "oev2.native-build-capture/6".into(),
        source_profile: session.side.source_profile.clone(),
        artifact: artifact_ref.clone(),
        driver: crate::binary_reference(&std::env::current_exe()?, own)?,
        normalizer_process: refs[0].clone(),
        generation_process: refs[1].clone(),
        runner_build_process: refs[2].clone(),
        drift_process: refs[3].clone(),
        identity_process: refs[4].clone(),
        side_context: input.side_context.clone(),
        build_script_metadata_bindings: crate::native_buildscript6::retained(
            root,
            &session.side,
            &session.recipe,
        )?,
        process_environment_bindings: crate::native_process6::binding_references(
            root,
            &invocations,
        )?,
        invocations,
        debug_images,
        loader_receipts,
    };
    verify_full_native_capture(
        root,
        &session.side.source_profile,
        &input.side_context,
        &artifact_ref,
        &capture,
    )?;
    deadline_timeout(deadline)?;
    write_new(
        &evidence.join("native-loader6-capture.json"),
        &serde_json::to_value(&capture)?,
    )?;
    Ok(capture)
}
pub(crate) fn verify_cargo_commands(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<[Value; 2]> {
    verify_cargo_commands_with_environment(root, side_ref, side, recipe, None)
}
/// A caller's explicit execution schema supplies a selected extra environment
/// entry. Original6/7 use None and retain the exact original environment map.
pub(crate) fn verify_cargo_commands_with_environment(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    extra: Option<&CodeNativeEnvironmentEntry>,
) -> Result<[Value; 2]> {
    verify_cargo_commands_mode(root, side_ref, side, recipe, extra, false)
}

pub(crate) fn verify_cargo_commands11(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    extra: &CodeNativeEnvironmentEntry,
) -> Result<[Value; 2]> {
    verify_cargo_commands_mode(root, side_ref, side, recipe, Some(extra), true)
}

fn verify_cargo_commands_mode(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    extra: Option<&CodeNativeEnvironmentEntry>,
    stream11: bool,
) -> Result<[Value; 2]> {
    verify_cargo_stages(
        root,
        side_ref,
        side,
        recipe,
        extra,
        stream11,
        ["build-normalizer", "build-runner"],
    )
}

pub(crate) fn verify_normalizer_command11(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    extra: &CodeNativeEnvironmentEntry,
) -> Result<()> {
    verify_cargo_stages(
        root,
        side_ref,
        side,
        recipe,
        Some(extra),
        true,
        ["build-normalizer"],
    )?;
    Ok(())
}

fn verify_cargo_stages<const N: usize>(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    extra: Option<&CodeNativeEnvironmentEntry>,
    stream11: bool,
    stages: [&str; N],
) -> Result<[Value; N]> {
    let mut environments = Vec::new();
    for stage in stages {
        let context = crate::native_link6::validate_context(root, side_ref, side, recipe, stage)?;
        let command = read(
            root,
            &reference(
                root,
                &Path::new(&side.build.evidence_path)
                    .join(stage)
                    .with_extension("command.json"),
            )?,
        )?;
        let evidence_stage = Path::new(&side.build.evidence_path).join(stage);
        let process = if stream11 {
            let content = crate::native_stream11::inspect_capture_content11(
                root,
                &reference(root, &evidence_stage.join("stream-capture11.json"))?,
            )?;
            let plan: CodeNativeCargoStreamPlan11 =
                serde_json::from_value(read(root, &content.capture().plan)?)?;
            ensure!(
                plan.side_context == *side_ref
                    && plan.parent_stage == stage
                    && plan.body_output_bytes == side.body_budget.output_bytes
                    && plan.body_timeout_ms == side.body_budget.timeout_ms
                    && Some(plan.deadline_epoch_ms) == context["deadline_epoch_ms"].as_u64(),
                "stream11_cargo_original_context_changed"
            );
            read(root, &content.capture().process)?
        } else {
            let process_ref = reference(root, &evidence_stage.join("process.json"))?;
            let process = read(root, &process_ref)?;
            crate::native_runtime::verify_frozen_command_budget(
                root,
                &process_ref,
                side.body_budget.output_bytes,
                side.body_budget.timeout_ms,
                Some(
                    context["deadline_epoch_ms"]
                        .as_u64()
                        .context("native6_deadline")?,
                ),
            )?;
            process
        };
        let values = command["environment"]
            .as_object()
            .context("native6_cargo_environment")?;
        let mut expected = BTreeMap::from([
            (
                "CARGO_ENCODED_RUSTFLAGS",
                json!(recipe.raw_encoded_rustflags),
            ),
            ("CARGO_HOME", json!(side.build.cargo_home)),
            ("RUSTC", json!(side.build.rustc_path)),
            (
                "RUSTC_WRAPPER",
                json!(
                    crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?
                        .rustc_wrapper
                        .path
                ),
            ),
            ("CARGO_TARGET_DIR", json!(side.build.target_dir)),
            (
                "OE_NATIVE_STAGE_CONTEXT",
                json!(
                    Path::new(&side.stage_root)
                        .join(stage)
                        .join("stage-context.json")
                ),
            ),
            (
                "OE_NATIVE_STAGE_CONTEXT_SHA256",
                json!(
                    reference(
                        root,
                        &Path::new(&side.stage_root)
                            .join(stage)
                            .join("stage-context.json")
                    )?
                    .sha256
                ),
            ),
            ("TMPDIR", context["temporary_root"].clone()),
            ("PATH", json!("/usr/bin:/bin")),
            ("LC_ALL", json!("C")),
        ]);
        for (key, val) in [
            ("DEBUG", recipe.debug.debug.to_string()),
            ("SPLIT_DEBUGINFO", recipe.debug.split_debuginfo.clone()),
            ("STRIP", recipe.debug.strip.clone()),
            ("CODEGEN_UNITS", recipe.debug.codegen_units.to_string()),
            ("INCREMENTAL", recipe.debug.incremental.to_string()),
            ("LTO", recipe.debug.lto.to_string()),
        ] {
            // The names are fixed by the original Cargo profile contract.
            let key = match key {
                "DEBUG" => "CARGO_PROFILE_DEV_DEBUG",
                "SPLIT_DEBUGINFO" => "CARGO_PROFILE_DEV_SPLIT_DEBUGINFO",
                "STRIP" => "CARGO_PROFILE_DEV_STRIP",
                "CODEGEN_UNITS" => "CARGO_PROFILE_DEV_CODEGEN_UNITS",
                "INCREMENTAL" => "CARGO_PROFILE_DEV_INCREMENTAL",
                _ => "CARGO_PROFILE_DEV_LTO",
            };
            expected.insert(key, json!(val));
        }
        if let Some(entry) = extra {
            entry
                .validate()
                .map_err(|e| anyhow::anyhow!("native6_extra_environment:{e:?}"))?;
            ensure!(
                !expected.contains_key(entry.name.as_str()),
                "native6_extra_environment_overrides_original"
            );
            expected.insert(entry.name.as_str(), json!(entry.value));
        }
        ensure!(
            command["schema"] == "oev2.native-command-before-dispatch/1"
                && serde_json::to_value(&expected)? == command["environment"]
                && command["cwd"] == root.path().to_str().context("native6_path")?
                && command["executable"]["path"] == side.build.cargo_path
                && command["arguments"] == process["arguments"]
                && command["timeout_seconds"] == process["timeout_seconds"]
                && command["deadline_epoch_ms"] == context["deadline_epoch_ms"]
                && command["output_bytes"] == side.body_budget.output_bytes
                && command["timeout_seconds"].as_f64().is_some_and(
                    |t| t > 0.0 && t * 1000.0 <= side.body_budget.timeout_ms as f64 + 0.001
                ),
            "native6_cargo_command_changed"
        );
        let removed = command["removed_environment"]
            .as_array()
            .context("native6_cargo_removed")?;
        let mut names = std::collections::BTreeSet::new();
        ensure!(
            removed.iter().all(|v| v
                .as_str()
                .is_some_and(|s| !values.contains_key(s) && names.insert(s))),
            "native6_cargo_removed_invalid"
        );
        environments
            .push(json!({"added_keys":values.keys().collect::<Vec<_>>(),"removed_keys":removed}));
    }
    Ok(environments
        .try_into()
        .map_err(|_| anyhow::anyhow!("native6_cargo_stage_count"))?)
}
pub fn verify_full_native_capture(
    root: &SkillRoot,
    source_profile: &ArtifactReference,
    side_ref: &ArtifactReference,
    artifact_ref: &ArtifactReference,
    c: &CodeNativeBuildCapture6,
) -> Result<()> {
    let loader_refs = crate::native_loader6::verify_retained(
        root,
        &crate::native_profile6::load_side(root, side_ref)?,
        &crate::native_profile6::load_recipe(root, source_profile)?,
    )?;
    ensure!(
        c.loader_receipts == loader_refs,
        "native6_loader_receipts_changed"
    );
    c.validate()
        .map_err(|e| anyhow::anyhow!("native6_capture:{e:?}"))?;
    ensure!(
        c.schema == "oev2.native-build-capture/6"
            && c.source_profile == *source_profile
            && c.side_context == *side_ref
            && c.artifact == *artifact_ref,
        "native6_capture_binding_changed"
    );
    let side = crate::native_profile6::load_side(root, side_ref)?;
    ensure!(
        side.source_profile == *source_profile,
        "native6_capture_profile_changed"
    );
    let profile = crate::native_profile6::inspect_profile(root, source_profile)?;
    let recipe = crate::native_profile6::load_recipe(root, source_profile)?;
    let a: CodeSourceArtifact = serde_json::from_value(read(root, artifact_ref)?)?;
    crate::verify_artifact(&a)?;
    let (baseline, wanted) =
        crate::runtime_identity::inspect_profile(root, &profile.baseline_profile)?;
    ensure!(
        a.manifest == wanted
            && a.compiled["package_version"] == baseline.expected_package_version
            && a.compiled["target"] == baseline.expected_target
            && a.compiled["language"] == "rust"
            && a.generation["receipt"]["normalizer_binding"]["target"] == baseline.expected_target,
        "native6_actual_compiled_identity_mismatch"
    );
    crate::verify_binary(&c.driver)?;
    let evidence = root.private_path(Path::new(&side.build.evidence_path))?;
    let target = root.private_path(Path::new(&side.build.target_dir))?;
    ensure!(
        artifact_ref == &reference(root, &evidence.join("artifact.json"))?
            && a.artifact_id == side.build.artifact_id
            && a.source_root == side.build.source_root
            && a.source_root == root.path().to_str().context("native6_path")?
            && a.platform == side.build.platform
            && a.manifest == public_manifest(root.path())?
            && a.runner.path
                == target
                    .join(&side.build.platform)
                    .join("debug/oe-code-runner")
                    .to_str()
                    .context("native6_path")?
            && a.tools.len() == 4
            && a.tools[0].path == side.build.cargo_path
            && a.tools[1].path == side.build.rustc_path
            && a.tools[2].path == side.build.ggen_path
            && Path::new(&a.tools[3].path) == target.join("debug/oev2-design-bootstrap")
            && a.profile == crate::native_profile6::PROFILE
            && a.build_processes.len() == 4
            && a.compiled["rustflags_sha256"] == recipe.raw_encoded_rustflags_sha256
            && a.generation["fixed_build_driver"]["executable_sha256"] == c.driver.sha256
            && a.generation["fixed_build_driver"]["source_manifest_sha256"]
                == c.driver.source_manifest_sha256,
        "native6_authorized_artifact_changed"
    );
    ensure!(
        a.generation["native_build"]
            == native_build_facts(root, side_ref, &side, &profile, &recipe)?,
        "native6_artifact_recipe_changed"
    );
    let environments = verify_cargo_commands(root, side_ref, &side, &recipe)?;
    crate::native_runtime::verify_original_stages_core(
        root,
        &side.build,
        &side.body_budget,
        &a,
        [
            &c.normalizer_process,
            &c.generation_process,
            &c.runner_build_process,
            &c.drift_process,
            &c.identity_process,
        ],
        Some(&environments),
    )?;
    let actual = crate::native_link6::load_invocations(root, side_ref, &side, &recipe)?;
    ensure!(
        actual == c.invocations
            && crate::native_buildscript6::retained(root, &side, &recipe)?
                == c.build_script_metadata_bindings
            && crate::native_process6::binding_references(root, &actual)?
                == c.process_environment_bindings,
        "native6_invocation_inventory_changed"
    );
    let images = crate::native_profile6::load_debug_images(root, source_profile)?;
    ensure!(
        c.debug_images.len() == 2 && images.images.len() == 2,
        "native6_debug_image_inventory"
    );
    for (image, observation) in images.images.iter().zip(&c.debug_images) {
        crate::native_debug::verify_loader6(
            root,
            side_ref,
            &side,
            &recipe,
            artifact_ref,
            &a,
            &c.invocations,
            image,
            observation,
        )?;
    }
    verify_pins(root, &recipe)?;
    Ok(())
}
pub struct NativeRuntimeBuilder6;
pub fn native_full_link_entry() -> Result<i32> {
    native_full_link_arguments(std::env::args().skip(1).collect())
}
pub fn native_full_link_arguments(args: Vec<String>) -> Result<i32> {
    crate::native_link6::entry(args)
}
impl CodeNativeBudgetedBuild6Port for NativeRuntimeBuilder6 {
    fn build_full_native_budgeted_runtime(
        &self,
        input: &CodeNativeBudgetedBuildRequest6,
    ) -> std::result::Result<CodeNativeBuildCapture6, FailureRecord> {
        build_full_native_budgeted(input).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,phase:"native_full_budgeted_build".into(),
            retryable:false,partial_result:Some(json!({"execution":"failed","reason":format!("{e:#}"),"compilation_origin":"unknown","release_eligibility":"unknown"}))})
    }
}
