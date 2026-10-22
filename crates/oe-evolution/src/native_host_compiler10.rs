//! Explicit actual host compilation conditions. Pure content is insufficient:
//! the wrapper separately verifies a living parent and genuine SDK8 producer.
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, no_symlinks};
use std::path::{Component, Path};

fn validate_scope(
    value: &CodeNativeHostCompilerEnvironment10,
    target_dir: &Path,
    platform: &str,
    package: &str,
    package_id: &str,
    cargo_manifest: &CodeNativeFileBinding,
    producer: &CodeNativeBuildScriptProducer9,
    configuration: &ArtifactReference,
    selected: &CodeNativeCargoLinkerConfiguration8,
) -> Result<()> {
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("probe10_host_environment_contract:{e:?}"))?;
    ensure!(
        value.raw_encoded_rustflags.is_empty()
            && platform == "aarch64-apple-darwin"
            && value.host == platform
            && value.target == platform
            && selected.target == platform
            && selected.environment.name == "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER"
            && selected.environment.value == selected.linker.path
            && value.cargo_linker_configuration == *configuration
            && value.cargo_linker_environment == selected.environment,
        "probe10_host_environment_or_configuration_changed"
    );
    ensure!(
        value.producer == *producer
            && producer.package_id == package_id
            && producer.cargo_manifest == *cargo_manifest
            && producer.parent_pid > 0
            && producer.parent_executable.path != producer.linked_output.path
            && producer.parent_executable.sha256 == producer.linked_output.sha256
            && producer.parent_executable.bytes == producer.linked_output.bytes
            && producer.parent_executable.mode == producer.linked_output.mode
            && producer.parent_executable.mode & 0o111 != 0
            && producer.inspection == "same_stage_source_and_native_artifacts_only",
        "probe10_host_original_producer_content_changed"
    );
    let out = Path::new(&value.out_dir);
    ensure!(
        out.is_absolute()
            && !value.out_dir.contains('\\')
            && out
                .components()
                .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
            && out.file_name().is_some_and(|name| name == "out"),
        "probe10_host_output_path"
    );
    let folder = out.parent().context("probe10_host_output_folder")?;
    ensure!(
        folder.parent() == Some(target_dir.join("debug/build").as_path()),
        "probe10_host_output_not_unplatformed_current_stage"
    );
    let name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .context("probe10_host_output_encoding")?;
    let fingerprint = name
        .strip_prefix(&format!("{package}-"))
        .context("probe10_host_output_package")?;
    ensure!(
        fingerprint.len() == 16
            && fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "probe10_host_output_fingerprint"
    );
    Ok(())
}

pub(crate) fn verify_record(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context: &CodeNativeStageContext6,
    source: &CodeNativeCompilationSourceRoot4,
    producer: &CodeNativeBuildScriptProducer9,
    value: &CodeNativeHostCompilerEnvironment10,
) -> Result<()> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?
            && producer.stage_context
                == crate::native_runtime::reference(
                    root,
                    &Path::new(&side.stage_root)
                        .join(&context.parent_stage)
                        .join("stage-context.json")
                )?,
        "probe10_host_current_source_or_stage_changed"
    );
    let (configuration, selected) =
        crate::native_runtime8::verify_cargo_linker_configuration_stage(
            root,
            side,
            recipe,
            &context.parent_stage,
        )?;
    validate_scope(
        value,
        Path::new(&side.build.target_dir),
        &side.build.platform,
        &source.package_name,
        &source.package_id,
        &source.cargo_manifest,
        producer,
        &configuration,
        &selected,
    )?;
    let out = root.private_path(Path::new(&value.out_dir))?;
    no_symlinks(&out)?;
    ensure!(
        out.is_dir() && out.canonicalize()? == out,
        "probe10_host_output_not_current_directory"
    );
    Ok(())
}

