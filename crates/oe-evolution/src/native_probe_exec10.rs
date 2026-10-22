//! Explicit execution10 uses the original owner and complete SDK8 artifacts.
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

pub const PROFILE: &str = "semantica.rust-full-native-build-script-probes/10";
const POLICY_FILE: &str = "compiler-probe-policy9.json";
const MAX_RECORDS: usize = 16384;

fn selected_policy10(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
) -> Result<CodeNativeCompilerProbePolicy10> {
    let profile = crate::native_profile6::inspect_profile(root, &side.source_profile)?;
    selected_policy10_with_profile(root, side, recipe, context_path, context, &profile)
}
fn selected_policy10_with_profile(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
    profile: &CodeNativeRuntimeProfile6,
) -> Result<CodeNativeCompilerProbePolicy10> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?,
        "probe10_complete_sdk8_adapter_required"
    );
    let loader = crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?;
    ensure!(
        loader.metadata_probe == crate::native_buildscript6::POLICY
            && Path::new(&loader.rustc_wrapper.path)
                .file_name()
                .is_some_and(|v| v == "oe-native-probe10-rustc-exec"),
        "probe10_original_metadata_policy_or_explicit_wrapper_not_selected"
    );
    ensure!(
        context.schema == "oev2.native-build-stage/6"
            && context.source_root == root.path().to_str().context("probe10_root_utf8")?
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
        "probe10_current_original_stage_changed"
    );
    let value = CodeNativeCompilerProbePolicy10 {
        schema: "oev2.native-compiler-probe-policy/10".into(),
        execution_profile: PROFILE.into(),
        source_profile: side.source_profile.clone(),
        stage_context: reference(root, context_path)?,
        original_loader_policy: recipe.cargo_loader_policy.clone(),
        source_manifest_sha256: digest(&context.source_manifest)?,
        deadline_epoch_ms: context.deadline_epoch_ms,
        output_bytes: context.output_bytes,
        domain: "original_file_or_actual_fifo_metadata_only/10".into(),
    };
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("probe10_policy_contract:{e:?}"))?;
    Ok(value)
}

pub(crate) fn write_policy10(
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
    let policy = selected_policy10(
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

pub(crate) fn inspect_policy10(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
) -> Result<(ArtifactReference, CodeNativeCompilerProbePolicy10)> {
    let path = context_path.with_file_name(POLICY_FILE);
    let record = reference(root, &path)?;
    let policy: CodeNativeCompilerProbePolicy10 = serde_json::from_value(read(root, &record)?)?;
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("probe10_selected_policy_contract:{e:?}"))?;
    ensure!(
        policy == selected_policy10(root, side, recipe, context_path, context)?
            && reference(root, &path)? == record,
        "probe10_selected_policy_changed"
    );
    Ok((record, policy))
}

/// Reuse only the already fully inspected stage owned by this call. Other
/// policy write/retention paths still use the full original profile inspector.
fn inspect_policy10_checked(
    root: &SkillRoot,
    checked: &crate::native_loader6::CheckedCompilerStage6,
    context_path: &Path,
) -> Result<(ArtifactReference, CodeNativeCompilerProbePolicy10)> {
    checked.recheck(root)?;
    let (context, side, recipe, profile) = checked.facts();
    let path = context_path.with_file_name(POLICY_FILE);
    let record = reference(root, &path)?;
    let policy: CodeNativeCompilerProbePolicy10 = serde_json::from_value(read(root, &record)?)?;
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("probe10_selected_policy_contract:{e:?}"))?;
    ensure!(
        policy
            == selected_policy10_with_profile(root, side, recipe, context_path, context, profile)?
            && reference(root, &path)? == record,
        "probe10_selected_policy_changed"
    );
    Ok((record, policy))
}

