//! Current-stage build-script metadata admission and retained CONTENT checks.
//! Only the wrapper supplies its independently observed living parent. Reading
//! this serialized binding never recreates that parent, exec or native authority.
use crate::native_runtime::{read, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes};
use std::path::{Path, PathBuf};

pub(crate) const POLICY: &str = "cargo_1_95_and_current_bound_build_script_metadata_f2d3ce0bd/3";
const SCHEMA: &str = "oev2.native-build-script-metadata-binding/6";

pub(crate) fn metadata(arguments: &[String]) -> Result<()> {
    ensure!(
        [
            vec!["--version"],
            vec!["-vV"],
            vec!["--version", "--verbose"],
            vec!["--verbose", "--version"]
        ]
        .iter()
        .any(|v| arguments.iter().map(String::as_str).eq(v.iter().copied())),
        "loader6_build_script_not_exact_metadata"
    );
    Ok(())
}
pub(crate) fn same_image(
    parent: &CodeNativeFileBinding,
    output: &CodeNativeFileBinding,
) -> Result<()> {
    crate::native_profile::verify_file(parent)?;
    crate::native_profile::verify_file(output)?;
    ensure!(
        parent.path != output.path
            && parent.sha256 == output.sha256
            && parent.bytes == output.bytes
            && parent.mode == output.mode
            && parent.mode & 0o111 != 0,
        "loader6_build_script_parent_not_linked_image"
    );
    Ok(())
}
pub(crate) fn original_build_source(
    source: &CodeNativeCompilationSourceRoot4,
) -> Result<CodeNativeFileBinding> {
    crate::native_profile::verify_file(&source.cargo_manifest)?;
    let manifest: toml::Value = std::str::from_utf8(&read_regular_bytes(
        Path::new(&source.cargo_manifest.path),
        16 * 1024 * 1024,
    )?)?
    .parse()?;
    let declared = match manifest.get("package").and_then(|p| p.get("build")) {
        Some(toml::Value::String(s)) => s.as_str(),
        None | Some(toml::Value::Boolean(true)) => "build.rs",
        _ => anyhow::bail!("loader6_package_has_no_selected_build_script"),
    };
    let relative = Path::new(declared);
    ensure!(
        !relative.is_absolute()
            && relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "loader6_build_script_source_escape"
    );
    let path = Path::new(&source.root).join(relative);
    no_symlinks(&path)?;
    ensure!(
        path.canonicalize()? == path,
        "loader6_build_script_source_not_canonical"
    );
    let listed = source
        .files
        .iter()
        .filter(|f| Path::new(&f.path) == path)
        .collect::<Vec<_>>();
    ensure!(
        listed.len() == 1,
        "loader6_build_script_source_not_unique_inventory"
    );
    crate::native_profile::verify_file(listed[0])?;
    Ok(listed[0].clone())
}
pub(crate) fn output_path(
    side: &CodeNativeSideContext6,
    source: &CodeNativeCompilationSourceRoot4,
    parent: &CodeNativeFileBinding,
    stage: &str,
) -> Result<PathBuf> {
    let path = Path::new(&parent.path);
    no_symlinks(path)?;
    ensure!(
        path.is_absolute()
            && path.canonicalize()? == path
            && path.file_name().and_then(|n| n.to_str()) == Some("build-script-build"),
        "loader6_build_script_parent_path"
    );
    let folder = path
        .parent()
        .context("loader6_build_script_parent_folder")?;
    let name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .context("loader6_build_script_folder_encoding")?;
    let prefix = format!("{}-", source.package_name);
    let suffix = name
        .strip_prefix(&prefix)
        .context("loader6_build_script_package_folder")?;
    ensure!(
        suffix.len() == 16
            && suffix
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "loader6_build_script_cargo_fingerprint_domain"
    );
    let build = folder
        .parent()
        .context("loader6_build_script_build_folder")?;
    let target = Path::new(&side.build.target_dir);
    let host = target.join("debug/build");
    let selected = target.join(&side.build.platform).join("debug/build");
    ensure!(
        build == host || (stage == "build-runner" && build == selected),
        "loader6_build_script_not_current_target"
    );
    Ok(folder.join(format!("build_script_build-{suffix}")))
}
pub(crate) fn option_value<'a>(arguments: &'a [String], option: &str) -> Result<&'a str> {
    let found = arguments
        .windows(2)
        .filter(|w| w[0] == option)
        .collect::<Vec<_>>();
    ensure!(
        found.len() == 1,
        "loader6_build_script_original_compiler_option_missing_duplicate:{option}"
    );
    Ok(found[0][1].as_str())
}

