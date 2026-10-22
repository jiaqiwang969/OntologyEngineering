//! Windows observations mapped to the shared protocol. No independent state rules.
use crate::windows::*;
use anyhow::{Result, ensure};
use oe_nx_protocol as protocol;
use sha2::{Digest as _, Sha256};
use std::{ffi::OsStr, io::Read};

pub fn root_identity(root: &StableRoot) -> protocol::RootIdentity {
    protocol::RootIdentity {
        host_id: root.identity().host_id.clone(),
        volume_id: root.identity().volume_id.clone(),
        directory_id: root.identity().directory_id.clone(),
    }
}
pub fn current_process_identity(
    image: protocol::BlobIdentity,
) -> Result<protocol::ProcessIdentity> {
    let identity = ProcessWatch::current()?.identity();
    let result = protocol::ProcessIdentity {
        host_id: host_identity()?,
        pid: identity.pid,
        creation_time_100ns: identity.creation_time_100ns,
        image,
    };
    result.validate()?;
    Ok(result)
}
pub fn file_identity(pin: &mut PinnedFile) -> Result<protocol::BlobIdentity> {
    let mut hash = Sha256::new();
    let size = pin.size();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    let reader = pin.reader()?;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .ok_or_else(|| anyhow::anyhow!("file byte count overflow"))?;
        ensure!(count <= size, "pinned file grew");
        hash.update(&buffer[..n]);
    }
    pin.verify()?;
    ensure!(count == size, "pinned file size differs");
    Ok(protocol::BlobIdentity {
        sha256: protocol::Digest::new(format!("{:x}", hash.finalize()))?,
        bytes: size,
    })
}
pub fn pin_relative_file(
    dir: &PinnedDirectory<'_>,
    path: &protocol::RelativePath,
    max_bytes: u64,
) -> Result<PinnedFile> {
    fn inner(dir: &PinnedDirectory<'_>, parts: &[&str], max_bytes: u64) -> Result<PinnedFile> {
        if parts.len() == 1 {
            return dir
                .pin_file(OsStr::new(parts[0]), max_bytes)?
                .ok_or_else(|| anyhow::anyhow!("required file missing"));
        }
        let child = dir.directory(OsStr::new(parts[0]))?;
        inner(&child, &parts[1..], max_bytes)
    }
    inner(
        dir,
        &path.as_str().split('/').collect::<Vec<_>>(),
        max_bytes,
    )
}
pub fn hold_input(facts: &DirectoryRecords) -> protocol::HoldInput {
    if facts.pending.is_some() {
        return protocol::HoldInput::PendingWrite;
    }
    if let Some(error) = &facts.error {
        return protocol::HoldInput::IoUncertain(error.clone());
    }
    if !facts.unexpected_control_files.is_empty() {
        return protocol::HoldInput::IoUncertain("orphan_pending_state_temporary".into());
    }
    match &facts.state {
        Some(bytes) => protocol::HoldInput::StateBytes(bytes.clone()),
        None => protocol::HoldInput::Missing,
    }
}
pub fn assess_facts(
    root: &protocol::RootIdentity,
    facts: &DirectoryRecords,
) -> protocol::HoldAssessment {
    let Some(name) = facts.name.to_str() else {
        return protocol::classify_hold(&protocol::HoldInput::IoUncertain(
            "job_directory_encoding".into(),
        ));
    };
    let Ok(job_id) = protocol::JobId::new(name) else {
        return protocol::classify_hold(&protocol::HoldInput::IoUncertain(
            "invalid_job_directory_name".into(),
        ));
    };
    protocol::classify_job_hold(&hold_input(facts), &job_id, root)
}
pub fn scan_holds(
    lock: &WriterLock<'_>,
) -> Result<Vec<(DirectoryRecords, protocol::HoldAssessment)>> {
    let root = root_identity(lock.root());
    let records = scan_writer_records(lock)?;
    Ok(records
        .into_iter()
        .map(|fact| {
            let assessment = assess_facts(&root, &fact);
            (fact, assessment)
        })
        .collect())
}
/// This writes only a shared-protocol validated state to its actual bound job.
/// Callers obtain transitions exclusively through StateRecord methods.
pub fn persist_state(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    job: &protocol::JobManifest,
    expected: Option<&[u8]>,
    next: &protocol::StateRecord,
) -> Result<()> {
    job.validate()?;
    next.validate(job)?;
    ensure!(
        job.root == root_identity(lock.root()),
        "persisted root differs"
    );
    ensure!(
        dir.path().file_name() == Some(OsStr::new(job.job_id.as_str())),
        "persisted job directory differs"
    );
    if let Some(bytes) = expected {
        let old: protocol::StateRecord = protocol::decode(bytes)?;
        old.validate(job)?;
    }
    let bytes = serde_json::to_vec(next)?;
    ensure!(
        bytes.len() <= protocol::MAX_RECORD_BYTES,
        "state exceeds protocol bound"
    );
    write_state_atomic(dir, lock, expected, &bytes)?;
    Ok(())
}

