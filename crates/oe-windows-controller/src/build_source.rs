//! Bounded source archive verification. This does not materialize a filesystem,
//! resolve Cargo dependencies, execute a compiler, or authenticate build records.
use crate::build::{MAX_BUILD_TREE_BYTES, MAX_BUILD_TREE_FILES};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    BlobIdentity, FileEntry, MAX_PAYLOAD_BYTES, MAX_RECORD_BYTES, RelativePath,
    compatibility_collision_key, decode, path_collision_key,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};
use zip::ZipArchive;

pub const SOURCE_SCHEMA: &str = "cad-agent.nx-worker-source-archive/v1";
pub const WORKSPACE_MEMBERS: [&str; 11] = [
    "oe-windows-controller",
    "oe-nx-worker",
    "oe-wire",
    "oe-nx-protocol",
    "oe-core",
    "oe-semantic",
    "oe-engineering",
    "oe-knowledge",
    "oe-browser",
    "oe-cad",
    "oe-cli",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceManifest {
    pub schema: String,
    /// Explicit archive prefix stripped only after every raw path is verified.
    pub archive_root: RelativePath,
    /// Every regular file, relative to archive_root; no implicit exclusions.
    pub files: Vec<FileEntry>,
}
impl SourceManifest {
    pub fn validate(&self) -> Result<u64> {
        ensure!(
            self.schema == SOURCE_SCHEMA,
            "source manifest schema differs"
        );
        ensure!(
            !self.archive_root.as_str().contains('/'),
            "source archive root must be one component"
        );
        build_inventory(&self.files)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAssessment {
    pub archive: BlobIdentity,
    pub manifest: BlobIdentity,
    pub cargo_lock: BlobIdentity,
    pub root_cargo_manifest: BlobIdentity,
    pub expanded_files: u32,
    pub expanded_bytes: u64,
    pub workspace_members: Vec<String>,
    pub lock_packages: usize,
    pub registry_packages: usize,
    pub archive_verification: String,
    pub filesystem_materialization: String,
    pub cargo_resolution: String,
    pub compilation_execution: String,
}
/// Private fields prevent callers from minting a verified archive by decoding
/// externally supplied assessments. The returned bytes remain unexecuted source.
pub struct VerifiedSourceArchive {
    members: BTreeMap<String, Vec<u8>>,
    assessment: SourceAssessment,
}
impl VerifiedSourceArchive {
    pub fn members(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.members
    }
    pub fn assessment(&self) -> &SourceAssessment {
        &self.assessment
    }
    pub fn into_members(self) -> BTreeMap<String, Vec<u8>> {
        self.members
    }
}

/// Compare actual ZIP members and decompressed bytes to a complete externally
/// pinned inventory, then bind the exact root lockfile and workspace structure.
pub fn verify_source_archive(
    archive_bytes: &[u8],
    manifest_bytes: &[u8],
    expected_lock: &[u8],
) -> Result<VerifiedSourceArchive> {
    let manifest: SourceManifest = decode(manifest_bytes)?;
    let declared_total = manifest.validate()?;
    ensure!(
        !expected_lock.is_empty() && expected_lock.len() <= MAX_RECORD_BYTES,
        "source lockfile bound exceeded"
    );
    let central = central_inventory(archive_bytes)?;
    ensure!(
        central.len() == manifest.files.len(),
        "source archive member count differs"
    );
    let prefix = format!("{}/", manifest.archive_root.as_str());
    let expected: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|v| (v.path.as_str(), v))
        .collect();
    let mut raw_names = BTreeSet::new();
    for (name, size) in &central {
        let relative = name
            .strip_prefix(&prefix)
            .context("source entry outside explicit root")?;
        let entry = expected
            .get(relative)
            .context("unlisted source archive member")?;
        ensure!(
            entry.identity.bytes == *size && raw_names.insert(relative),
            "source member metadata differs or duplicate"
        );
    }
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes))?;
    ensure!(
        archive.offset() == 0 && archive.len() == central.len(),
        "source ZIP index view differs"
    );
    ensure!(
        !archive.has_overlapping_files()?,
        "source ZIP data overlaps"
    );
    let mut members = BTreeMap::new();
    let mut actual_total = 0u64;
    for (index, (raw_name, size)) in central.iter().enumerate() {
        let mut file = archive.by_index(index)?;
        ensure!(
            !file.is_dir() && matches!(file.unix_mode().unwrap_or(0) & 0o170000, 0 | 0o100000),
            "source member is not a regular file"
        );
        ensure!(
            file.name_raw() == raw_name.as_bytes() && file.size() == *size,
            "source ZIP interpretation differs"
        );
        let mut bytes = Vec::new();
        (&mut file)
            .take(size.checked_add(1).context("source size overflow")?)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == *size,
            "source decompressed size differs"
        );
        actual_total = actual_total
            .checked_add(bytes.len() as u64)
            .context("source total overflow")?;
        ensure!(
            actual_total <= MAX_BUILD_TREE_BYTES,
            "source actual expansion exceeds bound"
        );
        let relative = raw_name
            .strip_prefix(&prefix)
            .context("source root differs")?;
        expected[relative].identity.verify(&bytes)?;
        ensure!(
            members.insert(relative.to_owned(), bytes).is_none(),
            "duplicate source member"
        );
    }
    ensure!(
        actual_total == declared_total && members.len() == manifest.files.len(),
        "source expanded closure differs"
    );
    let lock = members
        .get("Cargo.lock")
        .context("missing root Cargo.lock")?;
    ensure!(
        lock == expected_lock,
        "root Cargo.lock differs from separately pinned bytes"
    );
    let (lock_packages, registry_packages) = workspace_contract(&members)?;
    let assessment = SourceAssessment {
        archive: BlobIdentity::of(archive_bytes),
        manifest: BlobIdentity::of(manifest_bytes),
        cargo_lock: BlobIdentity::of(lock),
        root_cargo_manifest: BlobIdentity::of(&members["Cargo.toml"]),
        expanded_files: members.len().try_into()?,
        expanded_bytes: actual_total,
        workspace_members: WORKSPACE_MEMBERS
            .iter()
            .map(|name| format!("crates/{name}"))
            .collect(),
        lock_packages,
        registry_packages,
        archive_verification: "verified_names_crc_sizes_hashes_and_root_workspace_lock".into(),
        filesystem_materialization: "not_performed".into(),
        cargo_resolution: "not_executed".into(),
        compilation_execution: "not_executed".into(),
    };
    Ok(VerifiedSourceArchive {
        members,
        assessment,
    })
}

