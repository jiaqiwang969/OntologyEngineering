//! Explicit SDK8 host metadata observations. Serialized records are content,
//! never living-parent, compiler, budget or registry authority.
use crate::native_runtime::{read, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, no_symlinks};
use std::path::{Component, Path};

fn validate_scope(
    value: &CodeNativeHostMetadataEnvironment8,
    target_dir: &Path,
    platform: &str,
    package: &str,
    arguments: &[String],
    selected: &CodeNativeCargoLinkerConfiguration8,
) -> Result<()> {
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("sdk8_host_metadata_contract:{e:?}"))?;
    crate::native_buildscript6::metadata(arguments)?;
    ensure!(
        value.raw_encoded_rustflags.is_empty()
            && value.host == platform
            && value.target == platform
            && platform == "aarch64-apple-darwin"
            && value.cargo_linker_environment == selected.environment,
        "sdk8_host_metadata_environment_changed"
    );
    let path = Path::new(&value.out_dir);
    ensure!(
        path.is_absolute()
            && path
                .components()
                .all(|p| matches!(p, Component::RootDir | Component::Normal(_)))
            && path.file_name().is_some_and(|n| n == "out"),
        "sdk8_host_metadata_out_dir"
    );
    let folder = path.parent().context("sdk8_host_metadata_folder")?;
    ensure!(
        folder.parent() == Some(target_dir.join("debug/build").as_path()),
        "sdk8_host_metadata_not_host_output"
    );
    let name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .context("sdk8_host_metadata_folder_encoding")?;
    let suffix = name
        .strip_prefix(&format!("{package}-"))
        .context("sdk8_host_metadata_package_output")?;
    ensure!(
        suffix.len() == 16
            && suffix
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "sdk8_host_metadata_output_fingerprint"
    );
    Ok(())
}

fn verify_record(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context: &CodeNativeStageContext6,
    source: &CodeNativeCompilationSourceRoot4,
    binding: &CodeNativeBuildScriptMetadataBinding6,
    value: &CodeNativeHostMetadataEnvironment8,
) -> Result<()> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?
            && binding.package_id == source.package_id
            && binding.cargo_manifest == source.cargo_manifest
            && binding.execution == "metadata_only_before_exec",
        "sdk8_host_metadata_selected_producer_binding"
    );
    let (configuration, selected) =
        crate::native_runtime8::verify_cargo_linker_configuration_stage(
            root,
            side,
            recipe,
            &context.parent_stage,
        )?;
    ensure!(
        value.cargo_linker_configuration == configuration,
        "sdk8_host_metadata_configuration_changed"
    );
    validate_scope(
        value,
        Path::new(&side.build.target_dir),
        &side.build.platform,
        &source.package_name,
        &binding.arguments,
        &selected,
    )?;
    let out = root.private_path(Path::new(&value.out_dir))?;
    no_symlinks(&out)?;
    ensure!(
        out.is_dir() && out.canonicalize()? == out,
        "sdk8_host_metadata_output_not_current_directory"
    );
    Ok(())
}

pub(crate) fn observe_live(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context: &CodeNativeStageContext6,
    source: &CodeNativeCompilationSourceRoot4,
    binding: &CodeNativeBuildScriptMetadataBinding6,
) -> Result<CodeNativeHostMetadataEnvironment8> {
    // Called only after the wrapper verified the actual live parent and genuine
    // same-stage native producer through inspect_current_sdk8. No input bool.
    let (configuration, _) = crate::native_runtime8::verify_cargo_linker_configuration_stage(
        root,
        side,
        recipe,
        &context.parent_stage,
    )?;
    let value = CodeNativeHostMetadataEnvironment8 {
        schema: "oev2.native-host-metadata-environment/8".into(),
        raw_encoded_rustflags: std::env::var("CARGO_ENCODED_RUSTFLAGS")?,
        host: std::env::var("HOST")?,
        target: std::env::var("TARGET")?,
        out_dir: std::env::var("OUT_DIR")?,
        cargo_linker_configuration: configuration,
        cargo_linker_environment: CodeNativeEnvironmentEntry {
            name: "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER".into(),
            value: std::env::var("CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER")?,
        },
    };
    verify_record(root, side, recipe, context, source, binding, &value)?;
    Ok(value)
}

