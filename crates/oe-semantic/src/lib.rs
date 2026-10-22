//! Verified historical package transport and source-bound review projections.
//!
//! Formal v2 evaluation is provided by oe-kernel through the injected executor.
//! This crate retains transport, input provenance, authority checks and orchestration;
//! premise snapshots and completed requests do not establish physical acceptance.

pub mod book_release;
pub mod book_sources;
pub mod books;
pub mod bootstrap;
pub mod chapter_contracts;
pub mod contracts;
pub mod delta;
pub mod lifecycle;
pub mod mcp;
pub mod native;
pub mod rebind_books;
pub mod refinery_runtime;
pub mod refinery_storage;
pub mod release_evidence;
pub mod release_sources;
pub mod review;
pub mod runtime;
pub mod runtime_build;
pub mod runtime_install;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(windows))]
use std::fs::File;
use std::io::{Cursor, Read};
#[cfg(not(windows))]
use std::path::Component;
use std::path::Path;
#[cfg(not(any(unix, windows)))]
use std::{fs, path::PathBuf};

pub const LOCK_PATH: &str = "runtime/semantic-bundles.json";
pub const RUNTIME_LOCK_PATH: &str = "runtime/semantica-source-lock.json";
pub const PREMISES_SCHEMA: &str = "ontology-engineering.semantic-premises/v1";
pub const MAX_EXPANDED_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SELECTIONS: usize = 8;
pub const MAX_ASSETS: usize = 32;
pub const MAX_PREMISE_BYTES: usize = 64 * 1024;

pub type Payload = BTreeMap<String, Vec<u8>>;

/// A transport-validated package, not an installed or adopted semantic package.
#[derive(Debug, Clone)]
pub struct LoadedBundle {
    pub spec: Value,
    pub payload: Payload,
    pub manifest: Value,
    pub report: Value,
}

pub use oe_core::sha256 as digest;

/// Accept only a literal, nonempty POSIX relative path, without normalization.
pub fn safe_relative(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty() && !value.contains('\\'),
        "invalid relative path"
    );
    ensure!(
        !value.starts_with('/')
            && !value
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".."),
        "path must remain inside its package"
    );
    ensure!(
        !value.contains(':') && !value.chars().any(|c| c < '\u{20}'),
        "invalid path characters"
    );
    Ok(value)
}

#[cfg(not(any(unix, windows)))]
fn regular_inside(root: &Path, relative: &str) -> Result<PathBuf> {
    safe_relative(relative)?;
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let mut cursor = PathBuf::new();
    // Do not canonicalize away a symlink before checking it. The trusted root
    // can be '.' but it cannot contain parent traversal or symlink ancestors.
    for part in absolute.components() {
        match part {
            Component::CurDir => continue,
            Component::ParentDir => bail!("skill root contains parent traversal"),
            _ => cursor.push(part.as_os_str()),
        }
        let metadata = fs::symlink_metadata(&cursor).context("skill root is unavailable")?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "bundle path contains a symbolic link"
        );
        ensure!(metadata.is_dir(), "skill root must be a directory");
    }
    for (index, part) in relative.split('/').enumerate() {
        cursor.push(part);
        let metadata = fs::symlink_metadata(&cursor).context("declared bundle file is missing")?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "bundle path contains a symbolic link"
        );
        if index + 1 == relative.split('/').count() {
            ensure!(metadata.is_file(), "declared bundle file must be regular");
        } else {
            ensure!(metadata.is_dir(), "bundle path parent must be a directory");
        }
    }
    Ok(cursor)
}

#[cfg(windows)]
fn read_regular(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>> {
    use oe_windows_controller::windows::StableRoot;
    use std::ffi::OsStr;
    safe_relative(relative)?;
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let selected = StableRoot::open(&absolute, None).context("windows_bundle_root_unavailable")?;
    let mut directory = selected.as_directory()?;
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            directory = directory
                .directory(OsStr::new(part))
                .context("windows_bundle_parent_unavailable")?;
        } else {
            let mut file = directory
                .pin_file(OsStr::new(part), limit as u64)
                .context("windows_bundle_file_unsafe_or_unavailable")?
                .context("windows_bundle_file_missing")?;
            return file
                .read_all(limit as u64)
                .context("windows_bundle_read_failed");
        }
    }
    bail!("windows_bundle_file_missing")
}

