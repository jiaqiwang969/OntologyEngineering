//! Rust verification of immutable historical Semantica archive identities.
//! External Python execution is retired for every production feature set.
//! Source-lock, wheel, PEP 610 and RECORD observations remain historical evidence;
//! they do not establish current Rust execution or engineering adoption.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use super::{
    MAX_ARCHIVE_BYTES, MAX_EXPANDED_BYTES, digest, read_regular, safe_relative, strict_json,
    zip_inventory,
};

pub const SOURCE_LOCK_SCHEMA: &str = "ontology-engineering.semantica-source-lock/v1";
pub const STAGING_RUNTIME_SCHEMA: &str = "ontology-engineering.semantica-staging-runtime/v1";
pub const MAX_PROBE_BYTES: usize = 1024 * 1024;
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// Coordinates native Rust calls with final-path runtime replacement. Legacy
/// entry points do not participate until cutover; this is not a service handover.
pub struct RuntimeAccessGuard(oe_core::AdvisoryFileLock);
impl RuntimeAccessGuard {
    fn acquire(root: &Path, exclusive: bool) -> Result<Self> {
        let root = oe_core::SkillRoot::new(root)?;
        let path = root.private_path(Path::new("var/state/runtime-access.lock"))?;
        Ok(Self(oe_core::AdvisoryFileLock::acquire(&path, exclusive)?))
    }
    pub fn shared(root: &Path) -> Result<Self> {
        Self::acquire(root, false)
    }
    pub fn exclusive(root: &Path) -> Result<Self> {
        Self::acquire(root, true)
    }
    pub fn ensure_current(&self) -> Result<()> {
        self.0.ensure_current()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSourceLock {
    pub commit: String,
    pub version: String,
    pub artifact_filename: String,
    pub artifact_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StagingRuntimeDescriptor {
    pub commit: String,
    pub version: String,
    pub wheel_filename: String,
    pub wheel_sha256: String,
    pub descriptor_sha256: String,
}

impl StagingRuntimeDescriptor {
    pub fn as_runtime_source_lock(&self) -> RuntimeSourceLock {
        RuntimeSourceLock {
            commit: self.commit.clone(),
            version: self.version.clone(),
            artifact_filename: self.wheel_filename.clone(),
            artifact_sha256: self.wheel_sha256.clone(),
        }
    }
}

/// Untrusted coordinates, either from a fixture or an isolated external probe.
/// Constructing this value is not evidence of an installed/executed runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledRuntimeObservation {
    pub python_version: [u64; 3],
    pub python_executable: PathBuf,
    pub environment_prefix: PathBuf,
    pub imported_version: String,
    pub distribution_version: String,
    pub package_root: PathBuf,
    pub imported_file: PathBuf,
    pub imported_paths: Vec<PathBuf>,
    pub direct_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeVerification {
    pub source_lock: RuntimeSourceLock,
    pub record_files: usize,
    pub observation_kind: String,
    pub semantic_execution: String,
    pub staging_descriptor_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub level: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone)]
struct RecordEntry {
    hash: String,
    size: usize,
}

/// Immutable proof derived from one hash-verified wheel snapshot.
#[derive(Debug, Clone)]
pub struct VerifiedWheel {
    identity: RuntimeSourceLock,
    record: BTreeMap<String, RecordEntry>,
    payload: std::sync::Arc<BTreeMap<String, Vec<u8>>>,
    archive_bytes: usize,
    requires_python: Option<String>,
    tags: Vec<String>,
}

impl VerifiedWheel {
    pub fn identity(&self) -> &RuntimeSourceLock {
        &self.identity
    }
    pub fn record_file_count(&self) -> usize {
        self.record.len()
    }
    pub fn requires_python(&self) -> Option<&str> {
        self.requires_python.as_deref()
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    /// Bytes from the same ZIP snapshot whose artifact and package RECORD
    /// were verified. No second filesystem read or mutable payload is exposed.
    pub fn members(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.payload
    }
    pub fn member(&self, name: &str) -> Option<&[u8]> {
        self.payload.get(name).map(Vec::as_slice)
    }
    pub fn archive_size(&self) -> usize {
        self.archive_bytes
    }
    /// Release inventories require every wheel member, including dist-info,
    /// to be closed by RECORD. Installed-runtime identity retains its existing
    /// package-only contract; callers opt into this stronger artifact closure.
    pub fn verify_complete_record(&self) -> Result<()> {
        let names: Vec<_> = self
            .payload
            .keys()
            .filter(|n| n.ends_with(".dist-info/RECORD"))
            .collect();
        ensure!(names.len() == 1, "wheel must contain one unique RECORD");
        let record = names[0];
        let mut seen = BTreeSet::new();
        for row in csv_rows(std::str::from_utf8(&self.payload[record])?)? {
            ensure!(row.len() == 3, "wheel RECORD row is malformed");
            safe_relative(&row[0])?;
            ensure!(
                seen.insert(row[0].clone()),
                "wheel RECORD duplicates a member"
            );
            let bytes = self
                .payload
                .get(&row[0])
                .context("wheel RECORD names a missing member")?;
            if &row[0] == record {
                ensure!(
                    row[1].is_empty() && row[2].is_empty(),
                    "wheel RECORD self-row must omit hash and size"
                );
            } else {
                ensure!(
                    row[1] == format!("sha256={}", urlsafe_sha256(bytes))
                        && row[2] == bytes.len().to_string(),
                    "wheel member differs from complete RECORD"
                );
            }
        }
        ensure!(
            seen == self.payload.keys().cloned().collect(),
            "wheel has members absent from complete RECORD"
        );
        Ok(())
    }
}

fn is_hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("missing or invalid {name}"))
}

fn identity(commit: &str, version: &str, filename: &str, hash: &str) -> Result<RuntimeSourceLock> {
    ensure!(is_hex(commit, 40), "runtime identity has an invalid commit");
    ensure!(
        !version.is_empty() && version == version.trim() && !version.chars().any(char::is_control),
        "runtime identity has an invalid version"
    );
    ensure!(
        filename.ends_with(".whl") && safe_relative(filename).is_ok() && !filename.contains('/'),
        "runtime identity has an invalid wheel filename"
    );
    ensure!(
        is_hex(hash, 64),
        "runtime identity has an invalid wheel digest"
    );
    Ok(RuntimeSourceLock {
        commit: commit.into(),
        version: version.into(),
        artifact_filename: filename.into(),
        artifact_sha256: hash.into(),
    })
}

fn expand_user(path: &Path) -> Result<PathBuf> {
    let text = path.as_os_str().to_string_lossy();
    if text == "~" || text.starts_with("~/") {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context("home directory is unavailable")?;
        return Ok(PathBuf::from(home).join(text.strip_prefix("~/").unwrap_or("")));
    }
    ensure!(
        !text.starts_with('~'),
        "named-user expansion is unsupported"
    );
    Ok(path.to_path_buf())
}

fn read_path(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let path = expand_user(path)?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .context("file basename must be UTF-8")?;
    read_regular(path.parent().unwrap_or(Path::new(".")), name, limit)
}

pub fn read_runtime_source_lock(
    root: &Path,
    verify_vendored_artifact: bool,
) -> Result<RuntimeSourceLock> {
    let document = strict_json(&read_regular(
        root,
        "runtime/semantica-source-lock.json",
        MAX_PROBE_BYTES,
    )?)?;
    ensure!(document.is_object(), "source lock root must be an object");
    ensure!(
        field(&document, "$schema")? == SOURCE_LOCK_SCHEMA,
        "source lock has an unsupported schema"
    );
    let lock = identity(
        field(&document["source"], "commit")?,
        field(&document["source"], "version")?,
        field(&document["artifact"], "filename")?,
        field(&document["artifact"], "sha256")?,
    )?;
    if verify_vendored_artifact {
        let bytes = read_regular(
            root,
            &format!("runtime/vendor/{}", lock.artifact_filename),
            MAX_ARCHIVE_BYTES,
        )?;
        ensure!(
            digest(&bytes) == lock.artifact_sha256,
            "vendored Semantica wheel differs from the source lock"
        );
    }
    Ok(lock)
}

pub fn read_staging_runtime_descriptor(path: &Path) -> Result<StagingRuntimeDescriptor> {
    let bytes = read_path(path, MAX_PROBE_BYTES)?;
    let value = strict_json(&bytes)?;
    let object = value
        .as_object()
        .context("staging descriptor root must be an object")?;
    let fields = [
        "$schema",
        "commit",
        "version",
        "wheel_filename",
        "wheel_sha256",
    ];
    ensure!(
        object.len() == fields.len() && fields.iter().all(|key| object.contains_key(*key)),
        "staging descriptor has missing or unsupported fields"
    );
    ensure!(
        field(&value, "$schema")? == STAGING_RUNTIME_SCHEMA,
        "staging descriptor has an unsupported schema"
    );
    let lock = identity(
        field(&value, "commit")?,
        field(&value, "version")?,
        field(&value, "wheel_filename")?,
        field(&value, "wheel_sha256")?,
    )?;
    Ok(StagingRuntimeDescriptor {
        commit: lock.commit,
        version: lock.version,
        wheel_filename: lock.artifact_filename,
        wheel_sha256: lock.artifact_sha256,
        descriptor_sha256: digest(&bytes),
    })
}

/// Prefer modern PEP 610 archive_info.hashes.sha256; retain its legacy form.
pub fn installed_runtime_artifact_sha256(direct_url: &[u8]) -> Result<String> {
    let value = strict_json(direct_url)?;
    let archive = value
        .get("archive_info")
        .and_then(Value::as_object)
        .context("installed wheel provenance is unavailable")?;
    let modern = archive
        .get("hashes")
        .and_then(Value::as_object)
        .and_then(|v| v.get("sha256"));
    let hash = match modern {
        None | Some(Value::Null) => archive
            .get("hash")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix("sha256=")),
        Some(Value::String(s)) if s.is_empty() => archive
            .get("hash")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix("sha256=")),
        Some(Value::String(s)) => Some(s.as_str()),
        Some(_) => None,
    }
    .context("installed wheel provenance is invalid")?;
    ensure!(is_hex(hash, 64), "installed wheel provenance is invalid");
    Ok(hash.into())
}

