//! Full native /6 adapter: ordered typed archive and response preparation.
use crate::native_runtime::{deadline_timeout, epoch_ms, read, reference};
use crate::native_runtime6::{role, verify_pins};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{SkillRoot, read_regular_bytes, write_new};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const PREFIX: &str = "oe-native-debug";
pub(crate) fn environment() -> Vec<CodeNativeEnvironmentEntry> {
    vec![
        CodeNativeEnvironmentEntry {
            name: "LC_ALL".into(),
            value: "C".into(),
        },
        CodeNativeEnvironmentEntry {
            name: "PATH".into(),
            value: "/usr/bin:/bin".into(),
        },
    ]
}
fn envmap() -> BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    BTreeMap::from([
        ("LC_ALL".into(), "C".into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ])
}
pub(crate) fn validate_context(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    parent_stage: &str,
) -> Result<Value> {
    ensure!(
        ["build-normalizer", "build-runner"].contains(&parent_stage),
        "native6_parent_stage"
    );
    let stage = Path::new(&side.stage_root).join(parent_stage);
    let context = read(root, &reference(root, &stage.join("stage-context.json"))?)?;
    let keys = context
        .as_object()
        .context("native6_stage_object")?
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let is_stage4 = context["schema"] == "oev2.native-build-stage/6";
    if is_stage4 {
        let typed: CodeNativeStageContext6 = serde_json::from_value(context.clone())?;
        typed
            .validate()
            .map_err(|e| anyhow::anyhow!("native6_stage_contract:{e:?}"))?;
        crate::native_sources6::inspect(
            root,
            side,
            parent_stage,
            typed.deadline_epoch_ms,
            &typed.compilation_sources,
        )?;
    }
    let mut expected_keys = BTreeSet::from([
        "schema",
        "source_root",
        "source_manifest",
        "side_context",
        "recipe",
        "parent_stage",
        "deadline_epoch_ms",
        "output_bytes",
        "invocation_root",
        "temporary_root",
    ]);
    if is_stage4 {
        expected_keys.insert("compilation_sources");
    }
    ensure!(keys == expected_keys, "native6_stage_unknown_field");
    let profile = crate::native_profile6::inspect_profile(root, &side.source_profile)?;
    ensure!(
        (context["schema"] == "oev2.native-build-stage/6" || is_stage4)
            && context["source_root"] == root.path().to_str().context("native6_path")?
            && context["source_manifest"] == public_manifest(root.path())?
            && context["side_context"] == serde_json::to_value(side_ref)?
            && context["recipe"] == serde_json::to_value(profile.recipe)?
            && context["parent_stage"] == parent_stage
            && context["invocation_root"] == stage.to_str().context("native6_path")?
            && context["temporary_root"]
                == Path::new(&side.build.target_dir)
                    .join("native-tmp")
                    .join(parent_stage)
                    .to_str()
                    .context("native6_path")?
            && context["output_bytes"] == side.body_budget.output_bytes
            && context["deadline_epoch_ms"].as_u64().is_some_and(|x| x > 0)
            && recipe.raw_encoded_rustflags_sha256
                == sha256(recipe.raw_encoded_rustflags.as_bytes()),
        "native6_stage_binding_changed"
    );
    Ok(context)
}
/// Prefix only the exact input occurrence coordinates, including -Wl components.
/// The typed facts remain relative to the inner stage, clang runs in the outer.
pub(crate) fn execution_arguments(f: &CodeNativeInputPreparationFacts) -> Result<Vec<String>> {
    let mut args = f.executed_arguments.clone();
    let mut coordinates = BTreeSet::new();
    for (ordinal, component, logical) in f
        .direct_objects
        .iter()
        .map(|m| {
            (
                m.input_argument_ordinal,
                m.argument_component_ordinal,
                m.logical_path.as_str(),
            )
        })
        .chain(f.archives.iter().map(|m| {
            (
                m.input_argument_ordinal,
                m.argument_component_ordinal,
                m.logical_path.as_str(),
            )
        }))
    {
        ensure!(
            coordinates.insert((ordinal, component)),
            "native6_duplicate_input_coordinate"
        );
        let slot = args
            .get_mut(usize::try_from(ordinal)?)
            .context("native6_input_coordinate_missing")?;
        let name = format!("{PREFIX}/{logical}");
        if let Some(component) = component {
            let prefix = slot
                .strip_prefix("-Wl,")
                .context("native6_input_component_not_wl")?;
            let mut parts = prefix.split(',').map(str::to_owned).collect::<Vec<_>>();
            let part = parts
                .get_mut(usize::try_from(component)?)
                .context("native6_component_missing")?;
            ensure!(*part == logical, "native6_component_logical_changed");
            *part = name;
            *slot = format!("-Wl,{}", parts.join(","));
        } else {
            ensure!(*slot == logical, "native6_argument_logical_changed");
            *slot = name;
        }
    }
    Ok(args)
}
pub(crate) fn append_native(
    root: &SkillRoot,
    args: &mut Vec<String>,
    recipe: &CodeNativeBuildRecipe6,
    cwd: &Path,
) -> Result<Vec<String>> {
    let sdk = verify_pins(root, recipe)?;
    let suffix = vec![
        "-isysroot".into(),
        sdk.canonical_root,
        "-v".into(),
        format!("-fuse-ld={}", role(recipe, "native_linker")?.path),
        "-Xlinker".into(),
        "-reproducible".into(),
        "-Xlinker".into(),
        "-oso_prefix".into(),
        "-Xlinker".into(),
        format!("{}/", cwd.display()),
    ];
    args.extend(suffix.clone());
    Ok(suffix)
}
pub(crate) fn input_files(
    f: &CodeNativeInputPreparationFacts,
    recipe: &CodeNativeBuildRecipe6,
) -> Vec<CodeNativeFileBinding> {
    recipe
        .tools
        .iter()
        .map(|t| t.file.clone())
        .chain(recipe.external_archive_bindings.clone())
        .chain(
            f.direct_objects
                .iter()
                .flat_map(|m| [m.original_file.clone(), m.staged_file.clone()]),
        )
        .chain(
            f.archives
                .iter()
                .flat_map(|m| [m.original_file.clone(), m.staged_file.clone()]),
        )
        .chain(f.responses.iter().map(|m| m.file.clone()))
        .collect()
}
/// Finite admitted clang syntax and declared SDK libraries. Search directories
/// cannot introduce a same-named shadow library or an unselected external root.
pub(crate) fn check_native_scope(
    root: &SkillRoot,
    f: &CodeNativeInputPreparationFacts,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<()> {
    check_base_scope(root, f, recipe, None)
}
/// A /7 content substep may declare the one dynamic-library occurrence only
/// after checking the original compiler source/type/output relationship. This
/// shared syntax check grants no native dispatch; /6 still passes no extension.
pub(crate) fn check_complete_base_scope7(
    root: &SkillRoot,
    f: &CodeNativeInputPreparationFacts,
    recipe: &CodeNativeBuildRecipe6,
    dynamic_ordinal: usize,
) -> Result<()> {
    ensure!(
        f.expanded_arguments
            .get(dynamic_ordinal)
            .is_some_and(|a| a == "-dynamiclib")
            && f.expanded_arguments
                .iter()
                .filter(|a| *a == "-dynamiclib")
                .count()
                == 1,
        "native7_dynamic_occurrence_missing_or_duplicate"
    );
    check_base_scope(root, f, recipe, Some(dynamic_ordinal))
}
fn check_base_scope(
    root: &SkillRoot,
    f: &CodeNativeInputPreparationFacts,
    recipe: &CodeNativeBuildRecipe6,
    dynamic_ordinal: Option<usize>,
) -> Result<()> {
    let args = &f.expanded_arguments;
    require_nodefaultlibs(args)?;
    let target = Path::new(&f.request.target_root);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if dynamic_ordinal == Some(i) {
            ensure!(arg == "-dynamiclib", "native7_dynamic_occurrence_changed");
            i += 1;
            continue;
        }
        if arg == "-o" {
            ensure!(args.get(i + 1).is_some(), "native6_output_missing");
            i += 2;
            continue;
        }
        if arg == "-arch" {
            ensure!(
                args.get(i + 1).is_some_and(|s| s == "arm64"),
                "native6_arch_unsupported"
            );
            i += 2;
            continue;
        }
        if arg.starts_with("-L") {
            let directory = if arg == "-L" {
                i += 1;
                args.get(i).context("native6_search_path_missing")?
            } else {
                &arg[2..]
            };
            let path = Path::new(directory);
            oe_core::no_symlinks(path)?;
            ensure!(
                path.is_absolute()
                    && (path.starts_with(target)
                        || recipe
                            .external_archive_bindings
                            .iter()
                            .any(|a| Path::new(&a.path).parent() == Some(path))),
                "native6_search_path_unselected"
            );
            crate::native_sdk4::check_library_search_paths4(&[path.to_owned()])?;
            for name in [
                "libSystem.a",
                "libSystem.dylib",
                "libSystem.tbd",
                "libc.a",
                "libc.dylib",
                "libc.tbd",
                "libm.a",
                "libm.dylib",
                "libm.tbd",
            ] {
                ensure!(
                    !path.join(name).exists()
                        && std::fs::symlink_metadata(path.join(name)).is_err(),
                    "native6_system_library_shadow_unsupported"
                );
            }
            i += 1;
            continue;
        }
        if [
            "-nodefaultlibs",
            "-lSystem",
            "-lc",
            "-lm",
            "-lproc",
            "-liconv",
            "-lcharset",
            "-Wl,-dead_strip",
            "-mmacosx-version-min=11.0.0",
        ]
        .contains(&arg.as_str())
        {
            i += 1;
            continue;
        }
        if arg.starts_with("-Wl,-force_load,") {
            ensure!(
                f.archives
                    .iter()
                    .any(|m| m.input_argument_ordinal == i as u64
                        && m.argument_component_ordinal == Some(1)),
                "native6_force_load_unbound"
            );
            i += 1;
            continue;
        }
        ensure!(
            f.direct_objects
                .iter()
                .any(|m| m.input_argument_ordinal == i as u64
                    && m.argument_component_ordinal.is_none())
                || f.archives
                    .iter()
                    .any(|m| m.input_argument_ordinal == i as u64
                        && m.argument_component_ordinal.is_none())
                || recipe
                    .external_archive_bindings
                    .iter()
                    .any(|m| m.path == *arg),
            "native6_received_flag_or_input_unsupported:{arg}"
        );
        i += 1;
    }
    // MachO embedded linker options are another input channel. Retain complete
    // payload but explicitly refuse until its library facts have a typed domain.
    for m in &f.direct_objects {
        reject_embedded_link_inputs(&snapshot(root, &m.original_bytes)?)?;
    }
    for a in &f.archives {
        for m in &a.members {
            reject_embedded_link_inputs(&snapshot(root, &m.original_payload)?)?;
        }
    }
    for external in &recipe.external_archive_bindings {
        crate::native_profile::verify_file(external)?;
        crate::native_archive::inspect_original_archive(
            &read_regular_bytes(Path::new(&external.path), 128 * 1024 * 1024)?,
            reject_embedded_link_inputs,
        )?;
        crate::native_profile::verify_file(external)?;
    }
    verify_pins(root, recipe)?;
    Ok(())
}
fn require_nodefaultlibs(args: &[String]) -> Result<()> {
    ensure!(
        args.iter().any(|s| s == "-nodefaultlibs"),
        "native6_nodefaultlibs_required_from_original_argv"
    );
    Ok(())
}
pub(crate) fn check_original_input(path: &Path, raw: &[u8]) -> Result<()> {
    if path.extension().is_some_and(|e| e == "a" || e == "rlib") {
        crate::native_archive::inspect_original_archive(raw, reject_embedded_link_inputs)
    } else {
        reject_embedded_link_inputs(raw)
    }
}
fn snapshot(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    let raw = read_regular_bytes(&root.private_path(Path::new(&r.path))?, 128 * 1024 * 1024)?;
    ensure!(sha256(&raw) == r.sha256, "native6_snapshot_changed");
    Ok(raw)
}
fn reject_embedded_link_inputs(raw: &[u8]) -> Result<()> {
    ensure!(
        raw.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]),
        "native6_object_format_unsupported"
    );
    let u32_at = |offset: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            raw.get(offset..offset + 4)
                .context("native6_macho_header")?
                .try_into()?,
        ))
    };
    ensure!(
        u32_at(4)? == 0x0100000c && u32_at(12)? == 1,
        "native6_object_target_or_filetype_unsupported"
    );
    let count = u32_at(16)?;
    let bytes = usize::try_from(u32_at(20)?)?;
    ensure!(
        count <= 4096 && 32usize.checked_add(bytes).is_some_and(|e| e <= raw.len()),
        "native6_macho_load_commands"
    );
    let mut at = 32usize;
    for _ in 0..count {
        let cmd = u32_at(at)?;
        let size = usize::try_from(u32_at(at + 4)?)?;
        ensure!(
            size >= 8 && at.checked_add(size).is_some_and(|e| e <= 32 + bytes),
            "native6_macho_load_command"
        );
        ensure!(
            cmd != 0x2d
                && ![
                    0xc,
                    0x18 | 0x80000000,
                    0x1f | 0x80000000,
                    0x20,
                    0x23 | 0x80000000,
                    0x1c | 0x80000000
                ]
                .contains(&cmd),
            "native6_embedded_library_input_unsupported"
        );
        ensure!(
            [0x19, 0x32, 0x2, 0xb, 0x24, 0x29, 0x2e].contains(&cmd),
            "native6_object_load_command_unsupported:{cmd}"
        );
        let minimum = match cmd {
            0x19 => 72,
            0x32 => 24,
            0x2 => 24,
            0xb => 80,
            _ => 16,
        };
        ensure!(
            size >= minimum && size % 8 == 0,
            "native6_object_load_command_structure"
        );
        if cmd == 0x19 {
            let sections = usize::try_from(u32_at(at + 64)?)?;
            ensure!(
                sections <= 16384
                    && sections.checked_mul(80).and_then(|s| s.checked_add(72)) == Some(size),
                "native6_object_segment_structure"
            );
        } else if cmd == 0x32 {
            let tools = usize::try_from(u32_at(at + 20)?)?;
            ensure!(
                tools <= 128 && tools.checked_mul(8).and_then(|s| s.checked_add(24)) == Some(size),
                "native6_object_build_version_structure"
            );
        }
        at += size;
    }
    ensure!(at == 32 + bytes, "native6_macho_load_command_size");
    Ok(())
}
pub(crate) fn observe_process(
    root: &SkillRoot,
    exe: &CodeNativeFileBinding,
    args: Vec<String>,
    cwd: &Path,
    folder: &Path,
    deadline: Instant,
    end: u64,
    limit: usize,
    role_name: &str,
    parent: &crate::native_loader6::ActualCompilerParent,
) -> Result<CodeNativeInputProcessEvidence> {
    let mut spec = CommandSpec {
        executable: exe.path.clone().into(),
        arguments: args.iter().map(Into::into).collect(),
        cwd: cwd.into(),
        environment: envmap(),
        remove_environment: crate::native_loader6::selected_child_removals()?,
        timeout: deadline_timeout(deadline)?,
        output_limit: limit,
    };
    crate::native_runtime::freeze_command(root, &mut spec, folder, end)?;
    let raw = run_command(&spec, folder)?;
    let evidence = CodeNativeInputProcessEvidence {
        schema: "oev2.native-input-process-evidence/3".into(),
        role: role_name.into(),
        executable: exe.clone(),
        arguments: args,
        cwd: cwd.to_str().context("native6_path")?.into(),
        environment: environment(),
        process: reference(root, &folder.join("process.json"))?,
        stdout: reference(root, &folder.join("stdout"))?,
        stderr: reference(root, &folder.join("stderr"))?,
        status: raw.status,
        exit_code: raw.exit_code.map(i64::from),
    };
    crate::native_process6::retain(root, &evidence, parent)?;
    Ok(evidence)
}
pub(crate) fn entry(received: Vec<String>) -> Result<i32> {
    entry_with_revision(received, 6)
}
pub(crate) fn entry_compiler7(received: Vec<String>) -> Result<i32> {
    entry_with_revision(received, 7)
}
pub(crate) fn entry_sdk8(received: Vec<String>) -> Result<i32> {
    entry_with_revision(received, 8)
}
fn entry_with_revision(received: Vec<String>, revision: u8) -> Result<i32> {
    let compiler7 = revision == 7;
    ensure!(
        received.len() <= 10000 && received.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "native6_received_argv_limit"
    );
    let context_path = PathBuf::from(
        std::env::var("OE_NATIVE_STAGE_CONTEXT").context("native6_stage_context_missing")?,
    );
    let expected_sha =
        std::env::var("OE_NATIVE_STAGE_CONTEXT_SHA256").context("native6_stage_digest_missing")?;
    let bytes = read_regular_bytes(&context_path, 16 * 1024 * 1024)?;
    ensure!(sha256(&bytes) == expected_sha, "native6_context_changed");
    let context = oe_wire::json::strict_json(&bytes)?;
    let root = SkillRoot::new(
        context["source_root"]
            .as_str()
            .context("native6_source_root")?,
    )?;
    let side_ref: ArtifactReference = serde_json::from_value(context["side_context"].clone())?;
    // Consume one fresh complete inspection of these jointly bound facts.
    // The original checker performs its full before/final physical checks;
    // validate_context and every later execution check below remain separate.
    let (side, _profile, recipe) =
        crate::native_profile6::load_compiler_entry(&root, &side_ref)?.into_parts();
    ensure!(
        match revision {
            6 =>
                !crate::native_link7::is_adapter7(&recipe)?
                    && !crate::native_link8::is_adapter8(&recipe)?,
            7 => crate::native_link7::is_adapter7(&recipe)?,
            8 => crate::native_link8::is_adapter8(&recipe)?,
            _ => false,
        },
        "native_link_dispatch_revision_or_selected_adapter_changed"
    );
    let parent_stage = context["parent_stage"]
        .as_str()
        .context("native6_parent_stage")?;
    ensure!(
        validate_context(&root, &side_ref, &side, &recipe, parent_stage)? == context,
        "native6_context_replay_changed"
    );
    let stage = Path::new(&side.stage_root).join(parent_stage);
    let actual_caller = std::env::current_dir()?;
    // Retain the original exact source-root caller domain. The selected-only
    // helper authorizes additional real package roots without projecting cwd.
    let approved_caller =
        if context["schema"] == "oev2.native-build-stage/6" && actual_caller != root.path() {
            Some(crate::native_sources6::caller_root(
                &root,
                &side,
                parent_stage,
                &context,
                &actual_caller,
            )?)
        } else {
            None
        };
    let received_context_matches = context_path == stage.join("stage-context.json")
        && (actual_caller == root.path()
            || approved_caller
                .as_ref()
                .is_some_and(|p| p == &actual_caller))
        && std::env::var("CARGO_ENCODED_RUSTFLAGS")? == recipe.raw_encoded_rustflags
        && std::env::var_os("TMPDIR").is_some_and(|p| {
            PathBuf::from(p) == PathBuf::from(context["temporary_root"].as_str().unwrap_or(""))
        });
    // Private failure diagnosis preserves the complete original predicate.
    // Only explicitly selected non-secret paths and flags digests are emitted.
    if !received_context_matches {
        eprintln!(
            "native6_private_actual_context_diagnostic:{}",
            json!({
            "actual_cwd":std::env::current_dir()?, "expected_cwd":root.path(),
            "actual_context_path":context_path, "expected_context_path":stage.join("stage-context.json"),
            "actual_flags_sha256":std::env::var("CARGO_ENCODED_RUSTFLAGS").ok().map(|s|sha256(s.as_bytes())),
            "expected_flags_sha256":recipe.raw_encoded_rustflags_sha256,
            "actual_tmpdir":std::env::var_os("TMPDIR").map(PathBuf::from),
            "expected_tmpdir":context["temporary_root"], "guard":"original_complete_predicate_unchanged"})
        );
    }
    ensure!(
        received_context_matches,
        "native6_received_context_or_flags_changed"
    );
    let own = &crate::driver_root().context("native6_linker_root")?;
    ensure!(
        digest(&public_manifest(own)?)? == compiled()?["manifest_sha256"]
            && std::env::current_exe()?.canonicalize()?
                == Path::new(&role(&recipe, "linker_adapter")?.path),
        "native6_adapter_source_or_tool_changed"
    );
    let loader_parent = crate::native_loader6::actual_compiler_parent(
        &root,
        &side,
        &recipe,
        &context_path,
        &received,
    )?;
    let end = context["deadline_epoch_ms"]
        .as_u64()
        .context("native6_deadline")?;
    let remaining = end
        .checked_sub(epoch_ms()?)
        .context("native6_deadline_exhausted")?;
    ensure!(
        remaining > 0 && remaining <= side.body_budget.timeout_ms,
        "native6_deadline_not_same_body"
    );
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(remaining))
        .context("native6_deadline_overflow")?;
    let (ordinal, cwd) = crate::native_link::fresh_invocation(&root, &stage)?;
    write_new(
        &cwd.join("received.json"),
        &json!({"side_context":side_ref,"stage_context_sha256":expected_sha,"received_arguments":received,
        "received_cwd":actual_caller,"raw_encoded_rustflags":recipe.raw_encoded_rustflags}),
    )?;
    let request = CodeNativeInputPreparationRequest {
        schema: "oev2.native-input-preparation-request/3".into(),
        algorithm: recipe.algorithm.clone(),
        received_arguments: received,
        caller_cwd: actual_caller.to_str().context("native6_path")?.into(),
        target_root: side.build.target_dir.clone(),
        stage_root: cwd.join(PREFIX).to_str().context("native6_path")?.into(),
        evidence_root: cwd
            .join("input-evidence")
            .to_str()
            .context("native6_path")?
            .into(),
        external_archives: recipe.external_archive_bindings.clone(),
        index_executable: role(&recipe, "archive_reader")?.clone(),
        index_environment: environment(),
    };
    let mut callbacks = 0u64;
    let preparation = crate::native_archive::prepare_link_inputs_scoped_verified(
        &root,
        &request,
        &mut |index: &CodeNativeArchiveIndexRequest| {
            deadline_timeout(deadline)?;
            verify_pins(&root, &recipe)?;
            let p = cwd.join(format!("archive-index-{callbacks:06}"));
            callbacks += 1;
            observe_process(
                &root,
                &index.executable,
                index.arguments.clone(),
                Path::new(&index.cwd),
                &p,
                deadline,
                end,
                usize::try_from(side.body_budget.output_bytes)?,
                "archive_index",
                &loader_parent,
            )
        },
        &mut require_nodefaultlibs,
        Some(&mut check_original_input),
        approved_caller.as_deref(),
        &mut |p| crate::native_process6::verify(&root, p, &side, &recipe, &context_path),
    );
    let facts = match preparation {
        Ok(f) => f,
        Err(e) => {
            if revision == 8 && format!("{e:#}") == "native_input_implicit_library_unsupported" {
                ensure!(callbacks == 0, "sdk8_original_refusal_after_index_callback");
                write_new(
                    &cwd.join("original3-preparation-refusal.json"),
                    &json!({
                    "execution":"original_domain_refused","reason":format!("{e:#}"),"actual_index_callbacks":callbacks}),
                )?;
                let result = crate::native_link8::capture_execute(
                    &root,
                    &side_ref,
                    &side,
                    &recipe,
                    parent_stage,
                    &context_path,
                    &context,
                    &bytes,
                    ordinal,
                    &cwd,
                    &loader_parent,
                    &request,
                    approved_caller.as_deref(),
                    deadline,
                    end,
                );
                write_new(
                    &cwd.join("sdk8-status.json"),
                    &json!({"schema":"oev2.native-sdk8-status/1",
                    "original_domain":"refused","original_index_callbacks":callbacks,
                    "result":match &result{Ok(code)=>json!({"state":"native_process_observed","exit_code":code}),
                        Err(error)=>json!({"state":"failed_or_partial","reason":format!("{error:#}")})},
                    "whole_pair":"not_qualified_by_this_record"}),
                )?;
                return result;
            }
            // Preserve the original refusal. While the actual selected Rustc
            // still waits for this child, retain explicit /7 content evidence.
            // No v3 invocation, successful link or compiler capability is
            // reconstructed from the preparation projection.
            if request
                .received_arguments
                .iter()
                .any(|a| a == "-exported_symbols_list" || a == "-Wl,-exported_symbols_list")
                && format!("{e:#}").contains("native_input_hidden_file_option_unsupported")
            {
                let mut content_callbacks = 0u64;
                let capture = crate::native_input_content7::capture(
                    &root,
                    &side_ref,
                    &reference(&root, &context_path)?,
                    &loader_parent,
                    &request,
                    &reference(&root, &cwd.join("received.json"))?,
                    &cwd,
                    approved_caller.as_deref(),
                    &mut |index: &CodeNativeArchiveIndexRequest| {
                        deadline_timeout(deadline)?;
                        verify_pins(&root, &recipe)?;
                        let folder =
                            cwd.join(format!("content7-archive-index-{content_callbacks:06}"));
                        content_callbacks += 1;
                        observe_process(
                            &root,
                            &index.executable,
                            index.arguments.clone(),
                            Path::new(&index.cwd),
                            &folder,
                            deadline,
                            end,
                            usize::try_from(side.body_budget.output_bytes)?,
                            "archive_index",
                            &loader_parent,
                        )
                    },
                    &mut |p| {
                        crate::native_process6::verify(&root, p, &side, &recipe, &context_path)
                    },
                )
                .and_then(|observation| {
                    deadline_timeout(deadline)?;
                    let assessment = crate::native_compiler_link7::inspect_retained_complete7(
                        &root,
                        &reference(&root, &cwd.join("compiler-input-content7.json"))?,
                    )?;
                    write_new(
                        &cwd.join("compiler-complete-link-inputs7.json"),
                        &serde_json::to_value(assessment)?,
                    )?;
                    verify_pins(&root, &recipe)?;
                    ensure!(
                        validate_context(&root, &side_ref, &side, &recipe, parent_stage)?
                            == context
                            && read_regular_bytes(&context_path, 16 * 1024 * 1024)? == bytes,
                        "native6_content7_source_context_changed_after_capture"
                    );
                    Ok(observation)
                });
                write_new(
                    &cwd.join("compiler-input-content7-status.json"),
                    &json!({
                        "schema":"oev2.native-compiler-input-content-status/7",
                        "original_refusal":format!("{e:#}"), "original_index_callbacks":callbacks,
                        "content_index_callbacks":content_callbacks,
                        "capture":match &capture { Ok(_) => json!({"state":"retained_content_only"}),
                            Err(error) => json!({"state":"failed_or_partial","reason":format!("{error:#}")}) },
                        "native_execution":"not_run", "original_admission":"refused", "whole_pair":"not_qualified"
                    }),
                )?;
                if compiler7 || revision == 8 {
                    write_new(
                        &cwd.join("original3-preparation-refusal.json"),
                        &json!({
                            "execution":"original_domain_refused","reason":format!("{e:#}"),"actual_index_callbacks":callbacks
                        }),
                    )?;
                    let observation = capture?;
                    if revision == 8 {
                        return crate::native_link7::execute_complete_in_adapter8(
                            &root,
                            &side_ref,
                            &side,
                            &recipe,
                            parent_stage,
                            &context_path,
                            &context,
                            &bytes,
                            ordinal,
                            &cwd,
                            &loader_parent,
                            &observation,
                            approved_caller.as_deref(),
                            deadline,
                            end,
                        );
                    }
                    return crate::native_link7::execute_complete(
                        &root,
                        &side_ref,
                        &side,
                        &recipe,
                        parent_stage,
                        &context_path,
                        &context,
                        &bytes,
                        ordinal,
                        &cwd,
                        &loader_parent,
                        &observation,
                        approved_caller.as_deref(),
                        deadline,
                        end,
                    );
                }
            }
            write_new(
                &cwd.join("rejected.json"),
                &json!({"execution":"not_qualified","reason":format!("{e:#}"),"actual_index_callbacks":callbacks}),
            )?;
            return Err(e);
        }
    };
    check_native_scope(&root, &facts, &recipe)?;
    let output = facts
        .expanded_arguments
        .windows(2)
        .find(|w| w[0] == "-o")
        .context("native6_output_missing")?;
    ensure!(!Path::new(&output[1]).exists(), "native6_output_not_fresh");
    let mut args = execution_arguments(&facts)?;
    let suffix = append_native(&root, &mut args, &recipe, &cwd)?;
    let before = input_files(&facts, &recipe);
    for file in &before {
        crate::native_profile::verify_file(file)?;
    }
    crate::native_sdkroot6::recheck_actual(&root, &recipe, &loader_parent.sdk_root)?;
    let sdk = verify_pins(&root, &recipe)?;
    let selected = crate::native_sdk4::bind_selected_libraries4(&sdk, &facts.expanded_arguments)?;
    crate::native_loader6::retain_native_receipt(&root, &cwd, &loader_parent, selected, &sdk)?;
    write_new(
        &cwd.join("inputs-before.json"),
        &json!({"files":before,"sdk":sdk,"source_manifest":public_manifest(root.path())?,"raw_encoded_rustflags":recipe.raw_encoded_rustflags}),
    )?;
    let process = observe_process(
        &root,
        role(&recipe, "clang_driver")?,
        args,
        &cwd,
        &cwd.join("native-process"),
        deadline,
        end,
        usize::try_from(side.body_budget.output_bytes)?,
        "native_link",
        &loader_parent,
    )?;
    crate::native_sdkroot6::recheck_actual(&root, &recipe, &loader_parent.sdk_root)?;
    crate::native_archive::verify_live_inputs(&facts)?;
    for file in &before {
        crate::native_profile::verify_file(file)?;
    }
    ensure!(
        verify_pins(&root, &recipe)? == sdk
            && public_manifest(root.path())? == context["source_manifest"]
            && read_regular_bytes(&context_path, 16 * 1024 * 1024)? == bytes,
        "native6_inputs_source_context_changed_after_link"
    );
    ensure!(
        validate_context(&root, &side_ref, &side, &recipe, parent_stage)? == context,
        "native6_compilation_sources_changed_after_native_link"
    );
    write_new(
        &cwd.join("inputs-after.json"),
        &json!({"files":input_files(&facts,&recipe),"sdk":sdk,"source_manifest":public_manifest(root.path())?,"raw_encoded_rustflags":recipe.raw_encoded_rustflags}),
    )?;
    let invocation=CodeNativeArchiveResponseInvocation{schema:"oev2.native-archive-response-invocation/3".into(),ordinal,parent_stage:parent_stage.into(),inputs:facts,native_recipe_suffix:suffix,native_process:process,
        uncovered:vec!["Finite original input, regular archive and response domain; whole platform/engineering qualification remains separate".into()]};
    write_new(
        &cwd.join("invocation.json"),
        &serde_json::to_value(&invocation)?,
    )?;
    crate::native_process6::verify(
        &root,
        &invocation.native_process,
        &side,
        &recipe,
        &context_path,
    )?;
    i32::try_from(
        invocation
            .native_process
            .exit_code
            .context("native6_exit_missing")?,
    )
    .context("native6_exit_range")
}
pub(crate) fn load_invocations(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<CodeNativeArchiveResponseInvocation>> {
    let mut all = Vec::new();
    for parent_stage in ["build-normalizer", "build-runner"] {
        let context = validate_context(root, side_ref, side, recipe, parent_stage)?;
        let stage = Path::new(&side.stage_root).join(parent_stage);
        let mut dirs = std::fs::read_dir(&stage)?
            .map(|r| r.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|p| p.to_str())
                    .is_some_and(|p| p.starts_with("call-"))
            })
            .collect::<Vec<_>>();
        dirs.sort();
        ensure!(
            !dirs.is_empty() && dirs.len() <= 4096,
            "native6_invocation_inventory_empty_or_limit"
        );
        for (ordinal, cwd) in dirs.iter().enumerate() {
            ensure!(
                cwd.file_name().and_then(|p| p.to_str())
                    == Some(format!("call-{ordinal:06}").as_str()),
                "native6_invocation_gap"
            );
            let v: CodeNativeArchiveResponseInvocation = serde_json::from_value(read(
                root,
                &reference(root, &cwd.join("invocation.json"))?,
            )?)?;
            v.validate()
                .map_err(|e| anyhow::anyhow!("native6_invocation_contract:{e:?}"))?;
            ensure!(
                v.schema == "oev2.native-archive-response-invocation/3"
                    && v.ordinal == ordinal as u64
                    && v.parent_stage == parent_stage,
                "native6_invocation_changed"
            );
            let caller = Path::new(&v.inputs.request.caller_cwd);
            let approved = if caller != root.path() {
                Some(crate::native_sources6::caller_root(
                    root,
                    side,
                    parent_stage,
                    &context,
                    caller,
                )?)
            } else {
                None
            };
            verify_one_invocation(
                root,
                side_ref,
                side,
                recipe,
                parent_stage,
                &context,
                cwd,
                &v,
                approved.as_deref(),
            )?;
            all.push(v);
        }
        ensure!(
            validate_context(root, side_ref, side, recipe, parent_stage)? == context,
            "native6_source_inventory_changed_during_invocation_inspection"
        );
    }
    Ok(all)
}

