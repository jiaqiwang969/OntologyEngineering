//! Negative and relocation checks for real native ZIP64 transport.
use anyhow::Result;
use oe_core::{SkillRoot, fingerprint_file, sha256};
use oe_dist::package::{
    FORMAT, PortableManifest, SCOPE, SourceFile, package_skill, read_manifest, verify_archive,
    verify_package,
};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn fixture(workspace: bool) -> Result<(tempfile::TempDir, SkillRoot, PortableManifest)> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().canonicalize()?;
    let root_files = ["AGENTS.md", "LICENSE", "README.md", "SKILL.md", "VERSION"];
    let mut contents: BTreeMap<String, Vec<u8>> = root_files
        .into_iter()
        .map(|name| (name.to_owned(), b"Portable fixture\n".to_vec()))
        .collect();
    contents.insert(
        "scripts/check.sh".to_owned(),
        b"#!/bin/sh\nexit 0\n".to_vec(),
    );
    if workspace {
        contents.insert(
            "Cargo.toml".to_owned(),
            b"[workspace]\nmembers=[\"crates/sample\"]\nresolver=\"3\"\n".to_vec(),
        );
        contents.insert(
            "Cargo.lock".to_owned(),
            b"version = 4\n\n[[package]]\nname = \"sample\"\nversion = \"0.1.0\"\n".to_vec(),
        );
        contents.insert(
            "crates/sample/Cargo.toml".to_owned(),
            b"[package]\nname=\"sample\"\nversion=\"0.1.0\"\nedition=\"2024\"\n".to_vec(),
        );
        contents.insert(
            "crates/sample/src/lib.rs".to_owned(),
            b"pub fn answer() -> u8 { 42 }\n".to_vec(),
        );
        contents.insert(
            "crates/sample/tests/answer.rs".to_owned(),
            b"#[test]\nfn check() { assert_eq!(sample::answer(), 42); }\n".to_vec(),
        );
    }
    let mut files = Vec::new();
    for (name, bytes) in contents {
        let file = path.join(&name);
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(&file, &bytes)?;
        files.push(SourceFile {
            path: name,
            sha256: sha256(&bytes),
            bytes: bytes.len() as u64,
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            path.join("scripts/check.sh"),
            std::fs::Permissions::from_mode(0o755),
        )?;
    }
    let root = SkillRoot::new(&path)?;
    let manifest = PortableManifest {
        format: FORMAT.to_owned(),
        entry: "SKILL.md".to_owned(),
        scope: SCOPE.to_owned(),
        files,
    };
    inventory(&root, &manifest)?;
    Ok((temporary, root, manifest))
}
fn inventory(root: &SkillRoot, manifest: &PortableManifest) -> Result<PathBuf> {
    let directory = root.path().join("var");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("inventory.json");
    std::fs::write(&path, serde_json::to_vec_pretty(manifest)?)?;
    Ok(path)
}
fn produce(root: &SkillRoot) -> Result<oe_dist::package::PackageResult> {
    package_skill(
        root,
        Path::new("var/inventory.json"),
        Path::new("var/delivery"),
    )
}
fn digest(path: &Path) -> Result<String> {
    Ok(fingerprint_file(path, 64 * 1024 * 1024)?.sha256)
}