fn headers(bytes: &[u8]) -> Result<BTreeMap<String, Vec<String>>> {
    let text = std::str::from_utf8(bytes).context("wheel metadata is not UTF-8")?;
    let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut previous: Option<String> = None;
    for line in text.lines() {
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            let name = previous
                .as_ref()
                .context("metadata continuation lacks a field")?;
            let value = result
                .get_mut(name)
                .and_then(|v| v.last_mut())
                .context("invalid metadata continuation")?;
            value.push(' ');
            value.push_str(line.trim());
        } else {
            let (key, value) = line
                .split_once(':')
                .context("malformed wheel metadata header")?;
            ensure!(
                !key.is_empty() && key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'),
                "invalid wheel metadata header name"
            );
            let key = key.to_ascii_lowercase();
            result
                .entry(key.clone())
                .or_default()
                .push(value.trim().into());
            previous = Some(key);
        }
    }
    Ok(result)
}

fn singleton<'a>(headers: &'a BTreeMap<String, Vec<String>>, key: &str) -> Result<Option<&'a str>> {
    match headers.get(key) {
        None => Ok(None),
        Some(values) if values.len() == 1 => Ok(Some(values[0].as_str())),
        _ => bail!("duplicate wheel metadata field: {key}"),
    }
}

/// RFC-4180-style fields, including quoted commas/newlines and doubled quotes.
/// Invalid quoting is rejected rather than silently changing a RECORD path.
fn csv_rows(text: &str) -> Result<Vec<Vec<String>>> {
    Ok(oe_core::csv::rows(text)?)
}

