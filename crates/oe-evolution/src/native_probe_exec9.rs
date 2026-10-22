//! Explicit execution9 uses the original owner and complete SDK8 artifacts.
//! Serialized policies, producer facts and old PIDs never authorize execution.
use crate::native_runtime::{epoch_ms, read, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const PROFILE: &str = "semantica.rust-full-native-build-script-probes/9";
const POLICY_FILE: &str = "compiler-probe-policy9.json";
const MAX_RECORDS: usize = 16384;

fn selected_policy9(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
) -> Result<CodeNativeCompilerProbePolicy9> {
    let profile = crate::native_profile6::inspect_profile(root, &side.source_profile)?;
    selected_policy9_with_profile(root, side, recipe, context_path, context, &profile)
}
fn selected_policy9_with_profile(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
    profile: &CodeNativeRuntimeProfile6,
) -> Result<CodeNativeCompilerProbePolicy9> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?,
        "probe9_complete_sdk8_adapter_required"
    );
    let loader = crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?;
    ensure!(
        loader.metadata_probe == crate::native_buildscript6::POLICY
            && Path::new(&loader.rustc_wrapper.path)
                .file_name()
                .is_some_and(|v| v == "oe-native-probe9-rustc-exec"),
        "probe9_original_metadata_policy_or_explicit_wrapper_not_selected"
    );
    ensure!(
        context.schema == "oev2.native-build-stage/6"
            && context.source_root == root.path().to_str().context("probe9_root_utf8")?
            && context.source_manifest == public_manifest(root.path())?
            && context.recipe == profile.recipe
            && context.output_bytes == side.body_budget.output_bytes
            && (1..=4 * 1024 * 1024).contains(&context.output_bytes)
            && context.deadline_epoch_ms > 0
            && ["build-normalizer", "build-runner"].contains(&context.parent_stage.as_str())
            && context_path
                == Path::new(&side.stage_root)
                    .join(&context.parent_stage)
                    .join("stage-context.json"),
        "probe9_current_original_stage_changed"
    );
    let value = CodeNativeCompilerProbePolicy9 {
        schema: "oev2.native-compiler-probe-policy/9".into(),
        execution_profile: PROFILE.into(),
        source_profile: side.source_profile.clone(),
        stage_context: reference(root, context_path)?,
        original_loader_policy: recipe.cargo_loader_policy.clone(),
        source_manifest_sha256: digest(&context.source_manifest)?,
        deadline_epoch_ms: context.deadline_epoch_ms,
        output_bytes: context.output_bytes,
        domain: "original_file_or_actual_fifo_metadata_only/9".into(),
    };
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("probe9_policy_contract:{e:?}"))?;
    Ok(value)
}

pub(crate) fn write_policy9(
    session: &crate::native_runtime6::NativeBuildSession6,
    stage: &str,
) -> Result<()> {
    crate::native_runtime::deadline_timeout(session.deadline)?;
    let context_path = Path::new(&session.side.stage_root)
        .join(stage)
        .join("stage-context.json");
    let context: CodeNativeStageContext6 = serde_json::from_value(read(
        &session.root,
        &reference(&session.root, &context_path)?,
    )?)?;
    let policy = selected_policy9(
        &session.root,
        &session.side,
        &session.recipe,
        &context_path,
        &context,
    )?;
    write_new(
        &session
            .root
            .private_path(&context_path.with_file_name(POLICY_FILE))?,
        &serde_json::to_value(policy)?,
    )?;
    crate::native_runtime::deadline_timeout(session.deadline)?;
    Ok(())
}

pub(crate) fn inspect_policy9(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
) -> Result<(ArtifactReference, CodeNativeCompilerProbePolicy9)> {
    let path = context_path.with_file_name(POLICY_FILE);
    let record = reference(root, &path)?;
    let policy: CodeNativeCompilerProbePolicy9 = serde_json::from_value(read(root, &record)?)?;
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("probe9_selected_policy_contract:{e:?}"))?;
    ensure!(
        policy == selected_policy9(root, side, recipe, context_path, context)?
            && reference(root, &path)? == record,
        "probe9_selected_policy_changed"
    );
    Ok((record, policy))
}

/// Reuse only the already fully inspected stage owned by this call. Other
/// policy write/retention paths still use the full original profile inspector.
fn inspect_policy9_checked(
    root: &SkillRoot,
    checked: &crate::native_loader6::CheckedCompilerStage6,
    context_path: &Path,
) -> Result<(ArtifactReference, CodeNativeCompilerProbePolicy9)> {
    checked.recheck(root)?;
    let (context, side, recipe, profile) = checked.facts();
    let path = context_path.with_file_name(POLICY_FILE);
    let record = reference(root, &path)?;
    let policy: CodeNativeCompilerProbePolicy9 = serde_json::from_value(read(root, &record)?)?;
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("probe9_selected_policy_contract:{e:?}"))?;
    ensure!(
        policy
            == selected_policy9_with_profile(root, side, recipe, context_path, context, profile)?
            && reference(root, &path)? == record,
        "probe9_selected_policy_changed"
    );
    Ok((record, policy))
}