fn build_inventory(files: &[FileEntry]) -> Result<u64> {
    ensure!(
        !files.is_empty() && files.len() <= MAX_BUILD_TREE_FILES as usize,
        "source file count exceeds bound"
    );
    let mut canonical = BTreeSet::new();
    let mut compatible = BTreeSet::new();
    let mut total = 0u64;
    for file in files {
        ensure!(
            canonical.insert(path_collision_key(file.path.as_str())),
            "source case or Unicode alias"
        );
        ensure!(
            compatible.insert(compatibility_collision_key(file.path.as_str())),
            "source Windows compatibility alias"
        );
        total = total
            .checked_add(file.identity.bytes)
            .context("source byte overflow")?;
        ensure!(
            total <= MAX_BUILD_TREE_BYTES,
            "source declared expansion exceeds bound"
        );
    }
    for name in canonical.iter().chain(&compatible) {
        for (i, _) in name.match_indices('/') {
            ensure!(
                !canonical.contains(&name[..i]) && !compatible.contains(&name[..i]),
                "source file/directory collision"
            );
        }
    }
    Ok(total)
}

fn document(bytes: &[u8]) -> Result<toml::Value> {
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "Cargo metadata record exceeds bound"
    );
    Ok(toml::from_str(std::str::from_utf8(bytes)?)?)
}
fn workspace_contract(files: &BTreeMap<String, Vec<u8>>) -> Result<(usize, usize)> {
    let root = document(files.get("Cargo.toml").context("missing root Cargo.toml")?)?;
    ensure!(
        root.get("package").is_none(),
        "expected virtual root workspace"
    );
    let workspace = root
        .get("workspace")
        .and_then(toml::Value::as_table)
        .context("missing root workspace")?;
    ensure!(
        workspace.get("resolver").and_then(toml::Value::as_str) == Some("3"),
        "expected workspace resolver 3"
    );
    let members = workspace
        .get("members")
        .and_then(toml::Value::as_array)
        .context("missing explicit workspace members")?;
    let expected: BTreeSet<_> = WORKSPACE_MEMBERS
        .iter()
        .map(|v| format!("crates/{v}"))
        .collect();
    let mut actual = BTreeSet::new();
    for member in members {
        let path = member.as_str().context("non-string workspace member")?;
        RelativePath::new(path)?;
        ensure!(actual.insert(path.to_owned()), "duplicate workspace member");
    }
    ensure!(actual == expected, "root workspace member set differs");
    if let Some(exclude) = workspace.get("exclude") {
        ensure!(
            exclude.as_array().is_some_and(Vec::is_empty),
            "workspace exclusions unsupported"
        );
    }
    let workspace_version = workspace
        .get("package")
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str);
    let mut versions = BTreeMap::new();
    for name in WORKSPACE_MEMBERS {
        let path = format!("crates/{name}/Cargo.toml");
        let member = document(files.get(&path).context("missing member Cargo.toml")?)?;
        ensure!(
            member.get("workspace").is_none(),
            "nested member workspace unsupported"
        );
        let package = member
            .get("package")
            .context("member package declaration missing")?;
        ensure!(
            package.get("name").and_then(toml::Value::as_str) == Some(name),
            "member package name differs"
        );
        ensure!(
            package.get("workspace").is_none(),
            "member workspace override unsupported"
        );
        let version = package.get("version").context("member version missing")?;
        let version = if version.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
            workspace_version.context("inherited workspace version missing")?
        } else {
            version.as_str().context("invalid member version")?
        };
        versions.insert(name, version.to_owned());
    }
    let lock = document(&files["Cargo.lock"])?;
    ensure!(
        lock.get("version").and_then(toml::Value::as_integer) == Some(4),
        "expected Cargo lockfile version 4"
    );
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .context("missing lock packages")?;
    ensure!(
        packages.len() <= MAX_BUILD_TREE_FILES as usize,
        "lock package count exceeds bound"
    );
    let mut local = BTreeMap::new();
    let mut all = BTreeSet::new();
    let mut registry = 0;
    for package in packages {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .context("lock package name missing")?;
        let version = package
            .get("version")
            .and_then(toml::Value::as_str)
            .context("lock package version missing")?;
        let source = package
            .get("source")
            .map(|v| v.as_str().context("invalid lock source"))
            .transpose()?;
        ensure!(
            all.insert((name, version, source)),
            "duplicate lock package"
        );
        if let Some(source) = source {
            ensure!(
                source == "registry+https://github.com/rust-lang/crates.io-index",
                "non-crates.io lock source unsupported"
            );
            oe_nx_protocol::Digest::new(
                package
                    .get("checksum")
                    .and_then(toml::Value::as_str)
                    .context("registry checksum missing")?,
            )?;
            registry += 1;
        } else {
            ensure!(
                local.insert(name, version.to_owned()).is_none(),
                "duplicate local lock package"
            );
        }
    }
    ensure!(
        local == versions,
        "workspace and root lock local packages differ"
    );
    Ok((packages.len(), registry))
}

