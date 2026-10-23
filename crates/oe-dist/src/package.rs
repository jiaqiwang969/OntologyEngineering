//! Native transport of an explicit frozen portable-skill/v1 inventory.
//! Transport integrity and privacy never establish release or engineering approval.
use anyhow::{Context, Result, ensure};
use oe_core::{FileIdentity, SkillRoot, fingerprint_file, open_regular_file, sha256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_normalization::UnicodeNormalization;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};
#[path = "zip_transport.rs"]
mod container;

pub const FORMAT: &str = "ontology-engineering.portable-skill/v1";
pub const SCOPE: &str = "Transport integrity only; release authorization is recorded separately.";
const PREFIX: &str = "ontology-engineering/";
const V2_PREFIX: &str = "ontology-engineering-v2/";
const MAX_SKILL_BYTES: usize = 1024 * 1024;
const MANIFEST_NAME: &str = "PORTABLE-MANIFEST.json";
pub(crate) const MAX_FILES: usize = 10_000;
pub(crate) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub(crate) const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub(crate) const MAX_ARCHIVE_BYTES: u64 = MAX_TOTAL_BYTES + 64 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const REQUIRED: &[&str] = &["SKILL.md", "AGENTS.md", "README.md", "VERSION", "LICENSE"];

/// Read the root name in the supported frontmatter profile. Plain historical
/// fixtures have no declared name. Missing/ambiguous declarations are errors.
pub fn declared_skill_name(bytes: &[u8]) -> Result<Option<&str>> {
    let mut lines = std::str::from_utf8(bytes)?.lines();
    if lines.next() != Some("---") {
        return Ok(None);
    }
    let mut name = None;
    for line in lines {
        if line == "---" {
            return Ok(Some(name.context("package_skill_identity_missing")?));
        }
        if let Some(value) = line.strip_prefix("name:") {
            ensure!(name.is_none(), "package_skill_identity_ambiguous");
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            ensure!(!value.is_empty(), "package_skill_identity_missing");
            name = Some(value);
        }
    }
    anyhow::bail!("package_skill_frontmatter_unclosed")
}
fn archive_prefix(bytes: &[u8]) -> Result<&'static str> {
    match declared_skill_name(bytes)? {
        Some("ontology-engineering-v2") => Ok(V2_PREFIX),
        Some("ontology-engineering") | None => Ok(PREFIX),
        _ => anyhow::bail!("package_skill_identity_unsupported"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortableManifest {
    pub format: String,
    pub entry: String,
    pub scope: String,
    pub files: Vec<SourceFile>,
}
fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn path_key(name: &str) -> String {
    oe_wire::unicode_casefold::casefold(&name.nfc().collect::<String>())
}
fn payload_path(name: &str) -> Result<()> {
    oe_semantic::safe_relative(name)?;
    // The library is private input. Only its exact public entry document is
    // distributable, including when a supplied inventory tries to add a PDF,
    // extraction, catalog or a case-variant directory.
    ensure!(
        !name
            .split('/')
            .any(|component| path_key(component) == "sources")
            || name == "sources/README.md",
        "package_private_sources_payload"
    );
    ensure!(
        name.len() <= 4096 && name.split('/').count() <= 32,
        "package_path_limit"
    );
    ensure!(
        !name.chars().any(|c| c.is_control()),
        "package_control_in_path"
    );
    for component in name.split('/') {
        ensure!(
            component.len() <= 255
                && !component.ends_with(['.', ' '])
                && !component.contains(['<', '>', '"', '|', '?', '*']),
            "package_nonportable_path"
        );
        let key = path_key(component);
        ensure!(
            !matches!(
                key.as_str(),
                "var" | ".git" | ".venv" | "__pycache__" | "target"
            ),
            "package_private_or_generated_path"
        );
        let stem = key.split('.').next().unwrap_or("");
        ensure!(
            !matches!(stem, "con" | "prn" | "aux" | "nul")
                && !(stem.len() == 4
                    && (stem.starts_with("com") || stem.starts_with("lpt"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "package_reserved_device_path"
        );
    }
    ensure!(
        path_key(name) != path_key(MANIFEST_NAME),
        "package_manifest_reserved"
    );
    Ok(())
}
impl PortableManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format == FORMAT && self.entry == "SKILL.md" && self.scope == SCOPE,
            "package_manifest_contract"
        );
        ensure!(
            !self.files.is_empty() && self.files.len() <= MAX_FILES,
            "package_file_count_limit"
        );
        let mut names = BTreeSet::new();
        let mut directories = BTreeMap::new();
        let mut total = 0u64;
        let mut previous: Option<&str> = None;
        for row in &self.files {
            payload_path(&row.path)?;
            ensure!(
                previous.is_none_or(|v| v < row.path.as_str()),
                "package_inventory_not_sorted_or_duplicate"
            );
            previous = Some(&row.path);
            ensure!(digest_valid(&row.sha256), "package_invalid_digest");
            ensure!(row.bytes <= MAX_FILE_BYTES, "package_file_size_limit");
            total = total
                .checked_add(row.bytes)
                .context("package_size_overflow")?;
            ensure!(total <= MAX_TOTAL_BYTES, "package_total_size_limit");
            ensure!(
                names.insert(path_key(&row.path)),
                "package_portable_name_collision"
            );
            let mut parent = String::new();
            let parts: Vec<_> = row.path.split('/').collect();
            for component in &parts[..parts.len() - 1] {
                if !parent.is_empty() {
                    parent.push('/');
                }
                parent.push_str(component);
                let key = path_key(&parent);
                if let Some(old) = directories.insert(key, parent.clone()) {
                    ensure!(old == parent, "package_directory_name_collision");
                }
            }
        }
        ensure!(
            directories.keys().all(|key| !names.contains(key)),
            "package_file_directory_collision"
        );
        for required in REQUIRED {
            ensure!(
                self.files.iter().any(|row| row.path == *required),
                "package_required_file_missing: {required}"
            );
        }
        Ok(())
    }
}
fn decode_manifest(bytes: &[u8]) -> Result<PortableManifest> {
    let manifest: PortableManifest = serde_json::from_value(oe_core::json::strict_json(bytes)?)?;
    manifest.validate()?;
    Ok(manifest)
}
pub fn read_manifest(path: &Path) -> Result<PortableManifest> {
    decode_manifest(&oe_core::read_regular_bytes(path, MAX_MANIFEST_BYTES)?)
}
fn read_toml(path: &Path) -> Result<toml::Value> {
    let bytes = oe_core::read_regular_bytes(path, 4 * 1024 * 1024)?;
    toml::from_str(std::str::from_utf8(&bytes)?).context("package_invalid_cargo_toml")
}
fn members(document: &toml::Value) -> Result<Vec<String>> {
    let list = document
        .get("workspace")
        .and_then(|v| v.get("members"))
        .and_then(toml::Value::as_array)
        .context("package_explicit_workspace_members_required")?;
    ensure!(
        !list.is_empty() && list.len() <= 128,
        "package_workspace_member_limit"
    );
    let mut seen = BTreeSet::new();
    list.iter()
        .map(|value| {
            let name = value.as_str().context("package_invalid_workspace_member")?;
            payload_path(name)?;
            ensure!(
                !name.contains(['*', '?', '[', ']']),
                "package_workspace_glob_not_supported"
            );
            ensure!(
                seen.insert(path_key(name)),
                "package_workspace_duplicate_member"
            );
            Ok(name.to_owned())
        })
        .collect()
}
fn require_workspace(
    manifest: &PortableManifest,
    documents: &BTreeMap<String, toml::Value>,
) -> Result<()> {
    if !manifest.files.iter().any(|v| v.path == "Cargo.toml") {
        ensure!(
            !manifest.files.iter().any(|v| v.path.starts_with("crates/")),
            "package_workspace_root_missing"
        );
        return Ok(());
    }
    let paths: BTreeSet<_> = manifest.files.iter().map(|v| v.path.as_str()).collect();
    ensure!(paths.contains("Cargo.lock"), "package_cargo_lock_missing");
    if paths.contains("crates/oe-design/Cargo.toml") {
        ensure!(
            paths.contains("crates/oe-design/build.rs")
                && paths.contains("build_support/provenance.rs"),
            "package_compiler_binding_inputs_missing"
        );
    }
    if paths.contains("crates/oe-cli/build.rs") {
        ensure!(
            paths.contains("generated/contracts.rs"),
            "package_compiled_contract_input_missing"
        );
    }
    for member in members(
        documents
            .get("Cargo.toml")
            .context("package_cargo_root_unreadable")?,
    )? {
        let cargo = format!("{member}/Cargo.toml");
        ensure!(
            paths.contains(cargo.as_str()),
            "package_workspace_member_missing: {cargo}"
        );
        let document = documents
            .get(&cargo)
            .context("package_member_cargo_unreadable")?;
        let prefix = format!("{member}/");
        ensure!(
            paths
                .iter()
                .any(|path| path.starts_with(&prefix) && path.ends_with(".rs")),
            "package_member_source_missing: {member}"
        );
        if let Some(library) = document.get("lib") {
            let path = library
                .get("path")
                .and_then(toml::Value::as_str)
                .unwrap_or("src/lib.rs");
            payload_path(path)?;
            ensure!(
                paths.contains(format!("{prefix}{path}").as_str()),
                "package_library_target_missing"
            );
        }
        for kind in ["bin", "example", "test", "bench"] {
            if let Some(targets) = document.get(kind) {
                for target in targets
                    .as_array()
                    .context("package_invalid_cargo_targets")?
                {
                    if let Some(path) = target.get("path").and_then(toml::Value::as_str) {
                        payload_path(path)?;
                        ensure!(
                            paths.contains(format!("{prefix}{path}").as_str()),
                            "package_declared_target_missing"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn check_source_closure(root: &SkillRoot, manifest: &PortableManifest) -> Result<()> {
    let cargo = root.path().join("Cargo.toml");
    if !cargo.try_exists()? {
        return Ok(());
    }
    ensure!(
        manifest.files.iter().any(|v| v.path == "Cargo.toml"),
        "package_workspace_root_missing"
    );
    let expected = manifest
        .files
        .iter()
        .find(|v| v.path == "Cargo.toml")
        .context("package_cargo_root_missing")?;
    let bytes = oe_core::read_regular_bytes(&cargo, 4 * 1024 * 1024)?;
    ensure!(
        bytes.len() as u64 == expected.bytes && sha256(&bytes) == expected.sha256,
        "package_workspace_declaration_changed"
    );
    let document: toml::Value = toml::from_str(std::str::from_utf8(&bytes)?)?;
    let names: BTreeSet<_> = manifest.files.iter().map(|v| v.path.as_str()).collect();
    fn walk(
        base: &Path,
        path: &Path,
        names: &BTreeSet<&str>,
        depth: usize,
        count: &mut usize,
    ) -> Result<()> {
        ensure!(depth <= 32, "package_source_depth_limit");
        oe_core::no_symlinks(path)?;
        for item in std::fs::read_dir(path)? {
            let item = item?;
            *count += 1;
            ensure!(*count <= MAX_FILES * 2, "package_source_node_limit");
            let relative = oe_core::portable_source_path(base, &item.path())?;
            payload_path(&relative)?;
            let kind = item.file_type()?;
            ensure!(!kind.is_symlink(), "package_source_symlink");
            if kind.is_dir() {
                walk(base, &item.path(), names, depth + 1, count)?;
            } else {
                ensure!(kind.is_file(), "package_source_not_regular");
                ensure!(
                    names.contains(relative.as_str()),
                    "package_unlisted_workspace_source: {relative}"
                );
            }
        }
        Ok(())
    }
    let mut count = 0;
    for member in members(&document)? {
        walk(
            root.path(),
            &root.path().join(member),
            &names,
            0,
            &mut count,
        )?;
    }
    Ok(())
}
pub(crate) fn copy_locked(source: &Path, destination: &Path, expected: &SourceFile) -> Result<u32> {
    let mut input = open_regular_file(source, MAX_FILE_BYTES)?;
    let before = input.metadata()?;
    ensure!(
        before.len() == expected.bytes,
        "package_source_size_changed"
    );
    let mut output = oe_core::create_new_regular_file(destination)?;
    let mut digest = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .context("package_copy_size_overflow")?;
        ensure!(count <= expected.bytes, "package_source_grew");
        output.write_all(&buffer[..n])?;
        digest.update(&buffer[..n]);
    }
    let after = input.metadata()?;
    ensure!(
        count == expected.bytes
            && count == after.len()
            && before.modified()? == after.modified()?
            && format!("{:x}", digest.finalize()) == expected.sha256,
        "package_source_changed"
    );
    output.sync_all()?;
    ensure!(
        fingerprint_file(source, MAX_FILE_BYTES)?
            == FileIdentity {
                bytes: expected.bytes,
                sha256: expected.sha256.clone()
            },
        "package_source_path_changed"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(before.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        Ok(0o644)
    }
}
pub(crate) fn verify_privacy(report: &Value, manifest: &PortableManifest) -> Result<()> {
    ensure!(
        report["ok"] == true
            && report["inputs_unchanged"] == true
            && report["selection_unchanged"] == true,
        "package_privacy_blocked"
    );
    let candidates = report["candidates"]
        .as_array()
        .context("package_privacy_inventory_missing")?;
    ensure!(
        candidates.len() == manifest.files.len(),
        "package_privacy_inventory_differs"
    );
    let expected: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|v| (v.path.as_str(), v))
        .collect();
    let mut seen = BTreeSet::new();
    for observed in candidates {
        let name = observed["path"]
            .as_str()
            .context("package_privacy_path_missing")?;
        ensure!(seen.insert(name), "package_privacy_duplicate");
        let row = expected
            .get(name)
            .context("package_privacy_unlisted_file")?;
        ensure!(
            observed["sha256"] == row.sha256
                && observed["bytes"].as_u64() == Some(row.bytes)
                && observed["unchanged"] == true,
            "package_privacy_bytes_differ"
        );
    }
    Ok(())
}
fn archive_snapshot(
    snapshot: &Path,
    output: &Path,
    manifest: &PortableManifest,
    modes: &[u32],
    prefix: &str,
) -> Result<()> {
    let mut writer = ZipWriter::new(oe_core::create_new_regular_file(output)?);
    for (row, mode) in manifest.files.iter().zip(modes) {
        let method = if matches!(
            Path::new(&row.path).extension().and_then(|s| s.to_str()),
            Some("pdf" | "whl" | "zip" | "xz")
        ) {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        writer.start_file(
            format!("{prefix}{}", row.path),
            SimpleFileOptions::default()
                .large_file(true)
                .compression_method(method)
                .unix_permissions(*mode),
        )?;
        let mut input = open_regular_file(&snapshot.join(&row.path), MAX_FILE_BYTES)?;
        let mut digest = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(n as u64)
                .context("package_archive_size_overflow")?;
            ensure!(count <= row.bytes, "package_snapshot_grew");
            writer.write_all(&buffer[..n])?;
            digest.update(&buffer[..n]);
        }
        ensure!(
            count == row.bytes && format!("{:x}", digest.finalize()) == row.sha256,
            "package_snapshot_changed"
        );
    }
    writer.start_file(
        format!("{prefix}{MANIFEST_NAME}"),
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644),
    )?;
    writer.write_all(&serde_json::to_vec_pretty(manifest)?)?;
    writer.write_all(b"\n")?;
    writer.finish()?.sync_all()?;
    Ok(())
}
/// Bound producer result; caller booleans cannot substitute for actual observations.
pub struct PackageResult {
    directory: PathBuf,
    archive_identity: FileIdentity,
    manifest_identity: FileIdentity,
    receipt_identity: FileIdentity,
    archive_namespace: String,
}
impl PackageResult {
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn to_json(&self) -> Value {
        json!({"schema":"ontology-engineering.portable-package-result/v1",
            "status":"transport_verified", "directory":self.directory,
            "archive_namespace":self.archive_namespace,
            "archive":{"path":"skill.zip","sha256":self.archive_identity.sha256,"bytes":self.archive_identity.bytes},
            "manifest":{"path":MANIFEST_NAME,"sha256":self.manifest_identity.sha256,"bytes":self.manifest_identity.bytes},
            "receipt":{"path":"receipt.json","sha256":self.receipt_identity.sha256,"bytes":self.receipt_identity.bytes},
            "scope":"explicit_inventory_transport_and_privacy",
            "receiver_cargo_execution":"not_run",
            "entrypoint_cutover":false,"publication":"not_performed","engineering_acceptance":"not_established"})
    }
}
/// Failed attempts retain their snapshot and failure receipt under var/staging/.
pub fn package_skill(root: &SkillRoot, inventory: &Path, output: &Path) -> Result<PackageResult> {
    let inventory = root.expand(inventory)?;
    let bytes = oe_core::read_regular_bytes(&inventory, MAX_MANIFEST_BYTES)?;
    let manifest = decode_manifest(&bytes)?;
    let output = root.private_path(output)?;
    ensure!(!output.try_exists()?, "package_output_already_exists");
    check_source_closure(root, &manifest)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let staging = root.new_private_dir(&PathBuf::from(format!(
        "var/staging/package-{}-{nonce}",
        std::process::id()
    )))?;
    let attempt = (|| -> Result<(FileIdentity, FileIdentity, String)> {
        let snapshot = staging.join("snapshot");
        oe_core::create_new_private_dir(&snapshot)?;
        let mut modes = Vec::new();
        for row in &manifest.files {
            let destination = snapshot.join(&row.path);
            oe_core::ensure_directory(destination.parent().context("package_parent_missing")?)?;
            modes.push(copy_locked(
                &root.path().join(&row.path),
                &destination,
                row,
            )?);
        }
        let snapshot_root = SkillRoot::new(&snapshot)?;
        let report = crate::privacy::scan(
            &snapshot_root,
            &crate::privacy::ScanOptions {
                git: PathBuf::from("git"),
                output: PathBuf::from("var/privacy"),
                tracked_only: false,
                include_ignored: true,
            },
        )?;
        verify_privacy(&report, &manifest)?;
        let mut documents = BTreeMap::new();
        for row in &manifest.files {
            if row.path == "Cargo.toml" || row.path.ends_with("/Cargo.toml") {
                documents.insert(row.path.clone(), read_toml(&snapshot.join(&row.path))?);
            }
        }
        require_workspace(&manifest, &documents)?;
        check_source_closure(root, &manifest)?;
        oe_core::write_new_bytes(&staging.join(MANIFEST_NAME), &bytes)?;
        let archive = staging.join("skill.zip");
        let skill_bytes = oe_core::read_regular_bytes(&snapshot.join("SKILL.md"), MAX_SKILL_BYTES)?;
        let prefix = archive_prefix(&skill_bytes)?;
        archive_snapshot(&snapshot, &archive, &manifest, &modes, prefix)?;
        let identity = fingerprint_file(&archive, MAX_ARCHIVE_BYTES)?;
        let received = verify_archive(&archive, &staging.join(MANIFEST_NAME), &identity.sha256)?;
        let privacy_identity =
            fingerprint_file(&snapshot.join("var/privacy/report.json"), 16 * 1024 * 1024)?;
        oe_core::write_new(
            &staging.join("receipt.json"),
            &json!({
                "schema":"ontology-engineering.portable-package-receipt/v1",
                "status":"transport_verified","inventory_input_sha256":sha256(&bytes),
                "archive_namespace":prefix,
                "archive":identity,"receiver":received,"privacy_report":privacy_identity,
                "privacy_report_path":"snapshot/var/privacy/report.json",
                "member_modes":manifest.files.iter().zip(&modes).map(|(row,mode)|json!({"path":row.path,"mode":mode})).collect::<Vec<_>>(),
                "engineering_acceptance":"not_established","publication":"not_performed"
            }),
        )?;
        ensure!(
            oe_core::read_regular_bytes(&inventory, MAX_MANIFEST_BYTES)? == bytes,
            "package_inventory_changed"
        );
        Ok((
            identity,
            fingerprint_file(&staging.join(MANIFEST_NAME), MAX_MANIFEST_BYTES as u64)?,
            prefix.to_owned(),
        ))
    })();
    let (archive_identity, manifest_identity, archive_namespace) = match attempt {
        Ok(value) => value,
        Err(error) => {
            oe_core::write_new(
                &staging.join("failure.json"),
                &json!({
                    "schema":"ontology-engineering.portable-package-failure/v1",
                    "status":"failed","error":format!("{error:#}"),"output_published":false
                }),
            )
            .context("package_failure_receipt_write_failed")?;
            return Err(error)
                .with_context(|| format!("package_attempt_retained: {}", staging.display()));
        }
    };
    let receipt_identity = fingerprint_file(&staging.join("receipt.json"), 4 * 1024 * 1024)?;
    oe_core::ensure_directory(output.parent().context("package_output_parent_missing")?)?;
    oe_core::rename_directory_exclusive(&staging, &output)?;
    Ok(PackageResult {
        directory: output,
        archive_identity,
        manifest_identity,
        receipt_identity,
        archive_namespace,
    })
}
pub fn verify_package(result: &PackageResult) -> Result<Value> {
    ensure!(
        fingerprint_file(
            &result.directory.join(MANIFEST_NAME),
            MAX_MANIFEST_BYTES as u64
        )? == result.manifest_identity,
        "package_manifest_receipt_changed"
    );
    ensure!(
        fingerprint_file(&result.directory.join("receipt.json"), 4 * 1024 * 1024)?
            == result.receipt_identity,
        "package_receipt_changed"
    );
    let receipt = oe_core::json::strict_json(&oe_core::read_regular_bytes(
        &result.directory.join("receipt.json"),
        4 * 1024 * 1024,
    )?)?;
    let privacy: FileIdentity = serde_json::from_value(receipt["privacy_report"].clone())?;
    ensure!(
        fingerprint_file(
            &result.directory.join("snapshot/var/privacy/report.json"),
            16 * 1024 * 1024
        )? == privacy,
        "package_privacy_receipt_changed"
    );
    ensure!(
        receipt["archive_namespace"] == result.archive_namespace,
        "package_namespace_receipt_changed"
    );
    verify_archive(
        &result.directory.join("skill.zip"),
        &result.directory.join(MANIFEST_NAME),
        &result.archive_identity.sha256,
    )
}
/// Receiver verification needs an independently retained archive digest and
/// external inventory. An internal manifest alone is not identity proof.
pub fn verify_archive(
    archive_path: &Path,
    inventory: &Path,
    expected_sha256: &str,
) -> Result<Value> {
    ensure!(
        digest_valid(expected_sha256),
        "package_invalid_expected_digest"
    );
    let inventory_bytes = oe_core::read_regular_bytes(inventory, MAX_MANIFEST_BYTES)?;
    let manifest = decode_manifest(&inventory_bytes)?;
    let inventory_identity = FileIdentity {
        bytes: inventory_bytes.len() as u64,
        sha256: sha256(&inventory_bytes),
    };
    let mut file = open_regular_file(archive_path, MAX_ARCHIVE_BYTES)?;
    let before = file.metadata()?;
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .context("package_archive_size_overflow")?;
        ensure!(bytes <= MAX_ARCHIVE_BYTES, "package_archive_size_limit");
        digest.update(&buffer[..n]);
    }
    ensure!(
        bytes == before.len() && format!("{:x}", digest.finalize()) == expected_sha256,
        "package_archive_digest_mismatch"
    );
    let raw = container::inventory(&mut file)?;
    file.seek(SeekFrom::Start(0))?;
    let mut archive = ZipArchive::new(file)?;
    ensure!(
        archive.offset() == 0
            && archive.len() == raw.len()
            && raw.len() == manifest.files.len() + 1,
        "package_archive_inventory_differs"
    );
    let prefix = if raw
        .iter()
        .any(|member| member.name == format!("{V2_PREFIX}SKILL.md"))
    {
        V2_PREFIX
    } else {
        PREFIX
    };
    let skill_row = manifest
        .files
        .iter()
        .find(|f| f.path == "SKILL.md")
        .context("package_skill_entry_missing")?;
    let skill_bytes = {
        let mut member = archive.by_name(&format!("{prefix}SKILL.md"))?;
        ensure!(
            member.size() == skill_row.bytes && member.size() <= MAX_SKILL_BYTES as u64,
            "package_skill_entry_limit"
        );
        let mut bytes = Vec::new();
        (&mut member)
            .take(MAX_SKILL_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == skill_row.bytes && sha256(&bytes) == skill_row.sha256,
            "package_skill_entry_digest_mismatch"
        );
        bytes
    };
    ensure!(
        archive_prefix(&skill_bytes)? == prefix,
        "package_skill_namespace_mismatch"
    );
    let expected: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|v| (format!("{prefix}{}", v.path), v))
        .collect();
    let mut internal_seen = false;
    let mut documents = BTreeMap::new();
    for (index, observed) in raw.iter().enumerate() {
        let mut member = archive.by_index(index)?;
        ensure!(
            member.name_raw() == observed.name.as_bytes()
                && member.name() == observed.name
                && member.size() == observed.bytes
                && member.header_start() == observed.offset
                && member.data_start() == observed.data_start
                && !member.encrypted()
                && member.is_file(),
            "package_member_metadata_differs"
        );
        if member.name() == format!("{prefix}{MANIFEST_NAME}") {
            ensure!(
                !internal_seen && member.size() <= MAX_MANIFEST_BYTES as u64,
                "package_internal_manifest_limit"
            );
            let mut internal = Vec::new();
            (&mut member)
                .take(MAX_MANIFEST_BYTES as u64 + 1)
                .read_to_end(&mut internal)?;
            ensure!(
                internal.len() <= MAX_MANIFEST_BYTES && decode_manifest(&internal)? == manifest,
                "package_internal_manifest_differs"
            );
            internal_seen = true;
            continue;
        }
        let row = expected
            .get(member.name())
            .context("package_unlisted_archive_member")?;
        ensure!(member.size() == row.bytes, "package_member_size_differs");
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut cargo = Vec::new();
        let is_cargo = row.path == "Cargo.toml" || row.path.ends_with("/Cargo.toml");
        if is_cargo {
            ensure!(row.bytes <= 4 * 1024 * 1024, "package_cargo_size_limit");
        }
        loop {
            let n = member.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(n as u64)
                .context("package_expanded_size_overflow")?;
            ensure!(count <= row.bytes, "package_expanded_size_differs");
            hash.update(&buffer[..n]);
            if is_cargo {
                cargo.extend_from_slice(&buffer[..n]);
            }
        }
        ensure!(
            count == row.bytes && format!("{:x}", hash.finalize()) == row.sha256,
            "package_member_digest_mismatch"
        );
        if is_cargo {
            documents.insert(
                row.path.clone(),
                toml::from_str(std::str::from_utf8(&cargo)?)?,
            );
        }
    }
    ensure!(internal_seen, "package_internal_manifest_missing");
    require_workspace(&manifest, &documents)?;
    let file = archive.into_inner();
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len() && before.modified()? == after.modified()?,
        "package_archive_changed_during_readback"
    );
    ensure!(
        fingerprint_file(archive_path, MAX_ARCHIVE_BYTES)?
            == FileIdentity {
                bytes,
                sha256: expected_sha256.to_owned()
            },
        "package_archive_path_changed"
    );
    ensure!(
        fingerprint_file(inventory, MAX_MANIFEST_BYTES as u64)? == inventory_identity,
        "package_receiver_inventory_changed"
    );
    Ok(
        json!({"schema":"ontology-engineering.portable-package-verification/v1",
        "status":"transport_verified","archive_sha256":expected_sha256,"archive_bytes":bytes,
        "archive_namespace":prefix,
        "file_count":manifest.files.len(),"inventory":inventory_identity,
        "scope":"exact_archive_and_member_bytes","receiver_cargo_execution":"not_run",
        "privacy":"producer_receipt_required","publication":"not_performed","engineering_acceptance":"not_established"}),
    )
}
