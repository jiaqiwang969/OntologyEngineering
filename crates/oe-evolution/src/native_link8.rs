//! Complete SDK-bound executable links on the existing native parent/owner.
//! Library-free archive3 facts are an explicit preparation substep, never an
//! original3 invocation. The complete original argument map stays in /8.
use crate::native_runtime::{deadline_timeout, read, reference};
use crate::native_runtime6::{role, verify_pins};
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, read_regular_bytes, write_new};
use std::{path::Path, time::Instant};

pub(crate) fn is_adapter8(recipe: &CodeNativeBuildRecipe6) -> Result<bool> {
    Ok(Path::new(&role(recipe, "linker_adapter")?.path)
        .file_name()
        .and_then(|v| v.to_str())
        == Some("oe-native-sdk8-linker"))
}
fn project_library_arguments(args: &[String]) -> (Vec<String>, Vec<u64>) {
    let mut base = Vec::new();
    let mut ordinals = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !arg.starts_with("-l") {
            base.push(arg.clone());
            ordinals.push(i as u64);
        }
    }
    (base, ordinals)
}
fn complete_arguments(
    original: &[String],
    ordinals: &[u64],
    base_original: &[String],
    executed: &[String],
) -> Result<Vec<String>> {
    let (expected, expected_ordinals) = project_library_arguments(original);
    ensure!(
        expected == base_original
            && expected_ordinals == ordinals
            && executed.len() == ordinals.len(),
        "sdk8_base_original_ordinal_map_changed"
    );
    let mut result = original.to_vec();
    for (ordinal, value) in ordinals.iter().zip(executed) {
        *result
            .get_mut(usize::try_from(*ordinal)?)
            .context("sdk8_original_ordinal_missing")? = value.clone();
    }
    Ok(result)
}
fn verify_preparation(
    root: &SkillRoot,
    v: &CodeNativeSdkLinkPreparation8,
    approved: Option<&Path>,
    process_check: &mut dyn FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
) -> Result<()> {
    v.validate()
        .map_err(|e| anyhow::anyhow!("sdk8_preparation_contract:{e:?}"))?;
    ensure!(
        v.schema == "oev2.native-sdk-link-preparation/8"
            && v.source_assessment
                == crate::native_system_link8::inspect_source_bound_system_inputs8(
                    root,
                    &v.source_assessment.request
                )?
            && v.original_request.received_arguments == v.source_assessment.original_arguments,
        "sdk8_original_source_or_request_changed"
    );
    let (base, ordinals) = project_library_arguments(&v.original_request.received_arguments);
    let mut expected = v.original_request.clone();
    expected.received_arguments = base;
    ensure!(
        v.base_inputs.request == expected
            && v.base_inputs.expanded_arguments == expected.received_arguments
            && v.base_argument_ordinals == ordinals,
        "sdk8_preparation_substep_or_ordinals_changed"
    );
    crate::native_archive::verify_prepared_scoped_verified(
        root,
        &v.base_inputs,
        approved,
        process_check,
    )?;
    ensure!(
        v.complete_arguments
            == complete_arguments(
                &v.original_request.received_arguments,
                &ordinals,
                &v.base_inputs.expanded_arguments,
                &crate::native_link6::execution_arguments(&v.base_inputs)?
            )?,
        "sdk8_complete_original_argument_map_changed"
    );
    Ok(())
}
fn prepare_sdk_bound_inputs8<F>(
    root: &SkillRoot,
    request: &CodeNativeInputPreparationRequest,
    source: CodeNativeSystemInputAssessment8,
    approved: Option<&Path>,
    indexer: &mut F,
    process_check: &mut dyn FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
) -> Result<CodeNativeSdkLinkPreparation8>
where
    F: FnMut(&CodeNativeArchiveIndexRequest) -> Result<CodeNativeInputProcessEvidence>,
{
    ensure!(
        source.original_arguments == request.received_arguments,
        "sdk8_original_vector_changed_before_preparation"
    );
    let (base, ordinals) = project_library_arguments(&request.received_arguments);
    let mut inner = request.clone();
    inner.received_arguments = base;
    let base_inputs = crate::native_archive::prepare_link_inputs_scoped_verified(
        root,
        &inner,
        indexer,
        &mut |args| {
            ensure!(
                args.iter().any(|a| a == "-nodefaultlibs"),
                "sdk8_original_nodefaultlibs_missing"
            );
            Ok(())
        },
        Some(&mut crate::native_link6::check_original_input),
        approved,
        process_check,
    )?;
    let context = read(root, &source.request.stage_context)?;
    let recipe_ref: ArtifactReference = serde_json::from_value(context["recipe"].clone())?;
    let recipe: CodeNativeBuildRecipe6 = serde_json::from_value(read(root, &recipe_ref)?)?;
    crate::native_link6::check_native_scope(root, &base_inputs, &recipe)?;
    let complete = complete_arguments(
        &request.received_arguments,
        &ordinals,
        &base_inputs.expanded_arguments,
        &crate::native_link6::execution_arguments(&base_inputs)?,
    )?;
    let result=CodeNativeSdkLinkPreparation8{schema:"oev2.native-sdk-link-preparation/8".into(),original_request:request.clone(),
        source_assessment:source,base_inputs,base_argument_ordinals:ordinals,complete_arguments:complete,
        uncovered:vec!["Library-free archive3 facts are a checked staging substep; no complete original3 invocation is created".into()]};
    verify_preparation(root, &result, approved, process_check)?;
    Ok(result)
}
pub(crate) fn capture_execute(
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
    request: &CodeNativeInputPreparationRequest,
    approved: Option<&Path>,
    deadline: Instant,
    end: u64,
) -> Result<i32> {
    ensure!(is_adapter8(recipe)?, "sdk8_selected_adapter_required");
    crate::native_loader6::recheck_live_parent(parent)?;
    let source_request = CodeNativeSystemInputRequest8 {
        schema: "oev2.native-system-input-request/8".into(),
        stage_context: reference(root, context_path)?,
        compiler_intent: parent.intent.clone(),
        received: reference(root, &cwd.join("received.json"))?,
        original_refusal: reference(root, &cwd.join("original3-preparation-refusal.json"))?,
    };
    let source =
        crate::native_system_link8::inspect_source_bound_system_inputs8(root, &source_request)?;
    deadline_timeout(deadline)?;
    let mut count = 0;
    let inputs = prepare_sdk_bound_inputs8(
        root,
        request,
        source,
        approved,
        &mut |index: &CodeNativeArchiveIndexRequest| {
            deadline_timeout(deadline)?;
            verify_pins(root, recipe)?;
            crate::native_loader6::recheck_live_parent(parent)?;
            let folder = cwd.join(format!("sdk8-archive-index-{count:06}"));
            count += 1;
            crate::native_link6::observe_process(
                root,
                &index.executable,
                index.arguments.clone(),
                Path::new(&index.cwd),
                &folder,
                deadline,
                end,
                usize::try_from(side.body_budget.output_bytes)?,
                "archive_index",
                parent,
            )
        },
        &mut |p| crate::native_process6::verify(root, p, side, recipe, context_path),
    )?;
    write_new(
        &cwd.join("sdk8-preparation.json"),
        &serde_json::to_value(&inputs)?,
    )?;
    crate::native_archive::verify_live_inputs(&inputs.base_inputs)?;
    ensure!(
        !Path::new(&inputs.source_assessment.domain.original_output).exists(),
        "sdk8_output_not_fresh"
    );
    ensure!(
        crate::native_link6::validate_context(root, side_ref, side, recipe, stage)? == *context
            && read_regular_bytes(context_path, 16 * 1024 * 1024)? == context_bytes,
        "sdk8_source_context_changed_before_link"
    );
    let mut args = inputs.complete_arguments.clone();
    let suffix = crate::native_link6::append_native(root, &mut args, recipe, cwd)?;
    let files = crate::native_link6::input_files(&inputs.base_inputs, recipe);
    for file in &files {
        crate::native_profile::verify_file(file)?;
    }
    crate::native_sdkroot6::recheck_actual(root, recipe, &parent.sdk_root)?;
    let sdk = verify_pins(root, recipe)?;
    crate::native_loader6::retain_native_receipt(
        root,
        cwd,
        parent,
        inputs.source_assessment.selected_libraries.clone(),
        &sdk,
    )?;
    let snapshot = serde_json::json!({"files":files,"sdk":sdk,"source_manifest":crate::public_manifest(root.path())?,
        "raw_encoded_rustflags":recipe.raw_encoded_rustflags});
    write_new(&cwd.join("inputs-before.json"), &snapshot)?;
    deadline_timeout(deadline)?;
    crate::native_loader6::recheck_live_parent(parent)?;
    let process = crate::native_link6::observe_process(
        root,
        role(recipe, "clang_driver")?,
        args,
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
    crate::native_archive::verify_live_inputs(&inputs.base_inputs)?;
    for file in &files {
        crate::native_profile::verify_file(file)?;
    }
    ensure!(
        verify_pins(root, recipe)? == sdk
            && crate::public_manifest(root.path())? == context["source_manifest"]
            && read_regular_bytes(context_path, 16 * 1024 * 1024)? == context_bytes
            && crate::native_link6::validate_context(root, side_ref, side, recipe, stage)?
                == *context,
        "sdk8_inputs_changed_after_link"
    );
    write_new(&cwd.join("inputs-after.json"), &snapshot)?;
    let invocation = CodeNativeSdkLinkInvocation8 {
        schema: "oev2.native-sdk-link-invocation/8".into(),
        ordinal,
        parent_stage: stage.into(),
        inputs,
        native_recipe_suffix: suffix,
        native_process: process,
        uncovered: vec![
            "Actual same compiler/index/clang owner; no release or engineering adoption authority"
                .into(),
        ],
    };
    write_new(
        &cwd.join("invocation.json"),
        &serde_json::to_value(&invocation)?,
    )?;
    verify_one(
        root,
        side_ref,
        side,
        recipe,
        stage,
        context,
        cwd,
        &invocation,
        approved,
    )?;
    deadline_timeout(deadline)?;
    i32::try_from(
        invocation
            .native_process
            .exit_code
            .context("sdk8_native_exit_missing")?,
    )
    .context("sdk8_exit_range")
}
pub(crate) fn verify_one(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    stage: &str,
    context: &serde_json::Value,
    cwd: &Path,
    v: &CodeNativeSdkLinkInvocation8,
    approved: Option<&Path>,
) -> Result<()> {
    v.validate()
        .map_err(|e| anyhow::anyhow!("sdk8_invocation_contract:{e:?}"))?;
    ensure!(
        is_adapter8(recipe)?
            && v.schema == "oev2.native-sdk-link-invocation/8"
            && v.parent_stage == stage
            && cwd.file_name().and_then(|p| p.to_str())
                == Some(format!("call-{:06}", v.ordinal).as_str()),
        "sdk8_invocation_schema_or_ordinal"
    );
    let context_path = Path::new(&side.stage_root)
        .join(stage)
        .join("stage-context.json");
    let r = &v.inputs.original_request;
    ensure!(
        v.inputs.source_assessment.request.stage_context == reference(root, &context_path)?
            && v.inputs.source_assessment.request.received
                == reference(root, &cwd.join("received.json"))?
            && v.inputs.source_assessment.request.original_refusal
                == reference(root, &cwd.join("original3-preparation-refusal.json"))?
            && r.algorithm == recipe.algorithm
            && r.target_root == side.build.target_dir
            && Path::new(&r.stage_root) == cwd.join("oe-native-debug")
            && Path::new(&r.evidence_root) == cwd.join("input-evidence")
            && r.external_archives == recipe.external_archive_bindings
            && r.index_executable == *role(recipe, "archive_reader")?
            && r.index_environment == crate::native_link6::environment()
            && read(root, &reference(root, &cwd.join("received.json"))?)?["side_context"]
                == serde_json::to_value(side_ref)?
            && v.inputs
                == serde_json::from_value(read(
                    root,
                    &reference(root, &cwd.join("sdk8-preparation.json"))?
                )?)?,
        "sdk8_complete_original_preparation_changed"
    );
    verify_preparation(root, &v.inputs, approved, &mut |p| {
        crate::native_process6::verify(root, p, side, recipe, &context_path)
    })?;
    crate::native_link6::check_native_scope(root, &v.inputs.base_inputs, recipe)?;
    let mut args = v.inputs.complete_arguments.clone();
    let suffix = crate::native_link6::append_native(root, &mut args, recipe, cwd)?;
    ensure!(
        v.native_recipe_suffix == suffix
            && v.native_process.arguments == args
            && v.native_process.executable == *role(recipe, "clang_driver")?
            && Path::new(&v.native_process.cwd) == cwd
            && v.native_process.environment == crate::native_link6::environment()
            && v.native_process.role == "native_link"
            && v.native_process.process
                == reference(root, &cwd.join("native-process/process.json"))?
            && v.native_process.stdout == reference(root, &cwd.join("native-process/stdout"))?
            && v.native_process.stderr == reference(root, &cwd.join("native-process/stderr"))?,
        "sdk8_actual_process_or_arguments_changed"
    );
    let before = read(root, &reference(root, &cwd.join("inputs-before.json"))?)?;
    let after = read(root, &reference(root, &cwd.join("inputs-after.json"))?)?;
    ensure!(
        before == after
            && before["files"]
                == serde_json::to_value(crate::native_link6::input_files(
                    &v.inputs.base_inputs,
                    recipe
                ))?
            && before["sdk"] == serde_json::to_value(verify_pins(root, recipe)?)?
            && before["source_manifest"] == crate::public_manifest(root.path())?
            && before["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
        "sdk8_full_before_after_changed"
    );
    let end = context["deadline_epoch_ms"]
        .as_u64()
        .context("sdk8_original_deadline_missing")?;
    for (i, archive) in v.inputs.base_inputs.archives.iter().enumerate() {
        let folder = cwd.join(format!("sdk8-archive-index-{i:06}"));
        ensure!(
            archive.index_process.process == reference(root, &folder.join("process.json"))?,
            "sdk8_index_callback_side_changed"
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
            &archive.index_request.executable,
            Path::new(&archive.index_request.cwd),
            &archive.index_request.arguments,
        )?;
    }
    crate::native_process6::verify(root, &v.native_process, side, recipe, &context_path)?;
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
    Ok(())
}
pub(crate) fn inspect_envelope(v: &CodeNativeInvocationEnvelope8) -> Result<()> {
    v.validate()
        .map_err(|e| anyhow::anyhow!("sdk8_envelope_contract:{e:?}"))?;
    ensure!(
        v.schema == "oev2.native-invocation-envelope/8"
            && match v.kind.as_str() {
                "archive3" => v.archive3.is_some() && v.compiler7.is_none() && v.sdk8.is_none(),
                "compiler7" => v.archive3.is_none() && v.compiler7.is_some() && v.sdk8.is_none(),
                "sdk8" => v.archive3.is_none() && v.compiler7.is_none() && v.sdk8.is_some(),
                _ => false,
            },
        "sdk8_exclusive_actual_record_changed"
    );
    Ok(())
}
pub(crate) fn borrowed_inputs(
    v: &CodeNativeInvocationEnvelope8,
) -> Result<(
    &CodeNativeInputPreparationFacts,
    &CodeNativeInputProcessEvidence,
)> {
    inspect_envelope(v)?;
    Ok(if let Some(a) = &v.archive3 {
        (&a.inputs, &a.native_process)
    } else if let Some(a) = &v.compiler7 {
        (&a.inputs.base_inputs, &a.native_process)
    } else {
        let a = v.sdk8.as_ref().unwrap();
        (&a.inputs.base_inputs, &a.native_process)
    })
}
pub(crate) fn parent_stage(v: &CodeNativeInvocationEnvelope8) -> Result<&str> {
    inspect_envelope(v)?;
    Ok(if let Some(a) = &v.archive3 {
        &a.parent_stage
    } else if let Some(a) = &v.compiler7 {
        &a.parent_stage
    } else {
        &v.sdk8.as_ref().unwrap().parent_stage
    })
}
pub(crate) fn process_bindings(
    root: &SkillRoot,
    all: &[CodeNativeInvocationEnvelope8],
) -> Result<Vec<ArtifactReference>> {
    let mut result = Vec::new();
    for v in all {
        let (inputs, native) = borrowed_inputs(v)?;
        for p in inputs
            .archives
            .iter()
            .map(|a| &a.index_process)
            .chain(std::iter::once(native))
        {
            result.push(crate::native_process6::binding_reference(root, p)?);
        }
    }
    Ok(result)
}
pub(crate) fn load_invocations(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<CodeNativeInvocationEnvelope8>> {
    ensure!(is_adapter8(recipe)?, "sdk8_selected_adapter_required");
    let mut all = Vec::new();
    for stage in ["build-normalizer", "build-runner"] {
        let context = crate::native_link6::validate_context(root, side_ref, side, recipe, stage)?;
        let folder = Path::new(&side.stage_root).join(stage);
        let mut calls = std::fs::read_dir(&folder)?
            .map(|p| p.map(|p| p.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        calls.retain(|p| {
            p.file_name()
                .and_then(|p| p.to_str())
                .is_some_and(|p| p.starts_with("call-"))
        });
        calls.sort();
        ensure!(
            !calls.is_empty() && calls.len() <= 4096,
            "sdk8_complete_call_inventory_empty_or_limit"
        );
        for (ordinal, cwd) in calls.iter().enumerate() {
            ensure!(
                cwd.file_name().and_then(|p| p.to_str())
                    == Some(format!("call-{ordinal:06}").as_str()),
                "sdk8_call_inventory_gap"
            );
            let record = read(root, &reference(root, &cwd.join("invocation.json"))?)?;
            let mut e = CodeNativeInvocationEnvelope8 {
                schema: "oev2.native-invocation-envelope/8".into(),
                kind: String::new(),
                archive3: None,
                compiler7: None,
                sdk8: None,
            };
            let caller = match record["schema"].as_str() {
                Some("oev2.native-archive-response-invocation/3") => {
                    let a: CodeNativeArchiveResponseInvocation = serde_json::from_value(record)?;
                    let caller = a.inputs.request.caller_cwd.clone();
                    e.kind = "archive3".into();
                    e.archive3 = Some(a);
                    caller
                }
                Some("oev2.native-compiler-link-invocation/7") => {
                    let a: CodeNativeCompilerLinkInvocation7 = serde_json::from_value(record)?;
                    let caller = a.inputs.base_inputs.request.caller_cwd.clone();
                    e.kind = "compiler7".into();
                    e.compiler7 = Some(a);
                    caller
                }
                Some("oev2.native-sdk-link-invocation/8") => {
                    let a: CodeNativeSdkLinkInvocation8 = serde_json::from_value(record)?;
                    let caller = a.inputs.original_request.caller_cwd.clone();
                    e.kind = "sdk8".into();
                    e.sdk8 = Some(a);
                    caller
                }
                _ => anyhow::bail!("sdk8_unknown_original_invocation_schema"),
            };
            let approved = if Path::new(&caller) == root.path() {
                None
            } else {
                Some(crate::native_sources6::caller_root(
                    root,
                    side,
                    stage,
                    &context,
                    Path::new(&caller),
                )?)
            };
            if let Some(a) = &e.archive3 {
                ensure!(
                    a.ordinal == ordinal as u64 && a.parent_stage == stage,
                    "sdk8_original3_ordinal_changed"
                );
                crate::native_link6::verify_one_invocation(
                    root,
                    side_ref,
                    side,
                    recipe,
                    stage,
                    &context,
                    cwd,
                    a,
                    approved.as_deref(),
                )?;
            }
            if let Some(a) = &e.compiler7 {
                ensure!(
                    a.ordinal == ordinal as u64 && a.parent_stage == stage,
                    "sdk8_compiler7_ordinal_changed"
                );
                crate::native_link7::verify_one_in_adapter8(
                    root,
                    side_ref,
                    side,
                    recipe,
                    stage,
                    &context,
                    cwd,
                    a,
                    approved.as_deref(),
                )?;
            }
            if let Some(a) = &e.sdk8 {
                ensure!(
                    a.ordinal == ordinal as u64 && a.parent_stage == stage,
                    "sdk8_ordinal_changed"
                );
                verify_one(
                    root,
                    side_ref,
                    side,
                    recipe,
                    stage,
                    &context,
                    cwd,
                    a,
                    approved.as_deref(),
                )?;
            }
            inspect_envelope(&e)?;
            all.push(e);
        }
        ensure!(
            crate::native_link6::validate_context(root, side_ref, side, recipe, stage)? == context,
            "sdk8_current_sources_changed_after_inventory"
        );
    }
    Ok(all)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|v| (*v).into()).collect()
    }
    #[test]
    fn library_projection_preserves_repeats_and_every_other_original_ordinal() {
        let original = args(&[
            "first.o",
            "-lc",
            "-liconv",
            "-L",
            "/selected",
            "second.a",
            "-lc",
            "-o",
            "/target/out",
            "-nodefaultlibs",
        ]);
        let (base, ordinals) = project_library_arguments(&original);
        assert_eq!(ordinals, vec![0, 3, 4, 5, 7, 8, 9]);
        let mut staged = base.clone();
        staged[0] = "oe-native-debug/objects/first.o".into();
        staged[3] = "oe-native-debug/archives/second.a".into();
        let full = complete_arguments(&original, &ordinals, &base, &staged).unwrap();
        assert_eq!(
            full,
            args(&[
                "oe-native-debug/objects/first.o",
                "-lc",
                "-liconv",
                "-L",
                "/selected",
                "oe-native-debug/archives/second.a",
                "-lc",
                "-o",
                "/target/out",
                "-nodefaultlibs"
            ])
        );
    }
    #[test]
    fn missing_duplicate_reordered_or_relabelled_projection_is_rejected() {
        let original = args(&["first.o", "-liconv", "-o", "/target/out"]);
        let (base, ordinals) = project_library_arguments(&original);
        for map in [vec![0, 2], vec![0, 0, 3], vec![2, 0, 3], vec![0, 2, 99]] {
            assert!(complete_arguments(&original, &map, &base, &base).is_err());
        }
        assert!(complete_arguments(&original, &ordinals, &base, &base[..2]).is_err());
        let changed = args(&["first.o", "-o", "/other/out"]);
        assert!(complete_arguments(&original, &ordinals, &changed, &changed).is_err());
    }
    #[test]
    fn sdk8_empty_or_old_schema_envelope_cannot_forge_an_original_record() {
        for (schema, kind) in [
            ("oev2.native-invocation-envelope/8", "sdk8"),
            ("oev2.native-invocation-envelope/8", "archive3"),
            ("oev2.native-invocation-envelope/7", "compiler7"),
        ] {
            assert!(
                inspect_envelope(&CodeNativeInvocationEnvelope8 {
                    schema: schema.into(),
                    kind: kind.into(),
                    archive3: None,
                    compiler7: None,
                    sdk8: None
                })
                .is_err()
            );
        }
    }
}
