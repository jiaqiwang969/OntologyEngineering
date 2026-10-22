//! Anonymous in-memory archives. No private files, compiler, filesystem or NX.
use oe_nx_protocol::{BlobIdentity, FileEntry, RelativePath};
use oe_windows_controller::{
    build::{MAX_BUILD_TREE_BYTES, MAX_BUILD_TREE_FILES},
    build_source::*,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn files() -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let root = format!(
        "[workspace]\nmembers = [{}]\nresolver = \"3\"\n\n[workspace.package]\nversion = \"0.0.1\"\n",
        WORKSPACE_MEMBERS
            .iter()
            .map(|n| format!("\"crates/{n}\""))
            .collect::<Vec<_>>()
            .join(",")
    );

    files.insert("Cargo.toml".into(), root.into_bytes());
    let mut lock = "version = 4\n".to_owned();
    for name in WORKSPACE_MEMBERS {
        files.insert(
            format!("crates/{name}/Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion.workspace = true\n").into_bytes(),
        );
        files.insert(
            format!("crates/{name}/src/lib.rs"),
            b"// source fixture; never compiled\n".to_vec(),
        );
        lock.push_str(&format!(
            "\n[[package]]\nname = \"{name}\"\nversion = \"0.0.1\"\n"
        ));
    }
    files.insert("Cargo.lock".into(), lock.into_bytes());
    files.insert(
        "notes/装配 空间.txt".into(),
        b"synthetic source note\n".to_vec(),
    );
    files
}
fn manifest(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    serde_json::to_vec(&SourceManifest {
        schema: SOURCE_SCHEMA.into(),
        archive_root: RelativePath::new("source").unwrap(),
        files: files
            .iter()
            .map(|(n, b)| FileEntry::from_bytes(n, b).unwrap())
            .collect(),
    })
    .unwrap()
}
fn archive(files: &BTreeMap<String, Vec<u8>>, method: CompressionMethod) -> Vec<u8> {
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for (n, b) in files {
        w.start_file(
            format!("source/{n}"),
            SimpleFileOptions::default()
                .compression_method(method)
                .unix_permissions(0o600),
        )
        .unwrap();
        w.write_all(b).unwrap();
    }
    w.finish().unwrap().into_inner()
}
fn verify(files: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<VerifiedSourceArchive> {
    verify_source_archive(
        &archive(files, CompressionMethod::Deflated),
        &manifest(files),
        &files["Cargo.lock"],
    )
}
fn positions(bytes: &[u8]) -> Vec<usize> {
    bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, s)| (s == b"PK\x01\x02").then_some(i))
        .collect()
}
fn u32at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn exact_regular_source_tree_and_root_lock_are_verified_without_compiling() {
    let files = files();
    let inventory = manifest(&files);
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let zip = archive(&files, method);
        let verified = verify_source_archive(&zip, &inventory, &files["Cargo.lock"]).unwrap();
        assert_eq!(verified.members(), &files);
        assert_eq!(verified.assessment().archive, BlobIdentity::of(&zip));
        assert_eq!(verified.assessment().manifest, BlobIdentity::of(&inventory));
        assert_eq!(
            verified.assessment().expanded_bytes,
            files.values().map(|b| b.len() as u64).sum::<u64>()
        );
        assert_eq!(verified.assessment().workspace_members.len(), 12);
        assert_eq!(verified.assessment().lock_packages, 12);
        assert_eq!(verified.assessment().compilation_execution, "not_executed");
        assert_eq!(
            verified.assessment().filesystem_materialization,
            "not_performed"
        );
        assert_eq!(verified.assessment().cargo_resolution, "not_executed");
    }
}

#[test]
fn every_byte_and_member_is_bound_and_separate_lock_cannot_disagree() {
    let original = files();
    let inventory = manifest(&original);
    for key in original.keys() {
        let mut changed = original.clone();
        changed.remove(key);
        assert!(
            verify_source_archive(
                &archive(&changed, CompressionMethod::Stored),
                &inventory,
                &original["Cargo.lock"]
            )
            .is_err(),
            "missing {key}"
        );
        let mut changed = original.clone();
        changed.get_mut(key).unwrap().push(b'x');
        assert!(
            verify_source_archive(
                &archive(&changed, CompressionMethod::Stored),
                &inventory,
                &original["Cargo.lock"]
            )
            .is_err(),
            "changed {key}"
        );
    }
    let mut changed = original.clone();
    changed.insert("unexpected.txt".into(), vec![]);
    assert!(
        verify_source_archive(
            &archive(&changed, CompressionMethod::Stored),
            &inventory,
            &original["Cargo.lock"]
        )
        .is_err()
    );
    assert!(
        verify_source_archive(
            &archive(&original, CompressionMethod::Stored),
            &inventory,
            b"wrong independently bound lock"
        )
        .is_err()
    );
    let mut wrong: SourceManifest = oe_nx_protocol::decode(&inventory).unwrap();
    wrong.files[0].identity.sha256 = oe_nx_protocol::Digest::of(b"wrong hash, unchanged size");
    assert!(
        verify_source_archive(
            &archive(&original, CompressionMethod::Stored),
            &serde_json::to_vec(&wrong).unwrap(),
            &original["Cargo.lock"]
        )
        .is_err()
    );
}

