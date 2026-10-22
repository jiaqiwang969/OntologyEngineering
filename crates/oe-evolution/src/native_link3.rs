//! Full native /3 adapter: ordered typed archive and response preparation.
use crate::native_runtime::{deadline_timeout, epoch_ms, read, reference};
use crate::native_runtime3::{role, verify_pins};
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
#[cfg(test)]
#[path = "native_link3_fixed_inputs.rs"]
mod fixed_inputs;
fn environment() -> Vec<CodeNativeEnvironmentEntry> {
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
fn reject_inherited_native_environment() -> Result<()> {
    for (key, _) in std::env::vars_os() {
        let key = key.to_str().context("native3_env_utf8")?;
        ensure!(
            !key.starts_with("DYLD_")
                && ![
                    "LIBRARY_PATH",
                    "CPATH",
                    "C_INCLUDE_PATH",
                    "CPLUS_INCLUDE_PATH",
                    "OBJC_INCLUDE_PATH",
                    "SDKROOT",
                    "DEVELOPER_DIR",
                    "MACOSX_DEPLOYMENT_TARGET",
                    "COMPILER_PATH",
                    "CCC_OVERRIDE_OPTIONS",
                    "RC_DEBUG_OPTIONS"
                ]
                .contains(&key),
            "native3_inherited_native_input_environment_unsupported:{key}"
        );
    }
    Ok(())
}
pub(crate) fn validate_context(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext3,
    recipe: &CodeNativeBuildRecipe3,
    parent_stage: &str,
) -> Result<Value> {
    ensure!(
        ["build-normalizer", "build-runner"].contains(&parent_stage),
        "native3_parent_stage"
    );
    let stage = Path::new(&side.stage_root).join(parent_stage);
    let context = read(root, &reference(root, &stage.join("stage-context.json"))?)?;
    let keys = context
        .as_object()
        .context("native3_stage_object")?
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let is_stage4 = context["schema"] == "oev2.native-build-stage/4";
    if is_stage4 {
        let typed: CodeNativeStageContext4 = serde_json::from_value(context.clone())?;
        typed
            .validate()
            .map_err(|e| anyhow::anyhow!("native4_stage_contract:{e:?}"))?;
        crate::native_sources4::inspect(
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
    ensure!(keys == expected_keys, "native3_stage_unknown_field");
    let profile = crate::native_profile3::inspect_profile(root, &side.source_profile)?;
    ensure!(
        (context["schema"] == "oev2.native-build-stage/3" || is_stage4)
            && context["source_root"] == root.path().to_str().context("native3_path")?
            && context["source_manifest"] == public_manifest(root.path())?
            && context["side_context"] == serde_json::to_value(side_ref)?
            && context["recipe"] == serde_json::to_value(profile.recipe)?
            && context["parent_stage"] == parent_stage
            && context["invocation_root"] == stage.to_str().context("native3_path")?
            && context["temporary_root"]
                == Path::new(&side.build.target_dir)
                    .join("native-tmp")
                    .join(parent_stage)
                    .to_str()
                    .context("native3_path")?
            && context["output_bytes"] == side.body_budget.output_bytes
            && context["deadline_epoch_ms"].as_u64().is_some_and(|x| x > 0)
            && recipe.raw_encoded_rustflags_sha256
                == sha256(recipe.raw_encoded_rustflags.as_bytes()),
        "native3_stage_binding_changed"
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
            "native3_duplicate_input_coordinate"
        );
        let slot = args
            .get_mut(usize::try_from(ordinal)?)
            .context("native3_input_coordinate_missing")?;
        let name = format!("{PREFIX}/{logical}");
        if let Some(component) = component {
            let prefix = slot
                .strip_prefix("-Wl,")
                .context("native3_input_component_not_wl")?;
            let mut parts = prefix.split(',').map(str::to_owned).collect::<Vec<_>>();
            let part = parts
                .get_mut(usize::try_from(component)?)
                .context("native3_component_missing")?;
            ensure!(*part == logical, "native3_component_logical_changed");
            *part = name;
            *slot = format!("-Wl,{}", parts.join(","));
        } else {
            ensure!(*slot == logical, "native3_argument_logical_changed");
            *slot = name;
        }
    }
    Ok(args)
}
fn append_native(
    root: &SkillRoot,
    args: &mut Vec<String>,
    recipe: &CodeNativeBuildRecipe3,
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
fn input_files(
    f: &CodeNativeInputPreparationFacts,
    recipe: &CodeNativeBuildRecipe3,
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
fn check_native_scope(
    root: &SkillRoot,
    f: &CodeNativeInputPreparationFacts,
    recipe: &CodeNativeBuildRecipe3,
) -> Result<()> {
    let args = &f.expanded_arguments;
    require_nodefaultlibs(args)?;
    let target = Path::new(&f.request.target_root);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-o" {
            ensure!(args.get(i + 1).is_some(), "native3_output_missing");
            i += 2;
            continue;
        }
        if arg == "-arch" {
            ensure!(
                args.get(i + 1).is_some_and(|s| s == "arm64"),
                "native3_arch_unsupported"
            );
            i += 2;
            continue;
        }
        if arg.starts_with("-L") {
            let directory = if arg == "-L" {
                i += 1;
                args.get(i).context("native3_search_path_missing")?
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
                "native3_search_path_unselected"
            );
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
                    "native3_system_library_shadow_unsupported"
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
                "native3_force_load_unbound"
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
            "native3_received_flag_or_input_unsupported:{arg}"
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
        "native3_nodefaultlibs_required_from_original_argv"
    );
    Ok(())
}
fn check_original_input(path: &Path, raw: &[u8]) -> Result<()> {
    if path.extension().is_some_and(|e| e == "a" || e == "rlib") {
        crate::native_archive::inspect_original_archive(raw, reject_embedded_link_inputs)
    } else {
        reject_embedded_link_inputs(raw)
    }
}
fn snapshot(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    let raw = read_regular_bytes(&root.private_path(Path::new(&r.path))?, 128 * 1024 * 1024)?;
    ensure!(sha256(&raw) == r.sha256, "native3_snapshot_changed");
    Ok(raw)
}
fn reject_embedded_link_inputs(raw: &[u8]) -> Result<()> {
    ensure!(
        raw.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]),
        "native3_object_format_unsupported"
    );
    let u32_at = |offset: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            raw.get(offset..offset + 4)
                .context("native3_macho_header")?
                .try_into()?,
        ))
    };
    ensure!(
        u32_at(4)? == 0x0100000c && u32_at(12)? == 1,
        "native3_object_target_or_filetype_unsupported"
    );
    let count = u32_at(16)?;
    let bytes = usize::try_from(u32_at(20)?)?;
    ensure!(
        count <= 4096 && 32usize.checked_add(bytes).is_some_and(|e| e <= raw.len()),
        "native3_macho_load_commands"
    );
    let mut at = 32usize;
    for _ in 0..count {
        let cmd = u32_at(at)?;
        let size = usize::try_from(u32_at(at + 4)?)?;
        ensure!(
            size >= 8 && at.checked_add(size).is_some_and(|e| e <= 32 + bytes),
            "native3_macho_load_command"
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
            "native3_embedded_library_input_unsupported"
        );
        ensure!(
            [0x19, 0x32, 0x2, 0xb, 0x24, 0x29, 0x2e].contains(&cmd),
            "native3_object_load_command_unsupported:{cmd}"
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
            "native3_object_load_command_structure"
        );
        if cmd == 0x19 {
            let sections = usize::try_from(u32_at(at + 64)?)?;
            ensure!(
                sections <= 16384
                    && sections.checked_mul(80).and_then(|s| s.checked_add(72)) == Some(size),
                "native3_object_segment_structure"
            );
        } else if cmd == 0x32 {
            let tools = usize::try_from(u32_at(at + 20)?)?;
            ensure!(
                tools <= 128 && tools.checked_mul(8).and_then(|s| s.checked_add(24)) == Some(size),
                "native3_object_build_version_structure"
            );
        }
        at += size;
    }
    ensure!(at == 32 + bytes, "native3_macho_load_command_size");
    Ok(())
}
fn observe_process(
    root: &SkillRoot,
    exe: &CodeNativeFileBinding,
    args: Vec<String>,
    cwd: &Path,
    folder: &Path,
    deadline: Instant,
    end: u64,
    limit: usize,
    role_name: &str,
) -> Result<CodeNativeInputProcessEvidence> {
    let mut spec = CommandSpec {
        executable: exe.path.clone().into(),
        arguments: args.iter().map(Into::into).collect(),
        cwd: cwd.into(),
        environment: envmap(),
        remove_environment: vec![],
        timeout: deadline_timeout(deadline)?,
        output_limit: limit,
    };
    crate::native_runtime::freeze_command(root, &mut spec, folder, end)?;
    let raw = run_command(&spec, folder)?;
    Ok(CodeNativeInputProcessEvidence {
        schema: "oev2.native-input-process-evidence/3".into(),
        role: role_name.into(),
        executable: exe.clone(),
        arguments: args,
        cwd: cwd.to_str().context("native3_path")?.into(),
        environment: environment(),
        process: reference(root, &folder.join("process.json"))?,
        stdout: reference(root, &folder.join("stdout"))?,
        stderr: reference(root, &folder.join("stderr"))?,
        status: raw.status,
        exit_code: raw.exit_code.map(i64::from),
    })
}
pub(crate) fn entry(received: Vec<String>) -> Result<i32> {
    ensure!(
        received.len() <= 10000 && received.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "native3_received_argv_limit"
    );
    let context_path = PathBuf::from(
        std::env::var("OE_NATIVE_STAGE_CONTEXT").context("native3_stage_context_missing")?,
    );
    let expected_sha =
        std::env::var("OE_NATIVE_STAGE_CONTEXT_SHA256").context("native3_stage_digest_missing")?;
    let bytes = read_regular_bytes(&context_path, 16 * 1024 * 1024)?;
    ensure!(sha256(&bytes) == expected_sha, "native3_context_changed");
    let context = oe_wire::json::strict_json(&bytes)?;
    let root = SkillRoot::new(
        context["source_root"]
            .as_str()
            .context("native3_source_root")?,
    )?;
    let side_ref: ArtifactReference = serde_json::from_value(context["side_context"].clone())?;
    let side = crate::native_profile3::load_side(&root, &side_ref)?;
    let recipe = crate::native_profile3::load_recipe(&root, &side.source_profile)?;
    let parent_stage = context["parent_stage"]
        .as_str()
        .context("native3_parent_stage")?;
    ensure!(
        validate_context(&root, &side_ref, &side, &recipe, parent_stage)? == context,
        "native3_context_replay_changed"
    );
    let stage = Path::new(&side.stage_root).join(parent_stage);
    let actual_caller = std::env::current_dir()?;
    // Retain the original exact source-root caller domain. The selected-only
    // helper authorizes additional real package roots without projecting cwd.
    let approved_caller =
        if context["schema"] == "oev2.native-build-stage/4" && actual_caller != root.path() {
            Some(crate::native_sources4::caller_root(
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
            "native3_private_actual_context_diagnostic:{}",
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
        "native3_received_context_or_flags_changed"
    );
    let own = &crate::driver_root().context("native3_linker_root")?;
    ensure!(
        digest(&public_manifest(own)?)? == compiled()?["manifest_sha256"]
            && std::env::current_exe()?.canonicalize()?
                == Path::new(&role(&recipe, "linker_adapter")?.path),
        "native3_adapter_source_or_tool_changed"
    );
    reject_inherited_native_environment()?;
    let end = context["deadline_epoch_ms"]
        .as_u64()
        .context("native3_deadline")?;
    let remaining = end
        .checked_sub(epoch_ms()?)
        .context("native3_deadline_exhausted")?;
    ensure!(
        remaining > 0 && remaining <= side.body_budget.timeout_ms,
        "native3_deadline_not_same_body"
    );
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(remaining))
        .context("native3_deadline_overflow")?;
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
        caller_cwd: actual_caller.to_str().context("native3_path")?.into(),
        target_root: side.build.target_dir.clone(),
        stage_root: cwd.join(PREFIX).to_str().context("native3_path")?.into(),
        evidence_root: cwd
            .join("input-evidence")
            .to_str()
            .context("native3_path")?
            .into(),
        external_archives: recipe.external_archive_bindings.clone(),
        index_executable: role(&recipe, "archive_reader")?.clone(),
        index_environment: environment(),
    };
    let mut callbacks = 0u64;
    let preparation = crate::native_archive::prepare_link_inputs_scoped(
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
            )
        },
        &mut require_nodefaultlibs,
        Some(&mut check_original_input),
        approved_caller.as_deref(),
    );
    let facts = match preparation {
        Ok(f) => f,
        Err(e) => {
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
        .context("native3_output_missing")?;
    ensure!(!Path::new(&output[1]).exists(), "native3_output_not_fresh");
    let mut args = execution_arguments(&facts)?;
    let suffix = append_native(&root, &mut args, &recipe, &cwd)?;
    let before = input_files(&facts, &recipe);
    for file in &before {
        crate::native_profile::verify_file(file)?;
    }
    let sdk = verify_pins(&root, &recipe)?;
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
    )?;
    crate::native_archive::verify_live_inputs(&facts)?;
    for file in &before {
        crate::native_profile::verify_file(file)?;
    }
    ensure!(
        verify_pins(&root, &recipe)? == sdk
            && public_manifest(root.path())? == context["source_manifest"]
            && read_regular_bytes(&context_path, 16 * 1024 * 1024)? == bytes,
        "native3_inputs_source_context_changed_after_link"
    );
    ensure!(
        validate_context(&root, &side_ref, &side, &recipe, parent_stage)? == context,
        "native4_compilation_sources_changed_after_native_link"
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
    crate::native_archive::verify_process_evidence(&root, &invocation.native_process)?;
    i32::try_from(
        invocation
            .native_process
            .exit_code
            .context("native3_exit_missing")?,
    )
    .context("native3_exit_range")
}
pub(crate) fn load_invocations(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext3,
    recipe: &CodeNativeBuildRecipe3,
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
            "native3_invocation_inventory_empty_or_limit"
        );
        for (ordinal, cwd) in dirs.iter().enumerate() {
            ensure!(
                cwd.file_name().and_then(|p| p.to_str())
                    == Some(format!("call-{ordinal:06}").as_str()),
                "native3_invocation_gap"
            );
            let v: CodeNativeArchiveResponseInvocation = serde_json::from_value(read(
                root,
                &reference(root, &cwd.join("invocation.json"))?,
            )?)?;
            v.validate()
                .map_err(|e| anyhow::anyhow!("native3_invocation_contract:{e:?}"))?;
            ensure!(
                v.schema == "oev2.native-archive-response-invocation/3"
                    && v.ordinal == ordinal as u64
                    && v.parent_stage == parent_stage,
                "native3_invocation_changed"
            );
            let req = &v.inputs.request;
            let actual_caller = Path::new(&req.caller_cwd);
            let approved_caller = if context["schema"] == "oev2.native-build-stage/4"
                && actual_caller != root.path()
            {
                Some(crate::native_sources4::caller_root(
                    root,
                    side,
                    parent_stage,
                    &context,
                    actual_caller,
                )?)
            } else {
                None
            };
            ensure!(
                req.algorithm == recipe.algorithm
                    && (actual_caller == root.path()
                        || approved_caller.as_deref() == Some(actual_caller))
                    && req.target_root == side.build.target_dir
                    && Path::new(&req.stage_root) == cwd.join(PREFIX)
                    && Path::new(&req.evidence_root) == cwd.join("input-evidence")
                    && req.index_executable == *role(recipe, "archive_reader")?
                    && req.index_environment == environment()
                    && req.external_archives == recipe.external_archive_bindings,
                "native3_pure_stage_relationship_changed"
            );
            crate::native_archive::verify_prepared_scoped(
                root,
                &v.inputs,
                approved_caller.as_deref(),
            )?;
            check_native_scope(root, &v.inputs, recipe)?;
            let received = read(root, &reference(root, &cwd.join("received.json"))?)?;
            ensure!(
                received["side_context"] == serde_json::to_value(side_ref)?
                    && received["stage_context_sha256"]
                        == reference(root, &stage.join("stage-context.json"))?.sha256
                    && received["received_arguments"]
                        == serde_json::to_value(&req.received_arguments)?
                    && received["received_cwd"] == req.caller_cwd
                    && received["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
                "native3_received_binding_changed"
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
                    && v.native_process.stdout
                        == reference(root, &cwd.join("native-process/stdout"))?
                    && v.native_process.stderr
                        == reference(root, &cwd.join("native-process/stderr"))?,
                "native3_execution_argv_or_evidence_changed"
            );
            let before = read(root, &reference(root, &cwd.join("inputs-before.json"))?)?;
            let after = read(root, &reference(root, &cwd.join("inputs-after.json"))?)?;
            ensure!(
                before == after
                    && before["files"] == serde_json::to_value(input_files(&v.inputs, recipe))?
                    && before["sdk"] == serde_json::to_value(verify_pins(root, recipe)?)?
                    && before["source_manifest"] == public_manifest(root.path())?
                    && before["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
                "native3_before_after_inputs_changed"
            );
            crate::native_archive::verify_process_evidence(root, &v.native_process)?;
            crate::native_runtime::verify_frozen_command(
                root,
                &v.native_process.process,
                side.body_budget.output_bytes,
                side.body_budget.timeout_ms,
                Some(
                    context["deadline_epoch_ms"]
                        .as_u64()
                        .context("native3_deadline")?,
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
                    "native3_archive_callback_wrong_side"
                );
                crate::native_runtime::verify_frozen_command(
                    root,
                    &a.index_process.process,
                    side.body_budget.output_bytes,
                    side.body_budget.timeout_ms,
                    Some(
                        context["deadline_epoch_ms"]
                            .as_u64()
                            .context("native3_deadline")?,
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
            all.push(v);
        }
        ensure!(
            validate_context(root, side_ref, side, recipe, parent_stage)? == context,
            "native4_source_inventory_changed_during_invocation_inspection"
        );
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixed_bytes(hex: &str, wanted: &str) -> Vec<u8> {
        let raw = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(sha256(&raw), wanted);
        raw
    }
    fn reference_value() -> Value {
        json!({"path":"var/raw","sha256":"0".repeat(64)})
    }
    fn binding(path: &str) -> Value {
        json!({"path":path,"sha256":"0".repeat(64),"bytes":4,"mode":384})
    }
    fn facts() -> CodeNativeInputPreparationFacts {
        let r = reference_value();
        let direct = |arg: u64, logical: &str| {
            json!({"input_argument_ordinal":arg,"argument_component_ordinal":null,
            "original_file":binding("/root/var/target/duplicate.o"),"staged_file":binding("/root/var/target-native-stage/build-runner/call-000000/oe-native-debug/objects/file.o"),
            "logical_path":logical,"original_bytes":r,"staged_bytes":r})
        };
        serde_json::from_value(json!({"schema":"oev2.native-input-preparation-facts/3","request":{"schema":"oev2.native-input-preparation-request/3","algorithm":"darwin-archive-response/1",
            "received_arguments":[],"caller_cwd":"/root","target_root":"/root/var/target","stage_root":"/root/var/target-native-stage/build-runner/call-000000/oe-native-debug",
            "evidence_root":"/root/var/evidence","external_archives":[],"index_executable":binding("/tool"),"index_environment":[]},
            "expanded_arguments":[],"executed_arguments":["objects/000000-same.o","objects/000001-same.o","-Wl,-force_load,archives/ordered.rlib","/toolchain/libstd.rlib","-o","/root/var/target/output"],
            "direct_objects":[direct(0,"objects/000000-same.o"),direct(1,"objects/000001-same.o")],"archives":[{"input_argument_ordinal":2,"argument_component_ordinal":1,
                "original_path":"/root/var/target/original.rlib","original_file":binding("/root/var/target/original.rlib"),"original_bytes":r,
                "staged_file":binding("/root/var/staged.rlib"),"staged_bytes":r,"logical_path":"archives/ordered.rlib","members":[],"structure_before":[],"structure_after":[],
                "index_request":{"schema":"oev2.native-archive-index-request/3","input_argument_ordinal":2,"argument_component_ordinal":1,"executable":binding("/tool"),"arguments":[],"cwd":"/root","environment":[],"input":binding("/root/var/before"),"input_bytes":r,"ordered_payload_sha256":[]},
                "index_process":{"schema":"oev2.native-input-process-evidence/3","role":"archive_index","executable":binding("/tool"),"arguments":[],"cwd":"/root","environment":[],"process":r,"stdout":r,"stderr":r,"status":"completed","exit_code":0}}],"responses":[],"uncovered":[]})).unwrap()
    }
    #[test]
    fn outer_clang_prefix_preserves_duplicate_coordinates_and_wl_component() {
        let f = facts();
        let actual = execution_arguments(&f).unwrap();
        assert_eq!(
            actual,
            [
                "oe-native-debug/objects/000000-same.o",
                "oe-native-debug/objects/000001-same.o",
                "-Wl,-force_load,oe-native-debug/archives/ordered.rlib",
                "/toolchain/libstd.rlib",
                "-o",
                "/root/var/target/output"
            ]
        );
        assert_eq!(
            f.executed_arguments[2],
            "-Wl,-force_load,archives/ordered.rlib"
        );
        assert_eq!(f.direct_objects.len(), 2);
        let mut bad = f.clone();
        bad.direct_objects[1].input_argument_ordinal = 0;
        assert!(execution_arguments(&bad).is_err());
        let mut bad = f.clone();
        bad.archives[0].argument_component_ordinal = Some(0);
        assert!(execution_arguments(&bad).is_err());
        let mut bad = f.clone();
        bad.direct_objects[0].logical_path = "objects/missing.o".into();
        assert!(execution_arguments(&bad).is_err());
    }
    #[test]
    fn embedded_macho_native_input_channel_is_explicitly_refused() {
        let mut object = vec![0u8; 56];
        object[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        object[4..8].copy_from_slice(&0x0100000cu32.to_le_bytes());
        object[12..16].copy_from_slice(&1u32.to_le_bytes());
        object[16..20].copy_from_slice(&1u32.to_le_bytes());
        object[20..24].copy_from_slice(&8u32.to_le_bytes());
        object[32..36].copy_from_slice(&0x2du32.to_le_bytes());
        object[36..40].copy_from_slice(&8u32.to_le_bytes());
        assert!(
            reject_embedded_link_inputs(&object)
                .unwrap_err()
                .to_string()
                .contains("embedded_library_input_unsupported")
        );
        object[32..36].copy_from_slice(&2u32.to_le_bytes());
        object[20..24].copy_from_slice(&24u32.to_le_bytes());
        object[36..40].copy_from_slice(&24u32.to_le_bytes());
        assert!(reject_embedded_link_inputs(&object).is_ok());
        object[36..40].copy_from_slice(&64u32.to_le_bytes());
        assert!(reject_embedded_link_inputs(&object).is_err());
    }
    #[test]
    fn complete_fixed_native_objects_refuse_fat_unknown_and_embedded_inputs() {
        use fixed_inputs::*;
        let plain = fixed_bytes(MAIN_HEX, MAIN_SHA256);
        reject_embedded_link_inputs(&plain).unwrap();
        let arm = fixed_bytes(AUTO_ARM_HEX, AUTO_ARM_SHA256);
        assert!(
            format!("{:#}", reject_embedded_link_inputs(&arm).unwrap_err())
                .contains("embedded_library_input_unsupported")
        );
        let fat = fixed_bytes(AUTO_FAT_HEX, AUTO_FAT_SHA256);
        assert_eq!(fat.len(), 33200);
        assert!(
            reject_embedded_link_inputs(&fat)
                .unwrap_err()
                .to_string()
                .contains("object_format_unsupported")
        );
        for raw in [
            b"rust opaque rmeta".as_slice(),
            &[0xce, 0xfa, 0xed, 0xfe],
            &[0xfe, 0xed, 0xfa, 0xcf],
            &[0x42, 0x43, 0xc0, 0xde],
        ] {
            assert!(reject_embedded_link_inputs(raw).is_err());
        }
        for (offset, value) in [(4, 0x01000007u32), (12, 2u32)] {
            let mut changed = plain.clone();
            changed[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(reject_embedded_link_inputs(&changed).is_err());
        }
        for command in [
            0xc, 0x80000018, 0x8000001f, 0x20, 0x80000023, 0x8000001c, 0x2d,
        ] {
            let mut changed = plain.clone();
            changed[32..36].copy_from_slice(&u32::to_le_bytes(command));
            assert!(
                reject_embedded_link_inputs(&changed)
                    .unwrap_err()
                    .to_string()
                    .contains("embedded_library_input_unsupported")
            );
        }
    }
    #[test]
    fn external_archive_complete_payload_scan_preserves_original_index_and_bytes() {
        use fixed_inputs::*;
        let raw = fixed_bytes(LIBEXTERNAL_HEX, LIBEXTERNAL_SHA256);
        let before = raw.clone();
        let mut visited = Vec::new();
        crate::native_archive::inspect_original_archive(&raw, |member| {
            visited.push(sha256(member));
            Ok(())
        })
        .unwrap();
        assert_eq!(visited, [AUTO_ARM_SHA256]);
        assert!(
            format!(
                "{:#}",
                crate::native_archive::inspect_original_archive(&raw, reject_embedded_link_inputs)
                    .unwrap_err()
            )
            .contains("embedded_library_input_unsupported")
        );
        assert_eq!(raw, before);
        assert_eq!(sha256(&raw), LIBEXTERNAL_SHA256);
        let mut changed = raw.clone();
        changed[8..24].copy_from_slice(b"/               ");
        assert!(crate::native_archive::inspect_original_archive(&changed, |_| Ok(())).is_err());
    }
    #[test]
    fn original_nodefaultlibs_and_all_input_formats_are_checked_before_index_callback() {
        use fixed_inputs::*;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().canonicalize().unwrap();
        std::fs::write(path.join("SKILL.md"), b"test").unwrap();
        std::fs::create_dir(path.join("var")).unwrap();
        let target = path.join("var/target");
        std::fs::create_dir(&target).unwrap();
        let object = target.join("main.o");
        std::fs::write(&object, fixed_bytes(MAIN_HEX, MAIN_SHA256)).unwrap();
        let archive = path.join("var/external.a");
        std::fs::write(&archive, fixed_bytes(LIBEXTERNAL_HEX, LIBEXTERNAL_SHA256)).unwrap();
        let root = SkillRoot::new(&path).unwrap();
        let mut request = CodeNativeInputPreparationRequest {
            schema: "oev2.native-input-preparation-request/3".into(),
            algorithm: "darwin-archive-response/1".into(),
            received_arguments: vec![
                object.display().to_string(),
                archive.display().to_string(),
                "-o".into(),
                target.join("output").display().to_string(),
            ],
            caller_cwd: path.display().to_string(),
            target_root: target.display().to_string(),
            stage_root: path.join("var/target-stage").display().to_string(),
            evidence_root: path.join("var/evidence").display().to_string(),
            external_archives: vec![crate::native_archive::bind_file(&archive).unwrap()],
            index_executable: crate::native_archive::bind_file(Path::new("/usr/bin/true")).unwrap(),
            index_environment: environment(),
        };
        let mut callbacks = 0usize;
        let mut indexer =
            |_: &CodeNativeArchiveIndexRequest| -> Result<CodeNativeInputProcessEvidence> {
                callbacks += 1;
                anyhow::bail!("unexpected callback")
            };
        let error = crate::native_archive::prepare_link_inputs_checked(
            &root,
            &request,
            &mut indexer,
            &mut require_nodefaultlibs,
            Some(&mut check_original_input),
        )
        .unwrap_err();
        assert!(error.to_string().contains("nodefaultlibs_required"));
        assert!(!Path::new(&request.stage_root).exists());
        assert!(!Path::new(&request.evidence_root).exists());
        let original = request.received_arguments.clone();
        request
            .received_arguments
            .insert(0, "-nodefaultlibs".into());
        let error = crate::native_archive::prepare_link_inputs_checked(
            &root,
            &request,
            &mut indexer,
            &mut require_nodefaultlibs,
            Some(&mut check_original_input),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("embedded_library_input_unsupported"));
        assert!(!Path::new(&request.stage_root).exists());
        assert_eq!(request.received_arguments[1..], original);
        request.received_arguments.remove(2);
        std::fs::write(&object, fixed_bytes(AUTO_FAT_HEX, AUTO_FAT_SHA256)).unwrap();
        assert!(
            crate::native_archive::prepare_link_inputs_checked(
                &root,
                &request,
                &mut indexer,
                &mut require_nodefaultlibs,
                Some(&mut check_original_input)
            )
            .unwrap_err()
            .to_string()
            .contains("object_format_unsupported")
        );
        assert!(!Path::new(&request.stage_root).exists());
        assert_eq!(callbacks, 0);
        std::fs::write(&object, fixed_bytes(MAIN_HEX, MAIN_SHA256)).unwrap();
        let facts = crate::native_archive::prepare_link_inputs_checked(
            &root,
            &request,
            &mut |_| anyhow::bail!("unexpected callback"),
            &mut require_nodefaultlibs,
            Some(&mut check_original_input),
        )
        .unwrap();
        assert_eq!(facts.expanded_arguments, request.received_arguments);
        assert_eq!(facts.direct_objects.len(), 1);
        assert_eq!(facts.archives.len(), 0);
        crate::native_archive::verify_prepared(&root, &facts).unwrap();
    }
}