fn urlsafe_sha256(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let hash = Sha256::digest(bytes);
    let mut output = String::with_capacity(43);
    for chunk in hash.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(c & 63) as usize] as char);
        }
    }
    output
}

fn parse_record(bytes: &[u8]) -> Result<BTreeMap<String, RecordEntry>> {
    let mut expected = BTreeMap::new();
    for row in csv_rows(std::str::from_utf8(bytes).context("wheel RECORD is not UTF-8")?)? {
        ensure!(row.len() == 3, "wheel RECORD row is malformed");
        let relative = &row[0];
        if !relative.starts_with("semantica/") {
            continue;
        }
        safe_relative(relative).context("wheel RECORD contains an unsafe package path")?;
        let hash = row[1]
            .strip_prefix("sha256=")
            .context("wheel package row lacks hash or size")?;
        ensure!(
            hash.len() == 43
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
            "wheel RECORD SHA-256 is invalid"
        );
        ensure!(
            !row[2].is_empty() && row[2].bytes().all(|c| c.is_ascii_digit()),
            "wheel package row lacks hash or size"
        );
        let size: usize = row[2].parse().context("wheel RECORD size overflows")?;
        ensure!(
            size <= MAX_EXPANDED_BYTES,
            "wheel RECORD file exceeds byte limit"
        );
        ensure!(
            expected
                .insert(
                    relative.clone(),
                    RecordEntry {
                        hash: hash.into(),
                        size
                    }
                )
                .is_none(),
            "wheel RECORD duplicates a package path"
        );
    }
    ensure!(
        expected.contains_key("semantica/__init__.py"),
        "wheel RECORD has no package root"
    );
    Ok(expected)
}

