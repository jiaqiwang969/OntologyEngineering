//! Declared source closure checks; actual CAD reference coverage is separate.
use anyhow::{Result, ensure};
use oe_contracts::{NxAssemblySourceClosure, NxAssemblySourceFile};
use oe_nx_protocol::{BlobIdentity, Digest, RelativePath};
use std::collections::BTreeSet;
pub fn validate_source_closure(c: &NxAssemblySourceClosure) -> Result<()> {
    c.validate()
        .map_err(|_| anyhow::anyhow!("motion_source_generated_invalid"))?;
    ensure!(
        c.schema == "oev2.nx-assembly-source-closure/1",
        "motion_source_closure_invalid"
    );
    RelativePath::new(c.root_part.clone())?;
    validate_source_files(&c.files, c.max_total_bytes)?;
    ensure!(
        c.files.iter().any(|f| f.path == c.root_part),
        "motion_source_root_missing"
    );
    Ok(())
}
/// Shared source-set integrity; unlike an existing assembly closure, this does
/// not assert that any input is an assembly root.
pub fn validate_source_files(files: &[NxAssemblySourceFile], max_total_bytes: u64) -> Result<()> {
    ensure!(
        !files.is_empty() && files.len() <= 4096,
        "motion_source_closure_invalid"
    );
    ensure!(
        (1..=4096 * 100 * 1024 * 1024u64).contains(&max_total_bytes),
        "motion_source_total_budget_invalid"
    );
    let mut compatible = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for f in files {
        RelativePath::new(f.path.clone())?;
        Digest::new(f.sha256.clone())?;
        ensure!(
            (1..=oe_nx_protocol::MAX_PAYLOAD_BYTES).contains(&f.bytes),
            "motion_source_file_size_invalid"
        );
        ensure!(
            names.insert(oe_nx_protocol::path_collision_key(&f.path))
                && compatible.insert(oe_nx_protocol::compatibility_collision_key(&f.path)),
            "motion_source_case_alias"
        );
        total = total
            .checked_add(f.bytes)
            .ok_or_else(|| anyhow::anyhow!("motion_source_size_overflow"))?;
        ensure!(
            total <= max_total_bytes,
            "motion_source_total_budget_exceeded"
        );
    }
    for name in names.iter().chain(compatible.iter()) {
        let mut prefix = String::new();
        let parts = name.split('/').collect::<Vec<_>>();
        for p in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(p);
            ensure!(
                !names.contains(&prefix) && !compatible.contains(&prefix),
                "motion_source_file_directory_conflict"
            );
        }
    }
    source_file_directories(files)?;
    Ok(())
}
pub fn verify_source_file(f: &NxAssemblySourceFile, bytes: &[u8]) -> Result<()> {
    RelativePath::new(f.path.clone())?;
    Digest::new(f.sha256.clone())?;
    let identity = BlobIdentity::of(bytes);
    ensure!(
        bytes.len() as u64 == f.bytes && identity.sha256.as_str() == f.sha256,
        "motion_source_bytes_changed"
    );
    Ok(())
}

/// Deterministic parent-first directory plan; reject ambiguous spelling and
/// bound directory handles before creating any output hierarchy.
pub fn source_directories(c: &NxAssemblySourceClosure) -> Result<Vec<String>> {
    source_file_directories(&c.files)
}
pub fn source_file_directories(files: &[NxAssemblySourceFile]) -> Result<Vec<String>> {
    let mut dirs = std::collections::BTreeMap::<String, String>::new();
    let mut compatible = std::collections::BTreeMap::<String, String>::new();
    for file in files {
        RelativePath::new(file.path.clone())?;
        let parts = file.path.split('/').collect::<Vec<_>>();
        let mut prefix = String::new();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            for (map, key) in [
                (&mut dirs, oe_nx_protocol::path_collision_key(&prefix)),
                (
                    &mut compatible,
                    oe_nx_protocol::compatibility_collision_key(&prefix),
                ),
            ] {
                if let Some(prior) = map.get(&key) {
                    ensure!(prior == &prefix, "motion_source_directory_alias");
                } else {
                    map.insert(key, prefix.clone());
                }
            }
            ensure!(
                dirs.len() <= oe_nx_protocol::MAX_FILES,
                "motion_source_directory_budget"
            );
        }
    }
    let mut result = dirs.into_values().collect::<Vec<_>>();
    result.sort_by(|a, b| {
        a.matches('/')
            .count()
            .cmp(&b.matches('/').count())
            .then(a.cmp(b))
    });
    Ok(result)
}
#[cfg(windows)]
pub mod windows;
