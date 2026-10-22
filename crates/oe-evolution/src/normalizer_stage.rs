//! Shared normalizer stage. Invocation retains the caller's native session,
//! source/tool guards, output resources and deadline; this is not a build receipt.
use crate::{CodeBinaryRef, binary_reference, public_manifest};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{ffi::OsString, path::Path};

pub(crate) fn build(
    root: &Path,
    source_before: &Value,
    target: &Path,
    evidence: &Path,
    invoke: &impl Fn(Vec<OsString>, &Path) -> Result<Value>,
) -> Result<(CodeBinaryRef, Value)> {
    // Keep the original Cargo arguments and native stage name. The bootstrap's
    // own production require() still rejects compiled-normalizer source drift.
    let process = invoke(
        vec![
            "build".into(),
            "--locked".into(),
            "--offline".into(),
            "-j4".into(),
            "-p".into(),
            "oe-design".into(),
            "--bin".into(),
            "oev2-design-bootstrap".into(),
        ],
        &evidence.join("build-normalizer"),
    )?;
    ensure!(
        public_manifest(root)? == *source_before,
        "code_source_changed_during_normalizer_build"
    );
    let normalizer = binary_reference(&target.join("debug/oev2-design-bootstrap"), root)?;
    Ok((normalizer, process))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::write(root.join("SKILL.md"), "test source").unwrap();
        std::fs::create_dir_all(root.join("var/target/debug")).unwrap();
        (temp, root)
    }

    #[test]
    fn normalizer_stage_preserves_invocation_and_source_binding() {
        let (_temp, root) = fixture();
        let before = public_manifest(&root).unwrap();
        let target = root.join("var/target");
        let evidence = root.join("var/evidence");
        let (binary, process) = build(&root, &before, &target, &evidence, &|args, dir| {
            assert_eq!(
                args,
                [
                    "build",
                    "--locked",
                    "--offline",
                    "-j4",
                    "-p",
                    "oe-design",
                    "--bin",
                    "oev2-design-bootstrap"
                ]
                .map(OsString::from)
            );
            assert_eq!(dir, evidence.join("build-normalizer"));
            std::fs::write(target.join("debug/oev2-design-bootstrap"), b"fixture bytes")?;
            Ok(serde_json::json!({"fixture_process": 17}))
        })
        .unwrap();
        assert_eq!(binary.sha256, oe_core::sha256(b"fixture bytes"));
        assert_eq!(
            binary.source_manifest_sha256,
            crate::digest(&before).unwrap()
        );
        assert_eq!(process, serde_json::json!({"fixture_process": 17}));
        assert_eq!(public_manifest(&root).unwrap(), before);
        assert!(!evidence.join("actual-ggen").exists());
        assert!(!evidence.join("artifact.json").exists());
    }

    #[test]
    fn normalizer_stage_rejects_source_mutation_before_accepting_binary() {
        let (_temp, root) = fixture();
        let before = public_manifest(&root).unwrap();
        let error = build(
            &root,
            &before,
            &root.join("var/target"),
            &root.join("var/evidence"),
            &|_, _| {
                std::fs::write(root.join("SKILL.md"), "changed source")?;
                Ok(serde_json::json!({}))
            },
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "code_source_changed_during_normalizer_build"
        );
    }

    #[test]
    fn normalizer_stage_preserves_invocation_failure() {
        let (_temp, root) = fixture();
        let before = public_manifest(&root).unwrap();
        let error = build(
            &root,
            &before,
            &root.join("var/target"),
            &root.join("var/evidence"),
            &|_, _| anyhow::bail!("fixture_native_session_rejected"),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "fixture_native_session_rejected");
    }
}
