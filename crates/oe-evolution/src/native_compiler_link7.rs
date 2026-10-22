//! Source-bound complete /7 input semantics, without native execution authority.
//! The original /6 hidden-file and dynamic-library domains remain unchanged.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes};
use std::path::{Component, Path};

const DOMAIN: &str = "oev2.native-compiler-link-domain/7";
const ASSESSMENT: &str = "oev2.native-compiler-link-input-assessment/7";
const UNCOVERED: [&str; 4] = [
    "retained compiler/source relationships cannot restore a live parent or native dispatch authority",
    "source-bound input assessment is separate from a complete native /7 invocation and runtime capture",
    "original /6 hidden-file and dynamic-library refusals remain; build-script compilation is a separate domain",
    "complete pair bytes/debug/signatures, original whole library scope and engineering adoption remain required",
];

fn unique_flag(args: &[String], flag: &str) -> Result<String> {
    let mut values = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if arg == flag {
            values.push(
                args.get(i + 1)
                    .context("compiler7_flag_value_missing")?
                    .clone(),
            );
        } else if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            values.push(value.to_owned());
        }
    }
    ensure!(
        values.len() == 1 && !values[0].is_empty() && !values[0].contains('\0'),
        "compiler7_flag_missing_or_duplicate:{flag}"
    );
    Ok(values.remove(0))
}
fn codegen_value(args: &[String], name: &str) -> Result<String> {
    let mut values = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let option = if arg == "-C" {
            args.get(i + 1).map(String::as_str)
        } else {
            arg.strip_prefix("-C")
        };
        if let Some(value) = option.and_then(|a| a.strip_prefix(&format!("{name}="))) {
            values.push(value.to_owned());
        }
    }
    ensure!(
        values.len() == 1 && !values[0].contains('\0'),
        "compiler7_codegen_missing_or_duplicate:{name}"
    );
    Ok(values.remove(0))
}
fn normal_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute()
            && path
                .components()
                .all(|c| !matches!(c, Component::CurDir | Component::ParentDir))
            && !path
                .to_str()
                .context("compiler7_non_utf8_path")?
                .split('/')
                .any(|s| s == "." || s == ".."),
        "compiler7_nonabsolute_or_traversal"
    );
    no_symlinks(path)
}
fn manifest_library(
    manifest: &toml::Value,
    package_name: &str,
    crate_name: &str,
    crate_type: &str,
) -> Result<String> {
    let library = manifest
        .get("lib")
        .and_then(toml::Value::as_table)
        .context("compiler7_manifest_library_missing")?;
    let expected_name = library
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or(package_name)
        .replace('-', "_");
    ensure!(
        crate_name == expected_name,
        "compiler7_crate_name_not_selected_manifest"
    );
    if crate_type == "proc-macro" {
        ensure!(
            library.get("proc-macro").and_then(toml::Value::as_bool) == Some(true),
            "compiler7_proc_macro_not_declared_by_source"
        );
    } else {
        ensure!(
            ["dylib", "cdylib"].contains(&crate_type)
                && library.get("proc-macro").and_then(toml::Value::as_bool) != Some(true)
                && library
                    .get("crate-type")
                    .and_then(toml::Value::as_array)
                    .is_some_and(|types| types.iter().any(|v| v.as_str() == Some(crate_type))),
            "compiler7_dynamic_type_not_declared_by_source"
        );
    }
    let source = library
        .get("path")
        .and_then(toml::Value::as_str)
        .unwrap_or("src/lib.rs");
    ensure!(
        Path::new(source)
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "compiler7_manifest_source_escape"
    );
    Ok(source.into())
}
fn declared_output(
    args: &[String],
    native: &[String],
    crate_name: &str,
    target_root: &str,
) -> Result<(String, String, String)> {
    let directory = unique_flag(args, "--out-dir")?;
    normal_path(Path::new(&directory))?;
    ensure!(
        Path::new(&directory).is_dir() && Path::new(&directory).starts_with(target_root),
        "compiler7_output_directory_outside_target"
    );
    let suffix = codegen_value(args, "extra-filename")?;
    ensure!(
        suffix.starts_with('-')
            && suffix.len() == 17
            && suffix.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),
        "compiler7_extra_filename_uncovered"
    );
    let output = unique_flag(native, "-o")?;
    normal_path(Path::new(&output))?;
    ensure!(
        Path::new(&output) == Path::new(&directory).join(format!("lib{crate_name}{suffix}.dylib")),
        "compiler7_native_output_not_compiler_declared_image"
    );
    Ok((directory, suffix, output))
}
fn derive_domain(
    content: &CodeNativeCompilerInputContentObservation7,
    intent: &CodeNativeCompilerExecIntent6,
    sources_ref: &ArtifactReference,
    sources: &CodeNativeCompilationSources4,
) -> Result<CodeNativeCompilerLinkDomain7> {
    let caller = &content.base_inputs.request.caller_cwd;
    ensure!(
        intent.cwd == *caller
            && sources.schema == "oev2.native-compilation-sources/4"
            && sources.target == "aarch64-apple-darwin"
            && sources.host_target == sources.target,
        "compiler7_selected_caller_or_target_changed"
    );
    let selected = sources
        .roots
        .iter()
        .filter(|s| s.root == *caller)
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 1,
        "compiler7_source_root_missing_or_duplicate"
    );
    let selected = selected[0];
    let args = &intent.arguments;
    let crate_name = unique_flag(args, "--crate-name")?;
    ensure!(
        !crate_name.is_empty()
            && crate_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !crate_name.as_bytes()[0].is_ascii_digit(),
        "compiler7_crate_name_invalid"
    );
    let crate_type = unique_flag(args, "--crate-type")?;
    ensure!(
        ["proc-macro", "dylib", "cdylib"].contains(&crate_type.as_str()),
        "compiler7_crate_type_uncovered"
    );
    ensure!(
        unique_flag(args, "--emit")?
            .split(',')
            .filter(|s| *s == "link")
            .count()
            == 1,
        "compiler7_emit_link_missing"
    );
    let target_count = args
        .iter()
        .filter(|a| a.as_str() == "--target" || a.starts_with("--target="))
        .count();
    ensure!(
        target_count <= 1
            && (target_count == 0 || unique_flag(args, "--target")? == sources.target),
        "compiler7_compiler_target_changed"
    );
    let package_manifest = selected.cargo_manifest.clone();
    ensure!(
        Path::new(&package_manifest.path) == Path::new(caller).join("Cargo.toml"),
        "compiler7_manifest_not_selected_root"
    );
    crate::native_archive::verify_binding(&package_manifest)?;
    let manifest_raw = read_regular_bytes(Path::new(&package_manifest.path), 4 * 1024 * 1024)?;
    let manifest: toml::Value = std::str::from_utf8(&manifest_raw)?.parse()?;
    let source_rel = manifest_library(&manifest, &selected.package_name, &crate_name, &crate_type)?;
    let expected_source = Path::new(caller).join(source_rel);
    normal_path(&expected_source)?;
    let source_args = args
        .iter()
        .filter(|a| !a.starts_with('-') && a.ends_with(".rs"))
        .collect::<Vec<_>>();
    ensure!(
        source_args.len() == 1,
        "compiler7_source_argument_missing_or_ambiguous"
    );
    let received_source = Path::new(source_args[0]);
    let received_source = if received_source.is_absolute() {
        received_source.to_owned()
    } else {
        Path::new(caller).join(received_source)
    };
    normal_path(&received_source)?;
    ensure!(
        received_source == expected_source,
        "compiler7_source_not_selected_library"
    );
    let source = selected
        .files
        .iter()
        .filter(|f| Path::new(&f.path) == expected_source)
        .collect::<Vec<_>>();
    ensure!(
        source.len() == 1,
        "compiler7_source_file_not_selected_inventory"
    );
    let source = (*source[0]).clone();
    crate::native_archive::verify_binding(&source)?;
    let (output_directory, extra_filename, output) = declared_output(
        args,
        &content.file_inputs.request.received_arguments,
        &crate_name,
        &content.file_inputs.request.target_root,
    )?;
    let dynamic = content
        .file_inputs
        .base_arguments
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "-dynamiclib")
        .collect::<Vec<_>>();
    ensure!(
        dynamic.len() == 1,
        "compiler7_dynamic_flag_missing_or_duplicate"
    );
    let domain = CodeNativeCompilerLinkDomain7 {
        schema: DOMAIN.into(),
        compiler_intent: content.compiler_intent.clone(),
        compilation_sources: sources_ref.clone(),
        package_id: selected.package_id.clone(),
        package_manifest,
        source,
        crate_name,
        crate_type,
        target: sources.target.clone(),
        output_directory,
        extra_filename,
        original_output: output,
        dynamic_library_base_ordinal: dynamic[0].0 as u64,
        execution: "not_run".into(),
    };
    domain
        .validate()
        .map_err(|e| anyhow::anyhow!("compiler7_domain_contract:{e:?}"))?;
    crate::native_archive::verify_binding(&domain.package_manifest)?;
    Ok(domain)
}
fn complete_arguments(content: &CodeNativeCompilerInputContentObservation7) -> Result<Vec<String>> {
    let files = &content.file_inputs;
    let base = &content.base_inputs;
    ensure!(
        base.request.received_arguments == files.base_arguments
            && base.expanded_arguments == files.base_arguments
            && base.executed_arguments.len() == files.base_argument_ordinals.len(),
        "compiler7_base_projection_changed"
    );
    let base_args = crate::native_link6::execution_arguments(base)?;
    let mut result = files.request.received_arguments.clone();
    for (ordinal, arg) in files.base_argument_ordinals.iter().zip(base_args) {
        *result
            .get_mut(usize::try_from(*ordinal)?)
            .context("compiler7_original_ordinal_missing")? = arg;
    }
    for file in &files.files {
        let path = format!("generated-files7/{}", file.logical_path);
        let value = if file.option_spelling.starts_with("-Wl,") {
            format!("-Wl,{path}")
        } else {
            path
        };
        *result
            .get_mut(usize::try_from(file.value_argument_ordinal)?)
            .context("compiler7_file_ordinal_missing")? = value;
    }
    Ok(result)
}