fn events(side: &CodeNativeSideContext6, context: &CodeNativeStageContext6) -> PathBuf {
    Path::new(&side.stage_root)
        .join(&context.parent_stage)
        .join("loader-events")
}
fn input_content9(intent: &CodeNativeCompilerProbeIntent9) -> Result<()> {
    intent
        .validate()
        .map_err(|e| anyhow::anyhow!("probe9_intent_contract:{e:?}"))?;
    ensure!(
        intent.pid > 0
            && intent.parent_pid > 0
            && intent.execution == "before_compiler_spawn"
            && intent.producer.parent_pid == intent.parent_pid
            && intent.producer.parent_executable == intent.parent_executable
            && intent.producer.stage_context == intent.stage_context
            && intent.producer.package_id == intent.domain.package_id
            && intent.arguments == intent.domain.arguments
            && intent.cwd == intent.domain.cwd,
        "probe9_current_intent_producer_domain_changed"
    );
    match (
        intent.domain.input_kind.as_str(),
        &intent.domain.source_file,
        &intent.stdin_observation,
        &intent.captured_stdin_file,
    ) {
        ("original_file", Some(_), None, None) => Ok(()),
        ("actual_stdin_required", None, Some(observation), Some(file)) => {
            ensure!(
                observation.delivery == "captured_only_no_compiler_exec"
                    && observation.bytes == file.bytes
                    && observation.sha256 == file.sha256
                    && file.mode == 0o600
                    && observation.bytes <= observation.original_byte_limit,
                "probe9_actual_stdin_content_changed"
            );
            Ok(())
        }
        _ => anyhow::bail!("probe9_input_observation_missing_or_foreign"),
    }
}