fn events(side: &CodeNativeSideContext6, context: &CodeNativeStageContext6) -> PathBuf {
    Path::new(&side.stage_root)
        .join(&context.parent_stage)
        .join("loader-events")
}
fn input_content10(intent: &CodeNativeCompilerProbeIntent10) -> Result<()> {
    intent
        .validate()
        .map_err(|e| anyhow::anyhow!("probe10_intent_contract:{e:?}"))?;
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
        "probe10_current_intent_producer_domain_changed"
    );
    match (
        intent.domain.configuration_kind.as_str(),
        &intent.host_environment,
    ) {
        ("selected_target", None) => ensure!(
            !intent.domain.raw_encoded_rustflags.is_empty()
                && intent.domain.raw_encoded_rustflags == intent.domain.selected_target_flags,
            "probe10_target_condition_changed"
        ),
        ("actual_host_empty", Some(host)) => {
            host.validate()
                .map_err(|e| anyhow::anyhow!("probe10_host_contract:{e:?}"))?;
            ensure!(
                intent.domain.raw_encoded_rustflags.is_empty()
                    && host.raw_encoded_rustflags.is_empty()
                    && !intent.domain.selected_target_flags.is_empty()
                    && host.producer == intent.producer
                    && host.out_dir == intent.domain.cargo_out_dir
                    && host.host == intent.domain.target
                    && host.target == intent.domain.target,
                "probe10_host_content_changed"
            );
        }
        _ => anyhow::bail!("probe10_host_condition_missing_or_foreign"),
    }
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
                "probe10_actual_stdin_content_changed"
            );
            Ok(())
        }
        _ => anyhow::bail!("probe10_input_observation_missing_or_foreign"),
    }
}