#[cfg(not(any(unix, windows)))]
fn read_regular(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>> {
    let path = regular_inside(root, relative)?;
    let before = fs::symlink_metadata(&path)?;
    ensure!(
        before.len() <= limit as u64,
        "bundle input exceeds byte limit"
    );
    let mut options = File::options();
    options.read(true);
    // The final component is opened without following a link; parents are
    // checked before and after the read. Nothing is materialized or executed.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100); // O_NOFOLLOW
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000); // O_NOFOLLOW
    }
    let file = options
        .open(&path)
        .context("cannot open locked bundle input")?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file() && opened.len() <= limit as u64,
        "bundle input must be a bounded regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            before.dev() == opened.dev() && before.ino() == opened.ino(),
            "bundle input changed while opening"
        );
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "bundle input exceeds byte limit");
    let after = fs::symlink_metadata(regular_inside(root, relative)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            opened.dev() == after.dev() && opened.ino() == after.ino(),
            "bundle input changed while reading"
        );
    }
    ensure!(
        after.len() == bytes.len() as u64,
        "bundle input changed while reading"
    );
    Ok(bytes)
}

#[cfg(unix)]
fn read_regular(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>> {
    use std::ffi::{CString, OsStr};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    fn child(parent: &File, name: &OsStr, directory: bool) -> Result<File> {
        let name = CString::new(name.as_bytes()).context("invalid NUL in bundle path")?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            };
        // SAFETY: parent owns its fd, name is NUL-terminated, and a successful
        // open returns a new owned fd, immediately transferred into File.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error())
                .context("bundle input is missing, non-regular or contains a symbolic link");
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    safe_relative(relative)?;
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    // Walk from an open filesystem-root descriptor. Every subsequent lookup is
    // relative to its verified parent fd, so replacing an ancestor with a
    // symlink between checks cannot redirect a read outside the selected root.
    let mut current = File::open("/")?;
    for part in absolute.components() {
        match part {
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => bail!("skill root contains parent traversal"),
            Component::Normal(name) => current = child(&current, name, true)?,
            Component::Prefix(_) => bail!("invalid skill root prefix"),
        }
    }
    let mut components = relative.split('/').peekable();
    while let Some(name) = components.next() {
        current = child(&current, OsStr::new(name), components.peek().is_some())?;
    }
    let metadata = current.metadata()?;
    ensure!(metadata.is_file(), "declared bundle file must be regular");
    ensure!(
        metadata.len() <= limit as u64,
        "bundle input exceeds byte limit"
    );
    let mut bytes = Vec::new();
    current.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "bundle input exceeds byte limit");
    Ok(bytes)
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing or invalid string: {key}"))
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("missing or invalid array: {key}"))
}

fn count(value: &Value, key: &str) -> Result<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .with_context(|| format!("missing or invalid count: {key}"))
}

fn sha_field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let text = string(value, key)?;
    ensure!(
        text.len() == 64
            && text
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "{key} must be a lowercase SHA-256"
    );
    Ok(text)
}

/// Shared safe JSON parser; no package-controlled key text enters diagnostics.
pub fn strict_json(raw: &[u8]) -> Result<Value> {
    Ok(oe_core::json::strict_json(raw)?)
}

#[derive(Debug)]
struct ZipMember {
    name: String,
    size: u32,
}

fn word(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .context("truncated ZIP metadata")?;
    Ok(u16::from_le_bytes(bytes.try_into().expect("two bytes")))
}

