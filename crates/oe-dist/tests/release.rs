#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use anyhow::Result;
use oe_dist::release::{self, MANIFEST_NAME, PROGRAM_NAMES, ReleaseFile, ReleaseManifest, TARGET};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

fn fixture(root: &Path, version: &str) -> Result<String> {
    fs::create_dir(root)?;
    let mut values = vec![
        (
            "skill/SKILL.md".to_owned(),
            b"---\nname: ontology-engineering-v2\n---\n".to_vec(),
            0o644,
        ),
        (
            "skill/VERSION".to_owned(),
            version.as_bytes().to_vec(),
            0o644,
        ),
    ];
    for name in [
        "AGENTS.md",
        "README.md",
        "LICENSE",
        "Cargo.lock",
        "rust-toolchain.toml",
        "sources/README.md",
    ] {
        values.push((format!("skill/{name}"), b"fixture only\n".to_vec(), 0o644));
    }
    values.push((
        "skill/Cargo.toml".into(),
        format!("[workspace.package]\nversion = \"{version}\"\n").into_bytes(),
        0o644,
    ));
    for name in PROGRAM_NAMES {
        values.push((
            format!("bin/{name}"),
            format!("not executed: {version}\n").into_bytes(),
            0o755,
        ));
    }
    values.sort_by(|a, b| a.0.cmp(&b.0));
    let mut files = Vec::new();
    for (path, bytes, mode) in values {
        let destination = root.join(&path);
        fs::create_dir_all(destination.parent().unwrap())?;
        fs::write(&destination, &bytes)?;
        fs::set_permissions(&destination, fs::Permissions::from_mode(mode))?;
        files.push(ReleaseFile {
            path,
            sha256: oe_core::sha256(&bytes),
            bytes: bytes.len() as u64,
            mode: u64::from(mode),
        });
    }
    let m = ReleaseManifest {
        schema: "oev2.release-manifest/1".into(),
        version: version.into(),
        target: TARGET.into(),
        entry: "bin/oev2".into(),
        skill_root: "skill".into(),
        files,
    };
    let bytes = serde_json::to_vec_pretty(&m)?;
    fs::write(root.join(MANIFEST_NAME), &bytes)?;
    fs::set_permissions(root.join(MANIFEST_NAME), fs::Permissions::from_mode(0o600))?;
    Ok(oe_core::sha256(&bytes))
}
fn temporary() -> Result<(tempfile::TempDir, PathBuf)> {
    let directory = tempfile::tempdir()?;
    let root = fs::canonicalize(directory.path())?;
    Ok((directory, root))
}

#[test]
fn release_install_and_atomic_activation_roundtrip() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let installed = root.join("installed");
    let received = release::install(&source, &digest, &installed)?;
    assert_eq!(received.root, installed);
    assert_eq!(received.skill_root, installed.join("skill"));
    assert_eq!(received.entry, installed.join("bin/oev2"));
    assert_eq!(received.programs.len(), 7);
    let store = root.join("state");
    release::init_store(&store)?;
    let first = release::activate(&store, &installed, &digest, None)?;
    assert_eq!(release::read_active(&store)?, first);
    assert_eq!(first.record.scope, "local_distribution_activation_only");
    let next = root.join("next");
    let next_digest = fixture(&next, "2.0.0-rc.2")?;
    let second = release::activate(&store, &next, &next_digest, Some(&first.record_sha256))?;
    assert_eq!(second.record.generation, 2);
    assert_eq!(
        second.record.previous.as_ref(),
        Some(&first.release.reference())
    );
    let old = release::rollback(&store, &second.record_sha256)?;
    assert_eq!(old.release, received);
    assert_eq!(old.record.generation, 3);
    assert_eq!(old.record.operation, "rollback");
    assert_eq!(release::read_active(&store)?, old);
    assert!(source.join("bin/oev2").exists());
    assert!(next.join("bin/oev2").exists());
    Ok(())
}

