//! Portable bounded v2 job archive validation, shared by local preparation and
//! Windows intake. No destination is created or changed by this module.
use crate::*;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

/// Exact member bytes, not a claim that the binary was built from its sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub job: JobManifest,
    pub members: BTreeMap<String, Vec<u8>>,
}

pub fn insert(members: &mut BTreeMap<String, Vec<u8>>, name: &str, bytes: Vec<u8>) -> Result<()> {
    RelativePath::new(name)?;
    ensure!(!members.contains_key(name), "duplicate package member");
    let total = members
        .values()
        .try_fold(bytes.len() as u64, |total, item| {
            total
                .checked_add(item.len() as u64)
                .context("package byte overflow")
        })?;
    ensure!(
        total <= MAX_PAYLOAD_BYTES && members.len() < MAX_FILES,
        "package transfer bound exceeded"
    );
    members.insert(name.to_owned(), bytes);
    Ok(())
}
pub fn payload_inventory(members: &BTreeMap<String, Vec<u8>>) -> Result<Vec<FileEntry>> {
    members
        .iter()
        .filter(|(name, _)| name.as_str() != "job.json")
        .map(|(name, bytes)| FileEntry::from_bytes(name, bytes))
        .collect()
}
impl Package {
    pub fn verify(&self) -> Result<()> {
        self.job.require_archive_stage()?;
        let decoded: JobManifest = decode(
            self.members
                .get("job.json")
                .context("missing job manifest")?,
        )?;
        ensure!(decoded == self.job, "job manifest bytes differ");
        self.job
            .verify_payload(&payload_inventory(&self.members)?)?;
        self.job.validate_parameters(
            self.members
                .get("params.json")
                .context("missing parameters")?,
        )?;
        let total = self.members.values().try_fold(0u64, |total, bytes| {
            total
                .checked_add(bytes.len() as u64)
                .context("package byte overflow")
        })?;
        ensure!(
            self.members.len() <= MAX_FILES && total <= MAX_PAYLOAD_BYTES,
            "complete package exceeds bound"
        );
        Ok(())
    }
    /// The exact returned ZIP SHA/length is bound in the stage receipt;
    /// canonical compressed bytes are not assumed across library versions.
    pub fn archive(&self) -> Result<Vec<u8>> {
        self.verify()?;
        archive_members(&self.members)
    }
    pub fn from_archive(bytes: &[u8]) -> Result<Self> {
        from_archive(bytes)
    }
}

/// Encode bounded ordinary evidence members (including legacy v1 receipts)
/// without interpreting their JSON as a new v2 executable job. The caller owns
/// which task-owned files may be fetched and binds this exact ZIP in its receipt.
pub fn archive_members(members: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    ensure!(!members.is_empty(), "empty evidence archive");
    validate_inventory(
        &members
            .iter()
            .map(|(name, bytes)| FileEntry::from_bytes(name, bytes))
            .collect::<Result<Vec<_>>>()?,
    )?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    for (name, bytes) in members {
        writer.start_file(name, options)?;
        writer.write_all(bytes)?;
    }
    let bytes = writer.finish()?.into_inner();
    ensure!(
        bytes.len() as u64 <= MAX_PAYLOAD_BYTES,
        "compressed transfer exceeds bound"
    );
    Ok(bytes)
}