#[test]
fn native_zip64_roundtrip_preserves_contract_modes_and_relocated_receiver() -> Result<()> {
    let (_temporary, root, manifest) = fixture(true)?;
    let result = produce(&root)?;
    assert_eq!(verify_package(&result)?["status"], "transport_verified");
    let source = result.directory().join("skill.zip");
    let mut archive = ZipArchive::new(std::fs::File::open(&source)?)?;
    assert_eq!(
        archive
            .by_name("ontology-engineering/scripts/check.sh")?
            .unix_mode()
            .unwrap()
            & 0o777,
        // Windows has no Unix execute mode. Its existing portable ZIP transport
        // emits 0644; that metadata never proves native execution permissions.
        if cfg!(unix) { 0o755 } else { 0o644 }
    );
    let receiver = tempfile::tempdir()?;
    let receiver = receiver.path().canonicalize()?;
    let received_zip = receiver.join("received.zip");
    let received_manifest = receiver.join("manifest.json");
    std::fs::copy(&source, &received_zip)?;
    std::fs::copy(
        result.directory().join("PORTABLE-MANIFEST.json"),
        &received_manifest,
    )?;
    let value = verify_archive(&received_zip, &received_manifest, &digest(&source)?)?;
    assert_eq!(value["file_count"], manifest.files.len());
    assert_eq!(value["receiver_cargo_execution"], "not_run");
    assert_eq!(value["engineering_acceptance"], "not_established");
    for row in &manifest.files {
        let mut member = archive.by_name(&format!("ontology-engineering/{}", row.path))?;
        let destination = receiver.join("unpacked").join(&row.path);
        std::fs::create_dir_all(destination.parent().unwrap())?;
        let mut bytes = Vec::new();
        member.read_to_end(&mut bytes)?;
        std::fs::write(destination, bytes)?;
    }
    // Run the receiver's real Cargo, offline, against the actual transported code.
    let check = std::process::Command::new("cargo")
        .args(["test", "--locked", "--offline"])
        .current_dir(receiver.join("unpacked"))
        .env("CARGO_TARGET_DIR", receiver.join("target"))
        .output()?;
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_workspace_paths_match_portable_inventory_and_missing_source_refuses() -> Result<()> {
    let (_temporary, root, manifest) = fixture(true)?;
    let result = produce(&root)?;
    assert_eq!(verify_package(&result)?["status"], "transport_verified");
    // The frozen Windows expectation is ZIP metadata only, with no claim of
    // Unix executable permission. This check does not spawn a child Cargo.
    let mut archive = ZipArchive::new(std::fs::File::open(result.directory().join("skill.zip"))?)?;
    assert_eq!(
        archive
            .by_name("ontology-engineering/scripts/check.sh")?
            .unix_mode()
            .expect("transport records explicit ZIP mode")
            & 0o777,
        0o644
    );
    assert!(
        manifest
            .files
            .iter()
            .any(|f| f.path == "crates/sample/src/lib.rs")
    );
    let (_temporary_bad, bad_root, mut incomplete) = fixture(true)?;
    incomplete
        .files
        .retain(|f| f.path != "crates/sample/src/lib.rs");
    inventory(&bad_root, &incomplete)?;
    let error = produce(&bad_root)
        .err()
        .expect("omitted workspace source must refuse")
        .to_string();
    assert!(
        error.contains("package_unlisted_workspace_source"),
        "{error}"
    );
    assert!(!bad_root.path().join("var/delivery").exists());
    Ok(())
}

#[test]
fn v2_archive_namespace_follows_the_skill_identity() -> Result<()> {
    let (_temporary, root, mut manifest) = fixture(false)?;
    let skill =
        b"---\nname: ontology-engineering-v2\ndescription: synthetic namespace acceptance\n---\n";
    std::fs::write(root.path().join("SKILL.md"), skill)?;
    let row = manifest
        .files
        .iter_mut()
        .find(|f| f.path == "SKILL.md")
        .unwrap();
    row.bytes = skill.len() as u64;
    row.sha256 = sha256(skill);
    inventory(&root, &manifest)?;
    let result = produce(&root)?;
    let archive = ZipArchive::new(std::fs::File::open(result.directory().join("skill.zip"))?)?;
    let names: Vec<_> = archive.file_names().collect();
    assert!(
        names
            .iter()
            .all(|name| name.starts_with("ontology-engineering-v2/")),
        "v2 packaged with wrong namespace: {names:?}"
    );
    assert_eq!(
        verify_package(&result)?["archive_namespace"],
        "ontology-engineering-v2/"
    );
    Ok(())
}

#[test]
fn fresh_digest_cannot_hide_a_wrong_or_mixed_v2_namespace() -> Result<()> {
    let (_temporary, root, mut manifest) = fixture(false)?;
    let skill =
        b"---\nname: ontology-engineering-v2\ndescription: synthetic namespace acceptance\n---\n";
    std::fs::write(root.path().join("SKILL.md"), skill)?;
    let row = manifest
        .files
        .iter_mut()
        .find(|f| f.path == "SKILL.md")
        .unwrap();
    row.bytes = skill.len() as u64;
    row.sha256 = sha256(skill);
    inventory(&root, &manifest)?;
    let wrong = custom_archive(&root, &manifest, "wrong-namespace")?;
    let error = verify_archive(
        &wrong,
        &root.path().join("var/inventory.json"),
        &digest(&wrong)?,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "package_skill_namespace_mismatch");

    let mixed = root.path().join("var/mixed-namespace.zip");
    let mut archive = ZipWriter::new(std::fs::File::create(&mixed)?);
    for row in &manifest.files {
        let prefix = if row.path == "README.md" {
            "ontology-engineering/"
        } else {
            "ontology-engineering-v2/"
        };
        archive.start_file(
            format!("{prefix}{}", row.path),
            SimpleFileOptions::default().unix_permissions(0o644),
        )?;
        archive.write_all(&std::fs::read(root.path().join(&row.path))?)?;
    }
    archive.start_file(
        "ontology-engineering-v2/PORTABLE-MANIFEST.json",
        SimpleFileOptions::default().unix_permissions(0o644),
    )?;
    archive.write_all(&serde_json::to_vec(&manifest)?)?;
    archive.finish()?;
    assert!(
        verify_archive(
            &mixed,
            &root.path().join("var/inventory.json"),
            &digest(&mixed)?
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn existing_delivery_and_changed_frozen_source_never_publish() -> Result<()> {
    for changed in [false, true] {
        let (_temporary, root, _manifest) = fixture(false)?;
        if changed {
            std::fs::write(root.path().join("README.md"), b"changed")?;
        } else {
            std::fs::create_dir(root.path().join("var/delivery"))?;
            std::fs::write(root.path().join("var/delivery/keep"), b"existing")?;
        }
        assert!(produce(&root).is_err());
        if changed {
            assert!(!root.path().join("var/delivery").exists());
            let attempt = std::fs::read_dir(root.path().join("var/staging"))?
                .next()
                .unwrap()?
                .path();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(
                    attempt.join("failure.json")
                )?)?["output_published"],
                false
            );
        } else {
            assert_eq!(
                std::fs::read(root.path().join("var/delivery/keep"))?,
                b"existing"
            );
        }
    }
    Ok(())
}
#[test]
fn usage_of_the_same_digest_or_receipt_after_tampering_is_rejected() -> Result<()> {
    for name in [
        "skill.zip",
        "PORTABLE-MANIFEST.json",
        "receipt.json",
        "snapshot/var/privacy/report.json",
    ] {
        let (_temporary, root, _manifest) = fixture(false)?;
        let result = produce(&root)?;
        std::fs::write(result.directory().join(name), b"changed")?;
        assert!(verify_package(&result).is_err(), "{name}");
    }
    Ok(())
}
#[test]
fn invalid_inventory_paths_types_collisions_and_limits_are_rejected() -> Result<()> {
    let (_temporary, root, base) = fixture(false)?;
    for name in [
        "../escape",
        "var/private",
        "sources/private",
        "sources/private.pdf",
        "sources/.catalog/manifest.json",
        "sources/chapters/text.md",
        "sources/readme.md",
        "Sources/README.md",
        "sources/README.MD",
        "sources/README.md/child",
        "docs/sources/README.md",
        "docs/sources/private.pdf",
        "assets/aux.txt",
        "assets/a.",
        "a\\b",
        "docs/con.json",
    ] {
        let mut manifest = base.clone();
        manifest.files.push(SourceFile {
            path: name.to_owned(),
            sha256: "0".repeat(64),
            bytes: 1,
        });
        manifest.files.sort_by(|a, b| a.path.cmp(&b.path));
        assert!(manifest.validate().is_err(), "{name}");
    }
    for (a, b) in [
        ("docs/A.md", "docs/a.md"),
        ("docs/é.md", "docs/e\u{301}.md"),
        ("Straße/a.rs", "STRASSE/b.rs"),
        ("asset", "asset/child"),
    ] {
        let mut manifest = base.clone();
        for name in [a, b] {
            manifest.files.push(SourceFile {
                path: name.to_owned(),
                sha256: "0".repeat(64),
                bytes: 1,
            });
        }
        manifest.files.sort_by(|a, b| a.path.cmp(&b.path));
        assert!(manifest.validate().is_err(), "{a}, {b}");
    }
    let path = root.path().join("var/inventory.json");
    for bytes in [
        b"{\"files\":{},\"files\":[]}".as_slice(),
        b"[]",
        b"{\"format\":3}",
    ] {
        std::fs::write(&path, bytes)?;
        assert!(read_manifest(&path).is_err());
    }
    let mut value = serde_json::to_value(base)?;
    value["unexpected"] = true.into();
    std::fs::write(path, serde_json::to_vec(&value)?)?;
    assert!(read_manifest(&root.path().join("var/inventory.json")).is_err());
    Ok(())
}
#[test]
fn only_public_sources_readme_is_transported_with_local_library_present() -> Result<()> {
    let (_temporary, root, baseline) = fixture(false)?;
    std::fs::create_dir_all(root.path().join("sources"))?;
    let readme = b"Public library entry instructions\n";
    std::fs::write(root.path().join("sources/README.md"), readme)?;
    let private = [
        "sources/books/original.pdf",
        "sources/misumi/original.pdf",
        "sources/.indexes/catalog.sqlite3",
        "sources/books/README.md",
    ];
    for path in private {
        std::fs::create_dir_all(root.path().join(path).parent().unwrap())?;
        std::fs::write(root.path().join(path), b"private source fixture")?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        root.path().join("absent-private-original"),
        root.path().join("sources/unreadable-original"),
    )?;
    // Exercise the same public identity owner used by compiled-source capture,
    // not a hand-built transport whitelist that might omit the README.
    let manifest = oe_dist::release_build::inventory(&root)?;
    assert_eq!(manifest.files.len(), baseline.files.len() + 1);
    assert!(
        manifest
            .files
            .iter()
            .any(|row| row.path == "sources/README.md")
    );
    inventory(&root, &manifest)?;
    let result = produce(&root)?;
    assert_eq!(verify_package(&result)?["status"], "transport_verified");
    let mut archive = ZipArchive::new(std::fs::File::open(result.directory().join("skill.zip"))?)?;
    let mut actual = Vec::new();
    archive
        .by_name("ontology-engineering/sources/README.md")?
        .read_to_end(&mut actual)?;
    assert_eq!(actual, readme);
    for path in private {
        assert!(
            archive
                .by_name(&format!("ontology-engineering/{path}"))
                .is_err()
        );
        assert_eq!(
            std::fs::read(root.path().join(path))?,
            b"private source fixture"
        );
        let mut leaked = manifest.clone();
        leaked.files.push(SourceFile {
            path: path.into(),
            sha256: sha256(b"private source fixture"),
            bytes: b"private source fixture".len() as u64,
        });
        leaked.files.sort_by(|a, b| a.path.cmp(&b.path));
        inventory(&root, &leaked)?;
        assert!(
            package_skill(
                &root,
                Path::new("var/inventory.json"),
                Path::new("var/leaked")
            )
            .is_err()
        );
        assert!(!root.path().join("var/leaked").exists());
    }
    Ok(())
}
#[test]
fn missing_workspace_source_or_root_cannot_be_reported_complete() -> Result<()> {
    for omitted in [
        "Cargo.toml",
        "Cargo.lock",
        "crates/sample/Cargo.toml",
        "crates/sample/tests/answer.rs",
        "crates/sample/src/lib.rs",
    ] {
        let (_temporary, root, mut manifest) = fixture(true)?;
        manifest.files.retain(|row| row.path != omitted);
        inventory(&root, &manifest)?;
        assert!(produce(&root).is_err(), "{omitted}");
        assert!(!root.path().join("var/delivery").exists());
    }
    Ok(())
}
#[test]
fn privacy_failure_retains_scan_and_never_publishes() -> Result<()> {
    let (_temporary, root, mut manifest) = fixture(false)?;
    // Synthetic content is assembled so this test source is not itself private.
    let content = ["/Users/", "sample-account", "/private-work"]
        .concat()
        .into_bytes();
    std::fs::write(root.path().join("README.md"), &content)?;
    let row = manifest
        .files
        .iter_mut()
        .find(|row| row.path == "README.md")
        .unwrap();
    row.bytes = content.len() as u64;
    row.sha256 = sha256(&content);
    inventory(&root, &manifest)?;
    assert!(produce(&root).is_err());
    assert!(!root.path().join("var/delivery").exists());
    let attempt = std::fs::read_dir(root.path().join("var/staging"))?
        .next()
        .unwrap()?
        .path();
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(
        attempt.join("snapshot/var/privacy/report.json"),
    )?)?;
    assert_eq!(report["ok"], false);
    assert!(!report["findings"].as_array().unwrap().is_empty());
    Ok(())
}
fn custom_archive(
    root: &SkillRoot,
    manifest: &PortableManifest,
    alteration: &str,
) -> Result<PathBuf> {
    let output = root.path().join(format!("var/{alteration}.zip"));
    let mut zip = ZipWriter::new(std::fs::File::create(&output)?);
    for (index, row) in manifest.files.iter().enumerate() {
        if alteration == "missing" && index == 0 {
            continue;
        }
        let name = if alteration == "escape" && index == 0 {
            "ontology-engineering/../escaped".to_owned()
        } else {
            format!("ontology-engineering/{}", row.path)
        };
        zip.start_file(
            name,
            SimpleFileOptions::default()
                .unix_permissions(0o644)
                .large_file(true),
        )?;
        let bytes = if alteration == "wrong-content" && index == 0 {
            b"tampered".to_vec()
        } else {
            std::fs::read(root.path().join(&row.path))?
        };
        zip.write_all(&bytes)?;
    }
    zip.start_file(
        "ontology-engineering/PORTABLE-MANIFEST.json",
        SimpleFileOptions::default().unix_permissions(0o644),
    )?;
    zip.write_all(&serde_json::to_vec(manifest)?)?;
    if alteration == "extra" {
        zip.start_file(
            "ontology-engineering/extra.txt",
            SimpleFileOptions::default(),
        )?;
        zip.write_all(b"extra")?;
    }
    zip.finish()?;
    Ok(output)
}
#[test]
fn fresh_digest_does_not_hide_changed_missing_extra_or_escaping_members() -> Result<()> {
    let (_temporary, root, manifest) = fixture(false)?;
    for alteration in ["missing", "escape", "extra", "wrong-content"] {
        let archive = custom_archive(&root, &manifest, alteration)?;
        assert!(
            verify_archive(
                &archive,
                &root.path().join("var/inventory.json"),
                &digest(&archive)?
            )
            .is_err(),
            "{alteration}"
        );
    }
    Ok(())
}
#[test]
fn central_local_disagreement_duplicate_name_and_trailing_bytes_are_rejected() -> Result<()> {
    let (_temporary, root, _manifest) = fixture(false)?;
    let result = produce(&root)?;
    let original = std::fs::read(result.directory().join("skill.zip"))?;
    let central: Vec<_> = original
        .windows(4)
        .enumerate()
        .filter_map(|(i, v)| (v == b"PK\x01\x02").then_some(i))
        .collect();
    for alteration in [
        "local-name",
        "duplicate-central",
        "trailer",
        "symlink",
        "preamble",
        "oversized",
    ] {
        let mut bytes = original.clone();
        match alteration {
            "local-name" => bytes[30] ^= 1,
            "duplicate-central" => {
                let first = central[0];
                let second = central[1];
                // Mutate VERSION (7 bytes) to LICENSE (7 bytes), keeping all extents.
                let version = *central
                    .iter()
                    .find(|&&at| {
                        let n = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
                        &bytes[at + 46..at + 46 + n] == b"ontology-engineering/VERSION"
                    })
                    .unwrap();
                let name = b"ontology-engineering/LICENSE";
                bytes[version + 46..version + 46 + name.len()].copy_from_slice(name);
                assert_ne!(first, second);
            }
            "trailer" => bytes.extend_from_slice(b"extra"),
            "symlink" => bytes[central[0] + 38..central[0] + 42]
                .copy_from_slice(&(0o120777u32 << 16).to_le_bytes()),
            "preamble" => bytes.splice(..0, b"prefix".iter().copied()).for_each(drop),
            _ => {
                let at = central[0];
                let name_len = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
                let extra = at + 46 + name_len;
                assert_eq!(u16::from_le_bytes([bytes[extra], bytes[extra + 1]]), 1);
                bytes[at + 24..at + 28].copy_from_slice(&u32::MAX.to_le_bytes());
                bytes[extra + 4..extra + 12]
                    .copy_from_slice(&(16 * 1024 * 1024 * 1024u64 + 1).to_le_bytes());
            }
        }
        let path = root.path().join(format!("var/{alteration}.zip"));
        std::fs::write(&path, &bytes)?;
        assert!(
            verify_archive(
                &path,
                &root.path().join("var/inventory.json"),
                &sha256(&bytes)
            )
            .is_err(),
            "{alteration}"
        );
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn input_and_output_symlinks_are_rejected_before_publication() -> Result<()> {
    for input in [true, false] {
        let (_temporary, root, _manifest) = fixture(false)?;
        let target = root.path().join("README.md");
        let link = if input {
            std::fs::rename(&target, root.path().join("actual"))?;
            target
        } else {
            root.path().join("var/delivery")
        };
        std::os::unix::fs::symlink(root.path().join("actual"), link)?;
        assert!(produce(&root).is_err());
    }
    Ok(())
}
#[test]
fn true_zip64_end_records_and_the_current_python_transport_interoperate() -> Result<()> {
    let (_temporary, root, manifest) = fixture(false)?;
    let result = produce(&root)?;
    let original = std::fs::read(result.directory().join("skill.zip"))?;
    let end = original.len() - 22;
    assert_eq!(&original[end..end + 4], b"PK\x05\x06");
    let count = u16::from_le_bytes([original[end + 10], original[end + 11]]) as u64;
    let size = u32::from_le_bytes(original[end + 12..end + 16].try_into()?) as u64;
    let start = u32::from_le_bytes(original[end + 16..end + 20].try_into()?) as u64;
    let mut record = Vec::new();
    record.extend_from_slice(b"PK\x06\x06");
    record.extend_from_slice(&44u64.to_le_bytes());
    record.extend_from_slice(&45u16.to_le_bytes());
    record.extend_from_slice(&45u16.to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    for value in [count, count, size, start] {
        record.extend_from_slice(&value.to_le_bytes());
    }
    let mut locator = Vec::new();
    locator.extend_from_slice(b"PK\x06\x07");
    locator.extend_from_slice(&0u32.to_le_bytes());
    locator.extend_from_slice(&(end as u64).to_le_bytes());
    locator.extend_from_slice(&1u32.to_le_bytes());
    let mut eocd = original[end..].to_vec();
    eocd[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    eocd[10..12].copy_from_slice(&u16::MAX.to_le_bytes());
    eocd[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    eocd[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut full = original[..end].to_vec();
    full.extend_from_slice(&record);
    full.extend_from_slice(&locator);
    full.extend_from_slice(&eocd);
    let zip64 = root.path().join("var/zip64.zip");
    std::fs::write(&zip64, &full)?;
    assert_eq!(
        verify_archive(
            &zip64,
            &root.path().join("var/inventory.json"),
            &sha256(&full)
        )?["file_count"],
        manifest.files.len()
    );
    // The historical test identity is retained. "current" means the frozen
    // predecessor writer, not a restored production Python transport.
    // Execute only the compiled, hash-bound stdlib closure; no source module
    // import, sys.path changes, Python service or mutable oracle-file reread.
    let reference = include_str!("fixtures/history/package_skill_zip64_oe7.py");
    assert_eq!(
        sha256(reference.as_bytes()),
        "8ca8d39dea7b7453a5a718e380373404b12b0a1e96e1f0c9f0e60283ca555aab"
    );
    let output = root.path().join("var/python-reference.zip");
    let code = format!(
        "{}\n{}",
        reference,
        r#"
import sys
root,output=map(Path,sys.argv[1:])
doc=json.loads((root/'var/inventory.json').read_text())
write_archive(root,[root/item['path'] for item in doc['files']],output,
              {item['path']:item['sha256'] for item in doc['files']})
"#,
    );
    let python = std::process::Command::new("python3")
        .args(["-I", "-c"])
        .arg(code)
        .arg(root.path())
        .arg(&output)
        .output()?;
    assert!(
        python.status.success(),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );
    assert_eq!(
        verify_archive(
            &output,
            &root.path().join("var/inventory.json"),
            &digest(&output)?
        )?["file_count"],
        manifest.files.len()
    );
    Ok(())
}
