//! Shared application-boundary helpers. Reading a hash never admits an engineering fact.
use anyhow::{Result, ensure};
use oe_contracts::{ArtifactReference, ErrorCode, FailureRecord};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::Path;

pub fn read(root: &SkillRoot, reference: &ArtifactReference) -> Result<Vec<u8>> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("artifact_contract_invalid"))?;
    oe_semantic::safe_relative(&reference.path)?;
    let bytes = read_regular_bytes(&root.path().join(&reference.path), 16 * 1024 * 1024)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "artifact_source_changed"
    );
    Ok(bytes)
}
pub fn json(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    Ok(oe_core::json::strict_json(&read(root, reference)?)?)
}
pub fn reference(root: &SkillRoot, path: &Path) -> Result<ArtifactReference> {
    let bytes = read_regular_bytes(path, 16 * 1024 * 1024)?;
    Ok(ArtifactReference {
        path: oe_core::portable_source_path(root.path(), path)?,
        sha256: sha256(bytes),
    })
}
pub fn private_policy(root: &SkillRoot, path: &str) -> Result<(Value, String)> {
    let path = root.private_path(Path::new(path))?;
    #[cfg(windows)]
    {
        // Reuse the native owner/DACL, identity and bounded-byte reader. This
        // adds Windows support to the existing policy boundary; it grants no
        // authority by itself and never repairs input permissions.
        let snapshot = oe_core::private_file::read(&path, 1024 * 1024)?;
        let value = oe_core::json::strict_json(&snapshot.bytes)?;
        snapshot.recheck(&path, 1024 * 1024)?;
        Ok((value, snapshot.identity.sha256))
    }
    #[cfg(not(windows))]
    {
        let bytes = read_regular_bytes(&path, 1024 * 1024)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata = std::fs::metadata(&path)?;
            ensure!(
                metadata.permissions().mode() & 0o077 == 0
                    && metadata.uid() == unsafe { libc::geteuid() },
                "authority_policy_not_private_or_owned"
            );
        }
        #[cfg(not(unix))]
        anyhow::bail!("authority_policy_backend_not_verified");
        #[allow(unreachable_code)]
        Ok((oe_core::json::strict_json(&bytes)?, sha256(bytes)))
    }
}
pub fn failure(phase: &str, error: anyhow::Error) -> FailureRecord {
    let code = if phase == "engineering_work" {
        ErrorCode::SemanticUnknown
    } else {
        ErrorCode::GovernanceDenied
    };
    FailureRecord {
        code,
        phase: phase.into(),
        retryable: false,
        partial_result: Some(json!({"execution":"failed","conclusion":"unknown",
            "reason":error.to_string(),"engineering_adoption":"not_established"})),
    }
}
