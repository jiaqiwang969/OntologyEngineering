//! Production direct-object implementation. Own archives and response files are
//! intentionally refused by this explicit recipe; no flattening fallback exists.
use crate::native_runtime::{
    deadline_timeout, epoch_ms, file_binding, read, reference, role, verify_pins,
};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{create_new_private_dir, read_regular_bytes, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn arguments_with_binding<F>(
    received: &[String],
    side: &CodeNativeSideContext,
    recipe: &CodeNativeBuildRecipe,
    mut own_binding: F,
) -> Result<(Vec<String>, Vec<CodeNativeObjectMapping>)>
where
    F: FnMut(usize, &Path) -> Result<CodeNativeFileBinding>,
{
    ensure!(
        received.len() <= 10_000 && received.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "native_argv_limit"
    );
    let target = Path::new(&side.build.target_dir);
    let mut mappings = Vec::new();
    let mut transformed = Vec::new();
    let mut i = 0;
    let mut output = None;
    while i < received.len() {
        let arg = &received[i];
        ensure!(
            !arg.starts_with('@'),
            "native_response_unsupported_direct_recipe"
        );
        if arg == "-o" {
            let value = received.get(i + 1).context("native_output_missing")?;
            let path = Path::new(value);
            oe_core::no_symlinks(path)?;
            ensure!(
                path.is_absolute() && path.starts_with(target) && path != target,
                "native_output_not_own_side"
            );
            ensure!(
                output.replace(value.clone()).is_none(),
                "native_duplicate_output"
            );
            transformed.extend([arg.clone(), value.clone()]);
            i += 2;
            continue;
        }
        if arg == "-arch" {
            ensure!(
                received.get(i + 1).map(String::as_str) == Some("arm64"),
                "native_arch_unsupported"
            );
            transformed.extend([arg.clone(), "arm64".into()]);
            i += 2;
            continue;
        }
        if [
            "-lSystem",
            "-lc",
            "-lm",
            "-nodefaultlibs",
            "-Wl,-dead_strip",
            "-mmacosx-version-min=11.0.0",
        ]
        .contains(&arg.as_str())
        {
            transformed.push(arg.clone());
            i += 1;
            continue;
        }
        let path = Path::new(arg);
        oe_core::no_symlinks(path)?;
        if matches!(
            path.extension().and_then(|v| v.to_str()),
            Some("a" | "rlib")
        ) {
            ensure!(
                !path.starts_with(target),
                "native_own_archive_unsupported_direct_recipe"
            );
            let pin = recipe
                .external_archive_bindings
                .iter()
                .find(|p| p.path == *arg)
                .context("native_external_archive_unbound")?;
            crate::native_profile::verify_file(pin)?;
            mappings.push(CodeNativeObjectMapping {
                argument_ordinal: i as u64,
                kind: "external_archive".into(),
                original: pin.clone(),
                staged: None,
                logical_name: arg.clone(),
                payload_sha256: pin.sha256.clone(),
            });
            transformed.push(arg.clone());
            i += 1;
            continue;
        }
        ensure!(
            path.is_absolute()
                && path.starts_with(target)
                && path.extension().is_some_and(|s| s == "o"),
            "native_unknown_or_cross_side_input"
        );
        let original = own_binding(i, path)?;
        original
            .validate()
            .map_err(|e| anyhow::anyhow!("native_original_binding:{e:?}"))?;
        ensure!(
            original.path == *arg,
            "native_original_binding_path_changed"
        );
        let logical_name = format!(
            "{}/objects/{i:06}-{}.o",
            side.logical_debug_root, original.sha256
        );
        mappings.push(CodeNativeObjectMapping {
            argument_ordinal: i as u64,
            kind: "own_object".into(),
            payload_sha256: original.sha256.clone(),
            original,
            staged: None,
            logical_name: logical_name.clone(),
        });
        transformed.push(logical_name);
        i += 1;
    }
    ensure!(
        output.is_some() && mappings.iter().any(|m| m.kind == "own_object"),
        "native_required_output_or_objects_missing"
    );
    Ok((transformed, mappings))
}
pub(crate) fn native_arguments(
    received: &[String],
    side: &CodeNativeSideContext,
    recipe: &CodeNativeBuildRecipe,
) -> Result<(Vec<String>, Vec<CodeNativeObjectMapping>)> {
    let parsed = arguments_with_binding(received, side, recipe, |_, path| file_binding(path))?;
    let output = received
        .windows(2)
        .find(|w| w[0] == "-o")
        .context("native_output_missing")?;
    ensure!(
        !Path::new(&output[1]).exists(),
        "native_output_not_fresh_own_side"
    );
    Ok(parsed)
}
fn validate_context(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext,
    recipe: &CodeNativeBuildRecipe,
    parent_stage: &str,
    context: &Value,
) -> Result<()> {
    let keys: BTreeSet<_> = context
        .as_object()
        .context("native_stage_context_object")?
        .keys()
        .map(String::as_str)
        .collect();
    ensure!(
        keys == BTreeSet::from([
            "schema",
            "source_root",
            "source_manifest",
            "side_context",
            "recipe",
            "parent_stage",
            "deadline_epoch_ms",
            "output_bytes",
            "invocation_root",
            "temporary_root"
        ]),
        "native_stage_context_unknown_field"
    );
    let profile = crate::native_profile::inspect_profile(root, &side.source_profile)?;
    let stage = Path::new(&side.stage_root).join(parent_stage);
    let temporary = Path::new(&side.build.target_dir)
        .join("native-tmp")
        .join(parent_stage);
    ensure!(
        ["build-normalizer", "build-runner"].contains(&parent_stage)
            && context["schema"] == "oev2.native-build-stage/1"
            && context["source_root"] == root.path().to_str().context("native_path_utf8")?
            && context["source_manifest"] == public_manifest(root.path())?
            && context["side_context"] == serde_json::to_value(side_ref)?
            && context["recipe"] == serde_json::to_value(&profile.recipe)?
            && context["parent_stage"] == parent_stage
            && context["invocation_root"] == stage.to_str().context("native_path_utf8")?
            && context["temporary_root"] == temporary.to_str().context("native_path_utf8")?
            && context["output_bytes"] == side.body_budget.output_bytes
            && context["deadline_epoch_ms"].as_u64().is_some_and(|v| v > 0)
            && recipe.raw_encoded_rustflags_sha256
                == sha256(recipe.raw_encoded_rustflags.as_bytes()),
        "native_stage_context_changed"
    );
    Ok(())
}
fn stage_inputs(
    recipe: &CodeNativeBuildRecipe,
    maps: &[CodeNativeObjectMapping],
) -> Vec<CodeNativeFileBinding> {
    recipe
        .tools
        .iter()
        .map(|t| t.file.clone())
        .chain(recipe.sdk_bindings.clone())
        .chain(recipe.external_archive_bindings.clone())
        .chain(maps.iter().map(|m| m.original.clone()))
        .chain(maps.iter().filter_map(|m| m.staged.clone()))
        .collect()
}
pub(crate) fn fresh_invocation(root: &SkillRoot, stage: &Path) -> Result<(u64, PathBuf)> {
    for ordinal in 0..4096u64 {
        let path = stage.join(format!("call-{ordinal:06}"));
        match create_new_private_dir(&root.private_path(&path)?) {
            Ok(()) => return Ok((ordinal, path)),
            Err(e) if path.is_dir() => {
                let _ = e;
            }
            Err(e) => return Err(e),
        }
    }
    anyhow::bail!("native_invocation_inventory_limit")
}
fn append_native(args: &mut Vec<String>, recipe: &CodeNativeBuildRecipe, cwd: &Path) -> Result<()> {
    let sdk = recipe
        .sdk_bindings
        .iter()
        .find(|p| {
            Path::new(&p.path)
                .file_name()
                .is_some_and(|n| n == "SDKSettings.json")
        })
        .and_then(|p| Path::new(&p.path).parent())
        .context("native_sdk_missing")?;
    args.extend([
        "-isysroot".into(),
        sdk.to_str().context("native_path_utf8")?.into(),
        "-v".into(),
        format!("-fuse-ld={}", role(recipe, "native_linker")?.path),
        "-Xlinker".into(),
        "-reproducible".into(),
        "-Xlinker".into(),
        "-oso_prefix".into(),
        "-Xlinker".into(),
        format!("{}/", cwd.display()),
    ]);
    Ok(())
}
pub(crate) fn entry(received: Vec<String>) -> Result<i32> {
    let context_path = PathBuf::from(
        std::env::var("OE_NATIVE_STAGE_CONTEXT").context("native_stage_context_missing")?,
    );
    let expected_sha =
        std::env::var("OE_NATIVE_STAGE_CONTEXT_SHA256").context("native_stage_digest_missing")?;
    let bytes = read_regular_bytes(&context_path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&bytes) == expected_sha,
        "native_stage_context_changed"
    );
    let context = oe_wire::json::strict_json(&bytes)?;
    let keys: BTreeSet<_> = context
        .as_object()
        .context("native_stage_context_object")?
        .keys()
        .map(String::as_str)
        .collect();
    ensure!(
        keys == BTreeSet::from([
            "schema",
            "source_root",
            "source_manifest",
            "side_context",
            "recipe",
            "parent_stage",
            "deadline_epoch_ms",
            "output_bytes",
            "invocation_root",
            "temporary_root"
        ]),
        "native_stage_context_unknown_field"
    );
    ensure!(
        context["schema"] == "oev2.native-build-stage/1",
        "native_stage_context_schema"
    );
    let root = SkillRoot::new(
        context["source_root"]
            .as_str()
            .context("native_source_root")?,
    )?;
    ensure!(
        root.private_path(&context_path)? == context_path,
        "native_stage_context_path_changed"
    );
    let side_ref: ArtifactReference = serde_json::from_value(context["side_context"].clone())?;
    let side = crate::native_profile::load_side(&root, &side_ref)?;
    let recipe = crate::native_profile::load_recipe(&root, &side.source_profile)?;
    let profile = crate::native_profile::inspect_profile(&root, &side.source_profile)?;
    ensure!(
        context["recipe"] == serde_json::to_value(&profile.recipe)?,
        "native_recipe_changed"
    );
    let parent_stage = context["parent_stage"]
        .as_str()
        .context("native_parent_stage")?;
    ensure!(
        ["build-normalizer", "build-runner"].contains(&parent_stage),
        "native_parent_stage_invalid"
    );
    let stage = Path::new(&side.stage_root).join(parent_stage);
    ensure!(
        context["invocation_root"] == stage.to_str().context("native_path_utf8")?
            && context_path == stage.join("stage-context.json"),
        "native_stage_outside_side"
    );
    let temporary = Path::new(&side.build.target_dir)
        .join("native-tmp")
        .join(parent_stage);
    ensure!(
        context["temporary_root"] == temporary.to_str().context("native_path_utf8")?
            && std::env::var_os("TMPDIR").is_some_and(|p| PathBuf::from(p) == temporary),
        "native_temp_root_changed"
    );
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").context("native_raw_flags_missing")?;
    ensure!(
        flags == recipe.raw_encoded_rustflags
            && sha256(flags.as_bytes()) == recipe.raw_encoded_rustflags_sha256,
        "native_raw_flags_changed"
    );
    let own_source = &crate::driver_root().context("native_linker_source_root")?;
    ensure!(
        digest(&public_manifest(own_source)?)? == compiled()?["manifest_sha256"],
        "native_linker_compiled_source_changed"
    );
    ensure!(
        std::env::current_exe()?.canonicalize()?
            == Path::new(&role(&recipe, "linker_adapter")?.path).canonicalize()?,
        "native_linker_executable_changed"
    );
    ensure!(
        public_manifest(root.path())? == context["source_manifest"],
        "native_source_changed_before_link"
    );
    verify_pins(&recipe)?;
    let remaining = context["deadline_epoch_ms"]
        .as_u64()
        .context("native_deadline_missing")?
        .checked_sub(epoch_ms()?)
        .context("native_deadline_exhausted")?;
    ensure!(
        remaining > 0 && remaining <= side.body_budget.timeout_ms,
        "native_deadline_not_same_body"
    );
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(remaining))
        .context("native_deadline_overflow")?;
    let limit = context["output_bytes"]
        .as_u64()
        .context("native_output_limit_missing")?;
    ensure!(
        limit == side.body_budget.output_bytes && (1..=4 * 1024 * 1024).contains(&limit),
        "native_output_limit_changed"
    );
    validate_context(&root, &side_ref, &side, &recipe, parent_stage, &context)?;
    ensure!(
        std::env::current_dir()? == root.path(),
        "native_received_cwd_changed"
    );
    let (ordinal, cwd) = fresh_invocation(&root, &stage)?;
    write_new(
        &cwd.join("received.json"),
        &json!({"side_context":side_ref,"stage_context_sha256":expected_sha,"received_arguments":received,"received_cwd":std::env::current_dir()?,"raw_encoded_rustflags":flags}),
    )?;
    let prepared = native_arguments(&received, &side, &recipe);
    let (mut args, mut maps) = match prepared {
        Ok(v) => v,
        Err(e) => {
            write_new(
                &cwd.join("rejected.json"),
                &json!({"execution":"not_run","reason":format!("{e:#}")}),
            )?;
            return Err(e);
        }
    };
    create_new_private_dir(&cwd.join(&side.logical_debug_root).join("objects"))?;
    for mapping in &mut maps {
        if mapping.kind == "own_object" {
            let raw = read_regular_bytes(Path::new(&mapping.original.path), 512 * 1024 * 1024)?;
            ensure!(
                sha256(&raw) == mapping.original.sha256,
                "native_object_changed"
            );
            let staged = cwd.join(&mapping.logical_name);
            write_new_bytes(&staged, &raw)?;
            mapping.staged = Some(file_binding(&staged)?);
            ensure!(
                mapping.staged.as_ref().unwrap().sha256 == mapping.original.sha256,
                "native_staged_payload_changed"
            );
        }
    }
    append_native(&mut args, &recipe, &cwd)?;
    let before = stage_inputs(&recipe, &maps);
    for p in &before {
        crate::native_profile::verify_file(p)?;
    }
    write_new(
        &cwd.join("inputs-before.json"),
        &serde_json::to_value(&before)?,
    )?;
    let environment = BTreeMap::from([
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("LC_ALL".into(), "C".into()),
    ]);
    let mut spec = CommandSpec {
        executable: role(&recipe, "clang_driver")?.path.clone().into(),
        arguments: args.iter().map(Into::into).collect(),
        cwd: cwd.clone(),
        environment,
        remove_environment: std::env::vars_os()
            .map(|(k, _)| k)
            .filter(|k| k != "PATH" && k != "LC_ALL")
            .collect(),
        timeout: deadline_timeout(deadline)?,
        output_limit: usize::try_from(limit)?,
    };
    crate::native_runtime::freeze_command(
        &root,
        &mut spec,
        &cwd.join("native-process"),
        context["deadline_epoch_ms"]
            .as_u64()
            .context("native_deadline_missing")?,
    )?;
    let observed = run_command(&spec, &cwd.join("native-process"))?;
    for p in &before {
        crate::native_profile::verify_file(p)?;
    }
    ensure!(
        public_manifest(root.path())? == context["source_manifest"]
            && read_regular_bytes(&context_path, 16 * 1024 * 1024)? == bytes,
        "native_source_or_context_changed_after_link"
    );
    let after = stage_inputs(&recipe, &maps);
    ensure!(before == after, "native_inputs_changed_after_link");
    write_new(
        &cwd.join("inputs-after.json"),
        &serde_json::to_value(&after)?,
    )?;
    let invocation = CodeNativeInvocation {
        ordinal,
        parent_stage: parent_stage.into(),
        executable: role(&recipe, "clang_driver")?.clone(),
        cwd: cwd.to_str().context("native_path_utf8")?.into(),
        received_arguments: received,
        executed_arguments: args,
        environment: vec![
            CodeNativeEnvironmentEntry {
                name: "LC_ALL".into(),
                value: "C".into(),
            },
            CodeNativeEnvironmentEntry {
                name: "PATH".into(),
                value: "/usr/bin:/bin".into(),
            },
        ],
        response_files: vec![],
        objects: maps,
        process: reference(&root, &cwd.join("native-process/process.json"))?,
        stdout: reference(&root, &cwd.join("native-process/stdout"))?,
        stderr: reference(&root, &cwd.join("native-process/stderr"))?,
        stage_inputs_before: before,
        stage_inputs_after: after,
    };
    write_new(
        &cwd.join("invocation.json"),
        &serde_json::to_value(&invocation)?,
    )?;
    ensure!(
        observed.status == "completed" || observed.status == "process_failed",
        "native_process_budget_or_capture_failed"
    );
    Ok(observed.exit_code.context("native_exit_missing")?)
}