/// Inspect every raw central-directory member before zip 6 builds its IndexMap
/// (which otherwise hides exact duplicate filenames). This controlled v2 wire
/// format uses one disk and ZIP32; its 100 MiB / 10,000-entry limits never require
/// ZIP64. Unrecognized extra records/volumes are refused, not silently dropped.
fn central_inventory(bytes: &[u8]) -> Result<Vec<(String, u64)>> {
    ensure!(
        bytes.len() >= 22 && bytes.len() as u64 <= MAX_PAYLOAD_BYTES,
        "archive wire bound exceeded"
    );
    let lower = bytes.len().saturating_sub(22 + u16::MAX as usize);
    let end = (lower..=bytes.len() - 22)
        .rev()
        .find(|&at| {
            bytes[at..at + 4] == *b"PK\x05\x06"
                && at
                    + 22
                    + u16::from_le_bytes(bytes[at + 20..at + 22].try_into().unwrap()) as usize
                    == bytes.len()
        })
        .context("missing exact ZIP32 end record")?;
    let u16at = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let u32at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    ensure!(
        u16at(end + 4) == 0 && u16at(end + 6) == 0,
        "multi-disk ZIP rejected"
    );
    let count = u16at(end + 10) as usize;
    ensure!(
        count > 0 && count <= MAX_FILES && u16at(end + 8) as usize == count,
        "archive member bound or split count differs"
    );
    let central_length = u32at(end + 12) as usize;
    let mut position = u32at(end + 16) as usize;
    ensure!(
        position.checked_add(central_length) == Some(end),
        "ZIP64, offset prefix or unaccounted central data rejected"
    );
    let mut inventory = Vec::new();
    let mut bounded = Vec::new();
    while position < end {
        ensure!(
            position.checked_add(46).is_some_and(|n| n <= end)
                && bytes[position..position + 4] == *b"PK\x01\x02",
            "invalid central directory entry"
        );
        ensure!(inventory.len() < MAX_FILES, "archive member bound exceeded");
        let flags = u16at(position + 8);
        let method = u16at(position + 10);
        ensure!(
            flags & (1 | 64) == 0 && matches!(method, 0 | 8),
            "encrypted or unsupported ZIP entry"
        );
        ensure!(u16at(position + 34) == 0, "multi-disk member rejected");
        let compressed = u32at(position + 20);
        let expanded = u32at(position + 24);
        ensure!(
            compressed != u32::MAX && expanded != u32::MAX && u32at(position + 42) != u32::MAX,
            "ZIP64 member rejected"
        );
        let name_len = u16at(position + 28) as usize;
        let extra_len = u16at(position + 30) as usize;
        let comment_len = u16at(position + 32) as usize;
        let next = position
            .checked_add(46 + name_len + extra_len + comment_len)
            .context("central entry overflow")?;
        ensure!(next <= end, "truncated central entry");
        let name = std::str::from_utf8(&bytes[position + 46..position + 46 + name_len])?.to_owned();
        let path = RelativePath::new(&name)?;
        let cap = if name == "job.json" || name == "params.json" {
            MAX_RECORD_BYTES as u64
        } else {
            MAX_PAYLOAD_BYTES
        };
        ensure!(
            expanded as u64 <= cap,
            "archive member expansion exceeds bound"
        );
        bounded.push(FileEntry {
            path,
            identity: BlobIdentity {
                sha256: Digest::of(b"metadata-only"),
                bytes: expanded as u64,
            },
        });
        inventory.push((name, expanded as u64));
        position = next;
    }
    ensure!(inventory.len() == count, "raw central entry count differs");
    validate_inventory(&bounded)?;
    Ok(inventory)
}

/// Check all names/metadata/closure and actual decompressed CRC before any
/// adapter creates a destination. Archive bytes and expanded bytes are bounded.
pub fn decode_members(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    let declared = central_inventory(bytes)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    ensure!(
        archive.offset() == 0 && archive.len() == declared.len(),
        "archive member view differs"
    );
    ensure!(!archive.has_overlapping_files()?, "archive data overlaps");
    let mut members = BTreeMap::new();
    for (index, (declared_name, declared_size)) in declared.iter().enumerate() {
        let mut file = archive.by_index(index)?;
        ensure!(!file.is_dir(), "explicit archive directory rejected");
        let mode = file.unix_mode().unwrap_or(0) & 0o170000;
        ensure!(
            mode == 0 || mode == 0o100000,
            "archive non-regular file rejected"
        );
        let name = std::str::from_utf8(file.name_raw())?.to_owned();
        ensure!(
            name == *declared_name && file.size() == *declared_size,
            "archive metadata interpretation differs"
        );
        let mut output = Vec::new();
        (&mut file)
            .take(declared_size + 1)
            .read_to_end(&mut output)?;
        ensure!(
            output.len() as u64 == *declared_size,
            "archive size mismatch"
        );
        insert(&mut members, &name, output)?;
    }
    Ok(members)
}

pub fn from_archive(bytes: &[u8]) -> Result<Package> {
    let members = decode_members(bytes)?;
    let job: JobManifest = decode(
        members
            .get("job.json")
            .context("missing archived manifest")?,
    )?;
    let package = Package { job, members };
    package.verify()?;
    Ok(package)
}