/// Full retained inspection of the /7 outer input record. Compiler, process and
/// source documents remain their original identities; this never calls clang,
/// reconstructs a living PID, allocates a budget or selects a code version.
pub fn inspect_retained_complete7(
    root: &SkillRoot,
    input: &ArtifactReference,
) -> Result<CodeNativeCompilerLinkInputAssessment7> {
    let value = crate::native_runtime::read(root, input)?;
    let content: CodeNativeCompilerInputContentObservation7 = serde_json::from_value(value)?;
    let context = crate::native_runtime::read(root, &content.stage_context)?;
    let side: CodeNativeSideContext6 =
        serde_json::from_value(crate::native_runtime::read(root, &content.side_context)?)?;
    side.validate()
        .map_err(|e| anyhow::anyhow!("compiler7_side_contract:{e:?}"))?;
    let recipe_ref: ArtifactReference = serde_json::from_value(context["recipe"].clone())?;
    let recipe: CodeNativeBuildRecipe6 =
        serde_json::from_value(crate::native_runtime::read(root, &recipe_ref)?)?;
    recipe
        .validate()
        .map_err(|e| anyhow::anyhow!("compiler7_recipe_contract:{e:?}"))?;
    let stage = context["parent_stage"]
        .as_str()
        .context("compiler7_stage_missing")?;
    ensure!(
        context["schema"] == "oev2.native-build-stage/6"
            && ["build-normalizer", "build-runner"].contains(&stage)
            && context["side_context"] == serde_json::to_value(&content.side_context)?
            && recipe_ref.sha256 == side.recipe_sha256
            && recipe.raw_encoded_rustflags_sha256
                == oe_core::sha256(recipe.raw_encoded_rustflags.as_bytes()),
        "compiler7_original_stage_recipe_relationship_changed"
    );
    let sources_ref: ArtifactReference =
        serde_json::from_value(context["compilation_sources"].clone())?;
    let sources: CodeNativeCompilationSources4 =
        serde_json::from_value(crate::native_runtime::read(root, &sources_ref)?)?;
    sources
        .validate()
        .map_err(|e| anyhow::anyhow!("compiler7_sources_contract:{e:?}"))?;
    ensure!(
        sources.source_manifest_sha256 == crate::digest(&context["source_manifest"])?
            && content.file_inputs.request.target_root == side.build.target_dir,
        "compiler7_original_source_manifest_changed"
    );
    let intent: CodeNativeCompilerExecIntent6 =
        serde_json::from_value(crate::native_runtime::read(root, &content.compiler_intent)?)?;
    let domain = derive_domain(&content, &intent, &sources_ref, &sources)?;
    let caller = Path::new(&content.base_inputs.request.caller_cwd);
    let approved = if caller == root.path() {
        None
    } else {
        Some(caller)
    };
    crate::native_input_content7::verify_retained(root, &content, approved, &mut |p| {
        crate::native_process6::verify(
            root,
            p,
            &side,
            &recipe,
            Path::new(&content.stage_context.path),
        )
    })?;
    let call = root
        .private_path(Path::new(&content.received.path))?
        .parent()
        .context("compiler7_received_parent")?
        .to_owned();
    let received = crate::native_runtime::read(root, &content.received)?;
    ensure!(
        *input
            == crate::native_runtime::reference(root, &call.join("compiler-input-content7.json"))?
            && content.base_inputs.request.algorithm == recipe.algorithm
            && content.base_inputs.request.external_archives == recipe.external_archive_bindings
            && content.base_inputs.request.index_executable
                == *crate::native_runtime6::role(&recipe, "archive_reader")?
            && received["raw_encoded_rustflags"] == recipe.raw_encoded_rustflags,
        "compiler7_original_preparation_or_recipe_changed"
    );
    let original_base_scope_refusal =
        crate::native_link6::check_native_scope(root, &content.base_inputs, &recipe)
            .err()
            .context("compiler7_original_native6_scope_must_refuse_dynamic_library")?;
    let original_base_scope_refusal = format!("{original_base_scope_refusal:#}");
    ensure!(
        original_base_scope_refusal
            .contains("native6_received_flag_or_input_unsupported:-dynamiclib"),
        "compiler7_original_base_scope_failure_not_dynamic_library"
    );
    crate::native_link6::check_complete_base_scope7(
        root,
        &content.base_inputs,
        &recipe,
        usize::try_from(domain.dynamic_library_base_ordinal)?,
    )?;
    let assessment = CodeNativeCompilerLinkInputAssessment7 {
        schema: ASSESSMENT.into(),
        content_observation: input.clone(),
        domain,
        complete_arguments: complete_arguments(&content)?,
        original_base_scope_refusal,
        retained_file_count: content.file_inputs.files.len() as u64,
        inspection: "source_bound_content_only".into(),
        uncovered: UNCOVERED.iter().map(|s| (*s).into()).collect(),
    };
    assessment
        .validate()
        .map_err(|e| anyhow::anyhow!("compiler7_assessment_contract:{e:?}"))?;
    ensure!(
        crate::native_runtime::read(root, input)? == serde_json::to_value(&content)?
            && crate::native_runtime::read(root, &content.stage_context)? == context,
        "compiler7_original_content_changed_after_inspection"
    );
    Ok(assessment)
}