/// Enumerate every member, including hidden entries, while pinning all normal
/// file bytes and directory ancestors. No links, case aliases or extras vanish.
fn inventory_tree_bounded(
    dir: &PinnedDirectory<'_>,
    prefix: &str,
    max_bytes: u64,
    max_file_bytes: u64,
    max_files: usize,
) -> Result<(Vec<protocol::FileEntry>, Vec<PinnedFile>)> {
    fn walk(
        dir: &PinnedDirectory<'_>,
        prefix: &str,
        max: u64,
        max_file: u64,
        max_files: usize,
        total: &mut u64,
        entries: &mut Vec<protocol::FileEntry>,
        pins: &mut Vec<PinnedFile>,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("nonunicode inventory member"))?;
            let path = protocol::RelativePath::new(format!("{prefix}{name}"))?;
            let metadata = entry.file_type()?;
            if metadata.is_dir() {
                let child = dir.directory(OsStr::new(name))?;
                walk(
                    &child,
                    &format!("{}/", path.as_str()),
                    max,
                    max_file,
                    max_files,
                    total,
                    entries,
                    pins,
                )?;
            } else {
                let mut pin = dir
                    .pin_file(OsStr::new(name), max_file)?
                    .ok_or_else(|| anyhow::anyhow!("inventory file disappeared"))?;
                let identity = file_identity(&mut pin)?;
                *total = total
                    .checked_add(identity.bytes)
                    .ok_or_else(|| anyhow::anyhow!("inventory overflow"))?;
                ensure!(
                    *total <= max && entries.len() < max_files,
                    "inventory exceeds bound"
                );
                entries.push(protocol::FileEntry { path, identity });
                pins.push(pin);
            }
        }
        dir.verify()?;
        Ok(())
    }
    let (mut total, mut entries, mut pins) = (0, Vec::new(), Vec::new());
    walk(
        dir,
        prefix,
        max_bytes,
        max_file_bytes,
        max_files,
        &mut total,
        &mut entries,
        &mut pins,
    )?;
    Ok((entries, pins))
}
/// Original v2 inventory contract, including the original total-size ceiling.
pub fn inventory_tree(
    dir: &PinnedDirectory<'_>,
    prefix: &str,
    max_bytes: u64,
) -> Result<(Vec<protocol::FileEntry>, Vec<PinnedFile>)> {
    let result = inventory_tree_bounded(dir, prefix, max_bytes, max_bytes, protocol::MAX_FILES)?;
    protocol::validate_inventory(&result.0)?;
    Ok(result)
}
/// A streaming observation bounded by the job's explicit input/output contract.
pub fn inventory_job_tree(
    dir: &PinnedDirectory<'_>,
    prefix: &str,
    job: &protocol::JobManifest,
    output: bool,
) -> Result<(Vec<protocol::FileEntry>, Vec<PinnedFile>)> {
    let (total, file, count) = job.inventory_bounds(output)?;
    let result = inventory_tree_bounded(dir, prefix, total, file, count)?;
    job.validate_inventory_capacity(&result.0, output)?;
    Ok(result)
}
/// A read-only observation of directories, including empty `out/`. This is
/// derived controller evidence, not an invented native job file or state.
pub fn directory_layout(dir: &PinnedDirectory<'_>) -> Result<Vec<String>> {
    fn walk(dir: &PinnedDirectory<'_>, prefix: &str, out: &mut Vec<String>) -> Result<()> {
        for entry in std::fs::read_dir(dir.path())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name();
                let name = name
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("nonunicode directory member"))?;
                let relative = protocol::RelativePath::new(format!("{prefix}{name}"))?;
                ensure!(
                    out.len() < protocol::MAX_FILES,
                    "directory envelope exceeds bound"
                );
                let child = dir.directory(OsStr::new(name))?;
                out.push(relative.as_str().into());
                walk(&child, &format!("{}/", relative.as_str()), out)?;
                child.verify()?;
            }
        }
        dir.verify()?;
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out)?;
    out.sort();
    Ok(out)
}
pub fn create_relative_new(
    dir: &PinnedDirectory<'_>,
    path: &protocol::RelativePath,
    bytes: &[u8],
) -> Result<()> {
    fn inner(dir: &PinnedDirectory<'_>, parts: &[&str], bytes: &[u8]) -> Result<()> {
        if parts.len() == 1 {
            dir.create_file_new(OsStr::new(parts[0]), bytes, protocol::MAX_PAYLOAD_BYTES)?;
            return Ok(());
        }
        let child = match dir.directory(OsStr::new(parts[0])) {
            Ok(v) => v,
            Err(e) if e.raw_os_error() == Some(2) => {
                dir.create_directory_new(OsStr::new(parts[0]))?
            }
            Err(e) => return Err(e.into()),
        };
        inner(&child, &parts[1..], bytes)
    }
    inner(dir, &path.as_str().split('/').collect::<Vec<_>>(), bytes)
}