pub(crate) fn verify_retained_intent(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context: &CodeNativeStageContext6,
    intent: &CodeNativeCompilerExecIntent6,
) -> Result<()> {
    let events = Path::new(&side.stage_root)
        .join(&context.parent_stage)
        .join("loader-events");
    let raw_path = events.join(format!("pre-environment-{}.json", intent.pid));
    let raw_ref = reference(root, &raw_path)?;
    let raw = read(root, &raw_ref)?;
    let flags = raw["actual_encoded_rustflags8"]
        .as_str()
        .context("sdk8_actual_encoded_flags_not_retained")?;
    let path = events.join(format!("host-metadata-environment8-{}.json", intent.pid));
    if flags == recipe.raw_encoded_rustflags {
        ensure!(
            !path.try_exists()?,
            "sdk8_unexpected_host_metadata_observation"
        );
        return Ok(());
    }
    ensure!(
        flags.is_empty() && intent.command_scope == "bound_build_script_metadata_probe",
        "sdk8_retained_nonmetadata_encoded_flags_changed"
    );
    let binding = intent
        .build_script_binding
        .as_ref()
        .context("sdk8_retained_host_metadata_binding")?;
    let source = crate::native_sources6::caller_source(
        root,
        side,
        &context.parent_stage,
        &serde_json::to_value(context)?,
        Path::new(&intent.cwd),
    )?;
    let record_ref = reference(root, &path)?;
    let value: CodeNativeHostMetadataEnvironment8 =
        serde_json::from_value(read(root, &record_ref)?)?;
    verify_record(root, side, recipe, context, &source, binding, &value)?;
    ensure!(
        reference(root, &raw_path)? == raw_ref && reference(root, &path)? == record_ref,
        "sdk8_host_metadata_record_changed"
    );
    Ok(())
}

