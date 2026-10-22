//! Explicit complete compiler /7 execution on the one existing five-stage owner.
//! The /6 source, SDK, stage, loader and process facts are shared content, not
//! qualification of /6 execution. All mixed invocation records stay intact.
use crate::native_runtime::{STAGES, deadline_timeout, read, reference};
use crate::native_runtime6::{NativeBuildSession6, verify_cargo_commands, verify_pins};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, create_new_private_dir, write_new};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
pub const PROFILE: &str = "semantica.rust-full-native-compiler-inputs/7";
fn native_build_facts(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    profile: &CodeNativeRuntimeProfile6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Value> {
    let mut facts =
        crate::native_runtime6::native_build_facts(root, side_ref, side, profile, recipe)?;
    facts["schema"] = json!("oev2.native-build-facts/7");
    facts["complete_invocation_schema"] = json!("oev2.native-compiler-link-invocation/7");
    facts["shared_source_conditions_schema"] = json!("oev2.native-runtime-profile/6");
    Ok(facts)
}
pub(crate) fn bind_artifact(
    session: &NativeBuildSession6,
    a: &mut CodeSourceArtifact,
) -> Result<()> {
    deadline_timeout(session.deadline)?;
    verify_pins(&session.root, &session.recipe)?;
    ensure!(
        crate::native_link7::is_adapter7(&session.recipe)?
            && a.compiled["rustflags_sha256"] == session.recipe.raw_encoded_rustflags_sha256,
        "compiler7_adapter_or_compiled_flags_changed"
    );
    a.profile = PROFILE.into();
    a.generation["native_build"] = native_build_facts(
        &session.root,
        &session.side_ref,
        &session.side,
        &session.profile,
        &session.recipe,
    )?;
    a.uncovered.extend(["Complete compiler7 execution uses the original owner, selected conditions and body deadline; it does not confer release/adoption authority".into(),
        "Original whole Semantica/platform and engineering scope remains independently required".into()]);
    Ok(())
}
pub fn build_full_native_budgeted(
    input: &CodeNativeBudgetedBuildRequest7,
) -> Result<CodeNativeBuildCapture7> {
    let started = Instant::now();
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("native6_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.native-budgeted-build-request/7"
            && (1..=3_600_000).contains(&input.budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&input.budget.output_bytes),
        "native6_request_schema_or_bounds"
    );
    let deadline = started
        .checked_add(Duration::from_millis(input.budget.timeout_ms))
        .context("native6_deadline_overflow")?;
    let session =
        NativeBuildSession6::new_parts(&input.build, &input.budget, &input.side_context, deadline)?;
    ensure!(
        crate::native_link7::is_adapter7(&session.recipe)?,
        "compiler7_selected_adapter_required"
    );
    let artifact = crate::build_with_deadline_mode(
        &input.build,
        Some(deadline),
        session.output_limit,
        crate::BuildMode::Native7(&session),
    )?;
    let root = &session.root;
    let evidence = root.private_path(Path::new(&input.build.evidence_path))?;
    let artifact_ref = reference(root, &evidence.join("artifact.json"))?;
    let invocations = crate::native_link7::load_invocations(
        root,
        &session.side_ref,
        &session.side,
        &session.recipe,
    )?;
    ensure!(
        invocations
            .iter()
            .any(|x| crate::native_link7::parent_stage(x).ok() == Some("build-normalizer"))
            && invocations
                .iter()
                .any(|x| crate::native_link7::parent_stage(x).ok() == Some("build-runner")),
        "native6_stage_inventory_empty"
    );
    let loader_receipts =
        crate::native_loader6::prepare_retained7(root, &session.side, &session.recipe)?;
    let images = crate::native_profile6::load_debug_images(root, &session.side.source_profile)?;
    create_new_private_dir(&evidence.join("native-debug"))?;
    let original_images = crate::native_link7::original_invocations(&invocations)?;
    let mut debug_images = Vec::new();
    for image in &images.images {
        let observation = crate::native_debug::observe_loader6(
            &session,
            &artifact_ref,
            &artifact,
            &original_images,
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
    let capture = CodeNativeBuildCapture7 {
        schema: "oev2.native-build-capture/7".into(),
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
        process_environment_bindings: crate::native_link7::process_bindings(root, &invocations)?,
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
        &evidence.join("native-compiler7-capture.json"),
        &serde_json::to_value(&capture)?,
    )?;
    Ok(capture)
}
pub fn verify_full_native_capture(
    root: &SkillRoot,
    source_profile: &ArtifactReference,
    side_ref: &ArtifactReference,
    artifact_ref: &ArtifactReference,
    c: &CodeNativeBuildCapture7,
) -> Result<()> {
    let loader_refs = crate::native_loader6::verify_retained7(
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
        c.schema == "oev2.native-build-capture/7"
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
            && a.profile == PROFILE
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
    let actual = crate::native_link7::load_invocations(root, side_ref, &side, &recipe)?;
    ensure!(
        actual == c.invocations
            && crate::native_buildscript6::retained(root, &side, &recipe)?
                == c.build_script_metadata_bindings
            && crate::native_link7::process_bindings(root, &actual)?
                == c.process_environment_bindings,
        "native6_invocation_inventory_changed"
    );
    let original_images = crate::native_link7::original_invocations(&actual)?;
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
            &original_images,
            image,
            observation,
        )?;
    }
    verify_pins(root, &recipe)?;
    Ok(())
}
pub struct NativeRuntimeBuilder7;
pub fn native_full_link_entry() -> Result<i32> {
    native_full_link_arguments(std::env::args().skip(1).collect())
}
pub fn native_full_link_arguments(args: Vec<String>) -> Result<i32> {
    crate::native_link6::entry_compiler7(args)
}
impl CodeNativeBudgetedBuild7Port for NativeRuntimeBuilder7 {
    fn build_full_native_budgeted_runtime(
        &self,
        input: &CodeNativeBudgetedBuildRequest7,
    ) -> std::result::Result<CodeNativeBuildCapture7, FailureRecord> {
        build_full_native_budgeted(input).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,phase:"native_full_budgeted_build".into(),
            retryable:false,partial_result:Some(json!({"execution":"failed","reason":format!("{e:#}"),"compilation_origin":"unknown","release_eligibility":"unknown"}))})
    }
}