// ZIP32 preflight is deliberately independent of job payload limits. Reuses
// protocol path/Unicode rules, and checks raw central entries before zip's map
// could hide duplicate names. This source format permits no extra records,
// directories, encrypted/split/ZIP64 members, Unix links or Windows reparse bits.
fn central_inventory(bytes: &[u8]) -> Result<Vec<(String, u64)>> {
    ensure!(
        bytes.len() >= 22 && bytes.len() as u64 <= MAX_PAYLOAD_BYTES,
        "source ZIP compressed bound exceeded"
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
        .context("missing exact source ZIP32 end record")?;
    let u16at = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let u32at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    ensure!(
        u16at(end + 4) == 0 && u16at(end + 6) == 0,
        "source multi-disk ZIP rejected"
    );
    let count = u16at(end + 10) as usize;
    ensure!(
        count > 0 && count <= MAX_BUILD_TREE_FILES as usize && u16at(end + 8) as usize == count,
        "source ZIP member count differs"
    );
    let start = u32at(end + 16) as usize;
    ensure!(
        start.checked_add(u32at(end + 12) as usize) == Some(end),
        "source ZIP central offset differs"
    );
    let mut position = start;
    let mut declared = Vec::new();
    let mut bounded = Vec::new();
    while position < end {
        ensure!(
            position.checked_add(46).is_some_and(|n| n <= end)
                && bytes[position..position + 4] == *b"PK\x01\x02",
            "invalid source central header"
        );
        ensure!(
            declared.len() < MAX_BUILD_TREE_FILES as usize,
            "source ZIP member limit"
        );
        let flags = u16at(position + 8);
        let method = u16at(position + 10);
        ensure!(
            flags & (1 | 64) == 0 && matches!(method, 0 | 8),
            "unsupported source ZIP encryption or method"
        );
        ensure!(
            u16at(position + 34) == 0 && u16at(position + 30) == 0,
            "source ZIP extras or split entry rejected"
        );
        let attributes = u32at(position + 38);
        ensure!(
            attributes & 0x410 == 0 && matches!((attributes >> 16) & 0o170000, 0 | 0o100000),
            "source ZIP link/reparse/directory rejected"
        );
        let compressed = u32at(position + 20);
        let expanded = u32at(position + 24);
        let local = u32at(position + 42);
        ensure!(
            ![compressed, expanded, local].contains(&u32::MAX),
            "source ZIP64 entry rejected"
        );
        let name_len = u16at(position + 28) as usize;
        let next = position
            .checked_add(46 + name_len + u16at(position + 32) as usize)
            .context("source central length overflow")?;
        ensure!(next <= end, "truncated source central entry");
        let name = std::str::from_utf8(&bytes[position + 46..position + 46 + name_len])?.to_owned();
        let local = local as usize;
        ensure!(
            local.checked_add(30).is_some_and(|n| n <= start)
                && bytes[local..local + 4] == *b"PK\x03\x04",
            "invalid source local header"
        );
        ensure!(
            u16at(local + 6) == flags
                && u16at(local + 8) == method
                && u16at(local + 26) as usize == name_len
                && u16at(local + 28) == 0,
            "source local metadata differs"
        );
        let data = local
            .checked_add(30 + name_len)
            .context("source local offset overflow")?;
        ensure!(
            data.checked_add(compressed as usize)
                .is_some_and(|n| n <= start)
                && bytes[local + 30..data] == *name.as_bytes(),
            "source local name or data bound differs"
        );
        if flags & 8 == 0 {
            ensure!(
                u32at(local + 14) == u32at(position + 16)
                    && u32at(local + 18) == compressed
                    && u32at(local + 22) == expanded,
                "source local CRC/size differs"
            );
        }
        bounded.push(FileEntry {
            path: RelativePath::new(&name)?,
            identity: BlobIdentity {
                sha256: oe_nx_protocol::Digest::of(b"metadata-only"),
                bytes: expanded as u64,
            },
        });
        declared.push((name, expanded as u64));
        position = next;
    }
    ensure!(declared.len() == count, "source raw central count differs");
    build_inventory(&bounded)?;
    Ok(declared)
}
