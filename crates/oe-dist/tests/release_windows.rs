//! Independent Windows release /2 expectations. Synthetic programs are never
//! executed: this checks transport/selection, not PE execution or engineering.
#![cfg(all(windows, target_arch = "x86_64"))]

use anyhow::{Context, Result, ensure};
use oe_dist::release::{self, ReleaseFile, ReleaseManifest};
use std::{
    fs,
    os::windows::fs::{OpenOptionsExt, symlink_dir, symlink_file},
    path::{Path, PathBuf},
    process::Command,
};

// Independently frozen, rather than importing the producer's platform constants.
const MANIFEST: &str = "RELEASE-MANIFEST.json";
const PROGRAMS: &[&str] = &[
    "oev2.exe",
    "oev2-design-bootstrap.exe",
    "oe-code-runner.exe",
    "oe-code-evaluator.exe",
    "oe-evolution-driver.exe",
    "oe-runtime-linker2.exe",
    "oe-runtime-rustc2.exe",
];

// .NET security objects independently create fixtures. Production Rust ACL
// setters/checkers are deliberately not used to construct their own oracle.
const ACL_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$root = $env:OE_RELEASE_TEST_ACL_ROOT
$mode = $env:OE_RELEASE_TEST_ACL_MODE
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$trusted = @($user, [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'), [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
$items = @(Get-Item -LiteralPath $root -Force)
if ($items[0].PSIsContainer -and $mode -eq 'secure-tree') {
  $items += @(Get-ChildItem -LiteralPath $root -Force -Recurse)
}
foreach ($item in $items) {
  if ($item.PSIsContainer) {
    $acl = [System.Security.AccessControl.DirectorySecurity]::new()
    $inheritance = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
  } else {
    $acl = [System.Security.AccessControl.FileSecurity]::new()
    $inheritance = [System.Security.AccessControl.InheritanceFlags]::None
  }
  $acl.SetOwner($user)
  $acl.SetAccessRuleProtection(($mode -ne 'unprotected'), $false)
  foreach ($sid in $trusted) {
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid, [System.Security.AccessControl.FileSystemRights]::FullControl, $inheritance, [System.Security.AccessControl.PropagationFlags]::None, [System.Security.AccessControl.AccessControlType]::Allow))
  }
  if ($mode -eq 'unsafe') {
    $everyone = [System.Security.Principal.SecurityIdentifier]::new('S-1-1-0')
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($everyone, [System.Security.AccessControl.FileSystemRights]::FullControl, $inheritance, [System.Security.AccessControl.PropagationFlags]::None, [System.Security.AccessControl.AccessControlType]::Allow))
  }
  Set-Acl -LiteralPath $item.FullName -AclObject $acl
  $observed = Get-Acl -LiteralPath $item.FullName
  if (($mode -ne 'unprotected') -and !$observed.AreAccessRulesProtected) { throw 'fixture ACL protection failed' }
  if (($mode -eq 'unprotected') -and $observed.AreAccessRulesProtected) { throw 'fixture unprotected ACL failed' }
}
"#;

fn acl(path: &Path, mode: &str) -> Result<()> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            ACL_SCRIPT,
        ])
        .env("OE_RELEASE_TEST_ACL_ROOT", path)
        .env("OE_RELEASE_TEST_ACL_MODE", mode)
        .output()?;
    ensure!(
        output.status.success(),
        "independent_fixture_acl_failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn fixture(root: &Path, version: &str) -> Result<String> {
    fs::create_dir(root)?;
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
        values.push((
            format!("skill/{name}"),
            b"public synthetic fixture only\n".to_vec(),
        ));
    }
    for name in PROGRAMS {
        values.push((
            format!("bin/{name}"),
            format!("never executed: {name} {version}\n").into_bytes(),
        ));
    }
    values.sort_by(|a, b| a.0.cmp(&b.0));
    let mut files = Vec::new();
    for (path, bytes) in values {
        let destination = root.join(&path);
        fs::create_dir_all(destination.parent().context("fixture_parent")?)?;
        fs::write(destination, &bytes)?;
        files.push(ReleaseFile {
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
        files,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    fs::write(root.join(MANIFEST), &bytes)?;
    acl(root, "secure-tree")?;
    Ok(oe_core::sha256(&bytes))
}

fn temporary() -> Result<(tempfile::TempDir, PathBuf)> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    Ok((directory, root))
}

fn replace_manifest(root: &Path, value: &serde_json::Value) -> Result<String> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(root.join(MANIFEST), &bytes)?;
    Ok(oe_core::sha256(&bytes))
}

