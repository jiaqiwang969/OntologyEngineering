//! Retain real compiler-child inputs after the original /3 file-input refusal.
//! This is content evidence, never a new permission to execute a native link.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, write_new};
use std::path::Path;

const SCHEMA: &str = "oev2.native-compiler-input-content-observation/7";
const EXECUTION: &str = "content_only_after_original_hidden_file_refusal";
const UNCOVERED: [&str; 4] = [
    "actual compiler parent was observed; retained content cannot restore its live authority",
    "base archive facts describe the explicit file-option-free preparation substep, not a complete original /3 invocation",
    "opaque object content and export list capture do not qualify native object scope or authorize linking",
    "complete native pair bytes/debug/signatures and original whole library scope remain required",
];

/// This producer can only be called inside the crate with the actual parent
/// obtained by native_loader6, and borrows the existing stage/index checker.
pub(crate) fn capture<F, P>(
    root: &SkillRoot,
    side_context: &ArtifactReference,
    stage_context: &ArtifactReference,
    parent: &crate::native_loader6::ActualCompilerParent,
    original: &CodeNativeInputPreparationRequest,
    received: &ArtifactReference,
    cwd: &Path,
    approved_caller: Option<&Path>,
    indexer: &mut F,
    process_check: &mut P,
) -> Result<CodeNativeCompilerInputContentObservation7>
where
    F: FnMut(&CodeNativeArchiveIndexRequest) -> Result<CodeNativeInputProcessEvidence>,
    P: FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
{
    let request = CodeNativeLinkFileInputRequest7 {
        schema: "oev2.native-link-file-input-request/7".into(),
        received_arguments: original.received_arguments.clone(),
        caller_cwd: original.caller_cwd.clone(),
        target_root: original.target_root.clone(),
        capture_root: cwd
            .join("generated-files7")
            .to_str()
            .context("input_content7_path")?
            .into(),
    };
    let files = crate::native_link_files7::prepare_link_file_inputs7(root, &request)?;
    ensure!(
        !files.files.is_empty(),
        "input_content7_no_selected_file_option"
    );
    let mut base = original.clone();
    base.received_arguments = files.base_arguments.clone();
    // The old normalization rejected hidden I/O before creating its stage.
    // Capture opaque bytes here; the original native object admission checker
    // is still required by execution and is deliberately not replaced by this.
    let base_inputs = crate::native_archive::prepare_link_inputs_scoped_verified(
        root,
        &base,
        indexer,
        &mut |args| {
            ensure!(
                args.iter().any(|a| a == "-nodefaultlibs"),
                "input_content7_original_nodefaultlibs_missing"
            );
            Ok(())
        },
        None,
        approved_caller,
        process_check,
    )?;
    crate::native_link_files7::verify_live7(root, &files)?;
    crate::native_archive::verify_live_inputs(&base_inputs)?;
    let observation = CodeNativeCompilerInputContentObservation7 {
        schema: SCHEMA.into(),
        side_context: side_context.clone(),
        stage_context: stage_context.clone(),
        received: received.clone(),
        compiler_intent: parent.intent.clone(),
        compiler_pid: parent.pid,
        compiler_executable: parent.executable.clone(),
        file_inputs: files,
        index_process_count: base_inputs.archives.len() as u64,
        base_inputs,
        execution: EXECUTION.into(),
        uncovered: UNCOVERED.iter().map(|s| (*s).into()).collect(),
    };
    verify_retained(root, &observation, approved_caller, process_check)?;
    write_new(
        &cwd.join("compiler-input-content7.json"),
        &serde_json::to_value(&observation)?,
    )?;
    Ok(observation)
}

/// Inspect retained content and relationships. No JSON becomes a live parent,
/// native process owner, current original file, or engineering adoption here.
pub(crate) fn verify_retained<P>(
    root: &SkillRoot,
    observation: &CodeNativeCompilerInputContentObservation7,
    approved_caller: Option<&Path>,
    process_check: &mut P,
) -> Result<()>
where
    P: FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
{
    observation
        .validate()
        .map_err(|e| anyhow::anyhow!("input_content7_contract:{e:?}"))?;
    ensure!(
        observation.schema == SCHEMA
            && observation.execution == EXECUTION
            && observation.uncovered == UNCOVERED,
        "input_content7_schema_or_scope_changed"
    );
    crate::native_link_files7::verify_retained7(root, &observation.file_inputs)?;
    crate::native_archive::verify_prepared_scoped_verified(
        root,
        &observation.base_inputs,
        approved_caller,
        process_check,
    )?;
    let files = &observation.file_inputs;
    let base = &observation.base_inputs;
    ensure!(
        base.request.received_arguments == files.base_arguments
            && base.expanded_arguments == files.base_arguments
            && base.request.caller_cwd == files.request.caller_cwd
            && base.request.target_root == files.request.target_root
            && observation.index_process_count == base.archives.len() as u64,
        "input_content7_complete_base_projection_changed"
    );
    let original = crate::native_runtime::read(root, &observation.received)?;
    ensure!(
        original["side_context"] == serde_json::to_value(&observation.side_context)?
            && original["received_arguments"]
                == serde_json::to_value(&files.request.received_arguments)?
            && original["received_cwd"] == files.request.caller_cwd
            && original["stage_context_sha256"] == observation.stage_context.sha256,
        "input_content7_original_received_changed"
    );
    let intent: CodeNativeCompilerExecIntent6 = serde_json::from_value(
        crate::native_runtime::read(root, &observation.compiler_intent)?,
    )?;
    intent
        .validate()
        .map_err(|e| anyhow::anyhow!("input_content7_intent_contract:{e:?}"))?;
    ensure!(
        intent.schema == "oev2.native-compiler-exec-intent/6"
            && intent.execution == "before_exec"
            && intent.command_scope == "cargo_compiler_command"
            && intent.build_script_binding.is_none()
            && intent.pid == observation.compiler_pid
            && intent.compiler == observation.compiler_executable
            && intent.stage_context == observation.stage_context,
        "input_content7_original_compiler_relationship_changed"
    );
    let context = crate::native_runtime::read(root, &observation.stage_context)?;
    let cwd = root
        .private_path(Path::new(&observation.received.path))?
        .parent()
        .context("input_content7_received_parent")?
        .to_owned();
    ensure!(
        context["side_context"] == serde_json::to_value(&observation.side_context)?
            && original["raw_encoded_rustflags"].is_string()
            && Path::new(&files.request.capture_root) == cwd.join("generated-files7")
            && Path::new(&base.request.stage_root) == cwd.join("oe-native-debug")
            && Path::new(&base.request.evidence_root) == cwd.join("input-evidence")
            && observation.received
                == crate::native_runtime::reference(root, &cwd.join("received.json"))?,
        "input_content7_same_actual_stage_changed"
    );
    Ok(())
}
