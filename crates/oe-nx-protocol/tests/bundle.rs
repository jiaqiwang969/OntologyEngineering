//! Real ZIP/CRC intake tests using synthetic payloads, without filesystem, NX or
//! oe-core. Root-bound file acquisition/publication remain adapter tests.
use oe_nx_protocol::{bundle::*, *};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn file(name: &str, bytes: &[u8]) -> FileEntry {
    FileEntry::from_bytes(name, bytes).unwrap()
}
fn package() -> Package {
    let exe = b"MZfixture-only-never-executed";
    let source = b"// fixture, not compiled evidence";
    let build = b"fixture, not a real toolchain run";
    let params = "{\"schema\":\"fixture.inspect/v1\",\"parts\":[\"装配/零件一.prt\"]}".as_bytes();
    let part = b"synthetic-source-part";
    let worker = WorkerManifest {
        schema: WORKER_SCHEMA.into(),
        worker_id: "synthetic-native".into(),
        version: "test".into(),
        target: "x86_64-pc-windows-gnu".into(),
        executable: file("worker/native.exe", exe),
        runtime_files: vec![],
        source_files: vec![file("sources/main.rs", source)],
        build_recipe: file("build/recipe.txt", build),
        toolchain: "synthetic-no-compiler".into(),
        required_runtime: RuntimeIdentity {
            product: "NX".into(),
            product_label: "mixed-per-library-versions".into(),
            architecture: "x64".into(),
            libraries: vec![file("NXBIN/libufun.dll", b"synthetic-identity-only")],
        },
        capabilities: vec![Capability {
            action: "inspect".into(),
            params_schema: "fixture.inspect/v1".into(),
        }],
    };
    let job = JobManifest {
        capacity: None,
        auxiliary_capacity: None,
        schema: JOB_SCHEMA.into(),
        job_id: JobId::new("package-test").unwrap(),
        root: RootIdentity {
            host_id: "fixture-host".into(),
            volume_id: "fixture-volume".into(),
            directory_id: "fixture-directory".into(),
        },
        worker,
        action: "inspect".into(),
        params_schema: "fixture.inspect/v1".into(),
        params: file("params.json", params),
        inputs: vec![file("inputs/装配/零件一.prt", part)],
    };
    let mut members = BTreeMap::new();
    for (name, bytes) in [
        ("worker/native.exe", &exe[..]),
        ("sources/main.rs", &source[..]),
        ("build/recipe.txt", &build[..]),
        ("params.json", params),
        ("inputs/装配/零件一.prt", &part[..]),
    ] {
        insert(&mut members, name, bytes.to_vec()).unwrap();
    }
    insert(
        &mut members,
        "job.json",
        serde_json::to_vec_pretty(&job).unwrap(),
    )
    .unwrap();
    Package { job, members }
}
fn stored(items: impl IntoIterator<Item = (String, Vec<u8>)>) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in items {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
#[test]
fn complete_source_package_round_trip_preserves_original_unicode_references() {
    let original = package();
    original.verify().unwrap();
    for bytes in [
        original.archive().unwrap(),
        stored(original.members.clone()),
    ] {
        let recovered = from_archive(&bytes).unwrap();
        assert_eq!(recovered, original);
        assert!(recovered.members.contains_key("inputs/装配/零件一.prt"));
        assert_eq!(
            recovered.members["params.json"],
            original.members["params.json"]
        );
    }
}
#[test]
fn every_source_build_executable_parameter_input_and_manifest_byte_is_bound() {
    let package = package();
    for key in package.members.keys() {
        let mut changed = package.clone();
        changed.members.get_mut(key).unwrap().push(b'x');
        assert!(changed.verify().is_err(), "{key}");
        assert!(from_archive(&stored(changed.members)).is_err(), "{key}");
        let mut missing = package.clone();
        missing.members.remove(key);
        assert!(missing.verify().is_err(), "{key}");
    }
    let mut mismatch = package.clone();
    mismatch.job.action = "unknown-action".into();
    assert!(mismatch.verify().is_err());
}
#[test]
fn changed_parameter_schema_or_duplicate_json_keys_fail_even_when_hash_is_updated() {
    for params in [
        br#"{"schema":"other/v1"}"#.as_slice(),
        br#"{"schema":"fixture.inspect/v1","x":1,"x":2}"#,
    ] {
        let mut package = package();
        package
            .members
            .insert("params.json".into(), params.to_vec());
        package.job.params = file("params.json", params);
        package
            .members
            .insert("job.json".into(), serde_json::to_vec(&package.job).unwrap());
        assert!(from_archive(&stored(package.members)).is_err());
    }
}
#[test]
fn traversal_reserved_names_case_aliases_and_extra_files_fail_all_member_intake() {
    for name in [
        "../escape",
        "inputs/C:evil.prt",
        "inputs/NUL.prt",
        "inputs/装配/零件一.PRT",
        "extra.txt",
        "worker/native.exe/child",
    ] {
        let mut package = package();
        package.members.insert(name.into(), b"extra".to_vec());
        assert!(from_archive(&stored(package.members)).is_err(), "{name}");
    }
}
#[test]
fn actual_stored_zip_crc_damage_is_rejected() {
    let mut bytes = stored(package().members);
    let needle = b"synthetic-source-part";
    let at = bytes
        .windows(needle.len())
        .position(|p| p == needle)
        .unwrap();
    bytes[at] ^= 1;
    let error = from_archive(&bytes).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("checksum"),
        "{error:#}"
    );
}
#[test]
fn duplicate_raw_central_member_cannot_hide_behind_zip_indexmap() {
    let bytes = stored(package().members);
    let end = bytes.len() - 22;
    let start = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
    let field = |offset| {
        u16::from_le_bytes(
            bytes[start + offset..start + offset + 2]
                .try_into()
                .unwrap(),
        ) as usize
    };
    let length = 46 + field(28) + field(30) + field(32);
    let entry = &bytes[start..start + length];
    let mut duplicate = bytes[..end].to_vec();
    duplicate.extend(entry);
    duplicate.extend(&bytes[end..]);
    let end = end + length;
    for offset in [8, 10] {
        let count = u16::from_le_bytes(
            duplicate[end + offset..end + offset + 2]
                .try_into()
                .unwrap(),
        );
        duplicate[end + offset..end + offset + 2].copy_from_slice(&(count + 1).to_le_bytes());
    }
    let size = u32::from_le_bytes(duplicate[end + 12..end + 16].try_into().unwrap());
    duplicate[end + 12..end + 16].copy_from_slice(&(size + length as u32).to_le_bytes());
    // Demonstrates the upstream IndexMap behavior which made a member-loop-only
    // validator miss this extra raw central entry.
    assert_eq!(
        ZipArchive::new(Cursor::new(&duplicate)).unwrap().len(),
        package().members.len()
    );
    assert!(from_archive(&duplicate).is_err());
}
#[test]
fn archive_metadata_bombs_split_volumes_zip64_and_truncation_are_refused() {
    let bytes = stored(package().members);
    let end = bytes.len() - 22;
    let start = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
    let mut oversized = bytes.clone();
    oversized[start + 24..start + 28]
        .copy_from_slice(&((MAX_PAYLOAD_BYTES + 1) as u32).to_le_bytes());
    assert!(from_archive(&oversized).is_err());
    let mut split = bytes.clone();
    split[end + 4..end + 6].copy_from_slice(&1u16.to_le_bytes());
    assert!(from_archive(&split).is_err());
    let mut zip64 = bytes.clone();
    zip64[end + 16..end + 20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(from_archive(&zip64).is_err());
    let mut excessive = bytes.clone();
    excessive[end + 10..end + 12].copy_from_slice(&((MAX_FILES + 1) as u16).to_le_bytes());
    assert!(from_archive(&excessive).is_err());
    for cut in [0, 3, bytes.len() / 2, bytes.len() - 1] {
        assert!(from_archive(&bytes[..cut]).is_err());
    }
}
#[test]
fn symlink_and_explicit_directory_archive_entries_are_not_regular_payloads() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink(
            "inputs/link.prt",
            "../outside",
            SimpleFileOptions::default(),
        )
        .unwrap();
    assert!(decode_members(&writer.finish().unwrap().into_inner()).is_err());
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_directory("inputs/", SimpleFileOptions::default())
        .unwrap();
    assert!(decode_members(&writer.finish().unwrap().into_inner()).is_err());
}

#[test]
fn generic_fetch_zip_preserves_legacy_receipts_without_v2_execution_authority() {
    let mut members = BTreeMap::new();
    insert(
        &mut members,
        "job.json",
        br#"{"schema":"cad-agent.nx-direct-job/v1","job_id":"legacy"}"#.to_vec(),
    )
    .unwrap();
    insert(
        &mut members,
        "state.json",
        br#"{"state":"UNKNOWN_AFTER_DISPATCH"}"#.to_vec(),
    )
    .unwrap();
    insert(
        &mut members,
        "receipts/旧回执.json",
        br#"{"scope":"historical-only"}"#.to_vec(),
    )
    .unwrap();
    insert(&mut members, "out/子装配.prt", b"fixture bytes".to_vec()).unwrap();
    let archive = archive_members(&members).unwrap();
    assert_eq!(decode_members(&archive).unwrap(), members);
    assert!(from_archive(&archive).is_err());
    members.insert("OUT/子装配.prt".into(), b"alias".to_vec());
    assert!(archive_members(&members).is_err());
    assert!(archive_members(&BTreeMap::new()).is_err());
}
