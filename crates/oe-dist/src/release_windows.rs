//! Windows /2 release owner. Transaction order and independent byte checks follow
//! the macOS owner; permissions, identity, locking and commit use native Windows
//! primitives. Trusted local users cooperate through the persistent store lock.
//! Local NTFS only; no claim of crash/ power-loss durability or protection from a
//! trusted user deliberately ignoring the lock.
use super::windows_fs::{self as platform, AdvisoryFileLock, Stamp};
use super::*;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_MANIFEST: usize = 8 * 1024 * 1024;
const MAX_RECORD: usize = 64 * 1024;
const CURRENT: &str = "current.json";
const STORE: &str = "store.json";
const STORE_BYTES: &[u8] =
    b"{\"schema\":\"oev2.release-store/1\",\"scope\":\"local_distribution_activation_only\"}\n";
static SERIAL: AtomicU64 = AtomicU64::new(0);

fn existing_dir(path: &Path) -> Result<PathBuf> {
    Ok(platform::directory(path)?.path().to_owned())
}
fn new_destination(path: &Path) -> Result<PathBuf> {
    platform::validate_path(path)?;
    ensure!(
        fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "release_destination_exists_or_unavailable"
    );
    let parent = existing_dir(path.parent().context("release_parent_missing")?)?;
    Ok(parent.join(path.file_name().context("release_name_missing")?))
}
fn stamp(path: &Path) -> Result<Stamp> {
    platform::stamp(path, true)
}

fn nonce() -> Result<String> {
    Ok(format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ))
}
fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn safe_mode(mode: u64) -> bool {
    mode == 0
}

fn manifest(bytes: &[u8]) -> Result<ReleaseManifest> {
    let value = oe_core::json::strict_json(bytes)?;
    let object = value
        .as_object()
        .context("release_manifest_object_required")?;
    ensure!(
        object.len() == 6
            && [
                "schema",
                "version",
                "target",
                "entry",
                "skill_root",
                "files"
            ]
            .iter()
            .all(|name| object.contains_key(*name)),
        "release_manifest_unknown_or_missing_field"
    );
    for value in object["files"]
        .as_array()
        .context("release_manifest_files_required")?
    {
        let row = value
            .as_object()
            .context("release_manifest_file_object_required")?;
        ensure!(
            row.len() == 4
                && ["path", "sha256", "bytes", "mode"]
                    .iter()
                    .all(|name| row.contains_key(*name)),
            "release_manifest_file_unknown_or_missing_field"
        );
    }
    let m: ReleaseManifest = serde_json::from_value(value)?;
    ensure!(
        m.schema == SCHEMA && m.target == TARGET && m.entry == ENTRY && m.skill_root == "skill",
        "release_manifest_contract"
    );
    ensure!(
        !m.version.is_empty()
            && m.version.len() <= 128
            && m.version
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || b".-+".contains(&v)),
        "release_version_invalid"
    );
    ensure!(
        m.files.len() <= crate::package::MAX_FILES,
        "release_inventory_limit"
    );
    let mut public = Vec::new();
    let mut binaries = BTreeSet::new();
    let mut previous: Option<&str> = None;
    let mut total = 0u64;
    for row in &m.files {
        ensure!(
            previous.is_none_or(|old| old < row.path.as_str()),
            "release_inventory_order_or_duplicate"
        );
        previous = Some(&row.path);
        ensure!(
            digest_valid(&row.sha256)
                && row.bytes <= crate::package::MAX_FILE_BYTES
                && safe_mode(row.mode),
            "release_file_identity_invalid"
        );
        total = total
            .checked_add(row.bytes)
            .context("release_size_overflow")?;
        ensure!(
            total <= crate::package::MAX_TOTAL_BYTES,
            "release_total_size_limit"
        );
        if let Some(name) = row.path.strip_prefix("skill/") {
            public.push(crate::package::SourceFile {
                path: name.into(),
                sha256: row.sha256.clone(),
                bytes: row.bytes,
            });
        } else if let Some(name) = row.path.strip_prefix("bin/") {
            ensure!(
                PROGRAM_NAMES.contains(&name) && row.mode == 0,
                "release_program_invalid"
            );
            binaries.insert(name);
        } else {
            anyhow::bail!("release_payload_outside_bin_or_skill");
        }
    }
    ensure!(
        binaries.len() == PROGRAM_NAMES.len(),
        "release_program_missing"
    );
    // Reuse the original transport owner's required files, privacy exclusions,
    // portable path/case-collision and namespace rules for the public skill.
    crate::package::PortableManifest {
        format: crate::package::FORMAT.into(),
        entry: "SKILL.md".into(),
        scope: crate::package::SCOPE.into(),
        files: public,
    }
    .validate()?;
    for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
        ensure!(
            m.files
                .iter()
                .any(|row| row.path == format!("skill/{name}")),
            "release_source_lock_missing"
        );
    }
    Ok(m)
}