fn raw_intent10(
    raw: &CodeNativeCompilerProbeRaw10,
    intent: &CodeNativeCompilerProbeIntent10,
) -> Result<()> {
    raw.validate()
        .map_err(|e| anyhow::anyhow!("probe10_raw_contract:{e:?}"))?;
    input_content10(intent)?;
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
        "probe10_raw_intent_binding_changed"
    );
    if let Some(host) = &intent.host_environment {
        ensure!(
            raw.actual_host.as_deref() == Some(host.host.as_str())
                && raw.actual_target.as_deref() == Some(host.target.as_str())
                && raw.actual_out_dir.as_deref() == Some(host.out_dir.as_str())
                && raw.actual_cargo_linker.as_deref()
                    == Some(host.cargo_linker_environment.value.as_str()),
            "probe10_host_raw_content_changed"
        );
    }
    if let Some(stdin) = &intent.stdin_observation {
        ensure!(
            raw.stdin_descriptor.as_ref() == Some(&stdin.original_descriptor),
            "probe10_original_fifo_identity_changed"
        );
    }
    ensure!(
        raw.selected_loader_environment_raw.len() == 2,
        "probe10_raw_loader_occurrences"
    );
    let raw_values = raw
        .selected_loader_environment_raw
        .iter()
        .map(|entry| {
            Ok((
                entry["name"].as_str().context("probe10_raw_loader_name")?,
                entry["value_bytes"].clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    ensure!(
        raw_values.len() == 2
            && raw_values.contains_key("DYLD_FALLBACK_LIBRARY_PATH")
            && raw_values.contains_key("DYLD_PRINT_LIBRARIES"),
        "probe10_raw_loader_inventory"
    );
    for (name, value) in raw_values {
        let selected = intent
            .loader_environment
            .iter()
            .filter(|entry| entry.name == name)
            .collect::<Vec<_>>();
        ensure!(selected.len() <= 1, "probe10_duplicate_loader_value");
        let expected = selected
            .first()
            .map_or(Value::Null, |entry| json!(entry.value.as_bytes()));
        ensure!(value == expected, "probe10_original_loader_value_changed");
    }
    Ok(())
}

fn terminal_kind10(process: &Value) -> Result<&'static str> {
    ensure!(
        process["capture_complete"] == true
            && process["streams_collected"] == 2
            && process["stdout"]["truncated"] == false
            && process["stderr"]["truncated"] == false
            && process["executable"]["unchanged"] == true
            && process["spawn_error_kind"].is_null()
            && process["borrowed_deadline"]["schema"] == "oe-core.process-borrowed-deadline/1"
            && process["borrowed_deadline"]["expired"] == false,
        "probe10_terminal_partial_changed_or_expired"
    );
    if process.get("stdin").is_some() {
        ensure!(
            process["stdin"]["configured_for_spawn"] == true
                && process["stdin"]["process_spawned"] == true
                && process["stdin"]["unchanged"] == true
                && process["stdin"]["target_consumption"] == "not_inferred"
                && process["stdin"]["semantic_support"] == "not_inferred",
            "probe10_terminal_stdin_changed"
        );
    }
    match (process["status"].as_str(), process["exit_code"].as_i64()) {
        (Some("completed"), Some(0)) => Ok("compiler_completed"),
        (Some("process_failed"), Some(code)) if code > 0 => Ok("compiler_rejected"),
        _ => anyhow::bail!("probe10_terminal_not_real_compiler_completion"),
    }
}
fn file(path: &Path) -> Result<CodeNativeFileBinding> {
    crate::native_runtime::file_binding(path)
}

#[cfg(unix)]
pub fn compiler_probe_entry10() -> Result<i32> {
    use std::os::{fd::AsFd, unix::ffi::OsStrExt};
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        argv.len() >= 2
            && argv.len() <= 10000
            && argv.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "probe10_compiler_arguments"
    );
    let context_path = PathBuf::from(std::env::var("OE_NATIVE_STAGE_CONTEXT")?);
    let raw_context =
        oe_wire::json::strict_json(&read_regular_bytes(&context_path, 16 * 1024 * 1024)?)?;
    let root = SkillRoot::new(
        raw_context["source_root"]
            .as_str()
            .context("probe10_root")?,
    )?;
    let checked = crate::native_loader6::checked_stage6(&root, &context_path)?;
    let (context, side, recipe, _) = checked.facts();
    let (policy_ref, policy) = inspect_policy10_checked(&root, &checked, &context_path)?;
    let loader = crate::native_loader6::inspect_policy(&root, &recipe.cargo_loader_policy)?;
    ensure!(
        argv[0] == loader.compiler.path
            && argv[0] == side.build.rustc_path
            && std::env::current_exe()?.canonicalize()? == Path::new(&loader.rustc_wrapper.path),
        "probe10_compiler_or_wrapper_changed"
    );
    let own = &crate::driver_root().context("probe10_own_root")?;
    ensure!(
        digest(&public_manifest(own)?)? == compiled()?["manifest_sha256"],
        "probe10_wrapper_source_mismatch"
    );
    let parent_pid = crate::native_loader6::parent_pid()?;
    let parent = crate::native_loader6::process_executable(parent_pid)?;
    // The original6 entry handles all actual Cargo compiler commands and the
    // unchanged four read-only build-script metadata vectors at this same PID.
    if parent == loader.cargo || crate::native_buildscript6::metadata(&argv[1..]).is_ok() {
        crate::native_loader6::compiler_exec_entry_from_checked_stage6(checked, argv)?;
        anyhow::bail!("probe10_original_exec_unexpected_return");
    }
    let cwd = std::env::current_dir()?;
    let directory = events(&side, &context);
    let pid = std::process::id() as u64;
    let stdin = std::io::stdin();
    let raw = CodeNativeCompilerProbeRaw10 {
        schema: "oev2.native-compiler-probe-pre-admission-raw/10".into(), pid, parent_pid,
        parent_executable: parent.clone(), compiler: loader.compiler.clone(), wrapper: loader.rustc_wrapper.clone(),
        arguments: argv[1..].to_vec(), cwd: cwd.to_str().context("probe10_cwd")?.into(),
        stage_context: reference(&root, &context_path)?, probe_policy: policy_ref.clone(),
        actual_encoded_rustflags: std::env::var("CARGO_ENCODED_RUSTFLAGS").ok(),
        actual_host: std::env::var("HOST").ok(), actual_target: std::env::var("TARGET").ok(),
        actual_out_dir: std::env::var("OUT_DIR").ok(),
        actual_cargo_linker: std::env::var("CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER").ok(),
        stdin_descriptor: crate::native_compiler_probe9::descriptor(stdin.as_fd()).ok(),
        selected_loader_environment_raw: ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"].into_iter()
            .map(|name| json!({"name": name,"value_bytes": std::env::var_os(name).map(|value| value.as_bytes().to_vec())})).collect(),
        execution: "before_admission_no_exec".into(),
    };
    let raw_path = root.private_path(&directory.join(format!("probe10-raw-{pid}.json")))?;
    write_new(&raw_path, &serde_json::to_value(&raw)?)?;
    let actual_flags = raw
        .actual_encoded_rustflags
        .as_deref()
        .context("probe10_original_actual_flags_missing")?;
    ensure!(
        actual_flags == recipe.raw_encoded_rustflags || actual_flags.is_empty(),
        "probe10_original_actual_flags_changed"
    );
    let remaining = policy
        .deadline_epoch_ms
        .checked_sub(epoch_ms()?)
        .filter(|v| *v > 0)
        .context("probe10_original_stage_deadline")?;
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(remaining))
        .context("probe10_deadline_overflow")?;
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
    let host_environment = if actual_flags.is_empty() {
        Some(crate::native_host_compiler10::observe_live(
            &root, side, recipe, context, &source, &producer,
        )?)
    } else {
        None
    };
    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    let target = root.private_path(Path::new(&side.build.target_dir))?;
    let scope = crate::native_compiler_probe10::ProbeInputScope10 {
        target_dir: &target,
        platform: &side.build.platform,
        stage: &context.parent_stage,
        cargo_out_dir: &out,
        encoded_flags: &recipe.raw_encoded_rustflags,
        actual_encoded_flags: raw.actual_encoded_rustflags.as_deref().unwrap(),
        cwd: &cwd,
    };
    let domain = crate::native_compiler_probe10::check_input_domain10(&source, &argv[1..], &scope)?;
    let (stdin_observation, captured_stdin_file) = if domain.input_kind == "actual_stdin_required" {
        let captured = crate::native_compiler_probe9::capture_stdin9(
            stdin.as_fd(),
            deadline,
            policy.output_bytes as usize,
        )?;
        let path = root.private_path(&directory.join(format!("probe10-input-{pid}.bin")))?;
        write_new_bytes(&path, &captured.bytes)?;
        (Some(captured.observation), Some(file(&path)?))
    } else {
        crate::native_loader6::inspect_null_descriptor6(0)?;
        (None, None)
    };
    let intent = CodeNativeCompilerProbeIntent10 {
        schema: "oev2.native-compiler-probe-intent/10".into(), pid, parent_pid, parent_executable: parent.clone(),
        compiler: loader.compiler.clone(), wrapper: loader.rustc_wrapper.clone(), arguments: argv[1..].to_vec(),
        cwd: raw.cwd.clone(), stage_context: raw.stage_context.clone(), probe_policy: policy_ref.clone(), raw_observation: reference(&root, &raw_path)?, producer, domain, host_environment,
        stdin_observation, captured_stdin_file,
        loader_environment: crate::native_loader6::selected_environment(&side, &loader, true, None)?,
        loaded_images: crate::native_loader6::current_images()?, execution: "before_compiler_spawn".into(),
        uncovered: vec!["Original parent ProcessTree monotonic deadline is authoritative; this child checks one inherited absolute deadline and shares its local Instant across capture and replay, never a renewed budget".into(),
            "Wrapper image observations do not prove every child compiler loader/OS-cache image or actual symbol use; source/transient/hard-I/O and engineering closure remain separate".into()],
    };
    raw_intent10(&raw, &intent)?;
    let intent_path = directory.join(format!("probe10-intent-{pid}.json"));
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
        "probe10_actual_parent_or_original_deadline_changed_before_spawn"
    );
    ensure!(
        inspect_policy10_checked(&root, &checked, &context_path)? == (policy_ref, policy),
        "probe10_policy_changed_before_spawn"
    );
    let process_directory = root.private_path(&directory.join(format!("probe10-process-{pid}")))?;
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
        crate::native_compiler_probe10::check_input_domain10(&source, &intent.arguments, &scope)?;
        if let Some(host) = &intent.host_environment {
            ensure!(
                crate::native_host_compiler10::observe_live(
                    &root,
                    side,
                    recipe,
                    context,
                    &source,
                    &intent.producer
                )? == *host,
                "probe10_host_conditions_changed_after_exec"
            );
        }
        if let Some(input) = &intent.captured_stdin_file {
            crate::native_stdin_file9::verify_stdin_file9(input, context.output_bytes, 0o600)?;
        }
        ensure!(
            epoch_ms()? < context.deadline_epoch_ms && Instant::now() < deadline,
            "probe10_original_deadline_after_exec"
        );
        Ok(())
    })();
    let kind = if unchanged.is_ok() {
        terminal_kind10(&process_value).unwrap_or("failed")
    } else {
        "failed"
    };
    let completion = CodeNativeCompilerProbeCompletion10 {
        schema: "oev2.native-compiler-probe-completion/10".into(), intent: reference(&root, &intent_path)?,
        process: reference(&root, &process_path)?, stdout: file(&process_directory.join("stdout"))?,
        stderr: file(&process_directory.join("stderr"))?,
        stdin_snapshot: intent.captured_stdin_file.as_ref().map(|_| file(&process_directory.join("stdin"))).transpose()?,
        execution: kind.into(), process_status: actual.status.clone(), actual_exit_code: actual.exit_code.map(i64::from),
        uncovered: vec!["Actual full streams and real compiler outcome only; Cargo success, configured input, native/loader closure and engineering feature applicability are separate".into(),
            unchanged.err().map(|e| format!("post_exec_recheck_failed:{e:#}")).unwrap_or_else(|| "original_source_tools_input_and_deadline_rechecked".into())],
    };
    completion
        .validate()
        .map_err(|e| anyhow::anyhow!("probe10_completion_contract:{e:?}"))?;
    write_new(
        &root.private_path(&directory.join(format!("probe10-completion-{pid}.json")))?,
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
            .context("probe10_actual_compiler_exit_missing")
    }
}
#[cfg(not(unix))]
pub fn compiler_probe_entry10() -> Result<i32> {
    anyhow::bail!("probe10_platform_unsupported")
}