/// Called with the selected original source inventory returned by this entry's
/// current source read, plus actual parent PID/executable from libproc.
pub(crate) fn inspect_current(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
    policy: &CodeNativeCargoLoaderPolicy6,
    source: &CodeNativeCompilationSourceRoot4,
    parent_pid: u64,
    parent: &CodeNativeFileBinding,
    arguments: &[String],
) -> Result<CodeNativeBuildScriptMetadataBinding6> {
    inspect_current_mode(
        root,
        side,
        recipe,
        context_path,
        context,
        policy,
        source,
        parent_pid,
        parent,
        arguments,
        false,
    )
}

/// Explicit SDK8 producer domain, borrowing the original exact metadata policy
/// and binding fields. A genuine SDK8 artifact reference is never a projected
/// original3 invocation or permission obtained from a retained PID.
pub(crate) fn inspect_current_sdk8(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
    policy: &CodeNativeCargoLoaderPolicy6,
    source: &CodeNativeCompilationSourceRoot4,
    parent_pid: u64,
    parent: &CodeNativeFileBinding,
    arguments: &[String],
) -> Result<CodeNativeBuildScriptMetadataBinding6> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?,
        "sdk8_metadata_selected_adapter_required"
    );
    inspect_current_mode(
        root,
        side,
        recipe,
        context_path,
        context,
        policy,
        source,
        parent_pid,
        parent,
        arguments,
        true,
    )
}

fn inspect_current_mode(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
    policy: &CodeNativeCargoLoaderPolicy6,
    source: &CodeNativeCompilationSourceRoot4,
    parent_pid: u64,
    parent: &CodeNativeFileBinding,
    arguments: &[String],
    sdk8: bool,
) -> Result<CodeNativeBuildScriptMetadataBinding6> {
    ensure!(
        policy.metadata_probe == POLICY && parent_pid > 0 && Path::new(&source.root) != root.path(),
        "loader6_build_script_metadata_domain_unselected"
    );
    metadata(arguments)?;
    let producer = crate::native_producer9::inspect(
        root,
        side,
        recipe,
        context_path,
        context,
        policy,
        source,
        parent_pid,
        parent,
        sdk8,
    )?;
    metadata_binding_from_producer(&producer, arguments)
}

/// The original metadata contract still requires an exact read-only vector.
/// Producer content does not authorize compilation or attest a retained PID.
pub(crate) fn metadata_binding_from_producer(
    producer: &CodeNativeBuildScriptProducer9,
    arguments: &[String],
) -> Result<CodeNativeBuildScriptMetadataBinding6> {
    metadata(arguments)?;
    producer
        .validate()
        .map_err(|e| anyhow::anyhow!("producer9_content_contract:{e:?}"))?;
    let binding = CodeNativeBuildScriptMetadataBinding6 {
        schema: SCHEMA.into(),
        parent_pid: producer.parent_pid,
        parent_executable: producer.parent_executable.clone(),
        linked_output: producer.linked_output.clone(),
        linked_output_record: producer.linked_output_record.clone(),
        build_script_source: producer.build_script_source.clone(),
        cargo_manifest: producer.cargo_manifest.clone(),
        package_id: producer.package_id.clone(),
        compiler_intent: producer.compiler_intent.clone(),
        native_invocation: producer.native_invocation.clone(),
        stage_context: producer.stage_context.clone(),
        arguments: arguments.to_vec(),
        execution: "metadata_only_before_exec".into(),
    };
    binding
        .validate()
        .map_err(|e| anyhow::anyhow!("loader6_build_script_binding_contract:{e:?}"))?;
    Ok(binding)
}