#[test]
fn existing_install_root_and_stale_activation_are_preserved() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let destination = root.join("existing");
    fs::create_dir(&destination)?;
    fs::write(destination.join("user-data"), b"keep me")?;
    assert!(release::install(&source, &digest, &destination).is_err());
    assert_eq!(fs::read(destination.join("user-data"))?, b"keep me");
    let store = root.join("state");
    release::init_store(&store)?;
    assert!(release::init_store(&store).is_err());
    let active = release::activate(&store, &source, &digest, None)?;
    let old_bytes = fs::read(store.join("current.json"))?;
    assert!(release::activate(&store, &source, &digest, Some(&"0".repeat(64))).is_err());
    assert!(release::activate(&store, &source, &digest, None).is_err());
    assert!(release::rollback(&store, &active.record_sha256).is_err());
    assert_eq!(fs::read(store.join("current.json"))?, old_bytes);
    Ok(())
}

#[test]
fn corrupt_candidate_or_previous_does_not_change_current() -> Result<()> {
    let (_temp, root) = temporary()?;
    let one = root.join("one");
    let two = root.join("two");
    let a = fixture(&one, "2.0.0-rc.1")?;
    let b = fixture(&two, "2.0.0-rc.2")?;
    let store = root.join("state");
    release::init_store(&store)?;
    let first = release::activate(&store, &one, &a, None)?;
    let baseline = fs::read(store.join("current.json"))?;
    assert!(release::activate(&store, &two, &"0".repeat(64), Some(&first.record_sha256)).is_err());
    assert_eq!(fs::read(store.join("current.json"))?, baseline);
    let second = release::activate(&store, &two, &b, Some(&first.record_sha256))?;
    let baseline = fs::read(store.join("current.json"))?;
    fs::write(one.join("bin/oev2"), b"broken")?;
    assert!(release::rollback(&store, &second.record_sha256).is_err());
    assert_eq!(fs::read(store.join("current.json"))?, baseline);
    assert_eq!(release::read_active(&store)?, second);
    Ok(())
}

#[test]
fn corrupt_current_can_roll_back_to_verified_previous() -> Result<()> {
    let (_temp, root) = temporary()?;
    let one = root.join("one");
    let two = root.join("two");
    let a = fixture(&one, "2.0.0-rc.1")?;
    let b = fixture(&two, "2.0.0-rc.2")?;
    let store = root.join("state");
    release::init_store(&store)?;
    let first = release::activate(&store, &one, &a, None)?;
    let second = release::activate(&store, &two, &b, Some(&first.record_sha256))?;
    fs::write(two.join("bin/oev2"), b"broken current program")?;
    assert!(release::read_active(&store).is_err());
    let recovered = release::rollback(&store, &second.record_sha256)?;
    assert_eq!(recovered.release, first.release);
    Ok(())
}

#[test]
fn release_rejects_symlinks_modes_extra_public_and_private_distribution() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let program = source.join("bin/oev2");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o644))?;
    assert!(release::verify(&source, &digest).is_err());
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    let extra = source.join("skill/unlisted.rs");
    fs::write(&extra, b"extra")?;
    assert!(release::verify(&source, &digest).is_err());
    fs::remove_file(&extra)?;
    let original = fs::read(&program)?;
    let outside = root.join("outside");
    fs::write(&outside, &original)?;
    fs::remove_file(&program)?;
    symlink(&outside, &program)?;
    assert!(release::verify(&source, &digest).is_err());
    fs::remove_file(&program)?;
    fs::write(&program, original)?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    fs::create_dir(source.join("skill/var"))?;
    // The verifier intentionally never reads private contents, including links.
    symlink(
        root.join("does-not-exist"),
        source.join("skill/var/private-input"),
    )?;
    assert!(release::verify(&source, &digest).is_ok());
    assert!(release::install(&source, &digest, &root.join("received")).is_err());
    assert!(!root.join("received").exists());
    Ok(())
}

