//! Shared build-script producer artifact checks used by the original metadata
//! entry and the separate compiler-probe domain. The wrapper observes and
//! rechecks the real parent. Serialized content never supplies that observation,
//! execution permission, a new budget, or a completed compiler probe.
use crate::native_buildscript6::{
    POLICY, option_value, original_build_source, output_path, same_image, sdk8_image_proof,
};
use crate::native_runtime::{read, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, no_symlinks};
use std::path::Path;

/// Reuses the original complete producer checks, including genuine SDK8 output
/// records. In-progress parallel Cargo calls cannot supply a completed image.
/// Parent PID and executable are supplied by the wrapper's actual observation;
/// this returned record is a source/producer content binding, not live authority.
pub(crate) fn inspect(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    context: &CodeNativeStageContext6,
    policy: &CodeNativeCargoLoaderPolicy6,
    source: &CodeNativeCompilationSourceRoot4,
    parent_pid: u64,
    parent: &CodeNativeFileBinding,
    sdk8: bool,
) -> Result<CodeNativeBuildScriptProducer9> {
    ensure!(
        policy.metadata_probe == POLICY && parent_pid > 0 && Path::new(&source.root) != root.path(),
        "loader6_build_script_metadata_domain_unselected"
    );
    let original = original_build_source(source)?;
    let output_path = output_path(side, source, parent, &context.parent_stage)?;
    let output = crate::native_archive::bind_file(&output_path)?;
    same_image(parent, &output)?;
    let stage = Path::new(&side.stage_root).join(&context.parent_stage);
    ensure!(
        context_path == stage.join("stage-context.json")
            && context.invocation_root
                == stage
                    .to_str()
                    .context("loader6_build_script_stage_encoding")?,
        "loader6_build_script_stage_path"
    );
    let mut dirs = std::fs::read_dir(&stage)?
        .map(|e| e.map(|v| v.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("call-"))
        })
        .collect::<Vec<_>>();
    dirs.sort();
    ensure!(dirs.len() <= 4096, "loader6_build_script_invocation_limit");
    let context_ref = reference(root, context_path)?;
    let mut found = Vec::new();
    for dir in dirs {
        no_symlinks(&dir)?;
        let file = dir.join("invocation.json");
        // Other parallel Cargo jobs may still be in progress. They cannot supply
        // the selected completed output and are checked at the final full capture.
        if !file.try_exists()? {
            continue;
        }
        let invocation_ref = reference(root, &file)?;
        let value = read(root, &invocation_ref)?;
        if value["schema"] == "oev2.native-compiler-link-invocation/7" {
            ensure!(
                if sdk8 {
                    crate::native_link8::is_adapter8(recipe)?
                } else {
                    crate::native_link7::is_adapter7(recipe)?
                },
                "compiler7_selected_adapter_required"
            );
            let library: CodeNativeCompilerLinkInvocation7 = serde_json::from_value(value)?;
            library
                .validate()
                .map_err(|e| anyhow::anyhow!("compiler7_metadata_library_contract:{e:?}"))?;
            let output = library
                .native_process
                .arguments
                .windows(2)
                .filter(|w| w[0] == "-o")
                .collect::<Vec<_>>();
            ensure!(
                output.len() == 1
                    && output[0][1] == library.input_assessment.domain.original_output
                    && ["proc-macro", "dylib", "cdylib"]
                        .contains(&library.input_assessment.domain.crate_type.as_str())
                    && Path::new(&output[0][1])
                        .extension()
                        .is_some_and(|e| e == "dylib")
                    && Path::new(&output[0][1]) != output_path,
                "compiler7_library_cannot_supply_build_script_metadata_image"
            );
            // Library records remain in the full mixed inventory and are
            // verified there; they cannot supply the required bin build script.
            continue;
        }
        if sdk8 && value["schema"] == "oev2.native-sdk-link-invocation/8" {
            if let Some(proof) = sdk8_image_proof(
                root,
                side,
                recipe,
                context,
                policy,
                source,
                &original,
                &output_path,
                &output,
                &dir,
                &invocation_ref,
                value,
            )? {
                found.push(proof);
            }
            continue;
        }
        let invocation: CodeNativeArchiveResponseInvocation = serde_json::from_value(value)?;
        let outputs = invocation
            .native_process
            .arguments
            .windows(2)
            .filter(|w| w[0] == "-o")
            .collect::<Vec<_>>();
        if outputs.len() != 1 || Path::new(&outputs[0][1]) != output_path {
            continue;
        }
        ensure!(
            invocation.native_process.status == "completed"
                && invocation.native_process.exit_code == Some(0)
                && invocation.inputs.request.caller_cwd == source.root,
            "loader6_build_script_native_not_completed_selected_source"
        );
        crate::native_link6::verify_one_invocation(
            root,
            &context.side_context,
            side,
            recipe,
            &context.parent_stage,
            &serde_json::to_value(context)?,
            &dir,
            &invocation,
            Some(Path::new(&source.root)),
        )?;
        let (linked_record, actual_output) =
            crate::native_process6::output_record(root, &invocation.native_process)?;
        ensure!(
            actual_output == output,
            "loader6_metadata_image_changed_since_actual_link"
        );
        let receipt_ref = reference(root, &dir.join("loader-receipt.json"))?;
        let receipt: CodeNativeLoaderReceipt6 = serde_json::from_value(read(root, &receipt_ref)?)?;
        receipt
            .validate()
            .map_err(|e| anyhow::anyhow!("loader6_build_script_original_loader_contract:{e:?}"))?;
        let intent: CodeNativeCompilerExecIntent6 =
            serde_json::from_value(read(root, &receipt.compiler_intent)?)?;
        intent
            .validate()
            .map_err(|e| anyhow::anyhow!("loader6_build_script_original_intent_contract:{e:?}"))?;
        let intent_path = stage
            .join("loader-events")
            .join(format!("compiler-{}.json", intent.pid));
        ensure!(
            receipt.compiler_intent == reference(root, &intent_path)?
                && intent.execution == "before_exec"
                && intent.command_scope == "cargo_compiler_command"
                && intent.build_script_binding.is_none()
                && intent.stage_context == context_ref
                && intent.parent_executable == policy.cargo
                && intent.parent_executable.path == side.build.cargo_path
                && intent.compiler == policy.compiler
                && intent.wrapper == policy.rustc_wrapper
                && intent.cwd == source.root
                && receipt.parent_pid == intent.pid
                && receipt.parent_executable == intent.compiler,
            "loader6_build_script_original_cargo_intent_changed"
        );
        ensure!(
            option_value(&intent.arguments, "--crate-name")? == "build_script_build"
                && option_value(&intent.arguments, "--crate-type")? == "bin"
                && Path::new(option_value(&intent.arguments, "--out-dir")?)
                    == output_path
                        .parent()
                        .context("loader6_build_script_out_dir")?
                && intent
                    .arguments
                    .iter()
                    .filter(|a| a.ends_with(".rs"))
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    == vec![original.path.as_str()],
            "loader6_build_script_original_source_or_role_changed"
        );
        ensure!(
            reference(root, &file)? == invocation_ref
                && reference(root, &dir.join("loader-receipt.json"))? == receipt_ref,
            "loader6_build_script_prior_records_changed"
        );
        found.push((invocation_ref, receipt.compiler_intent, linked_record));
    }
    ensure!(
        found.len() == 1,
        "loader6_build_script_prior_native_missing_or_ambiguous"
    );
    crate::native_profile::verify_file(&source.cargo_manifest)?;
    crate::native_profile::verify_file(&original)?;
    same_image(parent, &output)?;
    ensure!(
        reference(root, context_path)? == context_ref,
        "loader6_build_script_context_changed"
    );
    let (native_invocation, compiler_intent, linked_output_record) = found.remove(0);
    let binding = CodeNativeBuildScriptProducer9 {
        schema: "oev2.native-build-script-producer/9".into(),
        parent_pid,
        parent_executable: parent.clone(),
        linked_output: output,
        linked_output_record,
        build_script_source: original,
        cargo_manifest: source.cargo_manifest.clone(),
        package_id: source.package_id.clone(),
        compiler_intent,
        native_invocation,
        stage_context: context_ref,
        inspection: "same_stage_source_and_native_artifacts_only".into(),
    };
    binding
        .validate()
        .map_err(|e| anyhow::anyhow!("producer9_build_script_binding_contract:{e:?}"))?;
    Ok(binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic content fixture only. This never supplies an actual process or
    // passes the common native producer inspection.
    fn content() -> CodeNativeBuildScriptProducer9 {
        let file = |role: &str| CodeNativeFileBinding {
            path: format!("/finite-content/{role}"),
            sha256: oe_core::sha256(role.as_bytes()),
            bytes: role.len() as u64,
            mode: 0o700,
        };
        let artifact = |role: &str| ArtifactReference {
            path: format!("var/finite-content/{role}.json"),
            sha256: oe_core::sha256(role.as_bytes()),
        };
        CodeNativeBuildScriptProducer9 {
            schema: "oev2.native-build-script-producer/9".into(),
            parent_pid: 19,
            parent_executable: file("observed-parent"),
            linked_output: file("linked-output"),
            linked_output_record: artifact("actual-linked-output"),
            build_script_source: file("original-build-source"),
            cargo_manifest: file("original-manifest"),
            package_id: "finite-content@0.1.0".into(),
            compiler_intent: artifact("original-compiler-intent"),
            native_invocation: artifact("original-native-invocation"),
            stage_context: artifact("original-stage-context"),
            inspection: "same_stage_source_and_native_artifacts_only".into(),
        }
    }
    #[test]
    fn producer9_keeps_original_metadata_record_and_all_four_vectors() {
        let producer = content();
        producer.validate().unwrap();
        let original = serde_json::to_value(&producer).unwrap();
        for vector in [
            vec!["--version"],
            vec!["-vV"],
            vec!["--version", "--verbose"],
            vec!["--verbose", "--version"],
        ] {
            let arguments = vector.into_iter().map(str::to_owned).collect::<Vec<_>>();
            let value =
                crate::native_buildscript6::metadata_binding_from_producer(&producer, &arguments)
                    .unwrap();
            value.validate().unwrap();
            let mut expected = original.clone();
            expected.as_object_mut().unwrap().remove("inspection");
            expected["schema"] = serde_json::json!("oev2.native-build-script-metadata-binding/6");
            expected["arguments"] = serde_json::json!(arguments);
            expected["execution"] = serde_json::json!("metadata_only_before_exec");
            assert_eq!(serde_json::to_value(&value).unwrap(), expected);
            assert_eq!(serde_json::to_value(&producer).unwrap(), original);
        }
    }
    #[test]
    fn producer9_content_does_not_authorize_compile_or_extended_metadata() {
        let producer = content();
        for vector in [
            vec![],
            vec!["--version", "--print=sysroot"],
            vec!["--version", "extra"],
            vec!["-"],
            vec!["probe.rs", "--emit=metadata"],
            vec!["@args"],
            vec!["--print=file-names"],
            vec!["--version=verbose"],
        ] {
            let arguments = vector.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(
                crate::native_buildscript6::metadata_binding_from_producer(&producer, &arguments)
                    .is_err()
            );
        }
    }
    #[test]
    fn producer9_decoder_rejects_old_identity_missing_source_and_permission() {
        let value = serde_json::to_value(content()).unwrap();
        for field in [
            "schema",
            "parent_executable",
            "linked_output_record",
            "build_script_source",
            "cargo_manifest",
            "package_id",
            "compiler_intent",
            "native_invocation",
            "stage_context",
            "inspection",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<CodeNativeBuildScriptProducer9>(missing).is_err(),
                "{field}"
            );
        }
        let mut old = value.clone();
        old["schema"] = serde_json::json!("oev2.native-build-script-metadata-binding/6");
        let old: CodeNativeBuildScriptProducer9 = serde_json::from_value(old).unwrap();
        assert!(old.validate().is_err());
        for field in ["permission", "caller_verified", "live_parent", "execution"] {
            let mut invented = value.clone();
            invented[field] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<CodeNativeBuildScriptProducer9>(invented).is_err(),
                "{field}"
            );
        }
        let mut fake = value;
        fake["inspection"] = serde_json::json!("compiler_exec_completed");
        let fake: CodeNativeBuildScriptProducer9 = serde_json::from_value(fake).unwrap();
        assert!(fake.validate().is_err());
    }
}