fn sdk8_source_argument(
    arguments: &[String],
    cwd: &Path,
    original: &CodeNativeFileBinding,
) -> Result<()> {
    let sources = arguments
        .iter()
        .filter(|a| a.ends_with(".rs"))
        .collect::<Vec<_>>();
    ensure!(
        sources.len() == 1,
        "sdk8_metadata_original_source_missing_or_duplicate"
    );
    let source = Path::new(sources[0]);
    let absolute = if source.is_absolute() {
        source.to_path_buf()
    } else {
        ensure!(
            source
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
                && !sources[0].contains('\\'),
            "sdk8_metadata_original_source_escape"
        );
        cwd.join(source)
    };
    no_symlinks(&absolute)?;
    ensure!(
        absolute == Path::new(&original.path) && absolute.canonicalize()? == absolute,
        "sdk8_metadata_original_source_not_selected"
    );
    crate::native_profile::verify_file(original)
}

/// Inspect the actual complete SDK8 producer. All original library occurrences
/// stay in that record; its library-free preparation substep is never relabelled.
pub(crate) fn sdk8_image_proof(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context: &CodeNativeStageContext6,
    policy: &CodeNativeCargoLoaderPolicy6,
    source: &CodeNativeCompilationSourceRoot4,
    original: &CodeNativeFileBinding,
    output_path: &Path,
    output: &CodeNativeFileBinding,
    dir: &Path,
    invocation_ref: &ArtifactReference,
    value: serde_json::Value,
) -> Result<Option<(ArtifactReference, ArtifactReference, ArtifactReference)>> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?,
        "sdk8_metadata_selected_adapter_required"
    );
    let v: CodeNativeSdkLinkInvocation8 = serde_json::from_value(value)?;
    v.validate()
        .map_err(|e| anyhow::anyhow!("sdk8_metadata_producer_contract:{e:?}"))?;
    let outputs = v
        .native_process
        .arguments
        .windows(2)
        .filter(|w| w[0] == "-o")
        .collect::<Vec<_>>();
    ensure!(
        outputs.len() == 1,
        "sdk8_metadata_producer_output_missing_or_duplicate"
    );
    if Path::new(&outputs[0][1]) != output_path {
        return Ok(None);
    }
    let domain = &v.inputs.source_assessment.domain;
    ensure!(
        domain.domain_kind == "build_script"
            && domain.crate_name == "build_script_build"
            && domain.crate_type == "bin"
            && domain.source == *original
            && domain.package_manifest == source.cargo_manifest
            && domain.package_id == source.package_id
            && Path::new(&domain.original_output) == output_path
            && v.native_process.status == "completed"
            && v.native_process.exit_code == Some(0),
        "sdk8_metadata_producer_not_selected_build_script"
    );
    let caller = Path::new(&v.inputs.original_request.caller_cwd);
    let context_value = serde_json::to_value(context)?;
    let approved = if caller == root.path() {
        None
    } else {
        Some(crate::native_sources6::caller_root(
            root,
            side,
            &context.parent_stage,
            &context_value,
            caller,
        )?)
    };
    crate::native_link8::verify_one(
        root,
        &context.side_context,
        side,
        recipe,
        &context.parent_stage,
        &context_value,
        dir,
        &v,
        approved.as_deref(),
    )?;
    let (linked_record, actual_output) =
        crate::native_process6::output_record(root, &v.native_process)?;
    ensure!(
        actual_output == *output,
        "sdk8_metadata_image_changed_since_actual_link"
    );
    let receipt_ref = reference(root, &dir.join("loader-receipt.json"))?;
    let receipt: CodeNativeLoaderReceipt6 = serde_json::from_value(read(root, &receipt_ref)?)?;
    receipt
        .validate()
        .map_err(|e| anyhow::anyhow!("sdk8_metadata_loader_contract:{e:?}"))?;
    let intent: CodeNativeCompilerExecIntent6 =
        serde_json::from_value(read(root, &receipt.compiler_intent)?)?;
    intent
        .validate()
        .map_err(|e| anyhow::anyhow!("sdk8_metadata_intent_contract:{e:?}"))?;
    let stage = Path::new(&side.stage_root).join(&context.parent_stage);
    ensure!(
        receipt.compiler_intent
            == reference(
                root,
                &stage
                    .join("loader-events")
                    .join(format!("compiler-{}.json", intent.pid))
            )?
            && receipt.compiler_intent == v.inputs.source_assessment.request.compiler_intent
            && intent.execution == "before_exec"
            && intent.command_scope == "cargo_compiler_command"
            && intent.build_script_binding.is_none()
            && intent.stage_context == reference(root, &stage.join("stage-context.json"))?
            && intent.parent_executable == policy.cargo
            && intent.parent_executable.path == side.build.cargo_path
            && intent.compiler == policy.compiler
            && intent.wrapper == policy.rustc_wrapper
            && intent.cwd == v.inputs.original_request.caller_cwd
            && receipt.parent_pid == intent.pid
            && receipt.parent_executable == intent.compiler,
        "sdk8_metadata_original_cargo_intent_changed"
    );
    ensure!(
        option_value(&intent.arguments, "--crate-name")? == "build_script_build"
            && option_value(&intent.arguments, "--crate-type")? == "bin"
            && Path::new(option_value(&intent.arguments, "--out-dir")?)
                == output_path.parent().context("sdk8_metadata_out_dir")?,
        "sdk8_metadata_original_compiler_role_changed"
    );
    sdk8_source_argument(&intent.arguments, Path::new(&intent.cwd), original)?;
    ensure!(
        reference(root, &dir.join("invocation.json"))? == *invocation_ref
            && reference(root, &dir.join("loader-receipt.json"))? == receipt_ref,
        "sdk8_metadata_prior_records_changed"
    );
    Ok(Some((
        invocation_ref.clone(),
        receipt.compiler_intent,
        linked_record,
    )))
}