fn raw_intent9(
    raw: &CodeNativeCompilerProbeRaw9,
    intent: &CodeNativeCompilerProbeIntent9,
) -> Result<()> {
    raw.validate()
        .map_err(|e| anyhow::anyhow!("probe9_raw_contract:{e:?}"))?;
    input_content9(intent)?;
    ensure!(
        raw.pid == intent.pid
            && raw.parent_pid == intent.parent_pid
            && raw.parent_executable == intent.parent_executable
            && raw.compiler == intent.compiler
            && raw.wrapper == intent.wrapper
            && raw.arguments == intent.arguments
            && raw.cwd == intent.cwd
            && raw.stage_context == intent.stage_context
            && raw.probe_policy == intent.probe_policy
            && raw.actual_encoded_rustflags.as_deref()
                == Some(intent.domain.raw_encoded_rustflags.as_str()),
        "probe9_raw_intent_binding_changed"
    );
    if let Some(stdin) = &intent.stdin_observation {
        ensure!(
            raw.stdin_descriptor.as_ref() == Some(&stdin.original_descriptor),
            "probe9_original_fifo_identity_changed"
        );
    }
    ensure!(
        raw.selected_loader_environment_raw.len() == 2,
        "probe9_raw_loader_occurrences"
    );
    let raw_values = raw
        .selected_loader_environment_raw
        .iter()
        .map(|entry| {
            Ok((
                entry["name"].as_str().context("probe9_raw_loader_name")?,
                entry["value_bytes"].clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    ensure!(
        raw_values.len() == 2
            && raw_values.contains_key("DYLD_FALLBACK_LIBRARY_PATH")
            && raw_values.contains_key("DYLD_PRINT_LIBRARIES"),
        "probe9_raw_loader_inventory"
    );
    for (name, value) in raw_values {
        let selected = intent
            .loader_environment
            .iter()
            .filter(|entry| entry.name == name)
            .collect::<Vec<_>>();
        ensure!(selected.len() <= 1, "probe9_duplicate_loader_value");
        let expected = selected
            .first()
            .map_or(Value::Null, |entry| json!(entry.value.as_bytes()));
        ensure!(value == expected, "probe9_original_loader_value_changed");
    }
    Ok(())
}

fn terminal_kind9(process: &Value) -> Result<&'static str> {
    ensure!(
        process["capture_complete"] == true
            && process["streams_collected"] == 2
            && process["stdout"]["truncated"] == false
            && process["stderr"]["truncated"] == false
            && process["executable"]["unchanged"] == true
            && process["spawn_error_kind"].is_null()
            && process["borrowed_deadline"]["schema"] == "oe-core.process-borrowed-deadline/1"
            && process["borrowed_deadline"]["expired"] == false,
        "probe9_terminal_partial_changed_or_expired"
    );
    if process.get("stdin").is_some() {
        ensure!(
            process["stdin"]["configured_for_spawn"] == true
                && process["stdin"]["process_spawned"] == true
                && process["stdin"]["unchanged"] == true
                && process["stdin"]["target_consumption"] == "not_inferred"
                && process["stdin"]["semantic_support"] == "not_inferred",
            "probe9_terminal_stdin_changed"
        );
    }
    match (process["status"].as_str(), process["exit_code"].as_i64()) {
        (Some("completed"), Some(0)) => Ok("compiler_completed"),
        (Some("process_failed"), Some(code)) if code > 0 => Ok("compiler_rejected"),
        _ => anyhow::bail!("probe9_terminal_not_real_compiler_completion"),
    }
}
fn file(path: &Path) -> Result<CodeNativeFileBinding> {
    crate::native_runtime::file_binding(path)
}

#[cfg(unix)]
pub fn compiler_probe_entry9() -> Result<i32> {
    use std::os::{fd::AsFd, unix::ffi::OsStrExt};
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        argv.len() >= 2
            && argv.len() <= 10000
            && argv.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "probe9_compiler_arguments"
    );
    let context_path = PathBuf::from(std::env::var("OE_NATIVE_STAGE_CONTEXT")?);
    let raw_context =
        oe_wire::json::strict_json(&read_regular_bytes(&context_path, 16 * 1024 * 1024)?)?;
    let root = SkillRoot::new(raw_context["source_root"].as_str().context("probe9_root")?)?;
    let checked = crate::native_loader6::checked_stage6(&root, &context_path)?;
    let (context, side, recipe, _) = checked.facts();
    let (policy_ref, policy) = inspect_policy9_checked(&root, &checked, &context_path)?;
    let loader = crate::native_loader6::inspect_policy(&root, &recipe.cargo_loader_policy)?;
    ensure!(
        argv[0] == loader.compiler.path
            && argv[0] == side.build.rustc_path
            && std::env::current_exe()?.canonicalize()? == Path::new(&loader.rustc_wrapper.path),
        "probe9_compiler_or_wrapper_changed"
    );
    let own = &crate::driver_root().context("probe9_own_root")?;
    ensure!(
        digest(&public_manifest(own)?)? == compiled()?["manifest_sha256"],
        "probe9_wrapper_source_mismatch"
    );
    let parent_pid = crate::native_loader6::parent_pid()?;
    let parent = crate::native_loader6::process_executable(parent_pid)?;
    // The original6 entry handles all actual Cargo compiler commands and the
    // unchanged four read-only build-script metadata vectors at this same PID.
    if parent == loader.cargo || crate::native_buildscript6::metadata(&argv[1..]).is_ok() {
        crate::native_loader6::compiler_exec_entry_from_checked_stage6(checked, argv)?;
        anyhow::bail!("probe9_original_exec_unexpected_return");
    }
    let cwd = std::env::current_dir()?;
    let directory = events(&side, &context);
    let pid = std::process::id() as u64;
    let stdin = std::io::stdin();
    let raw = CodeNativeCompilerProbeRaw9 {
        schema: "oev2.native-compiler-probe-pre-admission-raw/9".into(), pid, parent_pid,
        parent_executable: parent.clone(), compiler: loader.compiler.clone(), wrapper: loader.rustc_wrapper.clone(),
        arguments: argv[1..].to_vec(), cwd: cwd.to_str().context("probe9_cwd")?.into(),
        stage_context: reference(&root, &context_path)?, probe_policy: policy_ref.clone(),
        actual_encoded_rustflags: std::env::var("CARGO_ENCODED_RUSTFLAGS").ok(),
        stdin_descriptor: crate::native_compiler_probe9::descriptor(stdin.as_fd()).ok(),
        selected_loader_environment_raw: ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"].into_iter()
            .map(|name| json!({"name": name,"value_bytes": std::env::var_os(name).map(|value| value.as_bytes().to_vec())})).collect(),
        execution: "before_admission_no_exec".into(),
    };
    let raw_path = root.private_path(&directory.join(format!("probe9-raw-{pid}.json")))?;
    write_new(&raw_path, &serde_json::to_value(&raw)?)?;
    ensure!(
        raw.actual_encoded_rustflags.as_deref() == Some(recipe.raw_encoded_rustflags.as_str()),
        "probe9_original_actual_flags_changed"
    );
    let remaining = policy
        .deadline_epoch_ms
        .checked_sub(epoch_ms()?)
        .filter(|v| *v > 0)
        .context("probe9_original_stage_deadline")?;
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(remaining))
        .context("probe9_deadline_overflow")?;
    let source = crate::native_sources6::caller_source(
        &root,
        &side,
        &context.parent_stage,
        &serde_json::to_value(&context)?,
        &cwd,
    )?;
    let producer = crate::native_producer9::inspect(
        &root,
        &side,
        &recipe,
        &context_path,
        &context,
        &loader,
        &source,
        parent_pid,
        &parent,
        true,
    )?;
    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    let target = root.private_path(Path::new(&side.build.target_dir))?;
    let scope = crate::native_compiler_probe9::ProbeInputScope9 {
        target_dir: &target,
        platform: &side.build.platform,
        stage: &context.parent_stage,
        cargo_out_dir: &out,
        encoded_flags: &recipe.raw_encoded_rustflags,
        actual_encoded_flags: raw.actual_encoded_rustflags.as_deref().unwrap(),
        cwd: &cwd,
    };
    let domain = crate::native_compiler_probe9::check_input_domain9(&source, &argv[1..], &scope)?;
    let (stdin_observation, captured_stdin_file) = if domain.input_kind == "actual_stdin_required" {
        let captured = crate::native_compiler_probe9::capture_stdin9(
            stdin.as_fd(),
            deadline,
            policy.output_bytes as usize,
        )?;
        let path = root.private_path(&directory.join(format!("probe9-input-{pid}.bin")))?;
        write_new_bytes(&path, &captured.bytes)?;
        (Some(captured.observation), Some(file(&path)?))
    } else {
        crate::native_loader6::inspect_null_descriptor6(0)?;
        (None, None)
    };
    let intent = CodeNativeCompilerProbeIntent9 {
        schema: "oev2.native-compiler-probe-intent/9".into(), pid, parent_pid, parent_executable: parent.clone(),
        compiler: loader.compiler.clone(), wrapper: loader.rustc_wrapper.clone(), arguments: argv[1..].to_vec(),
        cwd: raw.cwd.clone(), stage_context: raw.stage_context.clone(), probe_policy: policy_ref.clone(), raw_observation: reference(&root, &raw_path)?, producer, domain,
        stdin_observation, captured_stdin_file,
        loader_environment: crate::native_loader6::selected_environment(&side, &loader, true, None)?,
        loaded_images: crate::native_loader6::current_images()?, execution: "before_compiler_spawn".into(),
        uncovered: vec!["Original parent ProcessTree monotonic deadline is authoritative; this child checks one inherited absolute deadline and shares its local Instant across capture and replay, never a renewed budget".into(),
            "Wrapper image observations do not prove every child compiler loader/OS-cache image or actual symbol use; source/transient/hard-I/O and engineering closure remain separate".into()],
    };
    raw_intent9(&raw, &intent)?;
    let intent_path = directory.join(format!("probe9-intent-{pid}.json"));
    write_new(
        &root.private_path(&intent_path)?,
        &serde_json::to_value(&intent)?,
    )?;
    for selected in [&loader.compiler, &loader.rustc_wrapper, &parent] {
        crate::native_profile::verify_file(selected)?;
    }
    ensure!(
        crate::native_loader6::process_executable(parent_pid)? == parent
            && epoch_ms()? < policy.deadline_epoch_ms
            && Instant::now() < deadline,
        "probe9_actual_parent_or_original_deadline_changed_before_spawn"
    );
    ensure!(
        inspect_policy9_checked(&root, &checked, &context_path)? == (policy_ref, policy),
        "probe9_policy_changed_before_spawn"
    );
    let process_directory = root.private_path(&directory.join(format!("probe9-process-{pid}")))?;
    let spec = oe_core::process::CommandSpec {
        executable: loader.compiler.path.clone().into(),
        arguments: intent.arguments.iter().map(Into::into).collect(),
        cwd: cwd.clone(),
        environment: BTreeMap::new(),
        remove_environment: vec![],
        timeout: deadline.saturating_duration_since(Instant::now()),
        output_limit: context.output_bytes as usize,
    };
    let actual = if let Some(input) = &intent.captured_stdin_file {
        let expected = oe_core::FileIdentity {
            sha256: input.sha256.clone(),
            bytes: input.bytes,
        };
        oe_core::process::run_command_with_stdin_file_deadline(
            &spec,
            &process_directory,
            deadline,
            Path::new(&input.path),
            &expected,
        )?
    } else {
        oe_core::process::run_command_with_deadline(&spec, &process_directory, deadline)?
    };
    let process_path = process_directory.join("process.json");
    let process_value = read(&root, &reference(&root, &process_path)?)?;
    let unchanged = (|| -> Result<()> {
        for selected in [&loader.compiler, &loader.rustc_wrapper, &parent] {
            crate::native_profile::verify_file(selected)?;
        }
        crate::native_compiler_probe9::check_input_domain9(&source, &intent.arguments, &scope)?;
        if let Some(input) = &intent.captured_stdin_file {
            crate::native_stdin_file9::verify_stdin_file9(input, context.output_bytes, 0o600)?;
        }
        ensure!(
            epoch_ms()? < context.deadline_epoch_ms && Instant::now() < deadline,
            "probe9_original_deadline_after_exec"
        );
        Ok(())
    })();
    let kind = if unchanged.is_ok() {
        terminal_kind9(&process_value).unwrap_or("failed")
    } else {
        "failed"
    };
    let completion = CodeNativeCompilerProbeCompletion9 {
        schema: "oev2.native-compiler-probe-completion/9".into(), intent: reference(&root, &intent_path)?,
        process: reference(&root, &process_path)?, stdout: file(&process_directory.join("stdout"))?,
        stderr: file(&process_directory.join("stderr"))?,
        stdin_snapshot: intent.captured_stdin_file.as_ref().map(|_| file(&process_directory.join("stdin"))).transpose()?,
        execution: kind.into(), process_status: actual.status.clone(), actual_exit_code: actual.exit_code.map(i64::from),
        uncovered: vec!["Actual full streams and real compiler outcome only; Cargo success, configured input, native/loader closure and engineering feature applicability are separate".into(),
            unchanged.err().map(|e| format!("post_exec_recheck_failed:{e:#}")).unwrap_or_else(|| "original_source_tools_input_and_deadline_rechecked".into())],
    };
    completion
        .validate()
        .map_err(|e| anyhow::anyhow!("probe9_completion_contract:{e:?}"))?;
    write_new(
        &root.private_path(&directory.join(format!("probe9-completion-{pid}.json")))?,
        &serde_json::to_value(&completion)?,
    )?;
    use std::io::Write;
    std::io::stdout().write_all(&actual.stdout)?;
    std::io::stderr().write_all(&actual.stderr)?;
    if kind == "failed" {
        Ok(102)
    } else {
        actual
            .exit_code
            .context("probe9_actual_compiler_exit_missing")
    }
}
#[cfg(not(unix))]
pub fn compiler_probe_entry9() -> Result<i32> {
    anyhow::bail!("probe9_platform_unsupported")
}

#[derive(Debug, PartialEq)]
pub(crate) struct RetainedProbeInventory9 {
    pub policies: Vec<ArtifactReference>,
    pub intents: Vec<ArtifactReference>,
    pub completions: Vec<ArtifactReference>,
}

fn inventory9(directory: &Path) -> Result<BTreeMap<&'static str, BTreeMap<u64, PathBuf>>> {
    let mut inventory: BTreeMap<_, BTreeMap<_, _>> =
        ["raw", "intent", "completion", "process", "input"]
            .into_iter()
            .map(|v| (v, BTreeMap::new()))
            .collect();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .context("probe9_inventory_utf8")?;
        if !name.starts_with("probe9-") {
            continue;
        }
        let mut recognized = false;
        for (kind, suffix) in [
            ("raw", ".json"),
            ("intent", ".json"),
            ("completion", ".json"),
            ("process", ""),
            ("input", ".bin"),
        ] {
            let prefix = format!("probe9-{kind}-");
            let Some(serial) = name
                .strip_prefix(&prefix)
                .and_then(|v| v.strip_suffix(suffix))
            else {
                continue;
            };
            let pid = serial.parse::<u64>().context("probe9_inventory_pid")?;
            ensure!(
                pid > 0 && name == format!("{prefix}{pid}{suffix}"),
                "probe9_inventory_noncanonical_pid"
            );
            ensure!(
                inventory
                    .get_mut(kind)
                    .unwrap()
                    .insert(pid, path.clone())
                    .is_none(),
                "probe9_inventory_duplicate"
            );
            recognized = true;
            break;
        }
        ensure!(recognized, "probe9_unknown_inventory_entry");
    }
    let keys = |kind| inventory[kind].keys().copied().collect::<BTreeSet<_>>();
    ensure!(
        inventory["raw"].len() <= MAX_RECORDS
            && keys("raw") == keys("intent")
            && keys("raw") == keys("completion")
            && keys("raw") == keys("process")
            && keys("input").is_subset(&keys("raw")),
        "probe9_refusal_missing_or_dangling_inventory"
    );
    Ok(inventory)
}