fn tree(root: &Path, m: &ReleaseManifest, private: bool) -> Result<BTreeMap<String, Stamp>> {
    let mut expected = BTreeMap::<String, bool>::new();
    expected.insert(MANIFEST_NAME.into(), false);
    for row in &m.files {
        expected.insert(row.path.clone(), false);
        let mut parent = Path::new(&row.path).parent();
        while let Some(path) = parent.filter(|v| !v.as_os_str().is_empty()) {
            expected.insert(path.to_str().context("release_path_utf8")?.into(), true);
            parent = path.parent();
        }
    }
    let mut observed = BTreeMap::new();
    observed.insert(String::new(), platform::stamp(root, true)?);
    fn walk(
        root: &Path,
        dir: &Path,
        expected: &BTreeMap<String, bool>,
        private: bool,
        observed: &mut BTreeMap<String, Stamp>,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            let relative = oe_core::portable_source_path(root, &path)?;
            let metadata = fs::symlink_metadata(&path)?;
            platform::reject_reparse(&metadata)?;
            if matches!(relative.as_str(), "skill/var" | "skill/sources") {
                ensure!(
                    private && metadata.is_dir(),
                    "release_private_payload_forbidden"
                );
                continue; // Never traverse private user/project data.
            }
            let directory = expected.get(&relative).context("release_unlisted_path")?;
            ensure!(
                if *directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                },
                "release_file_type_mismatch"
            );
            observed.insert(relative, platform::stamp(&path, true)?);
            if *directory {
                walk(root, &path, expected, private, observed)?;
            }
        }
        Ok(())
    }
    walk(root, root, &expected, private, &mut observed)?;
    ensure!(
        observed.len() == expected.len() + 1 && expected.keys().all(|v| observed.contains_key(v)),
        "release_namespace_incomplete"
    );
    Ok(observed)
}

fn inspect_file(path: &Path, expected: &ReleaseFile) -> Result<Stamp> {
    ensure!(expected.mode == 0, "release_windows_mode_must_be_zero");
    let mut file = platform::open_file(path, expected.bytes, true)?;
    let before = file.stamp()?;
    ensure!(
        before.bytes == expected.bytes,
        "release_size_or_mode_mismatch"
    );
    let mut count = 0u64;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .context("release_size_overflow")?;
        ensure!(count <= expected.bytes, "release_file_grew");
        digest.update(&buffer[..n]);
    }
    ensure!(
        count == expected.bytes && format!("{:x}", digest.finalize()) == expected.sha256,
        "release_digest_mismatch"
    );
    ensure!(
        before == file.stamp()? && before == platform::stamp(path, true)?,
        "release_file_changed"
    );
    Ok(before)
}

