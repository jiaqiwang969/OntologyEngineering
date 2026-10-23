//! Lossless, selected-member recovery. This module never extracts an archive tree.
use anyhow::{Context, Result, ensure};
use oe_core::{json::strict_json, sha256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_SEGMENT_MEMBERS: usize = 128;
const MAX_SHARD_BYTES: u64 = 34 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSource {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub source_file_page: u64,
}

pub(crate) fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn relative_path(value: &str) -> Result<&Path> {
    ensure!(
        !value.is_empty() && !value.contains(['\\', ':']) && !value.chars().any(char::is_control),
        "source_pack_invalid_path"
    );
    let p = Path::new(value);
    ensure!(
        !p.is_absolute()
            && value
                .split('/')
                .all(|x| !x.is_empty() && x != "." && x != ".."),
        "source_pack_invalid_path"
    );
    ensure!(
        p.components().all(|c| matches!(c, Component::Normal(_))),
        "source_pack_invalid_path"
    );
    Ok(p)
}

pub(crate) fn within_root(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = oe_core::SkillRoot::new(root)?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.path().join(path)
    };
    ensure!(
        path.starts_with(root.path()) && path != root.path(),
        "knowledge_path_outside_root"
    );
    oe_core::no_symlinks(&path)?;
    Ok(path)
}

pub(crate) fn file_hash(path: &Path, limit: u64) -> Result<(String, u64)> {
    let mut file = oe_core::open_regular_file(path, limit)?;
    hash_file(&mut file, limit)
}
fn hash_file(file: &mut File, limit: u64) -> Result<(String, u64)> {
    let before = file.metadata()?;
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .context("source_byte_overflow")?;
        ensure!(count <= limit, "source_exceeds_limit");
        hash.update(&buffer[..n]);
    }
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len()
            && before.modified()? == after.modified()?
            && count == after.len(),
        "source_changed_during_read"
    );
    Ok((format!("{:x}", hash.finalize()), count))
}

