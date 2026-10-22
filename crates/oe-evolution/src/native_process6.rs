//! Explicit native6 PROCESS CONTENT profile. Retained JSON can demonstrate
//! bytes/bindings; it never recreates a live parent, dispatch or owner capability.
use crate::native_runtime::{read, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, write_new};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SCHEMA: &str = "oev2.native-process-environment-binding/6";
const NAMES: [&str; 2] = ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"];
/// Not Deserialize, Clone or caller-constructible: one immediate content check.
/// This is a verifier result, not a live execution or owner capability.
pub(crate) struct CheckedProcess6 {
    process: ArtifactReference,
    removed: Vec<CodeNativeEnvironmentEntry>,
}
impl CheckedProcess6 {
    pub(crate) fn process(&self) -> &ArtifactReference {
        &self.process
    }
    pub(crate) fn removed_keys(&self) -> Value {
        json!(self.removed.iter().map(|e| &e.name).collect::<Vec<_>>())
    }
}
fn binding_path(root: &SkillRoot, e: &CodeNativeInputProcessEvidence) -> Result<PathBuf> {
    let path = root.private_path(Path::new(&e.process.path))?;
    ensure!(
        path.file_name().and_then(|p| p.to_str()) == Some("process.json"),
        "native6_process_path_not_original_report"
    );
    Ok(path
        .parent()
        .context("native6_process_parent")?
        .join("environment-binding6.json"))
}
pub(crate) fn binding_reference(
    root: &SkillRoot,
    e: &CodeNativeInputProcessEvidence,
) -> Result<ArtifactReference> {
    reference(root, &binding_path(root, e)?)
}
fn removals(environment: &[CodeNativeEnvironmentEntry]) -> Result<Vec<CodeNativeEnvironmentEntry>> {
    let rows = NAMES
        .into_iter()
        .map(|name| {
            let all = environment
                .iter()
                .filter(|e| e.name == name)
                .collect::<Vec<_>>();
            ensure!(
                all.len() == 1,
                "native6_process_selected_removal_missing_duplicate"
            );
            Ok(all[0].clone())
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        rows[1].value == "1",
        "native6_process_diagnostic_value_changed"
    );
    Ok(rows)
}
/// Output is read only after this owned actual process has returned. A retained
/// pin is compared with current bytes; later equal files cannot replace it.
fn returned_output(
    root: &SkillRoot,
    e: &CodeNativeInputProcessEvidence,
) -> Result<Option<CodeNativeFileBinding>> {
    if e.role != "native_link" || e.status != "completed" || e.exit_code != Some(0) {
        return Ok(None);
    }
    let output = e
        .arguments
        .windows(2)
        .filter(|w| w[0] == "-o")
        .collect::<Vec<_>>();
    ensure!(output.len() == 1, "native6_linked_output_missing_duplicate");
    let path = Path::new(&output[0][1]);
    let approved = root.private_path(path)?;
    ensure!(
        path.is_absolute() && approved == path && path.canonicalize()? == path,
        "native6_linked_output_not_original_private_path"
    );
    Ok(Some(crate::native_archive::bind_file(path)?))
}
pub(crate) fn output_record(
    root: &SkillRoot,
    e: &CodeNativeInputProcessEvidence,
) -> Result<(ArtifactReference, CodeNativeFileBinding)> {
    let r = reference(root, &binding_path(root, e)?)?;
    let b: CodeNativeProcessEnvironmentBinding6 = serde_json::from_value(read(root, &r)?)?;
    b.validate()
        .map_err(|e| anyhow::anyhow!("native6_output_binding_contract:{e:?}"))?;
    ensure!(
        b.process == e.process
            && e.role == "native_link"
            && e.status == "completed"
            && e.exit_code == Some(0),
        "native6_output_not_original_completed_link"
    );
    let output = b
        .linked_output
        .context("native6_actual_producer_output_pin_missing")?;
    ensure!(
        Some(&output) == returned_output(root, e)?.as_ref(),
        "native6_actual_producer_output_changed"
    );
    ensure!(
        reference(root, &binding_path(root, e)?)? == r,
        "native6_output_record_changed"
    );
    Ok((r, output))
}
/// Called only with the actual live-parent result, after the same owned shared
/// process returned and wrote its full raw report. Failed processes are retained.
pub(crate) fn retain(
    root: &SkillRoot,
    e: &CodeNativeInputProcessEvidence,
    parent: &crate::native_loader6::ActualCompilerParent,
) -> Result<()> {
    let intent: CodeNativeCompilerExecIntent6 =
        serde_json::from_value(read(root, &parent.intent)?)?;
    ensure!(
        intent.pid == parent.pid && intent.compiler == parent.executable,
        "native6_process_live_parent_binding_changed"
    );
    let b = CodeNativeProcessEnvironmentBinding6 {
        schema: SCHEMA.into(),
        process: e.process.clone(),
        compiler_intent: parent.intent.clone(),
        stage_context: intent.stage_context,
        removed_environment: removals(&parent.environment)?,
        sdk_root: parent.sdk_root.clone(),
        linked_output: returned_output(root, e)?,
    };
    b.validate()
        .map_err(|x| anyhow::anyhow!("native6_process_environment_contract:{x:?}"))?;
    write_new(&binding_path(root, e)?, &serde_json::to_value(b)?)
}
fn fields(
    b: &CodeNativeProcessEnvironmentBinding6,
    e: &CodeNativeInputProcessEvidence,
    intent: &CodeNativeCompilerExecIntent6,
    context: &ArtifactReference,
    expected_removed: &[CodeNativeEnvironmentEntry],
    expected_sdk: &Option<CodeNativeCompilerSdkRootBinding6>,
) -> Result<()> {
    b.validate()
        .map_err(|x| anyhow::anyhow!("native6_process_environment_contract:{x:?}"))?;
    ensure!(
        b.schema == SCHEMA
            && b.process == e.process
            && b.stage_context == *context
            && intent.stage_context == *context,
        "native6_process_environment_report_or_stage_changed"
    );
    ensure!(
        b.removed_environment == expected_removed && b.sdk_root == *expected_sdk,
        "native6_process_selected_environment_changed"
    );
    Ok(())
}
pub(crate) fn verify(
    root: &SkillRoot,
    e: &CodeNativeInputProcessEvidence,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
) -> Result<()> {
    let binding_ref = reference(root, &binding_path(root, e)?)?;
    let b: CodeNativeProcessEnvironmentBinding6 =
        serde_json::from_value(read(root, &binding_ref)?)?;
    let context = reference(root, context_path)?;
    let intent: CodeNativeCompilerExecIntent6 =
        serde_json::from_value(read(root, &b.compiler_intent)?)?;
    intent
        .validate()
        .map_err(|x| anyhow::anyhow!("native6_process_intent:{x:?}"))?;
    let policy = crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?;
    ensure!(
        intent.schema == "oev2.native-compiler-exec-intent/6"
            && intent.execution == "before_exec"
            && intent.command_scope == "cargo_compiler_command"
            && intent.build_script_binding.is_none()
            && intent.compiler == policy.compiler
            && intent.wrapper == policy.rustc_wrapper
            && intent.parent_executable == policy.cargo
            && intent.parent_executable.path == side.build.cargo_path,
        "native6_process_selected_compiler_intent_changed"
    );
    let fallback = intent
        .environment
        .iter()
        .filter(|e| e.name == NAMES[0])
        .collect::<Vec<_>>();
    ensure!(
        fallback.len() == 1,
        "native6_process_original_fallback_missing_duplicate"
    );
    crate::native_loader6::validate_fallback(&fallback[0].value, side, &policy)?;
    let expected = vec![
        fallback[0].clone(),
        CodeNativeEnvironmentEntry {
            name: NAMES[1].into(),
            value: "1".into(),
        },
    ];
    let sdk = crate::native_sdkroot6::inspect_actual(
        root,
        recipe,
        &policy,
        b.sdk_root
            .as_ref()
            .map(|x| std::ffi::OsStr::new(&x.raw_value)),
    )?;
    fields(&b, e, &intent, &context, &expected, &sdk)?;
    ensure!(
        b.linked_output == returned_output(root, e)?,
        "native6_actual_producer_output_changed"
    );
    let checked = CheckedProcess6 {
        process: e.process.clone(),
        removed: expected,
    };
    crate::native_archive::verify_process_evidence_loader6(root, e, &checked)?;
    ensure!(
        reference(root, &binding_path(root, e)?)? == binding_ref
            && reference(root, context_path)? == context,
        "native6_process_environment_changed_during_inspection"
    );
    Ok(())
}
pub(crate) fn binding_references(
    root: &SkillRoot,
    invocations: &[CodeNativeArchiveResponseInvocation],
) -> Result<Vec<ArtifactReference>> {
    let mut refs = Vec::new();
    for i in invocations {
        for e in i
            .inputs
            .archives
            .iter()
            .map(|a| &a.index_process)
            .chain(std::iter::once(&i.native_process))
        {
            refs.push(reference(root, &binding_path(root, e)?)?);
        }
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference_value(n: &str) -> ArtifactReference {
        ArtifactReference {
            path: format!("var/{n}"),
            sha256: "a".repeat(64),
        }
    }
    #[test]
    fn fixed_removed_domain_is_exact_original_values_and_order() {
        let rows = vec![
            CodeNativeEnvironmentEntry {
                name: NAMES[0].into(),
                value: "/selected/toolchain/lib:/usr/local/lib:/usr/lib".into(),
            },
            CodeNativeEnvironmentEntry {
                name: NAMES[1].into(),
                value: "1".into(),
            },
            CodeNativeEnvironmentEntry {
                name: "SDKROOT".into(),
                value: "/selected/sdk".into(),
            },
        ];
        assert_eq!(removals(&rows).unwrap(), rows[..2]);
        let mut bad = rows.clone();
        bad[1].value = "0".into();
        assert!(removals(&bad).is_err());
        let mut duplicate = rows.clone();
        duplicate.push(rows[0].clone());
        assert!(removals(&duplicate).is_err());
        assert!(removals(&rows[1..]).is_err());
    }
    #[test]
    fn supplemental_contract_rejects_unknown_and_missing_required_field() {
        let b = CodeNativeProcessEnvironmentBinding6 {
            schema: SCHEMA.into(),
            process: reference_value("process.json"),
            compiler_intent: reference_value("intent.json"),
            stage_context: reference_value("stage.json"),
            removed_environment: vec![],
            sdk_root: None,
            linked_output: None,
        };
        let mut v = serde_json::to_value(&b).unwrap();
        v["caller_passed"] = json!(true);
        assert!(serde_json::from_value::<CodeNativeProcessEnvironmentBinding6>(v).is_err());
        let mut v = serde_json::to_value(&b).unwrap();
        v.as_object_mut().unwrap().remove("stage_context");
        assert!(serde_json::from_value::<CodeNativeProcessEnvironmentBinding6>(v).is_err());
    }
    #[test]
    fn actual_shared_process_two_removals_keeps_default3_refusal() {
        use oe_core::process::{CommandSpec, run_command};
        use std::collections::BTreeMap;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        std::fs::write(path.join("SKILL.md"), b"private controlled process fixture").unwrap();
        let root = SkillRoot::new(&path).unwrap();
        let folder = path.join("var/process");
        let exe =
            crate::native_archive::bind_file(&Path::new("/usr/bin/true").canonicalize().unwrap())
                .unwrap();
        let spec = CommandSpec {
            executable: exe.path.clone().into(),
            arguments: vec![],
            cwd: path.clone(),
            environment: BTreeMap::new(),
            remove_environment: NAMES.into_iter().map(Into::into).collect(),
            timeout: std::time::Duration::from_secs(5),
            output_limit: 1024,
        };
        let raw = run_command(&spec, &folder).unwrap();
        assert_eq!(raw.exit_code, Some(0));
        let e = CodeNativeInputProcessEvidence {
            schema: "oev2.native-input-process-evidence/3".into(),
            role: "native_link".into(),
            executable: exe,
            arguments: vec![],
            cwd: path.to_str().unwrap().into(),
            environment: vec![],
            process: reference(&root, &folder.join("process.json")).unwrap(),
            stdout: reference(&root, &folder.join("stdout")).unwrap(),
            stderr: reference(&root, &folder.join("stderr")).unwrap(),
            status: raw.status,
            exit_code: raw.exit_code.map(i64::from),
        };
        assert!(
            crate::native_archive::verify_process_evidence(&root, &e)
                .unwrap_err()
                .to_string()
                .contains("environment_key_mismatch")
        );
        // Internal unit content fixture: not a live compiler/owner capability.
        let checked = CheckedProcess6 {
            process: e.process.clone(),
            removed: vec![
                CodeNativeEnvironmentEntry {
                    name: NAMES[0].into(),
                    value: "original selected fallback".into(),
                },
                CodeNativeEnvironmentEntry {
                    name: NAMES[1].into(),
                    value: "1".into(),
                },
            ],
        };
        crate::native_archive::verify_process_evidence_loader6(&root, &e, &checked).unwrap();
        let mut wrong = e.clone();
        wrong.process.sha256 = "b".repeat(64);
        assert!(
            crate::native_archive::verify_process_evidence_loader6(&root, &wrong, &checked)
                .is_err()
        );
        let report = folder.join("process.json");
        let mut v: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        v["environment_removed_keys"] = json!([NAMES[0], NAMES[1], "SDKROOT"]);
        std::fs::write(&report, serde_json::to_vec(&v).unwrap()).unwrap();
        let changed = CodeNativeInputProcessEvidence {
            process: reference(&root, &report).unwrap(),
            ..e
        };
        let bound = CheckedProcess6 {
            process: changed.process.clone(),
            removed: checked.removed,
        };
        assert!(
            crate::native_archive::verify_process_evidence_loader6(&root, &changed, &bound)
                .is_err()
        );
    }
    #[test]
    fn link_return_pin_rejects_later_equal_image_changes_and_mode_changes() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        std::fs::write(base.join("SKILL.md"), b"finite output content check").unwrap();
        let root = SkillRoot::new(&base).unwrap();
        let folder = base.join("var/pin-control");
        std::fs::create_dir_all(&folder).unwrap();
        let output = folder.join("image");
        std::fs::write(&output, b"same-size finite image bytes").unwrap();
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o700)).unwrap();
        let process = folder.join("process.json");
        std::fs::write(&process, b"{}\n").unwrap();
        let r = reference(&root, &process).unwrap();
        // Pure content fixture: no live compiler, native completion or owner right.
        let e = CodeNativeInputProcessEvidence {
            schema: "oev2.native-input-process-evidence/3".into(),
            role: "native_link".into(),
            executable: crate::native_archive::bind_file(&output).unwrap(),
            arguments: vec!["-o".into(), output.display().to_string()],
            cwd: folder.display().to_string(),
            environment: vec![],
            process: r.clone(),
            stdout: r.clone(),
            stderr: r.clone(),
            status: "completed".into(),
            exit_code: Some(0),
        };
        let pin = returned_output(&root, &e).unwrap().unwrap();
        let b = CodeNativeProcessEnvironmentBinding6 {
            schema: SCHEMA.into(),
            process: r.clone(),
            compiler_intent: r.clone(),
            stage_context: r,
            removed_environment: vec![],
            sdk_root: None,
            linked_output: Some(pin.clone()),
        };
        std::fs::write(
            binding_path(&root, &e).unwrap(),
            serde_json::to_vec(&b).unwrap(),
        )
        .unwrap();
        assert_eq!(output_record(&root, &e).unwrap().1, pin);
        let parent = folder.join("build-script-build");
        std::fs::copy(&output, &parent).unwrap();
        let changed = b"same-size finite image byteS";
        assert_eq!(changed.len() as u64, pin.bytes);
        std::fs::write(&output, changed).unwrap();
        std::fs::write(&parent, changed).unwrap();
        assert_eq!(
            std::fs::read(&output).unwrap(),
            std::fs::read(parent).unwrap()
        );
        assert!(output_record(&root, &e).is_err());
        std::fs::write(&output, b"same-size finite image bytes").unwrap();
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(output_record(&root, &e).is_err());
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut absent = e.clone();
        absent.arguments.clear();
        assert!(returned_output(&root, &absent).is_err());
        let mut duplicate = e.clone();
        duplicate.arguments.extend(e.arguments.clone());
        assert!(returned_output(&root, &duplicate).is_err());
        let mut failed = e.clone();
        failed.exit_code = Some(1);
        assert!(returned_output(&root, &failed).unwrap().is_none());
        let mut index = e;
        index.role = "archive_index".into();
        assert!(returned_output(&root, &index).unwrap().is_none());
    }
}