struct Checked {
    release: VerifiedRelease,
    manifest: ReleaseManifest,
    bytes: Vec<u8>,
    snapshot: BTreeMap<String, Stamp>,
}
fn check(root: &Path, expected_sha: &str, private: bool) -> Result<Checked> {
    ensure!(
        digest_valid(expected_sha),
        "release_expected_digest_invalid"
    );
    let root_pin = platform::directory(root)?;
    let root = root_pin.path().to_owned();
    let manifest_path = root.join(MANIFEST_NAME);
    let manifest_before = platform::stamp(&manifest_path, true)?;
    let bytes = platform::read_bytes(&manifest_path, MAX_MANIFEST)?;
    ensure!(
        oe_core::sha256(&bytes) == expected_sha,
        "release_manifest_digest_mismatch"
    );
    let m = manifest(&bytes)?;
    let before = tree(&root, &m, private)?;
    ensure!(
        before.get(MANIFEST_NAME) == Some(&manifest_before),
        "release_manifest_changed"
    );
    for row in &m.files {
        ensure!(
            before.get(&row.path) == Some(&inspect_file(&root.join(&row.path), row)?),
            "release_file_changed"
        );
    }
    ensure!(
        std::str::from_utf8(&platform::read_bytes(&root.join("skill/VERSION"), 1024)?)?.trim()
            == m.version,
        "release_version_file_mismatch"
    );
    let cargo_bytes = platform::read_bytes(&root.join("skill/Cargo.toml"), 4 * 1024 * 1024)?;
    let cargo: toml::Value = toml::from_str(std::str::from_utf8(&cargo_bytes)?)?;
    ensure!(
        cargo
            .get("workspace")
            .and_then(|v| v.get("package"))
            .and_then(|v| v.get("version"))
            .and_then(toml::Value::as_str)
            == Some(m.version.as_str()),
        "release_cargo_version_mismatch"
    );
    let skill = platform::read_bytes(&root.join("skill/SKILL.md"), 1024 * 1024)?;
    ensure!(
        crate::package::declared_skill_name(&skill)? == Some("ontology-engineering-v2"),
        "release_skill_identity_mismatch"
    );
    ensure!(before == tree(&root, &m, private)?, "release_tree_changed");
    let programs = m
        .files
        .iter()
        .filter(|v| v.path.starts_with("bin/"))
        .map(|v| ProgramReference {
            path: root.join(&v.path),
            sha256: v.sha256.clone(),
            bytes: v.bytes,
            mode: v.mode,
        })
        .collect();
    let release = VerifiedRelease {
        skill_root: root.join("skill"),
        entry: root.join(&m.entry),
        root,
        version: m.version.clone(),
        target: m.target.clone(),
        manifest_sha256: expected_sha.into(),
        programs,
    };
    Ok(Checked {
        release,
        manifest: m,
        bytes,
        snapshot: before,
    })
}

/// Verify every public file and reject extras, while never scanning skill/var or
/// skill/sources. A valid bundle is not a privacy-scan or execution qualification.
pub fn verify(root: &Path, expected_sha: &str) -> Result<VerifiedRelease> {
    Ok(check(root, expected_sha, true)?.release)
}

fn copy_file(
    source: &Path,
    destination: &Path,
    expected: &ReleaseFile,
    original: &Stamp,
) -> Result<()> {
    let mut input = platform::open_file(source, expected.bytes, true)?;
    ensure!(original == &input.stamp()?, "release_copy_source_changed");
    let mut output = platform::create_file(destination)?;
    let mut digest = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = input.file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .context("release_size_overflow")?;
        ensure!(count <= expected.bytes, "release_copy_source_grew");
        output.write_all(&buffer[..n])?;
        digest.update(&buffer[..n]);
    }
    ensure!(
        count == expected.bytes && format!("{:x}", digest.finalize()) == expected.sha256,
        "release_copy_digest_mismatch"
    );
    ensure!(
        original == &input.stamp()? && original == &platform::stamp(source, true)?,
        "release_copy_source_changed"
    );
    output.sync_all()?;
    drop(output);
    inspect_file(destination, expected)?;
    Ok(())
}

/// Install only into a nonexistent explicit root. A failed staging tree is kept
/// for diagnosis; no existing version or private data is deleted or overwritten.
pub fn install(source: &Path, expected_sha: &str, new_root: &Path) -> Result<VerifiedRelease> {
    let _source_pin = platform::directory(source)?;
    let original = check(source, expected_sha, false)?;
    let destination = new_destination(new_root)?;
    ensure!(
        !destination.starts_with(&original.release.root)
            && !original.release.root.starts_with(&destination),
        "release_install_roots_overlap"
    );
    let parent = destination.parent().context("release_parent_missing")?;
    let _parent_pin = platform::directory(parent)?;
    let parent_stamp = platform::stamp(parent, false)?;
    let staging = parent.join(format!(".oev2-release-stage-{}", nonce()?));
    platform::create_dir(&staging)?;
    let staged = (|| -> Result<()> {
        for row in &original.manifest.files {
            let output = staging.join(&row.path);
            platform::ensure_directories(
                &staging,
                output.parent().context("release_parent_missing")?,
            )?;
            copy_file(
                &original.release.root.join(&row.path),
                &output,
                row,
                original
                    .snapshot
                    .get(&row.path)
                    .context("release_source_snapshot_missing")?,
            )?;
        }
        platform::write_new_bytes(&staging.join(MANIFEST_NAME), &original.bytes)?;
        check(&staging, expected_sha, false)?;
        let after = check(&original.release.root, expected_sha, false)?;
        ensure!(
            original.snapshot == after.snapshot,
            "release_install_source_changed"
        );
        ensure!(
            parent_stamp.same_object(&platform::stamp(parent, false)?),
            "release_install_parent_changed"
        );
        Ok(())
    })();
    staged.with_context(|| format!("release_failed_stage_retained:{}", staging.display()))?;
    oe_core::rename_directory_exclusive(&staging, &destination)?;
    // Publication does not activate the tree; an error after publication cannot
    // damage a prior current record. The explicit destination remains inspectable.
    verify(&destination, expected_sha)
}

