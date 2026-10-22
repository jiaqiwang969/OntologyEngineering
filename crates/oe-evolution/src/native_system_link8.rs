//! Source-declared executable and SDK library content, after an actual /3 refusal.
//! This inspector borrows original /6 and SDK4 records. It cannot spawn, select
//! a version, renew a deadline, or recreate the compiler that made those records.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes};
use std::path::{Component, Path};

fn unique(args: &[String], flag: &str) -> Result<String> {
    let mut values = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if arg == flag {
            values.push(
                args.get(i + 1)
                    .context("system8_flag_operand_missing")?
                    .clone(),
            );
        } else if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            values.push(value.into());
        }
    }
    ensure!(
        values.len() == 1 && !values[0].is_empty() && !values[0].contains('\0'),
        "system8_flag_missing_or_duplicate:{flag}"
    );
    Ok(values.remove(0))
}
fn extra_filename(args: &[String]) -> Result<String> {
    let mut values = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let value = if arg == "-C" {
            args.get(i + 1).map(String::as_str)
        } else {
            arg.strip_prefix("-C")
        };
        if let Some(value) = value.and_then(|v| v.strip_prefix("extra-filename=")) {
            values.push(value.into());
        }
    }
    ensure!(
        values.len() == 1,
        "system8_extra_filename_missing_or_duplicate"
    );
    let value: String = values.remove(0);
    ensure!(
        value.starts_with('-')
            && value.len() == 17
            && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),
        "system8_extra_filename_uncovered"
    );
    Ok(value)
}
fn absolute_normal(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute()
            && path
                .components()
                .all(|p| !matches!(p, Component::CurDir | Component::ParentDir))
            && !path
                .to_str()
                .context("system8_path_encoding")?
                .split('/')
                .any(|p| p == "." || p == ".."),
        "system8_path_not_absolute_normal"
    );
    no_symlinks(path)
}
fn relative_source(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && !value.contains('\\')
            && Path::new(value)
                .components()
                .all(|p| matches!(p, Component::Normal(_)))
            && !value
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".."),
        "system8_manifest_source_escape"
    );
    Ok(())
}
fn declared_kind(
    manifest: &toml::Value,
    package: &str,
    crate_name: &str,
    relative: &str,
) -> Result<&'static str> {
    relative_source(relative)?;
    if crate_name == "build_script_build" {
        let source = match manifest.get("package").and_then(|p| p.get("build")) {
            None | Some(toml::Value::Boolean(true)) => "build.rs",
            Some(toml::Value::String(value)) => value,
            _ => anyhow::bail!("system8_build_script_not_declared"),
        };
        relative_source(source)?;
        ensure!(
            relative == source,
            "system8_source_not_declared_build_script"
        );
        return Ok("build_script");
    }
    let mut paths = Vec::new();
    if let Some(bins) = manifest.get("bin") {
        for bin in bins
            .as_array()
            .context("system8_bin_declaration_not_array")?
        {
            let name = bin
                .get("name")
                .and_then(toml::Value::as_str)
                .context("system8_bin_name_missing")?;
            if name.replace('-', "_") == crate_name {
                let path = bin
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("src/bin/{name}.rs"));
                relative_source(&path)?;
                paths.push(path);
            }
        }
    }
    ensure!(paths.len() <= 1, "system8_binary_declaration_ambiguous");
    if paths.is_empty()
        && manifest
            .get("package")
            .and_then(|p| p.get("autobins"))
            .and_then(toml::Value::as_bool)
            != Some(false)
    {
        let parts = relative.split('/').collect::<Vec<_>>();
        let auto_name = match parts.as_slice() {
            ["src", "main.rs"] => Some(package),
            ["src", "bin", file] => file.strip_suffix(".rs"),
            ["src", "bin", name, "main.rs"] => Some(*name),
            _ => None,
        };
        if auto_name.is_some_and(|name| name.replace('-', "_") == crate_name) {
            paths.push(relative.into());
        }
    }
    ensure!(
        paths.len() == 1 && paths[0] == relative,
        "system8_source_not_declared_binary"
    );
    Ok("binary")
}
fn declared_output(
    args: &[String],
    native: &[String],
    crate_name: &str,
    kind: &str,
    package: &str,
    target: &str,
) -> Result<(String, String, String)> {
    let directory = unique(args, "--out-dir")?;
    absolute_normal(Path::new(&directory))?;
    ensure!(
        Path::new(&directory).is_dir() && Path::new(&directory).starts_with(target),
        "system8_out_dir_outside_target"
    );
    let suffix = extra_filename(args)?;
    if kind == "build_script" {
        ensure!(
            Path::new(&directory).file_name().and_then(|v| v.to_str())
                == Some(format!("{package}{suffix}").as_str()),
            "system8_build_directory_not_selected_package"
        );
    }
    let output = unique(native, "-o")?;
    absolute_normal(Path::new(&output))?;
    ensure!(
        Path::new(&output) == Path::new(&directory).join(format!("{crate_name}{suffix}")),
        "system8_native_output_not_compiler_declared"
    );
    Ok((directory, suffix, output))
}

