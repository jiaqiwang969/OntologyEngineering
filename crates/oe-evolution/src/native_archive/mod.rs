//! Typed, bounded Darwin input preparation. No process is spawned here.
//! Byte snapshots support late CONTENT inspection; live original inputs have
//! a separate check because rustc may remove its temporary objects after linking.
mod archive;
mod prepare;
pub(crate) use prepare::{
    prepare_link_inputs_scoped, prepare_link_inputs_scoped_verified, verify_prepared_scoped,
    verify_prepared_scoped_verified,
};
mod response;
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{
    SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256, write_new_bytes,
};
use oe_wire::json::strict_json;
pub(crate) use prepare::prepare_link_inputs_checked;
pub use prepare::{prepare_link_inputs, verify_live_inputs, verify_prepared};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const ALGORITHM: &str = "darwin-archive-response/1";
/// Inspect every payload of an original external Darwin archive without
/// changing its member order, names, attributes, index or any original byte.
pub(crate) fn inspect_original_archive<F>(raw: &[u8], mut inspect: F) -> Result<()>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    let parsed = archive::parse(raw)?;
    ensure!(
        parsed
            .structure
            .iter()
            .all(|s| !matches!(s.name.as_str(), "/" | "/SYM64/" | "//")),
        "native3_external_gnu_archive_unsupported"
    );
    for member in &parsed.members {
        inspect(&member.payload).with_context(|| {
            format!("native3_archive_member:{}:{}", member.ordinal, member.name)
        })?;
    }
    archive::verify_index(&parsed)?;
    Ok(())
}
pub(crate) const FILE_LIMIT: usize = 128 * 1024 * 1024;
pub(crate) fn path_text(p: &Path) -> Result<String> {
    Ok(p.to_str().context("native_input_path_utf8")?.to_owned())
}
pub(crate) fn mode(p: &Path) -> Result<u64> {
    let m = std::fs::symlink_metadata(p)?;
    ensure!(
        m.is_file() && !m.file_type().is_symlink(),
        "native_input_not_regular"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(u64::from(m.permissions().mode() & 0o7777))
    }
    #[cfg(not(unix))]
    {
        anyhow::bail!("native_input_platform_uncovered")
    }
}
/// Zero byte files are legal for response/capture snapshots. This does not
/// change the original /2 recipe's strictly positive regular-file guard.
pub fn bind_file(p: &Path) -> Result<CodeNativeFileBinding> {
    ensure!(p.is_absolute(), "native_input_path_not_absolute");
    no_symlinks(p)?;
    let before = mode(p)?;
    let a = fingerprint_file(p, 512 * 1024 * 1024)?;
    let b = fingerprint_file(p, 512 * 1024 * 1024)?;
    ensure!(
        a == b && before == mode(p)?,
        "native_input_changed_during_read"
    );
    Ok(CodeNativeFileBinding {
        path: path_text(p)?,
        sha256: a.sha256,
        bytes: a.bytes,
        mode: before,
    })
}
pub fn verify_binding(p: &CodeNativeFileBinding) -> Result<()> {
    p.validate()
        .map_err(|e| anyhow::anyhow!("native_input_binding:{e:?}"))?;
    ensure!(
        bind_file(Path::new(&p.path))? == *p,
        "native_input_binding_changed"
    );
    Ok(())
}
pub(crate) fn reference_bytes(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("native_input_reference:{e:?}"))?;
    let p = root.private_path(Path::new(&r.path))?;
    let b = read_regular_bytes(&p, FILE_LIMIT)?;
    ensure!(sha256(&b) == r.sha256, "native_input_reference_changed");
    Ok(b)
}
pub(crate) fn snapshot_matches(
    root: &SkillRoot,
    r: &ArtifactReference,
    f: &CodeNativeFileBinding,
) -> Result<Vec<u8>> {
    let b = reference_bytes(root, r)?;
    ensure!(
        f.sha256 == sha256(&b)
            && f.bytes == b.len() as u64
            && f.mode <= 0o7777
            && Path::new(&f.path).is_absolute(),
        "native_input_snapshot_binding_mismatch"
    );
    Ok(b)
}
pub(crate) struct Store<'a> {
    pub root: &'a SkillRoot,
    dir: PathBuf,
    next: u64,
}
impl<'a> Store<'a> {
    pub fn new(root: &'a SkillRoot, dir: PathBuf) -> Self {
        Self { root, dir, next: 0 }
    }
    pub fn save(&mut self, role: &str, b: &[u8]) -> Result<ArtifactReference> {
        let p = self.dir.join(format!("{:06}-{role}.bytes", self.next));
        self.next += 1;
        write_new_bytes(&p, b)?;
        Ok(ArtifactReference {
            path: path_text(p.strip_prefix(self.root.path())?)?,
            sha256: sha256(b),
        })
    }
}
pub(crate) fn environment(e: &[CodeNativeEnvironmentEntry]) -> Result<()> {
    let mut keys = BTreeSet::new();
    for row in e {
        row.validate()
            .map_err(|e| anyhow::anyhow!("native_input_environment:{e:?}"))?;
        ensure!(
            keys.insert(&row.name) && !row.name.contains(['=', '\0']) && !row.value.contains('\0'),
            "native_input_environment_duplicate_or_invalid"
        );
    }
    Ok(())
}
/// Rechecks actual shared-process report and both complete retained streams.
/// Environment values are selected CommandSpec values, not independently
/// exposed by the shared report (which intentionally retains only key names).
pub fn verify_process_evidence(root: &SkillRoot, e: &CodeNativeInputProcessEvidence) -> Result<()> {
    verify_process_evidence_inner(root, e, &json!([]))
}
/// Explicit /6 content path; the private checked binding is created only after
/// actual selected compiler intent/stage/environment/SDK inspection. This
/// does not give a deserialized receipt any execution or owner capability.
pub(crate) fn verify_process_evidence_loader6(
    root: &SkillRoot,
    e: &CodeNativeInputProcessEvidence,
    checked: &crate::native_process6::CheckedProcess6,
) -> Result<()> {
    ensure!(
        checked.process() == &e.process,
        "native6_process_checked_binding_mismatch"
    );
    verify_process_evidence_inner(root, e, &checked.removed_keys())
}
fn verify_process_evidence_inner(
    root: &SkillRoot,
    e: &CodeNativeInputProcessEvidence,
    removed_keys: &Value,
) -> Result<()> {
    e.validate()
        .map_err(|x| anyhow::anyhow!("native_input_process_contract:{x:?}"))?;
    environment(&e.environment)?;
    verify_binding(&e.executable)?;
    let raw = reference_bytes(root, &e.process)?;
    let v: Value = strict_json(&raw)?;
    ensure!(
        v["schema"] == "ontology-engineering.external-process/v1"
            && v["executable"]["path"] == e.executable.path
            && v["executable"]["resolved"] == e.executable.path
            && v["executable"]["identity"]["sha256"] == e.executable.sha256
            && v["executable"]["identity"]["bytes"].as_u64() == Some(e.executable.bytes)
            && v["executable"]["unchanged"] == true,
        "native_input_process_executable_mismatch"
    );
    ensure!(
        v["arguments"] == serde_json::to_value(&e.arguments)?
            && v["cwd"] == e.cwd
            && v["status"] == e.status
            && v["exit_code"] == serde_json::to_value(e.exit_code)?,
        "native_input_process_invocation_mismatch"
    );
    ensure!(
        v["capture_complete"] == true
            && v["streams_collected"] == 2
            && v["descendant_cleanup"]["state"] == "local_cleanup_completed"
            && v["spawn_error_kind"].is_null()
            && ["completed", "process_failed"].contains(&e.status.as_str())
            && v["observed_status"] == e.status,
        "native_input_process_capture_incomplete"
    );
    let keys = e
        .environment
        .iter()
        .map(|x| x.name.clone())
        .collect::<BTreeSet<_>>();
    let observed_array = v["environment_added_keys"]
        .as_array()
        .context("native_input_process_environment_keys")?;
    let observed = observed_array
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_owned)
                .context("native_input_process_environment_key")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        observed_array.len() == observed.len()
            && keys == observed
            && &v["environment_removed_keys"] == removed_keys
            && v["environment_values"] == "not_recorded",
        "native_input_process_environment_key_mismatch"
    );
    let limit = v["per_stream_limit"]
        .as_u64()
        .context("native_input_process_stream_limit_missing")?;
    ensure!(
        limit > 0 && limit <= 64 * 1024 * 1024,
        "native_input_process_stream_limit"
    );
    for (name, r) in [("stdout", &e.stdout), ("stderr", &e.stderr)] {
        let b = reference_bytes(root, r)?;
        ensure!(
            v[name]["bytes"].as_u64() == Some(b.len() as u64)
                && v[name]["sha256"] == sha256(&b)
                && v[name]["truncated"] == false
                && b.len() as u64 <= limit,
            "native_input_process_stream_mismatch"
        );
        let rp = root.private_path(Path::new(&r.path))?;
        let pp = root.private_path(Path::new(&e.process.path))?;
        let stream_path = v[name]["path"]
            .as_str()
            .context("native_input_process_stream_path_missing")?;
        ensure!(
            pp.parent()
                .context("native_input_process_parent")?
                .join(stream_path)
                == rp,
            "native_input_process_stream_path_mismatch"
        );
    }
    Ok(())
}
pub fn inspect_preparation(
    root: &SkillRoot,
    r: &ArtifactReference,
) -> Result<CodeNativeInputPreparationFacts> {
    let raw = reference_bytes(root, r)?;
    let facts: CodeNativeInputPreparationFacts = serde_json::from_value(strict_json(&raw)?)?;
    verify_prepared(root, &facts)?;
    ensure!(
        reference_bytes(root, r)? == raw,
        "native_input_document_changed"
    );
    Ok(facts)
}
pub struct NativeFullInputInspector<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeFullInputInspectPort for NativeFullInputInspector<'_> {
    fn inspect_full_native_inputs(
        &self,
        input: &ArtifactReference,
    ) -> std::result::Result<CodeNativeInputPreparationFacts, FailureRecord> {
        inspect_preparation(self.root,input).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,phase:"native_full_input_content_inspection".into(),retryable:false,partial_result:Some(json!({"inspection":"rejected","reason":format!("{e:#}"),"execution":"not_run","compilation_origin":"unknown","engineering_adoption":"not_established"}))})
    }
}