#[derive(Debug, PartialEq)]
pub(crate) struct RetainedProbeInventory10 {
    pub policies: Vec<ArtifactReference>,
    pub intents: Vec<ArtifactReference>,
    pub completions: Vec<ArtifactReference>,
}

fn inventory10(directory: &Path) -> Result<BTreeMap<&'static str, BTreeMap<u64, PathBuf>>> {
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
            .context("probe10_inventory_utf8")?;
        ensure!(
            !name.starts_with("probe9-"),
            "probe10_foreign_execution9_record_in_current_stage"
        );
        if !name.starts_with("probe10-") {
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
            let prefix = format!("probe10-{kind}-");
            let Some(serial) = name
                .strip_prefix(&prefix)
                .and_then(|v| v.strip_suffix(suffix))
            else {
                continue;
            };
            let pid = serial.parse::<u64>().context("probe10_inventory_pid")?;
            ensure!(
                pid > 0 && name == format!("{prefix}{pid}{suffix}"),
                "probe10_inventory_noncanonical_pid"
            );
            ensure!(
                inventory
                    .get_mut(kind)
                    .unwrap()
                    .insert(pid, path.clone())
                    .is_none(),
                "probe10_inventory_duplicate"
            );
            recognized = true;
            break;
        }
        ensure!(recognized, "probe10_unknown_inventory_entry");
    }
    let keys = |kind| inventory[kind].keys().copied().collect::<BTreeSet<_>>();
    ensure!(
        inventory["raw"].len() <= MAX_RECORDS
            && keys("raw") == keys("intent")
            && keys("raw") == keys("completion")
            && keys("raw") == keys("process")
            && keys("input").is_subset(&keys("raw")),
        "probe10_refusal_missing_or_dangling_inventory"
    );
    Ok(inventory)
}