pub(crate) struct Stage {
    pub path: PathBuf,
}
impl Stage {
    pub fn new(root: &Path, label: &str) -> Result<Self> {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let t = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = oe_core::SkillRoot::new(root)?.new_private_dir(Path::new(&format!(
            "var/cache/knowledge/.{label}-{}-{t}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        )))?;
        Ok(Self { path })
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Clone)]
struct Entry {
    path: String,
    hash: String,
    size: u64,
}
fn entry(value: &Value, max: u64) -> Result<Entry> {
    let path = value["path"].as_str().context("source_pack_invalid_file")?;
    relative_path(path)?;
    let hash = value["sha256"]
        .as_str()
        .context("source_pack_invalid_file_identity")?;
    let size = value["bytes"]
        .as_u64()
        .context("source_pack_invalid_file_identity")?;
    ensure!(
        valid_sha(hash) && size <= max,
        "source_pack_invalid_file_identity"
    );
    Ok(Entry {
        path: path.to_owned(),
        hash: hash.to_owned(),
        size,
    })
}

/// Verify all declared paths and identities before selecting a shard. Unknown
/// optional metadata is retained; duplicate JSON keys are rejected by the reader.
pub fn load_manifest(root: &Path, manifest: &Path) -> Result<Value> {
    let path = within_root(root, manifest)?;
    let raw = oe_core::read_regular_bytes(&path, MAX_MANIFEST_BYTES)?;
    validate_manifest_value(strict_json(&raw)?)
}

fn validate_manifest_value(document: Value) -> Result<Value> {
    ensure!(
        document["schema"] == "ontology-engineering.misumi-source-pack/v1",
        "source_pack_schema_mismatch"
    );
    ensure!(
        entry(&document["index"], MAX_SHARD_BYTES)?.path == "catalog.sqlite3",
        "source_pack_index_path_mismatch"
    );
    let segments = document["segments"]
        .as_array()
        .context("source_pack_empty_segments")?;
    ensure!(!segments.is_empty(), "source_pack_empty_segments");
    let (mut ids, mut paths, mut members) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for segment in segments {
        let shard = entry(segment, MAX_SHARD_BYTES)?;
        let id = segment["id"]
            .as_str()
            .context("source_pack_invalid_segment")?;
        let files = segment["files"]
            .as_array()
            .context("source_pack_invalid_segment")?;
        ensure!(
            !id.is_empty()
                && ids.insert(id)
                && paths.insert(shard.path.clone())
                && shard.path.starts_with("segments/")
                && shard.path.ends_with(".tar.xz")
                && (1..=MAX_SEGMENT_MEMBERS).contains(&files.len()),
            "source_pack_invalid_segment"
        );
        for file in files {
            ensure!(
                members.insert(entry(file, MAX_SOURCE_BYTES)?.path),
                "source_pack_duplicate_member"
            );
        }
    }
    if let Some(assembled) = document.get("assembled_pdfs") {
        let mut seen = BTreeSet::new();
        for a in assembled
            .as_array()
            .context("source_pack_invalid_assembled_inventory")?
        {
            ensure!(
                seen.insert(entry(a, MAX_SHARD_BYTES)?.path) && a["catalog"].as_str().is_some(),
                "source_pack_invalid_assembled_inventory"
            );
            if let Some(url) = a.get("download_url") {
                ensure!(
                    url.is_null() || url.as_str().is_some_and(|s| s.starts_with("https://")),
                    "source_pack_invalid_download_url"
                );
            }
        }
    }
    Ok(document)
}

struct Limited<R> {
    inner: R,
    bytes: u64,
    limit: u64,
    read_time: Duration,
}
impl<R: Read> Read for Limited<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.read_time > Duration::from_secs(120) {
            return Err(std::io::Error::other("source_archive_timeout"));
        }
        let started = Instant::now();
        let result = self.inner.read(out);
        self.read_time += started.elapsed();
        let n = result?;
        self.bytes = self.bytes.saturating_add(n as u64);
        if self.bytes > self.limit {
            return Err(std::io::Error::other("source_archive_expansion_limit"));
        }
        Ok(n)
    }
}

/// Bounds are supplied by the source operation: page shards, metadata and full
/// books have different sizes. Callback work does not consume the decoder's read
/// time budget; PDF processing has its own independently enforced process limit.
#[derive(Debug, Clone)]
pub struct ArchiveLimits {
    pub max_file_bytes: u64,
    pub max_members: usize,
    pub max_uncompressed_bytes: u64,
}

impl ArchiveLimits {
    pub fn page_shard() -> Self {
        Self {
            max_file_bytes: MAX_SOURCE_BYTES,
            max_members: MAX_SEGMENT_MEMBERS,
            max_uncompressed_bytes: MAX_SHARD_BYTES,
        }
    }
}

/// The path exists only during the callback. Callers may read or copy it into
/// their private transaction; they must not mutate, move or publish it yet.
#[derive(Debug, Clone)]
pub struct VerifiedMember {
    pub relative_path: String,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveVisit {
    pub archive_sha256: String,
    pub compressed_bytes: u64,
    pub members_verified: usize,
    pub uncompressed_bytes: u64,
    pub members_visited: usize,
}

/// Stream one declared archive once, retaining at most one selected member on
/// disk. All members are hashed, including unselected ones. A successful callback
/// is provisional: only an Ok return verifies the complete inventory, XZ footer,
/// trailing data and unchanged archive identity. Callers commit after that point.
pub fn visit_archive(
    root: &Path,
    archive_path: &Path,
    descriptor: &Value,
    limits: &ArchiveLimits,
    selected: Option<&BTreeSet<String>>,
    mut visitor: impl FnMut(&VerifiedMember) -> Result<()>,
) -> Result<ArchiveVisit> {
    ensure!(
        limits.max_file_bytes > 0
            && limits.max_file_bytes <= MAX_SHARD_BYTES
            && (1..=65_536).contains(&limits.max_members)
            && limits.max_uncompressed_bytes > 0
            && limits.max_uncompressed_bytes <= MAX_SHARD_BYTES,
        "source_archive_limits_invalid"
    );
    let shard = entry(descriptor, MAX_SHARD_BYTES)?;
    let files = descriptor["files"]
        .as_array()
        .context("source_archive_inventory_missing")?;
    ensure!(
        !files.is_empty() && files.len() <= limits.max_members,
        "source_archive_member_limit"
    );
    let mut declared = BTreeMap::new();
    let mut total = 0_u64;
    for value in files {
        let member = entry(value, limits.max_file_bytes)?;
        total = total
            .checked_add(member.size)
            .context("source_archive_size_overflow")?;
        ensure!(
            declared.insert(member.path.clone(), member).is_none(),
            "source_pack_duplicate_member"
        );
    }
    ensure!(
        total <= limits.max_uncompressed_bytes,
        "source_archive_expansion_limit"
    );
    if let Some(selected) = selected {
        ensure!(
            selected.iter().all(|p| declared.contains_key(p)),
            "source_archive_selection_not_declared"
        );
    }
    let shard_path = within_root(root, archive_path)?;
    let mut input = oe_core::open_regular_file(&shard_path, shard.size)?;
    ensure!(
        hash_file(&mut input, shard.size)? == (shard.hash.clone(), shard.size),
        "source_segment_hash_mismatch"
    );
    input.seek(SeekFrom::Start(0))?;
    let expansion = declared.values().try_fold(10240_u64, |sum, e| {
        sum.checked_add(e.size.div_ceil(512) * 512 + 8192)
            .context("source_archive_size_overflow")
    })?;
    let stream = xz2::stream::Stream::new_stream_decoder(128 * 1024 * 1024, 0)?;
    let decoder = xz2::read::XzDecoder::new_stream(input, stream);
    let limited = Limited {
        inner: decoder,
        bytes: 0,
        limit: expansion,
        read_time: Duration::ZERO,
    };
    let mut archive = tar::Archive::new(limited);
    let stage = Stage::new(root, "archive-visit")?;
    let mut seen = BTreeSet::new();
    let mut visited = 0;
    for item in archive.entries()? {
        let mut item = item?;
        let name = std::str::from_utf8(&item.path_bytes())?.to_owned();
        relative_path(&name)?;
        let expected = declared
            .get(&name)
            .context("source_segment_undeclared_member")?;
        ensure!(
            seen.insert(name.clone())
                && item.header().entry_type().is_file()
                && item.size() == expected.size
                && item.link_name()?.is_none(),
            "source_segment_member_mismatch"
        );
        if let Some(extensions) = item.pax_extensions()? {
            for ext in extensions {
                ensure!(
                    !ext?.key()?.starts_with("GNU.sparse"),
                    "source_segment_sparse_member"
                );
            }
        }
        let path = stage.path.join("member");
        let mut output = if selected.is_none_or(|s| s.contains(&name)) {
            Some(oe_core::create_new_regular_file(&path)?)
        } else {
            None
        };
        let mut hash = Sha256::new();
        let mut count = 0_u64;
        let mut buffer = [0; 65536];
        loop {
            let n = item.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(n as u64)
                .context("source_archive_size_overflow")?;
            ensure!(
                count <= expected.size,
                "source_segment_member_size_mismatch"
            );
            hash.update(&buffer[..n]);
            if let Some(output) = &mut output {
                output.write_all(&buffer[..n])?;
            }
        }
        ensure!(
            count == expected.size && format!("{:x}", hash.finalize()) == expected.hash,
            "source_restored_page_hash_mismatch"
        );
        if let Some(output) = output {
            output.sync_all()?;
            drop(output);
            visitor(&VerifiedMember {
                relative_path: name,
                path: path.clone(),
                sha256: expected.hash.clone(),
                bytes: expected.size,
            })?;
            ensure!(
                file_hash(&path, expected.size)? == (expected.hash.clone(), expected.size),
                "source_archive_member_changed_by_visitor"
            );
            fs::remove_file(path)?;
            visited += 1;
        }
    }
    ensure!(
        seen == declared.keys().cloned().collect(),
        "source_segment_inventory_mismatch"
    );
    let mut reader = archive.into_inner();
    let mut tail = [0; 65536];
    loop {
        let n = reader.read(&mut tail)?;
        if n == 0 {
            break;
        }
        ensure!(
            tail[..n].iter().all(|x| *x == 0),
            "source_archive_trailing_payload"
        );
    }
    ensure!(
        reader.inner.total_in() == shard.size,
        "source_archive_trailing_compressed_bytes"
    );
    let mut input = reader.inner.into_inner();
    input.seek(SeekFrom::Start(0))?;
    ensure!(
        hash_file(&mut input, shard.size)? == (shard.hash.clone(), shard.size),
        "source_segment_changed_during_restore"
    );
    ensure!(
        file_hash(&shard_path, shard.size)? == (shard.hash.clone(), shard.size),
        "source_segment_replaced_during_restore"
    );
    Ok(ArchiveVisit {
        archive_sha256: shard.hash,
        compressed_bytes: shard.size,
        members_verified: seen.len(),
        uncompressed_bytes: total,
        members_visited: visited,
    })
}

pub fn restore_selected(
    root: &Path,
    manifest: &Path,
    relative: &str,
    expected_sha: &str,
) -> Result<ResolvedSource> {
    relative_path(relative)?;
    ensure!(
        valid_sha(expected_sha),
        "source_pack_page_identity_mismatch"
    );
    let manifest_path = within_root(root, manifest)?;
    let original = oe_core::read_regular_bytes(&manifest_path, MAX_MANIFEST_BYTES)?;
    let document = validate_manifest_value(strict_json(&original)?)?;
    ensure!(
        oe_core::read_regular_bytes(&manifest_path, MAX_MANIFEST_BYTES)? == original,
        "source_pack_manifest_changed"
    );
    let mut found = None;
    for segment in document["segments"].as_array().expect("validated") {
        for f in segment["files"].as_array().expect("validated") {
            if f["path"] == relative {
                found = Some((segment, entry(f, MAX_SOURCE_BYTES)?));
            }
        }
    }
    let (segment, wanted) = found.context("source_pack_page_identity_mismatch")?;
    ensure!(
        wanted.hash == expected_sha,
        "source_pack_page_identity_mismatch"
    );
    let shard = entry(segment, MAX_SHARD_BYTES)?;
    let shard_path = within_root(
        root,
        &manifest_path
            .parent()
            .context("source_pack_parent")?
            .join(&shard.path),
    )?;
    let stage = Stage::new(root, "restore")?;
    let stage_file = stage.path.join("selected.pdf");
    let selected = BTreeSet::from([relative.to_owned()]);
    visit_archive(
        root,
        &shard_path,
        segment,
        &ArchiveLimits::page_shard(),
        Some(&selected),
        |member| publish_same_or_new(&member.path, &stage_file, &member.sha256, member.bytes),
    )?;
    ensure!(
        oe_core::read_regular_bytes(&manifest_path, MAX_MANIFEST_BYTES)? == original,
        "source_pack_manifest_changed"
    );
    let target = within_root(
        root,
        &PathBuf::from(format!(
            "var/cache/knowledge/source-pages/{}/{}",
            sha256(&original),
            relative
        )),
    )?;
    publish_same_or_new(&stage_file, &target, expected_sha, wanted.size)?;
    Ok(ResolvedSource {
        path: target,
        sha256: expected_sha.to_owned(),
        bytes: wanted.size,
        source_file_page: 1,
    })
}

pub(crate) fn publish_same_or_new(stage: &Path, target: &Path, sha: &str, size: u64) -> Result<()> {
    // The core boundary links a verified regular staged file into an anchored
    // destination without replacing it. Existing cache bytes must match exactly.
    match publish_file_no_replace(stage, target) {
        Ok(()) => {}
        Err(e) => {
            if !target.exists() {
                return Err(e);
            }
            ensure!(
                file_hash(target, size)? == (sha.to_owned(), size),
                "source_cache_conflict"
            );
        }
    }
    ensure!(
        file_hash(target, size)? == (sha.to_owned(), size),
        "source_cache_conflict"
    );
    Ok(())
}

#[cfg(unix)]
pub(crate) fn directory_fd(path: &Path, create: bool) -> Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    ensure!(path.is_absolute(), "absolute_directory_required");
    let fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    ensure!(fd >= 0, "directory_open_failed");
    let mut directory = unsafe { File::from_raw_fd(fd) };
    for c in path.components() {
        let Component::Normal(name) = c else {
            ensure!(matches!(c, Component::RootDir), "directory_traversal");
            continue;
        };
        let name = std::ffi::CString::new(name.as_bytes())?;
        if create {
            let r = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
            if r != 0 {
                let e = std::io::Error::last_os_error();
                ensure!(
                    e.raw_os_error() == Some(libc::EEXIST),
                    "directory_create_failed: {e}"
                );
            }
        }
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        ensure!(fd >= 0, "directory_open_failed");
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(directory)
}
#[cfg(unix)]
pub(crate) fn publish_file_no_replace(stage: &Path, target: &Path) -> Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let old = directory_fd(stage.parent().context("stage_parent")?, false)?;
    let new = directory_fd(target.parent().context("cache_parent")?, true)?;
    let old_name = std::ffi::CString::new(stage.file_name().context("stage_name")?.as_bytes())?;
    let new_name = std::ffi::CString::new(target.file_name().context("cache_name")?.as_bytes())?;
    let result = unsafe {
        libc::linkat(
            old.as_raw_fd(),
            old_name.as_ptr(),
            new.as_raw_fd(),
            new_name.as_ptr(),
            0,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("source_cache_publish_failed");
    };
    Ok(())
}
#[cfg(not(unix))]
pub(crate) fn publish_file_no_replace(_stage: &Path, _target: &Path) -> Result<()> {
    anyhow::bail!("safe_cache_publication_requires_unix")
}