/// Called after the current wrapper's original fully checked stage and genuine
/// same-stage producer inspection. Retained JSON never reaches the exec entry.
pub(crate) fn observe_live(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context: &CodeNativeStageContext6,
    source: &CodeNativeCompilationSourceRoot4,
    producer: &CodeNativeBuildScriptProducer9,
) -> Result<CodeNativeHostCompilerEnvironment10> {
    let (configuration, _) = crate::native_runtime8::verify_cargo_linker_configuration_stage(
        root,
        side,
        recipe,
        &context.parent_stage,
    )?;
    let value = CodeNativeHostCompilerEnvironment10 {
        schema: "oev2.native-host-compiler-environment/10".into(),
        raw_encoded_rustflags: std::env::var("CARGO_ENCODED_RUSTFLAGS")?,
        host: std::env::var("HOST")?,
        target: std::env::var("TARGET")?,
        out_dir: std::env::var("OUT_DIR")?,
        cargo_linker_configuration: configuration,
        cargo_linker_environment: CodeNativeEnvironmentEntry {
            name: "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER".into(),
            value: std::env::var("CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER")?,
        },
        producer: producer.clone(),
    };
    verify_record(root, side, recipe, context, source, producer, &value)?;
    Ok(value)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn fixture() -> (
        CodeNativeHostCompilerEnvironment10,
        CodeNativeCargoLinkerConfiguration8,
    ) {
        let reference = ArtifactReference {
            path: "var/current-stage.json".into(),
            sha256: "b".repeat(64),
        };
        let file = CodeNativeFileBinding {
            path: "/selected/producer".into(),
            sha256: "a".repeat(64),
            bytes: 1,
            mode: 0o700,
        };
        let manifest = CodeNativeFileBinding {
            path: "/selected/source/Cargo.toml".into(),
            sha256: "c".repeat(64),
            bytes: 2,
            mode: 0o444,
        };
        let mut linked = file.clone();
        linked.path = "/selected/build_script_build-0123456789abcdef".into();
        let producer = CodeNativeBuildScriptProducer9 {
            schema: "oev2.native-build-script-producer/9".into(),
            parent_pid: 20,
            parent_executable: file.clone(),
            linked_output: linked,
            build_script_source: file.clone(),
            cargo_manifest: manifest,
            package_id: "proc-macro2@1.0.103".into(),
            compiler_intent: reference.clone(),
            native_invocation: reference.clone(),
            stage_context: reference.clone(),
            linked_output_record: reference.clone(),
            inspection: "same_stage_source_and_native_artifacts_only".into(),
        };
        let selected = CodeNativeCargoLinkerConfiguration8 {
            schema: "oev2.native-cargo-linker-configuration/8".into(),
            target: "aarch64-apple-darwin".into(),
            cargo: file.clone(),
            linker: file.clone(),
            environment: CodeNativeEnvironmentEntry {
                name: "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER".into(),
                value: file.path,
            },
        };
        let value = CodeNativeHostCompilerEnvironment10 {
            schema: "oev2.native-host-compiler-environment/10".into(),
            raw_encoded_rustflags: String::new(),
            host: selected.target.clone(),
            target: selected.target.clone(),
            out_dir: "/selected/target/debug/build/proc-macro2-0123456789abcdef/out".into(),
            cargo_linker_configuration: reference,
            cargo_linker_environment: selected.environment.clone(),
            producer,
        };
        (value, selected)
    }
    fn check(
        v: &CodeNativeHostCompilerEnvironment10,
        original: &CodeNativeHostCompilerEnvironment10,
        selected: &CodeNativeCargoLinkerConfiguration8,
    ) -> Result<()> {
        validate_scope(
            v,
            Path::new("/selected/target"),
            "aarch64-apple-darwin",
            "proc-macro2",
            &original.producer.package_id,
            &original.producer.cargo_manifest,
            &original.producer,
            &original.cargo_linker_configuration,
            selected,
        )
    }
    #[test]
    fn host10_rejects_wrong_source_producer_linker_host_target_and_out_dir() {
        let (value, selected) = fixture();
        check(&value, &value, &selected).unwrap();
        for (path, changed) in [
            (
                "/raw_encoded_rustflags",
                serde_json::json!("-C\u{1f}linker=/selected/producer"),
            ),
            ("/host", serde_json::json!("x86_64-apple-darwin")),
            ("/target", serde_json::json!("x86_64-apple-darwin")),
            (
                "/out_dir",
                serde_json::json!(
                    "/selected/target/aarch64-apple-darwin/debug/build/proc-macro2-0123456789abcdef/out"
                ),
            ),
            (
                "/out_dir",
                serde_json::json!("/selected/target/debug/build/other-0123456789abcdef/out"),
            ),
            (
                "/out_dir",
                serde_json::json!("/selected/target/debug/build/proc-macro2-0123456789abcdeF/out"),
            ),
            (
                "/out_dir",
                serde_json::json!(
                    "/selected/target/debug/build/../proc-macro2-0123456789abcdef/out"
                ),
            ),
            (
                "/cargo_linker_environment/value",
                serde_json::json!("/foreign/linker"),
            ),
            (
                "/cargo_linker_configuration/sha256",
                serde_json::json!("d".repeat(64)),
            ),
            ("/producer/parent_pid", serde_json::json!(0)),
            ("/producer/package_id", serde_json::json!("foreign@1")),
            (
                "/producer/cargo_manifest/sha256",
                serde_json::json!("d".repeat(64)),
            ),
            (
                "/producer/stage_context/sha256",
                serde_json::json!("d".repeat(64)),
            ),
            (
                "/producer/native_invocation/sha256",
                serde_json::json!("d".repeat(64)),
            ),
        ] {
            let mut changed_value = serde_json::to_value(&value).unwrap();
            *changed_value.pointer_mut(path).unwrap() = changed;
            let changed_value = serde_json::from_value(changed_value).unwrap();
            assert!(check(&changed_value, &value, &selected).is_err(), "{path}");
        }
    }
    #[test]
    fn host10_missing_empty_and_permission_are_distinct() {
        let (value, _) = fixture();
        value.validate().unwrap();
        assert!(
            CodeNativeEnvironmentEntry {
                name: "CARGO_ENCODED_RUSTFLAGS".into(),
                value: String::new()
            }
            .validate()
            .is_err()
        );
        let mut missing = serde_json::to_value(&value).unwrap();
        missing
            .as_object_mut()
            .unwrap()
            .remove("raw_encoded_rustflags");
        assert!(serde_json::from_value::<CodeNativeHostCompilerEnvironment10>(missing).is_err());
        let mut authority = serde_json::to_value(&value).unwrap();
        authority["permission"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CodeNativeHostCompilerEnvironment10>(authority).is_err());
        let mut old = serde_json::to_value(value).unwrap();
        old["schema"] = serde_json::json!("oev2.native-host-metadata-environment/8");
        assert!(
            serde_json::from_value::<CodeNativeHostCompilerEnvironment10>(old)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