fn dword(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .context("truncated ZIP metadata")?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

// Check the raw central inventory before the ZIP library can coalesce duplicate
// names into its lookup map. Frozen bounded packages do not require ZIP64,
// split archives, executable preambles, or encrypted entries.
fn zip_inventory(data: &[u8]) -> Result<Vec<ZipMember>> {
    ensure!(
        data.len() >= 22 && data.len() <= MAX_ARCHIVE_BYTES,
        "invalid or oversized ZIP container"
    );
    let start = data.len().saturating_sub(22 + u16::MAX as usize);
    let eocd = (start..=data.len() - 22)
        .rev()
        .find(|&pos| {
            data[pos..pos + 4] == *b"PK\x05\x06"
                && pos + 22 + u16::from_le_bytes([data[pos + 20], data[pos + 21]]) as usize
                    == data.len()
        })
        .context("missing ZIP end record or trailing container bytes")?;
    ensure!(
        word(data, eocd + 4)? == 0 && word(data, eocd + 6)? == 0,
        "split ZIP containers are not supported"
    );
    let entries = word(data, eocd + 10)?;
    let central_size = dword(data, eocd + 12)?;
    let central_start = dword(data, eocd + 16)?;
    ensure!(
        entries != u16::MAX && central_size != u32::MAX && central_start != u32::MAX,
        "ZIP64 exceeds the bounded transport contract"
    );
    ensure!(
        word(data, eocd + 8)? == entries,
        "split ZIP inventory mismatch"
    );
    ensure!(
        central_start as u64 + central_size as u64 == eocd as u64,
        "invalid ZIP central directory extent"
    );
    let mut cursor = central_start as usize;
    let mut names = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut expanded = 0u64;
    let mut result = Vec::new();
    for _ in 0..entries {
        ensure!(
            dword(data, cursor)? == 0x02014b50,
            "invalid ZIP central directory entry"
        );
        let flags = word(data, cursor + 8)?;
        let compression = word(data, cursor + 10)?;
        ensure!(flags & 0x41 == 0, "encrypted ZIP members are not allowed");
        ensure!(
            compression == 0 || compression == 8,
            "only stored or deflated data members are allowed"
        );
        let compressed = dword(data, cursor + 20)?;
        let size = dword(data, cursor + 24)?;
        let name_size = word(data, cursor + 28)? as usize;
        let extra_size = word(data, cursor + 30)? as usize;
        let comment_size = word(data, cursor + 32)? as usize;
        ensure!(
            word(data, cursor + 34)? == 0,
            "split ZIP member is not allowed"
        );
        let kind = (dword(data, cursor + 38)? >> 16) & 0o170000;
        ensure!(
            kind == 0 || kind == 0o100000,
            "bundle must contain regular files only"
        );
        let local_offset = dword(data, cursor + 42)?;
        ensure!(
            compressed != u32::MAX && size != u32::MAX && local_offset != u32::MAX,
            "ZIP64 member exceeds bounded transport contract"
        );
        let next = cursor
            .checked_add(46 + name_size + extra_size + comment_size)
            .context("ZIP metadata size overflow")?;
        ensure!(next <= eocd, "ZIP member extends outside central directory");
        let name_bytes = data
            .get(cursor + 46..cursor + 46 + name_size)
            .context("truncated ZIP name")?;
        let name = std::str::from_utf8(name_bytes).context("ZIP member name must be UTF-8")?;
        safe_relative(name)?;
        ensure!(names.insert(name.to_owned()), "duplicate archive member");
        expanded += size as u64;
        ensure!(
            expanded <= MAX_EXPANDED_BYTES as u64,
            "bundle exceeds expanded-size limit"
        );
        let local = local_offset as usize;
        ensure!(
            dword(data, local)? == 0x04034b50,
            "invalid ZIP local header"
        );
        ensure!(
            word(data, local + 6)? == flags && word(data, local + 8)? == compression,
            "ZIP local and central metadata differ"
        );
        let local_name_size = word(data, local + 26)? as usize;
        let local_extra_size = word(data, local + 28)? as usize;
        ensure!(
            data.get(local + 30..local + 30 + local_name_size) == Some(name_bytes),
            "ZIP local and central names differ"
        );
        let data_end = (local as u64)
            + 30
            + local_name_size as u64
            + local_extra_size as u64
            + compressed as u64;
        ensure!(
            data_end <= central_start as u64,
            "ZIP member overlaps central directory"
        );
        ranges.push((local, data_end as usize));
        result.push(ZipMember {
            name: name.to_owned(),
            size,
        });
        cursor = next;
    }
    ensure!(cursor == eocd, "ZIP central inventory size mismatch");
    ranges.sort_unstable();
    ensure!(
        ranges.first().is_none_or(|range| range.0 == 0),
        "ZIP executable preamble is not allowed"
    );
    ensure!(
        ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0),
        "overlapping ZIP members"
    );
    Ok(result)
}

