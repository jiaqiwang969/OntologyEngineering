//! Shared, bounded evidence capture for module projections.
use anyhow::{Result, ensure};
use oe_contracts::ArtifactReference;
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256, write_new_bytes};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
const MAX_REFERENCES: usize = 256;
const MAX_EVIDENCE: usize = 64 * 1024 * 1024;
pub(super) type Captured = BTreeMap<String, (ArtifactReference, Vec<u8>)>;
pub(super) fn input(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.data_expand(path)?;
    root.portable_data_path(&path)?;
    no_symlinks(&path)?;
    Ok(path)
}
// All evidence is read and bounded before allocating the output directory.
pub(super) fn retain(
    root: &SkillRoot,
    refs: &[ArtifactReference],
) -> Result<BTreeMap<String, (ArtifactReference, Vec<u8>)>> {
    ensure!(
        refs.len() <= MAX_REFERENCES,
        "projection_artifact_count_limit"
    );
    let mut result: BTreeMap<String, (ArtifactReference, Vec<u8>)> = BTreeMap::new();
    let mut total = 0usize;
    for item in refs {
        ensure!(
            item.validate().is_ok(),
            "projection_artifact_contract_invalid"
        );
        if let Some((prior, _)) = result.get(&item.path) {
            ensure!(prior == item, "projection_artifact_binding_conflict");
            continue;
        }
        let path = input(root, Path::new(&item.path))?;
        ensure!(
            root.portable_data_path(&path)? == item.path,
            "projection_artifact_relative_path_required"
        );
        let bytes = read_regular_bytes(&path, MAX_EVIDENCE - total)?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("projection_evidence_limit"))?;
        ensure!(total <= MAX_EVIDENCE, "projection_evidence_limit");
        ensure!(
            sha256(&bytes) == item.sha256,
            "projection_artifact_digest_mismatch"
        );
        result.insert(item.path.clone(), (item.clone(), bytes));
    }
    Ok(result)
}

pub(super) fn archive(
    root: &SkillRoot,
    output: &Path,
    captured: &Captured,
) -> Result<BTreeMap<String, ArtifactReference>> {
    let directory = output.join("evidence");
    oe_core::create_new_private_dir(&directory)?;
    let mut result = BTreeMap::new();
    for (index, (key, (reference, bytes))) in captured.iter().enumerate() {
        let target = directory.join(format!("{index}.bin"));
        write_new_bytes(&target, bytes)?;
        result.insert(
            key.clone(),
            ArtifactReference {
                path: root.portable_data_path(&target)?,
                sha256: reference.sha256.clone(),
            },
        );
    }
    Ok(result)
}