pub fn read_wheel_identity(path: &Path, expected: &RuntimeSourceLock) -> Result<VerifiedWheel> {
    identity(
        &expected.commit,
        &expected.version,
        &expected.artifact_filename,
        &expected.artifact_sha256,
    )?;
    let raw = read_path(path, MAX_ARCHIVE_BYTES)?;
    ensure!(
        digest(&raw) == expected.artifact_sha256,
        "selected Semantica wheel differs from its identity"
    );
    let inventory = zip_inventory(&raw).context("selected wheel ZIP inventory is invalid")?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&raw))?;
    ensure!(zip.len() == inventory.len(), "wheel ZIP inventories differ");
    let mut payload = BTreeMap::new();
    for (i, member) in inventory.iter().enumerate() {
        let mut entry = zip.by_index(i)?;
        ensure!(
            entry.name_raw() == member.name.as_bytes() && entry.size() == member.size as u64,
            "wheel ZIP metadata mismatch"
        );
        let mut bytes = Vec::new();
        (&mut entry)
            .take(member.size as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == member.size as usize,
            "wheel ZIP member size mismatch"
        );
        payload.insert(member.name.clone(), bytes);
    }
    let unique = |suffix: &str| -> Result<&str> {
        let names: Vec<_> = payload
            .keys()
            .filter(|name| name.ends_with(suffix))
            .collect();
        ensure!(names.len() == 1, "wheel must contain one unique {suffix}");
        Ok(names[0].as_str())
    };
    let metadata_name = unique(".dist-info/METADATA")?;
    let wheel_name = unique(".dist-info/WHEEL")?;
    let record_name = unique(".dist-info/RECORD")?;
    ensure!(
        Path::new(metadata_name).parent() == Path::new(wheel_name).parent()
            && Path::new(metadata_name).parent() == Path::new(record_name).parent(),
        "wheel metadata identities are split across dist-info directories"
    );
    let metadata = headers(&payload[metadata_name])?;
    let wheel = headers(&payload[wheel_name])?;
    ensure!(
        singleton(&metadata, "name")?.is_some_and(|s| s.eq_ignore_ascii_case("semantica")),
        "wheel distribution is not Semantica"
    );
    ensure!(
        singleton(&metadata, "version")? == Some(expected.version.as_str()),
        "wheel version differs from selected identity"
    );
    let record = parse_record(&payload[record_name])?;
    let packaged: BTreeSet<_> = payload
        .keys()
        .filter(|name| name.starts_with("semantica/"))
        .collect();
    ensure!(
        packaged == record.keys().collect(),
        "wheel RECORD package inventory differs from wheel files"
    );
    for (name, row) in &record {
        ensure!(
            payload[name].len() == row.size && urlsafe_sha256(&payload[name]) == row.hash,
            "wheel package file differs from authoritative RECORD: {name}"
        );
    }
    Ok(VerifiedWheel {
        identity: expected.clone(),
        record,
        requires_python: singleton(&metadata, "requires-python")?.map(str::to_owned),
        tags: wheel.get("tag").cloned().unwrap_or_default(),
        payload: std::sync::Arc::new(payload),
        archive_bytes: raw.len(),
    })
}

