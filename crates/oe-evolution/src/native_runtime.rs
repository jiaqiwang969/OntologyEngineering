//! Native execution uses the original builder pipeline and bounded process owner.
//! Capture verification checks content; it never confers registry authority.
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::CommandSpec;
use oe_core::{
    SkillRoot, create_new_private_dir, fingerprint_file, no_symlinks, read_regular_bytes, sha256,
    write_new,
};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const JSON_LIMIT: usize = 16 * 1024 * 1024;
pub(crate) const STAGES: [&str; 5] = [
    "build-normalizer",
    "actual-ggen",
    "build-runner",
    "actual-ggen-check",
    "runner-identity",
];
pub(crate) fn reference(root: &SkillRoot, path: &Path) -> Result<ArtifactReference> {
    let path = root.private_path(path)?;
    let id = fingerprint_file(&path, JSON_LIMIT as u64)?;
    Ok(ArtifactReference {
        path: path.to_str().context("native_path_utf8")?.into(),
        sha256: id.sha256,
    })
}
pub(crate) fn read(root: &SkillRoot, r: &ArtifactReference) -> Result<Value> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("native_reference:{e:?}"))?;
    let raw = read_regular_bytes(&root.private_path(Path::new(&r.path))?, JSON_LIMIT)?;
    ensure!(sha256(&raw) == r.sha256, "native_reference_changed");
    Ok(strict_json(&raw)?)
}
pub(crate) fn file_binding(path: &Path) -> Result<CodeNativeFileBinding> {
    no_symlinks(path)?;
    let id = fingerprint_file(path, 512 * 1024 * 1024)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        u64::from(std::fs::symlink_metadata(path)?.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let mode = { anyhow::bail!("native_platform_uncovered") };
    Ok(CodeNativeFileBinding {
        path: path
            .canonicalize()?
            .to_str()
            .context("native_path_utf8")?
            .into(),
        sha256: id.sha256,
        bytes: id.bytes,
        mode,
    })
}
pub(crate) fn verify_pins(recipe: &CodeNativeBuildRecipe) -> Result<()> {
    for pin in recipe
        .tools
        .iter()
        .map(|t| &t.file)
        .chain(&recipe.sdk_bindings)
        .chain(&recipe.external_archive_bindings)
    {
        crate::native_profile::verify_file(pin)?;
    }
    Ok(())
}
pub(crate) fn preserve_explicit_environment(spec: &mut CommandSpec) {
    spec.remove_environment
        .retain(|key| !spec.environment.contains_key(key));
}
pub(crate) fn epoch_ms() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
pub(crate) fn deadline_timeout(deadline: Instant) -> Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    ensure!(!remaining.is_zero(), "native_body_deadline_exhausted");
    Ok(remaining)
}
pub(crate) fn role<'a>(
    recipe: &'a CodeNativeBuildRecipe,
    role: &str,
) -> Result<&'a CodeNativeFileBinding> {
    recipe
        .tools
        .iter()
        .find(|t| t.role == role)
        .map(|t| &t.file)
        .context("native_tool_role_missing")
}

