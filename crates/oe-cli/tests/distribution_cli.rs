//! The one native CLI carries the explicit inventory and failure status.
#![cfg(unix)]
use anyhow::Result;
use oe_dist::package::{FORMAT, PortableManifest, SCOPE, SourceFile};
use serde_json::Value;
use std::{path::Path, process::Command};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
    command.env_clear().env("PATH", "").arg("--root").arg(root);
    command
}
#[test]
fn packaging_and_receiver_verification_work_without_legacy_runtime_or_git() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?;
    let mut files = Vec::new();
    for name in ["AGENTS.md", "LICENSE", "README.md", "SKILL.md", "VERSION"] {
        let bytes = b"Transport fixture\n";
        std::fs::write(root.join(name), bytes)?;
        files.push(SourceFile {
            path: name.to_owned(),
            sha256: oe_core::sha256(bytes),
            bytes: bytes.len() as u64,
        });
    }
    std::fs::create_dir(root.join("var"))?;
    let manifest = PortableManifest {
        format: FORMAT.to_owned(),
        entry: "SKILL.md".to_owned(),
        scope: SCOPE.to_owned(),
        files,
    };
    std::fs::write(root.join("var/input.json"), serde_json::to_vec(&manifest)?)?;
    let output = command(&root)
        .args([
            "dist",
            "package",
            "--inventory",
            "var/input.json",
            "--output",
            "var/delivery",
        ])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["status"], "transport_verified");
    assert_eq!(result["publication"], "not_performed");
    let digest = result["archive"]["sha256"].as_str().unwrap();
    let arguments = [
        "dist",
        "verify",
        "--archive",
        "var/delivery/skill.zip",
        "--inventory",
        "var/delivery/PORTABLE-MANIFEST.json",
        "--expected-sha256",
        digest,
    ];
    let verification = command(&root).args(arguments).output()?;
    assert_eq!(verification.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&verification.stdout)?["file_count"],
        5
    );
    std::fs::write(root.join("var/delivery/skill.zip"), b"changed")?;
    let failure = command(&root).args(arguments).output()?;
    assert_eq!(failure.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&failure.stderr).contains(root.to_str().unwrap()));
    let repeat = command(&root)
        .args([
            "dist",
            "package",
            "--inventory",
            "var/input.json",
            "--output",
            "var/delivery",
        ])
        .output()?;
    assert_eq!(repeat.status.code(), Some(2));
    assert_eq!(
        std::fs::read(root.join("var/delivery/skill.zip"))?,
        b"changed"
    );
    Ok(())
}