fn directory_identity(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut cursor = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::ParentDir => bail!("installed package path contains parent traversal"),
            Component::CurDir => continue,
            _ => cursor.push(part.as_os_str()),
        }
        let info =
            fs::symlink_metadata(&cursor).context("installed package directory is unavailable")?;
        ensure!(
            !info.file_type().is_symlink() && info.is_dir(),
            "installed package directory is missing or symbolic"
        );
    }
    absolute
        .canonicalize()
        .context("installed package directory cannot be resolved")
}

fn installed_files(root: &Path, directory: &Path, result: &mut BTreeSet<String>) -> Result<()> {
    for item in fs::read_dir(directory)? {
        let path = item?.path();
        let info = fs::symlink_metadata(&path)?;
        ensure!(
            !info.file_type().is_symlink(),
            "installed Semantica package contains a symbolic link"
        );
        if info.is_dir() {
            installed_files(root, &path, result)?;
        } else {
            ensure!(
                info.is_file(),
                "installed Semantica package contains a nonregular file"
            );
            let relative = path.strip_prefix(root)?;
            if relative
                .components()
                .any(|c| c.as_os_str() == "__pycache__")
                || matches!(
                    path.extension().and_then(|s| s.to_str()),
                    Some("pyc" | "pyo")
                )
            {
                continue;
            }
            let name = relative
                .components()
                .map(|part| match part {
                    Component::Normal(name) => name
                        .to_str()
                        .context("installed package filename is not UTF-8"),
                    _ => bail!("invalid installed package path component"),
                })
                .collect::<Result<Vec<_>>>()?
                .join("/");
            // A literal backslash is a filename character on Unix. Never turn
            // an unrecorded `data\\facts.json` into recorded `data/facts.json`.
            safe_relative(&name).context("unsafe installed package filename")?;
            result.insert(format!("semantica/{name}"));
        }
    }
    Ok(())
}