pub(crate) fn retained_references(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let mut refs = Vec::new();
    for stage_name in ["build-normalizer", "build-runner"] {
        let stage = Path::new(&side.stage_root).join(stage_name);
        let context: CodeNativeStageContext6 = serde_json::from_value(read(
            root,
            &reference(root, &stage.join("stage-context.json"))?,
        )?)?;
        let events = stage.join("loader-events");
        let mut paths = std::fs::read_dir(&events)?
            .map(|e| e.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        // Every current SDK8 intent must retain its actual flags. Only a bound
        // metadata intent with empty flags may have a companion observation.
        for path in &paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with("compiler-") && name.ends_with(".json") {
                let intent: CodeNativeCompilerExecIntent6 =
                    serde_json::from_value(read(root, &reference(root, path)?)?)?;
                intent
                    .validate()
                    .map_err(|e| anyhow::anyhow!("sdk8_host_metadata_retained_intent:{e:?}"))?;
                ensure!(
                    *path == events.join(format!("compiler-{}.json", intent.pid))
                        && intent.stage_context
                            == reference(root, &stage.join("stage-context.json"))?,
                    "sdk8_host_metadata_retained_intent_identity"
                );
                verify_retained_intent(root, side, recipe, &context, &intent)?;
            }
        }
        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(pid) = name
                .strip_prefix("host-metadata-environment8-")
                .and_then(|s| s.strip_suffix(".json"))
            else {
                continue;
            };
            let pid: u64 = pid.parse()?;
            ensure!(
                pid > 0 && path == events.join(format!("host-metadata-environment8-{pid}.json")),
                "sdk8_host_metadata_filename"
            );
            let intent: CodeNativeCompilerExecIntent6 = serde_json::from_value(read(
                root,
                &reference(root, &events.join(format!("compiler-{pid}.json")))?,
            )?)?;
            ensure!(intent.pid == pid, "sdk8_host_metadata_intent_pid_changed");
            verify_retained_intent(root, side, recipe, &context, &intent)?;
            refs.push(reference(root, &path)?);
            ensure!(refs.len() <= 16384, "sdk8_host_metadata_inventory_limit");
        }
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        CodeNativeHostMetadataEnvironment8,
        CodeNativeCargoLinkerConfiguration8,
    ) {
        let file = CodeNativeFileBinding {
            path: "/selected/tool".into(),
            sha256: "a".repeat(64),
            bytes: 1,
            mode: 0o700,
        };
        let configuration = CodeNativeCargoLinkerConfiguration8 {
            schema: "oev2.native-cargo-linker-configuration/8".into(),
            target: "aarch64-apple-darwin".into(),
            cargo: file.clone(),
            linker: file,
            environment: CodeNativeEnvironmentEntry {
                name: "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER".into(),
                value: "/selected/tool".into(),
            },
        };
        let value = CodeNativeHostMetadataEnvironment8 {
            schema: "oev2.native-host-metadata-environment/8".into(),
            raw_encoded_rustflags: String::new(),
            host: configuration.target.clone(),
            target: configuration.target.clone(),
            out_dir: "/selected/target/debug/build/rustversion-0123456789abcdef/out".into(),
            cargo_linker_configuration: ArtifactReference {
                path: "var/stage/cargo-linker-configuration8.json".into(),
                sha256: "b".repeat(64),
            },
            cargo_linker_environment: configuration.environment.clone(),
        };
        (value, configuration)
    }
    fn check(
        v: &CodeNativeHostMetadataEnvironment8,
        args: &[String],
        c: &CodeNativeCargoLinkerConfiguration8,
    ) -> Result<()> {
        validate_scope(
            v,
            Path::new("/selected/target"),
            "aarch64-apple-darwin",
            "rustversion",
            args,
            c,
        )
    }
    #[test]
    fn host_metadata8_keeps_four_read_only_vectors_and_compilation_refusals() {
        let (v, c) = fixture();
        for args in [
            vec!["--version"],
            vec!["-vV"],
            vec!["--version", "--verbose"],
            vec!["--verbose", "--version"],
        ] {
            check(
                &v,
                &args.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                &c,
            )
            .unwrap();
        }
        for args in [
            vec!["--version", "--print=sysroot"],
            vec!["probe.rs", "--emit=metadata"],
            vec!["-"],
            vec!["@args"],
        ] {
            assert!(
                check(
                    &v,
                    &args.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                    &c
                )
                .is_err()
            );
        }
    }
    #[test]
    fn host_metadata8_rejects_target_output_path_flags_and_selected_linker_changes() {
        let (v, c) = fixture();
        let args = vec!["--version".into()];
        for (path, changed) in [
            (
                "/raw_encoded_rustflags",
                serde_json::json!("-C\u{1f}debuginfo=2"),
            ),
            ("/host", serde_json::json!("x86_64-apple-darwin")),
            ("/target", serde_json::json!("x86_64-apple-darwin")),
            (
                "/out_dir",
                serde_json::json!(
                    "/selected/target/aarch64-apple-darwin/debug/build/rustversion-0123456789abcdef/out"
                ),
            ),
            (
                "/out_dir",
                serde_json::json!("/selected/target/debug/build/other-0123456789abcdef/out"),
            ),
            (
                "/out_dir",
                serde_json::json!("/selected/target/debug/build/rustversion-0123456789abcdeF/out"),
            ),
            (
                "/out_dir",
                serde_json::json!(
                    "/selected/target/debug/build/../rustversion-0123456789abcdef/out"
                ),
            ),
            (
                "/cargo_linker_environment/value",
                serde_json::json!("/unselected/linker"),
            ),
        ] {
            let mut changed_value = serde_json::to_value(&v).unwrap();
            *changed_value.pointer_mut(path).unwrap() = changed;
            let changed_value = serde_json::from_value(changed_value).unwrap();
            assert!(check(&changed_value, &args, &c).is_err(), "{path}");
        }
    }
    #[test]
    fn host_metadata8_empty_flag_record_does_not_relax_original_environment_entry() {
        let (v, _) = fixture();
        v.validate().unwrap();
        assert!(
            CodeNativeEnvironmentEntry {
                name: "CARGO_ENCODED_RUSTFLAGS".into(),
                value: String::new()
            }
            .validate()
            .is_err()
        );
        let mut extra = serde_json::to_value(&v).unwrap();
        extra["permission"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CodeNativeHostMetadataEnvironment8>(extra).is_err());
        let mut missing = serde_json::to_value(v).unwrap();
        missing
            .as_object_mut()
            .unwrap()
            .remove("raw_encoded_rustflags");
        assert!(serde_json::from_value::<CodeNativeHostMetadataEnvironment8>(missing).is_err());
    }
}