/// Initialize an explicit new local activation store, never adopting an
/// unrelated existing directory. Release payloads live outside this store.
pub fn init_store(new_state_dir: &Path) -> Result<()> {
    let destination = new_destination(new_state_dir)?;
    let parent = destination.parent().context("release_parent_missing")?;
    let _parent_pin = platform::directory(parent)?;
    let stage = parent.join(format!(".oev2-release-store-stage-{}", nonce()?));
    platform::create_dir(&stage)?;
    platform::write_new_bytes(&stage.join(STORE), STORE_BYTES)?;
    oe_core::rename_directory_exclusive(&stage, &destination)?;
    Ok(())
}

fn open_store(path: &Path) -> Result<(PathBuf, Stamp)> {
    let root = existing_dir(path)?;
    ensure!(
        platform::stamp(&root, true)?.directory,
        "release_store_mode_invalid"
    );
    ensure!(
        !platform::stamp(&root.join(STORE), true)?.directory,
        "release_store_marker_mode_invalid"
    );
    ensure!(
        platform::read_bytes(&root.join(STORE), MAX_RECORD)? == STORE_BYTES,
        "release_store_identity_mismatch"
    );
    Ok((root.clone(), stamp(&root)?))
}
fn load_record(store: &Path) -> Result<Option<(ActivationRecord, String, Stamp)>> {
    let path = store.join(CURRENT);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    let before = platform::stamp(&path, true)?;
    let bytes = platform::read_bytes(&path, MAX_RECORD)?;
    let record: ActivationRecord = serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    ensure!(
        record.schema == "oev2.release-current/1"
            && record.scope == SCOPE
            && record.generation > 0
            && matches!(record.operation.as_str(), "activate" | "rollback"),
        "release_current_contract"
    );
    ensure!(
        before == platform::stamp(&path, true)?,
        "release_current_changed"
    );
    Ok(Some((record, oe_core::sha256(&bytes), before)))
}
fn checked_reference(reference: &ReleaseReference) -> Result<VerifiedRelease> {
    let release = verify(&reference.root, &reference.manifest_sha256)?;
    ensure!(
        &release.reference() == reference,
        "release_reference_mismatch"
    );
    Ok(release)
}
fn assert_expected(
    record: &Option<(ActivationRecord, String, Stamp)>,
    expected: Option<&str>,
) -> Result<()> {
    ensure!(
        record.as_ref().map(|v| v.1.as_str()) == expected,
        "release_current_precondition_failed"
    );
    Ok(())
}

/// Read under the same lock as activation and rehash the actual current tree.
/// The returned entry must be launched with the returned skill_root explicitly.
pub fn read_active(state_dir: &Path) -> Result<ActiveRelease> {
    let (store, before) = open_store(state_dir)?;
    let lock = AdvisoryFileLock::acquire(&store.join(".release.lock"), false)?;
    let original = load_record(&store)?.context("release_current_missing")?;
    let release = checked_reference(&original.0.current)?;
    ensure!(
        load_record(&store)? == Some(original.clone()),
        "release_current_changed"
    );
    ensure!(before.same_object(&stamp(&store)?), "release_store_changed");
    lock.ensure_current()?;
    Ok(ActiveRelease {
        record: original.0,
        record_sha256: original.1,
        release,
    })
}

/// First activation requires expected_current_sha=None. Upgrades require the
/// exact digest returned by read_active. This is local transport activation, not
/// a replacement for evolution's qualification/selection/feedback owner.
pub fn activate(
    state_dir: &Path,
    root: &Path,
    expected_manifest_sha: &str,
    expected_current_sha: Option<&str>,
) -> Result<ActiveRelease> {
    transition(
        state_dir,
        Some((root, expected_manifest_sha)),
        expected_current_sha,
        |_| Ok(()),
    )
}