/// Validate disk files against the selected wheel, not installed RECORD rows.
/// A supplied observation alone is not a live interpreter identity proof.
pub fn verify_installed_runtime_record(
    wheel: &VerifiedWheel,
    observed: &InstalledRuntimeObservation,
) -> Result<usize> {
    ensure!(
        observed.distribution_version == wheel.identity.version,
        "installed distribution metadata differs from selected identity"
    );
    let root = directory_identity(&observed.package_root)?;
    ensure!(
        !fs::symlink_metadata(&observed.imported_file)?
            .file_type()
            .is_symlink(),
        "imported Semantica is symbolic"
    );
    ensure!(
        observed.imported_file.canonicalize()? == root.join("__init__.py"),
        "imported Semantica is outside selected installed distribution"
    );
    let paths: Vec<_> = observed
        .imported_paths
        .iter()
        .map(|p| directory_identity(p))
        .collect::<Result<_>>()?;
    ensure!(
        paths == [root.clone()],
        "imported Semantica package path is shadowed or namespace-extended"
    );
    for (name, row) in &wheel.record {
        let relative = name
            .strip_prefix("semantica/")
            .expect("verified RECORD package prefix");
        let bytes = read_regular(&root, relative, row.size)?;
        ensure!(
            bytes.len() == row.size && urlsafe_sha256(&bytes) == row.hash,
            "installed Semantica package file differs from wheel RECORD: {name}"
        );
    }
    let mut actual = BTreeSet::new();
    installed_files(&root, &root, &mut actual)?;
    let expected: BTreeSet<_> = wheel.record.keys().cloned().collect();
    let extras: Vec<_> = actual.difference(&expected).take(5).cloned().collect();
    ensure!(
        extras.is_empty(),
        "installed Semantica package contains unrecorded files: {}",
        extras.join(", ")
    );
    Ok(wheel.record.len())
}

/// A bounded subset of Python version specifiers; unknown is not pass.
/// Correct two old-doctor bugs: only == accepts .*, and equality without .*
/// compares zero-padded release tuples rather than accepting patch prefixes.
pub fn requires_python_satisfied(specifier: &str, current: [u64; 3]) -> Option<bool> {
    for raw in specifier.split(',') {
        let clause = raw.trim();
        let operator = [">=", "<=", "==", ">", "<"]
            .iter()
            .find(|op| clause.starts_with(**op))?;
        let raw = clause[operator.len()..].trim();
        let wildcard = raw.ends_with(".*");
        if wildcard && *operator != "==" {
            return None;
        }
        let raw = raw.strip_suffix(".*").unwrap_or(raw);
        let parts: Option<Vec<u64>> = raw
            .split('.')
            .map(|s| {
                if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) {
                    s.parse().ok()
                } else {
                    None
                }
            })
            .collect();
        let parts = parts?;
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        let mut expected = [0; 3];
        expected[..parts.len()].copy_from_slice(&parts);
        let good = match *operator {
            ">=" => current >= expected,
            ">" => current > expected,
            "<=" => current <= expected,
            "<" => current < expected,
            "==" if wildcard => current[..parts.len()] == parts,
            "==" => current == expected,
            _ => return None,
        };
        if !good {
            return Some(false);
        }
    }
    Some(true)
}

fn check(level: &str, code: &str, message: impl Into<String>) -> Check {
    Check {
        level: level.into(),
        code: code.into(),
        message: message.into(),
    }
}