/// Verify a data-only ZIP against the entire transport inventory, without extraction.
pub fn validate_data_archive(data: &[u8], spec: &Value) -> Result<Payload> {
    ensure!(
        data.len() <= MAX_ARCHIVE_BYTES,
        "bundle exceeds container-size limit"
    );
    ensure!(
        digest(data) == sha_field(spec, "sha256")?,
        "bundle hash mismatch"
    );
    let listed = array(spec, "files")?;
    let mut expected = BTreeMap::new();
    for row in listed {
        let name = safe_relative(string(row, "path")?)?;
        ensure!(
            expected.insert(name, sha_field(row, "sha256")?).is_none(),
            "duplicate locked file"
        );
    }
    let inventory = zip_inventory(data)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(data)).context("invalid ZIP container")?;
    ensure!(
        archive.len() == inventory.len(),
        "ZIP library and raw archive inventories differ"
    );
    let mut payload = Payload::new();
    for (index, member) in inventory.iter().enumerate() {
        let expected_hash = expected
            .get(member.name.as_str())
            .context("unlisted archive member")?;
        let extension = Path::new(&member.name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        ensure!(
            matches!(extension, "json" | "ttl" | "rq" | "facts" | "rules"),
            "transport contains a non-data asset"
        );
        let mut entry = archive.by_index(index).context("invalid ZIP member")?;
        ensure!(
            entry.name_raw() == member.name.as_bytes() && entry.size() == member.size as u64,
            "ZIP member metadata mismatch"
        );
        ensure!(!entry.is_dir(), "bundle must contain regular files only");
        let mut bytes = Vec::with_capacity(member.size as usize);
        (&mut entry)
            .take(member.size as u64 + 1)
            .read_to_end(&mut bytes)
            .context("cannot decode ZIP data member")?;
        ensure!(
            bytes.len() == member.size as usize,
            "ZIP expanded member size mismatch"
        );
        ensure!(digest(&bytes) == **expected_hash, "member hash mismatch");
        payload.insert(member.name.clone(), bytes);
    }
    ensure!(
        payload.len() == expected.len(),
        "incomplete bundle inventory"
    );
    Ok(payload)
}

/// Also verify native package identity, manifest assets, scenarios and origin.
pub fn validate_archive(data: &[u8], spec: &Value) -> Result<LoadedBundle> {
    let payload = validate_data_archive(data, spec)?;
    let manifest_name = safe_relative(string(spec, "manifest")?)?;
    let manifest_bytes = payload
        .get(manifest_name)
        .context("locked manifest is missing")?;
    ensure!(
        digest(manifest_bytes) == sha_field(spec, "manifest_sha256")?,
        "manifest hash mismatch"
    );
    let manifest = strict_json(manifest_bytes)?;
    ensure!(
        string(&manifest, "package_id")? == string(spec, "package_id")?
            && string(&manifest, "version")? == string(spec, "package_version")?,
        "package identity mismatch"
    );
    let declared = array(&manifest, "assets")?;
    ensure!(
        declared.len() == count(spec, "asset_count")?,
        "asset count mismatch"
    );
    let mut assets = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for asset in declared {
        let path = safe_relative(string(asset, "path")?)?;
        ensure!(
            assets.insert(path, asset).is_none() && ids.insert(string(asset, "asset_id")?),
            "duplicate native asset"
        );
        for field in ["asset_id", "kind", "role", "format"] {
            ensure!(
                !string(asset, field)?.is_empty(),
                "empty native asset field: {field}"
            );
        }
    }
    let mut native_inventory: BTreeSet<&str> = assets.keys().copied().collect();
    native_inventory.insert(manifest_name);
    native_inventory.insert("origin.json");
    ensure!(
        native_inventory == payload.keys().map(String::as_str).collect(),
        "transport and native asset inventories differ"
    );
    for (path, asset) in assets {
        ensure!(
            digest(&payload[path]) == sha_field(asset, "sha256")?,
            "native asset hash mismatch"
        );
    }
    let registry_path = safe_relative(string(spec, "scenario_registry")?)?;
    let registry = strict_json(
        payload
            .get(registry_path)
            .context("scenario registry is missing")?,
    )?;
    ensure!(
        string(&registry, "package_id")? == string(spec, "package_id")?,
        "scenario registry identity mismatch"
    );
    let mut scenarios = Vec::new();
    let mut seen = BTreeSet::new();
    for scenario in array(&registry, "scenarios")? {
        let id = string(scenario, "id")?;
        ensure!(seen.insert(id), "scenario inventory mismatch");
        scenarios.push(id.to_owned());
    }
    ensure!(
        scenarios.len() == count(spec, "scenario_count")?,
        "scenario inventory mismatch"
    );
    let origin = strict_json(&payload["origin.json"])?;
    ensure!(
        sha_field(&origin, "native_candidate_sha256")? == sha_field(spec, "candidate_sha256")?
            && sha_field(&origin, "native_package_sha256")? == sha_field(spec, "package_sha256")?,
        "native origin digest mismatch"
    );
    let report = json!({
        "bundle_sha256": spec["sha256"], "package_id": spec["package_id"],
        "package_version": spec["package_version"], "assets": declared.len(),
        "files": payload.len(), "scenarios": scenarios,
        "scope": "Frozen synthetic inputs; not registry installation or project authorization."
    });
    Ok(LoadedBundle {
        spec: spec.clone(),
        payload,
        manifest,
        report,
    })
}

