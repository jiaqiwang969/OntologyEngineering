//! Real CLI transport/activation tests. Synthetic programs are inert payloads;
//! only CARGO_BIN_EXE_oev2 is executed. No native or engineering claim is made.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use anyhow::{Context, Result};
use oe_dist::package::PortableManifest;
use oe_dist::release::{MANIFEST_NAME, PROGRAM_NAMES, ReleaseFile, ReleaseManifest, TARGET};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::{Command, Output};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
    command.env_clear().env("PATH", "").arg("--root").arg(root);
    command
}

fn success(output: Output) -> Result<Value> {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn rejected(output: Output, root: &Path) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "unexpected success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains(root.to_str().unwrap()));
}

fn public_files(version: &str) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    files.insert(
        "SKILL.md".into(),
        b"---\nname: ontology-engineering-v2\n---\n".to_vec(),
    );
    files.insert("VERSION".into(), version.as_bytes().to_vec());
    files.insert(
        "Cargo.toml".into(),
        format!("[workspace.package]\nversion = \"{version}\"\n").into_bytes(),
    );
    for name in [
        "AGENTS.md",
        "README.md",
        "LICENSE",
        "Cargo.lock",
        "rust-toolchain.toml",
    ] {
        files.insert(
            name.into(),
            format!("Transport fixture: {name}\n").into_bytes(),
        );
    }
    files
}

