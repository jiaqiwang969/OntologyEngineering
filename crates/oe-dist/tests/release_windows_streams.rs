//! Independent NTFS stream counterexamples: actual named-stream writes preserve
//! default content and external manifest identity, yet release verification refuses.
#![cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]

use anyhow::{Context, Result, ensure};
use oe_dist::release::{self, ReleaseFile, ReleaseManifest};
use std::{
    fs,
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

fn refuses_extra_stream(relative: &str) -> Result<()> {
    let (_temporary, root) = temporary()?;
    let bundle = root.join("source");
    let digest = fixture(&bundle, "2.0.0-ads.1")?;
    release::verify(&bundle, &digest)?;
    let manifest_before = fs::read(bundle.join(MANIFEST))?;
    let path = bundle.join(relative);
    let data_before = if path.is_file() {
        Some(fs::read(&path)?)
    } else {
        None
    };
    let mut name = path.as_os_str().to_os_string();
    name.push(":independent-counterexample:$DATA");
    let stream = PathBuf::from(name);
    let payload = b"frozen secret-free independent ADS counterexample";
    fs::write(&stream, payload)?;
    assert_eq!(fs::read(&stream)?, payload);
    assert_eq!(fs::read(bundle.join(MANIFEST))?, manifest_before);
    if let Some(original) = data_before {
        assert_eq!(fs::read(&path)?, original);
    }
    let refusal =
        release::verify(&bundle, &digest).expect_err("nondefault NTFS stream must not be accepted");
    assert!(format!("{refusal:#}").contains("stream"));
    // Verification must not repair or remove the counterexample.
    assert_eq!(fs::read(&stream)?, payload);
    assert_eq!(fs::read(bundle.join(MANIFEST))?, manifest_before);
    Ok(())
}
#[test]
fn windows_program_named_stream_refuses_with_default_bytes_unchanged() -> Result<()> {
    refuses_extra_stream("bin/oev2.exe")
}
#[test]
fn windows_manifest_named_stream_refuses_with_external_sha_unchanged() -> Result<()> {
    refuses_extra_stream(MANIFEST)
}
#[test]
fn windows_public_directory_named_stream_refuses() -> Result<()> {
    refuses_extra_stream("skill")
}