/// Rehash the previous release before swapping the local record. Corruption of
/// the current payload does not prevent rollback to an intact previous release.
pub fn rollback(state_dir: &Path, expected_current_sha: &str) -> Result<ActiveRelease> {
    transition(state_dir, None, Some(expected_current_sha), |_| Ok(()))
}

fn transition(
    state_dir: &Path,
    requested: Option<(&Path, &str)>,
    expected_current: Option<&str>,
    before_publish: impl FnOnce(&Path) -> Result<()>,
) -> Result<ActiveRelease> {
    let (store, store_before) = open_store(state_dir)?;
    let lock = AdvisoryFileLock::acquire(&store.join(".release.lock"), true)?;
    let original = load_record(&store)?;
    assert_expected(&original, expected_current)?;
    let release = if let Some((root, sha)) = requested {
        let selected = verify(root, sha)?;
        if let Some((old, _, _)) = &original {
            checked_reference(&old.current)?; // Preserve a usable rollback source on upgrades.
            ensure!(
                old.current != selected.reference(),
                "release_already_current"
            );
            ensure!(
                !selected.root.starts_with(&old.current.root)
                    && !old.current.root.starts_with(&selected.root),
                "release_versions_overlap"
            );
        }
        selected
    } else {
        let previous = original
            .as_ref()
            .and_then(|v| v.0.previous.as_ref())
            .context("release_previous_missing")?;
        checked_reference(previous)?
    };
    ensure!(
        !store.starts_with(&release.root) && !release.root.starts_with(&store),
        "release_store_payload_overlap"
    );
    let record = ActivationRecord {
        schema: "oev2.release-current/1".into(),
        scope: SCOPE.into(),
        generation: original
            .as_ref()
            .map_or(0, |v| v.0.generation)
            .checked_add(1)
            .context("release_generation_overflow")?,
        operation: if requested.is_some() {
            "activate"
        } else {
            "rollback"
        }
        .into(),
        current: release.reference(),
        previous: original.as_ref().map(|v| v.0.current.clone()),
    };
    let mut bytes = serde_json::to_vec_pretty(&record)?;
    bytes.push(b'\n');
    let temp = store.join(format!(".current-prepared-{}.json", nonce()?));
    platform::write_new_bytes(&temp, &bytes)?;
    before_publish(&temp)?;
    // All expected failures occur before the single commit point. The prepared
    // record remains after failure and does not alter the old current record.
    ensure!(load_record(&store)? == original, "release_current_changed");
    ensure!(
        store_before.same_object(&stamp(&store)?),
        "release_store_changed"
    );
    ensure!(
        platform::read_bytes(&store.join(STORE), MAX_RECORD)? == STORE_BYTES,
        "release_store_identity_mismatch"
    );
    lock.ensure_current()?;
    checked_reference(&record.current)?;
    if requested.is_some() {
        if let Some(previous) = &record.previous {
            checked_reference(previous)?;
        }
    }
    ensure!(
        platform::read_bytes(&temp, MAX_RECORD)? == bytes,
        "release_prepared_record_changed"
    );
    ensure!(load_record(&store)? == original, "release_current_changed");
    platform::replace_current(
        &temp,
        &store.join(CURRENT),
        &bytes,
        original.as_ref().map(|v| &v.2),
    )
    .context("release_commit_may_have_completed_read_current_before_retry")?;
    // The native handle rename is the only commit. There is intentionally no
    // fallible post-commit work whose failure could be mistaken for no change.
    Ok(ActiveRelease {
        record_sha256: oe_core::sha256(&bytes),
        record,
        release,
    })
}

#[cfg(test)]
mod failure_tests {
    use super::*;