pub struct BorrowedCompilerLinkInput7Inspector<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeCompilerLinkInput7InspectPort for BorrowedCompilerLinkInput7Inspector<'_> {
    fn inspect_complete_compiler_inputs7(
        &self,
        input: &ArtifactReference,
    ) -> std::result::Result<CodeNativeCompilerLinkInputAssessment7, FailureRecord> {
        inspect_retained_complete7(self.root,input).map_err(|e| FailureRecord {
            code:ErrorCode::InvalidContract,phase:"native_compiler_link_input7".into(),retryable:false,
            partial_result:Some(serde_json::json!({"reason":format!("{e:#}"),"execution":"not_run","whole_pair":"not_qualified"})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    #[test]
    fn duplicate_equals_and_split_rustc_flags_do_not_define_one_domain() {
        assert_eq!(
            unique_flag(&args(&["--crate-type=proc-macro"]), "--crate-type").unwrap(),
            "proc-macro"
        );
        assert!(
            unique_flag(
                &args(&["--crate-type", "proc-macro", "--crate-type=dylib"]),
                "--crate-type"
            )
            .is_err()
        );
        assert!(unique_flag(&args(&["--out-dir"]), "--out-dir").is_err());
        assert!(unique_flag(&args(&["--crate-name="]), "--crate-name").is_err());
    }
    #[test]
    fn duplicate_codegen_or_traversal_is_not_silent_output_authority() {
        assert_eq!(
            codegen_value(
                &args(&["-C", "extra-filename=-0123456789abcdef"]),
                "extra-filename"
            )
            .unwrap(),
            "-0123456789abcdef"
        );
        assert!(
            codegen_value(
                &args(&["-C", "extra-filename=-a", "-Cextra-filename=-b"]),
                "extra-filename"
            )
            .is_err()
        );
        assert!(normal_path(Path::new("/tmp/../escape")).is_err());
        assert!(normal_path(Path::new("relative/lib.rs")).is_err());
    }
    #[test]
    fn compiler_library_type_must_be_declared_by_the_selected_manifest() {
        let macro_manifest: toml::Value = "[lib]\nproc-macro=true\n".parse().unwrap();
        assert_eq!(
            manifest_library(&macro_manifest, "the-macro", "the_macro", "proc-macro").unwrap(),
            "src/lib.rs"
        );
        assert!(manifest_library(&macro_manifest, "the-macro", "the_macro", "dylib").is_err());
        assert!(
            manifest_library(&macro_manifest, "the-macro", "other_macro", "proc-macro").is_err()
        );
        let shared: toml::Value = "[lib]\ncrate-type=['cdylib']\n".parse().unwrap();
        assert!(manifest_library(&shared, "shared", "shared", "cdylib").is_ok());
        assert!(manifest_library(&shared, "shared", "shared", "dylib").is_err());
        assert!(manifest_library(&shared, "shared", "shared", "proc-macro").is_err());
    }
    #[test]
    fn a_manifest_cannot_bind_a_library_source_outside_its_package() {
        let escaped: toml::Value = "[lib]\nproc-macro=true\npath='../another/lib.rs'\n"
            .parse()
            .unwrap();
        assert!(manifest_library(&escaped, "owned", "owned", "proc-macro").is_err());
    }
    #[test]
    fn native_output_must_equal_the_compiler_image_in_the_selected_target() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let out = root.join("deps");
        std::fs::create_dir(&out).unwrap();
        let compiler = args(&[
            "--out-dir",
            out.to_str().unwrap(),
            "-C",
            "extra-filename=-0123456789abcdef",
        ]);
        let image = out.join("libowned-0123456789abcdef.dylib");
        let native = args(&["-o", image.to_str().unwrap()]);
        assert!(declared_output(&compiler, &native, "owned", root.to_str().unwrap()).is_ok());
        assert!(declared_output(&compiler, &native, "other", root.to_str().unwrap()).is_err());
        let outside = root.parent().unwrap().join("outside.dylib");
        assert!(
            declared_output(
                &compiler,
                &args(&["-o", outside.to_str().unwrap()]),
                "owned",
                root.to_str().unwrap()
            )
            .is_err()
        );
        let mut duplicate = native.clone();
        duplicate.extend(native);
        assert!(declared_output(&compiler, &duplicate, "owned", root.to_str().unwrap()).is_err());
    }
}