fn complete_pre_admission(root: &SkillRoot, stage: &Path) -> Result<()> {
    let events = stage.join("loader-events");
    let mut raws = std::fs::read_dir(&events)?
        .map(|e| e.map(|v| v.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("pre-environment-") && n.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    raws.sort();
    ensure!(
        !raws.is_empty() && raws.len() <= 16384,
        "loader6_current_pre_admission_inventory_empty_or_limit"
    );
    for file in raws {
        let rr = reference(root, &file)?;
        let raw = read(root, &rr)?;
        let pid = raw["pid"]
            .as_u64()
            .context("loader6_raw_pre_admission_pid")?;
        ensure!(
            pid > 0
                && raw["schema"] == "oev2.native-compiler-pre-admission-raw/6"
                && file == events.join(format!("pre-environment-{pid}.json")),
            "loader6_raw_pre_admission_identity_changed"
        );
        let intent_path = events.join(format!("compiler-{pid}.json"));
        ensure!(
            intent_path.try_exists()?,
            "loader6_current_wrapper_refusal_remains_unresolved"
        );
        let ir = reference(root, &intent_path)?;
        let i: CodeNativeCompilerExecIntent6 = serde_json::from_value(read(root, &ir)?)?;
        i.validate()
            .map_err(|e| anyhow::anyhow!("loader6_pre_admission_intent_contract:{e:?}"))?;
        ensure!(
            i.pid == pid
                && i.execution == "before_exec"
                && raw["parent_pid"] == i.parent_pid
                && raw["parent_executable"] == serde_json::to_value(&i.parent_executable)?
                && raw["compiler"] == serde_json::to_value(&i.compiler)?
                && raw["wrapper"] == serde_json::to_value(&i.wrapper)?
                && raw["arguments"] == serde_json::to_value(&i.arguments)?
                && raw["cwd"] == i.cwd
                && raw["stage_context"] == serde_json::to_value(&i.stage_context)?
                && i.stage_context == reference(root, &stage.join("stage-context.json"))?
                && reference(root, &file)? == rr
                && reference(root, &intent_path)? == ir,
            "loader6_raw_pre_admission_binding_changed"
        );
    }
    Ok(())
}

/// Post-run CONTENT verification. This never queries a historical PID or grants
/// permission to execute; it binds all metadata entries actually retained by the
/// same source-bound wrapper to their unchanged stage/source/process records.
pub(crate) fn retained(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    retained_mode(root, side, recipe, false)
}

pub(crate) fn retained_sdk8(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?,
        "sdk8_metadata_selected_adapter_required"
    );
    retained_mode(root, side, recipe, true)
}

fn retained_mode(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    sdk8: bool,
) -> Result<Vec<ArtifactReference>> {
    let policy = crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?;
    let mut refs = Vec::new();
    for stage_name in ["build-normalizer", "build-runner"] {
        let stage = Path::new(&side.stage_root).join(stage_name);
        if policy.metadata_probe == POLICY {
            complete_pre_admission(root, &stage)?;
        }
        let cp = stage.join("stage-context.json");
        let context: CodeNativeStageContext6 =
            serde_json::from_value(read(root, &reference(root, &cp)?)?)?;
        let mut files = std::fs::read_dir(stage.join("loader-events"))?
            .map(|e| e.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("compiler-") && n.ends_with(".json"))
            })
            .collect::<Vec<_>>();
        files.sort();
        ensure!(
            files.len() <= 16384,
            "loader6_metadata_intent_inventory_limit"
        );
        for file in files {
            let r = reference(root, &file)?;
            let i: CodeNativeCompilerExecIntent6 = serde_json::from_value(read(root, &r)?)?;
            i.validate()
                .map_err(|e| anyhow::anyhow!("loader6_retained_metadata_intent_contract:{e:?}"))?;
            if i.command_scope != "bound_build_script_metadata_probe" {
                ensure!(
                    i.build_script_binding.is_none(),
                    "loader6_foreign_scope_has_build_script_binding"
                );
                continue;
            }
            ensure!(
                i.execution == "before_exec"
                    && i.compiler == policy.compiler
                    && i.wrapper == policy.rustc_wrapper
                    && i.stage_context == reference(root, &cp)?
                    && file
                        == stage
                            .join("loader-events")
                            .join(format!("compiler-{}.json", i.pid)),
                "loader6_retained_metadata_identity_changed"
            );
            let b = i
                .build_script_binding
                .as_ref()
                .context("loader6_metadata_binding_missing")?;
            ensure!(
                b.parent_pid == i.parent_pid
                    && b.parent_executable == i.parent_executable
                    && b.arguments == i.arguments,
                "loader6_retained_metadata_parent_or_arguments_changed"
            );
            let selected = crate::native_sources6::caller_source(
                root,
                side,
                stage_name,
                &serde_json::to_value(&context)?,
                Path::new(&i.cwd),
            )?;
            let expected = inspect_current_mode(
                root,
                side,
                recipe,
                &cp,
                &context,
                &policy,
                &selected,
                i.parent_pid,
                &i.parent_executable,
                &i.arguments,
                sdk8,
            )?;
            ensure!(
                *b == expected && reference(root, &file)? == r,
                "loader6_retained_metadata_binding_changed"
            );
            if sdk8 {
                crate::native_host_metadata8::verify_retained_intent(
                    root, side, recipe, &context, &i,
                )?;
            }
            refs.push(r);
        }
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk8_metadata_source_uses_original_compiler_workspace_cwd() {
        let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let package = d.path().join("crates/finite");
        std::fs::create_dir_all(&package).unwrap();
        let source = selected_source(&package);
        let original = original_build_source(&source).unwrap();
        let arguments = vec![
            "--crate-name".into(),
            "build_script_build".into(),
            "crates/finite/build.rs".into(),
        ];
        sdk8_source_argument(&arguments, d.path(), &original).unwrap();
        assert!(sdk8_source_argument(&arguments, &package, &original).is_err());
        sdk8_source_argument(&[original.path.clone()], d.path(), &original).unwrap();
    }
    #[test]
    fn sdk8_metadata_source_rejects_escape_duplicates_and_changed_bytes() {
        let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let package = d.path().join("finite");
        std::fs::create_dir(&package).unwrap();
        let source = selected_source(&package);
        let original = original_build_source(&source).unwrap();
        for args in [
            vec!["finite/../finite/build.rs".into()],
            vec!["finite/build.rs".into(), original.path.clone()],
            vec![],
        ] {
            assert!(sdk8_source_argument(&args, d.path(), &original).is_err());
        }
        std::fs::write(&original.path, b"fn main() { }\n").unwrap();
        assert!(sdk8_source_argument(&["finite/build.rs".into()], d.path(), &original).is_err());
    }
    #[test]
    fn exact_original_metadata_vectors_do_not_authorize_compilation() {
        for v in [
            vec!["--version"],
            vec!["-vV"],
            vec!["--version", "--verbose"],
            vec!["--verbose", "--version"],
        ] {
            metadata(&v.into_iter().map(str::to_owned).collect::<Vec<_>>()).unwrap();
        }
        for v in [
            vec![],
            vec!["--version", "--print=sysroot"],
            vec!["--version", "extra"],
            vec!["-"],
            vec!["--print=file-names"],
            vec!["probe.rs", "--emit=metadata"],
            vec!["@args"],
        ] {
            assert!(metadata(&v.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());
        }
    }
    #[test]
    fn equal_image_content_still_requires_original_mode_and_distinct_paths() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let parent = d.path().join("build-script-build");
        let output = d.path().join("build_script_build-123456789abcdef0");
        std::fs::write(&parent, b"actual finite image content").unwrap();
        std::fs::copy(&parent, &output).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o700)).unwrap();
        let p = crate::native_archive::bind_file(&parent).unwrap();
        let o = crate::native_archive::bind_file(&output).unwrap();
        same_image(&p, &o).unwrap();
        assert!(same_image(&p, &p).is_err());
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(same_image(&p, &crate::native_archive::bind_file(&output).unwrap()).is_err());
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(&output, b"actual finite image contenT").unwrap();
        assert!(same_image(&p, &crate::native_archive::bind_file(&output).unwrap()).is_err());
    }
    fn selected_source(d: &Path) -> CodeNativeCompilationSourceRoot4 {
        let manifest = d.join("Cargo.toml");
        std::fs::write(
            &manifest,
            b"[package]\nname='finite'\nversion='0.1.0'\nbuild='build.rs'\n",
        )
        .unwrap();
        let source = d.join("build.rs");
        std::fs::write(&source, b"fn main() {}\n").unwrap();
        CodeNativeCompilationSourceRoot4 {
            package_id: "finite@0.1.0".into(),
            package_name: "finite".into(),
            package_version: "0.1.0".into(),
            source_kind: "path".into(),
            root: d.display().to_string(),
            cargo_manifest: crate::native_archive::bind_file(&manifest).unwrap(),
            files: vec![crate::native_archive::bind_file(&source).unwrap()],
            enabled_features: vec![],
            source_format: "finite content fixture".into(),
            package_archive: None,
            cargo_cache_metadata: vec![],
            archive_metadata: vec![],
        }
    }
    #[test]
    fn manifest_declared_source_is_exact_inventory_not_a_similarly_named_file() {
        let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let source = selected_source(d.path());
        let original = original_build_source(&source).unwrap();
        assert_eq!(Path::new(&original.path), d.path().join("build.rs"));
        let mut absent = source.clone();
        absent.files.clear();
        assert!(original_build_source(&absent).is_err());
        let mut duplicate = source.clone();
        duplicate.files.push(original.clone());
        assert!(original_build_source(&duplicate).is_err());
        std::fs::write(&original.path, b"fn main() { }\n").unwrap();
        assert!(original_build_source(&source).is_err());
        let mut escaped = selected_source(d.path());
        std::fs::write(
            &escaped.cargo_manifest.path,
            b"[package]\nname='finite'\nversion='0.1.0'\nbuild='../build.rs'\n",
        )
        .unwrap();
        escaped.cargo_manifest =
            crate::native_archive::bind_file(Path::new(&escaped.cargo_manifest.path)).unwrap();
        assert!(original_build_source(&escaped).is_err());
    }
    #[test]
    fn original_build_role_and_option_are_not_any_binary_or_prefix() {
        let args = vec![
            "--crate-name".into(),
            "build_script_build".into(),
            "--crate-type".into(),
            "bin".into(),
        ];
        assert_eq!(
            option_value(&args, "--crate-name").unwrap(),
            "build_script_build"
        );
        let mut duplicate = args.clone();
        duplicate.extend(["--crate-name".into(), "other".into()]);
        assert!(option_value(&duplicate, "--crate-name").is_err());
        assert!(option_value(&args, "--out-dir").is_err());
    }
    #[test]
    fn new_binding_decoder_rejects_missing_identity_and_fake_authority() {
        let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let source = selected_source(d.path());
        let file = source.files[0].clone();
        let r = ArtifactReference {
            path: "var/actual-content-only.json".into(),
            sha256: "a".repeat(64),
        };
        let b = CodeNativeBuildScriptMetadataBinding6 {
            schema: SCHEMA.into(),
            parent_pid: 1,
            parent_executable: file.clone(),
            linked_output: file.clone(),
            linked_output_record: r.clone(),
            build_script_source: file,
            cargo_manifest: source.cargo_manifest,
            package_id: source.package_id,
            compiler_intent: r.clone(),
            native_invocation: r.clone(),
            stage_context: r,
            arguments: vec!["--version".into()],
            execution: "metadata_only_before_exec".into(),
        };
        let mut value = serde_json::to_value(&b).unwrap();
        value["owner_observed"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CodeNativeBuildScriptMetadataBinding6>(value).is_err());
        let mut value = serde_json::to_value(b).unwrap();
        value.as_object_mut().unwrap().remove("native_invocation");
        assert!(serde_json::from_value::<CodeNativeBuildScriptMetadataBinding6>(value).is_err());
    }
    #[test]
    fn complete_capture_rejects_swallowed_wrapper_refusal_and_raw_mismatch() {
        let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let base = d.path();
        std::fs::write(base.join("SKILL.md"), b"finite raw content fixture").unwrap();
        let root = SkillRoot::new(base).unwrap();
        let stage = base.join("var/stage");
        let events = stage.join("loader-events");
        std::fs::create_dir_all(&events).unwrap();
        let context = stage.join("stage-context.json");
        std::fs::write(&context, b"{}\n").unwrap();
        let cp = reference(&root, &context).unwrap();
        let image = events.join("finite-image");
        std::fs::write(&image, b"finite content only").unwrap();
        let pin = crate::native_archive::bind_file(&image).unwrap();
        let i = CodeNativeCompilerExecIntent6 {
            schema: "oev2.native-compiler-exec-intent/6".into(),
            pid: 7,
            parent_pid: 3,
            parent_executable: pin.clone(),
            compiler: pin.clone(),
            wrapper: pin,
            arguments: vec!["-vV".into()],
            cwd: base.display().to_string(),
            stage_context: cp,
            environment: vec![],
            loaded_images: vec![],
            execution: "before_exec".into(),
            uncovered: vec!["synthetic content only; no live execution qualification".into()],
            command_scope: "cargo_version_probe".into(),
            build_script_binding: None,
        };
        let raw = serde_json::json!({"schema":"oev2.native-compiler-pre-admission-raw/6","pid":i.pid,"parent_pid":i.parent_pid,"parent_executable":i.parent_executable,"compiler":i.compiler,"wrapper":i.wrapper,"arguments":i.arguments,"cwd":i.cwd,"stage_context":i.stage_context,"qualification":"not_established"});
        let rp = events.join("pre-environment-7.json");
        std::fs::write(&rp, serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(
            complete_pre_admission(&root, &stage)
                .unwrap_err()
                .to_string()
                .contains("refusal_remains_unresolved")
        );
        std::fs::write(
            events.join("compiler-7.json"),
            serde_json::to_vec(&i).unwrap(),
        )
        .unwrap();
        complete_pre_admission(&root, &stage).unwrap();
        for (key, value) in [
            ("parent_pid", serde_json::json!(4)),
            ("arguments", serde_json::json!(["--version"])),
            ("cwd", serde_json::json!("/foreign")),
        ] {
            let mut changed = raw.clone();
            changed[key] = value;
            std::fs::write(&rp, serde_json::to_vec(&changed).unwrap()).unwrap();
            assert!(complete_pre_admission(&root, &stage).is_err());
        }
    }
}