fn write_file(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    fs::create_dir_all(path.parent().context("test_parent_missing")?)?;
    fs::write(path, bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn bundle(root: &Path, version: &str) -> Result<String> {
    fs::create_dir(root)?;
    let mut files = Vec::new();
    for (name, bytes) in public_files(version) {
        let path = format!("skill/{name}");
        write_file(&root.join(&path), &bytes, 0o644)?;
        files.push(ReleaseFile {
            path,
            sha256: oe_core::sha256(&bytes),
            bytes: bytes.len() as u64,
            mode: u64::from(0o644u32),
        });
    }
    for name in PROGRAM_NAMES {
        let bytes =
            format!("Inert transport fixture, never execute: {name} {version}\n").into_bytes();
        let path = format!("bin/{name}");
        write_file(&root.join(&path), &bytes, 0o755)?;
        files.push(ReleaseFile {
            path,
            sha256: oe_core::sha256(&bytes),
            bytes: bytes.len() as u64,
            mode: u64::from(0o755u32),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = ReleaseManifest {
        schema: "oev2.release-manifest/1".into(),
        version: version.into(),
        target: TARGET.into(),
        entry: "bin/oev2".into(),
        skill_root: "skill".into(),
        files,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    write_file(&root.join(MANIFEST_NAME), &bytes, 0o600)?;
    Ok(oe_core::sha256(&bytes))
}

#[test]
fn release_cli_manifest_and_transaction_failures() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let temporary_root = temporary.path().canonicalize()?;
    let root = temporary_root.join("managed-skill");
    for (name, bytes) in public_files("2.0.0-rc.1") {
        write_file(&root.join(name), &bytes, 0o644)?;
    }
    fs::create_dir(root.join("var"))?;
    let source_one = temporary_root.join("source-one");
    let source_two = temporary_root.join("source-two");
    let first_digest = bundle(&source_one, "2.0.0-rc.1")?;
    let second_digest = bundle(&source_two, "2.0.0-rc.2")?;
    let bad_digest = "0".repeat(64);

    let verified = success(
        command(&root)
            .args(["dist", "release", "verify", "--directory"])
            .arg(&source_one)
            .args(["--expected-sha256", &first_digest])
            .output()?,
    )?;
    assert_eq!(verified["manifest_sha256"], first_digest);
    assert_eq!(verified["programs"].as_array().unwrap().len(), 7);
    rejected(
        command(&root)
            .args(["dist", "release", "verify", "--directory"])
            .arg(&source_one)
            .args(["--expected-sha256", &bad_digest])
            .output()?,
        &root,
    );
    rejected(
        command(&root)
            .args(["dist", "release", "install", "--source"])
            .arg(&source_one)
            .args([
                "--target",
                "var/wrong-digest",
                "--expected-sha256",
                &bad_digest,
            ])
            .output()?,
        &root,
    );
    assert!(!root.join("var/wrong-digest").exists());

    for (source, target, digest) in [
        (&source_one, "var/release-one", &first_digest),
        (&source_two, "var/release-two", &second_digest),
    ] {
        let installed = success(
            command(&root)
                .args(["dist", "release", "install", "--source"])
                .arg(source)
                .args(["--target", target, "--expected-sha256", digest])
                .output()?,
        )?;
        assert_eq!(installed["root"], root.join(target).to_str().unwrap());
        assert_eq!(
            installed["entry"],
            root.join(target).join("bin/oev2").to_str().unwrap()
        );
    }
    let initialized = success(
        command(&root)
            .args([
                "dist",
                "release",
                "init-store",
                "--store",
                "var/active-store",
            ])
            .output()?,
    )?;
    assert_eq!(initialized["status"], "release_store_initialized");
    let first = success(
        command(&root)
            .args([
                "dist",
                "release",
                "activate",
                "--store",
                "var/active-store",
                "--directory",
                "var/release-one",
                "--expected-sha256",
                &first_digest,
            ])
            .output()?,
    )?;
    assert_eq!(
        first["record"]["scope"],
        "local_distribution_activation_only"
    );
    let first_record_sha = first["record_sha256"].as_str().unwrap();
    let current = root.join("var/active-store/current.json");
    let first_current_bytes = fs::read(&current)?;
    let first_program = root.join("var/release-one/bin/oev2");
    let first_program_bytes = fs::read(&first_program)?;
    let original_source = fs::read(root.join("SKILL.md"))?;

    // Bad manifest identity and a destination inside managed public source are
    // rejected before mutating the active record or any previous payload.
    rejected(
        command(&root)
            .args([
                "dist",
                "release",
                "activate",
                "--store",
                "var/active-store",
                "--directory",
                "var/release-two",
                "--expected-sha256",
                &bad_digest,
                "--expected-current-sha256",
                first_record_sha,
            ])
            .output()?,
        &root,
    );
    rejected(
        command(&root)
            .args(["dist", "release", "install", "--source"])
            .arg(&source_two)
            .args([
                "--target",
                "forbidden-managed-child",
                "--expected-sha256",
                &second_digest,
            ])
            .output()?,
        &root,
    );
    rejected(
        command(&root)
            .args(["dist", "release", "install", "--source"])
            .arg(&source_two)
            .args([
                "--target",
                "var/release-one",
                "--expected-sha256",
                &second_digest,
            ])
            .output()?,
        &root,
    );
    assert!(!root.join("forbidden-managed-child").exists());
    assert_eq!(fs::read(root.join("SKILL.md"))?, original_source);
    assert_eq!(fs::read(&current)?, first_current_bytes);
    assert_eq!(fs::read(&first_program)?, first_program_bytes);

    let private_note = root.join("var/release-one/skill/var/project-note");
    write_file(&private_note, b"Retain private project data\n", 0o600)?;
    let second = success(
        command(&root)
            .args([
                "dist",
                "release",
                "activate",
                "--store",
                "var/active-store",
                "--directory",
                "var/release-two",
                "--expected-sha256",
                &second_digest,
                "--expected-current-sha256",
                first_record_sha,
            ])
            .output()?,
    )?;
    assert_eq!(second["record"]["generation"], 2);
    let second_record_sha = second["record_sha256"].as_str().unwrap();
    let second_current_bytes = fs::read(&current)?;
    let second_program = root.join("var/release-two/bin/oev2");
    let second_program_bytes = fs::read(&second_program)?;
    rejected(
        command(&root)
            .args([
                "dist",
                "release",
                "activate",
                "--store",
                "var/active-store",
                "--directory",
                "var/release-one",
                "--expected-sha256",
                &first_digest,
                "--expected-current-sha256",
                first_record_sha,
            ])
            .output()?,
        &root,
    );
    rejected(
        command(&root)
            .args([
                "dist",
                "release",
                "rollback",
                "--store",
                "var/active-store",
                "--expected-current-sha256",
                first_record_sha,
            ])
            .output()?,
        &root,
    );
    assert_eq!(fs::read(&current)?, second_current_bytes);
    assert_eq!(fs::read(&first_program)?, first_program_bytes);
    assert_eq!(fs::read(&second_program)?, second_program_bytes);
    let status = success(
        command(&root)
            .args(["dist", "release", "status", "--store", "var/active-store"])
            .output()?,
    )?;
    assert_eq!(status, second);

    let rolled_back = success(
        command(&root)
            .args([
                "dist",
                "release",
                "rollback",
                "--store",
                "var/active-store",
                "--expected-current-sha256",
                second_record_sha,
            ])
            .output()?,
    )?;
    assert_eq!(rolled_back["record"]["operation"], "rollback");
    assert_eq!(rolled_back["record"]["generation"], 3);
    assert_eq!(rolled_back["release"], first["release"]);
    assert_eq!(
        success(
            command(&root)
                .args(["dist", "release", "status", "--store", "var/active-store"])
                .output()?
        )?,
        rolled_back
    );
    assert_eq!(fs::read(&private_note)?, b"Retain private project data\n");
    assert_eq!(fs::read(&first_program)?, first_program_bytes);
    assert_eq!(fs::read(&second_program)?, second_program_bytes);
    Ok(())
}

#[test]
fn maintain_manifest_covers_new_runtime_and_entry_contract_without_private_roots() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?;
    let mut expected = public_files("2.0.0-rc.1");
    expected.insert(
        "ENTRY-CONTRACT-DETAILS.md".into(),
        b"Complete entry contract\n".to_vec(),
    );
    expected.insert(
        "runtime/new-release-profile.json".into(),
        b"{\"fixture\":1}\n".to_vec(),
    );
    expected.insert(
        "runtime/new-owner/nested-contract.json".into(),
        b"{\"fixture\":2}\n".to_vec(),
    );
    expected.insert(
        "docs/NEW-RUNTIME.md".into(),
        b"New runtime profile fixture\n".to_vec(),
    );
    expected.insert(
        "sources/README.md".into(),
        b"Public source-library instructions\n".to_vec(),
    );
    for (path, bytes) in &expected {
        write_file(&root.join(path), bytes, 0o644)?;
    }
    for excluded in ["var", "sources", ".git"] {
        let directory = root.join(excluded);
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("not-public"), b"Must not be inventoried\n")?;
        // A dangling link would fail public inventory traversal, proving that
        // the excluded roots' contents are not accidentally scanned.
        symlink(
            root.join("absent-private-input"),
            directory.join("do-not-follow"),
        )?;
    }
    let output = success(
        command(&root)
            .args([
                "maintain",
                "manifest",
                "--output",
                "var/first-inventory.json",
            ])
            .output()?,
    )?;
    let manifest: PortableManifest = serde_json::from_value(output.clone())?;
    manifest.validate()?;
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(root.join("var/first-inventory.json"))?)?,
        output
    );
    let actual: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|row| (row.path.clone(), (row.bytes, row.sha256.clone())))
        .collect();
    let expected_rows: BTreeMap<_, _> = expected
        .iter()
        .map(|(path, bytes)| (path.clone(), (bytes.len() as u64, oe_core::sha256(bytes))))
        .collect();
    assert_eq!(actual, expected_rows);

    let new_path = "runtime/new-owner/added-after-first.json";
    let new_bytes = b"{\"fixture\":3}\n";
    write_file(&root.join(new_path), new_bytes, 0o644)?;
    let second = success(
        command(&root)
            .args([
                "maintain",
                "manifest",
                "--output",
                "var/second-inventory.json",
            ])
            .output()?,
    )?;
    let manifest: PortableManifest = serde_json::from_value(second)?;
    assert_eq!(manifest.files.len(), expected.len() + 1);
    let added = manifest
        .files
        .iter()
        .find(|row| row.path == new_path)
        .context("new_runtime_file_omitted")?;
    assert_eq!(added.sha256, oe_core::sha256(new_bytes));
    assert_eq!(added.bytes, new_bytes.len() as u64);
    assert!(!manifest.files.iter().any(|row| {
        row.path != "sources/README.md"
            && ["var/", "sources/", ".git/"]
                .iter()
                .any(|prefix| row.path.starts_with(prefix))
    }));
    let revised = b"Revised public source-library instructions\n";
    write_file(&root.join("sources/README.md"), revised, 0o644)?;
    let third = success(
        command(&root)
            .args([
                "maintain",
                "manifest",
                "--output",
                "var/third-inventory.json",
            ])
            .output()?,
    )?;
    let third: PortableManifest = serde_json::from_value(third)?;
    let readme = third
        .files
        .iter()
        .find(|row| row.path == "sources/README.md")
        .context("public_sources_readme_omitted")?;
    assert_eq!(readme.sha256, oe_core::sha256(revised));
    assert_eq!(readme.bytes, revised.len() as u64);
    assert_eq!(third.files.len(), expected.len() + 1);
    assert!(
        manifest
            .files
            .iter()
            .any(|row| row.path == "ENTRY-CONTRACT-DETAILS.md")
    );
    Ok(())
}