fn junction(link: &Path, target: &Path) -> Result<()> {
    let output = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:OE_RELEASE_TEST_JUNCTION -Target $env:OE_RELEASE_TEST_TARGET | Out-Null"])
        .env("OE_RELEASE_TEST_JUNCTION", link).env("OE_RELEASE_TEST_TARGET", target)
        .output()?;
    ensure!(
        output.status.success(),
        "independent_junction_fixture_failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn windows_release_install_upgrade_and_rollback_preserve_exact_selection() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let installed = root.join("installed");
    let received = release::install(&source, &digest, &installed)?;
    assert_eq!(received.root, installed);
    assert_eq!(received.target, "x86_64-pc-windows-msvc");
    assert_eq!(received.entry, installed.join("bin/oev2.exe"));
    assert_eq!(received.skill_root, installed.join("skill"));
    assert_eq!(received.programs.len(), PROGRAMS.len());
    for name in PROGRAMS {
        assert_eq!(
            fs::read(source.join("bin").join(name))?,
            fs::read(installed.join("bin").join(name))?
        );
    }
    let store = root.join("state");
    release::init_store(&store)?;
    let first = release::activate(&store, &installed, &digest, None)?;
    assert_eq!(release::read_active(&store)?, first);
    assert_eq!(first.record.generation, 1);
    assert_eq!(first.record.scope, "local_distribution_activation_only");
    let next = root.join("next");
    let next_digest = fixture(&next, "2.0.0-rc.2")?;
    let second = release::activate(&store, &next, &next_digest, Some(&first.record_sha256))?;
    assert_eq!(second.record.generation, 2);
    assert_eq!(second.record.previous, Some(first.release.reference()));
    let old = release::rollback(&store, &second.record_sha256)?;
    assert_eq!(old.release, received);
    assert_eq!(old.record.generation, 3);
    assert_eq!(old.record.operation, "rollback");
    assert_eq!(release::read_active(&store)?, old);
    assert!(source.join("bin/oev2.exe").is_file());
    assert!(next.join("bin/oev2.exe").is_file());
    Ok(())
}

#[test]
fn windows_existing_root_and_stale_current_do_not_overwrite() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let destination = root.join("existing");
    fs::create_dir(&destination)?;
    fs::write(destination.join("user-data"), b"keep unchanged")?;
    assert!(release::install(&source, &digest, &destination).is_err());
    assert_eq!(fs::read(destination.join("user-data"))?, b"keep unchanged");
    let store = root.join("state");
    release::init_store(&store)?;
    assert!(release::init_store(&store).is_err());
    let first = release::activate(&store, &source, &digest, None)?;
    let before = fs::read(store.join("current.json"))?;
    assert!(release::activate(&store, &source, &digest, Some(&"0".repeat(64))).is_err());
    assert!(release::activate(&store, &source, &digest, None).is_err());
    assert!(release::rollback(&store, &first.record_sha256).is_err());
    assert_eq!(fs::read(store.join("current.json"))?, before);
    Ok(())
}

#[test]
fn windows_corrupt_candidate_or_previous_does_not_change_current() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let one = root.join("one");
    let two = root.join("two");
    let a = fixture(&one, "2.0.0-rc.1")?;
    let b = fixture(&two, "2.0.0-rc.2")?;
    let store = root.join("state");
    release::init_store(&store)?;
    let first = release::activate(&store, &one, &a, None)?;
    let before = fs::read(store.join("current.json"))?;
    assert!(release::activate(&store, &two, &"0".repeat(64), Some(&first.record_sha256)).is_err());
    assert_eq!(fs::read(store.join("current.json"))?, before);
    let second = release::activate(&store, &two, &b, Some(&first.record_sha256))?;
    let before = fs::read(store.join("current.json"))?;
    fs::write(one.join("bin/oev2.exe"), b"broken previous")?;
    assert!(release::rollback(&store, &second.record_sha256).is_err());
    assert_eq!(fs::read(store.join("current.json"))?, before);
    assert_eq!(release::read_active(&store)?, second);
    Ok(())
}