fn verify_completion10(
    root: &SkillRoot,
    intent_ref: &ArtifactReference,
    intent: &CodeNativeCompilerProbeIntent10,
    completion: &CodeNativeCompilerProbeCompletion10,
    directory: &Path,
    original_output_bytes: u64,
) -> Result<()> {
    completion
        .validate()
        .map_err(|e| anyhow::anyhow!("probe10_retained_completion_contract:{e:?}"))?;
    let process_dir = directory.join(format!("probe10-process-{}", intent.pid));
    ensure!(
        completion.intent == *intent_ref
            && completion.process == reference(root, &process_dir.join("process.json"))?
            && completion.stdout == file(&process_dir.join("stdout"))?
            && completion.stderr == file(&process_dir.join("stderr"))?,
        "probe10_completion_files_or_intent_changed"
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
            && terminal_kind10(&process)? == completion.execution,
        "probe10_completion_not_exact_actual_process"
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
                .map_err(|_| anyhow::anyhow!("probe10_process_member_encoding"))?)
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        actual_files == expected_files,
        "probe10_process_terminal_inventory_changed"
    );
    for (key, output) in [
        ("stdout", &completion.stdout),
        ("stderr", &completion.stderr),
    ] {
        ensure!(
            process[key]["path"] == key
                && process[key]["sha256"] == output.sha256
                && process[key]["bytes"] == output.bytes,
            "probe10_complete_stream_changed"
        );
    }
    match (&intent.captured_stdin_file, &completion.stdin_snapshot) {
        (None, None) => ensure!(
            process.get("stdin").is_none()
                && process["schema"] == "ontology-engineering.external-process/v1",
            "probe10_file_stdin_changed"
        ),
        (Some(source), Some(snapshot)) => {
            crate::native_stdin_file9::verify_stdin_file9(source, original_output_bytes, 0o600)?;
            crate::native_stdin_file9::verify_stdin_file9(snapshot, original_output_bytes, 0o400)?;
            ensure!(
                source.path
                    == directory
                        .join(format!("probe10-input-{}.bin", intent.pid))
                        .to_str()
                        .context("probe10_input_path")?
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
                "probe10_actual_fifo_regular_snapshot_changed"
            );
        }
        _ => anyhow::bail!("probe10_completion_stdin_missing_or_foreign"),
    }
    Ok(())
}