/// Pure Rust, non-mutating lock/wheel preflight. No interpreter is started.
pub fn doctor_preflight(root: &Path, python_version: [u64; 3]) -> Vec<Check> {
    let mut checks = Vec::new();
    let identity = match read_runtime_source_lock(root, false) {
        Ok(identity) => Some(identity),
        Err(error) => {
            checks.push(check("error", "lock_invalid", error.to_string()));
            None
        }
    };
    if let Some(identity) = identity {
        checks.push(check(
            "ok",
            "lock_identity",
            format!(
                "Semantica {} is pinned to {}",
                identity.version, identity.commit
            ),
        ));
        let wheel_path = root
            .join("runtime/vendor")
            .join(&identity.artifact_filename);
        match read_wheel_identity(&wheel_path, &identity) {
            Err(error) => {
                let message = error.to_string();
                let code = if message.contains("differs from its identity") {
                    "wheel_hash"
                } else if !wheel_path.is_file()
                    || fs::symlink_metadata(&wheel_path).is_ok_and(|m| m.file_type().is_symlink())
                {
                    "wheel_missing"
                } else {
                    "wheel_invalid"
                };
                checks.push(check("error", code, message));
            }
            Ok(wheel) => {
                if let Some(spec) = wheel.requires_python() {
                    match requires_python_satisfied(spec, python_version) {
                        Some(true) => checks.push(check(
                            "ok",
                            "python_version",
                            format!("Python {python_version:?} satisfies {spec}"),
                        )),
                        Some(false) => checks.push(check(
                            "error",
                            "python_version",
                            format!("Python {python_version:?} does not satisfy {spec}"),
                        )),
                        None => checks.push(check(
                            "warning",
                            "python_specifier",
                            format!("could not evaluate Requires-Python: {spec}"),
                        )),
                    }
                }
                checks.push(if wheel.tags.iter().any(|tag| tag == "py3-none-any") {
                    check(
                        "ok",
                        "wheel_platform",
                        "locked wheel is portable py3-none-any",
                    )
                } else {
                    check(
                        "warning",
                        "wheel_platform",
                        "platform-specific wheel; full tag compatibility is not established",
                    )
                });
                checks.push(check(
                    "ok",
                    "wheel_hash",
                    format!(
                        "vendored wheel SHA-256 verified: {}",
                        identity.artifact_sha256
                    ),
                ));
            }
        }
    }
    checks.push(check("info", "offline_boundary", "the Semantica wheel is vendored, but first-time dependency installation may require a package index"));
    checks
}

/// Kept for callers to receive an explicit retirement error. Historical
/// observations are still validated by verify_installed_runtime_record;
/// no feature can execute an interpreter through the v2 library.
pub fn probe_external_runtime(_root: &Path, _python: &Path) -> Result<InstalledRuntimeObservation> {
    bail!("legacy_python_runtime_probe_retired_in_v2")
}

pub fn probe_external_runtime_with_limits(
    _root: &Path,
    _python: &Path,
    _timeout: Duration,
    _output_limit: usize,
) -> Result<InstalledRuntimeObservation> {
    bail!("legacy_python_runtime_probe_retired_in_v2")
}

/// Historical external-runtime execution is retired, including staging.
/// Rust semantic execution establishes its own current implementation identity.
pub fn verify_runtime_source_identity(
    _root: &Path,
    _staging_descriptor: Option<&Path>,
    _python: &Path,
) -> Result<RuntimeVerification> {
    bail!("legacy_python_runtime_probe_retired_in_v2")
}

/// Verify supplied historical package observations against the same selected
/// original wheel and RECORD. This never executes an interpreter and never
/// establishes a current Rust runtime or a live imported Python package.
pub fn verify_historical_runtime_observation(
    root: &Path,
    staging_descriptor: Option<&Path>,
    observed: &InstalledRuntimeObservation,
) -> Result<RuntimeVerification> {
    let staging = staging_descriptor
        .map(read_staging_runtime_descriptor)
        .transpose()?;
    let lock = match &staging {
        Some(value) => value.as_runtime_source_lock(),
        None => read_runtime_source_lock(root, true)?,
    };
    let wheel_path = match staging_descriptor {
        Some(path) => path
            .parent()
            .context("staging_descriptor_parent_missing")?
            .join(&lock.artifact_filename),
        None => root.join("runtime/vendor").join(&lock.artifact_filename),
    };
    let wheel = read_wheel_identity(&wheel_path, &lock)?;
    ensure!(
        observed.imported_version == lock.version,
        "historical_imported_version_mismatch"
    );
    ensure!(
        installed_runtime_artifact_sha256(observed.direct_url.as_bytes())? == lock.artifact_sha256,
        "historical_pep610_artifact_mismatch"
    );
    let record_files = verify_installed_runtime_record(&wheel, observed)?;
    Ok(RuntimeVerification {
        source_lock: lock,
        record_files,
        observation_kind: "supplied_historical_observation_not_live_runtime".into(),
        semantic_execution: "not_run".into(),
        staging_descriptor_sha256: staging.map(|value| value.descriptor_sha256),
    })
}