#[test]
fn windows_corrupt_current_can_roll_back_to_intact_previous() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let one = root.join("one");
    let two = root.join("two");
    let a = fixture(&one, "2.0.0-rc.1")?;
    let b = fixture(&two, "2.0.0-rc.2")?;
    let store = root.join("state");
    release::init_store(&store)?;
    let first = release::activate(&store, &one, &a, None)?;
    let second = release::activate(&store, &two, &b, Some(&first.record_sha256))?;
    fs::write(two.join("bin/oev2.exe"), b"broken current")?;
    assert!(release::read_active(&store).is_err());
    let restored = release::rollback(&store, &second.record_sha256)?;
    assert_eq!(restored.release, first.release);
    assert_eq!(release::read_active(&store)?, restored);
    Ok(())
}

#[test]
fn windows_fresh_digest_does_not_authorize_changed_manifest_contract() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    fixture(&source, "2.0.0-rc.1")?;
    let original: serde_json::Value = serde_json::from_slice(&fs::read(source.join(MANIFEST))?)?;
    let mut cases = Vec::new();
    for (field, value) in [
        ("schema", "oev2.release-manifest/1"),
        ("target", "aarch64-apple-darwin"),
        ("entry", "bin/oev2"),
    ] {
        let mut changed = original.clone();
        changed[field] = value.into();
        cases.push(changed);
    }
    for path in [
        "skill/../outside",
        "skill/var/private",
        "skill/sources/private",
        "bin/other.exe",
    ] {
        let mut changed = original.clone();
        changed["files"][0]["path"] = path.into();
        cases.push(changed);
    }
    for mode in [0o600u64, 0o755, (1 << 32) | 0o755] {
        let mut changed = original.clone();
        changed["files"][0]["mode"] = mode.into();
        cases.push(changed);
    }
    let mut changed = original.clone();
    changed["unrecognized"] = true.into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"][0]["unrecognized"] = true.into();
    cases.push(changed);
    let mut changed = original.clone();
    changed["files"].as_array_mut().unwrap().remove(0);
    cases.push(changed);
    for (index, value) in cases.iter().enumerate() {
        let digest = replace_manifest(&source, value)?;
        assert!(
            release::verify(&source, &digest).is_err(),
            "frozen invalid manifest {index} accepted"
        );
    }
    Ok(())
}

#[test]
fn windows_bound_cargo_version_must_match_release() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    fixture(&source, "2.0.0-rc.1")?;
    let changed = b"[workspace.package]\nversion = \"2.0.0-rc.2\"\n";
    fs::write(source.join("skill/Cargo.toml"), changed)?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join(MANIFEST))?)?;
    let row = manifest["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["path"] == "skill/Cargo.toml")
        .unwrap();
    row["sha256"] = oe_core::sha256(changed).into();
    row["bytes"] = (changed.len() as u64).into();
    let digest = replace_manifest(&source, &manifest)?;
    assert!(release::verify(&source, &digest).is_err());
    Ok(())
}

#[test]
fn windows_unlisted_public_data_refuses_and_private_data_is_not_distributed() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let extra = source.join("skill/unlisted.rs");
    fs::write(&extra, b"not in manifest")?;
    acl(&extra, "secure")?;
    assert!(release::verify(&source, &digest).is_err());
    fs::remove_file(extra)?;
    let private = source.join("skill/var");
    fs::create_dir(&private)?;
    acl(&private, "secure")?;
    symlink_file(
        root.join("missing-private-original"),
        private.join("unreadable-original"),
    )?;
    assert!(release::verify(&source, &digest).is_ok());
    let target = root.join("received");
    assert!(release::install(&source, &digest, &target).is_err());
    assert!(!target.exists());
    fs::remove_file(private.join("unreadable-original"))?;
    Ok(())
}

