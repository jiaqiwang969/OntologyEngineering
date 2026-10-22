//! Explicit full native /3 mode of the existing five-stage builder.
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
    recipe: &'a CodeNativeBuildRecipe3,
    role: &str,
) -> Result<&'a CodeNativeFileBinding> {
    recipe
        .tools
        .iter()
        .find(|t| t.role == role)
        .map(|t| &t.file)
        .context("native3_tool_role_missing")
}
pub(crate) fn verify_pins(
    root: &SkillRoot,
    recipe: &CodeNativeBuildRecipe3,
) -> Result<CodeNativeSdkLayout3> {
    for file in recipe
        .tools
        .iter()
        .map(|t| &t.file)
        .chain(&recipe.external_archive_bindings)
    {
        crate::native_profile::verify_file(file)?;
    }
    crate::native_sdk::inspect_sdk(root, &recipe.sdk_layout)
}
pub(crate) struct NativeBuildSession3 {
    pub root: SkillRoot,
    pub side_ref: ArtifactReference,
    pub side: CodeNativeSideContext3,
    pub profile: CodeNativeRuntimeProfile3,
    pub recipe: CodeNativeBuildRecipe3,
    pub deadline: Instant,
    pub deadline_epoch_ms: u64,
    pub output_limit: usize,
}
impl NativeBuildSession3 {
    fn new(input: &CodeNativeBudgetedBuildRequest3, deadline: Instant) -> Result<Self> {
        let root = SkillRoot::new(&input.build.source_root)?;
        let side = crate::native_profile3::load_side(&root, &input.side_context)?;
        ensure!(
            side.build == input.build && side.body_budget == input.budget,
            "native3_build_or_budget_changed"
        );
        let profile = crate::native_profile3::inspect_profile(&root, &side.source_profile)?;
        let recipe = crate::native_profile3::load_recipe(&root, &side.source_profile)?;
        verify_pins(&root, &recipe)?;
        let remaining = deadline_timeout(deadline)?;
        let deadline_epoch_ms = epoch_ms()?
            .checked_add(u64::try_from(remaining.as_millis())?)
            .context("native3_deadline_overflow")?;
        create_new_private_dir(&root.private_path(Path::new(&side.stage_root))?)?;
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
            "native3_parent_stage_invalid"
        );
        deadline_timeout(self.deadline)?;
        verify_pins(&self.root, &self.recipe)?;
        ensure!(
            read(&self.root, &self.side_ref)? == serde_json::to_value(&self.side)?,
            "native3_side_changed"
        );
        let stage = Path::new(&self.side.stage_root).join(parent_stage);
        create_new_private_dir(&self.root.private_path(&stage)?)?;
        let temporary = self.root.new_private_dir(
            &Path::new(&self.side.build.target_dir)
                .join("native-tmp")
                .join(parent_stage),
        )?;
        let context_path = stage.join("stage-context.json");
        let compilation_sources = crate::native_sources4::prepare(self, parent_stage)?;
        let context = CodeNativeStageContext4 {
            schema: "oev2.native-build-stage/4".into(),
            source_root: self.root.path().to_str().context("native4_utf8")?.into(),
            source_manifest: public_manifest(self.root.path())?,
            side_context: self.side_ref.clone(),
            recipe: self.profile.recipe.clone(),
            parent_stage: parent_stage.into(),
            deadline_epoch_ms: self.deadline_epoch_ms,
            output_bytes: self.output_limit as u64,
            invocation_root: stage.to_str().context("native4_utf8")?.into(),
            temporary_root: temporary.to_str().context("native4_utf8")?.into(),
            compilation_sources,
        };
        context
            .validate()
            .map_err(|e| anyhow::anyhow!("native4_stage_contract:{e:?}"))?;
        write_new(&context_path, &serde_json::to_value(&context)?)?;
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
            "native3_compiled_flags_changed"
        );
        a.profile = crate::native_profile3::PROFILE.into();
        a.generation["native_build"] = native_build_facts(
            &self.root,
            &self.side_ref,
            &self.side,
            &self.profile,
            &self.recipe,
        )?;
        a.uncovered.extend(["Full native3 execution facts do not confer build owner or engineering adoption authority".into(),
            "Complete whole Semantica/platform scope remains independently unqualified".into()]);
        Ok(())
    }
}
fn native_build_facts(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext3,
    profile: &CodeNativeRuntimeProfile3,
    recipe: &CodeNativeBuildRecipe3,
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
        json!({"schema":"oev2.native-build-facts/3","source_profile":side.source_profile,"side_context":side_ref,
        "recipe":profile.recipe,"raw_encoded_rustflags":recipe.raw_encoded_rustflags,
        "raw_encoded_rustflags_sha256":recipe.raw_encoded_rustflags_sha256,"stage_contexts":contexts,"cargo_commands":commands}),
    )
}
pub fn build_full_native_budgeted(
    input: &CodeNativeBudgetedBuildRequest3,
) -> Result<CodeNativeBuildCapture3> {
    let started = Instant::now();
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.native-budgeted-build-request/3"
            && (1..=3_600_000).contains(&input.budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&input.budget.output_bytes),
        "native3_request_schema_or_bounds"
    );
    let deadline = started
        .checked_add(Duration::from_millis(input.budget.timeout_ms))
        .context("native3_deadline_overflow")?;
    let session = NativeBuildSession3::new(input, deadline)?;
    let artifact = crate::build_with_deadline_mode(
        &input.build,
        Some(deadline),
        session.output_limit,
        crate::BuildMode::Native3(&session),
    )?;
    let root = &session.root;
    let evidence = root.private_path(Path::new(&input.build.evidence_path))?;
    let artifact_ref = reference(root, &evidence.join("artifact.json"))?;
    let invocations = crate::native_link3::load_invocations(
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
        "native3_stage_inventory_empty"
    );
    let images = crate::native_profile3::load_debug_images(root, &session.side.source_profile)?;
    create_new_private_dir(&evidence.join("native-debug"))?;
    let mut debug_images = Vec::new();
    for image in &images.images {
        let observation = crate::native_debug::observe_full(
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
    let own = &crate::driver_root().context("native3_driver_root")?;
    let capture = CodeNativeBuildCapture3 {
        schema: "oev2.native-build-capture/3".into(),
        source_profile: session.side.source_profile.clone(),
        artifact: artifact_ref.clone(),
        driver: crate::binary_reference(&std::env::current_exe()?, own)?,
        normalizer_process: refs[0].clone(),
        generation_process: refs[1].clone(),
        runner_build_process: refs[2].clone(),
        drift_process: refs[3].clone(),
        identity_process: refs[4].clone(),
        side_context: input.side_context.clone(),
        invocations,
        debug_images,
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
        &evidence.join("native-full-capture.json"),
        &serde_json::to_value(&capture)?,
    )?;
    Ok(capture)
}
fn verify_cargo_commands(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext3,
    recipe: &CodeNativeBuildRecipe3,
) -> Result<[Value; 2]> {
    let mut environments = Vec::new();
    for stage in ["build-normalizer", "build-runner"] {
        let context = crate::native_link3::validate_context(root, side_ref, side, recipe, stage)?;
        let command = read(
            root,
            &reference(
                root,
                &Path::new(&side.build.evidence_path)
                    .join(stage)
                    .with_extension("command.json"),
            )?,
        )?;
        let process = read(
            root,
            &reference(
                root,
                &Path::new(&side.build.evidence_path)
                    .join(stage)
                    .join("process.json"),
            )?,
        )?;
        let process_ref = reference(
            root,
            &Path::new(&side.build.evidence_path)
                .join(stage)
                .join("process.json"),
        )?;
        crate::native_runtime::verify_frozen_command_budget(
            root,
            &process_ref,
            side.body_budget.output_bytes,
            side.body_budget.timeout_ms,
            Some(
                context["deadline_epoch_ms"]
                    .as_u64()
                    .context("native3_deadline")?,
            ),
        )?;
        let values = command["environment"]
            .as_object()
            .context("native3_cargo_environment")?;
        let mut expected = BTreeMap::from([
            (
                "CARGO_ENCODED_RUSTFLAGS",
                json!(recipe.raw_encoded_rustflags),
            ),
            ("CARGO_HOME", json!(side.build.cargo_home)),
            ("RUSTC", json!(side.build.rustc_path)),
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
        ensure!(
            command["schema"] == "oev2.native-command-before-dispatch/1"
                && serde_json::to_value(&expected)? == command["environment"]
                && command["cwd"] == root.path().to_str().context("native3_path")?
                && command["executable"]["path"] == side.build.cargo_path
                && command["arguments"] == process["arguments"]
                && command["timeout_seconds"] == process["timeout_seconds"]
                && command["deadline_epoch_ms"] == context["deadline_epoch_ms"]
                && command["output_bytes"] == side.body_budget.output_bytes
                && command["timeout_seconds"].as_f64().is_some_and(
                    |t| t > 0.0 && t * 1000.0 <= side.body_budget.timeout_ms as f64 + 0.001
                ),
            "native3_cargo_command_changed"
        );
        let removed = command["removed_environment"]
            .as_array()
            .context("native3_cargo_removed")?;
        let mut names = std::collections::BTreeSet::new();
        ensure!(
            removed.iter().all(|v| v
                .as_str()
                .is_some_and(|s| !values.contains_key(s) && names.insert(s))),
            "native3_cargo_removed_invalid"
        );
        environments
            .push(json!({"added_keys":values.keys().collect::<Vec<_>>(),"removed_keys":removed}));
    }
    Ok(environments
        .try_into()
        .map_err(|_| anyhow::anyhow!("native3_cargo_stage_count"))?)
}
pub fn verify_full_native_capture(
    root: &SkillRoot,
    source_profile: &ArtifactReference,
    side_ref: &ArtifactReference,
    artifact_ref: &ArtifactReference,
    c: &CodeNativeBuildCapture3,
) -> Result<()> {
    c.validate()
        .map_err(|e| anyhow::anyhow!("native3_capture:{e:?}"))?;
    ensure!(
        c.schema == "oev2.native-build-capture/3"
            && c.source_profile == *source_profile
            && c.side_context == *side_ref
            && c.artifact == *artifact_ref,
        "native3_capture_binding_changed"
    );
    let side = crate::native_profile3::load_side(root, side_ref)?;
    ensure!(
        side.source_profile == *source_profile,
        "native3_capture_profile_changed"
    );
    let profile = crate::native_profile3::inspect_profile(root, source_profile)?;
    let recipe = crate::native_profile3::load_recipe(root, source_profile)?;
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
        "native3_actual_compiled_identity_mismatch"
    );
    crate::verify_binary(&c.driver)?;
    let evidence = root.private_path(Path::new(&side.build.evidence_path))?;
    let target = root.private_path(Path::new(&side.build.target_dir))?;
    ensure!(
        artifact_ref == &reference(root, &evidence.join("artifact.json"))?
            && a.artifact_id == side.build.artifact_id
            && a.source_root == side.build.source_root
            && a.source_root == root.path().to_str().context("native3_path")?
            && a.platform == side.build.platform
            && a.manifest == public_manifest(root.path())?
            && a.runner.path
                == target
                    .join(&side.build.platform)
                    .join("debug/oe-code-runner")
                    .to_str()
                    .context("native3_path")?
            && a.tools.len() == 4
            && a.tools[0].path == side.build.cargo_path
            && a.tools[1].path == side.build.rustc_path
            && a.tools[2].path == side.build.ggen_path
            && Path::new(&a.tools[3].path) == target.join("debug/oev2-design-bootstrap")
            && a.profile == crate::native_profile3::PROFILE
            && a.build_processes.len() == 4
            && a.compiled["rustflags_sha256"] == recipe.raw_encoded_rustflags_sha256
            && a.generation["fixed_build_driver"]["executable_sha256"] == c.driver.sha256
            && a.generation["fixed_build_driver"]["source_manifest_sha256"]
                == c.driver.source_manifest_sha256,
        "native3_authorized_artifact_changed"
    );
    ensure!(
        a.generation["native_build"]
            == native_build_facts(root, side_ref, &side, &profile, &recipe)?,
        "native3_artifact_recipe_changed"
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
    let actual = crate::native_link3::load_invocations(root, side_ref, &side, &recipe)?;
    ensure!(
        actual == c.invocations,
        "native3_invocation_inventory_changed"
    );
    let images = crate::native_profile3::load_debug_images(root, source_profile)?;
    ensure!(
        c.debug_images.len() == 2 && images.images.len() == 2,
        "native3_debug_image_inventory"
    );
    for (image, observation) in images.images.iter().zip(&c.debug_images) {
        crate::native_debug::verify_full(
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
pub struct NativeRuntimeBuilder3;
pub fn native_full_link_entry() -> Result<i32> {
    native_full_link_arguments(std::env::args().skip(1).collect())
}
pub fn native_full_link_arguments(args: Vec<String>) -> Result<i32> {
    crate::native_link3::entry(args)
}
impl CodeNativeBudgetedBuild3Port for NativeRuntimeBuilder3 {
    fn build_full_native_budgeted_runtime(
        &self,
        input: &CodeNativeBudgetedBuildRequest3,
    ) -> std::result::Result<CodeNativeBuildCapture3, FailureRecord> {
        build_full_native_budgeted(input).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,phase:"native_full_budgeted_build".into(),
            retryable:false,partial_result:Some(json!({"execution":"failed","reason":format!("{e:#}"),"compilation_origin":"unknown","release_eligibility":"unknown"}))})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native2_request_is_not_silently_upgraded_or_dispatched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        std::fs::write(root.join("SKILL.md"), b"test").unwrap();
        let r = json!({"path":root.join("var/nonexistent-side.json"),"sha256":"0".repeat(64)});
        let request:CodeNativeBudgetedBuildRequest3=serde_json::from_value(json!({"schema":"oev2.native-budgeted-build-request/2",
            "build":{"artifact_id":"native3-test","source_root":root,"evidence_path":root.join("var/evidence"),"cargo_path":"/cargo","rustc_path":"/rustc","ggen_path":"/ggen","cargo_home":"/cache","target_dir":root.join("var/target"),"platform":"aarch64-apple-darwin"},
            "budget":{"timeout_ms":1000,"output_bytes":4096},"side_context":r})).unwrap();
        let error = build_full_native_budgeted(&request).unwrap_err();
        assert!(error.to_string().contains("schema_or_bounds"));
        assert!(!root.join("var").exists());
    }
    #[test]
    fn actual_selected_environment_uses_shared_budget_and_keeps_native2_strict() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().canonicalize().unwrap();
        std::fs::write(path.join("SKILL.md"), b"test").unwrap();
        std::fs::create_dir(path.join("var")).unwrap();
        let root = SkillRoot::new(&path).unwrap();
        let flags = "-C\u{1f}linker=/frozen/full-adapter";
        let selected = BTreeMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("LC_ALL".into(), "C".into()),
            ("CARGO_ENCODED_RUSTFLAGS".into(), flags.into()),
            ("CARGO_PROFILE_DEV_DEBUG".into(), "2".into()),
            ("CARGO_PROFILE_DEV_CODEGEN_UNITS".into(), "256".into()),
        ]);
        let removed = std::env::vars_os()
            .map(|(k, _)| k)
            .filter(|k| !selected.contains_key(k))
            .collect();
        let folder = path.join("var/actual-selected-env");
        let mut spec = CommandSpec {
            executable: "/usr/bin/env".into(),
            arguments: vec![],
            cwd: path.clone(),
            environment: selected,
            remove_environment: removed,
            timeout: Duration::from_secs(1),
            output_limit: 4096,
        };
        let deadline = epoch_ms().unwrap() + 2000;
        crate::native_runtime::freeze_command(&root, &mut spec, &folder, deadline).unwrap();
        let actual = oe_core::process::run_command(&spec, &folder).unwrap();
        assert_eq!(actual.status, "completed");
        assert!(
            String::from_utf8(actual.stdout)
                .unwrap()
                .lines()
                .any(|s| s == format!("CARGO_ENCODED_RUSTFLAGS={flags}"))
        );
        let process_ref = reference(&root, &folder.join("process.json")).unwrap();
        crate::native_runtime::verify_frozen_command_budget(
            &root,
            &process_ref,
            4096,
            2000,
            Some(deadline),
        )
        .unwrap();
        assert!(
            crate::native_runtime::verify_frozen_command(
                &root,
                &process_ref,
                4096,
                2000,
                Some(deadline)
            )
            .is_err()
        );
        let frozen_path = folder.with_extension("command.json");
        let frozen = read(&root, &reference(&root, &frozen_path).unwrap()).unwrap();
        for which in 0..3 {
            let mut changed = frozen.clone();
            match which {
                0 => changed["prepared_epoch_ms"] = json!(deadline),
                1 => changed["executable"]["bytes"] = json!(1),
                _ => changed["output_bytes"] = json!(8192),
            };
            std::fs::write(&frozen_path, serde_json::to_vec(&changed).unwrap()).unwrap();
            assert!(
                crate::native_runtime::verify_frozen_command_budget(
                    &root,
                    &process_ref,
                    4096,
                    2000,
                    Some(deadline)
                )
                .is_err()
            );
        }
    }
}