pub(crate) struct NativeBuildSession {
    pub root: SkillRoot,
    pub side_ref: ArtifactReference,
    pub side: CodeNativeSideContext,
    pub profile: CodeNativeRuntimeProfile,
    pub recipe: CodeNativeBuildRecipe,
    pub deadline: Instant,
    pub deadline_epoch_ms: u64,
    pub output_limit: usize,
}
impl NativeBuildSession {
    fn new(input: &CodeNativeBudgetedBuildRequest, deadline: Instant) -> Result<Self> {
        let root = SkillRoot::new(&input.build.source_root)?;
        let side = crate::native_profile::load_side(&root, &input.side_context)?;
        ensure!(
            side.build == input.build && side.body_budget == input.budget,
            "native_authorized_build_or_budget_changed"
        );
        let profile = crate::native_profile::inspect_profile(&root, &side.source_profile)?;
        let recipe = crate::native_profile::load_recipe(&root, &side.source_profile)?;
        let remaining = deadline_timeout(deadline)?;
        let deadline_epoch_ms = epoch_ms()?
            .checked_add(u64::try_from(remaining.as_millis())?)
            .context("native_deadline_overflow")?;
        // The owner chooses this exact fresh sibling. The builder never reuses
        // another side's objects, response expansion, archive index or observer.
        let stage = root.private_path(Path::new(&side.stage_root))?;
        create_new_private_dir(&stage)?;
        Ok(Self {
            root,
            side_ref: input.side_context.clone(),
            side,
            profile,
            recipe,
            deadline,
            deadline_epoch_ms,
            output_limit: usize::try_from(input.budget.output_bytes)?,
        })
    }
    pub(crate) fn configure_cargo(&self, spec: &mut CommandSpec, parent_stage: &str) -> Result<()> {
        ensure!(
            ["build-normalizer", "build-runner"].contains(&parent_stage),
            "native_parent_stage_invalid"
        );
        deadline_timeout(self.deadline)?;
        verify_pins(&self.recipe)?;
        ensure!(
            read(&self.root, &self.side_ref)? == serde_json::to_value(&self.side)?,
            "native_side_changed"
        );
        let stage = Path::new(&self.side.stage_root).join(parent_stage);
        create_new_private_dir(&stage)?;
        let stage_context = stage.join("stage-context.json");
        let temporary_root = self.root.new_private_dir(
            &Path::new(&self.side.build.target_dir)
                .join("native-tmp")
                .join(parent_stage),
        )?;
        let context = json!({"schema":"oev2.native-build-stage/1", "source_root":self.root.path(), "source_manifest":public_manifest(self.root.path())?,
            "side_context":self.side_ref, "recipe":self.profile.recipe, "parent_stage":parent_stage,
            "deadline_epoch_ms":self.deadline_epoch_ms, "output_bytes":self.output_limit, "invocation_root":stage, "temporary_root":temporary_root});
        write_new(&stage_context, &context)?;
        spec.environment.extend(BTreeMap::from([
            (
                "CARGO_ENCODED_RUSTFLAGS".into(),
                self.recipe.raw_encoded_rustflags.clone().into(),
            ),
            (
                "OE_NATIVE_STAGE_CONTEXT".into(),
                stage_context.as_os_str().into(),
            ),
            (
                "OE_NATIVE_STAGE_CONTEXT_SHA256".into(),
                reference(&self.root, &stage_context)?.sha256.into(),
            ),
            ("TMPDIR".into(), temporary_root.as_os_str().into()),
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
        ]));
        // Existing run_command applies removals after its explicit environment.
        // Exclude explicit native keys here; leave its legacy semantics intact.
        preserve_explicit_environment(spec);
        Ok(())
    }
    pub(crate) fn bind_artifact(&self, artifact: &mut CodeSourceArtifact) -> Result<()> {
        deadline_timeout(self.deadline)?;
        verify_pins(&self.recipe)?;
        ensure!(
            artifact.compiled["rustflags_sha256"] == self.recipe.raw_encoded_rustflags_sha256,
            "native_compiled_raw_flags_changed"
        );
        let contexts = ["build-normalizer", "build-runner"]
            .into_iter()
            .map(|s| {
                reference(
                    &self.root,
                    &Path::new(&self.side.stage_root)
                        .join(s)
                        .join("stage-context.json"),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        artifact.profile = crate::native_profile::PROFILE.into();
        artifact.generation["native_build"] = json!({"schema":"oev2.native-build-facts/2", "source_profile":self.side.source_profile,
            "side_context":self.side_ref, "recipe":self.profile.recipe, "raw_encoded_rustflags":self.recipe.raw_encoded_rustflags,
            "raw_encoded_rustflags_sha256":self.recipe.raw_encoded_rustflags_sha256, "stage_contexts":contexts});
        artifact.uncovered.extend(["Native compiler/dependency/SDK and platform closure remains scoped, not whole Semantica acceptance".into(), "Finite independently frozen debug predicates do not cover all runtime behavior".into()]);
        Ok(())
    }
}

pub fn build_native_budgeted(
    input: &CodeNativeBudgetedBuildRequest,
) -> Result<CodeNativeBuildCapture> {
    let started = Instant::now();
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("native_build_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.native-budgeted-build-request/2"
            && (1..=3_600_000).contains(&input.budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&input.budget.output_bytes),
        "native_build_request_bounds"
    );
    let deadline = started
        .checked_add(Duration::from_millis(input.budget.timeout_ms))
        .context("native_deadline_overflow")?;
    let session = NativeBuildSession::new(input, deadline)?;
    let artifact = crate::build_with_deadline_mode(
        &input.build,
        Some(deadline),
        session.output_limit,
        crate::BuildMode::Native(&session),
    )?;
    let root = &session.root;
    let evidence = root.private_path(Path::new(&input.build.evidence_path))?;
    let artifact_ref = reference(root, &evidence.join("artifact.json"))?;
    let invocations = crate::native_link::load_invocations(
        root,
        &session.side_ref,
        &session.side,
        &session.recipe,
    )?;
    ensure!(
        invocations
            .iter()
            .any(|v| v.parent_stage == "build-normalizer")
            && invocations.iter().any(|v| v.parent_stage == "build-runner"),
        "native_compilation_inventory_empty"
    );
    let debug = crate::native_debug::observe(
        &session,
        &artifact_ref,
        &artifact,
        &invocations,
        &evidence.join("native-debug"),
    )?;
    let debug_path = evidence.join("native-debug-observation.json");
    write_new(&debug_path, &serde_json::to_value(&debug)?)?;
    let refs = STAGES
        .iter()
        .map(|s| reference(root, &evidence.join(s).join("process.json")))
        .collect::<Result<Vec<_>>>()?;
    let driver_root = &crate::driver_root().context("native_driver_root")?;
    let capture = CodeNativeBuildCapture {
        schema: "oev2.native-build-capture/2".into(),
        source_profile: session.side.source_profile.clone(),
        artifact: artifact_ref.clone(),
        driver: crate::binary_reference(&std::env::current_exe()?, driver_root)?,
        normalizer_process: refs[0].clone(),
        generation_process: refs[1].clone(),
        runner_build_process: refs[2].clone(),
        drift_process: refs[3].clone(),
        identity_process: refs[4].clone(),
        side_context: input.side_context.clone(),
        invocations,
        debug: reference(root, &debug_path)?,
    };
    verify_native_capture(
        root,
        &session.side.source_profile,
        &input.side_context,
        &artifact_ref,
        &capture,
    )?;
    deadline_timeout(deadline)?;
    write_new(
        &evidence.join("native-capture.json"),
        &serde_json::to_value(&capture)?,
    )?;
    Ok(capture)
}

/// Freeze exact native command values before dispatch. This projection cannot
/// replace the enclosing Cargo owner's monotonic deadline or PID subtree.
pub(crate) fn freeze_command(
    root: &SkillRoot,
    spec: &mut CommandSpec,
    folder: &Path,
    deadline_epoch_ms: u64,
) -> Result<()> {
    let prepared = epoch_ms()?;
    let remaining = deadline_epoch_ms
        .checked_sub(prepared)
        .context("native_body_deadline_exhausted")?;
    ensure!(remaining > 0, "native_body_deadline_exhausted");
    spec.timeout = spec.timeout.min(Duration::from_millis(remaining));
    let values = spec
        .environment
        .iter()
        .map(|(k, v)| {
            Ok((
                k.to_str().context("native_env_utf8")?.to_owned(),
                v.to_str().context("native_env_utf8")?.to_owned(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let removed = spec
        .remove_environment
        .iter()
        .map(|k| k.to_str().map(str::to_owned).context("native_env_utf8"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        removed.iter().all(|key| !values.contains_key(key)),
        "native_explicit_environment_removed"
    );
    write_new(
        &root.private_path(&folder.with_extension("command.json"))?,
        &json!({"schema":"oev2.native-command-before-dispatch/1","executable":file_binding(&spec.executable)?,
        "arguments":spec.arguments.iter().map(|s|s.to_str().context("native_argv_utf8")).collect::<Result<Vec<_>>>()?,"cwd":spec.cwd,"environment":values,"removed_environment":removed,
        "timeout_seconds":spec.timeout.as_secs_f64(),"output_bytes":spec.output_limit,"prepared_epoch_ms":prepared,"deadline_epoch_ms":deadline_epoch_ms}),
    )?;
    Ok(())
}
pub(crate) fn verify_frozen_command(
    root: &SkillRoot,
    process: &ArtifactReference,
    output_bytes: u64,
    budget_ms: u64,
    deadline: Option<u64>,
) -> Result<()> {
    let (frozen, raw) =
        verify_frozen_command_budget(root, process, output_bytes, budget_ms, deadline)?;
    let environment = frozen["environment"]
        .as_object()
        .context("native_command_environment")?;
    let added = environment.keys().collect::<Vec<_>>();
    let removed = frozen["removed_environment"]
        .as_array()
        .context("native_command_removed")?;
    ensure!(
        raw["environment_added_keys"] == serde_json::to_value(added)?
            && raw["environment_removed_keys"] == serde_json::to_value(removed)?
            && environment.get("LC_ALL") == Some(&json!("C"))
            && environment.get("PATH") == Some(&json!("/usr/bin:/bin"))
            && environment.len() == 2
            && removed
                .iter()
                .all(|key| key.as_str().is_some_and(|k| !environment.contains_key(k))),
        "native_command_environment_changed"
    );
    Ok(())
}
/// The exact raw command and remaining budget checks shared by the strict
/// two-key native observer and the explicitly frozen full3 Cargo environment.
pub(crate) fn verify_frozen_command_budget(
    root: &SkillRoot,
    process: &ArtifactReference,
    output_bytes: u64,
    budget_ms: u64,
    deadline: Option<u64>,
) -> Result<(Value, Value)> {
    let folder = Path::new(&process.path)
        .parent()
        .context("native_process_parent")?;
    let frozen = read(
        root,
        &reference(root, &folder.with_extension("command.json"))?,
    )?;
    let raw = read(root, process)?;
    let time = frozen["timeout_seconds"]
        .as_f64()
        .context("native_command_timeout")?;
    let prepared = frozen["prepared_epoch_ms"]
        .as_u64()
        .context("native_command_prepared")?;
    let end = frozen["deadline_epoch_ms"]
        .as_u64()
        .context("native_command_deadline")?;
    ensure!(
        frozen["schema"] == "oev2.native-command-before-dispatch/1"
            && time.is_finite()
            && time > 0.0
            && time * 1000.0 <= budget_ms as f64 + 0.001
            && end > prepared
            && time * 1000.0 <= (end - prepared) as f64 + 0.001
            && deadline.is_none_or(|d| d == end)
            && frozen["output_bytes"] == output_bytes
            && raw["timeout_seconds"] == frozen["timeout_seconds"]
            && raw["per_stream_limit"] == output_bytes
            && raw["executable"]["path"] == frozen["executable"]["path"]
            && raw["executable"]["identity"]["sha256"] == frozen["executable"]["sha256"]
            && raw["executable"]["identity"]["bytes"] == frozen["executable"]["bytes"]
            && raw["arguments"] == frozen["arguments"]
            && raw["cwd"] == frozen["cwd"],
        "native_command_budget_or_identity_changed"
    );
    Ok((frozen, raw))
}
/// Verify original raw process capture, not merely a fingerprinted declaration.
pub(crate) fn verify_process(
    root: &SkillRoot,
    r: &ArtifactReference,
    exe: &CodeNativeFileBinding,
    cwd: &Path,
    args: &[String],
) -> Result<(Vec<u8>, Vec<u8>)> {
    crate::native_profile::verify_file(exe)?;
    let v = read(root, r)?;
    ensure!(
        v["schema"] == "ontology-engineering.external-process/v1"
            && v["status"] == "completed"
            && v["exit_code"] == 0
            && v["observed_status"] == "completed"
            && v["capture_complete"] == true
            && v["streams_collected"] == 2
            && v["descendant_cleanup"]["state"] == "local_cleanup_completed"
            && v["descendant_cleanup"]["backend"] == "macos_libproc_observed_pid_birth_subtree"
            && v["process_tree"] == "unix_process_group"
            && v["semantic_execution"] == "not_inferred"
            && v["environment_values"] == "not_recorded"
            && v["executable"]["unchanged"] == true,
        "native_process_incomplete"
    );
    ensure!(
        v["executable"]["resolved"] == exe.path
            && v["executable"]["path"] == exe.path
            && v["executable"]["identity"]["sha256"] == exe.sha256
            && v["executable"]["identity"]["bytes"] == exe.bytes
            && v["cwd"] == cwd.to_str().context("native_path_utf8")?
            && v["arguments"] == serde_json::to_value(args)?,
        "native_process_invocation_changed"
    );
    let cap = v["per_stream_limit"]
        .as_u64()
        .context("native_process_cap_missing")?;
    let timeout = v["timeout_seconds"]
        .as_f64()
        .context("native_process_timeout_missing")?;
    ensure!(
        (1..=4 * 1024 * 1024).contains(&cap)
            && timeout.is_finite()
            && timeout > 0.0
            && timeout <= 3600.0
            && v["elapsed_seconds"]
                .as_f64()
                .is_some_and(|t| t.is_finite() && t >= 0.0),
        "native_process_budget_invalid"
    );
    let folder = Path::new(&r.path)
        .parent()
        .context("native_process_parent")?;
    let mut streams = Vec::new();
    for name in ["stdout", "stderr"] {
        ensure!(
            v[name]["path"] == name && v[name]["truncated"] == false,
            "native_process_stream_incomplete"
        );
        let raw = read_regular_bytes(&root.private_path(&folder.join(name))?, 4 * 1024 * 1024)?;
        ensure!(
            raw.len() as u64 <= cap
                && v[name]["bytes"] == raw.len() as u64
                && v[name]["sha256"] == sha256(&raw),
            "native_process_stream_changed"
        );
        streams.push(raw);
    }
    Ok((streams.remove(0), streams.remove(0)))
}

pub fn verify_native_capture(
    root: &SkillRoot,
    source_profile: &ArtifactReference,
    side_ref: &ArtifactReference,
    artifact_ref: &ArtifactReference,
    capture: &CodeNativeBuildCapture,
) -> Result<()> {
    capture
        .validate()
        .map_err(|e| anyhow::anyhow!("native_capture:{e:?}"))?;
    ensure!(
        capture.schema == "oev2.native-build-capture/2"
            && capture.source_profile == *source_profile
            && capture.side_context == *side_ref
            && capture.artifact == *artifact_ref,
        "native_capture_binding_changed"
    );
    let side = crate::native_profile::load_side(root, side_ref)?;
    ensure!(
        side.source_profile == *source_profile,
        "native_capture_source_profile_changed"
    );
    let recipe = crate::native_profile::load_recipe(root, source_profile)?;
    let artifact: CodeSourceArtifact = serde_json::from_value(read(root, artifact_ref)?)?;
    crate::verify_artifact(&artifact)?;
    crate::verify_binary(&capture.driver)?;
    let evidence = root.private_path(Path::new(&side.build.evidence_path))?;
    let target = root.private_path(Path::new(&side.build.target_dir))?;
    ensure!(
        artifact_ref.path
            == evidence
                .join("artifact.json")
                .to_str()
                .context("native_path_utf8")?
            && artifact.artifact_id == side.build.artifact_id
            && artifact.source_root == side.build.source_root
            && artifact.source_root == root.path().to_str().context("native_path_utf8")?
            && artifact.platform == side.build.platform
            && artifact.manifest == public_manifest(root.path())?
            && artifact.runner.path
                == target
                    .join(&side.build.platform)
                    .join("debug/oe-code-runner")
                    .to_str()
                    .context("native_path_utf8")?
            && artifact
                .tools
                .get(0)
                .is_some_and(|t| t.path == side.build.cargo_path)
            && artifact
                .tools
                .get(1)
                .is_some_and(|t| t.path == side.build.rustc_path)
            && artifact
                .tools
                .get(2)
                .is_some_and(|t| t.path == side.build.ggen_path)
            && artifact
                .tools
                .get(3)
                .is_some_and(|t| Path::new(&t.path) == target.join("debug/oev2-design-bootstrap")),
        "native_authorized_artifact_changed"
    );
    let profile = crate::native_profile::inspect_profile(root, source_profile)?;
    let contexts = ["build-normalizer", "build-runner"]
        .into_iter()
        .map(|s| {
            reference(
                root,
                &Path::new(&side.stage_root)
                    .join(s)
                    .join("stage-context.json"),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        artifact.generation["native_build"]
            == json!({"schema":"oev2.native-build-facts/2","source_profile":source_profile,"side_context":side_ref,"recipe":profile.recipe,
        "raw_encoded_rustflags":recipe.raw_encoded_rustflags,"raw_encoded_rustflags_sha256":recipe.raw_encoded_rustflags_sha256,"stage_contexts":contexts}),
        "native_artifact_recipe_changed"
    );
    ensure!(
        artifact.profile == crate::native_profile::PROFILE
            && artifact.tools.len() == 4
            && artifact.build_processes.len() == 4
            && artifact.compiled["rustflags_sha256"] == recipe.raw_encoded_rustflags_sha256
            && artifact.generation["fixed_build_driver"]["executable_sha256"]
                == capture.driver.sha256
            && artifact.generation["fixed_build_driver"]["source_manifest_sha256"]
                == capture.driver.source_manifest_sha256,
        "native_capture_artifact_changed"
    );
    // Reuse the existing capture's exact five argv/stream expectations below;
    // do not feed native profile/2 to the original profile/1 parser.
    verify_original_stages(root, &side, &artifact, capture)?;
    let actual = crate::native_link::load_invocations(root, side_ref, &side, &recipe)?;
    ensure!(
        actual == capture.invocations,
        "native_capture_invocation_inventory_changed"
    );
    crate::native_debug::verify_observation(
        root,
        &side,
        &recipe,
        artifact_ref,
        &artifact,
        &capture.invocations,
        &capture.debug,
    )?;
    Ok(())
}

fn verify_original_stages(
    root: &SkillRoot,
    side: &CodeNativeSideContext,
    a: &CodeSourceArtifact,
    c: &CodeNativeBuildCapture,
) -> Result<()> {
    verify_original_stages_core(
        root,
        &side.build,
        &side.body_budget,
        a,
        [
            &c.normalizer_process,
            &c.generation_process,
            &c.runner_build_process,
            &c.drift_process,
            &c.identity_process,
        ],
        None,
    )
}

pub(crate) fn verify_original_stages_core(
    root: &SkillRoot,
    build: &CodeBuildRequest,
    budget: &CodeBudget,
    a: &CodeSourceArtifact,
    refs: [&ArtifactReference; 5],
    native3_cargo_environments: Option<&[Value; 2]>,
) -> Result<()> {
    verify_stages_core_mode(
        root,
        build,
        budget,
        a,
        refs,
        native3_cargo_environments,
        None,
    )
}

pub(crate) fn verify_stream11_stages_core(
    root: &SkillRoot,
    build: &CodeBuildRequest,
    budget: &CodeBudget,
    a: &CodeSourceArtifact,
    refs: [&ArtifactReference; 5],
    environments: &[Value; 2],
    captures: &[ArtifactReference],
) -> Result<()> {
    ensure!(captures.len() == 2, "stream11_cargo_capture_inventory");
    verify_stages_core_mode(
        root,
        build,
        budget,
        a,
        refs,
        Some(environments),
        Some(captures),
    )
}

fn verify_stages_core_mode(
    root: &SkillRoot,
    build: &CodeBuildRequest,
    budget: &CodeBudget,
    a: &CodeSourceArtifact,
    refs: [&ArtifactReference; 5],
    native3_cargo_environments: Option<&[Value; 2]>,
    stream11: Option<&[ArtifactReference]>,
) -> Result<()> {
    let tools = [
        &a.tools[0],
        &a.tools[3],
        &a.tools[0],
        &a.tools[3],
        &a.runner,
    ];
    let args: Vec<Vec<String>> = vec![
        vec![
            "build",
            "--locked",
            "--offline",
            "-j4",
            "-p",
            "oe-design",
            "--bin",
            "oev2-design-bootstrap",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        vec![
            "generate".into(),
            a.source_root.clone(),
            a.tools[2].path.clone(),
        ],
        vec![
            "build".into(),
            "--locked".into(),
            "--offline".into(),
            "-j4".into(),
            "--target".into(),
            a.platform.clone(),
            "-p".into(),
            "oe-code-runner".into(),
        ],
        vec![
            "check".into(),
            a.source_root.clone(),
            a.tools[2].path.clone(),
        ],
        vec!["identity".into()],
    ];
    for (i, r) in refs.iter().enumerate() {
        crate::verify_binary(tools[i])?;
        let selected_stream = stream11.filter(|_| i == 0 || i == 2);
        let stdout = if let Some(captures) = selected_stream {
            crate::native_stream11::verify_native_stage11(
                root,
                &captures[if i == 0 { 0 } else { 1 }],
                r,
                &file_binding(Path::new(&tools[i].path))?,
                root.path(),
                &args[i],
            )?;
            Vec::new() // Cargo stdout is retained and checked as raw content, not parsed as JSON.
        } else {
            verify_process(
                root,
                r,
                &file_binding(Path::new(&tools[i].path))?,
                root.path(),
                &args[i],
            )?
            .0
        };
        let v = read(root, r)?;
        ensure!(
            r.path
                == Path::new(&build.evidence_path)
                    .join(STAGES[i])
                    .join(if selected_stream.is_some() {
                        "raw-stream11/process.json"
                    } else {
                        "process.json"
                    })
                    .to_str()
                    .context("native_path_utf8")?
                && v["per_stream_limit"]
                    == (if selected_stream.is_some() {
                        crate::native_stream11::selected_resources11().raw_stream_bytes
                    } else {
                        budget.output_bytes
                    })
                && v["timeout_seconds"].as_f64().is_some_and(|t| t * 1000.0
                    <= budget.timeout_ms as f64 + 0.001
                    && t <= if i == 0 || i == 2 {
                        1200.0
                    } else if i == 4 {
                        10.0
                    } else {
                        300.0
                    }),
            "native_original_stage_budget_or_path_changed"
        );
        let added: Vec<&str> = if i == 0 || i == 2 {
            vec![
                "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_HOME",
                "CARGO_PROFILE_DEV_CODEGEN_UNITS",
                "CARGO_PROFILE_DEV_DEBUG",
                "CARGO_PROFILE_DEV_INCREMENTAL",
                "CARGO_PROFILE_DEV_LTO",
                "CARGO_PROFILE_DEV_SPLIT_DEBUGINFO",
                "CARGO_PROFILE_DEV_STRIP",
                "CARGO_TARGET_DIR",
                "OE_NATIVE_STAGE_CONTEXT",
                "OE_NATIVE_STAGE_CONTEXT_SHA256",
                "RUSTC",
                "TMPDIR",
            ]
        } else {
            vec![]
        };
        let removed = if i == 0 || i == 2 {
            vec!["RUSTFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"]
        } else {
            vec![
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
            ]
        };
        if let Some(environments) = native3_cargo_environments.filter(|_| i == 0 || i == 2) {
            let frozen = &environments[if i == 0 { 0 } else { 1 }];
            ensure!(
                v["environment_added_keys"] == frozen["added_keys"]
                    && v["environment_removed_keys"] == frozen["removed_keys"],
                "native3_original_stage_environment_changed"
            );
        } else {
            ensure!(
                v["environment_added_keys"] == serde_json::to_value(added)?
                    && v["environment_removed_keys"] == serde_json::to_value(removed)?,
                "native_original_stage_environment_changed"
            );
        }
        if i < 4 {
            ensure!(v == a.build_processes[i], "native_original_stage_changed");
        }
        match i {
            1 => ensure!(
                strict_json(&stdout)? == a.generation["receipt"],
                "native_generation_stdout_changed"
            ),
            3 => {
                let check = strict_json(&stdout)?;
                ensure!(
                    check == a.generation["check"]
                        && check["schema"] == "oev2.generation-drift/1"
                        && check["status"] == "consistent"
                        && check["drift"] == json!([])
                        && check["generation"] == a.generation["receipt"],
                    "native_drift_stdout_changed"
                );
            }
            4 => ensure!(
                strict_json(&stdout)? == a.compiled,
                "native_identity_stdout_changed"
            ),
            _ => {}
        }
    }
    Ok(())
}

pub struct NativeRuntimeBuilder;
pub fn native_link_entry() -> Result<i32> {
    native_link_arguments(std::env::args().skip(1).collect())
}
pub fn native_link_arguments(arguments: Vec<String>) -> Result<i32> {
    crate::native_link::entry(arguments)
}
impl CodeNativeBudgetedBuildPort for NativeRuntimeBuilder {
    fn build_native_budgeted(
        &self,
        input: &CodeNativeBudgetedBuildRequest,
    ) -> std::result::Result<CodeNativeBuildCapture, FailureRecord> {
        build_native_budgeted(input).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,phase:"native_budgeted_build".into(),retryable:false,
            partial_result:Some(json!({"execution":"failed","reason":format!("{e:#}"),"compilation_origin":"unknown","release_eligibility":"unknown"}))})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oe_core::process::run_command;
    #[test]
    fn bounded_raw_capture_detects_mutation_and_missing_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().canonicalize().unwrap();
        std::fs::write(path.join("SKILL.md"), b"test").unwrap();
        std::fs::create_dir(path.join("var")).unwrap();
        let root = SkillRoot::new(&path).unwrap();
        let folder = path.join("var/printf");
        let exe = file_binding(Path::new("/usr/bin/printf")).unwrap();
        let args = vec!["%s".into(), "original bytes".into()];
        let mut spec = CommandSpec {
            executable: exe.path.clone().into(),
            arguments: args.iter().map(Into::into).collect(),
            cwd: path.clone(),
            environment: BTreeMap::from([
                ("LC_ALL".into(), "C".into()),
                ("PATH".into(), "/usr/bin:/bin".into()),
            ]),
            remove_environment: std::env::vars_os()
                .map(|(k, _)| k)
                .filter(|k| k != "LC_ALL" && k != "PATH")
                .collect(),
            timeout: Duration::from_secs(1),
            output_limit: 4096,
        };
        let deadline = epoch_ms().unwrap() + 2000;
        freeze_command(&root, &mut spec, &folder, deadline).unwrap();
        let actual = run_command(&spec, &folder).unwrap();
        assert_eq!(actual.status, "completed");
        let r = reference(&root, &folder.join("process.json")).unwrap();
        verify_frozen_command(&root, &r, 4096, 2000, Some(deadline)).unwrap();
        assert_eq!(
            verify_process(&root, &r, &exe, &path, &args).unwrap().0,
            b"original bytes"
        );
        std::fs::write(folder.join("stdout"), b"mutated bytes").unwrap();
        assert!(verify_process(&root, &r, &exe, &path, &args).is_err());
        std::fs::write(folder.join("stdout"), b"original bytes").unwrap();
        let mut report = read(&root, &r).unwrap();
        report["descendant_cleanup"]["state"] = json!("unverified");
        std::fs::write(
            folder.join("process.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
        let changed = reference(&root, &folder.join("process.json")).unwrap();
        assert!(verify_process(&root, &changed, &exe, &path, &args).is_err());
        assert!(verify_frozen_command(&root, &changed, 8192, 2000, Some(deadline)).is_err());
        assert!(deadline_timeout(Instant::now()).is_err());
    }
    #[test]
    fn native_encoded_flags_survive_actual_core_environment_removals() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().canonicalize().unwrap();
        let flags = "-C\u{1f}linker=/frozen/adapter";
        let mut spec = CommandSpec {
            executable: "/usr/bin/env".into(),
            arguments: vec![],
            cwd: path.clone(),
            environment: BTreeMap::from([("CARGO_ENCODED_RUSTFLAGS".into(), flags.into())]),
            remove_environment: std::env::vars_os()
                .map(|(k, _)| k)
                .chain(["CARGO_ENCODED_RUSTFLAGS".into()])
                .collect(),
            timeout: Duration::from_secs(1),
            output_limit: 4096,
        };
        preserve_explicit_environment(&mut spec);
        let actual = run_command(&spec, &path.join("native-env")).unwrap();
        assert_eq!(actual.status, "completed");
        assert!(
            String::from_utf8(actual.stdout)
                .unwrap()
                .lines()
                .any(|s| s == format!("CARGO_ENCODED_RUSTFLAGS={flags}"))
        );
        spec.remove_environment
            .push("CARGO_ENCODED_RUSTFLAGS".into());
        let cleared = run_command(&spec, &path.join("old-clear")).unwrap();
        assert_eq!(cleared.status, "completed");
        assert!(
            !String::from_utf8(cleared.stdout)
                .unwrap()
                .contains("CARGO_ENCODED_RUSTFLAGS=")
        );
    }
    #[test]
    fn bounded_native_processes_preserve_timeout_and_output_limit_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().canonicalize().unwrap();
        std::fs::write(path.join("SKILL.md"), b"test").unwrap();
        std::fs::create_dir(path.join("var")).unwrap();
        let root = SkillRoot::new(&path).unwrap();
        for (name, exe, args, wanted) in [
            ("timeout", "/bin/sleep", vec!["1".to_owned()], "timed_out"),
            ("cap", "/usr/bin/yes", vec![], "output_limit"),
        ] {
            let tool = file_binding(Path::new(exe)).unwrap();
            let folder = path.join("var").join(name);
            let mut spec = CommandSpec {
                executable: exe.into(),
                arguments: args.iter().map(Into::into).collect(),
                cwd: path.clone(),
                environment: BTreeMap::from([
                    ("LC_ALL".into(), "C".into()),
                    ("PATH".into(), "/usr/bin:/bin".into()),
                ]),
                remove_environment: std::env::vars_os()
                    .map(|(k, _)| k)
                    .filter(|k| k != "LC_ALL" && k != "PATH")
                    .collect(),
                timeout: Duration::from_millis(50),
                output_limit: 1024,
            };
            let deadline = epoch_ms().unwrap() + 500;
            freeze_command(&root, &mut spec, &folder, deadline).unwrap();
            let actual = run_command(&spec, &folder).unwrap();
            assert_eq!(actual.status, wanted);
            assert_eq!(
                actual.report["descendant_cleanup"]["state"],
                "local_cleanup_completed"
            );
            let r = reference(&root, &folder.join("process.json")).unwrap();
            assert!(verify_process(&root, &r, &tool, &path, &args).is_err());
            verify_frozen_command(&root, &r, 1024, 500, Some(deadline)).unwrap();
        }
    }
}