/// Resolve the selected package from the actual source path. Cargo normally
/// invokes workspace members with the workspace cwd, not the package cwd.
fn derive_domain(
    request: &CodeNativeSystemInputRequest8,
    intent: &CodeNativeCompilerExecIntent6,
    sources_ref: &ArtifactReference,
    sources: &CodeNativeCompilationSources4,
    native: &[String],
    target_root: &str,
) -> Result<CodeNativeExecutableDomain8> {
    ensure!(
        sources.schema == "oev2.native-compilation-sources/4"
            && sources.target == "aarch64-apple-darwin"
            && sources.host_target == sources.target
            && intent.schema == "oev2.native-compiler-exec-intent/6"
            && intent.command_scope == "cargo_compiler_command"
            && intent.build_script_binding.is_none()
            && intent.execution == "before_exec",
        "system8_original_compiler_scope_changed"
    );
    absolute_normal(Path::new(&intent.cwd))?;
    let args = &intent.arguments;
    let name = unique(args, "--crate-name")?;
    ensure!(
        name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !name.as_bytes()[0].is_ascii_digit(),
        "system8_crate_name_invalid"
    );
    let kind = unique(args, "--crate-type")?;
    ensure!(
        kind == "bin" && !args.iter().any(|a| a == "--test"),
        "system8_executable_crate_type_uncovered"
    );
    ensure!(
        unique(args, "--emit")?
            .split(',')
            .filter(|v| *v == "link")
            .count()
            == 1,
        "system8_emit_link_missing"
    );
    let targets = args
        .iter()
        .filter(|v| v.as_str() == "--target" || v.starts_with("--target="))
        .count();
    ensure!(
        targets <= 1 && (targets == 0 || unique(args, "--target")? == sources.target),
        "system8_target_changed"
    );
    let source_args = args
        .iter()
        .filter(|a| !a.starts_with('-') && a.ends_with(".rs"))
        .collect::<Vec<_>>();
    ensure!(source_args.len() == 1, "system8_source_argument_ambiguous");
    let source_path = Path::new(source_args[0]);
    let source_path = if source_path.is_absolute() {
        source_path.to_owned()
    } else {
        Path::new(&intent.cwd).join(source_path)
    };
    absolute_normal(&source_path)?;
    let selected = sources
        .roots
        .iter()
        .filter(|r| r.files.iter().any(|f| Path::new(&f.path) == source_path))
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 1,
        "system8_package_source_missing_or_ambiguous"
    );
    let selected = selected[0];
    absolute_normal(Path::new(&selected.root))?;
    ensure!(
        Path::new(&selected.cargo_manifest.path) == Path::new(&selected.root).join("Cargo.toml"),
        "system8_manifest_not_selected_package_root"
    );
    for file in &selected.files {
        ensure!(
            Path::new(&file.path).starts_with(&selected.root),
            "system8_selected_inventory_escape"
        );
        crate::native_archive::verify_binding(file)?;
    }
    crate::native_archive::verify_binding(&selected.cargo_manifest)?;
    let raw = read_regular_bytes(Path::new(&selected.cargo_manifest.path), 4 * 1024 * 1024)?;
    let manifest: toml::Value = std::str::from_utf8(&raw)?.parse()?;
    ensure!(
        manifest
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            == Some(selected.package_name.as_str()),
        "system8_manifest_package_name_changed"
    );
    let relative = source_path
        .strip_prefix(&selected.root)?
        .to_str()
        .context("system8_source_encoding")?;
    let domain_kind = declared_kind(&manifest, &selected.package_name, &name, relative)?;
    let source = selected
        .files
        .iter()
        .filter(|f| Path::new(&f.path) == source_path)
        .collect::<Vec<_>>();
    ensure!(source.len() == 1, "system8_duplicate_source_file_binding");
    let (directory, suffix, output) = declared_output(
        args,
        native,
        &name,
        domain_kind,
        &selected.package_name,
        target_root,
    )?;
    let domain = CodeNativeExecutableDomain8 {
        schema: "oev2.native-executable-domain/8".into(),
        compiler_intent: request.compiler_intent.clone(),
        compilation_sources: sources_ref.clone(),
        package_id: selected.package_id.clone(),
        package_manifest: selected.cargo_manifest.clone(),
        source: (*source[0]).clone(),
        crate_name: name,
        crate_type: kind,
        domain_kind: domain_kind.into(),
        target: sources.target.clone(),
        output_directory: directory,
        extra_filename: suffix,
        original_output: output,
        execution: "not_run".into(),
    };
    domain
        .validate()
        .map_err(|e| anyhow::anyhow!("system8_domain_contract:{e:?}"))?;
    crate::native_archive::verify_binding(&domain.package_manifest)?;
    Ok(domain)
}
fn library_search_paths(
    args: &[String],
    target: &str,
    external: &[CodeNativeFileBinding],
) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(tail) = args[i].strip_prefix("-L") {
            let value = if tail.is_empty() {
                i += 1;
                args.get(i)
                    .context("system8_library_search_operand_missing")?
                    .as_str()
            } else {
                tail
            };
            let path = Path::new(value);
            absolute_normal(path)?;
            ensure!(
                path.is_dir()
                    && (path.starts_with(target)
                        || external
                            .iter()
                            .any(|f| Path::new(&f.path).parent() == Some(path))),
                "system8_library_search_path_unselected"
            );
            crate::native_sdk4::check_library_search_paths4(&[path.to_owned()])?;
            paths.push(value.into());
        }
        i += 1;
    }
    Ok(paths)
}