pub(crate) fn retained10(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<RetainedProbeInventory10> {
    let mut result = RetainedProbeInventory10 {
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
        let (policy_ref, _) = inspect_policy10(root, side, recipe, &context_path, &context)?;
        result.policies.push(policy_ref.clone());
        let directory = events(side, &context);
        let records = inventory10(&directory)?;
        let mut stdin_pids = BTreeSet::new();
        for (pid, path) in &records["raw"] {
            ensure!(
                !directory
                    .join(format!("pre-environment-{pid}.json"))
                    .try_exists()?,
                "probe10_pid_collides_with_original6_raw"
            );
            let raw_ref = reference(root, path)?;
            let raw: CodeNativeCompilerProbeRaw10 = serde_json::from_value(read(root, &raw_ref)?)?;
            let intent_ref = reference(root, &records["intent"][pid])?;
            let intent: CodeNativeCompilerProbeIntent10 =
                serde_json::from_value(read(root, &intent_ref)?)?;
            raw_intent10(&raw, &intent)?;
            ensure!(
                intent.pid == *pid
                    && intent.stage_context == context_ref
                    && intent.probe_policy == policy_ref
                    && intent.raw_observation == raw_ref
                    && intent.compiler == loader.compiler
                    && intent.wrapper == loader.rustc_wrapper,
                "probe10_retained_selected_stage_or_tools_changed"
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
                "probe10_retained_producer_content_changed"
            );
            if let Some(host) = &intent.host_environment {
                crate::native_host_compiler10::verify_record(
                    root, side, recipe, &context, &source, &producer, host,
                )?;
            }
            let target = root.private_path(Path::new(&side.build.target_dir))?;
            let scope = crate::native_compiler_probe10::ProbeInputScope10 {
                target_dir: &target,
                platform: &side.build.platform,
                stage: name,
                cargo_out_dir: Path::new(&intent.domain.cargo_out_dir),
                encoded_flags: &recipe.raw_encoded_rustflags,
                actual_encoded_flags: raw
                    .actual_encoded_rustflags
                    .as_deref()
                    .context("probe10_retained_flags_missing")?,
                cwd: Path::new(&intent.cwd),
            };
            ensure!(
                crate::native_compiler_probe10::check_input_domain10(
                    &source,
                    &intent.arguments,
                    &scope
                )? == intent.domain,
                "probe10_retained_original_input_changed"
            );
            if intent.captured_stdin_file.is_some() {
                stdin_pids.insert(*pid);
            }
            let completion_ref = reference(root, &records["completion"][pid])?;
            let completion: CodeNativeCompilerProbeCompletion10 =
                serde_json::from_value(read(root, &completion_ref)?)?;
            verify_completion10(
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
                "probe10_retained_observations_changed"
            );
            result.intents.push(intent_ref);
            result.completions.push(completion_ref);
            ensure!(
                result.intents.len() <= MAX_RECORDS,
                "probe10_original_inventory_limit"
            );
        }
        ensure!(
            stdin_pids == records["input"].keys().copied().collect(),
            "probe10_dangling_or_missing_stdin_capture"
        );
        ensure!(
            inventory10(&directory)? == records && reference(root, &context_path)? == context_ref,
            "probe10_stage_inventory_changed_during_check"
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures() -> (
        CodeNativeCompilerProbeRaw10,
        CodeNativeCompilerProbeIntent10,
    ) {
        let (host, _) = crate::native_host_compiler10::tests::fixture();
        let parent = host.producer.parent_executable.clone();
        let context = host.producer.stage_context.clone();
        let arguments = vec!["probe.rs".into()];
        let domain = CodeNativeCompilerProbeDomain10 {
            schema: "oev2.native-compiler-probe-domain/10".into(),
            package_id: host.producer.package_id.clone(),
            cwd: "/selected/source".into(),
            arguments: arguments.clone(),
            input_argument_ordinal: 0,
            input_kind: "original_file".into(),
            source_file: Some(parent.clone()),
            cargo_out_dir: host.out_dir.clone(),
            out_dir: format!("{}/probe", host.out_dir),
            emit: "dep-info,metadata".into(),
            target: host.target.clone(),
            raw_encoded_rustflags: String::new(),
            selected_target_flags: "-C\x1flinker=/selected/producer".into(),
            configuration_kind: "actual_host_empty".into(),
        };
        let intent = CodeNativeCompilerProbeIntent10 {
            schema: "oev2.native-compiler-probe-intent/10".into(),
            pid: 10,
            parent_pid: host.producer.parent_pid,
            parent_executable: parent.clone(),
            compiler: parent.clone(),
            wrapper: parent.clone(),
            arguments: arguments.clone(),
            cwd: domain.cwd.clone(),
            stage_context: context.clone(),
            probe_policy: context.clone(),
            raw_observation: context.clone(),
            producer: host.producer.clone(),
            domain,
            stdin_observation: None,
            captured_stdin_file: None,
            loader_environment: vec![],
            loaded_images: vec![],
            execution: "before_compiler_spawn".into(),
            uncovered: vec!["content test only".into()],
            host_environment: Some(host.clone()),
        };
        let raw = CodeNativeCompilerProbeRaw10 {
            schema: "oev2.native-compiler-probe-pre-admission-raw/10".into(),
            pid: 10,
            parent_pid: intent.parent_pid,
            parent_executable: parent.clone(),
            compiler: parent.clone(),
            wrapper: parent,
            arguments,
            cwd: intent.cwd.clone(),
            stage_context: context.clone(),
            probe_policy: context,
            actual_encoded_rustflags: Some(String::new()),
            stdin_descriptor: None,
            selected_loader_environment_raw: vec![
                json!({"name":"DYLD_FALLBACK_LIBRARY_PATH","value_bytes":null}),
                json!({"name":"DYLD_PRINT_LIBRARIES","value_bytes":null}),
            ],
            execution: "before_admission_no_exec".into(),
            actual_host: Some(host.host),
            actual_target: Some(host.target),
            actual_out_dir: Some(host.out_dir),
            actual_cargo_linker: Some(host.cargo_linker_environment.value),
        };
        (raw, intent)
    }
    #[test]
    fn host10_raw_intent_keeps_missing_empty_and_actual_environment_distinct() {
        let (raw, intent) = fixtures();
        raw_intent10(&raw, &intent).unwrap();
        for (path, changed) in [
            ("/actual_encoded_rustflags", Value::Null),
            ("/actual_host", Value::Null),
            ("/actual_target", json!("x86_64-apple-darwin")),
            ("/actual_out_dir", json!("/foreign/out")),
            ("/actual_cargo_linker", json!("/foreign/linker")),
            ("/parent_pid", json!(99)),
            ("/arguments", json!(["foreign.rs"])),
            (
                "/schema",
                json!("oev2.native-compiler-probe-pre-admission-raw/9"),
            ),
        ] {
            let mut changed_raw = serde_json::to_value(&raw).unwrap();
            *changed_raw.pointer_mut(path).unwrap() = changed;
            let changed_raw = serde_json::from_value(changed_raw).unwrap();
            assert!(raw_intent10(&changed_raw, &intent).is_err(), "{path}");
        }
    }
    #[test]
    fn host10_intent_rejects_missing_host_foreign_producer_and_target_substitution() {
        let (_, original) = fixtures();
        input_content10(&original).unwrap();
        let mut changed = original.clone();
        changed.host_environment = None;
        assert!(input_content10(&changed).is_err());
        changed = original.clone();
        changed
            .host_environment
            .as_mut()
            .unwrap()
            .producer
            .native_invocation
            .sha256 = "d".repeat(64);
        assert!(input_content10(&changed).is_err());
        changed = original.clone();
        changed.domain.configuration_kind = "selected_target".into();
        assert!(input_content10(&changed).is_err());
        changed = original.clone();
        changed.domain.raw_encoded_rustflags = changed.domain.selected_target_flags.clone();
        assert!(input_content10(&changed).is_err());
        let mut missing = serde_json::to_value(&original.domain).unwrap();
        missing
            .as_object_mut()
            .unwrap()
            .remove("raw_encoded_rustflags");
        assert!(serde_json::from_value::<CodeNativeCompilerProbeDomain10>(missing).is_err());
        let mut foreign = serde_json::to_value(original).unwrap();
        foreign["permission"] = json!(true);
        assert!(serde_json::from_value::<CodeNativeCompilerProbeIntent10>(foreign).is_err());
    }
    #[test]
    fn probe10_inventory_retains_refusal_missing_terminal_and_foreign_protocol() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        inventory10(temp.path()).unwrap();
        std::fs::write(temp.path().join("probe10-raw-10.json"), "{}").unwrap();
        assert!(inventory10(temp.path()).is_err());
        for name in ["probe10-intent-10.json", "probe10-completion-10.json"] {
            std::fs::write(temp.path().join(name), "{}").unwrap();
        }
        std::fs::create_dir(temp.path().join("probe10-process-10")).unwrap();
        inventory10(temp.path()).unwrap();
        std::fs::write(temp.path().join("probe10-completion-11.json"), "{}").unwrap();
        assert!(inventory10(temp.path()).is_err());
        std::fs::remove_file(temp.path().join("probe10-completion-11.json")).unwrap();
        std::fs::write(temp.path().join("probe9-raw-10.json"), "{}").unwrap();
        assert!(inventory10(temp.path()).is_err());
    }
}