#[test]
fn source_aliases_traversal_directory_and_raw_duplicate_members_fail() {
    for (a, b) in [
        ("alias/a", "alias/A"),
        ("alias/café", "alias/cafe\u{301}"),
        ("alias/A", "alias/Ａ"),
        ("collision", "collision/child"),
    ] {
        let mut input = files();
        input.insert(a.into(), vec![]);
        input.insert(b.into(), vec![]);
        assert!(verify(&input).is_err(), "{a} {b}");
    }
    let original = files();
    let mut value: serde_json::Value = serde_json::from_slice(&manifest(&original)).unwrap();
    for path in [
        "../escape",
        "C:/absolute",
        "a/../../escape",
        "a\\b",
        "a:stream",
        "a/CON",
        "a/trailing.",
    ] {
        value["files"][0]["path"] = serde_json::json!(path);
        assert!(
            verify_source_archive(
                &archive(&original, CompressionMethod::Stored),
                &serde_json::to_vec(&value).unwrap(),
                &original["Cargo.lock"]
            )
            .is_err(),
            "{path}"
        );
    }
    let mut input = original.clone();
    input.insert("alias/a".into(), vec![]);
    input.insert("alias/b".into(), vec![]);
    let mut bytes = archive(&input, CompressionMethod::Stored);
    for at in 0..bytes.len() - 14 {
        if bytes[at..].starts_with(b"source/alias/b") {
            bytes[at + 13] = b'a';
        }
    }
    assert!(verify_source_archive(&bytes, &manifest(&input), &input["Cargo.lock"]).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(&manifest(&original)).unwrap();
    value["archive_root"] = serde_json::json!("different-root");
    assert!(
        verify_source_archive(
            &archive(&original, CompressionMethod::Stored),
            &serde_json::to_vec(&value).unwrap(),
            &original["Cargo.lock"]
        )
        .is_err()
    );
}

#[test]
fn links_reparse_encryption_extra_records_overlap_and_crc_damage_fail() {
    let input = files();
    let inventory = manifest(&input);
    let original = archive(&input, CompressionMethod::Stored);
    for mutation in [
        "symlink",
        "fifo",
        "reparse",
        "directory",
        "encrypted",
        "extra",
        "local_name",
        "overlap",
        "crc",
        "trailing",
        "truncated",
    ] {
        let mut bytes = original.clone();
        let central = positions(&bytes);
        let at = central[0];
        let local = u32at(&bytes, at + 42) as usize;
        match mutation {
            "symlink" => {
                bytes[at + 38..at + 42].copy_from_slice(&(0o120777u32 << 16).to_le_bytes())
            }
            "fifo" => bytes[at + 38..at + 42].copy_from_slice(&(0o010600u32 << 16).to_le_bytes()),
            "reparse" => {
                let v = u32at(&bytes, at + 38) | 0x400;
                bytes[at + 38..at + 42].copy_from_slice(&v.to_le_bytes());
            }
            "directory" => {
                let v = u32at(&bytes, at + 38) | 0x10;
                bytes[at + 38..at + 42].copy_from_slice(&v.to_le_bytes());
            }
            "encrypted" => bytes[at + 8] |= 1,
            "extra" => bytes[at + 30..at + 32].copy_from_slice(&1u16.to_le_bytes()),
            "local_name" => bytes[local + 30] ^= 1,
            "overlap" => {
                let offset = u32at(&bytes, at + 42);
                let second = central[1];
                bytes[second + 42..second + 46].copy_from_slice(&offset.to_le_bytes());
            }
            "crc" => {
                let len =
                    u16::from_le_bytes(bytes[local + 26..local + 28].try_into().unwrap()) as usize;
                bytes[local + 30 + len] ^= 1;
            }
            "trailing" => bytes.push(0),
            "truncated" => {
                bytes.truncate(bytes.len() - 1);
            }
            _ => unreachable!(),
        }
        assert!(
            verify_source_archive(&bytes, &inventory, &input["Cargo.lock"]).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn actual_source_count_may_exceed_job_limit_but_cannot_exceed_build_limit() {
    let mut input = files();
    for i in 0..10_001 {
        input.insert(format!("data/file-{i:05}"), vec![]);
    }
    let verified = verify(&input).unwrap();
    assert!(verified.assessment().expanded_files > oe_nx_protocol::MAX_FILES as u32);
    let mut record: SourceManifest = oe_nx_protocol::decode(&manifest(&input)).unwrap();
    record.files[0].identity.bytes = MAX_BUILD_TREE_BYTES + 1;
    assert!(record.validate().is_err());
    record.files.clear();
    for i in 0..=MAX_BUILD_TREE_FILES {
        record
            .files
            .push(FileEntry::from_bytes(&format!("data/{i}"), b"").unwrap());
    }
    assert!(record.validate().is_err());
    let small = files();
    let mut bytes = archive(&small, CompressionMethod::Stored);
    let at = positions(&bytes)[0];
    let local = u32at(&bytes, at + 42) as usize;
    let oversized = (MAX_BUILD_TREE_BYTES + 1) as u32;
    bytes[at + 24..at + 28].copy_from_slice(&oversized.to_le_bytes());
    bytes[local + 22..local + 26].copy_from_slice(&oversized.to_le_bytes());
    assert!(verify_source_archive(&bytes, &manifest(&small), &small["Cargo.lock"]).is_err());
}

#[test]
fn workspace_and_lock_structure_fail_even_if_all_hashes_are_updated() {
    for mutation in [
        "invalid_toml",
        "missing_member",
        "duplicate_member",
        "extra_member",
        "exclude",
        "root_package",
        "member_name",
        "member_version",
        "nested_workspace",
        "lock_version",
        "missing_local",
        "extra_local",
        "duplicate_local",
        "registry_checksum",
        "registry_source",
    ] {
        let mut input = files();
        let root = String::from_utf8(input["Cargo.toml"].clone()).unwrap();
        let lock = String::from_utf8(input["Cargo.lock"].clone()).unwrap();
        match mutation {
            "invalid_toml" => {
                input.insert("Cargo.toml".into(), b"[workspace\n".to_vec());
            }
            "missing_member" => {
                input.insert(
                    "Cargo.toml".into(),
                    root.replace("\"crates/oe-cli\"", "\"crates/missing\"")
                        .into_bytes(),
                );
            }
            "duplicate_member" => {
                input.insert(
                    "Cargo.toml".into(),
                    root.replace("\"crates/oe-cli\"", "\"crates/oe-cli\",\"crates/oe-cli\"")
                        .into_bytes(),
                );
            }
            "extra_member" => {
                input.insert(
                    "Cargo.toml".into(),
                    root.replace("\"crates/oe-cli\"", "\"crates/oe-cli\",\"outside\"")
                        .into_bytes(),
                );
            }
            "exclude" => {
                input.insert(
                    "Cargo.toml".into(),
                    root.replace(
                        "[workspace]\n",
                        "[workspace]\nexclude=[\"crates/oe-cli\"]\n",
                    )
                    .into_bytes(),
                );
            }
            "root_package" => {
                input.insert(
                    "Cargo.toml".into(),
                    format!("{root}\n[package]\nname=\"uncontrolled-root\"\n").into_bytes(),
                );
            }
            "member_name" => {
                input.insert(
                    "crates/oe-cli/Cargo.toml".into(),
                    b"[package]\nname=\"other\"\nversion=\"0.0.1\"\n".to_vec(),
                );
            }
            "member_version" => {
                input.insert(
                    "crates/oe-cli/Cargo.toml".into(),
                    b"[package]\nname=\"oe-cli\"\nversion=\"0.0.2\"\n".to_vec(),
                );
            }
            "nested_workspace" => {
                input
                    .get_mut("crates/oe-cli/Cargo.toml")
                    .unwrap()
                    .extend_from_slice(b"\n[workspace]\n");
            }
            "lock_version" => {
                input.insert(
                    "Cargo.lock".into(),
                    lock.replacen("version = 4", "version = 3", 1).into_bytes(),
                );
            }
            "missing_local" => {
                input.insert(
                    "Cargo.lock".into(),
                    lock.replace("name = \"oe-cli\"", "name = \"unknown\"")
                        .into_bytes(),
                );
            }
            "extra_local" => {
                input
                    .get_mut("Cargo.lock")
                    .unwrap()
                    .extend_from_slice(b"\n[[package]]\nname=\"extra\"\nversion=\"0.0.1\"\n");
            }
            "duplicate_local" => {
                input
                    .get_mut("Cargo.lock")
                    .unwrap()
                    .extend_from_slice(b"\n[[package]]\nname=\"oe-cli\"\nversion=\"0.0.1\"\n");
            }
            "registry_checksum" => {
                input.get_mut("Cargo.lock").unwrap().extend_from_slice(b"\n[[package]]\nname=\"dep\"\nversion=\"1.0.0\"\nsource=\"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum=\"invalid\"\n");
            }
            "registry_source" => {
                input.get_mut("Cargo.lock").unwrap().extend_from_slice(b"\n[[package]]\nname=\"dep\"\nversion=\"1.0.0\"\nsource=\"git+https://uncontrolled.example/dependency\"\n");
            }
            _ => unreachable!(),
        }
        assert!(verify(&input).is_err(), "{mutation}");
    }
}

#[test]
fn duplicate_manifest_json_keys_and_unlisted_fields_are_rejected() {
    let input = files();
    let inventory = manifest(&input);
    let text = String::from_utf8(inventory).unwrap();
    for raw in [
        text.replacen('{', "{\"schema\":\"duplicate\",", 1),
        text.replacen('{', "{\"command\":\"uncontrolled\",", 1),
    ] {
        assert!(
            verify_source_archive(
                &archive(&input, CompressionMethod::Stored),
                raw.as_bytes(),
                &input["Cargo.lock"]
            )
            .is_err()
        );
    }
}
