//! Process-scoped fake homes only. No real v1 installation is touched.
#![cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]

use anyhow::Result;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn command(root: &Path, home: Option<&Path>, profile: Option<&Path>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
    command
        .arg("--root")
        .arg(root)
        .env_remove("HOME")
        .env_remove("USERPROFILE");
    if let Some(home) = home {
        command.env("HOME", home);
    }
    if let Some(profile) = profile {
        command.env("USERPROFILE", profile);
    }
    command
}

fn rejected(output: Output) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn fixture(root: &Path) -> Result<(PathBuf, String, PathBuf)> {
    fs::create_dir_all(root)?;
    let source = root.join("source");
    fs::create_dir_all(source.join("skill"))?;
    fs::create_dir(source.join("bin"))?;
    let names = [
        "AGENTS.md",
        "Cargo.lock",
        "Cargo.toml",
        "LICENSE",
        "README.md",
        "SKILL.md",
        "VERSION",
        "rust-toolchain.toml",
    ];
    let mut rows = Vec::new();
    let mut inventory = Vec::new();
    for name in names {
        let bytes = match name {
            "SKILL.md" => b"---\nname: ontology-engineering-v2\n---\n".as_slice(),
            "VERSION" => b"2.0.0-rc.1".as_slice(),
            "Cargo.toml" => b"[workspace.package]\nversion = \"2.0.0-rc.1\"\n".as_slice(),
            _ => b"synthetic\n".as_slice(),
        };
        fs::write(root.join(name), bytes)?;
        fs::write(source.join("skill").join(name), bytes)?;
        let hash = oe_core::sha256(bytes);
        rows.push(
            json!({"path":format!("skill/{name}"),"sha256":hash,"bytes":bytes.len(),"mode":0}),
        );
        inventory.push(json!({"path":name,"sha256":hash,"bytes":bytes.len()}));
    }
    for name in [
        "oev2.exe",
        "oev2-design-bootstrap.exe",
        "oe-code-runner.exe",
        "oe-code-evaluator.exe",
        "oe-evolution-driver.exe",
        "oe-runtime-linker2.exe",
        "oe-runtime-rustc2.exe",
    ] {
        let bytes = format!("never executed: {name}\n");
        fs::write(source.join("bin").join(name), &bytes)?;
        rows.push(json!({"path":format!("bin/{name}"),"sha256":oe_core::sha256(bytes.as_bytes()),"bytes":bytes.len(),"mode":0}));
    }
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    inventory.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let raw = serde_json::to_vec(
        &json!({"schema":"oev2.release-manifest/2","version":"2.0.0-rc.1","target":"x86_64-pc-windows-msvc","entry":"bin/oev2.exe","skill_root":"skill","files":rows}),
    )?;
    fs::write(source.join("RELEASE-MANIFEST.json"), &raw)?;
    let inventory_path = root.join("inventory.json");
    fs::write(
        &inventory_path,
        serde_json::to_vec(
            &json!({"format":"ontology-engineering.portable-skill/v1","entry":"SKILL.md","scope":"Transport integrity only; release authorization is recorded separately.","files":inventory}),
        )?,
    )?;
    // Fixture ACL is constructed independently of the Rust receiver.
    let script = r#"
$ErrorActionPreference = 'Stop'
$root = $env:OE_CLI_TEST_SOURCE
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$trusted = @($user, [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'), [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
$items = @(Get-Item -LiteralPath $root -Force) + @(Get-ChildItem -LiteralPath $root -Recurse -Force)
foreach ($item in $items) {
  if ($item.PSIsContainer) { $acl = [System.Security.AccessControl.DirectorySecurity]::new(); $inherit = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit' }
  else { $acl = [System.Security.AccessControl.FileSecurity]::new(); $inherit = [System.Security.AccessControl.InheritanceFlags]::None }
  $acl.SetOwner($user); $acl.SetAccessRuleProtection($true, $false)
  foreach ($sid in $trusted) { $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid, [System.Security.AccessControl.FileSystemRights]::FullControl, $inherit, [System.Security.AccessControl.PropagationFlags]::None, [System.Security.AccessControl.AccessControlType]::Allow)) }
  Set-Acl -LiteralPath $item.FullName -AclObject $acl
}
"#;
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("OE_CLI_TEST_SOURCE", &source)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "fixture ACL: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((source, oe_core::sha256(raw), inventory_path))
}

#[test]
fn windows_release_cli_fake_home_and_userprofile_v1_are_read_only() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("managed");
    let (source, digest, inventory) = fixture(&root)?;
    let homes = [
        temporary.path().join("HomeA"),
        temporary.path().join("HomeB"),
    ];
    for home in &homes {
        let old = home.join(".codex/skills/ontology-engineering");
        fs::create_dir_all(&old)?;
        fs::write(old.join("sentinel"), b"original v1")?;
    }
    // USERPROFILE fallback, HOME canonical-prefix mismatch, and two independent
    // configured homes all protect the same existing v1 directory identity.
    for (home, profile, target_home) in [
        (None, Some(homes[1].as_path()), &homes[1]),
        (Some(homes[0].as_path()), None, &homes[0]),
        (
            Some(homes[0].as_path()),
            Some(homes[1].as_path()),
            &homes[1],
        ),
    ] {
        let old = target_home.join(".codex/skills/ontology-engineering");
        let target = old.canonicalize()?.join("new-release");
        rejected(
            command(&root, home, profile)
                .args(["dist", "release", "install", "--source"])
                .arg(&source)
                .arg("--target")
                .arg(&target)
                .args(["--expected-sha256", &digest])
                .output()?,
        );
        assert!(!target.exists());
        let output = command(&root, home, profile)
            .args(["maintain", "install", "--inventory"])
            .arg(&inventory)
            .arg("--target")
            .arg(&target)
            .output()?;
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["error"]["reason"], "receiver_v1_read_only");
        rejected(output);
        assert!(!target.exists());
        assert_eq!(fs::read(old.join("sentinel"))?, b"original v1");
    }
    // The guard must not reject a neighbouring v2 destination. A complete valid
    // inert bundle makes this a real CLI positive, not merely an error-code test.
    let output_parent = homes[1].join(".codex/skills/ontology-engineering-v2");
    fs::create_dir_all(&output_parent)?;
    let target = output_parent.join("new-release");
    let output = command(&root, None, Some(&homes[1]))
        .args(["dist", "release", "install", "--source"])
        .arg(&source)
        .arg("--target")
        .arg(&target)
        .args(["--expected-sha256", &digest])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(target.join("bin/oev2.exe"))?,
        fs::read(source.join("bin/oev2.exe"))?
    );
    // The protected root need not exist yet. A case alias of its missing tail
    // cannot turn an otherwise valid bundle into permission to create v1.
    let missing_home = temporary.path().join("MissingV1Home");
    fs::create_dir_all(missing_home.join(".codex/skills"))?;
    let missing_v1 = missing_home.join(".codex/skills/ONTOLOGY-ENGINEERING");
    rejected(
        command(&root, None, Some(&missing_home))
            .args(["dist", "release", "install", "--source"])
            .arg(&source)
            .arg("--target")
            .arg(&missing_v1)
            .args(["--expected-sha256", &digest])
            .output()?,
    );
    assert!(!missing_v1.exists());
    Ok(())
}