    // Fixture setup uses the native creation helper only to establish the
    // platform's required ACL. The transaction oracle is the independently saved
    // exact current bytes, generation/reference and injected failure location;
    // separate integration tests construct ACLs using PowerShell/.NET.
    fn fixture(root: &Path, version: &str) -> Result<String> {
        platform::create_dir(root)?;
        let mut values = vec![
            (
                "skill/SKILL.md".to_owned(),
                b"---\nname: ontology-engineering-v2\n---\n".to_vec(),
            ),
            ("skill/VERSION".to_owned(), version.as_bytes().to_vec()),
            (
                "skill/Cargo.toml".to_owned(),
                format!("[workspace.package]\nversion = \"{version}\"\n").into_bytes(),
            ),
        ];
        for name in [
            "AGENTS.md",
            "README.md",
            "LICENSE",
            "Cargo.lock",
            "rust-toolchain.toml",
        ] {
            values.push((format!("skill/{name}"), b"synthetic only\n".to_vec()));
        }
        for name in PROGRAM_NAMES {
            values.push((format!("bin/{name}"), b"not executed\n".to_vec()));
        }
        values.sort_by(|a, b| a.0.cmp(&b.0));
        let mut rows = Vec::new();
        for (path, bytes) in values {
            platform::ensure_directories(
                root,
                root.join(&path).parent().context("fixture_parent")?,
            )?;
            platform::write_new_bytes(&root.join(&path), &bytes)?;
            rows.push(ReleaseFile {
                path,
                sha256: oe_core::sha256(&bytes),
                bytes: bytes.len() as u64,
                mode: 0,
            });
        }
        let manifest = ReleaseManifest {
            schema: "oev2.release-manifest/2".into(),
            version: version.into(),
            target: "x86_64-pc-windows-msvc".into(),
            entry: "bin/oev2.exe".into(),
            skill_root: "skill".into(),
            files: rows,
        };
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        platform::write_new_bytes(&root.join(MANIFEST_NAME), &bytes)?;
        Ok(oe_core::sha256(&bytes))
    }
    #[test]
    fn prepared_record_failure_does_not_replace_current() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        let store = root.join("store");
        init_store(&store)?;
        let first = fixture(&root.join("first"), "2.0.0-rc.1")?;
        let second = fixture(&root.join("second"), "2.0.0-rc.2")?;
        let current = activate(&store, &root.join("first"), &first, None)?;
        let original = fs::read(store.join(CURRENT))?;
        let result = transition(
            &store,
            Some((&root.join("second"), &second)),
            Some(&current.record_sha256),
            |_| anyhow::bail!("independent_injected_precommit_failure"),
        );
        assert!(result.is_err());
        assert_eq!(fs::read(store.join(CURRENT))?, original);
        assert_eq!(read_active(&store)?, current);
        assert!(fs::read_dir(&store)?.filter_map(Result::ok).any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".current-prepared-")
        }));
        Ok(())
    }
    #[test]
    fn current_replacement_during_preparation_is_not_overwritten() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        let store = root.join("store");
        init_store(&store)?;
        let first = fixture(&root.join("first"), "2.0.0-rc.1")?;
        let second = fixture(&root.join("second"), "2.0.0-rc.2")?;
        let current = activate(&store, &root.join("first"), &first, None)?;
        let mut foreign = current.record.clone();
        foreign.generation = 9;
        let foreign_bytes = serde_json::to_vec(&foreign)?;
        let result = transition(
            &store,
            Some((&root.join("second"), &second)),
            Some(&current.record_sha256),
            |_| {
                fs::write(store.join(CURRENT), &foreign_bytes)?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(store.join(CURRENT))?, foreign_bytes);
        Ok(())
    }
    #[test]
    fn prepared_bytes_change_refuses_without_replacing_current() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        let store = root.join("store");
        init_store(&store)?;
        let first = fixture(&root.join("first"), "2.0.0-rc.1")?;
        let second = fixture(&root.join("second"), "2.0.0-rc.2")?;
        let current = activate(&store, &root.join("first"), &first, None)?;
        let original = fs::read(store.join(CURRENT))?;
        let result = transition(
            &store,
            Some((&root.join("second"), &second)),
            Some(&current.record_sha256),
            |prepared| {
                fs::write(prepared, b"independent changed prepared bytes")?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(store.join(CURRENT))?, original);
        assert_eq!(read_active(&store)?, current);
        Ok(())
    }
    #[test]
    fn native_shared_locks_coexist_and_exclude_writer() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        let store = root.join("store");
        init_store(&store)?;
        let path = store.join(".release.lock");
        let a = AdvisoryFileLock::acquire(&path, false)?;
        let b = AdvisoryFileLock::acquire(&path, false)?;
        assert!(AdvisoryFileLock::acquire(&path, true).is_err());
        a.ensure_current()?;
        b.ensure_current()?;
        drop((a, b));
        let exclusive = AdvisoryFileLock::acquire(&path, true)?;
        assert!(AdvisoryFileLock::acquire(&path, false).is_err());
        exclusive.ensure_current()?;
        Ok(())
    }
}
