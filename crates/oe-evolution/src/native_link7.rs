//! Complete compiler invocation /7, borrowing the original stage/process owner.
//! Original /3 invocations remain actual original records, never /7 projections.
use crate::native_runtime::{deadline_timeout, read, reference};
use crate::native_runtime6::{role, verify_pins};
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, read_regular_bytes, write_new};
use std::{path::Path, time::Instant};

pub(crate) fn is_adapter7(recipe: &CodeNativeBuildRecipe6) -> Result<bool> {
    Ok(Path::new(&role(recipe, "linker_adapter")?.path)
        .file_name()
        .and_then(|s| s.to_str())
        == Some("oe-native-loader7-linker"))
}
fn files(
    inputs: &CodeNativeCompilerInputContentObservation7,
    recipe: &CodeNativeBuildRecipe6,
) -> Vec<CodeNativeFileBinding> {
    let mut files = crate::native_link6::input_files(&inputs.base_inputs, recipe);
    files.extend(
        inputs
            .file_inputs
            .files
            .iter()
            .flat_map(|f| [f.original_file.clone(), f.captured_file.clone()]),
    );
    files
}
pub(crate) fn execute_complete(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    stage: &str,
    context_path: &Path,
    context: &serde_json::Value,
    context_bytes: &[u8],
    ordinal: u64,
    cwd: &Path,
    parent: &crate::native_loader6::ActualCompilerParent,
    inputs: &CodeNativeCompilerInputContentObservation7,
    approved_caller: Option<&Path>,
    deadline: Instant,
    end: u64,
) -> Result<i32> {
    execute_complete_mode(
        root,
        side_ref,
        side,
        recipe,
        stage,
        context_path,
        context,
        context_bytes,
        ordinal,
        cwd,
        parent,
        inputs,
        approved_caller,
        deadline,
        end,
        false,
    )
}
pub(crate) fn execute_complete_in_adapter8(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    stage: &str,
    context_path: &Path,
    context: &serde_json::Value,
    context_bytes: &[u8],
    ordinal: u64,
    cwd: &Path,
    parent: &crate::native_loader6::ActualCompilerParent,
    inputs: &CodeNativeCompilerInputContentObservation7,
    approved_caller: Option<&Path>,
    deadline: Instant,
    end: u64,
) -> Result<i32> {
    execute_complete_mode(
        root,
        side_ref,
        side,
        recipe,
        stage,
        context_path,
        context,
        context_bytes,
        ordinal,
        cwd,
        parent,
        inputs,
        approved_caller,
        deadline,
        end,
        true,
    )
}
fn execute_complete_mode(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    stage: &str,
    context_path: &Path,
    context: &serde_json::Value,
    context_bytes: &[u8],
    ordinal: u64,
    cwd: &Path,
    parent: &crate::native_loader6::ActualCompilerParent,
    inputs: &CodeNativeCompilerInputContentObservation7,
    approved_caller: Option<&Path>,
    deadline: Instant,
    end: u64,
    allow_adapter8: bool,
) -> Result<i32> {
    ensure!(
        if allow_adapter8 {
            crate::native_link8::is_adapter8(recipe)?
        } else {
            is_adapter7(recipe)?
        },
        "compiler7_selected_adapter_required"
    );
    crate::native_loader6::recheck_live_parent(parent)?;
    ensure!(
        inputs.compiler_pid == parent.pid
            && inputs.compiler_executable == parent.executable
            && inputs.compiler_intent == parent.intent
            && inputs.stage_context == reference(root, context_path)?
            && inputs.side_context == *side_ref,
        "compiler7_actual_parent_or_stage_changed"
    );
    let assessment = crate::native_compiler_link7::inspect_retained_complete7(
        root,
        &reference(root, &cwd.join("compiler-input-content7.json"))?,
    )?;
    deadline_timeout(deadline)?;
    crate::native_input_content7::verify_retained(root, inputs, approved_caller, &mut |p| {
        crate::native_process6::verify(root, p, side, recipe, context_path)
    })?;
    crate::native_link_files7::verify_live7(root, &inputs.file_inputs)?;
    crate::native_archive::verify_live_inputs(&inputs.base_inputs)?;
    ensure!(
        !Path::new(&assessment.domain.original_output).exists(),
        "compiler7_output_not_fresh"
    );
    ensure!(
        crate::native_link6::validate_context(root, side_ref, side, recipe, stage)? == *context
            && read_regular_bytes(context_path, 16 * 1024 * 1024)? == context_bytes,
        "compiler7_source_context_changed_before_link"
    );
    let mut arguments = assessment.complete_arguments.clone();
    let suffix = crate::native_link6::append_native(root, &mut arguments, recipe, cwd)?;
    let before = files(inputs, recipe);
    for file in &before {
        crate::native_profile::verify_file(file)?;
    }
    crate::native_sdkroot6::recheck_actual(root, recipe, &parent.sdk_root)?;
    let sdk = verify_pins(root, recipe)?;
    let selected = crate::native_sdk4::bind_selected_libraries4(
        &sdk,
        &inputs.file_inputs.request.received_arguments,
    )?;
    crate::native_loader6::retain_native_receipt(root, cwd, parent, selected, &sdk)?;
    write_new(
        &cwd.join("inputs-before.json"),
        &serde_json::json!({"files":before,"sdk":sdk,
        "source_manifest":crate::public_manifest(root.path())?,"raw_encoded_rustflags":recipe.raw_encoded_rustflags}),
    )?;
    deadline_timeout(deadline)?;
    crate::native_loader6::recheck_live_parent(parent)?;
    let process = crate::native_link6::observe_process(
        root,
        role(recipe, "clang_driver")?,
        arguments,
        cwd,
        &cwd.join("native-process"),
        deadline,
        end,
        usize::try_from(side.body_budget.output_bytes)?,
        "native_link",
        parent,
    )?;
    crate::native_sdkroot6::recheck_actual(root, recipe, &parent.sdk_root)?;
    crate::native_loader6::recheck_live_parent(parent)?;
    crate::native_link_files7::verify_live7(root, &inputs.file_inputs)?;
    crate::native_archive::verify_live_inputs(&inputs.base_inputs)?;
    for file in &before {
        crate::native_profile::verify_file(file)?;
    }
    ensure!(
        verify_pins(root, recipe)? == sdk
            && crate::public_manifest(root.path())? == context["source_manifest"]
            && read_regular_bytes(context_path, 16 * 1024 * 1024)? == context_bytes
            && crate::native_link6::validate_context(root, side_ref, side, recipe, stage)?
                == *context,
        "compiler7_inputs_source_context_changed_after_link"
    );
    write_new(
        &cwd.join("inputs-after.json"),
        &serde_json::json!({"files":files(inputs,recipe),"sdk":sdk,
        "source_manifest":crate::public_manifest(root.path())?,"raw_encoded_rustflags":recipe.raw_encoded_rustflags}),
    )?;
    let invocation=CodeNativeCompilerLinkInvocation7{schema:"oev2.native-compiler-link-invocation/7".into(),ordinal,parent_stage:stage.into(),
        inputs:inputs.clone(),input_assessment:assessment,native_recipe_suffix:suffix,native_process:process,
        uncovered:vec!["Same actual compiler/stage/index/link process owner; retained content never recreates live execution authority".into(),
            "Complete byte/debug/signature and whole original library/engineering scope remain separate".into()]};
    write_new(
        &cwd.join("invocation.json"),
        &serde_json::to_value(&invocation)?,
    )?;
    verify_one_mode(
        root,
        side_ref,
        side,
        recipe,
        stage,
        context,
        cwd,
        &invocation,
        approved_caller,
        allow_adapter8,
    )?;
    deadline_timeout(deadline)?;
    i32::try_from(
        invocation
            .native_process
            .exit_code
            .context("compiler7_exit_missing")?,
    )
    .context("compiler7_exit_range")
}
fn verify_one(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    stage: &str,
    context: &serde_json::Value,
    cwd: &Path,
    v: &CodeNativeCompilerLinkInvocation7,
    approved: Option<&Path>,
) -> Result<()> {
    verify_one_mode(
        root, side_ref, side, recipe, stage, context, cwd, v, approved, false,
    )
}
pub(crate) fn verify_one_in_adapter8(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    stage: &str,
    context: &serde_json::Value,
    cwd: &Path,
    v: &CodeNativeCompilerLinkInvocation7,
    approved: Option<&Path>,
) -> Result<()> {
    verify_one_mode(
        root, side_ref, side, recipe, stage, context, cwd, v, approved, true,
    )
}
fn verify_one_mode(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    stage: &str,
    context: &serde_json::Value,
    cwd: &Path,
    v: &CodeNativeCompilerLinkInvocation7,
    approved: Option<&Path>,
    allow_adapter8: bool,
) -> Result<()> {
    v.validate()
        .map_err(|e| anyhow::anyhow!("compiler7_invocation_contract:{e:?}"))?;
    ensure!(
        v.schema == "oev2.native-compiler-link-invocation/7"
            && v.parent_stage == stage
            && cwd.file_name().and_then(|s| s.to_str())
                == Some(format!("call-{:06}", v.ordinal).as_str())
            && (if allow_adapter8 {
                crate::native_link8::is_adapter8(recipe)?
            } else {
                is_adapter7(recipe)?
            }),
        "compiler7_invocation_revision_or_ordinal_changed"
    );
    let context_path = Path::new(&side.stage_root)
        .join(stage)
        .join("stage-context.json");
    ensure!(
        v.inputs.side_context == *side_ref
            && v.inputs.stage_context == reference(root, &context_path)?
            && v.inputs
                == serde_json::from_value(read(root, &v.input_assessment.content_observation)?)?,
        "compiler7_complete_original_input_record_changed"
    );
    crate::native_input_content7::verify_retained(root, &v.inputs, approved, &mut |p| {
        crate::native_process6::verify(root, p, side, recipe, &context_path)
    })?;
    let assessment = crate::native_compiler_link7::inspect_retained_complete7(
        root,
        &v.input_assessment.content_observation,
    )?;
    ensure!(
        assessment == v.input_assessment,
        "compiler7_source_output_or_argument_assessment_changed"
    );
    let mut args = assessment.complete_arguments.clone();
    let suffix = crate::native_link6::append_native(root, &mut args, recipe, cwd)?;
    ensure!(
        v.native_recipe_suffix == suffix
            && v.native_process.executable == *role(recipe, "clang_driver")?
            && v.native_process.arguments == args
            && Path::new(&v.native_process.cwd) == cwd
            && v.native_process.environment == crate::native_link6::environment()
            && v.native_process.role == "native_link"
            && v.native_process.process
                == reference(root, &cwd.join("native-process/process.json"))?
            && v.native_process.stdout == reference(root, &cwd.join("native-process/stdout"))?
            && v.native_process.stderr == reference(root, &cwd.join("native-process/stderr"))?,
        "compiler7_actual_command_or_receipt_changed"
    );
    let before = read(root, &reference(root, &cwd.join("inputs-before.json"))?)?;
    let after = read(root, &reference(root, &cwd.join("inputs-after.json"))?)?;
    ensure!(
        before == after
            && before["files"] == serde_json::to_value(files(&v.inputs, recipe))?
            && before["sdk"] == serde_json::to_value(verify_pins(root, recipe)?)?
            && before["source_manifest"] == crate::public_manifest(root.path())?
            && before["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
        "compiler7_before_after_content_changed"
    );
    crate::native_process6::verify(root, &v.native_process, side, recipe, &context_path)?;
    let end = context["deadline_epoch_ms"]
        .as_u64()
        .context("compiler7_original_deadline")?;
    crate::native_runtime::verify_frozen_command(
        root,
        &v.native_process.process,
        side.body_budget.output_bytes,
        side.body_budget.timeout_ms,
        Some(end),
    )?;
    crate::native_runtime::verify_process(
        root,
        &v.native_process.process,
        &v.native_process.executable,
        cwd,
        &args,
    )?;
    for (i, archive) in v.inputs.base_inputs.archives.iter().enumerate() {
        let folder = cwd.join(format!("content7-archive-index-{i:06}"));
        ensure!(
            archive.index_process.process == reference(root, &folder.join("process.json"))?,
            "compiler7_index_callback_wrong_side"
        );
        crate::native_runtime::verify_frozen_command(
            root,
            &archive.index_process.process,
            side.body_budget.output_bytes,
            side.body_budget.timeout_ms,
            Some(end),
        )?;
        crate::native_runtime::verify_process(
            root,
            &archive.index_process.process,
            &archive.index_process.executable,
            Path::new(&archive.index_request.cwd),
            &archive.index_request.arguments,
        )?;
    }
    Ok(())
}
pub(crate) fn inspect_envelope(v: &CodeNativeInvocationEnvelope7) -> Result<()> {
    v.validate()
        .map_err(|e| anyhow::anyhow!("compiler7_envelope_contract:{e:?}"))?;
    ensure!(
        v.schema == "oev2.native-invocation-envelope/7"
            && match v.kind.as_str() {
                "archive3" => v.archive3.is_some() && v.compiler7.is_none(),
                "compiler7" => v.archive3.is_none() && v.compiler7.is_some(),
                _ => false,
            },
        "compiler7_envelope_kind_or_exclusive_record_changed"
    );
    Ok(())
}
pub(crate) fn original_invocations(
    all: &[CodeNativeInvocationEnvelope7],
) -> Result<Vec<CodeNativeArchiveResponseInvocation>> {
    let mut out = Vec::new();
    for v in all {
        inspect_envelope(v)?;
        if let Some(original) = &v.archive3 {
            out.push(original.clone());
        }
    }
    // These are genuine original records. No compiler7/base projection is
    // converted to /3 for debug; the entire mixed inventory stays in capture7.
    Ok(out)
}
pub(crate) fn load_invocations(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<CodeNativeInvocationEnvelope7>> {
    ensure!(is_adapter7(recipe)?, "compiler7_selected_adapter_required");
    let mut all = Vec::new();
    for stage in ["build-normalizer", "build-runner"] {
        let context = crate::native_link6::validate_context(root, side_ref, side, recipe, stage)?;
        let folder = Path::new(&side.stage_root).join(stage);
        let mut dirs = std::fs::read_dir(&folder)?
            .map(|e| e.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        dirs.retain(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("call-"))
        });
        dirs.sort();
        ensure!(
            !dirs.is_empty() && dirs.len() <= 4096,
            "compiler7_invocation_inventory_empty_or_limit"
        );
        for (ordinal, cwd) in dirs.iter().enumerate() {
            ensure!(
                cwd.file_name().and_then(|s| s.to_str())
                    == Some(format!("call-{ordinal:06}").as_str()),
                "compiler7_invocation_gap"
            );
            let record = read(root, &reference(root, &cwd.join("invocation.json"))?)?;
            let envelope = match record["schema"].as_str() {
                Some("oev2.native-archive-response-invocation/3") => {
                    let original: CodeNativeArchiveResponseInvocation =
                        serde_json::from_value(record)?;
                    ensure!(
                        original.ordinal == ordinal as u64 && original.parent_stage == stage,
                        "compiler7_original_invocation_ordinal_changed"
                    );
                    let caller = Path::new(&original.inputs.request.caller_cwd);
                    let approved = if caller == root.path() {
                        None
                    } else {
                        Some(crate::native_sources6::caller_root(
                            root, side, stage, &context, caller,
                        )?)
                    };
                    crate::native_link6::verify_one_invocation(
                        root,
                        side_ref,
                        side,
                        recipe,
                        stage,
                        &context,
                        cwd,
                        &original,
                        approved.as_deref(),
                    )?;
                    CodeNativeInvocationEnvelope7 {
                        schema: "oev2.native-invocation-envelope/7".into(),
                        kind: "archive3".into(),
                        archive3: Some(original),
                        compiler7: None,
                    }
                }
                Some("oev2.native-compiler-link-invocation/7") => {
                    let complete: CodeNativeCompilerLinkInvocation7 =
                        serde_json::from_value(record)?;
                    ensure!(
                        complete.ordinal == ordinal as u64 && complete.parent_stage == stage,
                        "compiler7_invocation_ordinal_changed"
                    );
                    let caller = Path::new(&complete.inputs.base_inputs.request.caller_cwd);
                    let approved = if caller == root.path() {
                        None
                    } else {
                        Some(crate::native_sources6::caller_root(
                            root, side, stage, &context, caller,
                        )?)
                    };
                    verify_one(
                        root,
                        side_ref,
                        side,
                        recipe,
                        stage,
                        &context,
                        cwd,
                        &complete,
                        approved.as_deref(),
                    )?;
                    CodeNativeInvocationEnvelope7 {
                        schema: "oev2.native-invocation-envelope/7".into(),
                        kind: "compiler7".into(),
                        archive3: None,
                        compiler7: Some(complete),
                    }
                }
                _ => anyhow::bail!("compiler7_unknown_invocation_schema"),
            };
            inspect_envelope(&envelope)?;
            all.push(envelope);
        }
        ensure!(
            crate::native_link6::validate_context(root, side_ref, side, recipe, stage)? == context,
            "compiler7_current_sources_changed_after_inventory"
        );
    }
    Ok(all)
}
pub(crate) fn parent_stage(v: &CodeNativeInvocationEnvelope7) -> Result<&str> {
    inspect_envelope(v)?;
    Ok(if let Some(a) = &v.archive3 {
        &a.parent_stage
    } else {
        &v.compiler7.as_ref().unwrap().parent_stage
    })
}
pub(crate) fn process_bindings(
    root: &SkillRoot,
    all: &[CodeNativeInvocationEnvelope7],
) -> Result<Vec<ArtifactReference>> {
    let mut out = Vec::new();
    for v in all {
        inspect_envelope(v)?;
        let (inputs, native) = if let Some(a) = &v.archive3 {
            (&a.inputs, &a.native_process)
        } else {
            let a = v.compiler7.as_ref().unwrap();
            (&a.inputs.base_inputs, &a.native_process)
        };
        for p in inputs
            .archives
            .iter()
            .map(|a| &a.index_process)
            .chain(std::iter::once(native))
        {
            out.push(crate::native_process6::binding_reference(root, p)?);
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_empty_or_relabelled_envelope_is_not_a_complete_invocation() {
        for (schema, kind) in [
            ("oev2.native-invocation-envelope/7", "compiler7"),
            ("oev2.native-invocation-envelope/7", "archive3"),
            ("oev2.native-invocation-envelope/6", "compiler7"),
        ] {
            let v = CodeNativeInvocationEnvelope7 {
                schema: schema.into(),
                kind: kind.into(),
                archive3: None,
                compiler7: None,
            };
            assert!(inspect_envelope(&v).is_err());
        }
    }
}