#[test]
fn windows_program_and_root_symlinks_refuse_without_following() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let root_link = root.join("source-link");
    symlink_dir(&source, &root_link)?;
    assert!(release::verify(&root_link, &digest).is_err());
    fs::remove_dir(root_link)?;
    let program = source.join("bin/oev2.exe");
    let outside = root.join("outside.exe");
    fs::write(&outside, fs::read(&program)?)?;
    acl(&outside, "secure")?;
    fs::remove_file(&program)?;
    symlink_file(&outside, &program)?;
    assert!(release::verify(&source, &digest).is_err());
    fs::remove_file(&program)?;
    Ok(())
}

#[test]
fn windows_root_and_payload_parent_junctions_refuse() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let alias = root.join("source-alias");
    junction(&alias, &source)?;
    assert!(release::verify(&alias, &digest).is_err());
    fs::remove_dir(&alias)?;
    let retained = root.join("held-bin");
    fs::rename(source.join("bin"), &retained)?;
    junction(&source.join("bin"), &retained)?;
    assert!(release::verify(&source, &digest).is_err());
    fs::remove_dir(source.join("bin"))?;
    Ok(())
}

#[test]
fn windows_multiple_link_payload_refuses_even_with_identical_bytes() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let program = source.join("bin/oev2.exe");
    let alias = root.join("program-hardlink");
    fs::hard_link(&program, &alias)?;
    assert_eq!(fs::read(&program)?, fs::read(&alias)?);
    assert!(release::verify(&source, &digest).is_err());
    fs::remove_file(alias)?;
    assert!(release::verify(&source, &digest).is_ok());
    Ok(())
}

#[test]
fn windows_untrusted_or_unprotected_acl_refuses_without_changing_bytes() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let program = source.join("bin/oev2.exe");
    let before = fs::read(&program)?;
    for mode in ["unsafe", "unprotected"] {
        acl(&program, mode)?;
        assert!(
            release::verify(&source, &digest).is_err(),
            "{mode} accepted"
        );
        assert_eq!(fs::read(&program)?, before);
        acl(&program, "secure")?;
        assert!(release::verify(&source, &digest).is_ok());
    }
    acl(&source, "unsafe")?;
    assert!(release::verify(&source, &digest).is_err());
    acl(&source, "secure")?;
    assert!(release::verify(&source, &digest).is_ok());
    Ok(())
}

#[test]
fn windows_exclusive_lock_blocks_writer_and_release_permits_retry() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let source = root.join("source");
    let digest = fixture(&source, "2.0.0-rc.1")?;
    let store = root.join("state");
    release::init_store(&store)?;
    let lock_path = store.join(".release.lock");
    if !lock_path.exists() {
        fs::write(&lock_path, b"")?;
    }
    acl(&lock_path, "secure")?;
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&lock_path)?;
    assert!(release::activate(&store, &source, &digest, None).is_err());
    assert!(!store.join("current.json").exists());
    drop(held);
    let active = release::activate(&store, &source, &digest, None)?;
    assert_eq!(release::read_active(&store)?, active);
    Ok(())
}

#[test]
fn windows_malformed_current_is_preserved_and_exact_restore_allows_retry() -> Result<()> {
    let (_temporary, root) = temporary()?;
    let one = root.join("one");
    let two = root.join("two");
    let a = fixture(&one, "2.0.0-rc.1")?;
    let b = fixture(&two, "2.0.0-rc.2")?;
    let store = root.join("state");
    release::init_store(&store)?;
    let first = release::activate(&store, &one, &a, None)?;
    let current = store.join("current.json");
    let before = fs::read(&current)?;
    fs::write(&current, b"{partial")?;
    assert!(release::activate(&store, &two, &b, Some(&first.record_sha256)).is_err());
    assert_eq!(fs::read(&current)?, b"{partial");
    fs::write(&current, before)?;
    let next = release::activate(&store, &two, &b, Some(&first.record_sha256))?;
    assert_eq!(next.record.generation, 2);
    assert_eq!(release::read_active(&store)?, next);
    Ok(())
}