/// Load a named, runtime-bound bundle from an explicitly supplied skill root.
pub fn load_bundle(root: &Path, name: &str) -> Result<LoadedBundle> {
    // Private staged versions retain the exact same archive validation and runtime
    // origin identity. Loading is transport only, never promotion or adoption.
    let owned;
    let spec = if let Some(path) = name.strip_prefix("private:") {
        safe_relative(path)?;
        let skill = oe_core::SkillRoot::new(root)?;
        let path = skill.private_path(Path::new(path))?;
        owned = strict_json(&oe_core::read_regular_bytes(&path, MAX_EXPANDED_BYTES)?)?;
        &owned
    } else {
        let lock = strict_json(&read_regular(root, LOCK_PATH, MAX_EXPANDED_BYTES)?)?;
        ensure!(
            string(&lock, "schema_version")? == "ontology-engineering.semantica-bundle-lock/v1",
            "unknown bundle lock schema"
        );
        owned = lock
            .get("bundles")
            .and_then(Value::as_object)
            .and_then(|b| b.get(name))
            .context("bundle is not declared in the skill")?
            .clone();
        &owned
    };
    let runtime = strict_json(&read_regular(root, RUNTIME_LOCK_PATH, MAX_EXPANDED_BYTES)?)?;
    let actual = json!({
        "version": string(runtime.get("source").context("missing runtime source")?, "version")?,
        "commit": string(&runtime["source"], "commit")?,
        "wheel_sha256": sha_field(runtime.get("artifact").context("missing runtime artifact")?, "sha256")?
    });
    ensure!(
        spec.get("runtime") == Some(&actual),
        "bundle requires a different source-locked runtime"
    );
    let raw = read_regular(root, string(spec, "path")?, MAX_ARCHIVE_BYTES)?;
    validate_archive(&raw, spec)
}

fn identity(spec: &Value) -> Result<Value> {
    let mut result = Map::new();
    for key in [
        "package_id",
        "package_version",
        "package_sha256",
        "candidate_sha256",
        "sha256",
        "manifest_sha256",
        "runtime",
    ] {
        result.insert(
            key.to_owned(),
            spec.get(key)
                .with_context(|| format!("missing package identity: {key}"))?
                .clone(),
        );
    }
    Ok(Value::Object(result))
}

fn text<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    let value = value
        .as_str()
        .with_context(|| format!("{label} must be text"))?;
    ensure!(
        !value.trim().is_empty() && !value.chars().any(|c| c < '\u{20}'),
        "{label} must be nonempty text without control characters"
    );
    Ok(value)
}

/// Shared Python v1 snapshot identity; original asset bytes remain untouched.
pub fn canonical_json(value: &Value) -> Result<Vec<u8>> {
    Ok(oe_core::json::canonical_json(value)?)
}