fn verify_completion9(
    root: &SkillRoot,
    intent_ref: &ArtifactReference,
    intent: &CodeNativeCompilerProbeIntent9,
    completion: &CodeNativeCompilerProbeCompletion9,
    directory: &Path,
    original_output_bytes: u64,
) -> Result<()> {
    completion
        .validate()
        .map_err(|e| anyhow::anyhow!("probe9_retained_completion_contract:{e:?}"))?;
    let process_dir = directory.join(format!("probe9-process-{}", intent.pid));
    ensure!(
        completion.intent == *intent_ref
            && completion.process == reference(root, &process_dir.join("process.json"))?
            && completion.stdout == file(&process_dir.join("stdout"))?
            && completion.stderr == file(&process_dir.join("stderr"))?,
        "probe9_completion_files_or_intent_changed"
    );
    let process = read(root, &completion.process)?;
    ensure!(
        process["arguments"] == serde_json::to_value(&intent.arguments)?
            && process["cwd"] == intent.cwd
            && process["executable"]["path"] == intent.compiler.path
            && process["executable"]["resolved"] == intent.compiler.path
            && process["executable"]["identity"]["sha256"] == intent.compiler.sha256
            && process["executable"]["identity"]["bytes"] == intent.compiler.bytes
            && process["per_stream_limit"] == original_output_bytes
            && process["status"] == completion.process_status
            && process["exit_code"] == serde_json::to_value(completion.actual_exit_code)?
            && terminal_kind9(&process)? == completion.execution,
        "probe9_completion_not_exact_actual_process"
    );
    let expected_files = if intent.captured_stdin_file.is_some() {
        BTreeSet::from([
            "process.json".to_string(),
            "stdout".into(),
            "stderr".into(),
            "stdin".into(),
        ])
    } else {
        BTreeSet::from(["process.json".to_string(), "stdout".into(), "stderr".into()])
    };
    let actual_files = std::fs::read_dir(&process_dir)?
        .map(|entry| {
            let entry = entry?;
            Ok(entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("probe9_process_member_encoding"))?)
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        actual_files == expected_files,
        "probe9_process_terminal_inventory_changed"
    );
    for (key, output) in [
        ("stdout", &completion.stdout),
        ("stderr", &completion.stderr),
    ] {
        ensure!(
            process[key]["path"] == key
                && process[key]["sha256"] == output.sha256
                && process[key]["bytes"] == output.bytes,
            "probe9_complete_stream_changed"
        );
    }
    match (&intent.captured_stdin_file, &completion.stdin_snapshot) {
        (None, None) => ensure!(
            process.get("stdin").is_none()
                && process["schema"] == "ontology-engineering.external-process/v1",
            "probe9_file_stdin_changed"
        ),
        (Some(source), Some(snapshot)) => {
            crate::native_stdin_file9::verify_stdin_file9(source, original_output_bytes, 0o600)?;
            crate::native_stdin_file9::verify_stdin_file9(snapshot, original_output_bytes, 0o400)?;
            ensure!(
                source.path
                    == directory
                        .join(format!("probe9-input-{}.bin", intent.pid))
                        .to_str()
                        .context("probe9_input_path")?
                    && *snapshot == file(&process_dir.join("stdin"))?
                    && snapshot.mode == 0o400
                    && source.bytes == snapshot.bytes
                    && source.sha256 == snapshot.sha256
                    && process["schema"] == "ontology-engineering.external-process/stdin-file/1"
                    && process["stdin"]["schema"] == "oe-core.process-stdin-file/1"
                    && process["stdin"]["source"]["path"] == source.path
                    && process["stdin"]["source"]["identity"]["sha256"] == source.sha256
                    && process["stdin"]["source"]["identity"]["bytes"] == source.bytes
                    && process["stdin"]["snapshot"]["path"] == "stdin"
                    && process["stdin"]["transport"] == "explicit_private_regular_file_snapshot"
                    && process["stdin"]["original_byte_limit"] == original_output_bytes
                    && process["stdin"]["snapshot"]["identity"]
                        == process["stdin"]["source"]["identity"],
                "probe9_actual_fifo_regular_snapshot_changed"
            );
        }
        _ => anyhow::bail!("probe9_completion_stdin_missing_or_foreign"),
    }
    Ok(())
}