pub(crate) fn verify_one_invocation(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    parent_stage: &str,
    context: &Value,
    cwd: &Path,
    v: &CodeNativeArchiveResponseInvocation,
    approved_caller: Option<&Path>,
) -> Result<()> {
    let stage = Path::new(&side.stage_root).join(parent_stage);
    v.validate()
        .map_err(|e| anyhow::anyhow!("native6_invocation_contract:{e:?}"))?;
    ensure!(
        v.schema == "oev2.native-archive-response-invocation/3"
            && v.parent_stage == parent_stage
            && cwd.file_name().and_then(|s| s.to_str())
                == Some(format!("call-{:06}", v.ordinal).as_str()),
        "native6_invocation_changed"
    );
    let req = &v.inputs.request;
    let actual_caller = Path::new(&req.caller_cwd);
    ensure!(
        req.algorithm == recipe.algorithm
            && (actual_caller == root.path() || approved_caller == Some(actual_caller))
            && req.target_root == side.build.target_dir
            && Path::new(&req.stage_root) == cwd.join(PREFIX)
            && Path::new(&req.evidence_root) == cwd.join("input-evidence")
            && req.index_executable == *role(recipe, "archive_reader")?
            && req.index_environment == environment()
            && req.external_archives == recipe.external_archive_bindings,
        "native6_pure_stage_relationship_changed"
    );
    crate::native_archive::verify_prepared_scoped_verified(
        root,
        &v.inputs,
        approved_caller,
        &mut |p| {
            crate::native_process6::verify(root, p, side, recipe, &stage.join("stage-context.json"))
        },
    )?;
    check_native_scope(root, &v.inputs, recipe)?;
    let received = read(root, &reference(root, &cwd.join("received.json"))?)?;
    ensure!(
        received["side_context"] == serde_json::to_value(side_ref)?
            && received["stage_context_sha256"]
                == reference(root, &stage.join("stage-context.json"))?.sha256
            && received["received_arguments"] == serde_json::to_value(&req.received_arguments)?
            && received["received_cwd"] == req.caller_cwd
            && received["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
        "native6_received_binding_changed"
    );
    let mut args = execution_arguments(&v.inputs)?;
    let suffix = append_native(root, &mut args, recipe, cwd)?;
    ensure!(
        v.native_recipe_suffix == suffix
            && v.native_process.executable == *role(recipe, "clang_driver")?
            && v.native_process.arguments == args
            && Path::new(&v.native_process.cwd) == cwd
            && v.native_process.environment == environment()
            && v.native_process.role == "native_link"
            && v.native_process.process
                == reference(root, &cwd.join("native-process/process.json"))?
            && v.native_process.stdout == reference(root, &cwd.join("native-process/stdout"))?
            && v.native_process.stderr == reference(root, &cwd.join("native-process/stderr"))?,
        "native6_execution_argv_or_evidence_changed"
    );
    let before = read(root, &reference(root, &cwd.join("inputs-before.json"))?)?;
    let after = read(root, &reference(root, &cwd.join("inputs-after.json"))?)?;
    ensure!(
        before == after
            && before["files"] == serde_json::to_value(input_files(&v.inputs, recipe))?
            && before["sdk"] == serde_json::to_value(verify_pins(root, recipe)?)?
            && before["source_manifest"] == public_manifest(root.path())?
            && before["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
        "native6_before_after_inputs_changed"
    );
    crate::native_process6::verify(
        root,
        &v.native_process,
        side,
        recipe,
        &stage.join("stage-context.json"),
    )?;
    crate::native_runtime::verify_frozen_command(
        root,
        &v.native_process.process,
        side.body_budget.output_bytes,
        side.body_budget.timeout_ms,
        Some(
            context["deadline_epoch_ms"]
                .as_u64()
                .context("native6_deadline")?,
        ),
    )?;
    crate::native_runtime::verify_process(
        root,
        &v.native_process.process,
        &v.native_process.executable,
        cwd,
        &args,
    )?;
    for (index, a) in v.inputs.archives.iter().enumerate() {
        let folder = cwd.join(format!("archive-index-{index:06}"));
        ensure!(
            a.index_process.process == reference(root, &folder.join("process.json"))?,
            "native6_archive_callback_wrong_side"
        );
        crate::native_runtime::verify_frozen_command(
            root,
            &a.index_process.process,
            side.body_budget.output_bytes,
            side.body_budget.timeout_ms,
            Some(
                context["deadline_epoch_ms"]
                    .as_u64()
                    .context("native6_deadline")?,
            ),
        )?;
        crate::native_runtime::verify_process(
            root,
            &a.index_process.process,
            &a.index_process.executable,
            Path::new(&a.index_request.cwd),
            &a.index_request.arguments,
        )?;
    }
    Ok(())
}