/// Read only selected exact UTF-8 assets and preserve the established v1 hash.
/// An empty selection makes no filesystem or Semantica call.
pub fn build_premises(root: &Path, selections: &Value) -> Result<Value> {
    let selections = selections
        .as_array()
        .context("semantic premise selection budget exceeded or invalid list")?;
    ensure!(
        selections.len() <= MAX_SELECTIONS,
        "semantic premise selection budget exceeded or invalid list"
    );
    let mut packages = BTreeMap::<String, Value>::new();
    let mut loaded = BTreeMap::<String, LoadedBundle>::new();
    let mut result = Vec::new();
    let mut asset_count = 0usize;
    for selection in selections {
        let object = selection.as_object().context("missing or unknown fields")?;
        let fields = ["bundle", "asset_ids", "purpose", "applicability"];
        ensure!(
            object.len() == fields.len() && fields.iter().all(|key| object.contains_key(*key)),
            "missing or unknown fields"
        );
        let name = text(&selection["bundle"], "bundle")?;
        let purpose = text(&selection["purpose"], "purpose")?;
        let applicability = &selection["applicability"];
        ensure!(
            applicability.is_object(),
            "applicability must be caller-declared JSON object"
        );
        // The Python v1 contract stores the canonical roundtrip, including
        // recursively sorted object keys, before embedding it in a request.
        let applicability = strict_json(&canonical_json(applicability)?)?;
        let ids = selection["asset_ids"]
            .as_array()
            .context("asset_ids must be a nonempty unique list")?;
        let mut unique = BTreeSet::new();
        ensure!(
            !ids.is_empty()
                && ids.iter().all(|id| id
                    .as_str()
                    .is_some_and(|id| !id.is_empty() && unique.insert(id))),
            "asset_ids must be a nonempty unique list"
        );
        asset_count = asset_count
            .checked_add(ids.len())
            .context("semantic premise asset budget exceeded")?;
        ensure!(
            asset_count <= MAX_ASSETS,
            "semantic premise asset budget exceeded"
        );
        if !loaded.contains_key(name) {
            loaded.insert(name.to_owned(), load_bundle(root, name)?);
        }
        let bundle = &loaded[name];
        let package = identity(&bundle.spec)?;
        let package_id = string(&package, "package_id")?;
        if let Some(previous) = packages.get(package_id) {
            ensure!(
                *previous == package,
                "conflicting versions or identities of a semantic package"
            );
        }
        packages.insert(package_id.to_owned(), package.clone());
        let available: BTreeMap<_, _> = array(&bundle.manifest, "assets")?
            .iter()
            .map(|asset| Ok((string(asset, "asset_id")?, asset)))
            .collect::<Result<_>>()?;
        let mut assets = Vec::new();
        for id in ids {
            let id = id.as_str().expect("validated asset id");
            let native = available
                .get(id)
                .with_context(|| format!("unknown semantic asset: {id}"))?;
            let raw = &bundle.payload[string(native, "path")?];
            let content =
                std::str::from_utf8(raw).context("selected semantic asset is not valid UTF-8")?;
            assets.push(
                json!({"asset_id": id, "sha256": native["sha256"], "kind": native["kind"],
                "role": native["role"], "format": native["format"], "encoding": "utf-8",
                "byte_count": raw.len(), "content": content}),
            );
        }
        result.push(json!({"bundle": name, "package": package, "purpose": purpose,
            "applicability": applicability, "applicability_status": "caller_assertion_not_established", "assets": assets}));
        ensure!(
            canonical_json(&Value::Array(result.clone()))?.len() <= MAX_PREMISE_BYTES,
            "semantic premise byte budget exceeded"
        );
    }
    let selected_bytes = canonical_json(&Value::Array(result.clone()))?.len();
    let mut report = json!({"schema": PREMISES_SCHEMA, "status": if result.is_empty() { "not_provided" } else { "provided" },
        "selections": result, "asset_count": asset_count, "selected_bytes": selected_bytes,
        "semantic_execution": "not_run", "project_promotion": "not_established", "fact_admitted": false,
        "meaning": "Locked transport snapshots only; applicability is a caller assertion. Selection establishes neither project adoption nor a formal semantic check."});
    report["snapshot_sha256"] = digest(&canonical_json(&report)?).into();
    Ok(report)
}

pub mod native_models;
pub mod providers;

pub mod schema;
pub mod typed;
/// Same neutral business algorithm, without another dispatcher or source owner.
pub mod typed_relations8 {
    pub use semantica_runtime::typed_relations8::*;
}

pub mod refinery_full;