#[test]
fn installed_sources_are_private_but_public_readme_remains_verified() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    release::install(&source, &digest, &root.join("clean-install"))?;
    let private = source.join("skill/sources/private.pdf");
    fs::write(&private, b"private original")?;
    let catalog = source.join("skill/sources/.catalog");
    fs::create_dir(&catalog)?;
    symlink(
        root.join("unavailable-private-source"),
        catalog.join("unreadable"),
    )?;
    symlink(
        root.join("unavailable-direct-private-source"),
        source.join("skill/sources/unreadable"),
    )?;
    assert!(release::verify(&source, &digest).is_ok());
    fs::write(&private, b"updated private original")?;
    assert!(release::verify(&source, &digest).is_ok());
    assert!(release::install(&source, &digest, &root.join("private-install")).is_err());
    assert!(!root.join("private-install").exists());
    let public = source.join("skill/sources/README.md");
    fs::remove_file(&public)?;
    assert!(release::verify(&source, &digest).is_err());
    symlink(root.join("absent-public-readme"), &public)?;
    assert!(release::verify(&source, &digest).is_err());
    fs::remove_file(&public)?;
    fs::write(public, b"tampered public instructions")?;
    assert!(release::verify(&source, &digest).is_err());
    assert_eq!(fs::read(private)?, b"updated private original");
    Ok(())
}

#[test]
fn older_release_without_public_sources_directory_accepts_local_library_only() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    fixture(&source, "2.0.0-rc.1")?;
    let manifest_path = source.join(MANIFEST_NAME);
    let mut manifest: ReleaseManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    manifest
        .files
        .retain(|row| row.path != "skill/sources/README.md");
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    fs::write(manifest_path, &bytes)?;
    fs::remove_file(source.join("skill/sources/README.md"))?;
    fs::write(source.join("skill/sources/private.pdf"), b"local original")?;
    let digest = oe_core::sha256(&bytes);
    assert!(release::verify(&source, &digest).is_ok());
    // A README omitted by the historical manifest stays private under it.
    fs::write(
        source.join("skill/sources/README.md"),
        b"local instructions",
    )?;
    assert!(release::verify(&source, &digest).is_ok());
    assert!(release::install(&source, &digest, &root.join("received")).is_err());
    assert!(!root.join("received").exists());
    Ok(())
}

#[test]
fn activation_lock_refuses_a_competing_writer() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let store = root.join("state");
    release::init_store(&store)?;
    let lock = oe_core::AdvisoryFileLock::acquire(&store.join(".release.lock"), true)?;
    assert!(release::activate(&store, &source, &digest, None).is_err());
    assert!(!store.join("current.json").exists());
    drop(lock);
    assert!(release::activate(&store, &source, &digest, None).is_ok());
    Ok(())
}

#[test]
fn manifest_rejects_escape_private_payload_unknown_fields_and_wrong_target() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    fixture(&source, "2.0.0-rc.1")?;
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join(MANIFEST_NAME))?)?;
    let mut cases = Vec::new();
    let mut changed = original.clone();
    changed["target"] = "x86_64-pc-windows-msvc".into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"][0]["path"] = "skill/../outside".into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"][0]["path"] = "skill/var/private".into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"][0]["mode"] = 0o777.into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"][0]["mode"] = ((1u64 << 32) | 0o755).into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"][0]["unrecognized"] = true.into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"].as_array_mut().unwrap().remove(0);
    cases.push(changed);
    for value in cases {
        let bytes = serde_json::to_vec(&value)?;
        fs::write(source.join(MANIFEST_NAME), &bytes)?;
        assert!(release::verify(&source, &oe_core::sha256(&bytes)).is_err());
    }
    Ok(())
}

#[test]
fn bound_cargo_version_must_match_release_and_version_file() -> Result<()> {
    let (_temp, root) = temporary()?;
    let source = root.join("source");
    fixture(&source, "2.0.0-rc.1")?;
    let bytes = b"[workspace.package]\nversion = \"2.0.0-rc.2\"\n";
    fs::write(source.join("skill/Cargo.toml"), bytes)?;
    let mut m: ReleaseManifest = serde_json::from_slice(&fs::read(source.join(MANIFEST_NAME))?)?;
    let row = m
        .files
        .iter_mut()
        .find(|v| v.path == "skill/Cargo.toml")
        .unwrap();
    row.sha256 = oe_core::sha256(bytes);
    row.bytes = bytes.len() as u64;
    let manifest = serde_json::to_vec(&m)?;
    fs::write(source.join(MANIFEST_NAME), &manifest)?;
    let error = release::verify(&source, &oe_core::sha256(&manifest)).unwrap_err();
    assert!(error.to_string().contains("release_cargo_version_mismatch"));
    Ok(())
}