pub(crate) fn load_invocations(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext,
    recipe: &CodeNativeBuildRecipe,
) -> Result<Vec<CodeNativeInvocation>> {
    let mut all = Vec::new();
    for parent_stage in ["build-normalizer", "build-runner"] {
        let stage = Path::new(&side.stage_root).join(parent_stage);
        let context_ref = reference(root, &stage.join("stage-context.json"))?;
        let context = read(root, &context_ref)?;
        validate_context(root, side_ref, side, recipe, parent_stage, &context)?;
        let mut dirs = std::fs::read_dir(&stage)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|s| s.starts_with("call-"))
            })
            .map(|e| e.path())
            .collect::<Vec<_>>();
        dirs.sort();
        ensure!(dirs.len() <= 4096, "native_invocation_limit");
        for (ordinal, dir) in dirs.iter().enumerate() {
            ensure!(
                dir.file_name().and_then(|s| s.to_str())
                    == Some(format!("call-{ordinal:06}").as_str()),
                "native_invocation_gap"
            );
            let v: CodeNativeInvocation = serde_json::from_value(read(
                root,
                &reference(root, &dir.join("invocation.json"))?,
            )?)?;
            v.validate()
                .map_err(|e| anyhow::anyhow!("native_invocation:{e:?}"))?;
            ensure!(
                v.ordinal == ordinal as u64
                    && v.parent_stage == parent_stage
                    && Path::new(&v.cwd) == dir
                    && v.executable == *role(recipe, "clang_driver")?
                    && v.response_files.is_empty(),
                "native_invocation_changed"
            );
            let received = read(root, &reference(root, &dir.join("received.json"))?)?;
            ensure!(
                received["side_context"] == serde_json::to_value(side_ref)?
                    && received["received_cwd"]
                        == root.path().to_str().context("native_path_utf8")?
                    && received["stage_context_sha256"] == context_ref.sha256
                    && received["received_arguments"]
                        == serde_json::to_value(&v.received_arguments)?
                    && received["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
                "native_received_argv_or_flags_changed"
            );
            let (mut expected, mut objects) =
                arguments_with_binding(&v.received_arguments, side, recipe, |index, _| {
                    let hits = v
                        .objects
                        .iter()
                        .filter(|m| m.argument_ordinal == index as u64 && m.kind == "own_object")
                        .collect::<Vec<_>>();
                    ensure!(hits.len() == 1, "native_mapping_missing_or_ambiguous");
                    Ok(hits[0].original.clone())
                })?;
            for m in &mut objects {
                if m.kind == "own_object" {
                    let recorded = v
                        .objects
                        .iter()
                        .find(|p| p.argument_ordinal == m.argument_ordinal)
                        .context("native_mapping_missing")?;
                    let staged = recorded.staged.as_ref().context("native_staged_missing")?;
                    crate::native_profile::verify_file(staged)?;
                    ensure!(
                        staged.sha256 == m.original.sha256
                            && staged.bytes == m.original.bytes
                            && staged.path
                                == dir
                                    .join(&m.logical_name)
                                    .to_str()
                                    .context("native_path_utf8")?,
                        "native_mapping_payload_changed"
                    );
                    m.staged = Some(staged.clone());
                }
            }
            ensure!(
                objects == v.objects,
                "native_object_order_or_duplicates_changed"
            );
            append_native(&mut expected, recipe, dir)?;
            ensure!(
                expected == v.executed_arguments,
                "native_executed_argv_changed"
            );
            ensure!(
                v.process == reference(root, &dir.join("native-process/process.json"))?,
                "native_process_wrong_side"
            );
            let before: Vec<CodeNativeFileBinding> = serde_json::from_value(read(
                root,
                &reference(root, &dir.join("inputs-before.json"))?,
            )?)?;
            let after: Vec<CodeNativeFileBinding> = serde_json::from_value(read(
                root,
                &reference(root, &dir.join("inputs-after.json"))?,
            )?)?;
            ensure!(
                before == after
                    && before == v.stage_inputs_before
                    && after == v.stage_inputs_after
                    && before == stage_inputs(recipe, &v.objects),
                "native_inputs_before_after_changed"
            );
            ensure!(
                v.environment
                    == vec![
                        CodeNativeEnvironmentEntry {
                            name: "LC_ALL".into(),
                            value: "C".into()
                        },
                        CodeNativeEnvironmentEntry {
                            name: "PATH".into(),
                            value: "/usr/bin:/bin".into()
                        }
                    ],
                "native_environment_changed"
            );
            crate::native_runtime::verify_frozen_command(
                root,
                &v.process,
                side.body_budget.output_bytes,
                side.body_budget.timeout_ms,
                Some(
                    context["deadline_epoch_ms"]
                        .as_u64()
                        .context("native_deadline_missing")?,
                ),
            )?;
            let (stdout, stderr) = crate::native_runtime::verify_process(
                root,
                &v.process,
                &v.executable,
                dir,
                &v.executed_arguments,
            )?;
            ensure!(
                reference(root, &dir.join("native-process/stdout"))? == v.stdout
                    && reference(root, &dir.join("native-process/stderr"))? == v.stderr
                    && sha256(&stdout) == v.stdout.sha256
                    && sha256(&stderr) == v.stderr.sha256,
                "native_raw_stream_binding_changed"
            );
            all.push(v);
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures(root: &Path) -> (CodeNativeSideContext, CodeNativeBuildRecipe) {
        let r = json!({"path":root.join("ref.json"),"sha256":"0".repeat(64)});
        let build = json!({"artifact_id":"test","source_root":root,"evidence_path":root.join("var/evidence"),"cargo_path":"/cargo","rustc_path":"/rustc","ggen_path":"/ggen","cargo_home":"/cache","target_dir":root.join("var/target"),"platform":"aarch64-apple-darwin"});
        let side=serde_json::from_value(json!({"schema":"oev2.native-side-context/2","source_profile":r,"recipe_sha256":"0".repeat(64),"owner_context":r,"side_id":"a","build":build,"body_budget":{"timeout_ms":1000,"output_bytes":4096},"stage_root":root.join("var/target-native-stage"),"logical_debug_root":"oe-native-debug"})).unwrap();
        let recipe=serde_json::from_value(json!({"schema":"oev2.native-build-recipe/2","algorithm":"darwin-direct-objects/1","input_domain":"own_objects_external_archives","target":"aarch64-apple-darwin","raw_encoded_rustflags":"","raw_encoded_rustflags_sha256":"0".repeat(64),"debug":{"cargo_manifest":r,"cargo_profile":"dev","debug":2,"split_debuginfo":"unpacked","strip":"none","codegen_units":256,"incremental":true,"lto":false,"scope_packages":["oe-design","oe-code-runner"]},"tools":[],"native_parameter_preset":"test","sdk_bindings":[],"external_archive_bindings":[]})).unwrap();
        (side, recipe)
    }
    #[test]
    fn direct_domain_preserves_duplicate_ordinals_and_rejects_expansion_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let (side, recipe) = fixtures(&root);
        let target = Path::new(&side.build.target_dir);
        std::fs::create_dir_all(target).unwrap();
        let object = target.join("one.o");
        std::fs::write(&object, b"full original payload").unwrap();
        let args = vec![
            "-arch".into(),
            "arm64".into(),
            object.display().to_string(),
            object.display().to_string(),
            "-o".into(),
            target.join("out").display().to_string(),
            "-lSystem".into(),
        ];
        let (_, maps) = native_arguments(&args, &side, &recipe).unwrap();
        assert_eq!(maps.len(), 2);
        assert!(
            maps.iter()
                .all(|m| m.logical_name.starts_with("oe-native-debug/objects/"))
        );
        assert_eq!((maps[0].argument_ordinal, maps[1].argument_ordinal), (2, 3));
        assert_ne!(maps[0].logical_name, maps[1].logical_name);
        assert_eq!(maps[0].original.sha256, maps[1].original.sha256);
        let mut bad = args.clone();
        bad[5] = target.join("../escape").display().to_string();
        assert!(native_arguments(&bad, &side, &recipe).is_err());
        for changed in ["@input.rsp", "-unknown", "/other/one.o"] {
            let mut bad = args.clone();
            bad[2] = changed.into();
            assert!(native_arguments(&bad, &side, &recipe).is_err());
        }
        let mut bad = args.clone();
        bad[2] = target.join("own.rlib").display().to_string();
        assert!(native_arguments(&bad, &side, &recipe).is_err());
        let mut bad = args.clone();
        bad[1] = "x86_64".into();
        assert!(
            arguments_with_binding(&bad, &side, &recipe, |_, path| file_binding(path)).is_err()
        );
        std::fs::write(target.join("out"), b"already exists").unwrap();
        assert!(native_arguments(&args, &side, &recipe).is_err());
    }
}