pub(crate) fn retained9(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<RetainedProbeInventory9> {
    let mut result = RetainedProbeInventory9 {
        policies: vec![],
        intents: vec![],
        completions: vec![],
    };
    let loader = crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?;
    for name in ["build-normalizer", "build-runner"] {
        let context_path = Path::new(&side.stage_root)
            .join(name)
            .join("stage-context.json");
        let context_ref = reference(root, &context_path)?;
        let context: CodeNativeStageContext6 = serde_json::from_value(read(root, &context_ref)?)?;
        let (policy_ref, _) = inspect_policy9(root, side, recipe, &context_path, &context)?;
        result.policies.push(policy_ref.clone());
        let directory = events(side, &context);
        let records = inventory9(&directory)?;
        let mut stdin_pids = BTreeSet::new();
        for (pid, path) in &records["raw"] {
            ensure!(
                !directory
                    .join(format!("pre-environment-{pid}.json"))
                    .try_exists()?,
                "probe9_pid_collides_with_original6_raw"
            );
            let raw_ref = reference(root, path)?;
            let raw: CodeNativeCompilerProbeRaw9 = serde_json::from_value(read(root, &raw_ref)?)?;
            let intent_ref = reference(root, &records["intent"][pid])?;
            let intent: CodeNativeCompilerProbeIntent9 =
                serde_json::from_value(read(root, &intent_ref)?)?;
            raw_intent9(&raw, &intent)?;
            ensure!(
                intent.pid == *pid
                    && intent.stage_context == context_ref
                    && intent.probe_policy == policy_ref
                    && intent.raw_observation == raw_ref
                    && intent.compiler == loader.compiler
                    && intent.wrapper == loader.rustc_wrapper,
                "probe9_retained_selected_stage_or_tools_changed"
            );
            let source = crate::native_sources6::caller_source(
                root,
                side,
                name,
                &serde_json::to_value(&context)?,
                Path::new(&intent.cwd),
            )?;
            let producer = crate::native_producer9::inspect(
                root,
                side,
                recipe,
                &context_path,
                &context,
                &loader,
                &source,
                intent.parent_pid,
                &intent.parent_executable,
                true,
            )?;
            ensure!(
                producer == intent.producer,
                "probe9_retained_producer_content_changed"
            );
            let target = root.private_path(Path::new(&side.build.target_dir))?;
            let scope = crate::native_compiler_probe9::ProbeInputScope9 {
                target_dir: &target,
                platform: &side.build.platform,
                stage: name,
                cargo_out_dir: Path::new(&intent.domain.cargo_out_dir),
                encoded_flags: &recipe.raw_encoded_rustflags,
                actual_encoded_flags: raw
                    .actual_encoded_rustflags
                    .as_deref()
                    .context("probe9_retained_flags_missing")?,
                cwd: Path::new(&intent.cwd),
            };
            ensure!(
                crate::native_compiler_probe9::check_input_domain9(
                    &source,
                    &intent.arguments,
                    &scope
                )? == intent.domain,
                "probe9_retained_original_input_changed"
            );
            if intent.captured_stdin_file.is_some() {
                stdin_pids.insert(*pid);
            }
            let completion_ref = reference(root, &records["completion"][pid])?;
            let completion: CodeNativeCompilerProbeCompletion9 =
                serde_json::from_value(read(root, &completion_ref)?)?;
            verify_completion9(
                root,
                &intent_ref,
                &intent,
                &completion,
                &directory,
                side.body_budget.output_bytes,
            )?;
            ensure!(
                reference(root, path)? == raw_ref
                    && reference(root, &records["intent"][pid])? == intent_ref
                    && reference(root, &records["completion"][pid])? == completion_ref,
                "probe9_retained_observations_changed"
            );
            result.intents.push(intent_ref);
            result.completions.push(completion_ref);
            ensure!(
                result.intents.len() <= MAX_RECORDS,
                "probe9_original_inventory_limit"
            );
        }
        ensure!(
            stdin_pids == records["input"].keys().copied().collect(),
            "probe9_dangling_or_missing_stdin_capture"
        );
        ensure!(
            inventory9(&directory)? == records && reference(root, &context_path)? == context_ref,
            "probe9_stage_inventory_changed_during_check"
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding(path: &str) -> CodeNativeFileBinding {
        CodeNativeFileBinding {
            path: path.into(),
            sha256: "a".repeat(64),
            bytes: 1,
            mode: 0o700,
        }
    }
    fn reference_fixture() -> ArtifactReference {
        ArtifactReference {
            path: "var/frozen.json".into(),
            sha256: "b".repeat(64),
        }
    }
    fn intent_fixture() -> CodeNativeCompilerProbeIntent9 {
        let context = reference_fixture();
        let parent = binding("/selected/build-script-build");
        CodeNativeCompilerProbeIntent9 {
            schema: "oev2.native-compiler-probe-intent/9".into(),
            pid: 10,
            parent_pid: 20,
            parent_executable: parent.clone(),
            compiler: binding("/selected/rustc"),
            wrapper: binding("/selected/oe-native-probe9-rustc-exec"),
            arguments: vec!["probe.rs".into()],
            cwd: "/selected/source".into(),
            stage_context: context.clone(),
            probe_policy: context.clone(),
            raw_observation: context.clone(),
            producer: CodeNativeBuildScriptProducer9 {
                schema: "oev2.native-build-script-producer/9".into(),
                parent_pid: 20,
                parent_executable: parent.clone(),
                linked_output: parent,
                build_script_source: binding("/selected/source/build.rs"),
                cargo_manifest: binding("/selected/source/Cargo.toml"),
                package_id: "selected@1".into(),
                compiler_intent: context.clone(),
                native_invocation: context.clone(),
                stage_context: context.clone(),
                linked_output_record: context,
                inspection: "same_stage_source_and_native_artifacts_only".into(),
            },
            domain: CodeNativeCompilerProbeDomain9 {
                schema: "oev2.native-compiler-probe-domain/9".into(),
                package_id: "selected@1".into(),
                cwd: "/selected/source".into(),
                arguments: vec!["probe.rs".into()],
                input_argument_ordinal: 0,
                input_kind: "original_file".into(),
                source_file: Some(binding("/selected/source/probe.rs")),
                cargo_out_dir: "/selected/out".into(),
                out_dir: "/selected/out/probe".into(),
                emit: "dep-info,metadata".into(),
                target: "aarch64-apple-darwin".into(),
                raw_encoded_rustflags: "-C\x1flinker=/selected/linker".into(),
            },
            stdin_observation: None,
            captured_stdin_file: None,
            loader_environment: vec![],
            loaded_images: vec![],
            execution: "before_compiler_spawn".into(),
            uncovered: vec![
                "Synthetic content fixture only; no live admission/native execution".into(),
            ],
        }
    }
    fn raw_fixture(intent: &CodeNativeCompilerProbeIntent9) -> CodeNativeCompilerProbeRaw9 {
        CodeNativeCompilerProbeRaw9 {
            schema: "oev2.native-compiler-probe-pre-admission-raw/9".into(),
            pid: intent.pid,
            parent_pid: intent.parent_pid,
            parent_executable: intent.parent_executable.clone(),
            compiler: intent.compiler.clone(),
            wrapper: intent.wrapper.clone(),
            arguments: intent.arguments.clone(),
            cwd: intent.cwd.clone(),
            stage_context: intent.stage_context.clone(),
            probe_policy: intent.probe_policy.clone(),
            actual_encoded_rustflags: Some(intent.domain.raw_encoded_rustflags.clone()),
            stdin_descriptor: None,
            selected_loader_environment_raw: ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"]
                .into_iter()
                .map(|name| json!({"name":name,"value_bytes":null}))
                .collect(),
            execution: "before_admission_no_exec".into(),
        }
    }
    #[test]
    fn raw9_intent9_rejects_changed_args_parent_flags_and_schema() {
        let intent = intent_fixture();
        let raw = raw_fixture(&intent);
        raw_intent9(&raw, &intent).unwrap();
        for (pointer, value) in [
            ("/arguments", json!(["other.rs"])),
            ("/parent_pid", json!(21)),
            ("/compiler/path", json!("/other/rustc")),
            ("/actual_encoded_rustflags", json!("")),
            ("/probe_policy/sha256", json!("c".repeat(64))),
            ("/schema", json!("oev2.native-compiler-pre-admission-raw/6")),
        ] {
            let mut modified = serde_json::to_value(&raw).unwrap();
            *modified.pointer_mut(pointer).unwrap() = value;
            let modified = serde_json::from_value::<CodeNativeCompilerProbeRaw9>(modified).unwrap();
            assert!(raw_intent9(&modified, &intent).is_err(), "{pointer}");
        }
    }
    #[test]
    fn probe9_stdin_content_rejects_missing_capture_and_foreign_permission() {
        let mut intent = intent_fixture();
        intent.domain.input_kind = "actual_stdin_required".into();
        intent.domain.source_file = None;
        assert!(input_content9(&intent).is_err());
        let bytes = b"";
        intent.stdin_observation = Some(CodeNativeCompilerProbeStdin9 {
            schema: "oev2.native-compiler-probe-stdin/9".into(),
            original_descriptor: CodeNativeCompilerProbeDescriptor9 {
                device: 0,
                inode: 1,
                mode: 0o010600,
                special_device: 0,
            },
            sha256: sha256(bytes),
            bytes: 0,
            original_byte_limit: 4194304,
            delivery: "captured_only_no_compiler_exec".into(),
        });
        intent.captured_stdin_file = Some(CodeNativeFileBinding {
            path: "/selected/stdin.bin".into(),
            sha256: sha256(bytes),
            bytes: 0,
            mode: 0o600,
        });
        input_content9(&intent).unwrap();
        intent.captured_stdin_file.as_mut().unwrap().bytes = 1;
        assert!(input_content9(&intent).is_err());
        let mut foreign = serde_json::to_value(intent_fixture()).unwrap();
        foreign["permission"] = json!(true);
        assert!(serde_json::from_value::<CodeNativeCompilerProbeIntent9>(foreign).is_err());
    }
    fn terminal() -> Value {
        json!({"status":"completed","exit_code":0,"capture_complete":true,"streams_collected":2,
        "stdout":{"truncated":false},"stderr":{"truncated":false},"executable":{"unchanged":true},"spawn_error_kind":null,
        "borrowed_deadline":{"schema":"oe-core.process-borrowed-deadline/1","expired":false}})
    }
    #[test]
    fn probe9_terminal_preserves_actual_nonzero_and_rejects_partial_or_missing_exit() {
        let mut value = terminal();
        assert_eq!(terminal_kind9(&value).unwrap(), "compiler_completed");
        value["status"] = json!("process_failed");
        value["exit_code"] = json!(1);
        assert_eq!(terminal_kind9(&value).unwrap(), "compiler_rejected");
        for (pointer, bad) in [
            ("/exit_code", Value::Null),
            ("/status", json!("timed_out")),
            ("/stdout/truncated", json!(true)),
            ("/capture_complete", json!(false)),
            ("/borrowed_deadline/expired", json!(true)),
            ("/executable/unchanged", json!(false)),
        ] {
            let mut changed = value.clone();
            *changed.pointer_mut(pointer).unwrap() = bad;
            assert!(terminal_kind9(&changed).is_err(), "{pointer}");
        }
    }
    #[test]
    fn probe9_inventory_requires_each_raw_intent_completion_without_duplicates() {
        let root = tempfile::tempdir().unwrap();
        inventory9(root.path()).unwrap();
        std::fs::write(root.path().join("probe9-raw-10.json"), b"raw").unwrap();
        assert!(inventory9(root.path()).is_err());
        std::fs::write(root.path().join("probe9-intent-10.json"), b"intent").unwrap();
        assert!(inventory9(root.path()).is_err());
        std::fs::write(root.path().join("probe9-completion-10.json"), b"completion").unwrap();
        assert!(inventory9(root.path()).is_err());
        std::fs::create_dir(root.path().join("probe9-process-10")).unwrap();
        inventory9(root.path()).unwrap();
        std::fs::write(root.path().join("probe9-raw-010.json"), b"duplicate").unwrap();
        assert!(inventory9(root.path()).is_err());
        std::fs::remove_file(root.path().join("probe9-raw-010.json")).unwrap();
        std::fs::write(root.path().join("probe9-completion-11.json"), b"dangling").unwrap();
        assert!(inventory9(root.path()).is_err());
    }
}