/// Read-only content inspection, including every original library occurrence.
/// Object/archive staging, native dispatch and both full images remain separate.
pub fn inspect_source_bound_system_inputs8(
    root: &SkillRoot,
    request: &CodeNativeSystemInputRequest8,
) -> Result<CodeNativeSystemInputAssessment8> {
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("system8_request_contract:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-system-input-request/8",
        "system8_request_schema"
    );
    let refs = [
        &request.stage_context,
        &request.compiler_intent,
        &request.received,
        &request.original_refusal,
    ];
    let values = refs
        .iter()
        .map(|r| crate::native_runtime::read(root, r))
        .collect::<Result<Vec<_>>>()?;
    let context = &values[0];
    let received = &values[2];
    let refusal = &values[3];
    ensure!(
        context["schema"] == "oev2.native-build-stage/6"
            && ["build-normalizer", "build-runner"]
                .contains(&context["parent_stage"].as_str().unwrap_or("")),
        "system8_original_stage_schema_or_role"
    );
    let side_ref: ArtifactReference = serde_json::from_value(context["side_context"].clone())?;
    let side: CodeNativeSideContext6 =
        serde_json::from_value(crate::native_runtime::read(root, &side_ref)?)?;
    side.validate()
        .map_err(|e| anyhow::anyhow!("system8_side_contract:{e:?}"))?;
    let stage = context["parent_stage"]
        .as_str()
        .context("system8_parent_stage")?;
    let folder = Path::new(&side.stage_root).join(stage);
    let received_path = root.private_path(Path::new(&request.received.path))?;
    let call = received_path.parent().context("system8_received_parent")?;
    let call_leaf = call.file_name().and_then(|v| v.to_str()).unwrap_or("");
    ensure!(
        root.private_path(Path::new(&request.stage_context.path))?
            == folder.join("stage-context.json")
            && call.parent() == Some(folder.as_path())
            && call_leaf
                .strip_prefix("call-")
                .is_some_and(|v| v.len() == 6 && v.bytes().all(|b| b.is_ascii_digit()))
            && received_path
                .file_name()
                .is_some_and(|v| v == "received.json")
            && root
                .private_path(Path::new(&request.original_refusal.path))?
                .parent()
                == Some(call)
            && received["side_context"] == serde_json::to_value(&side_ref)?
            && received["stage_context_sha256"] == request.stage_context.sha256,
        "system8_original_call_or_stage_relationship"
    );
    ensure!(
        refusal["reason"] == "native_input_implicit_library_unsupported"
            && refusal["actual_index_callbacks"] == 0
            && ["not_qualified", "original_domain_refused"]
                .contains(&refusal["execution"].as_str().unwrap_or("")),
        "system8_original_refusal_not_preserved"
    );
    let recipe_ref: ArtifactReference = serde_json::from_value(context["recipe"].clone())?;
    let recipe: CodeNativeBuildRecipe6 =
        serde_json::from_value(crate::native_runtime::read(root, &recipe_ref)?)?;
    recipe
        .validate()
        .map_err(|e| anyhow::anyhow!("system8_recipe_contract:{e:?}"))?;
    ensure!(
        side.recipe_sha256 == recipe_ref.sha256
            && received["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags
            && oe_core::sha256(recipe.raw_encoded_rustflags.as_bytes())
                == recipe.raw_encoded_rustflags_sha256,
        "system8_original_recipe_changed"
    );
    let sources_ref: ArtifactReference =
        serde_json::from_value(context["compilation_sources"].clone())?;
    let sources: CodeNativeCompilationSources4 =
        serde_json::from_value(crate::native_runtime::read(root, &sources_ref)?)?;
    sources
        .validate()
        .map_err(|e| anyhow::anyhow!("system8_sources_contract:{e:?}"))?;
    ensure!(
        sources.source_manifest_sha256 == crate::digest(&context["source_manifest"])?
            && context["source_root"] == side.build.source_root,
        "system8_original_source_manifest_changed"
    );
    let intent: CodeNativeCompilerExecIntent6 = serde_json::from_value(values[1].clone())?;
    intent
        .validate()
        .map_err(|e| anyhow::anyhow!("system8_intent_contract:{e:?}"))?;
    let policy: CodeNativeCargoLoaderPolicy6 = serde_json::from_value(
        crate::native_runtime::read(root, &recipe.cargo_loader_policy)?,
    )?;
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("system8_policy_contract:{e:?}"))?;
    ensure!(
        intent.stage_context == request.stage_context
            && received["received_cwd"] == intent.cwd
            && root.private_path(Path::new(&request.compiler_intent.path))?
                == folder
                    .join("loader-events")
                    .join(format!("compiler-{}.json", intent.pid))
            && intent.compiler == policy.compiler
            && intent.wrapper == policy.rustc_wrapper
            && intent.parent_executable == policy.cargo,
        "system8_original_compiler_parent_or_intent_changed"
    );
    let arguments: Vec<String> = serde_json::from_value(received["received_arguments"].clone())?;
    ensure!(
        arguments.len() <= 10000
            && arguments.iter().map(String::len).sum::<usize>() <= 1024 * 1024
            && arguments
                .iter()
                .filter(|v| v.as_str() == "-nodefaultlibs")
                .count()
                == 1
            && !arguments.iter().any(|v| v.starts_with('@')),
        "system8_original_vector_bounds_or_default_libraries"
    );
    let sdk = crate::native_sdk4::inspect_sdk4(root, &recipe.sdk_layout)?;
    let libraries = crate::native_sdk4::bind_selected_libraries4(&sdk, &arguments)?;
    ensure!(
        libraries
            .iter()
            .any(|v| ["-liconv", "-lcharset", "-lproc"].contains(&v.argument.as_str())),
        "system8_no_explicit_sdk_domain_extension"
    );
    let paths = library_search_paths(
        &arguments,
        &side.build.target_dir,
        &recipe.external_archive_bindings,
    )?;
    let domain = derive_domain(
        request,
        &intent,
        &sources_ref,
        &sources,
        &arguments,
        &side.build.target_dir,
    )?;
    let assessment = CodeNativeSystemInputAssessment8 { schema:"oev2.native-system-input-assessment/8".into(),
        request:request.clone(),domain,sdk_layout:recipe.sdk_layout.clone(),selected_libraries:libraries,
        original_arguments:arguments,library_search_paths:paths,inspection:"source_bound_content_only".into(),
        uncovered:vec!["Retained historical records cannot recreate a living compiler parent or renew the original owner deadline".into(),
            "Direct objects, archives and full native argv still require checked preparation and actual same-owner dispatch".into(),
            "Complete native pair/debug/signatures, SDK ABI/runtime closure, full library scope and engineering adoption remain required".into()] };
    assessment
        .validate()
        .map_err(|e| anyhow::anyhow!("system8_assessment_contract:{e:?}"))?;
    ensure!(
        crate::native_sdk4::inspect_sdk4(root, &recipe.sdk_layout)? == sdk,
        "system8_sdk_changed_after_inspection"
    );
    for (reference, value) in refs.iter().zip(&values) {
        ensure!(
            crate::native_runtime::read(root, reference)? == *value,
            "system8_original_record_changed_after_inspection"
        );
    }
    Ok(assessment)
}
pub struct BorrowedSystemInput8Inspector<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeSystemInput8InspectPort for BorrowedSystemInput8Inspector<'_> {
    fn inspect_system_inputs8(
        &self,
        request: &CodeNativeSystemInputRequest8,
    ) -> std::result::Result<CodeNativeSystemInputAssessment8, FailureRecord> {
        inspect_source_bound_system_inputs8(self.root,request).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,
            phase:"native_system_link8_content".into(),retryable:false,
            partial_result:Some(serde_json::json!({"reason":format!("{e:#}"),"execution":"not_run","whole_pair":"not_qualified"}))})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).into()).collect()
    }
    #[test]
    fn build_script_requires_manifest_source_and_enabled_build() {
        let implicit: toml::Value = "[package]\nname='owned'\n".parse().unwrap();
        assert_eq!(
            declared_kind(&implicit, "owned", "build_script_build", "build.rs").unwrap(),
            "build_script"
        );
        let disabled: toml::Value = "[package]\nname='owned'\nbuild=false\n".parse().unwrap();
        assert!(declared_kind(&disabled, "owned", "build_script_build", "build.rs").is_err());
        let custom: toml::Value = "[package]\nname='owned'\nbuild='tools/build.rs'\n"
            .parse()
            .unwrap();
        assert!(declared_kind(&custom, "owned", "build_script_build", "build.rs").is_err());
        assert!(declared_kind(&custom, "owned", "build_script_build", "tools/build.rs").is_ok());
        let escape: toml::Value = "[package]\nname='owned'\nbuild='../other/build.rs'\n"
            .parse()
            .unwrap();
        assert!(declared_kind(&escape, "owned", "build_script_build", "build.rs").is_err());
    }
    #[test]
    fn executable_name_source_and_auto_bin_rules_stay_bound() {
        let manifest: toml::Value = "[package]\nname='owned-tool'\n".parse().unwrap();
        assert_eq!(
            declared_kind(&manifest, "owned-tool", "owned_tool", "src/main.rs").unwrap(),
            "binary"
        );
        assert!(declared_kind(&manifest, "owned-tool", "wrong", "src/main.rs").is_err());
        assert!(declared_kind(&manifest, "owned-tool", "worker", "src/bin/worker.rs").is_ok());
        let manual:toml::Value="[package]\nname='owned'\nautobins=false\n[[bin]]\nname='a-tool'\npath='tools/main.rs'\n".parse().unwrap();
        assert!(declared_kind(&manual, "owned", "a_tool", "tools/main.rs").is_ok());
        assert!(declared_kind(&manual, "owned", "worker", "src/bin/worker.rs").is_err());
        assert!(declared_kind(&manual, "owned", "a_tool", "src/main.rs").is_err());
    }
    #[test]
    fn duplicate_compiler_flags_and_traversal_are_not_new_authority() {
        assert!(
            unique(
                &args(&["--crate-type=bin", "--crate-type", "bin"]),
                "--crate-type"
            )
            .is_err()
        );
        assert!(unique(&args(&["--out-dir"]), "--out-dir").is_err());
        assert!(
            extra_filename(&args(&[
                "-Cextra-filename=-0123456789abcdef",
                "-C",
                "extra-filename=-0123456789abcdef"
            ]))
            .is_err()
        );
        assert!(relative_source("../other/main.rs").is_err());
        assert!(relative_source("src//main.rs").is_err());
        assert!(absolute_normal(Path::new("/tmp/../other")).is_err());
    }
    #[test]
    fn output_is_exact_compiler_executable_under_selected_target() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let dir = root.join("owned-0123456789abcdef");
        std::fs::create_dir(&dir).unwrap();
        let compiler = args(&[
            "--out-dir",
            dir.to_str().unwrap(),
            "-C",
            "extra-filename=-0123456789abcdef",
        ]);
        let path = dir.join("build_script_build-0123456789abcdef");
        let native = args(&["-o", path.to_str().unwrap()]);
        assert!(
            declared_output(
                &compiler,
                &native,
                "build_script_build",
                "build_script",
                "owned",
                root.to_str().unwrap()
            )
            .is_ok()
        );
        assert!(
            declared_output(
                &compiler,
                &native,
                "other",
                "build_script",
                "owned",
                root.to_str().unwrap()
            )
            .is_err()
        );
        assert!(
            declared_output(
                &compiler,
                &native,
                "build_script_build",
                "build_script",
                "different",
                root.to_str().unwrap()
            )
            .is_err()
        );
        let wrong = dir.join("libbuild_script_build-0123456789abcdef.dylib");
        assert!(
            declared_output(
                &compiler,
                &args(&["-o", wrong.to_str().unwrap()]),
                "build_script_build",
                "build_script",
                "owned",
                root.to_str().unwrap()
            )
            .is_err()
        );
    }
}
